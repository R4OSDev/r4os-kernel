//! Original linear BSP storage foundation/controller admission and evidence.
//! Generic FS/USB/display leaves stay at their current owners. All ordering,
//! source selection, fallback decisions and retained scan outcomes live here.
use crate::storage::{ahci, block_registry, mbr, normal_boot, partition_table::Label};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Counts {
    pub xhci: u32,
    pub ahci: u32,
    pub nvme: u32,
}
#[repr(C)]
struct RegistryEntry {
    source: u8,
    state: u8,
}
#[repr(C)]
struct BlockBasic {
    bus: u8,
    source: u8,
}
#[repr(C)]
struct BackendStatus {
    state: u32,
    last_error: u32,
    last_lba: u64,
    last_sectors: u32,
    recoveries: u64,
    recovery_failures: u64,
}
#[repr(C)]
struct DeviceDiagnostics {
    name: Label,
    bus: Label,
    source: Label,
    state: Label,
    last_error: Label,
    sector_size: u32,
    sector_count: u64,
    read_failures: u64,
    read_ops: u64,
    read_sectors: u64,
    last_lba: u64,
    completion_ticks: u64,
    last_sectors: u16,
}
#[derive(Clone, Copy)]
struct Evidence {
    index: usize,
    name: Label,
    report: mbr::Report,
}
const EMPTY_EVIDENCE: Evidence = Evidence {
    index: 0,
    name: Label::new(b""),
    report: mbr::Report {
        table: 0,
        result: 0,
        detail: Label::new(b""),
        table_valid: false,
        partition_count: 0,
        mountable_count: 0,
        filesystem_count: 0,
        best_system_score: 0,
        marker_found_mask: 0,
        marker_io_mask: 0,
        mbr_type: 0,
    },
};
struct State {
    foundation: bool,
    controllers: bool,
    scans: usize,
    successes: usize,
    evidence: [Evidence; 8],
    evidence_count: usize,
    ahci: u32,
    nvme: u32,
}
struct Storage(UnsafeCell<State>);
// The same original linear BSP owner publishes boot flags and scan evidence.
// Raw resident addresses are never borrowed across USB/FS/log callbacks.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    foundation: false,
    controllers: false,
    scans: 0,
    successes: 0,
    evidence: [EMPTY_EVIDENCE; 8],
    evidence_count: 0,
    ahci: 0,
    nvme: 0,
}));
fn state() -> *mut State {
    STATE.0.get()
}
unsafe extern "C" {
    fn r4os_zig_storage_boot_foundation(operation: u8) -> bool;
    fn r4os_zig_storage_boot_registry(
        operation: u8,
        index: usize,
        name: Label,
        kind: u16,
        version: u32,
        out: *mut RegistryEntry,
    ) -> usize;
    fn r4os_zig_storage_boot_usb(operation: u8, index: usize) -> usize;
    fn r4os_zig_storage_boot_block(index: usize, out: *mut BlockBasic) -> bool;
    fn r4os_zig_storage_boot_device_diagnostics(index: usize, out: *mut DeviceDiagnostics) -> bool;
    fn r4os_zig_storage_boot_backend(index: usize, out: *mut BackendStatus) -> bool;
    fn r4os_zig_storage_boot_fail(message: Label) -> bool;
    fn r4os_zig_storage_boot_output(operation: u8, text: Label, value: u64);
    fn r4os_zig_storage_boot_data_role() -> usize;
    fn r4os_zig_storage_boot_data_root(consume: unsafe extern "C" fn(*mut c_void));
    fn r4os_zig_storage_boot_data_path(context: *mut c_void, operation: u8, text: Label) -> bool;
}
fn output(operation: u8, text: &'static [u8], value: u64) {
    unsafe { r4os_zig_storage_boot_output(operation, Label::new(text), value) }
}
fn put(text: &'static [u8]) {
    output(0, text, 0)
}
fn dec(value: u64) {
    output(1, b"", value)
}
fn fail(text: Label) -> bool {
    unsafe { r4os_zig_storage_boot_fail(text) }
}
fn usb(operation: u8, index: usize) -> usize {
    unsafe { r4os_zig_storage_boot_usb(operation, index) }
}
fn registry(
    operation: u8,
    index: usize,
    name: &'static [u8],
    kind: u16,
    version: u32,
    out: *mut RegistryEntry,
) -> usize {
    unsafe {
        r4os_zig_storage_boot_registry(operation, index, Label::new(name), kind, version, out)
    }
}
fn find(name: &'static [u8]) -> usize {
    registry(0, 0, name, 0, 0, ptr::null_mut())
}
fn entry(index: usize) -> Option<RegistryEntry> {
    let mut out = MaybeUninit::uninit();
    if registry(1, index, b"", 0, 0, out.as_mut_ptr()) == 0 {
        None
    } else {
        Some(unsafe { out.assume_init() })
    }
}
fn begin(name: &'static [u8], kind: u16, version: u32) -> usize {
    registry(2, 0, name, kind, version, ptr::null_mut())
}
fn mark(slot: usize, status: u8) {
    if slot != usize::MAX {
        registry(3, slot, b"", 0, u32::from(status), ptr::null_mut());
    }
}
fn slots() -> usize {
    block_registry::r4os_rust_block_registry_query(1, 0)
}
fn basic(index: usize) -> Option<BlockBasic> {
    let mut out = MaybeUninit::uninit();
    if unsafe { r4os_zig_storage_boot_block(index, out.as_mut_ptr()) } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
fn has_preload(bus: u8) -> bool {
    for i in 0..slots() {
        if let Some(d) = basic(i)
            && d.bus == bus
            && d.source == 1
        {
            return true;
        }
    }
    false
}
fn probe_xhci(count: u32) {
    let index = usb(0, 0);
    if index != usize::MAX {
        let source = usb(1, index);
        if source == usize::MAX {
            return;
        }
        put(b"[XHCI] canonical host already initialized source=");
        put(match source {
            0 => b"built-in",
            1 => b"preload",
            2 => b"disk",
            _ => unreachable!(),
        });
        put(b"; no second controller owner\r\n");
        return;
    }
    let slot = if count > 0 {
        begin(b"XHCI", 255, 2)
    } else {
        usize::MAX
    };
    put(b"[XHCI][WARN] activation R4D missing; starting sole built-in rescue owner\r\n");
    if usb(2, 0) != 0 {
        mark(slot, 2);
        mark(slot, 3)
    } else {
        mark(slot, 6)
    }
}
fn probe_usb_msc() {
    let existing = find(b"USBMSC");
    let slot = if existing != usize::MAX {
        match entry(existing) {
            Some(e) if e.source == 2 && e.state == 3 => {
                put(b"[USBMSC] preload R4D active; legacy rescue data path armed; owner=preload\r\n");
                usb(3, 0);
                existing
            }
            Some(_) => {
                usb(4, 0);
                begin(b"USBMSC", 2, 1)
            }
            None => begin(b"USBMSC", 2, 1),
        }
    } else {
        usb(4, 0);
        begin(b"USBMSC", 2, 1)
    };
    if usb(5, 0) != 0 {
        mark(slot, 2);
        if usb(6, 0) > 0 {
            mark(slot, 3)
        }
    } else {
        mark(slot, 6)
    }
}
fn probe_atapio() {
    let index = find(b"ATAPIO");
    if index != usize::MAX
        && let Some(e) = entry(index)
        && e.source == 2
        && e.state == 3
    {
        put(if has_preload(1) {
            b"[ATAPIO] preload R4D active; built-in ATA-PIO data path removed\r\n"
        } else {
            b"[ATAPIO][WARN] preload R4D active without blockdevice; no built-in ATA-PIO fallback in standard kernel\r\n"
        });
        return;
    }
    put(b"[ATAPIO][WARN] preload R4D missing; no built-in ATA-PIO fallback in standard kernel\r\n");
}
fn probe_ahci(count: u32) {
    if count == 0 {
        ahci::r4os_rust_ahci_control(3, 0);
        ahci::probe();
        return;
    }
    let existing = find(b"AHCI");
    if existing != usize::MAX {
        let Some(e) = entry(existing) else {
            put(b"[AHCI][WARN] canonical preload registry entry unavailable; no second controller owner\r\n");
            return;
        };
        if e.source == 2 && e.state == 3 {
            put(if has_preload(2) {
                b"[AHCI] canonical preload R4D active; owner=preload\r\n"
            } else {
                b"[AHCI][WARN] canonical preload R4D active without blockdevice; no second controller owner\r\n"
            });
            return;
        }
        put(b"[AHCI][WARN] canonical preload R4D failed or inactive; built-in depth-one rescue requested\r\n");
    } else {
        put(b"[AHCI][WARN] canonical preload R4D missing; built-in depth-one rescue requested\r\n");
    }
    ahci::r4os_rust_ahci_control(3, 0);
    let slot = begin(b"AHCI", 2, 1);
    if ahci::probe() {
        mark(slot, 2);
        if ahci::r4os_rust_ahci_control(1, 0) > 0 {
            mark(slot, 3)
        }
    } else {
        mark(slot, 6)
    }
}
fn probe_nvme(count: u32) {
    if count == 0 {
        return;
    }
    let index = find(b"NVME");
    if index != usize::MAX {
        let Some(e) = entry(index) else {
            put(b"[NVME][WARN] canonical preload registry entry unavailable\r\n");
            return;
        };
        if e.source == 2 && e.state == 3 {
            put(if has_preload(3) {
                b"[NVME] canonical preload R4D active; owner=preload\r\n"
            } else {
                b"[NVME][WARN] canonical preload R4D active without blockdevice; no second controller owner\r\n"
            });
            return;
        }
        put(b"[NVME][WARN] canonical preload R4D failed or inactive; no second controller owner\r\n");
        return;
    }
    put(b"[NVME][WARN] canonical preload R4D missing; no built-in controller fallback\r\n");
}
fn scan(scanned: &mut [bool; 16], index: usize, name: &'static [u8]) {
    if index < scanned.len() {
        if scanned[index] {
            return;
        }
        scanned[index] = true
    }
    put(b"  Storage scan ");
    put(name);
    put(b" block=");
    dec(index as u64);
    put(b"\r\n");
    unsafe { (*state()).scans += 1 };
    let mut report = MaybeUninit::uninit();
    unsafe { mbr::r4os_rust_mbr_scan(index, report.as_mut_ptr()) };
    let report = unsafe { report.assume_init() };
    unsafe {
        let s = state();
        if report.table_valid {
            (*s).successes += 1
        }
        let n = (*s).evidence_count;
        if n < 8 {
            (*s).evidence[n] = Evidence {
                index,
                name: Label::new(name),
                report,
            };
            (*s).evidence_count += 1;
        }
    }
}
fn scan_registered(boot: usize) {
    let mut scanned = [false; 16];
    scan(&mut scanned, boot, b"BOOT-MBR");
    let index = usb(7, 0);
    if index != usize::MAX {
        usb(8, 0);
        scan(&mut scanned, index, b"USBMSC")
    }
    for port in 0..8 {
        let index = ahci::r4os_rust_ahci_control(5, port);
        if index != usize::MAX {
            scan(&mut scanned, index, b"AHCI")
        }
    }
    for index in 0..slots() {
        let Some(d) = basic(index) else { continue };
        if d.source == 0 && d.bus != 4 {
            continue;
        }
        let name: &'static [u8] = match d.bus {
            3 => b"NVME.R4D",
            2 => b"AHCI.R4D",
            4 => b"USBMSC.R4D",
            1 => b"ATAPIO.R4D",
            _ => b"R4D",
        };
        scan(&mut scanned, index, name)
    }
}
unsafe extern "C" fn data_directories(context: *mut c_void) {
    for name in [b"DOCS".as_slice(), b"TEMP", b"MEDIA"] {
        let mut path = [0; 16];
        path[0] = b'\\';
        let count = name.len().min(14);
        path[1..1 + count].copy_from_slice(&name[..count]);
        if unsafe {
            r4os_zig_storage_boot_data_path(
                context,
                0,
                Label {
                    data: path.as_ptr(),
                    len: 1 + count,
                },
            )
        } {
            continue;
        }
        let ok = unsafe { r4os_zig_storage_boot_data_path(context, 1, Label::new(name)) };
        put(b"  D:\\");
        put(name);
        put(if ok { b" [OK]\r\n" } else { b" [WARN]\r\n" });
    }
}
fn data_layout() {
    if unsafe { r4os_zig_storage_boot_data_role() } != 2 {
        return;
    }
    unsafe { r4os_zig_storage_boot_data_root(data_directories) }
}
fn foundation() -> bool {
    unsafe {
        if (*state()).foundation {
            return true;
        }
        if !r4os_zig_storage_boot_foundation(0) {
            return fail(Label::new(b"Storage foundation before memory core"));
        }
        for operation in 1..=5 {
            r4os_zig_storage_boot_foundation(operation);
        }
        (*state()).foundation = true;
        put(b"  Storage foundation ");
        put(b"[OK]\r\n");
        true
    }
}
fn controllers(counts: Counts) -> bool {
    unsafe {
        if (*state()).controllers {
            return true;
        }
        if !(*state()).foundation {
            return fail(Label::new(b"Storage controllers before foundation"));
        }
        (*state()).scans = 0;
        (*state()).successes = 0;
        (*state()).evidence = [EMPTY_EVIDENCE; 8];
        (*state()).evidence_count = 0;
        (*state()).ahci = counts.ahci;
        (*state()).nvme = counts.nvme;
        probe_xhci(counts.xhci);
        probe_usb_msc();
        probe_atapio();
        probe_ahci(counts.ahci);
        probe_nvme(counts.nvme);
        let mut result = MaybeUninit::uninit();
        normal_boot::r4os_rust_normal_mount(result.as_mut_ptr());
        let result = result.assume_init();
        match result.kind {
            0 => {}
            1 => scan_registered(result.index),
            2 => return fail(result.reason),
            _ => unreachable!(),
        }
        data_layout();
        (*state()).controllers = true;
        output(2, b"  Storage [OK]\r\n", 0);
        true
    }
}
fn diag_text(text: &'static [u8]) {
    output(4, text, 0)
}
fn diag_raw(text: Label) {
    unsafe { r4os_zig_storage_boot_output(4, text, 0) }
}
fn diag_dec(value: u64) {
    output(5, b"", value)
}
fn diag_hex(value: u64) {
    output(6, b"", value)
}
fn diag_line(text: &'static [u8]) {
    output(7, text, 0)
}
fn diag_end() {
    output(8, b"", 0)
}
fn diagnostics() {
    output(3, b"", 0);
    diag_text(b"[STORAGE] C: missing blocks=");
    diag_dec(block_registry::r4os_rust_block_registry_query(0, 0) as u64);
    diag_text(b" scans=");
    diag_dec(unsafe { (*state()).scans } as u64);
    diag_text(b" pci-ahci=");
    diag_dec(u64::from(unsafe { (*state()).ahci }));
    diag_text(b" pci-nvme=");
    diag_dec(u64::from(unsafe { (*state()).nvme }));
    diag_end();
    let count = unsafe { (*state()).evidence_count };
    if count == 0 {
        diag_line(b"[STORAGE] no registered block device was scanned");
        return;
    }
    for index in 0..count {
        let e = unsafe { (*state()).evidence[index] };
        let mut d = MaybeUninit::uninit();
        if !unsafe { r4os_zig_storage_boot_device_diagnostics(e.index, d.as_mut_ptr()) } {
            diag_text(b"dev#");
            diag_dec(e.index as u64);
            diag_line(b" unavailable after scan");
            continue;
        }
        let d = unsafe { d.assume_init() };
        diag_text(b"dev#");
        diag_dec(e.index as u64);
        diag_text(b" ");
        diag_raw(d.name);
        diag_text(b" scan=");
        diag_raw(e.name);
        diag_text(b" bus=");
        diag_raw(d.bus);
        diag_text(b" src=");
        diag_raw(d.source);
        diag_text(b" state=");
        diag_raw(d.state);
        diag_end();
        diag_text(b"  sector-size=");
        diag_dec(u64::from(d.sector_size));
        diag_text(b" sectors=");
        diag_dec(d.sector_count);
        diag_text(b" table=");
        diag_text(match e.report.table {
            0 => b"none",
            1 => b"mbr",
            2 => b"gpt_primary",
            3 => b"gpt_backup",
            _ => unreachable!(),
        });
        diag_end();
        const RESULTS: [&[u8]; 22] = [
            b"device_missing",
            b"logical_sector_unsupported",
            b"capacity_unavailable",
            b"sector_zero_read_failed",
            b"mbr_signature_invalid",
            b"no_mountable_partition",
            b"unsupported_partition",
            b"partition_invalid",
            b"gpt_invalid",
            b"gpt_entry_read_failed",
            b"partition_lba_unsupported",
            b"filesystem_probe_read_failed",
            b"filesystem_lookup_failed",
            b"filesystem_unknown",
            b"fat32_invalid",
            b"ntfs_invalid",
            b"boot_volume_only",
            b"data_volume_only",
            b"partition_size_unsupported",
            b"drive_letters_exhausted",
            b"mount_rejected",
            b"system_mounted",
        ];
        diag_text(b"  result=");
        diag_text(RESULTS[usize::from(e.report.result)]);
        diag_text(b" parts=");
        diag_dec(u64::from(e.report.partition_count));
        diag_text(b" candidates=");
        diag_dec(u64::from(e.report.mountable_count));
        diag_text(b" fs=");
        diag_dec(u64::from(e.report.filesystem_count));
        diag_text(b" score=");
        diag_dec(u64::from(e.report.best_system_score));
        diag_text(b"/6");
        diag_text(b" markers=");
        diag_hex(u64::from(e.report.marker_found_mask));
        diag_text(b" marker-io=");
        diag_hex(u64::from(e.report.marker_io_mask));
        diag_end();
        if e.report.detail.len != 0 || e.report.mbr_type != 0 || d.read_failures != 0 {
            diag_text(b"  detail=");
            if e.report.detail.len != 0 {
                diag_raw(e.report.detail)
            } else {
                diag_text(b"none")
            };
            if e.report.mbr_type != 0 {
                diag_text(b" mbr-type=");
                diag_hex(u64::from(e.report.mbr_type))
            }
            if d.read_failures != 0 {
                diag_text(b" read-failures=");
                diag_dec(d.read_failures);
                diag_text(b" last-io=");
                diag_raw(d.last_error)
            }
            diag_end();
        }
        diag_text(b"  reads=");
        diag_dec(d.read_ops);
        diag_text(b" sectors=");
        diag_dec(d.read_sectors);
        diag_text(b" last-lba=");
        diag_dec(d.last_lba);
        diag_text(b" n=");
        diag_dec(u64::from(d.last_sectors));
        diag_text(b" ticks=");
        diag_dec(d.completion_ticks);
        diag_end();
        let mut backend = MaybeUninit::uninit();
        if unsafe { r4os_zig_storage_boot_backend(e.index, backend.as_mut_ptr()) } {
            let backend = unsafe { backend.assume_init() };
            diag_text(b"  backend-state=");
            diag_dec(u64::from(backend.state));
            diag_text(b" error=");
            diag_hex(u64::from(backend.last_error));
            diag_text(b" recoveries=");
            diag_dec(backend.recoveries);
            diag_text(b" failed=");
            diag_dec(backend.recovery_failures);
            diag_end();
        }
    }
}
/// # Safety
/// Controller operation2 receives one immutable initialized C count record.
/// All initialization follows the existing exclusive BSP boot ordering.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_boot(operation: u8, counts: *const Counts) -> usize {
    match operation {
        0 => usize::from(foundation()),
        1 => usize::from(unsafe { (*state()).foundation }),
        2 => usize::from(controllers(unsafe { counts.read() })),
        3 => usize::from(unsafe { (*state()).controllers }),
        4 => unsafe { (*state()).scans },
        5 => unsafe { (*state()).successes },
        6 => {
            diagnostics();
            0
        }
        _ => unreachable!(),
    }
}
const _: () = {
    assert!(core::mem::size_of::<Counts>() == 12);
    assert!(core::mem::size_of::<BackendStatus>() == 40);
    assert!(core::mem::size_of::<DeviceDiagnostics>() == 144);
};
