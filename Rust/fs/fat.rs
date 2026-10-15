//! Complete native FAT owner entry. Every I/O operation has its own bounded
//! non-inlined frame. The Zig facade only projects original public values.
use super::{
    fat_directory as d, fat_files as f, fat_mutation as m, fat_names as n, fat_state as s,
    fat_types::*, vfs_types::Result as Output,
};
use core::{mem::MaybeUninit, sync::atomic::Ordering::Relaxed};
fn optional(value: Option<usize>) -> Output {
    value.map_or(Output::default(), |v| Output {
        value: v as u64,
        status: 0,
        present: true,
    })
}
unsafe fn location(v: Volume, name: Bytes, parent: u32) -> Option<Entry> {
    unsafe {
        let mut e = MaybeUninit::uninit();
        if d::find(v, parent, name, e.as_mut_ptr()) == Lookup::Found {
            Some(e.assume_init())
        } else {
            None
        }
    }
}
fn print_info(v: Volume) {
    s::log(b"      FAT32 BPB: bps=");
    s::decimal(u64::from(v.bytes_per_sector));
    s::log(b" spc=");
    s::decimal(u64::from(v.sectors_per_cluster));
    s::log(b" reserved=");
    s::decimal(u64::from(v.reserved_sectors));
    s::log(b" fats=");
    s::decimal(u64::from(v.fat_count));
    s::log(b"\r\n      FAT32 layout: fat_sectors=");
    s::decimal(u64::from(v.sectors_per_fat));
    s::log(b" root_cluster=");
    s::decimal(u64::from(v.root_cluster));
    s::log(b" fsinfo=");
    s::decimal(u64::from(v.fs_info_sector));
    s::log(b" backup=");
    s::decimal(u64::from(v.backup_boot_sector));
    s::log(b" data_lba=");
    s::decimal(u64::from(v.first_data()));
    s::log(b"\r\n");
}
/// # Safety
/// Exact initialized resident C arguments and disjoint aligned outputs.
/// Existing filesystem request/mutation and mount lifetimes span all waits.
/// Caller buffers stay owned until actual completion; no global Rust borrow
/// survives backend I/O, scheduler cooperation, or a diagnostic leaf.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fat_run(
    op: u8,
    args: *const Args,
    entry: *mut Entry,
    volume: *mut Volume,
) -> Output {
    unsafe {
        let a = args.read();
        let v = a.volume;
        match op {
            0 => {
                let mut node = 0;
                let status = d::resolve_path(v, a.name, &mut node);
                Output {
                    value: u64::from(node),
                    status: status as i32,
                    present: status == Lookup::Found,
                }
            }
            1 => Output::code(d::resolve(v, a.name, entry) as i32),
            2 => Output::code(d::find(v, a.parent, a.name, entry) as i32),
            5 => optional(f::read_file(v, a.entry.read(), a.buffer)),
            6 => optional(f::read_range(v, a.entry.read(), a.offset, a.buffer)),
            7 => optional(f::write_range(v, a.entry.read(), a.offset, a.data)),
            8 => Output::boolean(m::write_file(v, a.parent, a.name, a.data)),
            9 => Output::boolean(
                m::append(v, a.parent, a.name, None, true, false, a.data) == Append::Ok,
            ),
            10 => Output::code(m::append(
                v,
                a.parent,
                a.name,
                Some(u32::try_from(a.offset).expect("FAT offset")),
                false,
                false,
                a.data,
            ) as i32),
            11 => Output::code(m::append(
                v,
                a.parent,
                a.name,
                Some(u32::try_from(a.offset).expect("FAT offset")),
                false,
                true,
                a.data,
            ) as i32),
            12 => Output::boolean(m::publish_copy(v, a.parent, a.name, a.second)),
            13 => Output::boolean(m::copy(
                v,
                a.other,
                a.entry.read(),
                a.parent,
                a.name,
                a.flag,
            )),
            15 => Output::boolean(m::make_directory(v, a.parent, a.name)),
            16 => Output::boolean(m::delete(v, a.parent, a.name)),
            17 => Output::code(m::delete_match(v, a.parent, a.name, a.entry.read()) as i32),
            19 => n::equal_collated(a.name.view(), a.second.view()).map_or(
                Output::default(),
                |answer| Output {
                    value: u64::from(answer),
                    status: 0,
                    present: true,
                },
            ),
            20 => Output::boolean(m::remove_directory(v, a.parent, a.name)),
            21 => Output::code(m::rename(v, a.parent, a.name, a.second) as i32),
            22 => Output::code(m::replace(v, a.parent, a.name, a.second, a.third, a.flag) as i32),
            23 => Output::code(m::replace_create(v, a.parent, a.name, a.second, a.third) as i32),
            24 => Output::boolean(s::flush_volume(v)),
            25 => optional(d::read_directory(v, a.parent, a.buffer)),
            26 => Output::code(d::read_entry(v, a.parent, a.index, a.buffer, entry) as i32),
            27 => Output::code(d::next(v, a.parent, a.cursor, entry) as i32),
            28 => {
                let state = if let Some(state) = s::runtime_for(v, false) {
                    Some(state)
                } else {
                    s::initialize(v);
                    s::runtime_for(v, false)
                };
                state.map_or(Output::default(), |state| Output {
                    value: u64::from((*state).free),
                    status: 0,
                    present: true,
                })
            }
            29 => {
                s::log(b"      FAT32 root:\r\n");
                Output::boolean(d::list(v, v.root_cluster, 12))
            }
            30 => Output::boolean(n::valid_short(a.name.view())),
            31 => Output::boolean(n::same_copy(a.entry.read(), a.peer.read())),
            35 => {
                let dev = if let Some(dev) = s::device(v.device_index) {
                    dev
                } else {
                    return Output::default();
                };
                if u64::from(v.partition_lba) >= dev.sectors {
                    return Output::default();
                }
                let Some(parsed) = s::parse(
                    v.device_index,
                    v.partition_lba,
                    dev.sectors - u64::from(v.partition_lba),
                ) else {
                    return Output::default();
                };
                if a.flag {
                    print_info(parsed);
                }
                volume.write(parsed);
                Output::boolean(true)
            }
            36 => {
                let Some(parsed) = s::parse(v.device_index, v.partition_lba, a.count) else {
                    return Output::default();
                };
                if a.flag {
                    print_info(parsed);
                }
                volume.write(parsed);
                Output::boolean(true)
            }
            37 => {
                print_info(v);
                Output::boolean(true)
            }
            38 => Output::boolean(d::list(v, a.parent, 64)),
            39 => Output::boolean(f::print_file(v, a.entry.read(), a.index)),
            40 => location(v, a.name, a.parent).filter(|e| e.is_dir()).map_or(
                Output::default(),
                |e| Output {
                    value: u64::from(e.first_cluster),
                    status: 0,
                    present: true,
                },
            ),
            41 => Output::boolean(m::attributes(v, a.parent, a.name, a.set, a.clear)),
            42 => {
                s::forget(v.device_index, a.first, a.count);
                Output::boolean(true)
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// One exact readable neutral volume; geometry arithmetic keeps original
/// checked widths and bounds. No I/O/state mutation in these pure queries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fat_geometry(op: u8, volume: *const Volume) -> u64 {
    let v = unsafe { volume.read() };
    match op {
        0 => u64::from(v.first_data()),
        1 => u64::from(v.cluster_bytes()),
        2 => u64::from(v.total_clusters()),
        _ => unreachable!(),
    }
}
/// # Safety
/// Exclusive aligned native summary output. Advisory scalar counter snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fat_summary(out: *mut super::fat_stats::Summary) {
    unsafe {
        out.write(s::STATS.snapshot());
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fat_truncations() -> u64 {
    s::DIR_TRUNCATIONS.load(Relaxed)
}
