//! Existing bounded PCI scan and bus-zero ECAM geometry. A reader callback
//! sees published counter changes; no Rust result/state borrow spans it.
use core::{ffi::c_void, ptr};
pub const LEGACY: u8 = 1;
pub const ECAM: u8 = 2;
pub const MAX_DEVICES: usize = 64;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Device {
    pub bus_kind: u8,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
}
impl Device {
    pub const EMPTY: Self = Self {
        bus_kind: 0,
        bus: 0,
        device: 0,
        function: 0,
        vendor_id: 0,
        device_id: 0,
        class_code: 0,
        subclass: 0,
        prog_if: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Metrics {
    pub found: u32,
    pub stored: u32,
    pub dropped: u32,
    pub truncated: bool,
    pub vendor_probes: u64,
    pub class_reads: u64,
    pub header_reads: u64,
    pub config_reads: u64,
    pub function_pages: u64,
    pub early_stops: u64,
}
impl Metrics {
    const EMPTY: Self = Self {
        found: 0,
        stored: 0,
        dropped: 0,
        truncated: false,
        vendor_probes: 0,
        class_reads: 0,
        header_reads: 0,
        config_reads: 0,
        function_pages: 0,
        early_stops: 0,
    };
}
#[repr(C)]
pub struct Result {
    pub devices: [Device; MAX_DEVICES],
    pub metrics: Metrics,
}
impl Result {
    pub const EMPTY: Self = Self {
        devices: [Device::EMPTY; MAX_DEVICES],
        metrics: Metrics::EMPTY,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Reader {
    pub context: *mut c_void,
    pub read_config32: unsafe extern "C" fn(*mut c_void, u8, u8, u8, u16) -> u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Range {
    pub bus_kind: u8,
    pub start_bus: u8,
    pub end_bus: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CoveragePlan {
    pub ranges: [Range; 3],
    pub count: u8,
    pub ecam_used: bool,
    pub legacy_used: bool,
    pub partial: bool,
}
impl CoveragePlan {
    fn append(&mut self, range: Range) {
        if self.count as usize >= self.ranges.len() {
            return;
        }
        self.ranges[self.count as usize] = range;
        self.count += 1;
        if range.bus_kind == ECAM {
            self.ecam_used = true;
        }
        if range.bus_kind == LEGACY {
            self.legacy_used = true;
        }
    }
}
pub fn plan_coverage(ready: bool, segment: u16, start: u8, end: u8) -> CoveragePlan {
    let mut plan = CoveragePlan::default();
    if !ready || segment != 0 || start > end {
        plan.append(Range {
            bus_kind: LEGACY,
            start_bus: 0,
            end_bus: 255,
        });
        return plan;
    }
    plan.append(Range {
        bus_kind: ECAM,
        start_bus: start,
        end_bus: end,
    });
    if start > 0 {
        plan.append(Range {
            bus_kind: LEGACY,
            start_bus: 0,
            end_bus: start - 1,
        });
    }
    if end < 255 {
        plan.append(Range {
            bus_kind: LEGACY,
            start_bus: end + 1,
            end_bus: 255,
        });
    }
    plan.partial = plan.legacy_used;
    plan
}
/// # Safety
/// Result is exclusive resident C storage, with valid capacity counters. The
/// reader/context remain resident through all bounded callback invocations.
pub unsafe fn scan_range(result: *mut Result, range: Range, reader: Reader) -> bool {
    for bus in u16::from(range.start_bus)..=u16::from(range.end_bus) {
        for device in 0..32 {
            for function in 0..8 {
                let raw = unsafe { read(result, reader, bus as u8, device, function, 0, 0) };
                let vendor = raw as u16;
                if vendor == u16::MAX {
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                let class = unsafe { read(result, reader, bus as u8, device, function, 8, 1) };
                let found = Device {
                    bus_kind: range.bus_kind,
                    bus: bus as u8,
                    device,
                    function,
                    vendor_id: vendor,
                    device_id: (raw >> 16) as u16,
                    class_code: (class >> 24) as u8,
                    subclass: (class >> 16) as u8,
                    prog_if: (class >> 8) as u8,
                };
                if !unsafe { store(result, found) } {
                    return false;
                }
                if function == 0 {
                    let header =
                        (unsafe { read(result, reader, bus as u8, device, function, 12, 2) } >> 16)
                            as u8;
                    if header & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }
    true
}
unsafe fn read(
    result: *mut Result,
    reader: Reader,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
    purpose: u8,
) -> u32 {
    let mut metrics = unsafe { ptr::addr_of!((*result).metrics).read() };
    metrics.config_reads = metrics.config_reads.wrapping_add(1);
    match purpose {
        0 => {
            metrics.vendor_probes = metrics.vendor_probes.wrapping_add(1);
            metrics.function_pages = metrics.function_pages.wrapping_add(1);
        }
        1 => metrics.class_reads = metrics.class_reads.wrapping_add(1),
        2 => metrics.header_reads = metrics.header_reads.wrapping_add(1),
        _ => unreachable!(),
    }
    unsafe { ptr::addr_of_mut!((*result).metrics).write(metrics) };
    unsafe { (reader.read_config32)(reader.context, bus, device, function, offset) }
}
unsafe fn store(result: *mut Result, device: Device) -> bool {
    let mut metrics = unsafe { ptr::addr_of!((*result).metrics).read() };
    metrics.found = metrics.found.wrapping_add(1);
    if metrics.stored as usize >= MAX_DEVICES {
        metrics.dropped = metrics.dropped.wrapping_add(1);
        metrics.truncated = true;
        metrics.early_stops = metrics.early_stops.wrapping_add(1);
        unsafe { ptr::addr_of_mut!((*result).metrics).write(metrics) };
        return false;
    }
    unsafe {
        ptr::addr_of_mut!((*result).devices)
            .cast::<Device>()
            .add(metrics.stored as usize)
            .write(device)
    };
    metrics.stored += 1;
    unsafe { ptr::addr_of_mut!((*result).metrics).write(metrics) };
    true
}
pub fn find_by_class(result: &Result, class: u8, subclass: u8, start: usize) -> Option<usize> {
    let stored = (result.metrics.stored as usize).min(MAX_DEVICES);
    (start..stored).find(|&index| {
        result.devices[index].class_code == class && result.devices[index].subclass == subclass
    })
}
pub fn ecam_offset(start: u8, bus: u8, device: u8, function: u8, offset: u16) -> Option<u64> {
    if bus < start || device >= 32 || function >= 8 || offset > 0x0fff {
        return None;
    }
    Some(
        (u64::from(bus) << 20)
            | (u64::from(device) << 15)
            | (u64::from(function) << 12)
            | u64::from(offset & 0x0ffc),
    )
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EcamWindow {
    pub base: u64,
    pub bytes: u64,
}
pub fn ecam_window(base: u64, start: u8, end: u8) -> Option<EcamWindow> {
    if base == 0 || start > end {
        return None;
    }
    let first = u64::from(start) << 20;
    let last = (u64::from(end) + 1) << 20;
    base.checked_add(last)?;
    Some(EcamWindow {
        base: base + first,
        bytes: last - first,
    })
}
/// # Safety
/// Output is aligned exclusive C storage for one CoveragePlan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_scan_plan(
    ready: bool,
    segment: u16,
    start: u8,
    end: u8,
    output: *mut CoveragePlan,
) {
    unsafe { output.write(plan_coverage(ready, segment, start, end)) };
}
/// # Safety
/// Result and reader obey scan_range's resident exclusive callback contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_scan_range(
    result: *mut Result,
    range: Range,
    reader: Reader,
) -> bool {
    unsafe { scan_range(result, range, reader) }
}
/// # Safety
/// Result is immutable resident C storage and output is exclusive C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_scan_find(
    result: *const Result,
    class: u8,
    subclass: u8,
    start: u64,
    output: *mut u64,
) -> bool {
    let Some(index) = find_by_class(unsafe { &*result }, class, subclass, start as usize) else {
        return false;
    };
    unsafe { output.write(index as u64) };
    true
}
/// # Safety
/// Output is exclusive aligned C storage for one u64.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_ecam_offset(
    start: u8,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
    output: *mut u64,
) -> bool {
    let Some(value) = ecam_offset(start, bus, device, function, offset) else {
        return false;
    };
    unsafe { output.write(value) };
    true
}
/// # Safety
/// Output is exclusive aligned C storage for one EcamWindow.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_pci_ecam_window(
    base: u64,
    start: u8,
    end: u8,
    output: *mut EcamWindow,
) -> bool {
    let Some(value) = ecam_window(base, start, end) else {
        return false;
    };
    unsafe { output.write(value) };
    true
}
const _: () = {
    assert!(core::mem::size_of::<Device>() == 12);
    assert!(core::mem::size_of::<Metrics>() == 64);
    assert!(core::mem::size_of::<Result>() == 832);
    assert!(core::mem::size_of::<Range>() == 3);
    assert!(core::mem::size_of::<CoveragePlan>() == 13);
    assert!(core::mem::size_of::<Reader>() == 16);
};
