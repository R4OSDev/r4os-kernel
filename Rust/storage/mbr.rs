//! Original legacy MBR/GPT mount scanner, error priority and marker policy.
//! Pure GPT decoding remains its existing owner. FS/drive calls receive only
//! fixed C metadata and a synchronous exact opaque volume lease.
use crate::storage::{
    block_io, gpt, media_inventory::DeviceInfo, normal_boot::DriveRequest, partition_table::Label,
};
use core::{ffi::c_void, mem::MaybeUninit, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Report {
    pub table: u8,
    pub result: u8,
    pub detail: Label,
    pub table_valid: bool,
    pub partition_count: u16,
    pub mountable_count: u16,
    pub filesystem_count: u16,
    pub best_system_score: u8,
    pub marker_found_mask: u8,
    pub marker_io_mask: u8,
    pub mbr_type: u8,
}
impl Default for Report {
    fn default() -> Self {
        Self {
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
        }
    }
}
#[derive(Clone, Copy)]
struct Partition {
    bootable: bool,
    type_id: u8,
    first: u64,
    sectors: u64,
}
#[repr(C)]
struct Probe {
    index: usize,
    lba: u32,
    sectors: u64,
    kind: u8,
}
struct Mount {
    index: usize,
    first: u64,
    sectors: u64,
    boot: bool,
    hint: u8,
    name: Label,
    report: *mut Report,
}

fn put(v: &[u8]) {
    unsafe {
        normal_log(v.as_ptr(), v.len());
    }
}
fn raw(v: Label) {
    unsafe {
        normal_log(v.data, v.len);
    }
}
fn dec(v: u64) {
    {
        crate::boot::log_route::dec(v);
    }
}
fn hex(v: u64) {
    {
        crate::boot::log_route::hex(v, 2);
    }
}
fn gpt_label(partition: bool, value: u8) -> Label {
    let v = gpt::r4os_rust_gpt_label(partition, value);
    Label {
        data: v.data,
        len: v.len,
    }
}
fn priority(value: u8) -> u8 {
    match value {
        0 | 1 | 2 | 3 | 4 | 8 => 0,
        5 => 1,
        6 => 10,
        7 => 20,
        16 => 30,
        13 => 40,
        11 => 50,
        14 | 15 => 60,
        10 | 18 => 70,
        9 => 75,
        17 => 80,
        12 => 85,
        19 => 90,
        20 => 100,
        21 => 255,
        _ => unreachable!(),
    }
}
unsafe fn note(report: *mut Report, result: u8, detail: Label) {
    unsafe {
        if priority(result) >= priority((*report).result) {
            (*report).result = result;
            (*report).detail = detail;
        }
    }
}
fn increment(value: u16) -> u16 {
    value.saturating_add(1)
}
unsafe fn read(index: usize, lba: u64, out: *mut u8) -> bool {
    unsafe { block_io::r4os_rust_block_read(index, lba, 1, out, 512, false) }
}
fn parse(raw: &[u8]) -> Partition {
    Partition {
        bootable: raw[0] == 0x80,
        type_id: raw[4],
        first: u64::from(gpt::le32(&raw[8..12])),
        sectors: u64::from(gpt::le32(&raw[12..16])),
    }
}
fn type_name(id: u8) -> &'static [u8] {
    match id {
        6 => b"FAT16",
        14 => b"FAT16-LBA",
        11 => b"FAT32",
        12 => b"FAT32-LBA",
        7 => b"NTFS/exFAT",
        238 => b"GPT-protective",
        _ => b"unknown",
    }
}
fn drive(operation: u8, letter: u8) -> bool {
    let input = DriveRequest {
        letter,
        kind: 0,
        role: 0,
        bytes: 0,
        index: 0,
        name: Label::new(b""),
        volume: ptr::null_mut(),
    };
    unsafe { crate::storage::normal_boot::drive_dispatch(operation, &input) }
}
/// # Safety
/// Original early boot scan, before user mount admission. Output owns one
/// initialized C report; only exact local snapshots cross backend/FS calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_mbr_scan(index: usize, out: *mut Report) {
    unsafe {
        out.write(Report::default());
    }
    let mut info = MaybeUninit::uninit();
    if !unsafe { media_device(index, info.as_mut_ptr()) } {
        put(b"  Partition scan: block device missing\r\n");
        return;
    }
    let info = unsafe { info.assume_init() };
    if info.sector_bytes != 512 {
        put(b"  Partition scan: unsupported logical sector size=");
        dec(u64::from(info.sector_bytes));
        put(b"\r\n");
        unsafe {
            (*out).result = 1;
        }
        return;
    }
    if info.sectors == 0 {
        put(b"  Partition scan: device capacity unavailable\r\n");
        unsafe {
            (*out).result = 2;
        }
        return;
    }
    let mut sector = MaybeUninit::<[u8; 512]>::uninit();
    if !unsafe { read(index, 0, sector.as_mut_ptr().cast()) } {
        put(b"  MBR: read failed\r\n");
        unsafe {
            (*out).result = 3;
        }
        return;
    }
    let sector = unsafe { sector.assume_init() };
    if sector[510] != 0x55 || sector[511] != 0xaa {
        put(b"  MBR: invalid signature\r\n");
        unsafe {
            (*out).result = 4;
        }
        return;
    }
    put(b"  MBR [OK]\r\n");
    for i in 0..4 {
        let p = parse(&sector[446 + i * 16..446 + (i + 1) * 16]);
        if p.type_id == 238 {
            put(b"    partition ");
            dec(i as u64 + 1);
            put(b": type=0xEE GPT-protective\r\n");
            unsafe {
                scan_gpt(index, info.sectors, out);
            }
            return;
        }
    }
    unsafe {
        (*out).table = 1;
        (*out).table_valid = true;
        (*out).result = 5;
    }
    for i in 0..4 {
        let p = parse(&sector[446 + i * 16..446 + (i + 1) * 16]);
        if p.type_id == 0 || p.sectors == 0 {
            continue;
        }
        unsafe {
            (*out).partition_count = increment((*out).partition_count);
            (*out).mbr_type = p.type_id;
        }
        put(b"    partition ");
        dec(i as u64 + 1);
        put(b": type=0x");
        hex(u64::from(p.type_id));
        put(b" ");
        put(type_name(p.type_id));
        put(b" lba=");
        dec(p.first);
        put(b" sectors=");
        dec(p.sectors);
        put(b"\r\n");
        let hint = match p.type_id {
            11 | 12 => 0,
            7 => 1,
            _ => {
                unsafe {
                    note(out, 6, Label::new(b"mbr-type"));
                }
                continue;
            }
        };
        unsafe {
            (*out).mountable_count = increment((*out).mountable_count);
            mount(Mount {
                index,
                first: p.first,
                sectors: p.sectors,
                boot: p.bootable,
                hint,
                name: Label::new(type_name(p.type_id)),
                report: out,
            });
        }
    }
}
unsafe fn load_header(
    index: usize,
    lba: u64,
    capacity: u64,
    sector: *mut [u8; 512],
    report: *mut Report,
) -> Option<gpt::Header> {
    if !unsafe { read(index, lba, sector.cast()) } {
        put(b"  GPT header: read failed lba=");
        dec(lba);
        put(b"\r\n");
        unsafe {
            (*report).detail = Label::new(b"header-read");
        }
        return None;
    }
    let header = match gpt::parse_header(unsafe { &*sector }, lba, capacity) {
        Ok(v) => v,
        Err(error) => {
            put(b"  GPT header: invalid lba=");
            dec(lba);
            put(b" reason=");
            raw(gpt_label(false, error as u8));
            put(b"\r\n");
            unsafe {
                (*report).detail = gpt_label(false, error as u8);
            }
            return None;
        }
    };
    let mut crc = gpt::Crc32::default();
    let mut remaining = header.entry_bytes();
    let mut block = header.entries_lba;
    while remaining > 0 {
        if !unsafe { read(index, block, sector.cast()) } {
            put(b"  GPT entries: read failed lba=");
            dec(block);
            put(b"\r\n");
            unsafe {
                (*report).detail = Label::new(b"entry-array-read");
            }
            return None;
        }
        let count = remaining.min(512) as usize;
        crc.update(unsafe { core::slice::from_raw_parts(sector.cast::<u8>(), count) });
        remaining -= count as u64;
        block += 1;
    }
    if crc.finish() != header.entries_crc32 {
        put(b"  GPT entries: CRC mismatch\r\n");
        unsafe {
            (*report).detail = Label::new(b"entry-array-crc");
        }
        return None;
    }
    Some(header)
}
unsafe fn scan_gpt(index: usize, capacity: u64, report: *mut Report) {
    put(b"  GPT protective MBR detected\r\n");
    unsafe {
        (*report).result = 8;
        (*report).detail = Label::new(b"no-valid-header");
    }
    // Same stable DMA destination for header, CRC and entry walking.
    let mut sector = MaybeUninit::<[u8; 512]>::uninit();
    let sector = sector.as_mut_ptr();
    let mut backup = false;
    let mut header = unsafe { load_header(index, 1, capacity, sector, report) };
    if header.is_none() && capacity > 1 {
        backup = true;
        put(b"  GPT: primary invalid; trying backup header\r\n");
        header = unsafe { load_header(index, capacity - 1, capacity, sector, report) };
    }
    let Some(header) = header else {
        put(b"  GPT: no valid partition table\r\n");
        return;
    };
    unsafe {
        (*report).table = if backup { 3 } else { 2 };
        (*report).table_valid = true;
        (*report).result = 5;
        (*report).detail = if backup {
            Label::new(b"primary-fallback")
        } else {
            Label::new(b"")
        };
    }
    put(b"  GPT [OK] header=");
    put(if backup { b"backup" } else { b"primary" });
    put(b" entries=");
    dec(u64::from(header.entry_count));
    put(b" entry-size=");
    dec(u64::from(header.entry_size));
    put(b"\r\n");
    let mut cache_lba = None;
    let mut raw_entry = [0u8; 512];
    for entry in 0..header.entry_count {
        let mut source = u64::from(entry) * u64::from(header.entry_size);
        let mut destination = 0;
        let mut remaining = header.entry_size as usize;
        while remaining > 0 {
            let lba = header.entries_lba + source / 512;
            let offset = (source % 512) as usize;
            if cache_lba != Some(lba) {
                if !unsafe { read(index, lba, sector.cast()) } {
                    put(b"    GPT entry read failed index=");
                    dec(u64::from(entry) + 1);
                    put(b"\r\n");
                    unsafe {
                        note(report, 9, Label::new(b"entry-read"));
                    }
                    return;
                }
                cache_lba = Some(lba);
            }
            let count = remaining.min(512 - offset);
            unsafe {
                ptr::copy_nonoverlapping(
                    sector.cast::<u8>().add(offset),
                    raw_entry.as_mut_ptr().add(destination),
                    count,
                );
            }
            source += count as u64;
            destination += count;
            remaining -= count;
        }
        let partition = match gpt::parse_partition(&raw_entry[..header.entry_size as usize], header)
        {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(error) => {
                put(b"    GPT partition ");
                dec(u64::from(entry) + 1);
                put(b": skipped reason=");
                raw(gpt_label(false, error as u8));
                put(b"\r\n");
                unsafe {
                    note(report, 7, gpt_label(false, error as u8));
                }
                continue;
            }
        };
        unsafe {
            (*report).partition_count = increment((*report).partition_count);
        }
        put(b"    GPT partition ");
        dec(u64::from(entry) + 1);
        put(b": type=");
        raw(gpt_label(true, partition.partition_type));
        put(b" lba=");
        dec(partition.first_lba);
        put(b" sectors=");
        dec(partition.sector_count());
        put(b"\r\n");
        let (hint, name) = match partition.partition_type {
            0 => (0, b"GPT EFI-system".as_slice()),
            1 => (2, b"GPT basic-data".as_slice()),
            2 => {
                put(b"      unsupported GPT partition type; not mounted\r\n");
                unsafe {
                    note(report, 6, Label::new(b"gpt-guid"));
                }
                continue;
            }
            _ => unreachable!(),
        };
        unsafe {
            (*report).mountable_count = increment((*report).mountable_count);
            mount(Mount {
                index,
                first: partition.first_lba,
                sectors: partition.sector_count(),
                boot: partition.boot_candidate(),
                hint,
                name: Label::new(name),
                report,
            });
        }
    }
}
unsafe fn signature(index: usize, lba: u64, report: *mut Report) -> u8 {
    let mut sector = MaybeUninit::<[u8; 512]>::uninit();
    if !unsafe { read(index, lba, sector.as_mut_ptr().cast()) } {
        put(b"      filesystem probe: boot sector read failed\r\n");
        unsafe {
            note(report, 11, Label::new(b"boot-sector-read"));
        }
        return 2;
    }
    let sector = unsafe { sector.assume_init() };
    if &sector[3..11] == b"NTFS    " {
        return 1;
    }
    if sector[510] == 0x55 && sector[511] == 0xaa {
        return 0;
    }
    unsafe {
        note(report, 13, Label::new(b"boot-signature"));
    }
    2
}
unsafe fn mount(mut context: Mount) {
    let report = context.report;
    let mut info = MaybeUninit::uninit();
    if !unsafe { media_device(context.index, info.as_mut_ptr()) } {
        unsafe {
            note(report, 0, Label::new(b"partition-device"));
        }
        return;
    }
    let info = unsafe { info.assume_init() };
    if info.sector_bytes != 512
        || context.first == 0
        || context.sectors == 0
        || context.first >= info.sectors
        || context.sectors > info.sectors - context.first
    {
        unsafe {
            note(report, 7, Label::new(b"partition-range"));
        }
        return;
    }
    if context.first > u64::from(u32::MAX) {
        put(b"      partition starts beyond current filesystem LBA limit; not mounted\r\n");
        unsafe {
            note(report, 10, Label::new(b"first-lba-u32"));
        }
        return;
    }
    if context.sectors > u64::MAX / 512 {
        put(b"      partition byte size overflow; not mounted\r\n");
        unsafe {
            note(report, 18, Label::new(b"byte-overflow"));
        }
        return;
    }
    if context.sectors * 512 > usize::MAX as u64 {
        put(b"      partition exceeds addressable drive size; not mounted\r\n");
        unsafe {
            note(report, 18, Label::new(b"address-space"));
        }
        return;
    }
    context.hint = if context.hint == 2 {
        unsafe { signature(context.index, context.first, report) }
    } else {
        context.hint
    };
    if context.hint == 2 {
        put(b"      unsupported filesystem signature; not mounted\r\n");
        return;
    }
    let input = Probe {
        index: context.index,
        lba: context.first as u32,
        sectors: context.sectors,
        kind: if context.hint == 0 { 2 } else { 3 },
    };
    if !unsafe { mbr_probe(&input, ptr::addr_of_mut!(context).cast(), mounted) } {
        if context.hint == 0 {
            put(b"      FAT32: not mounted (invalid BPB or unsupported layout)\r\n");
            unsafe {
                note(report, 14, Label::new(b"fat32-inspect"));
            }
        } else {
            put(b"      NTFS: not mounted (invalid boot sector or unsupported layout)\r\n");
            unsafe {
                note(report, 15, Label::new(b"ntfs-inspect"));
            }
        }
    }
}
unsafe fn exists(volume: *mut c_void, path: &[u8]) -> bool {
    let mut attr = 0;
    unsafe { mbr_lookup(volume, path.as_ptr(), path.len(), &mut attr) == 0 }
}
unsafe extern "C" fn mounted(raw_context: *mut c_void, volume: *mut c_void) {
    let c = unsafe { &*raw_context.cast::<Mount>() };
    let report = c.report;
    unsafe {
        (*report).filesystem_count = increment((*report).filesystem_count);
    }
    let mut score = 0u8;
    let mut found = 0u8;
    let mut io = 0u8;
    for (i, (path, directory)) in [
        (b"/BOOT/R4OS.ELF".as_slice(), false),
        (b"/BOOT/LIMINE.CONF".as_slice(), false),
        (b"/CONFIG.R4S".as_slice(), false),
        (b"/R4OS".as_slice(), true),
        (b"/R4OS/CONFIG".as_slice(), true),
        (b"/R4OS/SOFTWARE".as_slice(), true),
    ]
    .iter()
    .enumerate()
    {
        let mut attr = 0;
        match unsafe { mbr_lookup(volume, path.as_ptr(), path.len(), &mut attr) } {
            0 => {
                if *directory && attr & 0x10 == 0 {
                    continue;
                }
                score += 1;
                found |= 1u8 << i;
            }
            1 => {}
            2 => io |= 1u8 << i,
            _ => unreachable!(),
        }
    }
    unsafe {
        (*report).best_system_score = (*report).best_system_score.max(score);
        (*report).marker_found_mask |= found;
        (*report).marker_io_mask |= io;
        mbr_list(volume);
    }
    put(b"      R4OS system markers: ");
    dec(u64::from(score));
    put(b"/6 found=0x");
    hex(u64::from(found));
    put(b" io=0x");
    hex(u64::from(io));
    put(b" ");
    put(if score >= 3 {
        b"system-candidate"
    } else {
        b"data-volume"
    });
    put(b"\r\n");
    if io != 0 {
        put(b"      system-marker lookup failed; not mounted\r\n");
        unsafe {
            note(report, 12, Label::new(b"system-marker-io"));
        }
        return;
    }
    let input = DriveRequest {
        letter: 0,
        kind: 0,
        role: 0,
        bytes: 0,
        index: 0,
        name: Label::new(b""),
        volume,
    };
    if c.boot
        && score < 3
        && c.hint == 0
        && (unsafe { exists(volume, b"/BOOT/LIMINE.CONF") }
            || unsafe { exists(volume, b"/LIMINE-BIOS.SYS") })
    {
        if !unsafe { crate::storage::normal_boot::drive_dispatch(5, &input) } {
            unsafe {
                note(report, 20, Label::new(b"boot-mount-generation"));
            }
            return;
        }
        put(b"      boot partition (Limine): internal mount, unlettered by design\r\n");
        unsafe {
            note(report, 16, Label::new(b"limine-volume"));
        }
        return;
    }
    let letter = if score >= 3 && !drive(0, b'C') {
        Some(b'C')
    } else {
        (b'D'..=b'Z')
            .find(|&letter| !drive(0, letter))
            .or_else(|| if !drive(0, b'C') { Some(b'C') } else { None })
    };
    let Some(letter) = letter else {
        put(b"      no drive letter available; not mounted\r\n");
        unsafe {
            note(report, 19, Label::new(b"letters-a-z"));
        }
        return;
    };
    let role = if score >= 3 { 1 } else { 2 };
    let input = DriveRequest {
        letter,
        kind: if c.hint == 0 { 2 } else { 3 },
        role,
        bytes: (c.sectors * 512) as usize,
        index: c.index,
        name: c.name,
        volume,
    };
    if unsafe { crate::storage::normal_boot::drive_dispatch(1, &input) } {
        if letter == b'C' {
            drive(4, b'C');
        }
        if !unsafe { crate::storage::normal_boot::drive_dispatch(2, &input) } {
            unsafe {
                crate::storage::normal_boot::drive_dispatch(3, &input);
                note(report, 20, Label::new(b"mount-generation"));
            }
            return;
        }
        put(b"    mounted as ");
        put(&[letter]);
        put(b":\\ role=");
        put(if role == 1 { b"system" } else { b"data" });
        put(b"\r\n");
        unsafe {
            note(
                report,
                if score >= 3 { 21 } else { 17 },
                if score >= 3 {
                    Label::new(b"system-markers")
                } else {
                    Label::new(b"insufficient-markers")
                },
            );
        }
    } else {
        put(b"      drive registry rejected mount\r\n");
        unsafe {
            note(report, 20, Label::new(b"drive-registry"));
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<Report>() == 40);
    assert!(core::mem::size_of::<Probe>() == 32);
};

unsafe fn normal_log(text: *const u8, len: usize) {
    unsafe { crate::boot::log_route::r4os_rust_log_write(0, text, len) }
}

unsafe fn media_device(index: usize, out: *mut DeviceInfo) -> bool {
    let _guard = crate::memory::owner::Guard::acquire(1);
    let mut view = MaybeUninit::uninit();
    if !unsafe {
        crate::storage::block_registry::r4os_rust_block_registry_slot(
            index,
            1,
            0,
            true,
            view.as_mut_ptr(),
        )
    } {
        return false;
    }
    let view = unsafe { view.assume_init() };
    let c = unsafe {
        core::ptr::addr_of!((*view.device.cast::<super::block_device::Device>()).config).read()
    };
    unsafe {
        out.write(DeviceInfo {
            index,
            bus: c.bus,
            name: Label {
                data: c.name.data,
                len: c.name.len,
            },
            model: Label {
                data: c.model.data,
                len: c.model.len,
            },
            driver: Label {
                data: c.driver.data,
                len: c.driver.len,
            },
            sector_bytes: c.sector_size,
            sectors: c.sector_count,
            writable: c.writable,
        });
    }
    true
}

unsafe fn mbr_probe(
    input: *const Probe,
    context: *mut c_void,
    consumer: unsafe extern "C" fn(*mut c_void, *mut c_void),
) -> bool {
    unsafe {
        let a = &*input;
        assert!(matches!(a.kind, 2 | 3));
        let Some(v) = crate::fs::vfs_backends::probe(a.kind == 3, a.index, a.lba, a.sectors, true)
        else {
            return false;
        };
        let mut v = Some(v);
        consumer(context, ptr::from_mut(&mut v).cast());
        true
    }
}

unsafe fn mbr_lookup(volume: *mut c_void, path: *const u8, len: usize, attr: *mut u8) -> u8 {
    unsafe {
        let a = crate::fs::vfs_types::Args {
            volume: crate::storage::media_inventory::volume(volume),
            name: crate::fs::vfs_types::Bytes { data: path, len },
            ..Default::default()
        };
        let mut e = MaybeUninit::<crate::fs::vfs_types::Entry>::uninit();
        let r = crate::fs::vfs::r4os_rust_vfs_run(1, &a, e.as_mut_ptr().cast());
        if r.status == 0 {
            attr.write(e.assume_init().attr);
        }
        r.status.try_into().unwrap()
    }
}

unsafe fn mbr_list(volume: *mut c_void) {
    unsafe {
        let a = crate::fs::vfs_types::Args {
            volume: crate::storage::media_inventory::volume(volume),
            ..Default::default()
        };
        crate::fs::vfs::r4os_rust_vfs_run(29, &a, ptr::null_mut());
    }
}
