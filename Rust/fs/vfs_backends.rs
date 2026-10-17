//! Native projections to the original FAT/NTFS owners. Each backend keeps a
//! separate non-inlined frame and the original request/mount/I/O lifetime.
use super::{
    fat, fat_types as f, ntfs_adapter, ntfs_adapter_types as n, ntfs_format_types as wire,
    vfs_types::{Args, Entry, Result as Output, Volume},
};
use core::{mem::MaybeUninit, ptr};
fn fat_volume(v: Volume) -> f::Volume {
    f::Volume {
        has_ref: v.has_ref,
        reference: v.reference,
        device_index: v.fat.device,
        partition_lba: v.fat.partition,
        bytes_per_sector: v.fat.sector_bytes,
        sectors_per_cluster: v.fat.cluster_sectors,
        reserved_sectors: v.fat.reserved,
        fat_count: v.fat.fats,
        sectors_per_fat: v.fat.fat_sectors,
        total_sectors: v.fat.sectors,
        root_cluster: v.fat.root,
        fs_info_sector: v.fat.fsinfo,
        backup_boot_sector: v.fat.backup,
        write_batch: v.fat.write_batch,
    }
}
fn fat_entry(e: Entry) -> f::Entry {
    f::Entry {
        name: e.name,
        name_len: e.name_len,
        attr: e.attr,
        first_cluster: u32::try_from(e.node).expect("FAT node"),
        has_directory_lba: e.has_lba,
        directory_lba: e.directory_lba,
        directory_offset: e.directory_offset,
        size: u32::try_from(e.size).expect("FAT size"),
        created_time: e.times[0],
        created_date: e.times[1],
        access_date: e.times[2],
        modified_time: e.times[3],
        modified_date: e.times[4],
    }
}
fn from_fat(e: f::Entry) -> Entry {
    Entry {
        name: e.name,
        name_len: e.name_len,
        attr: e.attr,
        node: u64::from(e.first_cluster),
        generation: 0,
        directory_lba: e.directory_lba,
        has_lba: e.has_directory_lba,
        directory_offset: e.directory_offset,
        reparse: false,
        size: u64::from(e.size),
        times: [
            e.created_time,
            e.created_date,
            e.access_date,
            e.modified_time,
            e.modified_date,
        ],
    }
}
fn ntfs_entry(e: Entry) -> n::Entry {
    n::Entry {
        name: e.name,
        name_len: e.name_len,
        attr: e.attr,
        reparse: e.reparse,
        record: e.node,
        sequence: e.generation,
        size: e.size,
        created_time: e.times[0],
        created_date: e.times[1],
        access_date: e.times[2],
        modified_time: e.times[3],
        modified_date: e.times[4],
    }
}
fn from_ntfs(e: n::Entry) -> Entry {
    Entry {
        name: e.name,
        name_len: e.name_len,
        attr: e.attr,
        node: e.record,
        generation: e.sequence,
        reparse: e.reparse,
        size: e.size,
        times: [
            e.created_time,
            e.created_date,
            e.access_date,
            e.modified_time,
            e.modified_date,
        ],
        ..Entry::EMPTY
    }
}
fn bytes(v: super::vfs_types::Bytes) -> f::Bytes {
    f::Bytes {
        data: v.data,
        len: v.len,
    }
}
fn view(v: super::vfs_types::Bytes) -> wire::View {
    wire::View {
        data: v.data,
        len: v.len,
    }
}
fn mounted(v: Volume) -> n::Mounted {
    n::Mounted {
        state_slot: v.ntfs.slot,
        cluster_bytes: v.ntfs.cluster_bytes,
        total_sectors: v.ntfs.sectors,
    }
}
/// Exact initialized arguments, request admission, caller buffers and cursor
/// remain live until real backend completion; only successful lookups write out.
#[inline(never)]
pub(crate) unsafe fn fat(op: u8, args: *const Args, out: *mut Entry) -> Output {
    let a = unsafe { args.read() };
    let entry = if a.entry.is_null() {
        None
    } else {
        Some(fat_entry(unsafe { a.entry.read() }))
    };
    let peer = if a.peer.is_null() {
        None
    } else {
        Some(fat_entry(unsafe { a.peer.read() }))
    };
    let parent = if matches!(op,2|8..=13|15..=17|20..=23|25..=27|34) {
        u32::try_from(a.parent).expect("FAT parent")
    } else {
        0
    };
    let offset = if matches!(op, 10 | 11) {
        u32::try_from(a.offset).expect("FAT append offset") as usize
    } else if matches!(op, 6 | 7) {
        usize::try_from(a.offset).expect("FAT offset")
    } else {
        0
    };
    let native = f::Args {
        volume: fat_volume(a.volume),
        other: fat_volume(a.other),
        entry: entry.as_ref().map_or(ptr::null(), ptr::from_ref),
        peer: peer.as_ref().map_or(ptr::null(), ptr::from_ref),
        parent,
        offset,
        index: a.index,
        name: bytes(a.name),
        second: bytes(a.second),
        third: bytes(a.third),
        data: bytes(a.data),
        buffer: f::Buffer {
            data: a.buffer,
            len: a.buffer_len,
        },
        cursor: a.cursor.cast(),
        flag: if op == 13 {
            true
        } else if op == 34 {
            false
        } else {
            a.flag
        },
        ..Default::default()
    };
    let _payload = if matches!(op, 5 | 6) {
        unsafe { super::request::PayloadBorrow::begin(a.read_guard, a.volume.reference) }
    } else {
        None
    };
    let mut e = MaybeUninit::<f::Entry>::uninit();
    let mut result = unsafe {
        fat::r4os_rust_fat_run(
            if op == 34 { 13 } else { op },
            &native,
            e.as_mut_ptr(),
            ptr::null_mut(),
        )
    };
    if matches!(op, 1 | 2 | 26 | 27) && result.status == 0 {
        unsafe { out.write(from_fat(e.assume_init())) }
    }
    if op == 0 {
        result.present = false;
    }
    result
}
/// Same original NTFS operation/status projection; no backend state borrowed
/// across I/O. Whole/read-range and partial-write progress stay distinct.
#[inline(never)]
pub(crate) unsafe fn ntfs(op: u8, args: *const Args, out: *mut Entry) -> Output {
    let a = unsafe { args.read() };
    let entry = if a.entry.is_null() {
        None
    } else {
        Some(ntfs_entry(unsafe { a.entry.read() }))
    };
    let mut native = n::Args {
        volume: mounted(a.volume),
        entry: entry.as_ref().map_or(ptr::null(), ptr::from_ref),
        buffer: wire::Buffer {
            data: a.buffer,
            len: a.buffer_len,
        },
        name: view(a.name),
        other: view(a.second),
        third: view(a.third),
        number: a.parent,
        second: a.offset,
        count: 0,
        device: 0,
        state: ptr::null_mut(),
        flag: a.flag,
        read_guard: a.read_guard,
        read_reference: a.volume.reference,
    };
    let which = match op {
        0 => 7,
        1 => 8,
        2 => 9,
        3 => 10,
        4 => 22,
        5 => {
            native.number = 0;
            11
        }
        6 => {
            native.number = a.offset;
            12
        }
        7 => {
            native.number = a.offset;
            native.name = view(a.data);
            37
        }
        8 => {
            native.other = view(a.data);
            17
        }
        10 => {
            native.other = view(a.data);
            19
        }
        11 => {
            native.other = view(a.data);
            20
        }
        12 | 21 => 30,
        15 => 28,
        16 => 24,
        17 => 25,
        18 => 26,
        19 => 27,
        20 => 29,
        22 => 33,
        23 => 35,
        24 => 23,
        25 => 13,
        26 => {
            native.second = a.index as u64;
            14
        }
        27 => {
            native.state = a.cursor.cast();
            15
        }
        28 => 32,
        29 => 16,
        32 => 36,
        _ => panic!("invalid native NTFS projection"),
    };
    if op == 7 {
        let mut progress = MaybeUninit::<super::ntfs_volume_types::WriteProgress>::uninit();
        let _ = unsafe {
            ntfs_adapter::r4os_rust_ntfs_adapter(which, &native, progress.as_mut_ptr().cast())
        };
        let p = unsafe { progress.assume_init() };
        return if p.status == super::ntfs_volume_types::WriteStatus::Ok
            || (p.completed != 0 && p.completed < a.data.len)
        {
            Output {
                value: p.completed as u64,
                present: true,
                status: 0,
            }
        } else {
            Output::default()
        };
    }
    let mut e = MaybeUninit::<n::Entry>::uninit();
    let mut r =
        unsafe { ntfs_adapter::r4os_rust_ntfs_adapter(which, &native, e.as_mut_ptr().cast()) };
    if op == 27 {
        r.status = i32::try_from(r.value).expect("directory next status");
        r.value = 0;
    }
    if matches!(op, 1 | 2 | 3 | 26 | 27) && r.status == 0 {
        unsafe { out.write(from_ntfs(e.assume_init())) }
    }
    if matches!(op, 8 | 15 | 16 | 20) {
        return Output {
            present: r.status == 0,
            ..Default::default()
        };
    }
    if op == 4 {
        r.present = true
    }
    if op == 0 {
        r.present = false
    }
    if op == 32 {
        r.value = if r.status == 0 { r.value } else { 0 };
        r.status = 0;
        r.present = true;
    }
    r
}
pub(crate) unsafe fn location(volume: *const Volume, out: *mut super::vfs_types::Location) -> bool {
    let v = unsafe { volume.read() };
    let args = n::Args {
        volume: mounted(v),
        entry: ptr::null(),
        buffer: wire::Buffer {
            data: ptr::null_mut(),
            len: 0,
        },
        name: wire::View {
            data: ptr::null(),
            len: 0,
        },
        other: wire::View {
            data: ptr::null(),
            len: 0,
        },
        third: wire::View {
            data: ptr::null(),
            len: 0,
        },
        number: 0,
        second: 0,
        count: 0,
        device: 0,
        state: ptr::null_mut(),
        flag: false,
        read_guard: ptr::null_mut(),
        read_reference: Default::default(),
    };
    unsafe { ntfs_adapter::r4os_rust_ntfs_adapter(2, &args, out.cast()).present }
}

/// Original bounded FAT/NTFS admission; the mount owner publishes any result.
pub(crate) unsafe fn probe(
    is_ntfs: bool,
    index: usize,
    lba: u32,
    sectors: u64,
    print_fat: bool,
) -> Option<Volume> {
    unsafe {
        if is_ntfs {
            let a = n::Args {
                device: index,
                number: u64::from(lba),
                count: sectors,
                ..Default::default()
            };
            let mut m = MaybeUninit::<n::Mounted>::uninit();
            if !ntfs_adapter::r4os_rust_ntfs_adapter(1, &a, m.as_mut_ptr().cast()).present {
                return None;
            }
            let v = m.assume_init();
            Some(Volume {
                kind: 1,
                ntfs: super::vfs_types::Ntfs {
                    slot: v.state_slot,
                    cluster_bytes: v.cluster_bytes,
                    sectors: v.total_sectors,
                },
                ..Default::default()
            })
        } else {
            let a = f::Args {
                volume: f::Volume {
                    device_index: index,
                    partition_lba: lba,
                    ..Default::default()
                },
                count: sectors,
                flag: print_fat,
                ..Default::default()
            };
            let mut v = MaybeUninit::<f::Volume>::uninit();
            if !fat::r4os_rust_fat_run(36, &a, ptr::null_mut(), v.as_mut_ptr()).present {
                return None;
            }
            let v = v.assume_init();
            Some(Volume {
                kind: 0,
                has_ref: v.has_ref,
                reference: v.reference,
                fat: super::vfs_types::Fat {
                    device: v.device_index,
                    partition: v.partition_lba,
                    sector_bytes: v.bytes_per_sector,
                    cluster_sectors: v.sectors_per_cluster,
                    reserved: v.reserved_sectors,
                    fats: v.fat_count,
                    fat_sectors: v.sectors_per_fat,
                    sectors: v.total_sectors,
                    root: v.root_cluster,
                    fsinfo: v.fs_info_sector,
                    backup: v.backup_boot_sector,
                    write_batch: v.write_batch,
                },
                ..Default::default()
            })
        }
    }
}
pub(crate) unsafe fn forget(is_ntfs: bool, index: usize, first: u64, count: u64) {
    unsafe {
        if is_ntfs {
            ntfs_adapter::r4os_rust_ntfs_adapter(
                3,
                &n::Args {
                    device: index,
                    number: first,
                    count,
                    ..Default::default()
                },
                ptr::null_mut(),
            );
        } else {
            fat::r4os_rust_fat_run(
                42,
                &f::Args {
                    volume: f::Volume {
                        device_index: index,
                        ..Default::default()
                    },
                    first,
                    count,
                    ..Default::default()
                },
                ptr::null_mut(),
                ptr::null_mut(),
            );
        }
    }
}
