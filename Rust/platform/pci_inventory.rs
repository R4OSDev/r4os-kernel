//! Canonical PCI inventory. Boot publishes one bounded immutable result and
//! generation; passive lookup/materialization counts are independent atomics.
use super::{monotonic, pci, pci_scan as scan, pcie};
use crate::contract;
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use scan::{Device, MAX_DEVICES};
pub type Performance = contract::ProgramPciInventoryPerformanceInfo;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub enumerated: bool,
    pub ecam_used: bool,
    pub legacy_used: bool,
    pub partial: bool,
    pub truncated: bool,
    pub flags: u32,
    pub generation: u32,
    pub device_count: u32,
    pub stored_count: u32,
    pub dropped_count: u32,
    pub ecam_stored_count: u32,
    pub legacy_stored_count: u32,
    pub ahci_count: u32,
    pub nvme_count: u32,
    pub network_count: u32,
    pub hda_count: u32,
    pub xhci_count: u32,
    pub first_ahci: Device,
    pub first_nvme: Device,
    pub first_network: Device,
    pub first_hda: Device,
    pub first_xhci: Device,
    pub ecam_vendor_probes: u64,
    pub legacy_vendor_probes: u64,
    pub class_reads: u64,
    pub header_reads: u64,
    pub enumeration_config_reads: u64,
    pub function_pages: u64,
    pub early_stops: u64,
    pub enumeration_total_ns: u64,
    pub ecam_enumeration_ns: u64,
    pub legacy_enumeration_ns: u64,
    pub timing_unavailable: u64,
    pub reason: u8,
}
impl Status {
    pub const EMPTY: Self = Self {
        enumerated: false,
        ecam_used: false,
        legacy_used: false,
        partial: false,
        truncated: false,
        flags: 0,
        generation: 0,
        device_count: 0,
        stored_count: 0,
        dropped_count: 0,
        ecam_stored_count: 0,
        legacy_stored_count: 0,
        ahci_count: 0,
        nvme_count: 0,
        network_count: 0,
        hda_count: 0,
        xhci_count: 0,
        first_ahci: Device::EMPTY,
        first_nvme: Device::EMPTY,
        first_network: Device::EMPTY,
        first_hda: Device::EMPTY,
        first_xhci: Device::EMPTY,
        ecam_vendor_probes: 0,
        legacy_vendor_probes: 0,
        class_reads: 0,
        header_reads: 0,
        enumeration_config_reads: 0,
        function_pages: 0,
        early_stops: 0,
        enumeration_total_ns: 0,
        ecam_enumeration_ns: 0,
        legacy_enumeration_ns: 0,
        timing_unavailable: 0,
        reason: 0,
    };
}

struct State {
    status: Status,
    result: scan::Result,
    generation: u32,
}
struct Storage(UnsafeCell<State>);
// Exclusive BSP enumeration precedes driver/task/AP observers. The resident
// result is immutable afterwards; no state reference spans reader/clock/log calls.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    status: Status::EMPTY,
    result: scan::Result::EMPTY,
    generation: 0,
}));
static CLASS_CALLS: AtomicU64 = AtomicU64::new(0);
static CLASS_CANDIDATES: AtomicU64 = AtomicU64::new(0);
static DETAILS: AtomicU64 = AtomicU64::new(0);
static INTERRUPT_READS: AtomicU64 = AtomicU64::new(0);
static COMMAND_READS: AtomicU64 = AtomicU64::new(0);
static BAR_READS: AtomicU64 = AtomicU64::new(0);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    unsafe { f(&mut *STATE.0.get()) }
}
fn result() -> *mut scan::Result {
    unsafe { ptr::addr_of_mut!((*STATE.0.get()).result) }
}
fn metrics() -> scan::Metrics {
    unsafe { ptr::addr_of!((*result()).metrics).read() }
}
pub fn snapshot() -> Status {
    unsafe { ptr::addr_of!((*STATE.0.get()).status).read() }
}
unsafe extern "C" fn read_ecam(
    _: *mut c_void,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
) -> u32 {
    pcie::read_current_config32(bus, device, function, offset)
}
unsafe extern "C" fn read_legacy(
    _: *mut c_void,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
) -> u32 {
    if offset > 0xff {
        u32::MAX
    } else {
        pci::read_config32(bus, device, function, offset as u8)
    }
}
pub fn enumerate() -> Status {
    let total_start = monotonic::capture();
    state(|state| {
        state.result = scan::Result::EMPTY;
        state.status = Status::EMPTY;
    });
    for counter in [
        &CLASS_CALLS,
        &CLASS_CANDIDATES,
        &DETAILS,
        &INTERRUPT_READS,
        &COMMAND_READS,
        &BAR_READS,
    ] {
        counter.store(0, Ordering::Relaxed);
    }
    pci::reset_access_metrics();
    let ecam = pcie::snapshot();
    let plan = scan::plan_coverage(
        ecam.aperture_ready,
        ecam.segment,
        ecam.start_bus,
        ecam.end_bus,
    );
    state(|state| {
        state.status.ecam_used = plan.ecam_used;
        state.status.legacy_used = plan.legacy_used;
        state.status.partial = plan.partial;
    });
    for range in plan.ranges[..usize::from(plan.count)].iter().copied() {
        let before = metrics();
        let started = monotonic::capture();
        let reader = scan::Reader {
            context: ptr::null_mut(),
            read_config32: if range.bus_kind == scan::ECAM {
                read_ecam
            } else {
                read_legacy
            },
        };
        let complete = unsafe { scan::scan_range(result(), range, reader) };
        record_range(range.bus_kind, before, metrics(), started);
        if !complete {
            break;
        }
    }
    let metrics = metrics();
    state(|state| {
        state.generation = state.generation.wrapping_add(1);
        if state.generation == 0 {
            state.generation = 1;
        }
        let current = &mut state.status;
        current.enumerated = true;
        current.generation = state.generation;
        current.device_count = metrics.found;
        current.stored_count = metrics.stored;
        current.dropped_count = metrics.dropped;
        current.truncated = metrics.truncated;
        current.class_reads = metrics.class_reads;
        current.header_reads = metrics.header_reads;
        current.enumeration_config_reads = metrics.config_reads;
        current.function_pages = metrics.function_pages;
        current.early_stops = metrics.early_stops;
        current.flags = contract::pci_inventory_flag_enumerated
            | if current.ecam_used {
                contract::pci_inventory_flag_ecam
            } else {
                0
            }
            | if current.legacy_used {
                contract::pci_inventory_flag_legacy
            } else {
                0
            }
            | if current.partial {
                contract::pci_inventory_flag_partial
            } else {
                0
            }
            | if current.truncated {
                contract::pci_inventory_flag_truncated
            } else {
                0
            }
            | if ecam.aperture_ready {
                contract::pci_inventory_flag_ecam_aperture_ready
            } else {
                0
            }
            | if ecam.available && ecam.segment != 0 {
                contract::pci_inventory_flag_ecam_rejected_segment
            } else {
                0
            };
    });
    classify_stored();
    let elapsed = monotonic::elapsed(total_start, monotonic::capture());
    state(|state| {
        if let Some(elapsed) = elapsed.filter(|elapsed| *elapsed != 0) {
            state.status.enumeration_total_ns = elapsed;
        } else {
            state.status.timing_unavailable = state.status.timing_unavailable.wrapping_add(1);
        }
        state.status.reason = if state.status.truncated {
            1
        } else if state.status.partial {
            2
        } else if state.status.ecam_used {
            3
        } else {
            4
        };
    });
    log_summary();
    snapshot()
}
fn record_range(kind: u8, before: scan::Metrics, after: scan::Metrics, started: monotonic::Stamp) {
    let vendor = after.vendor_probes.wrapping_sub(before.vendor_probes);
    let stored = after.stored.wrapping_sub(before.stored);
    state(|state| {
        if kind == scan::ECAM {
            state.status.ecam_vendor_probes = state.status.ecam_vendor_probes.wrapping_add(vendor);
            state.status.ecam_stored_count = state.status.ecam_stored_count.wrapping_add(stored);
        } else {
            state.status.legacy_vendor_probes =
                state.status.legacy_vendor_probes.wrapping_add(vendor);
            state.status.legacy_stored_count =
                state.status.legacy_stored_count.wrapping_add(stored);
        }
    });
    let elapsed = monotonic::elapsed(started, monotonic::capture());
    state(|state| {
        if let Some(elapsed) = elapsed.filter(|elapsed| *elapsed != 0) {
            if kind == scan::ECAM {
                state.status.ecam_enumeration_ns =
                    state.status.ecam_enumeration_ns.wrapping_add(elapsed);
            } else {
                state.status.legacy_enumeration_ns =
                    state.status.legacy_enumeration_ns.wrapping_add(elapsed);
            }
        } else {
            state.status.timing_unavailable = state.status.timing_unavailable.wrapping_add(1);
        }
    });
}
fn classify_stored() {
    for index in 0..count().min(MAX_DEVICES) {
        let device = device_at(index).unwrap();
        state(|state| {
            let current = &mut state.status;
            if device.class_code == 1 && device.subclass == 6 {
                if current.ahci_count == 0 {
                    current.first_ahci = device;
                }
                current.ahci_count += 1;
            } else if device.class_code == 1 && device.subclass == 8 {
                if current.nvme_count == 0 {
                    current.first_nvme = device;
                }
                current.nvme_count += 1;
            } else if device.class_code == 2 && device.subclass == 0 {
                if current.network_count == 0 {
                    current.first_network = device;
                }
                current.network_count += 1;
            } else if device.class_code == 4 && device.subclass == 3 {
                if current.hda_count == 0 {
                    current.first_hda = device;
                }
                current.hda_count += 1;
            } else if device.class_code == 12 && device.subclass == 3 && device.prog_if == 0x30 {
                if current.xhci_count == 0 {
                    current.first_xhci = device;
                }
                current.xhci_count += 1;
            }
        });
    }
}
pub fn count() -> usize {
    snapshot().stored_count as usize
}
pub fn device_at(index: usize) -> Option<Device> {
    if index >= count() || index >= MAX_DEVICES {
        return None;
    }
    Some(unsafe {
        ptr::addr_of!((*result()).devices)
            .cast::<Device>()
            .add(index)
            .read()
    })
}
pub fn find_by_class(class: u8, subclass: u8, start: usize) -> Option<usize> {
    CLASS_CALLS.fetch_add(1, Ordering::Relaxed);
    let found = scan::find_by_class(unsafe { &*result() }, class, subclass, start);
    let stored = count();
    if start < stored {
        CLASS_CANDIDATES.fetch_add(
            found.map_or(stored - start, |index| index - start + 1) as u64,
            Ordering::Relaxed,
        );
    }
    found
}
pub fn note_detail_materialization() {
    DETAILS.fetch_add(1, Ordering::Relaxed);
}
pub fn read_config32_at(kind: u8, bus: u8, device: u8, function: u8, offset: u16) -> u32 {
    if kind == scan::ECAM {
        return pcie::read_current_config32(bus, device, function, offset);
    }
    if kind == scan::LEGACY && offset <= 0xff {
        return pci::read_config32(bus, device, function, offset as u8);
    }
    u32::MAX
}
pub fn write_config32_at(
    kind: u8,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
    value: u32,
) -> bool {
    if kind == scan::ECAM {
        return pcie::write_current_config32(bus, device, function, offset, value);
    }
    if kind == scan::LEGACY && offset <= 0xff {
        pci::write_config32(bus, device, function, offset as u8, value);
        return true;
    }
    false
}
pub fn read_config32(device: Device, offset: u16) -> u32 {
    read_config32_at(
        device.bus_kind,
        device.bus,
        device.device,
        device.function,
        offset,
    )
}
pub fn read_bar(device: Device, index: u8) -> u32 {
    if index >= 6 {
        return 0;
    }
    BAR_READS.fetch_add(1, Ordering::Relaxed);
    read_config32(device, 0x10 + u16::from(index) * 4)
}
pub fn read_bar64(device: Device, index: u8) -> u64 {
    let low = read_bar(device, index);
    if low & 1 != 0 || (low >> 1) & 3 != 2 || index >= 5 {
        return u64::from(low);
    }
    (u64::from(read_bar(device, index + 1)) << 32) | u64::from(low)
}
pub fn read_command(device: Device) -> u16 {
    COMMAND_READS.fetch_add(1, Ordering::Relaxed);
    read_config32(device, 4) as u16
}
pub fn write_command(device: Device, command: u16) -> bool {
    write_config32_at(
        device.bus_kind,
        device.bus,
        device.device,
        device.function,
        4,
        u32::from(command),
    )
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptRoute {
    pub line: u8,
    pub pin: u8,
}
pub fn read_interrupt_route(device: Device) -> InterruptRoute {
    INTERRUPT_READS.fetch_add(1, Ordering::Relaxed);
    let raw = read_config32(device, 0x3c);
    if raw == u32::MAX {
        InterruptRoute { line: 0xff, pin: 0 }
    } else {
        InterruptRoute {
            line: raw as u8,
            pin: (raw >> 8) as u8,
        }
    }
}
pub fn performance() -> Performance {
    let ecam = pcie::snapshot();
    let legacy = pci::access_metrics();
    let current = snapshot();
    Performance {
        flags: current.flags,
        generation: current.generation,
        mcfg_segment: u32::from(ecam.segment),
        mcfg_start_bus: u32::from(ecam.start_bus),
        mcfg_end_bus: u32::from(ecam.end_bus),
        found: u64::from(current.device_count),
        stored: u64::from(current.stored_count),
        dropped: u64::from(current.dropped_count),
        ecam_stored: u64::from(current.ecam_stored_count),
        legacy_stored: u64::from(current.legacy_stored_count),
        vendor_probes_ecam: current.ecam_vendor_probes,
        vendor_probes_legacy: current.legacy_vendor_probes,
        class_reads: current.class_reads,
        header_reads: current.header_reads,
        enumeration_config_reads: current.enumeration_config_reads,
        function_pages: current.function_pages,
        early_stops: current.early_stops,
        ecam_config_reads: ecam.config_reads,
        ecam_config_writes: ecam.config_writes,
        legacy_config_reads: legacy.config_reads,
        legacy_config_writes: legacy.config_writes,
        mapping_checks: ecam.mapping_checks,
        mapping_hits: ecam.mapping_hits,
        mapping_misses: ecam.mapping_misses,
        mapping_fast_accesses: ecam.fast_accesses,
        invalid_accesses: ecam.invalid_accesses,
        class_find_calls: CLASS_CALLS.load(Ordering::Relaxed),
        class_candidates: CLASS_CANDIDATES.load(Ordering::Relaxed),
        detail_materializations: DETAILS.load(Ordering::Relaxed),
        interrupt_dword_reads: INTERRUPT_READS.load(Ordering::Relaxed),
        command_reads: COMMAND_READS.load(Ordering::Relaxed),
        bar_reads: BAR_READS.load(Ordering::Relaxed),
        enumeration_total_ns: current.enumeration_total_ns,
        ecam_enumeration_ns: current.ecam_enumeration_ns,
        legacy_enumeration_ns: current.legacy_enumeration_ns,
        timing_unavailable: current.timing_unavailable,
        ..Performance::default()
    }
}
fn log_summary() {
    let current = snapshot();
    pci::puts(b"[PCI] canonical source=");
    pci::puts(if current.ecam_used && current.legacy_used {
        b"ecam+legacy"
    } else if current.ecam_used {
        b"ecam"
    } else {
        b"legacy"
    });
    pci::puts(b" found=");
    pci::dec(u64::from(current.device_count));
    pci::puts(b" stored=");
    pci::dec(u64::from(current.stored_count));
    pci::puts(b" dropped=");
    pci::dec(u64::from(current.dropped_count));
    pci::puts(b" vendor-probes=");
    pci::dec(current.ecam_vendor_probes + current.legacy_vendor_probes);
    pci::puts(b" config-reads=");
    pci::dec(current.enumeration_config_reads);
    pci::puts(b" truncated=");
    pci::puts(if current.truncated { b"yes" } else { b"no" });
    pci::puts(b"\r\n");
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pci_inventory_control(
    operation: u8,
    device: Device,
    argument: u16,
) -> u64 {
    match operation {
        0 => {
            enumerate();
            0
        }
        1 => count() as u64,
        2 => {
            note_detail_materialization();
            0
        }
        3 => u64::from(read_bar(device, argument as u8)),
        4 => read_bar64(device, argument as u8),
        5 => u64::from(read_command(device)),
        6 => u64::from(write_command(device, argument)),
        _ => 0,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_pci_inventory_config(
    operation: u8,
    kind: u8,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
    value: u32,
) -> u64 {
    if operation == 0 {
        u64::from(read_config32_at(kind, bus, device, function, offset))
    } else {
        u64::from(write_config32_at(
            kind, bus, device, function, offset, value,
        ))
    }
}
/// # Safety
/// Output is aligned exclusive C storage after boot inventory publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_inventory_status(output: *mut Status) {
    unsafe { output.write(snapshot()) };
}
/// # Safety
/// Output is aligned exclusive C storage after boot inventory publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_inventory_device(index: u64, output: *mut Device) -> bool {
    let Some(device) = device_at(index as usize) else {
        return false;
    };
    unsafe { output.write(device) };
    true
}
/// # Safety
/// Output is aligned exclusive C storage for one retained index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_inventory_find(
    class: u8,
    subclass: u8,
    start: u64,
    output: *mut u64,
) -> bool {
    let Some(index) = find_by_class(class, subclass, start as usize) else {
        return false;
    };
    unsafe { output.write(index as u64) };
    true
}
/// # Safety
/// Driver-init caller retains device admission and aligned exclusive C output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_inventory_route(
    device: Device,
    output: *mut InterruptRoute,
) {
    unsafe { output.write(read_interrupt_route(device)) };
}
/// # Safety
/// Output is aligned exclusive storage for the canonical Contract type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_inventory_performance(output: *mut Performance) -> i32 {
    unsafe { output.write(performance()) };
    i32::from(snapshot().enumerated)
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 216);
    assert!(core::mem::offset_of!(Status, first_ahci) == 56);
    assert!(core::mem::offset_of!(Status, reason) == 208);
    assert!(core::mem::size_of::<Performance>() == 280);
    assert!(MAX_DEVICES == contract::pci_inventory_capacity as usize);
};
