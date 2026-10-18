//! Existing cached IRQ/timer diagnostic view, owned by BSP initialization.
//! Controller owners supply copied values; no state reference crosses them.
use super::{acpi, pci, timer};
use crate::arch::{hpet, ioapic, lapic};
use core::{cell::UnsafeCell, mem::MaybeUninit};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub controller: u8,
    pub timer: u8,
    pub lapic_available: bool,
    pub lapic_runtime_enabled: bool,
    pub lapic_software_enabled: bool,
    pub ioapic_available: bool,
    pub ioapic_runtime_mapped: bool,
    pub ioapic_routing_active: bool,
    pub ioapic_programmed_routes: u8,
    pub hpet_available: bool,
    pub hpet_runtime_mapped: bool,
    pub hpet_enabled: bool,
    pub hpet_frequency_hz: u64,
    pub hpet_counter: u64,
    pub lapic_count: u32,
    pub lapic_enabled_count: u32,
    pub x2apic_count: u32,
    pub ioapic_count: u32,
    pub iso_count: u32,
    pub nmi_count: u32,
    pub lapic_address: u64,
    pub lapic_id: u32,
    pub lapic_version: u8,
    pub lapic_svr: u32,
    pub first_ioapic_id: u8,
    pub first_ioapic_reg_id: u8,
    pub first_ioapic_version: u8,
    pub first_ioapic_redirs: u8,
    pub first_ioapic_address: u32,
    pub first_ioapic_gsi_base: u32,
    pub first_iso_source: u8,
    pub first_iso_gsi: u32,
    pub first_iso_flags: u16,
    pub pit_hz: u32,
    pub fallback_reason: u8,
}
impl Status {
    const EMPTY: Self = Self {
        controller: 0,
        timer: 0,
        lapic_available: false,
        lapic_runtime_enabled: false,
        lapic_software_enabled: false,
        ioapic_available: false,
        ioapic_runtime_mapped: false,
        ioapic_routing_active: false,
        ioapic_programmed_routes: 0,
        hpet_available: false,
        hpet_runtime_mapped: false,
        hpet_enabled: false,
        hpet_frequency_hz: 0,
        hpet_counter: 0,
        lapic_count: 0,
        lapic_enabled_count: 0,
        x2apic_count: 0,
        ioapic_count: 0,
        iso_count: 0,
        nmi_count: 0,
        lapic_address: 0,
        lapic_id: 0,
        lapic_version: 0,
        lapic_svr: 0,
        first_ioapic_id: 0,
        first_ioapic_reg_id: 0,
        first_ioapic_version: 0,
        first_ioapic_redirs: 0,
        first_ioapic_address: 0,
        first_ioapic_gsi_base: 0,
        first_iso_source: 0,
        first_iso_gsi: 0,
        first_iso_flags: 0,
        pit_hz: 0,
        fallback_reason: 0,
    };
}

struct Storage(UnsafeCell<Status>);
// BSP initialization completes before driver/task/AP diagnostic observers.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Status::EMPTY));
pub fn snapshot() -> Status {
    unsafe { STATE.0.get().read() }
}
pub fn init(info: acpi::Info) -> Status {
    let mut lapic = MaybeUninit::uninit();
    let mut ioapic = MaybeUninit::uninit();
    let (lapic, ioapic) = unsafe {
        lapic::r4os_rust_lapic_status(lapic.as_mut_ptr());
        ioapic::r4os_rust_ioapic_status(ioapic.as_mut_ptr());
        (lapic.assume_init(), ioapic.assume_init())
    };
    let hpet = hpet::snapshot();
    let mut current = Status {
        controller: 0,
        timer: timer_backend(),
        lapic_available: info.madt_lapic_address != 0 && info.madt_lapic_enabled_count > 0,
        lapic_runtime_enabled: lapic.flags & 2 != 0,
        lapic_software_enabled: lapic.flags & 4 != 0,
        ioapic_available: info.madt_ioapic_count > 0,
        ioapic_runtime_mapped: ioapic.flags & 2 != 0,
        ioapic_routing_active: ioapic.flags & 4 != 0,
        ioapic_programmed_routes: ioapic.programmed_routes,
        hpet_available: info.hpet_base != 0,
        hpet_runtime_mapped: hpet.flags & 2 != 0,
        hpet_enabled: hpet.flags & 4 != 0,
        hpet_frequency_hz: hpet.frequency_hz,
        hpet_counter: hpet.counter,
        lapic_count: info.madt_lapic_count,
        lapic_enabled_count: info.madt_lapic_enabled_count,
        x2apic_count: info.madt_x2apic_count,
        ioapic_count: info.madt_ioapic_count,
        iso_count: info.madt_iso_count,
        nmi_count: info.madt_nmi_count,
        lapic_address: info.madt_lapic_address,
        lapic_id: lapic.id,
        lapic_version: lapic.version,
        lapic_svr: lapic.svr,
        first_ioapic_id: info.madt_first_ioapic_id,
        first_ioapic_reg_id: ioapic.reg_id,
        first_ioapic_version: ioapic.version,
        first_ioapic_redirs: if ioapic.flags & 2 != 0 {
            ioapic
                .max_redirection_entry
                .checked_add(1)
                .expect("IOAPIC redirection count overflow")
        } else {
            0
        },
        first_ioapic_address: info.madt_first_ioapic_address,
        first_ioapic_gsi_base: info.madt_first_ioapic_gsi_base,
        first_iso_source: info.madt_first_iso_source,
        first_iso_gsi: info.madt_first_iso_gsi,
        first_iso_flags: info.madt_first_iso_flags,
        pit_hz: timer::frequency(),
        fallback_reason: fallback_reason(info),
    };
    if current.lapic_runtime_enabled
        && current.lapic_software_enabled
        && current.ioapic_routing_active
    {
        current.controller = 2;
    } else if current.lapic_available && current.ioapic_available {
        current.controller = 1;
    }
    if timer::active_backend() == 0 && current.hpet_available {
        current.timer = 1;
    } else if timer::active_backend() == 0
        && current.lapic_runtime_enabled
        && current.lapic_software_enabled
    {
        current.timer = 2;
    }
    unsafe { STATE.0.get().write(current) };
    log(current);
    current
}
fn fallback_reason(info: acpi::Info) -> u8 {
    if info.madt_phys == 0 {
        return 1;
    }
    if info.madt_lapic_address == 0 || info.madt_lapic_enabled_count == 0 {
        return 2;
    }
    if info.madt_ioapic_count == 0 {
        return 3;
    }
    if ioapic::r4os_rust_ioapic_control(0, 0, false) {
        4
    } else {
        5
    }
}
fn timer_backend() -> u8 {
    match timer::active_backend() {
        0 => 0,
        1 => 3,
        2 => 4,
        _ => panic!("Invalid timer backend"),
    }
}
pub fn controller_name(controller: u8) -> &'static [u8] {
    match controller {
        0 => b"8259-PIC",
        1 => b"8259-PIC (APIC available)",
        2 => b"APIC/IOAPIC",
        _ => panic!("Invalid IRQ controller"),
    }
}
pub fn timer_name(timer: u8) -> &'static [u8] {
    match timer {
        0 => b"PIT",
        1 => b"PIT (HPET available)",
        2 => b"PIT (LAPIC available)",
        3 => b"HPET",
        4 => b"LAPIC timer",
        _ => panic!("Invalid IRQ timer"),
    }
}
pub fn reason_name(reason: u8) -> &'static [u8] {
    match reason {
        0 => b"MADT/APIC not active yet",
        1 => b"MADT missing, PIC/PIT fallback",
        2 => b"no enabled LAPIC, PIC/PIT fallback",
        3 => b"IOAPIC missing, PIC/PIT fallback",
        4 => b"IOAPIC routes active, 8259-PIC masked",
        5 => b"APIC/IOAPIC parsed, routing not enabled yet",
        _ => panic!("Invalid IRQ fallback"),
    }
}
fn log(current: Status) {
    pci::puts(b"[IRQ] controller=");
    pci::puts(controller_name(current.controller));
    pci::puts(b" timer=");
    pci::puts(timer_name(current.timer));
    pci::puts(b" lapic_runtime=");
    pci::puts(
        if current.lapic_runtime_enabled && current.lapic_software_enabled {
            b"on"
        } else {
            b"off"
        },
    );
    pci::puts(b" lapic=");
    pci::dec(u64::from(current.lapic_enabled_count));
    pci::puts(b"/");
    pci::dec(u64::from(current.lapic_count));
    pci::puts(b" ioapic=");
    pci::dec(u64::from(current.ioapic_count));
    pci::puts(b" ioapic_runtime=");
    pci::puts(if current.ioapic_runtime_mapped {
        b"mapped"
    } else {
        b"off"
    });
    pci::puts(b" ioapic_routing=");
    pci::puts(if current.ioapic_routing_active {
        b"active"
    } else {
        b"off"
    });
    pci::puts(b" iso=");
    pci::dec(u64::from(current.iso_count));
    pci::puts(b" hpet_runtime=");
    pci::puts(if current.hpet_runtime_mapped {
        b"mapped"
    } else {
        b"off"
    });
    pci::puts(b" fallback=");
    pci::puts(reason_name(current.fallback_reason));
    pci::puts(b"\r\n");
}
/// # Safety
/// BSP caller owns readable C input and exclusive aligned C output. No task
/// observes the diagnostic cache before final platform startup publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_irq_view_init(input: *const acpi::Info, output: *mut Status) {
    unsafe { output.write(init(input.read())) };
}
/// # Safety
/// Output is exclusive aligned C storage after the BSP cache publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_irq_view_status(output: *mut Status) {
    unsafe { output.write(snapshot()) };
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 112);
    assert!(core::mem::offset_of!(Status, fallback_reason) == 104);
};
