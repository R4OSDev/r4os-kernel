//! Single IOAPIC selector/window and routing owner. Existing runtime admission
//! serializes each MMIO transaction; no global reference crosses a C callback.
use super::runtime::RuntimeGuard;
use crate::memory::{paging, phys};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU32, Ordering},
};
const AVAILABLE: u32 = 1;
const MAPPED: u32 = 2;
const ROUTING: u32 = 4;
const MASKED: u32 = 1 << 16;
const MAX_CPUS: u32 = 32;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Iso {
    pub gsi: u32,
    pub flags: u16,
    pub bus: u8,
    pub source: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Input {
    pub phys_base: u32,
    pub gsi_base: u32,
    pub ioapic_count: u32,
    pub iso_count: u32,
    pub madt_id: u8,
    pub bsp_id: u8,
    pub reserved: [u8; 2],
    pub isos: [Iso; 16],
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RedirectionEntry {
    pub low: u32,
    pub high: u32,
    pub present: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IrqRoute {
    pub gsi: u32,
    pub flags: u16,
    pub irq: u8,
    pub vector: u8,
    pub target_lapic: u8,
    pub redir_index: u8,
    pub present: bool,
    pub in_range: bool,
    pub overridden: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub phys_base: u64,
    pub virt_base: u64,
    pub gsi_base: u32,
    pub flags: u32,
    pub madt_id: u8,
    pub reg_id: u8,
    pub version: u8,
    pub max_redirection_entry: u8,
    pub programmed_routes: u8,
    pub reason: u8,
    pub reserved: [u8; 2],
    pub redirections: [RedirectionEntry; 4],
    pub routes: [IrqRoute; 4],
}
impl Status {
    const EMPTY: Self = Self {
        phys_base: 0,
        virt_base: 0,
        gsi_base: 0,
        flags: 0,
        madt_id: 0,
        reg_id: 0,
        version: 0,
        max_redirection_entry: 0,
        programmed_routes: 0,
        reason: 0,
        reserved: [0; 2],
        redirections: [RedirectionEntry {
            low: 0,
            high: 0,
            present: false,
        }; 4],
        routes: [IrqRoute {
            gsi: 0,
            flags: 0,
            irq: 0,
            vector: 0,
            target_lapic: 0,
            redir_index: 0,
            present: false,
            in_range: false,
            overridden: false,
        }; 4],
    };
    fn mapped(self) -> bool {
        self.flags & MAPPED != 0
    }
}
struct Storage(UnsafeCell<Status>);
// SAFETY: only BSP initialization or the existing runtime owner mutates STATE.
// Status and routes are copied under that owner; MMIO selector/window is shared.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Status::EMPTY));
static FLAGS: AtomicU32 = AtomicU32::new(0);
static NEXT_RUNTIME_TARGET: AtomicU32 = AtomicU32::new(1);

fn state() -> Status {
    unsafe { *STATE.0.get() }
}
fn publish(value: Status) {
    unsafe { STATE.0.get().write(value) };
    FLAGS.store(value.flags, Ordering::Release);
}
fn inputs() -> Input {
    let mut value = core::mem::MaybeUninit::uninit();
    unsafe {
        ioapic_inputs(value.as_mut_ptr());
        value.assume_init()
    }
}
fn log(event: u8, value: Status) {
    unsafe { ioapic_log(event, &value) };
}
fn read_reg(value: Status, register: u8) -> u32 {
    let selector = core::ptr::with_exposed_provenance_mut::<u32>(value.virt_base as usize);
    let window = core::ptr::with_exposed_provenance_mut::<u32>((value.virt_base + 0x10) as usize);
    unsafe {
        selector.write_volatile(u32::from(register));
        window.read_volatile()
    }
}
fn write_reg(value: Status, register: u8, data: u32) {
    let selector = core::ptr::with_exposed_provenance_mut::<u32>(value.virt_base as usize);
    let window = core::ptr::with_exposed_provenance_mut::<u32>((value.virt_base + 0x10) as usize);
    unsafe {
        selector.write_volatile(u32::from(register));
        window.write_volatile(data)
    };
}
fn redirection(value: Status, index: u8) -> RedirectionEntry {
    if !value.mapped() || index > value.max_redirection_entry {
        return RedirectionEntry::default();
    }
    // Checked arithmetic deliberately preserves the original register bounds.
    let low = 0x10 + index * 2;
    RedirectionEntry {
        low: read_reg(value, low),
        high: read_reg(value, low + 1),
        present: true,
    }
}
fn build_route(value: Status, input: Input, irq: u8) -> IrqRoute {
    let mut route = IrqRoute {
        gsi: u32::from(irq),
        irq,
        vector: if irq <= 0xdf { 0x20 + irq } else { 0xff },
        target_lapic: input.bsp_id,
        present: value.mapped(),
        ..IrqRoute::default()
    };
    for iso in input.isos.iter().take((input.iso_count as usize).min(16)) {
        if iso.bus == 0 && iso.source == irq {
            route.gsi = iso.gsi;
            route.flags = iso.flags;
            route.overridden = true;
            break;
        }
    }
    if value.mapped()
        && route.gsi >= value.gsi_base
        && route.gsi - value.gsi_base <= u32::from(value.max_redirection_entry)
    {
        route.in_range = true;
        route.redir_index = (route.gsi - value.gsi_base) as u8;
    }
    route
}
fn route(value: Status, irq: u8) -> IrqRoute {
    if irq >= 32 {
        return IrqRoute {
            irq,
            ..IrqRoute::default()
        };
    }
    match irq {
        0 => value.routes[0],
        1 => value.routes[1],
        2 => value.routes[2],
        12 => value.routes[3],
        _ => build_route(value, inputs(), irq),
    }
}
fn program(value: Status, route: IrqRoute) -> bool {
    if !route.present || !route.in_range {
        return false;
    }
    let low_register = 0x10 + route.redir_index * 2;
    let mut low = u32::from(route.vector);
    if route.flags & 3 == 3 {
        low |= 1 << 13;
    }
    if (route.flags >> 2) & 3 == 3 {
        low |= 1 << 15;
    }
    write_reg(value, low_register + 1, u32::from(route.target_lapic) << 24);
    write_reg(value, low_register, low);
    true
}
fn refresh(mut value: Status, index: u8) -> Status {
    for slot in 0..4 {
        if value.routes[slot].in_range && value.routes[slot].redir_index == index {
            value.redirections[slot] = redirection(value, index);
        }
    }
    value
}
fn choose_target() -> u8 {
    let fallback = inputs().bsp_id;
    if unsafe { ioapic_cpu_mask() }.count_ones() <= 1 {
        return fallback;
    }
    for _ in 0..MAX_CPUS {
        let index = NEXT_RUNTIME_TARGET.fetch_add(1, Ordering::Relaxed) % MAX_CPUS;
        if index == 0 {
            continue;
        }
        let mut apic_id = 0;
        if unsafe { ioapic_cpu_id(index, &mut apic_id) } && apic_id <= 0xff {
            return apic_id as u8;
        }
    }
    fallback
}
pub fn init(input: Input) -> Status {
    let _owner = RuntimeGuard::acquire();
    let mut value = Status {
        phys_base: u64::from(input.phys_base),
        gsi_base: input.gsi_base,
        madt_id: input.madt_id,
        reason: 1,
        ..Status::EMPTY
    };
    if input.ioapic_count == 0 || input.phys_base == 0 {
        publish(value);
        log(0, value);
        return value;
    }
    value.flags |= AVAILABLE;
    let page = value.phys_base & !4095;
    let virtual_page = phys::r4os_rust_phys_to_virt(page);
    let mut found = 0;
    if !unsafe { paging::r4os_rust_paging_read(virtual_page, 3, false, &mut found) }
        && !paging::r4os_rust_paging_map(virtual_page, page, 2 | 16 | (1 << 63), false)
    {
        value.reason = 2;
        publish(value);
        log(0, value);
        return value;
    }
    value.virt_base = phys::r4os_rust_phys_to_virt(value.phys_base);
    let id = read_reg(value, 0);
    let version = read_reg(value, 1);
    value.reg_id = ((id >> 24) & 15) as u8;
    value.version = version as u8;
    value.max_redirection_entry = (version >> 16) as u8;
    value.flags |= MAPPED;
    for (slot, irq) in [0, 1, 2, 12].into_iter().enumerate() {
        value.redirections[slot] = redirection(value, irq);
        value.routes[slot] = build_route(value, input, irq);
    }
    value.reason = 3;
    publish(value);
    log(0, value);
    value
}
fn activate_routes(mut value: Status) -> bool {
    if !value.mapped() {
        log(2, value);
        return false;
    }
    if !value.routes[0].in_range || !value.routes[1].in_range || !value.routes[3].in_range {
        log(3, value);
        return false;
    }
    for index in 0..=value.max_redirection_entry {
        let low = 0x10 + index * 2;
        write_reg(value, low, read_reg(value, low) | MASKED);
    }
    value.programmed_routes = 0;
    for slot in [0, 1, 3] {
        if program(value, value.routes[slot]) {
            value.programmed_routes += 1;
        }
    }
    for (slot, index) in [0, 1, 2, 12].into_iter().enumerate() {
        value.redirections[slot] = redirection(value, index);
    }
    let active = value.programmed_routes == 3;
    if active {
        value.flags |= ROUTING;
    } else {
        value.flags &= !ROUTING;
    }
    value.reason = if active { 4 } else { 5 };
    publish(value);
    log(1, value);
    active
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_ioapic_control(operation: u8, irq: u8, masked: bool) -> bool {
    if operation == 0 {
        return FLAGS.load(Ordering::Acquire) & ROUTING != 0;
    }
    let _owner = RuntimeGuard::acquire();
    let value = state();
    if operation == 4 {
        return activate_routes(value);
    }
    if !value.mapped() {
        return false;
    }
    let mut route = route(value, irq);
    if !route.present || !route.in_range {
        return false;
    }
    match operation {
        1 => {
            let register = 0x10 + route.redir_index * 2;
            let low = read_reg(value, register);
            write_reg(
                value,
                register,
                if masked { low | MASKED } else { low & !MASKED },
            );
        }
        2 => {
            if !program(value, route) {
                return false;
            }
        }
        3 => {
            route.flags = 0x000f;
            route.target_lapic = choose_target();
            if !program(value, route) {
                return false;
            }
        }
        _ => panic!("Invalid private IOAPIC operation"),
    }
    publish(refresh(value, route.redir_index));
    true
}
/// # Safety
/// BSP caller supplies aligned readable fixed C input and writable C output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ioapic_init(input: *const Input, output: *mut Status) {
    let value = unsafe { input.read() };
    unsafe { output.write(init(value)) };
}
/// # Safety
/// Caller owns one aligned writable fixed C output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ioapic_status(output: *mut Status) {
    let _owner = RuntimeGuard::acquire();
    unsafe { output.write(state()) };
}
/// # Safety
/// Caller owns one aligned writable fixed C route output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ioapic_route(irq: u8, output: *mut IrqRoute) {
    let _owner = RuntimeGuard::acquire();
    let value = route(state(), irq);
    unsafe { output.write(value) };
}
/// # Safety
/// Caller owns one aligned writable fixed C redirection output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ioapic_redirection(index: u8, output: *mut RedirectionEntry) {
    let _owner = RuntimeGuard::acquire();
    unsafe { output.write(redirection(state(), index)) };
}

// Shared native presentation of the existing fixed diagnostic codes.
pub fn reason_name(code: u8) -> &'static [u8] {
    match code {
        0 => b"not initialized",
        1 => b"MADT has no IOAPIC",
        2 => b"IOAPIC MMIO mapping failed",
        3 => b"MMIO mapped, redirection table visible, routing unchanged",
        4 => b"legacy IRQ0/1/12 routed through IOAPIC",
        5 => b"IOAPIC routing incomplete, PIC fallback required",
        _ => panic!("Invalid ioapic diagnostic code"),
    }
}
const _: () = {
    assert!(core::mem::size_of::<Iso>() == 8);
    assert!(core::mem::size_of::<Input>() == 148);
    assert!(core::mem::offset_of!(Input, isos) == 20);
    assert!(core::mem::size_of::<RedirectionEntry>() == 12);
    assert!(core::mem::size_of::<IrqRoute>() == 16);
    assert!(core::mem::size_of::<Status>() == 144);
    assert!(core::mem::offset_of!(Status, routes) == 80);
};

unsafe fn ioapic_cpu_mask() -> u64 {
    crate::arch::percpu::r4os_rust_percpu_control(14, 0, 0, 0)
}

unsafe fn ioapic_cpu_id(index: u32, output: *mut u32) -> bool {
    if crate::arch::percpu::r4os_rust_percpu_control(13, index, 0, 0) == 0 {
        return false;
    }
    let id = crate::arch::percpu::r4os_rust_percpu_control(8, index, 0, 0);
    if id == u64::MAX {
        return false;
    }
    unsafe {
        output.write(u32::try_from(id).unwrap());
    }
    true
}

unsafe fn ioapic_log(event: u8, status: *const Status) {
    use crate::boot::log::{
        r4os_rust_bootlog_dec as dec, r4os_rust_bootlog_hex as hex, text as puts,
    };
    let s = unsafe { status.read() };

    match event {
        0 => {
            puts(b"[IOAPIC] ");
            puts(if s.flags & AVAILABLE != 0 {
                b"available"
            } else {
                b"missing"
            });
            puts(b" mapped=");
            puts(if s.mapped() { b"yes" } else { b"no" });
            puts(b" routing=");
            puts(if s.flags & ROUTING != 0 {
                b"active"
            } else {
                b"off"
            });
            puts(b" id=");
            dec(u64::from(s.reg_id));
            puts(b"/");
            dec(u64::from(s.madt_id));
            puts(b" ver=0x");
            hex(u64::from(s.version), 2);
            puts(b" redirs=");
            dec(if s.mapped() {
                u64::from(s.max_redirection_entry) + 1
            } else {
                0
            });
            puts(b" gsi_base=");
            dec(u64::from(s.gsi_base));
            puts(b" route_irq0_gsi=");
            dec(u64::from(s.routes[0].gsi));
            puts(b" route_irq1_gsi=");
            dec(u64::from(s.routes[1].gsi));
            puts(b" route_irq12_gsi=");
            dec(u64::from(s.routes[3].gsi));
            puts(b" reason=");
            puts(match s.reason {
                0 => b"not initialized",
                1 => b"MADT has no IOAPIC",
                2 => b"IOAPIC MMIO mapping failed",
                3 => b"MMIO mapped, redirection table visible, routing unchanged",
                4 => b"legacy IRQ0/1/12 routed through IOAPIC",
                5 => b"IOAPIC routing incomplete, PIC fallback required",
                _ => unreachable!(),
            });
            puts(b"\r\n");
        }
        1 => {
            puts(b"[IOAPIC] routing=");
            puts(if s.flags & ROUTING != 0 {
                b"active"
            } else {
                b"incomplete"
            });
            puts(b" programmed=");
            dec(u64::from(s.programmed_routes));
            puts(b" irq0=");
            dec(u64::from(s.routes[0].gsi));
            puts(b" irq1=");
            dec(u64::from(s.routes[1].gsi));
            puts(b" irq12=");
            dec(u64::from(s.routes[3].gsi));
            puts(b"\r\n");
        }
        2 => puts(b"[IOAPIC][WARN] legacy route activation skipped, IOAPIC not mapped\r\n"),
        3 => puts(b"[IOAPIC][WARN] legacy route activation skipped, route out of range\r\n"),
        _ => unreachable!(),
    }
}

unsafe fn ioapic_inputs(output: *mut Input) {
    let mut info = core::mem::MaybeUninit::uninit();
    let mut local = core::mem::MaybeUninit::uninit();
    unsafe {
        crate::platform::acpi::r4os_rust_acpi_info(info.as_mut_ptr());
        super::lapic::r4os_rust_lapic_status(local.as_mut_ptr());
    }
    let info = unsafe { info.assume_init() };
    let local = unsafe { local.assume_init() };
    let mut value = Input {
        phys_base: info.madt_first_ioapic_address,
        gsi_base: info.madt_first_ioapic_gsi_base,
        ioapic_count: info.madt_ioapic_count,
        iso_count: info.madt_iso_count,
        madt_id: info.madt_first_ioapic_id,
        bsp_id: local.id,
        reserved: [0; 2],
        isos: [Iso::default(); 16],
    };
    for (destination, source) in value
        .isos
        .iter_mut()
        .zip(info.madt_isos.iter())
        .take((info.madt_iso_count as usize).min(16))
    {
        *destination = Iso {
            gsi: source.gsi,
            flags: source.flags,
            bus: source.bus,
            source: source.source,
        };
    }
    unsafe { output.write(value) };
}
