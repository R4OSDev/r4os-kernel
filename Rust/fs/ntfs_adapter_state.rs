//! Original per-mount NTFS state, bounded page-cache seam and time conversion.
//! Every wait retains raw pointers to its exact mount; no whole state borrow.
use super::{
    ntfs_adapter_types::{Entry, Mounted},
    ntfs_format_types::{Run, View},
    ntfs_volume_cache::Cache,
    ntfs_volume_entry as entry, ntfs_volume_io as io, ntfs_volume_records as records,
    ntfs_volume_types as t,
};
use crate::{
    fs::{fat_state, page_cache},
    memory::heap,
    platform::timer,
    storage::block_registry,
};
use core::{
    cell::UnsafeCell,
    mem::{MaybeUninit, align_of, size_of},
    ptr,
};
pub const VOLUMES: usize = 26;
pub struct State {
    pub in_use: bool,
    pub device: usize,
    pub partition: u32,
    pub cluster: u32,
    pub record: u32,
    pub index: u32,
    pub sectors: u64,
    pub mft: [Run; 64],
    pub mft_count: usize,
    pub upcase: *mut u8,
    pub scratch: *mut t::Scratch,
    pub cache: MaybeUninit<Cache>,
    pub cache_ready: bool,
    pub first_lba: u64,
    pub end_lba: u64,
}
impl State {
    const EMPTY: Self = Self {
        in_use: false,
        device: 0,
        partition: 0,
        cluster: 0,
        record: 0,
        index: 0,
        sectors: 0,
        mft: [Run {
            length_clusters: 0,
            has_lcn: false,
            lcn: 0,
        }; 64],
        mft_count: 0,
        upcase: ptr::null_mut(),
        scratch: ptr::null_mut(),
        cache: MaybeUninit::uninit(),
        cache_ready: false,
        first_lba: 0,
        end_lba: 0,
    };
}
struct Table(UnsafeCell<[State; VOLUMES]>);
// SAFETY: Mount lifecycle and operations retain their original storage and
// per-volume filesystem request gates; independent slots never share scratch.
unsafe impl Sync for Table {}
static STATES: Table = Table(UnsafeCell::new([const { State::EMPTY }; VOLUMES]));
pub(super) unsafe fn state(slot: usize) -> *mut State {
    assert!(slot < VOLUMES);
    unsafe { STATES.0.get().cast::<State>().add(slot) }
}
pub(super) unsafe fn cache(s: *mut State) -> *mut Cache {
    let _owner = crate::memory::owner::Guard::acquire(1);
    unsafe {
        let c = ptr::addr_of_mut!((*s).cache).cast::<Cache>();
        if !(*s).cache_ready {
            Cache::initialize(c);
            (*s).cache_ready = true;
        }
        c
    }
}
unsafe fn contains(s: *mut State, lba: u64, count: u32) -> bool {
    unsafe {
        lba >= (*s).first_lba && lba <= (*s).end_lba && u64::from(count) <= (*s).end_lba - lba
    }
}
unsafe extern "C" fn read(
    ctx: *mut core::ffi::c_void,
    lba: u64,
    count: u32,
    out: *mut u8,
    len: usize,
) -> bool {
    unsafe {
        let s = ctx.cast::<State>();
        contains(s, lba, count)
            && page_cache::r4os_rust_pagecache_read_sectors((*s).device, lba, count, out, len)
    }
}
unsafe extern "C" fn write(
    ctx: *mut core::ffi::c_void,
    lba: u64,
    count: u32,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        let s = ctx.cast::<State>();
        if !contains(s, lba, count) {
            return false;
        }
        if count == 0 {
            return true;
        }
        if len < (count as usize) * 512 {
            return false;
        }
        if count == 1 {
            return page_cache::r4os_rust_pagecache_write_sector((*s).device, lba, data, 512, 0);
        }
        let mut done = 0;
        while done < count {
            let n = (count - done).min(2048);
            if !page_cache::r4os_rust_pagecache_write_direct(
                (*s).device,
                lba + u64::from(done),
                n as u16,
                data.add(done as usize * 512),
                n as usize * 512,
            ) {
                return false;
            }
            done += n;
        }
        true
    }
}
unsafe extern "C" fn flush(ctx: *mut core::ffi::c_void) -> bool {
    unsafe { page_cache::r4os_rust_pagecache_flush_device((*ctx.cast::<State>()).device, 0) }
}
pub(super) unsafe fn build(v: Mounted) -> t::Volume {
    unsafe {
        let s = state(usize::from(v.state_slot));
        t::Volume {
            io: io::Io {
                device: io::Device {
                    context: s.cast(),
                    read,
                    write,
                    flush,
                    max_transfer_sectors: 0,
                },
                geometry: t::Geometry {
                    partition_lba: (*s).partition,
                    cluster_bytes: (*s).cluster,
                    total_sectors: (*s).sectors,
                },
                scratch: (*s).scratch,
                cache: cache(s),
                work_counters: false,
                read_scratch: ptr::null_mut(),
                read_identity: ptr::null(),
            },
            record_bytes: (*s).record,
            index_block_bytes: (*s).index,
            mft_runs: ptr::addr_of_mut!((*s).mft).cast(),
            mft_capacity: 64,
            mft_count: ptr::addr_of_mut!((*s).mft_count),
            upcase: View {
                data: (*s).upcase,
                len: if (*s).upcase.is_null() { 0 } else { 131072 },
            },
            // No calendar acquisition for a read/cache/flush view. Timestamp
            // writers must explicitly admit one current operation time.
            now_filetime: 0,
            security_id_file: 265,
            security_id_dir: 264,
            cache_now_ticks: timer::tick_count(),
        }
    }
}
pub(super) fn with_write_time(mut volume: t::Volume) -> Option<t::Volume> {
    volume.now_filetime = super::ntfs_adapter_time::now_filetime()?;
    Some(volume)
}
pub(super) unsafe fn build_write(volume: Mounted) -> Option<t::Volume> {
    unsafe { with_write_time(build(volume)) }
}
pub(super) unsafe fn release(s: *mut State) {
    unsafe {
        if !(*s).upcase.is_null() {
            let _ = heap::r4os_rust_heap_free((*s).upcase, 131072);
        }
        if !(*s).scratch.is_null() {
            let _ = heap::r4os_rust_heap_free((*s).scratch.cast(), size_of::<t::Scratch>());
        }
        (*s).upcase = ptr::null_mut();
        (*s).scratch = ptr::null_mut();

        let _owner = crate::memory::owner::Guard::acquire(1);
        (*s).in_use = false;
        super::ntfs_volume_cache::with(cache(s), |cache| cache.invalidate_external());
    }
}
struct Rollback(*mut State, bool);
impl Drop for Rollback {
    fn drop(&mut self) {
        if !self.1 {
            unsafe { release(self.0) }
        }
    }
}

#[inline(never)]
pub(super) unsafe fn inspect(device: usize, first: u32, bound: Option<u64>) -> Option<Mounted> {
    unsafe {
        let physical = block_registry::cache_view(device)?;
        if u64::from(first) >= physical.sectors {
            return None;
        }
        let sectors = bound.unwrap_or(physical.sectors - u64::from(first));
        if physical.sector_bytes != 512 || sectors > physical.sectors - u64::from(first) {
            return None;
        }
        let mut slot = VOLUMES;
        for i in 0..VOLUMES {
            let s = state(i);
            if (*s).in_use && (*s).device == device && (*s).partition == first {
                slot = i;
                break;
            }
            if !(*s).in_use && slot == VOLUMES {
                slot = i;
            }
        }
        if slot >= VOLUMES {
            fat_state::log(b"      NTFS: no free volume slot\r\n");
            return None;
        }
        let s = state(slot);
        if (*s).in_use {
            if (*s).sectors > sectors {
                return None;
            }
            return Some(Mounted {
                state_slot: slot as u8,
                cluster_bytes: (*s).cluster,
                total_sectors: (*s).sectors,
            });
        }
        (*s).scratch =
            heap::r4os_rust_heap_alloc(size_of::<t::Scratch>(), align_of::<t::Scratch>()).cast();
        if (*s).scratch.is_null() {
            fat_state::log(b"      NTFS mount failed: scratch alloc\r\n");
            return None;
        }
        t::Scratch::initialize((*s).scratch);
        let mut rollback = Rollback(s, false);
        (*s).device = device;
        (*s).first_lba = u64::from(first);
        (*s).end_lba = u64::from(first) + sectors;
        let d = io::Device {
            context: s.cast(),
            read,
            write,
            flush,
            max_transfer_sectors: 0,
        };
        let Some(info) = records::mount(
            d,
            first,
            (*s).scratch,
            ptr::addr_of_mut!((*s).mft).cast(),
            64,
            false,
        ) else {
            fat_state::log(
                b"      NTFS: mount failed (invalid boot sector or unsupported layout)\r\n",
            );
            return None;
        };
        if info.cluster_bytes / 512 == 0
            || info.total_sectors > sectors
            || info.total_sectors / u64::from(info.cluster_bytes / 512) > 0xffff_ffff
        {
            return None;
        }
        (*s).end_lba = u64::from(first) + info.total_sectors;
        (*s).partition = first;
        (*s).cluster = info.cluster_bytes;
        (*s).record = info.record_bytes;
        (*s).index = info.index_block_bytes;
        (*s).sectors = info.total_sectors;
        (*s).mft_count = info.mft_run_count;
        super::ntfs_volume_cache::with(cache(s), |cache| cache.begin_mount(1000));
        {
            let _owner = crate::memory::owner::Guard::acquire(1);
            (*s).in_use = true;
        }
        let v = Mounted {
            state_slot: slot as u8,
            cluster_bytes: info.cluster_bytes,
            total_sectors: info.total_sectors,
        };
        if (*s).upcase.is_null() {
            (*s).upcase = heap::r4os_rust_heap_alloc(131072, 8);
            if (*s).upcase.is_null() {
                fat_state::log(b"      NTFS mount failed: upcase alloc\r\n");

                let _owner = crate::memory::owner::Guard::acquire(1);
                (*s).in_use = false;
                return None;
            }
        }
        let got = entry::read_file(
            build(v),
            10,
            None,
            0,
            super::ntfs_format_types::Buffer {
                data: (*s).upcase,
                len: 131072,
            },
        );
        match got {
            None => {
                fat_state::log(b"      NTFS mount failed: upcase read\r\n");
                return None;
            }
            Some(n) if n != 131072 => {
                fat_state::log(b"      NTFS mount failed: upcase size\r\n");
                return None;
            }
            _ => {}
        }
        fat_state::log(b"      NTFS 3.1: cluster=");
        fat_state::decimal(u64::from(info.cluster_bytes));
        fat_state::log(b" record=");
        fat_state::decimal(u64::from(info.record_bytes));
        fat_state::log(b" mft_runs=");
        fat_state::decimal(info.mft_run_count as u64);
        fat_state::log(if info.used_mft_mirror {
            b" read-only (MFT mirror bootstrap; repair required)\r\n"
        } else {
            b" read-write\r\n"
        });
        rollback.1 = true;
        Some(v)
    }
}

fn dos(stamp: u64) -> (u16, u16) {
    if stamp == 0 {
        return (0, 0);
    }
    let seconds = stamp / 10_000_000;
    let day_seconds = seconds % 86400;
    let z = i64::try_from(seconds / 86400).expect("FILETIME days") - 134774 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe.div_euclid(1460) + doe.div_euclid(36524) - doe.div_euclid(146096))
        .div_euclid(365);
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe.div_euclid(4) - yoe.div_euclid(100));
    let mp = (5 * doy + 2).div_euclid(153);
    let d = doy - (153 * mp + 2).div_euclid(5) + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = if m <= 2 { y + 1 } else { y };
    if !(1980..=2107).contains(&year) {
        return (0, 0);
    }
    let date = ((year - 1980) as u16) << 9 | (m as u16) << 5 | d as u16;
    let time = ((day_seconds / 3600) as u16) << 11
        | (((day_seconds % 3600) / 60) as u16) << 5
        | ((day_seconds % 60) / 2) as u16;
    (date, time)
}
pub fn project(e: t::Entry) -> Entry {
    let created = dos(e.created_time_nt);
    let modified = dos(e.modified_time_nt);
    let accessed = dos(e.access_time_nt);
    Entry {
        name: e.name,
        name_len: e.name_len,
        attr: e.attr,
        reparse: e.reparse,
        record: e.record,
        sequence: e.sequence,
        size: e.size,
        created_date: created.0,
        created_time: created.1,
        modified_date: modified.0,
        modified_time: modified.1,
        access_date: accessed.0,
    }
}
