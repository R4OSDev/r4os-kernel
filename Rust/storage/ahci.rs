//! Existing boot-only AHCI fallback. Productive AHCI R4Ds stay independent.
//! Probe is BSP-exclusive; block callbacks retain their original serialized
//! device lane. No Rust reference to resident state spans registration or any
//! other owner call. DMA allocations and bounded MMIO polling match Zig.
use crate::{
    boot::log,
    memory::{paging, phys},
    platform::{pci_inventory as pci, pci_scan::Device},
    storage::partition_table::Label,
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
const MAX_PORTS: usize = 8;
const WAIT_GUARD: u32 = 1_000_000;
const MAX_SECTORS: u16 = 8;
const CAP: u64 = 0;
const GHC: u64 = 4;
const IS: u64 = 8;
const PI: u64 = 12;
const VS: u64 = 16;
const CAP2: u64 = 36;
const BOHC: u64 = 40;
const CLB: u64 = 0;
const CLBU: u64 = 4;
const FB: u64 = 8;
const FBU: u64 = 12;
const PIS: u64 = 16;
const IE: u64 = 20;
const CMD: u64 = 24;
const TFD: u64 = 32;
const SIG: u64 = 36;
const SSTS: u64 = 40;
const SCTL: u64 = 44;
const SERR: u64 = 48;
const CI: u64 = 56;
const ST: u32 = 1;
const SUD: u32 = 2;
const POD: u32 = 4;
const FRE: u32 = 16;
const FR: u32 = 1 << 14;
const CR: u32 = 1 << 15;
const TFES: u32 = 1 << 30;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PortStatus {
    pub index: u8,
    pub implemented: bool,
    pub configured: bool,
    pub block_device_index: usize,
    pub sig: u32,
    pub ssts: u32,
    pub sctl: u32,
    pub serr: u32,
    pub tfd: u32,
    pub cmd: u32,
    pub ci: u32,
    pub commands: u64,
    pub failures: u64,
    pub timeouts: u64,
    pub recoveries: u64,
    pub last_command: u8,
    pub last_lba: u64,
    pub last_sectors: u16,
    pub last_error: Label,
}
impl PortStatus {
    const EMPTY: Self = Self {
        index: 0,
        implemented: false,
        configured: false,
        block_device_index: usize::MAX,
        sig: 0,
        ssts: 0,
        sctl: 0,
        serr: 0,
        tfd: 0,
        cmd: 0,
        ci: 0,
        commands: 0,
        failures: 0,
        timeouts: 0,
        recoveries: 0,
        last_command: 0,
        last_lba: 0,
        last_sectors: 0,
        last_error: Label::new(b"none"),
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub present: bool,
    pub mapped: bool,
    pub device: Device,
    pub bar5_raw: u32,
    pub mmio_phys: u64,
    pub mmio_virt: u64,
    pub cap: u32,
    pub ghc: u32,
    pub is: u32,
    pub pi: u32,
    pub vs: u32,
    pub cap2: u32,
    pub bohc: u32,
    pub ports_advertised: u8,
    pub command_slots: u8,
    pub supports_64bit: bool,
    pub supports_ncq: bool,
    pub supports_staggered_spinup: bool,
    pub implemented_ports: u8,
    pub active_sata_ports: u8,
    pub configured_ports: u8,
    pub block_devices: u8,
    pub hba_reset_ok: bool,
    pub bios_handoff: Label,
    pub commands: u64,
    pub failures: u64,
    pub timeouts: u64,
    pub recoveries: u64,
    pub first_ports: [PortStatus; MAX_PORTS],
    pub reason: Label,
}
impl Status {
    const EMPTY: Self = Self {
        present: false,
        mapped: false,
        device: Device::EMPTY,
        bar5_raw: 0,
        mmio_phys: 0,
        mmio_virt: 0,
        cap: 0,
        ghc: 0,
        is: 0,
        pi: 0,
        vs: 0,
        cap2: 0,
        bohc: 0,
        ports_advertised: 0,
        command_slots: 0,
        supports_64bit: false,
        supports_ncq: false,
        supports_staggered_spinup: false,
        implemented_ports: 0,
        active_sata_ports: 0,
        configured_ports: 0,
        block_devices: 0,
        hba_reset_ok: false,
        bios_handoff: Label::new(b"not-needed"),
        commands: 0,
        failures: 0,
        timeouts: 0,
        recoveries: 0,
        first_ports: [PortStatus::EMPTY; MAX_PORTS],
        reason: Label::new(b"not initialized"),
    };
}
#[derive(Clone, Copy)]
struct Runtime {
    port: u8,
    // Retained original write-only boot marker, separate from HW snapshots.
    _implemented: bool,
    configured: bool,
    device_present: bool,
    command_list_phys: u64,
    fis_phys: u64,
    command_table_phys: u64,
    dma_phys: u64,
    command_list_virt: u64,
    fis_virt: u64,
    command_table_virt: u64,
    dma_virt: u64,
    block_device_index: usize,
    commands: u64,
    failures: u64,
    timeouts: u64,
    recoveries: u64,
    last_command: u8,
    last_lba: u64,
    last_sectors: u16,
    last_error: Label,
}
impl Runtime {
    const EMPTY: Self = Self {
        port: 0,
        _implemented: false,
        configured: false,
        device_present: false,
        command_list_phys: 0,
        fis_phys: 0,
        command_table_phys: 0,
        dma_phys: 0,
        command_list_virt: 0,
        fis_virt: 0,
        command_table_virt: 0,
        dma_virt: 0,
        block_device_index: usize::MAX,
        commands: 0,
        failures: 0,
        timeouts: 0,
        recoveries: 0,
        last_command: 0,
        last_lba: 0,
        last_sectors: 0,
        last_error: Label::new(b"none"),
    };
}
struct State {
    current: Status,
    runtimes: [Runtime; MAX_PORTS],
    registered: [usize; MAX_PORTS],
    source: u8,
    external_owner: bool,
}
struct Storage(UnsafeCell<State>);
// Original BSP probe / serialized per-device callback ownership; no new lock.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    current: Status::EMPTY,
    runtimes: [Runtime::EMPTY; MAX_PORTS],
    registered: [usize::MAX; MAX_PORTS],
    source: 0,
    external_owner: false,
}));
fn state() -> *mut State {
    STATE.0.get()
}
fn current() -> *mut Status {
    unsafe { ptr::addr_of_mut!((*state()).current) }
}
fn runtime(port: usize) -> *mut Runtime {
    unsafe {
        ptr::addr_of_mut!((*state()).runtimes)
            .cast::<Runtime>()
            .add(port)
    }
}
unsafe extern "C" {
    fn r4os_zig_ahci_register(port: u8, source: u8, context: *mut c_void, name: Label) -> usize;
}
fn put(text: &[u8]) {
    unsafe {
        log::r4os_rust_bootlog_write(text.as_ptr(), text.len());
    }
}
fn dec(value: u64) {
    log::r4os_rust_bootlog_dec(value);
}
fn hex(value: u64, width: usize) {
    log::r4os_rust_bootlog_hex(value, width);
}
fn read(offset: u64) -> u32 {
    unsafe { ptr::read_volatile(((*current()).mmio_virt + offset) as *const u32) }
}
fn write(offset: u64, value: u32) {
    unsafe {
        ptr::write_volatile(((*current()).mmio_virt + offset) as *mut u32, value);
    }
}
fn base(port: u8) -> u64 {
    0x100 + u64::from(port) * 0x80
}
fn read_port(port: u8) -> PortStatus {
    let b = base(port);
    PortStatus {
        index: port,
        implemented: true,
        sig: read(b + SIG),
        ssts: read(b + SSTS),
        sctl: read(b + SCTL),
        serr: read(b + SERR),
        tfd: read(b + TFD),
        cmd: read(b + CMD),
        ci: read(b + CI),
        ..PortStatus::EMPTY
    }
}
fn active(p: PortStatus) -> bool {
    p.ssts & 15 == 3 && matches!((p.ssts >> 8) & 15, 1 | 2) && p.sig == 0x101
}
fn skip(p: PortStatus) -> Option<&'static [u8]> {
    let det = p.ssts & 15;
    let ipm = (p.ssts >> 8) & 15;
    if det == 0 {
        return Some(b"no-device");
    };
    if det != 3 {
        return Some(b"link-not-ready");
    };
    if ipm != 1 && ipm != 2 {
        return Some(b"link-power-inactive");
    };
    match p.sig {
        0x101 => None,
        0xeb140101 => Some(b"unsupported-atapi"),
        0xc33c0101 => Some(b"unsupported-enclosure"),
        0x96690101 => Some(b"unsupported-port-multiplier"),
        _ => Some(b"unsupported-device-type"),
    }
}
fn name(port: u8) -> Label {
    Label::new(match port {
        0 => b"ahci0",
        1 => b"ahci1",
        2 => b"ahci2",
        3 => b"ahci3",
        4 => b"ahci4",
        5 => b"ahci5",
        6 => b"ahci6",
        7 => b"ahci7",
        _ => b"ahci?",
    })
}
fn map_mmio(physical: u64) -> bool {
    for offset in (0..0x2000).step_by(4096) {
        let pa = (physical + offset) & !4095;
        let va = phys::r4os_rust_phys_to_virt(pa);
        let mut ignored = 0;
        if !unsafe { paging::r4os_rust_paging_read(va, 3, false, &mut ignored) }
            && !paging::r4os_rust_paging_map(va, pa, 2 | 16 | (1 << 63), false)
        {
            return false;
        }
    }
    true
}
fn alloc_zero() -> Option<u64> {
    let mut frame = 0;
    if !unsafe { phys::r4os_rust_phys_alloc(&mut frame) } {
        return None;
    };
    unsafe { ptr::write_bytes(phys::r4os_rust_phys_to_virt(frame) as *mut u8, 0, 4096) };
    Some(frame)
}
fn bios_handoff() {
    let c = current();
    unsafe {
        (*c).bohc = read(BOHC);
        if (*c).bohc & 1 == 0 {
            (*c).bios_handoff = Label::new(b"not-needed");
            return;
        };
        write(BOHC, (*c).bohc | 2);
        let mut guard = 0;
        while read(BOHC) & 1 != 0 && guard < WAIT_GUARD {
            guard += 1
        }
        (*c).bohc = read(BOHC);
        (*c).bios_handoff = Label::new(if (*c).bohc & 1 == 0 {
            b"ok"
        } else {
            b"timeout"
        });
    }
}
fn reset_controller() -> bool {
    write(GHC, read(GHC) | (1 << 31));
    write(GHC, read(GHC) | 1);
    let mut guard = 0;
    while read(GHC) & 1 != 0 && guard < WAIT_GUARD {
        guard += 1
    }
    write(GHC, read(GHC) | (1 << 31));
    read(GHC) & 1 == 0
}
unsafe fn stop(b: u64, rt: *mut Runtime) -> bool {
    write(b + CMD, read(b + CMD) & !ST);
    let mut guard = 0;
    while read(b + CMD) & CR != 0 && guard < WAIT_GUARD {
        guard += 1
    }
    write(b + CMD, read(b + CMD) & !FRE);
    guard = 0;
    while read(b + CMD) & FR != 0 && guard < WAIT_GUARD {
        guard += 1
    }
    if read(b + CMD) & (CR | FR) != 0 {
        unsafe { (*rt).last_error = Label::new(b"stop-timeout") };
        return false;
    }
    true
}
unsafe fn start(b: u64, rt: *mut Runtime) -> bool {
    write(b + CMD, read(b + CMD) | FRE | ST | SUD | POD);
    let mut guard = 0;
    while read(b + CMD) & CR == 0 && guard < WAIT_GUARD {
        guard += 1
    }
    if read(b + CMD) & CR == 0 {
        unsafe { (*rt).last_error = Label::new(b"start-timeout") };
        return false;
    }
    true
}
unsafe fn allocate(rt: *mut Runtime) -> bool {
    unsafe {
        let Some(frame) = alloc_zero() else {
            (*rt).last_error = Label::new(b"alloc-command-list");
            return false;
        };
        (*rt).command_list_phys = frame;
        let Some(frame) = alloc_zero() else {
            (*rt).last_error = Label::new(b"alloc-fis");
            return false;
        };
        (*rt).fis_phys = frame;
        let Some(frame) = alloc_zero() else {
            (*rt).last_error = Label::new(b"alloc-command-table");
            return false;
        };
        (*rt).command_table_phys = frame;
        let Some(frame) = alloc_zero() else {
            (*rt).last_error = Label::new(b"alloc-dma");
            return false;
        };
        (*rt).dma_phys = frame;
        (*rt).command_list_virt = phys::r4os_rust_phys_to_virt((*rt).command_list_phys);
        (*rt).fis_virt = phys::r4os_rust_phys_to_virt((*rt).fis_phys);
        (*rt).command_table_virt = phys::r4os_rust_phys_to_virt((*rt).command_table_phys);
        (*rt).dma_virt = phys::r4os_rust_phys_to_virt((*rt).dma_phys);
        true
    }
}
unsafe fn prepare(rt: *mut Runtime) -> bool {
    unsafe {
        if !allocate(rt) {
            return false;
        }
        let b = base((*rt).port);
        if !stop(b, rt) {
            return false;
        }
        write(b + CLB, (*rt).command_list_phys as u32);
        write(b + CLBU, ((*rt).command_list_phys >> 32) as u32);
        write(b + FB, (*rt).fis_phys as u32);
        write(b + FBU, ((*rt).fis_phys >> 32) as u32);
        write(b + SERR, u32::MAX);
        write(b + PIS, u32::MAX);
        write(b + IE, 0);
        if !start(b, rt) {
            return false;
        }
        (*rt).configured = true;
        (*rt).last_error = Label::new(b"none");
        true
    }
}
unsafe fn snapshot(rt: *mut Runtime) {
    unsafe {
        let port = (*rt).port;
        if usize::from(port) >= MAX_PORTS {
            return;
        }
        let mut p = read_port(port);
        p.configured = (*rt).configured;
        p.block_device_index = (*rt).block_device_index;
        p.commands = (*rt).commands;
        p.failures = (*rt).failures;
        p.timeouts = (*rt).timeouts;
        p.recoveries = (*rt).recoveries;
        p.last_command = (*rt).last_command;
        p.last_lba = (*rt).last_lba;
        p.last_sectors = (*rt).last_sectors;
        p.last_error = (*rt).last_error;
        (*current()).first_ports[usize::from(port)] = p;
    }
}
fn configure() {
    unsafe {
        let c = current();
        (*c).configured_ports = 0;
        (*c).block_devices = 0;
        (*c).active_sata_ports = 0;
        for port in 0..MAX_PORTS {
            if (*c).pi & (1 << port) == 0 {
                continue;
            }
            let rt = runtime(port);
            rt.write(Runtime {
                port: port as u8,
                _implemented: true,
                ..Runtime::EMPTY
            });
            if !prepare(rt) {
                let mut p = read_port(port as u8);
                p.last_error = (*rt).last_error;
                (*c).first_ports[port] = p;
                continue;
            }
            (*c).configured_ports += 1;
            if let Some(reason) = skip(read_port(port as u8)) {
                (*rt).last_error = Label::new(reason);
                snapshot(rt);
                continue;
            }
            (*rt).device_present = true;
            (*c).active_sata_ports += 1;
            let index =
                r4os_zig_ahci_register(port as u8, (*state()).source, rt.cast(), name(port as u8));
            if index == usize::MAX {
                (*rt).last_error = Label::new(b"block-register-failed");
                snapshot(rt);
                continue;
            }
            (*rt).block_device_index = index;
            (*state()).registered[port] = index;
            (*c).block_devices += 1;
            snapshot(rt);
            put(b"[AHCI] port ");
            dec(port as u64);
            put(b" block=#");
            dec(index as u64);
            put(b" dma-ready\r\n");
        }
    }
}
pub fn probe() -> bool {
    unsafe {
        let c = current();
        c.write(Status::EMPTY);
        ptr::addr_of_mut!((*state()).runtimes).write([Runtime::EMPTY; MAX_PORTS]);
        (*state()).registered = [usize::MAX; MAX_PORTS];
        let p = pci::snapshot();
        if p.ahci_count == 0 {
            (*c).reason = Label::new(b"no AHCI controller found");
            put(b"[AHCI] not found\r\n");
            return false;
        }
        (*c).present = true;
        (*c).device = p.first_ahci;
        (*c).bar5_raw = pci::read_bar(p.first_ahci, 5);
        let bar = (*c).bar5_raw;
        if bar & 1 != 0 {
            (*c).reason = Label::new(b"BAR5 is I/O space, expected MMIO");
            put(b"[AHCI][WARN] BAR5 is I/O space\r\n");
            return false;
        }
        (*c).mmio_phys = u64::from(bar & 0xfffffff0);
        if (*c).mmio_phys == 0 {
            (*c).reason = Label::new(b"BAR5 MMIO base is zero");
            put(b"[AHCI][WARN] BAR5 MMIO base is zero\r\n");
            return false;
        }
        if !map_mmio((*c).mmio_phys) {
            (*c).reason = Label::new(b"failed to map AHCI MMIO");
            put(b"[AHCI][WARN] MMIO map failed\r\n");
            return false;
        }
        (*c).mapped = true;
        (*c).mmio_virt = phys::r4os_rust_phys_to_virt((*c).mmio_phys);
        let command = pci::read_command((*c).device);
        let _ = pci::write_command((*c).device, command | 6);
        bios_handoff();
        (*c).hba_reset_ok = reset_controller();
        (*c).cap = read(CAP);
        (*c).ghc = read(GHC);
        (*c).is = read(IS);
        (*c).pi = read(PI);
        (*c).vs = read(VS);
        (*c).cap2 = read(CAP2);
        (*c).bohc = read(BOHC);
        (*c).ports_advertised = (((*c).cap & 31) + 1) as u8;
        (*c).command_slots = ((((*c).cap >> 8) & 31) + 1) as u8;
        (*c).supports_staggered_spinup = (*c).cap & (1 << 27) != 0;
        (*c).supports_ncq = (*c).cap & (1 << 30) != 0;
        (*c).supports_64bit = (*c).cap & (1 << 31) != 0;
        (*c).implemented_ports = 0;
        (*c).active_sata_ports = 0;
        for port in 0..32u8 {
            if (*c).pi & (1 << port) == 0 {
                continue;
            }
            (*c).implemented_ports += 1;
            let p = read_port(port);
            if usize::from(port) < MAX_PORTS {
                (*c).first_ports[usize::from(port)] = p
            }
            if active(p) {
                (*c).active_sata_ports += 1
            }
        }
        configure();
        (*c).reason = Label::new(if (*c).block_devices > 0 {
            b"AHCI block devices active"
        } else {
            b"diagnostic MMIO mapped, no active SATA block device"
        });
        let s = c.read();
        put(b"[AHCI] device ");
        dec(u64::from(s.device.bus));
        put(b":");
        dec(u64::from(s.device.device));
        put(b".");
        dec(u64::from(s.device.function));
        put(b" bar5=0x");
        hex(u64::from(s.bar5_raw), 8);
        put(b" mmio=0x");
        hex(s.mmio_phys, 16);
        put(b" cap=0x");
        hex(u64::from(s.cap), 8);
        put(b" pi=0x");
        hex(u64::from(s.pi), 8);
        put(b" ports=");
        dec(u64::from(s.implemented_ports));
        put(b" active_sata=");
        dec(u64::from(s.active_sata_ports));
        put(b" blockdevs=");
        dec(u64::from(s.block_devices));
        put(b"\r\n");
        true
    }
}
#[repr(C)]
struct Header {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    reserved: [u32; 4],
}
#[repr(C)]
struct Prdt {
    dba: u32,
    dbau: u32,
    reserved: u32,
    dbc_i: u32,
}
struct Request {
    command: u8,
    lba: u64,
    sectors: u16,
    write: bool,
    bytes: usize,
}
fn fis(command: u8, lba: u64, sectors: u16) -> [u8; 16] {
    [
        0x27,
        0x80,
        command,
        0,
        lba as u8,
        (lba >> 8) as u8,
        (lba >> 16) as u8,
        0x40,
        (lba >> 24) as u8,
        (lba >> 32) as u8,
        (lba >> 40) as u8,
        0,
        sectors as u8,
        (sectors >> 8) as u8,
        0,
        0,
    ]
}
unsafe fn prepare_command(rt: *mut Runtime, r: Request) {
    unsafe {
        ((*rt).command_list_virt as *mut Header).write(Header {
            flags: 5 | if r.write { 1 << 6 } else { 0 },
            prdtl: u16::from(r.bytes > 0),
            prdbc: 0,
            ctba: (*rt).command_table_phys as u32,
            ctbau: ((*rt).command_table_phys >> 32) as u32,
            reserved: [0; 4],
        });
        let table = (*rt).command_table_virt as *mut u8;
        ptr::write_bytes(table, 0, 256);
        let bytes = fis(r.command, r.lba, r.sectors);
        ptr::copy_nonoverlapping(bytes.as_ptr(), table, bytes.len());
        if r.bytes > 0 {
            table.add(128).cast::<Prdt>().write(Prdt {
                dba: (*rt).dma_phys as u32,
                dbau: ((*rt).dma_phys >> 32) as u32,
                reserved: 0,
                dbc_i: (r.bytes - 1) as u32 | (1 << 31),
            });
        }
    }
}
unsafe fn recover(rt: *mut Runtime, b: u64) -> bool {
    unsafe {
        (*rt).recoveries += 1;
        (*current()).recoveries += 1;
        let _ = stop(b, rt);
        write(b + SERR, u32::MAX);
        write(b + PIS, u32::MAX);
        start(b, rt)
    }
}
unsafe fn fail(rt: *mut Runtime, b: Option<u64>, reason: &'static [u8], recovery: bool) {
    unsafe {
        (*rt).failures += 1;
        (*current()).failures += 1;
        if reason == b"command-timeout" || reason == b"tfd-busy" {
            (*rt).timeouts += 1;
            (*current()).timeouts += 1
        }
        (*rt).last_error = Label::new(reason);
        if recovery && let Some(b) = b {
            let _ = recover(rt, b);
            (*rt).last_error = Label::new(reason);
        }
        snapshot(rt);
    }
}
unsafe fn issue(rt: *mut Runtime, r: Request) -> bool {
    unsafe {
        if !(*rt).configured {
            fail(rt, None, b"port-not-configured", false);
            return false;
        }
        let b = base((*rt).port);
        (*rt).commands += 1;
        (*current()).commands += 1;
        (*rt).last_command = r.command;
        (*rt).last_lba = r.lba;
        (*rt).last_sectors = r.sectors;
        snapshot(rt);
        let mut guard = 0;
        while read(b + TFD) & ((1 << 7) | (1 << 3)) != 0 && guard < WAIT_GUARD {
            guard += 1
        }
        if read(b + TFD) & ((1 << 7) | (1 << 3)) != 0 {
            fail(rt, Some(b), b"tfd-busy", true);
            return false;
        }
        prepare_command(rt, r);
        write(b + SERR, u32::MAX);
        write(b + PIS, u32::MAX);
        write(b + CI, 1);
        guard = 0;
        while read(b + CI) & 1 != 0 && guard < WAIT_GUARD {
            if read(b + PIS) & TFES != 0 {
                fail(rt, Some(b), b"task-file-error", true);
                return false;
            }
            guard += 1
        }
        if read(b + CI) & 1 != 0 {
            fail(rt, Some(b), b"command-timeout", true);
            return false;
        }
        let status = read(b + PIS);
        let tfd = read(b + TFD);
        if status & TFES != 0 || tfd & (1 | (1 << 7) | (1 << 3)) != 0 {
            fail(rt, Some(b), b"command-error", true);
            return false;
        }
        (*rt).last_error = Label::new(b"none");
        snapshot(rt);
        true
    }
}
/// # Safety
/// Exact registered resident runtime context; synchronous caller keeps len
/// bytes alive until physical completion. Original serialized lane retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ahci_io(
    context: *mut c_void,
    operation: u8,
    lba: u64,
    sectors: u16,
    data: *mut u8,
    len: usize,
) -> bool {
    unsafe {
        if context.is_null() {
            return false;
        }
        let rt = context.cast::<Runtime>();
        if operation == 2 {
            return issue(
                rt,
                Request {
                    command: 0xea,
                    lba: 0,
                    sectors: 0,
                    write: false,
                    bytes: 0,
                },
            );
        }
        if sectors == 0 || sectors > MAX_SECTORS {
            fail(rt, None, b"bad-sector-count", false);
            return false;
        }
        let bytes = usize::from(sectors) * 512;
        if len < bytes {
            fail(rt, None, b"buffer-too-small", false);
            return false;
        }
        if operation == 1 {
            ptr::copy_nonoverlapping(data, (*rt).dma_virt as *mut u8, bytes)
        }
        let ok = issue(
            rt,
            Request {
                command: if operation == 1 { 0x35 } else { 0x25 },
                lba,
                sectors,
                write: operation == 1,
                bytes,
            },
        );
        if ok && operation == 0 {
            ptr::copy_nonoverlapping((*rt).dma_virt as *const u8, data, bytes)
        }
        ok
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_ahci_control(operation: u8, index: usize) -> usize {
    unsafe {
        match operation {
            0 => usize::from(probe()),
            1 => usize::from((*current()).block_devices),
            2 => {
                (*state()).source = 1;
                (*state()).external_owner = true;
                0
            }
            3 => {
                (*state()).source = 0;
                (*state()).external_owner = false;
                0
            }
            4 => usize::from((*state()).external_owner),
            5 => {
                if index < MAX_PORTS {
                    (*state()).registered[index]
                } else {
                    usize::MAX
                }
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Caller owns initialized-size writable C Status storage for this snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_ahci_status(out: *mut Status) {
    unsafe {
        out.write(current().read());
    }
}
const _: () = {
    assert!(core::mem::size_of::<Header>() == 32);
    assert!(core::mem::size_of::<Prdt>() == 16);
    assert!(core::mem::size_of::<PortStatus>() == 120);
    assert!(core::mem::offset_of!(PortStatus, last_error) == 104);
    assert!(core::mem::size_of::<Status>() == 1104);
    assert!(core::mem::offset_of!(Status, first_ports) == 128);
};
