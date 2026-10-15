//! Original FAT resident runtime, cache/batch I/O and bounded cooperative work.
//! Existing filesystem/mount admission owns state; no borrow crosses a wait.
use super::{fat_stats::Counters, fat_types::*};
use crate::{
    contract::TimeState,
    fs::page_cache as cache,
    platform::{time, timer},
    sched::scheduler,
    storage::block_registry,
};
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};
pub static STATS: Counters = Counters::new();
#[unsafe(export_name = "dir_chain_truncations")]
pub static DIR_TRUNCATIONS: AtomicU64 = AtomicU64::new(0);
pub const VOLUMES: usize = 26;
pub const EXTENTS: usize = 2048;
pub const MAP_CLUSTERS: usize = 262144;
pub const MAP_BYTES: usize = 32768;
#[derive(Clone, Copy)]
pub struct AppendCache {
    pub valid: bool,
    pub device: usize,
    pub partition: u32,
    pub parent: u32,
    pub name: [u8; NAME_MAX],
    pub name_len: usize,
    pub lba: u32,
    pub offset: usize,
    pub attr: u8,
    pub first: u32,
    pub size: u32,
    pub last: u32,
}
impl AppendCache {
    pub const EMPTY: Self = Self {
        valid: false,
        device: 0,
        partition: 0,
        parent: 0,
        name: [0; NAME_MAX],
        name_len: 0,
        lba: 0,
        offset: 0,
        attr: 0,
        first: 0,
        size: 0,
        last: 0,
    };
}
#[derive(Clone, Copy)]
pub struct Extent {
    pub valid: bool,
    pub device: usize,
    pub partition: u32,
    pub first: u32,
    pub size: u32,
    pub cluster_index: usize,
    pub cluster: u32,
    pub count: usize,
    pub last_use: u64,
}
impl Extent {
    pub const EMPTY: Self = Self {
        valid: false,
        device: 0,
        partition: 0,
        first: 0,
        size: 0,
        cluster_index: 0,
        cluster: 0,
        count: 0,
        last_use: 0,
    };
}
#[derive(Clone, Copy)]
pub struct Hint {
    pub valid: bool,
    pub device: usize,
    pub partition: u32,
    pub next: u32,
}
impl Hint {
    pub const EMPTY: Self = Self {
        valid: false,
        device: 0,
        partition: 0,
        next: 3,
    };
}
#[derive(Clone, Copy)]
pub struct FsInfo {
    pub valid: bool,
    pub free: u32,
    pub next: u32,
}
impl FsInfo {
    pub const EMPTY: Self = Self {
        valid: false,
        free: UNKNOWN,
        next: UNKNOWN,
    };
}
pub struct Runtime {
    pub valid: bool,
    pub device: usize,
    pub partition: u32,
    pub clusters: u32,
    pub fsinfo_sector: u16,
    pub backup_sector: u16,
    pub fsinfo_present: bool,
    pub fsinfo_valid: bool,
    pub map_ready: bool,
    pub scan_pending: bool,
    pub map_capable: bool,
    pub free: u32,
    pub next: u32,
    pub map: [u8; MAP_BYTES],
}
impl Runtime {
    pub const EMPTY: Self = Self {
        valid: false,
        device: 0,
        partition: 0,
        clusters: 0,
        fsinfo_sector: 0,
        backup_sector: 0,
        fsinfo_present: false,
        fsinfo_valid: false,
        map_ready: false,
        scan_pending: false,
        map_capable: false,
        free: 0,
        next: 3,
        map: [0; MAP_BYTES],
    };
}
struct State {
    append: AppendCache,
    extents: [Extent; EXTENTS],
    clock: u64,
    hints: [Hint; VOLUMES],
    volumes: [Runtime; VOLUMES],
}
struct Resident(UnsafeCell<State>);
// BSP mount/retirement admission and the existing per-volume filesystem
// requests protect mutation. Raw snapshots survive only under those exact
// lifetime/drain contracts; no exclusive global reference crosses I/O/yield.
unsafe impl Sync for Resident {}
static STATE: Resident = Resident(UnsafeCell::new(State {
    append: AppendCache::EMPTY,
    extents: [Extent::EMPTY; EXTENTS],
    clock: 0,
    hints: [Hint::EMPTY; VOLUMES],
    volumes: [const { Runtime::EMPTY }; VOLUMES],
}));
pub fn append() -> *mut AppendCache {
    unsafe { ptr::addr_of_mut!((*STATE.0.get()).append) }
}
pub fn extent(i: usize) -> *mut Extent {
    assert!(i < EXTENTS);
    unsafe {
        ptr::addr_of_mut!((*STATE.0.get()).extents)
            .cast::<Extent>()
            .add(i)
    }
}
pub fn hint(i: usize) -> *mut Hint {
    assert!(i < VOLUMES);
    unsafe {
        ptr::addr_of_mut!((*STATE.0.get()).hints)
            .cast::<Hint>()
            .add(i)
    }
}
pub fn runtime(i: usize) -> *mut Runtime {
    assert!(i < VOLUMES);
    unsafe {
        ptr::addr_of_mut!((*STATE.0.get()).volumes)
            .cast::<Runtime>()
            .add(i)
    }
}
#[inline(never)]
pub(super) unsafe fn next_clock() -> u64 {
    unsafe {
        let s = STATE.0.get();
        (*s).clock = (*s).clock.wrapping_add(1);
        (*s).clock
    }
}
#[inline(never)]
pub(super) unsafe fn runtime_for(v: Volume, create: bool) -> Option<*mut Runtime> {
    unsafe {
        let mut fallback = None;
        for i in 0..VOLUMES {
            let s = runtime(i);
            if (*s).valid && (*s).device == v.device_index && (*s).partition == v.partition_lba {
                return Some(s);
            }
            if !(*s).valid && fallback.is_none() {
                fallback = Some(s);
            }
        }
        if create { fallback } else { None }
    }
}
#[inline(never)]
pub(super) unsafe fn forget(device: usize, first: u64, count: u64) {
    unsafe {
        for i in 0..VOLUMES {
            let s = runtime(i);
            if (*s).valid
                && (*s).device == device
                && u64::from((*s).partition) >= first
                && u64::from((*s).partition) - first < count
            {
                (*s).valid = false;
            }
            let h = hint(i);
            if (*h).valid
                && (*h).device == device
                && u64::from((*h).partition) >= first
                && u64::from((*h).partition) - first < count
            {
                (*h).valid = false;
            }
        }
        for i in 0..EXTENTS {
            let e = extent(i);
            if (*e).valid
                && (*e).device == device
                && u64::from((*e).partition) >= first
                && u64::from((*e).partition) - first < count
            {
                (*e).valid = false;
            }
        }
        let a = append();
        if (*a).valid
            && (*a).device == device
            && u64::from((*a).partition) >= first
            && u64::from((*a).partition) - first < count
        {
            (*a).valid = false;
        }
    }
}
#[inline(never)]
pub(super) unsafe fn invalidate_extents() {
    unsafe {
        for i in 0..EXTENTS {
            (*extent(i)).valid = false;
        }
    }
}
#[inline(never)]
pub(super) unsafe fn invalidate_append() {
    unsafe {
        (*append()).valid = false;
        invalidate_extents();
    }
}
#[inline(never)]
pub(super) unsafe fn read(device: usize, lba: u64, sectors: u16, out: Buffer) -> bool {
    unsafe {
        if sectors == 0 {
            return false;
        }
        let ok = if sectors == 1 {
            cache::r4os_rust_pagecache_read_sector(device, lba, out.data, out.len)
        } else {
            cache::r4os_rust_pagecache_read_sectors(
                device,
                lba,
                u32::from(sectors),
                out.data,
                out.len,
            )
        };
        if ok {
            STATS.read_sectors.fetch_add(u64::from(sectors), Relaxed);
        } else {
            STATS.read_failures.fetch_add(1, Relaxed);
        }
        ok
    }
}
#[inline(never)]
pub(super) unsafe fn write(v: Volume, lba: u64, sectors: u16, data: Bytes) -> bool {
    unsafe {
        if sectors == 0 {
            return false;
        }
        let ok = if sectors == 1 {
            cache::r4os_rust_pagecache_write_sector(
                v.device_index,
                lba,
                data.data,
                data.len,
                v.write_batch,
            )
        } else {
            cache::r4os_rust_pagecache_write_direct(
                v.device_index,
                lba,
                sectors,
                data.data,
                data.len,
            )
        };
        if ok {
            STATS.write_sectors.fetch_add(u64::from(sectors), Relaxed);
        } else {
            STATS.write_failures.fetch_add(1, Relaxed);
        }
        ok
    }
}
#[inline(never)]
pub(super) unsafe fn mutation(mut v: Volume) -> Option<Volume> {
    unsafe {
        let mut batch = 0;
        if !cache::r4os_rust_pagecache_batch(&mut batch) {
            return None;
        }
        v.write_batch = batch;
        Some(v)
    }
}
#[inline(never)]
pub(super) unsafe fn flush(device: usize, batch: u64) -> bool {
    unsafe {
        let start = timer::tick_count();
        let ok = cache::r4os_rust_pagecache_flush_device(device, batch);
        let elapsed = timer::tick_count().saturating_sub(start);
        STATS.flushes.fetch_add(1, Relaxed);
        if !ok {
            STATS.flush_failures.fetch_add(1, Relaxed);
        }
        STATS.flush_last_ticks.store(elapsed, Relaxed);
        STATS.flush_total_ticks.fetch_add(elapsed, Relaxed);
        STATS.flush_max_ticks.fetch_max(elapsed, Relaxed);
        ok
    }
}
#[inline(never)]
pub(super) unsafe fn flush_volume(v: Volume) -> bool {
    unsafe { flush(v.device_index, 0) }
}
#[inline(never)]
pub(super) unsafe fn flush_mutation(v: Volume) -> bool {
    unsafe { flush(v.device_index, v.write_batch) }
}
pub struct Operation {
    op: u32,
    start: u64,
    pub ok: bool,
}
impl Operation {
    pub fn begin(op: u32) -> Self {
        STATS.active_operation.store(op, Relaxed);
        Self {
            op,
            start: timer::tick_count(),
            ok: false,
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let elapsed = timer::tick_count().saturating_sub(self.start);
        STATS.operation_last_ticks.store(elapsed, Relaxed);
        STATS.operation_total_ticks.fetch_add(elapsed, Relaxed);
        STATS.operation_max_ticks.fetch_max(elapsed, Relaxed);
        STATS.last_operation.store(self.op, Relaxed);
        STATS.active_operation.store(0, Relaxed);
        if !self.ok {
            STATS.operation_failures.fetch_add(1, Relaxed);
        }
    }
}
#[inline(never)]
pub(super) unsafe fn cooperate(steps: &mut u32) {
    *steps = steps.wrapping_add(1);
    if *steps < 16 {
        return;
    }
    *steps = 0;
    STATS.yield_points.fetch_add(1, Relaxed);
    let mut before = MaybeUninit::<scheduler::Stats>::uninit();
    let mut after = MaybeUninit::<scheduler::Stats>::uninit();
    unsafe {
        scheduler::r4os_rust_scheduler_stats(before.as_mut_ptr());
        scheduler::r4os_rust_scheduler_control(8, 0, 0);
        scheduler::r4os_rust_scheduler_stats(after.as_mut_ptr());
        if before.assume_init().yields != after.assume_init().yields {
            STATS.yields.fetch_add(1, Relaxed);
        } else {
            STATS.yield_skips.fetch_add(1, Relaxed);
        }
    }
}
unsafe extern "C" {
    fn r4os_zig_fat_text(data: *const u8, len: usize);
    fn r4os_zig_fat_decimal(value: u64);
    fn r4os_zig_fat_char(value: u8);
}
pub fn log(s: &[u8]) {
    unsafe {
        r4os_zig_fat_text(s.as_ptr(), s.len());
    }
}
pub fn decimal(v: u64) {
    unsafe {
        r4os_zig_fat_decimal(v);
    }
}
pub fn character(v: u8) {
    unsafe {
        r4os_zig_fat_char(v);
    }
}
pub fn timestamp() -> (u16, u16) {
    let mut now = TimeState::default();
    unsafe {
        time::r4os_rust_time_state(&mut now);
    }
    let valid = now.valid != 0
        && (1980..=2107).contains(&now.year)
        && (1..=12).contains(&now.month)
        && now.day >= 1
        && now.day <= days(now.year, now.month)
        && now.hour <= 23
        && now.minute <= 59
        && now.second <= 59;
    if !valid {
        return (33, 0);
    }
    let date = ((now.year - 1980) << 9) | (u16::from(now.month) << 5) | u16::from(now.day);
    let time =
        (u16::from(now.hour) << 11) | (u16::from(now.minute) << 5) | u16::from(now.second / 2);
    (date, time)
}
fn days(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}
pub fn stamp(raw: &mut [u8]) {
    let (date, time) = timestamp();
    put16(&mut raw[14..16], time);
    put16(&mut raw[16..18], date);
    put16(&mut raw[18..20], date);
    put16(&mut raw[22..24], time);
    put16(&mut raw[24..26], date);
}
pub fn valid_cluster(v: Volume, c: u32) -> bool {
    if !(2..RESERVED).contains(&c) || v.sectors_per_cluster == 0 {
        return false;
    }
    let meta =
        u64::from(v.reserved_sectors) + u64::from(v.fat_count) * u64::from(v.sectors_per_fat);
    let total = u64::from(v.total_sectors);
    if meta >= total {
        return false;
    }
    let data = (total - meta) / u64::from(v.sectors_per_cluster);
    let fat = u64::from(v.sectors_per_fat) * u64::from(v.bytes_per_sector) / 4;
    if u64::from(c) >= (data + 2).min(fat) {
        return false;
    }
    let relative = meta + (u64::from(c) - 1) * u64::from(v.sectors_per_cluster);
    u64::from(v.partition_lba) + relative <= 0x1_0000_0000
}
pub fn entry_cluster_valid(v: Volume, e: Entry) -> bool {
    if e.is_dir() {
        valid_cluster(v, e.first_cluster)
    } else if e.first_cluster == 0 {
        e.size == 0
    } else {
        valid_cluster(v, e.first_cluster)
    }
}
#[inline(never)]
pub(super) unsafe fn fat_entry(v: Volume, c: u32) -> Option<u32> {
    unsafe {
        STATS.fat_reads.fetch_add(1, Relaxed);
        let mut sector = [0u8; SECTOR];
        if !read(
            v.device_index,
            u64::from(v.fat_lba(c)),
            1,
            Buffer::of(&mut sector),
        ) {
            return None;
        }
        Some(le32(&sector[v.fat_offset(c)..][..4]) & 0x0fff_ffff)
    }
}
pub fn alloc_end(v: Volume) -> u32 {
    let fat = v.sectors_per_fat * SECTOR as u32 / 4;
    let data = v.total_clusters() + 2;
    if data >= 2 && data < fat { data } else { fat }
}
#[inline(never)]
pub(super) unsafe fn device(index: usize) -> Option<block_registry::CacheView> {
    block_registry::cache_view(index)
}
#[inline(never)]
pub(super) unsafe fn parse(device_index: usize, first: u32, count: u64) -> Option<Volume> {
    unsafe {
        let dev = device(device_index)?;
        if dev.sector_bytes != SECTOR as u32
            || u64::from(first) >= dev.sectors
            || count > dev.sectors - u64::from(first)
        {
            return None;
        }
        let mut s = [0u8; SECTOR];
        if !read(device_index, u64::from(first), 1, Buffer::of(&mut s)) {
            log(b"      FAT BPB: read failed\r\n");
            return None;
        }
        if s[510] != 0x55 || s[511] != 0xaa {
            log(b"      FAT BPB: invalid signature\r\n");
            return None;
        }
        let bps = le16(&s[11..13]);
        let spc = s[13];
        let reserved = le16(&s[14..16]);
        let fats = s[16];
        let roots = le16(&s[17..19]);
        let fat16 = le16(&s[22..24]);
        let total16 = le16(&s[19..21]);
        let total = if total16 != 0 {
            u32::from(total16)
        } else {
            le32(&s[32..36])
        };
        let fat32 = le32(&s[36..40]);
        let root = le32(&s[44..48]);
        if bps != SECTOR as u16 {
            log(b"      FAT32: unsupported sector size\r\n");
            return None;
        }
        if spc == 0 || fats == 0 || fat32 == 0 || total == 0 || roots != 0 || fat16 != 0 {
            log(b"      FAT32: unsupported BPB layout\r\n");
            return None;
        }
        let v = Volume {
            device_index,
            partition_lba: first,
            bytes_per_sector: bps,
            sectors_per_cluster: spc,
            reserved_sectors: reserved,
            fat_count: fats,
            sectors_per_fat: fat32,
            total_sectors: total,
            root_cluster: root,
            fs_info_sector: le16(&s[48..50]),
            backup_boot_sector: le16(&s[50..52]),
            ..Volume::default()
        };
        let meta = u64::from(reserved) + u64::from(fats) * u64::from(fat32);
        if spc > 128
            || !spc.is_power_of_two()
            || reserved == 0
            || fats > 2
            || meta >= u64::from(total)
            || u64::from(total) > count
            || u64::from(first) + u64::from(total) > 0x1_0000_0000
        {
            return None;
        }
        let clusters = v.total_clusters();
        if !(65525..RESERVED - 2).contains(&clusters)
            || root < 2
            || root >= clusters + 2
            || (u64::from(clusters) + 2) * 4 > u64::from(fat32) * SECTOR as u64
        {
            return None;
        }
        runtime_for(v, true)?;
        initialize(v);
        Some(v)
    }
}
#[inline(never)]
pub(super) unsafe fn initialize(v: Volume) {
    unsafe {
        let clusters = v.total_clusters();
        if clusters == 0 {
            return;
        }
        let Some(s) = runtime_for(v, true) else {
            return;
        };
        let fsinfo = read_fsinfo(v);
        let start = timer::tick_count();
        (*s).valid = true;
        (*s).device = v.device_index;
        (*s).partition = v.partition_lba;
        (*s).clusters = clusters;
        (*s).fsinfo_sector = v.fs_info_sector;
        (*s).backup_sector = v.backup_boot_sector;
        (*s).fsinfo_present = fsinfo_valid(v);
        (*s).fsinfo_valid = fsinfo.valid;
        (*s).map_ready = clusters <= MAP_CLUSTERS as u32;
        (*s).scan_pending = false;
        (*s).map_capable = (*s).map_ready;
        (*s).free = 0;
        (*s).next = 3;
        ptr::write_bytes(ptr::addr_of_mut!((*s).map).cast::<u8>(), 0, MAP_BYTES);
        if !scan(v, s) {
            (*s).valid = false;
            return;
        }
        cross_check(v, s, fsinfo);
        log(b"FAT32 mount dev=");
        decimal(v.device_index as u64);
        log(b" clusters=");
        decimal(u64::from(clusters));
        log(b" scan=full ticks=");
        decimal(timer::tick_count().wrapping_sub(start));
        log(b"\r\n");
    }
}
pub fn fsinfo_valid(v: Volume) -> bool {
    v.fs_info_sector > 0 && v.fs_info_sector < v.reserved_sectors
}
fn backup_valid(v: Volume) -> bool {
    fsinfo_valid(v)
        && v.backup_boot_sector > 0
        && u32::from(v.backup_boot_sector) + u32::from(v.fs_info_sector)
            < u32::from(v.reserved_sectors)
}
fn fsinfo_lba(v: Volume) -> u64 {
    u64::from(v.partition_lba) + u64::from(v.fs_info_sector)
}
fn backup_lba(v: Volume) -> u64 {
    u64::from(v.partition_lba) + u64::from(v.backup_boot_sector) + u64::from(v.fs_info_sector)
}
#[inline(never)]
pub(super) unsafe fn read_fsinfo(v: Volume) -> FsInfo {
    unsafe {
        if !fsinfo_valid(v) {
            return FsInfo::EMPTY;
        }
        let mut s = [0; SECTOR];
        if !read(v.device_index, fsinfo_lba(v), 1, Buffer::of(&mut s)) {
            return FsInfo::EMPTY;
        }
        STATS.fsinfo_reads.fetch_add(1, Relaxed);
        if le32(&s[..4]) != 0x41615252
            || le32(&s[484..488]) != 0x61417272
            || le32(&s[508..]) != 0xaa550000
        {
            return FsInfo::EMPTY;
        }
        FsInfo {
            valid: true,
            free: le32(&s[488..492]),
            next: le32(&s[492..496]),
        }
    }
}
#[inline(never)]
pub(super) unsafe fn write_fsinfo(v: Volume, s: *const Runtime) -> bool {
    unsafe {
        if !fsinfo_valid(v) {
            return true;
        }
        let mut data = [0u8; SECTOR];
        put32(&mut data[..4], 0x41615252);
        put32(&mut data[484..488], 0x61417272);
        put32(&mut data[488..492], (*s).free);
        put32(
            &mut data[492..496],
            if (*s).next >= 2 && (*s).next < alloc_end(v) {
                (*s).next
            } else {
                UNKNOWN
            },
        );
        put32(&mut data[508..], 0xaa550000);
        if !write(v, fsinfo_lba(v), 1, Bytes::of(&data)) {
            return false;
        }
        if backup_valid(v)
            && backup_lba(v) != fsinfo_lba(v)
            && !write(v, backup_lba(v), 1, Bytes::of(&data))
        {
            return false;
        }
        STATS.fsinfo_writes.fetch_add(1, Relaxed);
        true
    }
}
fn map_index(c: u32) -> Option<usize> {
    if c < 2 {
        return None;
    }
    let i = (c - 2) as usize;
    if i >= MAP_CLUSTERS { None } else { Some(i) }
}
#[inline(never)]
pub(super) unsafe fn map_free(s: *const Runtime, c: u32) -> bool {
    unsafe {
        if !(*s).map_ready {
            return false;
        }
        let Some(i) = map_index(c) else { return false };
        i < (*s).clusters as usize && (*s).map[i / 8] & (1u8 << (i % 8)) == 0
    }
}
#[inline(never)]
pub(super) unsafe fn map_mark(s: *mut Runtime, c: u32, used: bool) {
    unsafe {
        if !(*s).map_ready {
            return;
        }
        let Some(i) = map_index(c) else { return };
        let bit = 1u8 << (i % 8);
        if used {
            (*s).map[i / 8] |= bit;
        } else {
            (*s).map[i / 8] &= !bit;
        }
    }
}
#[inline(never)]
pub(super) unsafe fn scan(v: Volume, s: *mut Runtime) -> bool {
    unsafe {
        if (*s).map_ready {
            ptr::write_bytes(
                ptr::addr_of_mut!((*s).map).cast::<u8>(),
                0,
                ((*s).clusters as usize).div_ceil(8),
            );
        }
        let end = (*s).clusters + 2;
        let mut c = 2u32;
        let mut sector = [0u8; SECTOR];
        let mut found = false;
        let mut steps = 0;
        while c < end {
            if !read(
                v.device_index,
                u64::from(v.fat_lba(c)),
                1,
                Buffer::of(&mut sector),
            ) {
                return false;
            }
            let mut off = v.fat_offset(c);
            while c < end && off + 4 <= SECTOR {
                STATS.fat_reads.fetch_add(1, Relaxed);
                if le32(&sector[off..off + 4]) & 0x0fff_ffff != 0 {
                    map_mark(s, c, true);
                } else {
                    (*s).free = (*s).free.wrapping_add(1);
                    if !found {
                        (*s).next = c;
                        found = true;
                    }
                }
                cooperate(&mut steps);
                c += 1;
                off += 4;
            }
        }
        if !found {
            (*s).next = end;
        }
        if (*s).map_ready {
            STATS.inusemap_builds.fetch_add(1, Relaxed);
            STATS
                .inusemap_clusters
                .fetch_add(u64::from((*s).clusters), Relaxed);
        }
        true
    }
}
#[inline(never)]
pub(super) unsafe fn cross_check(v: Volume, s: *mut Runtime, info: FsInfo) {
    unsafe {
        let mut rebuild = (*s).fsinfo_present && !info.valid;
        if info.valid {
            if info.free != UNKNOWN && info.free != (*s).free {
                rebuild = true
            }
            if info.next != UNKNOWN
                && info.next >= 2
                && info.next < (*s).clusters + 2
                && map_free(s, info.next)
            {
                (*s).next = info.next;
            } else if info.next != UNKNOWN {
                rebuild = true
            }
            if !rebuild {
                STATS.fsinfo_valid_mounts.fetch_add(1, Relaxed);
            }
        } else if (*s).fsinfo_present {
            rebuild = true
        }
        if rebuild {
            STATS.fsinfo_rebuilds.fetch_add(1, Relaxed);
            write_fsinfo(v, s);
        }
        super::fat_chain::seed_hint(v, (*s).next);
    }
}
#[inline(never)]
pub(super) unsafe fn ensure_scanned(v: Volume) {
    unsafe {
        let Some(s) = runtime_for(v, false) else {
            return;
        };
        if !(*s).scan_pending {
            return;
        }
        (*s).scan_pending = false;
        (*s).map_ready = (*s).map_capable;
        let info = FsInfo {
            valid: (*s).fsinfo_valid,
            free: (*s).free,
            next: (*s).next,
        };
        (*s).free = 0;
        (*s).next = 3;
        if !scan(v, s) {
            (*s).map_ready = false;
            (*s).free = info.free;
            (*s).next = info.next;
            return;
        }
        STATS.inusemap_deferred_builds.fetch_add(1, Relaxed);
        cross_check(v, s, info);
    }
}
