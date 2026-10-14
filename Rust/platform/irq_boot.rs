//! Existing one-time APIC/IOAPIC/HPET setup and validated timer handoff.
//! The bounded IF-enabled poll keeps no borrow into resident boot state.
use crate::arch::{hpet, ioapic, lapic, pic, pit, runtime};
use crate::platform::{acpi, boot, cpu, irq, monotonic, pci, timer};
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Text {
    pub pointer: *const u8,
    pub bytes: u64,
}
impl Text {
    fn new(text: &'static [u8]) -> Self {
        Self {
            pointer: text.as_ptr(),
            bytes: text.len() as u64,
        }
    }
    fn empty() -> Self {
        Self {
            pointer: core::ptr::null(),
            bytes: 0,
        }
    }
    fn slice(self) -> &'static [u8] {
        if self.bytes == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(self.pointer, self.bytes as usize) }
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Handoff {
    pub attempted: bool,
    pub active_backend: u8,
    pub requested_hz: u32,
    pub validated_ticks: u64,
    pub pit_irq0_route_masked: bool,
    pub fallback_reason: Text,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub initialized: bool,
    pub lapic_status: lapic::Status,
    pub ioapic_status: ioapic::Status,
    pub hpet_status: hpet::Status,
    pub ioapic_routing_active: bool,
    pub hpet_visible: bool,
    pub hpet_initialized: bool,
    pub pic_masked: bool,
    pub pic_master_mask: u8,
    pub pic_slave_mask: u8,
    pub pic_fallback_reason: Text,
    pub timer_handoff: Handoff,
    pub final_irq_status: irq::Status,
    pub status_finalized: bool,
    pub time_status_logged: bool,
}
impl Status {
    const EMPTY: Self = unsafe { core::mem::zeroed() };
    fn initial() -> Self {
        Self {
            pic_master_mask: 0xff,
            pic_slave_mask: 0xff,
            pic_fallback_reason: Text::new(b"not initialized"),
            timer_handoff: Handoff {
                requested_hz: pit::DEFAULT_HZ,
                fallback_reason: Text::new(b"not initialized"),
                ..unsafe { core::mem::zeroed() }
            },
            ..Self::EMPTY
        }
    }
}
struct Storage(UnsafeCell<Status>);
// Singleton BSP startup writes once; release publication precedes observers.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Status::EMPTY));
static INITIALIZED: AtomicBool = AtomicBool::new(false);
unsafe extern "C" {
    fn r4os_zig_irq_boot_step(operation: u8) -> bool;
}
fn step(operation: u8) -> bool {
    unsafe { r4os_zig_irq_boot_step(operation) }
}
pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}
pub fn snapshot() -> Status {
    if !initialized() {
        Status::initial()
    } else {
        unsafe { STATE.0.get().read() }
    }
}
fn lapic_status() -> lapic::Status {
    let mut output = MaybeUninit::uninit();
    unsafe {
        lapic::r4os_rust_lapic_status(output.as_mut_ptr());
        output.assume_init()
    }
}
fn ioapic_status() -> ioapic::Status {
    let mut output = MaybeUninit::uninit();
    unsafe {
        ioapic::r4os_rust_ioapic_status(output.as_mut_ptr());
        output.assume_init()
    }
}
pub fn init() -> bool {
    if initialized() {
        return true;
    }
    if !boot::initialized() {
        return step(0);
    }
    let mut info = MaybeUninit::<acpi::Info>::uninit();
    unsafe { boot::r4os_rust_platform_boot_acpi(info.as_mut_ptr()) };
    let info = unsafe { info.assume_init() };
    let mut current = Status::initial();
    current.pic_master_mask = pic::r4os_rust_pic_control(5, 0);
    current.pic_slave_mask = pic::r4os_rust_pic_control(6, 0);
    current.lapic_status = lapic::init(
        info.madt_lapic_address,
        info.madt_lapic_enabled_count,
        cpu::snapshot().features.x2apic,
    );
    let mut input = ioapic::Input {
        phys_base: info.madt_first_ioapic_address,
        gsi_base: info.madt_first_ioapic_gsi_base,
        ioapic_count: info.madt_ioapic_count,
        iso_count: info.madt_iso_count,
        madt_id: info.madt_first_ioapic_id,
        bsp_id: lapic_status().id,
        reserved: [0; 2],
        isos: [ioapic::Iso::default(); 16],
    };
    for index in 0..(info.madt_iso_count as usize).min(16) {
        let iso = info.madt_isos[index];
        input.isos[index] = ioapic::Iso {
            gsi: iso.gsi,
            flags: iso.flags,
            bus: iso.bus,
            source: iso.source,
        };
    }
    current.ioapic_status = ioapic::init(input);
    current.hpet_status = hpet::init(
        info.hpet_base,
        info.hpet_number,
        info.hpet_min_tick,
        info.hpet_legacy_replacement,
    );
    current.hpet_visible = current.hpet_status.flags & 1 != 0;
    current.hpet_initialized = current.hpet_status.flags & 6 == 6;
    monotonic::r4os_rust_monotonic_control(3, 0);
    if ioapic::r4os_rust_ioapic_control(4, 0, false) {
        pic::r4os_rust_pic_control(3, 0);
        current.ioapic_status = ioapic_status();
        current.ioapic_routing_active = true;
        current.pic_masked = true;
        current.pic_fallback_reason = Text::new(b"not needed; IOAPIC legacy routing active");
        pci::puts(b"[IRQ] IOAPIC legacy routing [OK]\r\n");
        step(1);
    } else {
        current.ioapic_status = ioapic_status();
        current.pic_fallback_reason = Text::new(ioapic::reason_name(current.ioapic_status.reason));
        step(2);
    }
    current.pic_master_mask = pic::r4os_rust_pic_control(5, 0);
    current.pic_slave_mask = pic::r4os_rust_pic_control(6, 0);
    handoff(core::ptr::addr_of_mut!(current));
    current.final_irq_status = irq::init(info);
    current.status_finalized = true;
    step(6);
    current.time_status_logged = true;
    current.initialized = true;
    unsafe { STATE.0.get().write(current) };
    INITIALIZED.store(true, Ordering::Release);
    log(current);
    true
}
#[derive(Clone, Copy)]
struct Attempt {
    ok: bool,
    ticks: u64,
    reason: Text,
}
impl Attempt {
    fn fail(reason: &'static [u8]) -> Self {
        Self {
            ok: false,
            ticks: 0,
            reason: Text::new(reason),
        }
    }
}
fn handoff(current: *mut Status) {
    unsafe {
        (*current).timer_handoff = Handoff {
            attempted: true,
            active_backend: timer::active_backend() as u8,
            requested_hz: pit::DEFAULT_HZ,
            validated_ticks: 0,
            pit_irq0_route_masked: false,
            fallback_reason: Text::new(b"PIT remains active until a modern timer validates"),
        }
    };
    let hpet = hpet_attempt(current);
    if hpet.ok {
        unsafe {
            (*current).timer_handoff.active_backend = timer::active_backend() as u8;
            (*current).timer_handoff.validated_ticks = hpet.ticks;
            (*current).timer_handoff.fallback_reason = Text::new(b"HPET timer active")
        };
        log_timer(b"HPET", hpet.ticks);
        step(3);
        return;
    }
    let lapic = lapic_attempt(current);
    if lapic.ok {
        unsafe {
            (*current).timer_handoff.active_backend = timer::active_backend() as u8;
            (*current).timer_handoff.validated_ticks = lapic.ticks;
            (*current).timer_handoff.fallback_reason = Text::new(b"LAPIC timer active")
        };
        log_timer(b"LAPIC", lapic.ticks);
        step(4);
        return;
    }
    timer::r4os_rust_timer_control(3, 0, 0);
    unsafe {
        (*current).timer_handoff.active_backend = timer::active_backend() as u8;
        (*current).timer_handoff.validated_ticks = hpet.ticks.max(lapic.ticks);
        (*current).timer_handoff.pit_irq0_route_masked = false;
        (*current).timer_handoff.fallback_reason = if lapic.reason.bytes != 0 {
            lapic.reason
        } else {
            hpet.reason
        }
    };
    step(5);
}
fn hpet_attempt(current: *mut Status) -> Attempt {
    if !unsafe { (*current).ioapic_routing_active } {
        return Attempt::fail(b"IOAPIC routing inactive; PIT remains active");
    }
    if timer::r4os_rust_timer_control(2, u64::from(pit::DEFAULT_HZ), 0) == 0 {
        let hpet = hpet::snapshot();
        unsafe { (*current).hpet_status = hpet };
        return Attempt::fail(hpet::reason_name(hpet.reason));
    }
    let (ok, ticks) = validate(2);
    if ok {
        unsafe { (*current).hpet_status = hpet::snapshot() };
        return Attempt {
            ok: true,
            ticks,
            reason: Text::empty(),
        };
    }
    timer::r4os_rust_timer_control(3, 0, 0);
    unsafe { (*current).hpet_status = hpet::snapshot() };
    Attempt {
        ok: false,
        ticks,
        reason: Text::new(b"HPET timer did not deliver validated ticks; PIT restored"),
    }
}
fn lapic_attempt(current: *mut Status) -> Attempt {
    if !unsafe { (*current).ioapic_routing_active } {
        return Attempt::fail(b"IOAPIC routing inactive; PIT remains active");
    }
    if timer::r4os_rust_timer_control(1, u64::from(pit::DEFAULT_HZ), 0) == 0 {
        let lapic = lapic_status();
        unsafe { (*current).lapic_status = lapic };
        return Attempt::fail(lapic::timer_reason_name(lapic.timer_reason));
    }
    if !ioapic::r4os_rust_ioapic_control(1, pit::IRQ, true) {
        timer::r4os_rust_timer_control(3, 0, 0);
        unsafe {
            (*current).lapic_status = lapic_status();
            (*current).ioapic_status = ioapic_status()
        };
        return Attempt::fail(b"IOAPIC PIT IRQ0 route could not be masked; PIT restored");
    }
    unsafe {
        (*current).timer_handoff.pit_irq0_route_masked = true;
        (*current).ioapic_status = ioapic_status()
    };
    let (ok, ticks) = validate(2);
    if ok {
        unsafe { (*current).lapic_status = lapic_status() };
        return Attempt {
            ok: true,
            ticks,
            reason: Text::empty(),
        };
    }
    timer::r4os_rust_timer_control(3, 0, 0);
    ioapic::r4os_rust_ioapic_control(1, pit::IRQ, false);
    unsafe {
        (*current).timer_handoff.pit_irq0_route_masked = false;
        (*current).lapic_status = lapic_status();
        (*current).ioapic_status = ioapic_status()
    };
    Attempt {
        ok: false,
        ticks,
        reason: Text::new(b"LAPIC timer did not deliver validated ticks; PIT route restored"),
    }
}
fn validate(minimum: u64) -> (bool, u64) {
    let hpet = hpet::snapshot();
    if hpet.flags & 4 == 0 || hpet.frequency_hz == 0 {
        return (false, 0);
    }
    let start_hpet = hpet::main_counter();
    let duration = hpet.frequency_hz / 20;
    let start_ticks = timer::tick_count();
    let deadline = timer::r4os_rust_timer_control(6, start_ticks, minimum);
    runtime::r4os_rust_runtime_control(1, 0);
    while hpet::elapsed(start_hpet, hpet::main_counter()) < duration
        && timer::tick_count() < deadline
    {
        core::hint::spin_loop();
    }
    runtime::r4os_rust_runtime_control(0, 0);
    let ticks = timer::tick_count() - start_ticks;
    (ticks >= minimum, ticks)
}
fn log_timer(name: &[u8], ticks: u64) {
    pci::puts(b"[TIMER] ");
    pci::puts(name);
    pci::puts(b" ");
    pci::dec(u64::from(timer::frequency()));
    pci::puts(b" Hz ticks=");
    pci::dec(ticks);
    pci::puts(b" [OK]\r\n");
}
fn backend_name() -> &'static [u8] {
    match timer::active_backend() {
        0 => b"PIT",
        1 => b"HPET",
        2 => b"LAPIC",
        _ => panic!("Invalid timer backend"),
    }
}
fn yes(value: bool) -> &'static [u8] {
    if value { b"yes" } else { b"no" }
}
fn log(current: Status) {
    pci::puts(b"[IRQBOOT] initialized=");
    pci::puts(yes(current.initialized));
    pci::puts(b" lapic=");
    pci::puts(if current.lapic_status.flags & 6 == 6 {
        b"enabled"
    } else {
        b"off"
    });
    pci::puts(b" ioapic_routing=");
    pci::puts(if current.ioapic_routing_active {
        b"active"
    } else {
        b"pic-fallback"
    });
    pci::puts(b" hpet=");
    pci::puts(if current.hpet_initialized {
        b"initialized"
    } else if current.hpet_visible {
        b"visible"
    } else {
        b"missing"
    });
    pci::puts(b" pic_masked=");
    pci::puts(yes(current.pic_masked));
    pci::puts(b" pic_master=0x");
    pci::hex(u64::from(current.pic_master_mask), 2);
    pci::puts(b" pic_slave=0x");
    pci::hex(u64::from(current.pic_slave_mask), 2);
    pci::puts(b" fallback=");
    pci::puts(current.pic_fallback_reason.slice());
    pci::puts(b" timer=");
    pci::puts(backend_name());
    pci::puts(b" validated_ticks=");
    pci::dec(current.timer_handoff.validated_ticks);
    pci::puts(b" pit_irq0_masked=");
    pci::puts(yes(current.timer_handoff.pit_irq0_route_masked));
    pci::puts(b" timer_reason=");
    pci::puts(current.timer_handoff.fallback_reason.slice());
    pci::puts(b" final_status=");
    pci::puts(yes(current.status_finalized));
    pci::puts(b" time_logged=");
    pci::puts(yes(current.time_status_logged));
    pci::puts(b"\r\n");
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_irq_boot_control(operation: u8) -> bool {
    match operation {
        0 => init(),
        1 => initialized(),
        _ => false,
    }
}
/// # Safety
/// Output is exclusive aligned C storage. Only copied, immutable boot metadata
/// escapes; the caller never receives the resident state owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_irq_boot_status(output: *mut Status) {
    unsafe { output.write(snapshot()) };
}
const _: () = {
    assert!(core::mem::size_of::<Handoff>() == 40);
    assert!(core::mem::size_of::<Status>() == 504);
    assert!(core::mem::offset_of!(Status, timer_handoff) == 352);
    assert!(core::mem::offset_of!(Status, final_irq_status) == 392);
};
