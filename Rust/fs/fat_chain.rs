//! Original FAT allocation, mirrored updates, physical chain bounds and
//! FSINFO bookkeeping. Raw state pointers never become wait-spanning borrows.
use super::{fat_state as s, fat_types::*};
use core::sync::atomic::Ordering::Relaxed;
use s::STATS;
#[inline(never)]
pub(super) unsafe fn write_fat(v: Volume, cluster: u32, value: u32) -> bool {
    unsafe { write_run(v, cluster, 1, value) }
}
#[inline(never)]
pub(super) unsafe fn write_run(v: Volume, first: u32, count: usize, fill: u32) -> bool {
    unsafe {
        if count == 0 || first < 2 {
            return false;
        }
        let end = first + u32::try_from(count - 1).expect("FAT run");
        if end >= s::alloc_end(v) {
            return false;
        }
        STATS.fat_writes.fetch_add(count as u64, Relaxed);
        for fat in 0..v.fat_count {
            let mut c = first;
            let mut remaining = count;
            let mut steps = 0;
            while remaining > 0 {
                let lba = v.partition_lba
                    + u32::from(v.reserved_sectors)
                    + u32::from(fat) * v.sectors_per_fat
                    + (c * 4) / u32::from(v.bytes_per_sector);
                let mut data = [0; SECTOR];
                if !s::read(v.device_index, u64::from(lba), 1, Buffer::of(&mut data)) {
                    return false;
                }
                let mut off = ((c * 4) % u32::from(v.bytes_per_sector)) as usize;
                while remaining > 0 && off + 4 <= SECTOR {
                    let next = if remaining == 1 { fill } else { c + 1 };
                    let old = le32(&data[off..off + 4]);
                    put32(
                        &mut data[off..off + 4],
                        (old & 0xf000_0000) | (next & 0x0fff_ffff),
                    );
                    c += 1;
                    remaining -= 1;
                    off += 4;
                }
                if !s::write(v, u64::from(lba), 1, Bytes::of(&data)) {
                    return false;
                }
                STATS.fat_mirror_writes.fetch_add(1, Relaxed);
                STATS.fat_sector_writes.fetch_add(1, Relaxed);
                s::cooperate(&mut steps);
            }
        }
        true
    }
}
#[inline(never)]
pub(super) unsafe fn hint_for(v: Volume) -> *mut s::Hint {
    unsafe {
        let mut fallback = None;
        for i in 0..s::VOLUMES {
            let h = s::hint(i);
            if (*h).valid && (*h).device == v.device_index && (*h).partition == v.partition_lba {
                return h;
            }
            if !(*h).valid && fallback.is_none() {
                fallback = Some(h);
            }
        }
        let h = fallback.unwrap_or_else(|| s::hint(0));
        let next = s::runtime_for(v, false).map_or(3, |state| (*state).next);
        h.write(s::Hint {
            valid: true,
            device: v.device_index,
            partition: v.partition_lba,
            next: next.max(2),
        });
        if next < 2 {
            (*h).next = 3;
        }
        h
    }
}
#[inline(never)]
pub(super) unsafe fn seed_hint(v: Volume, next: u32) {
    unsafe {
        (*hint_for(v)).next = if next >= 2 { next } else { 3 };
    }
}
#[inline(never)]
pub(super) unsafe fn next_start(v: Volume, requested: u32) -> u32 {
    unsafe {
        let normal = if requested >= 2 { requested } else { 3 };
        if normal > 3 {
            return normal;
        }
        if let Some(state) = s::runtime_for(v, false)
            && (*state).next > normal
            && (*state).next < s::alloc_end(v)
        {
            return (*state).next;
        }
        let h = hint_for(v);
        if (*h).next > normal && (*h).next < EOC {
            (*h).next
        } else {
            normal
        }
    }
}
#[inline(never)]
pub(super) unsafe fn next_free(s: *const s::Runtime, requested: u32) -> u32 {
    unsafe {
        let end = (*s).clusters + 2;
        let mut c = if requested >= 2 && requested < end {
            requested
        } else {
            2
        };
        if (*s).map_ready {
            while c < end {
                if s::map_free(s, c) {
                    return c;
                }
                c += 1;
            }
            c = 2;
            while c < requested && c < end {
                if s::map_free(s, c) {
                    return c;
                }
                c += 1;
            }
        }
        end
    }
}
#[inline(never)]
#[allow(dead_code)] // Original unused single-cluster allocation helper.
pub(super) unsafe fn allocated_one(v: Volume, c: u32) {
    unsafe {
        let h = hint_for(v);
        if c >= (*h).next {
            (*h).next = c + 1;
        }
        if let Some(state) = s::runtime_for(v, false) {
            if (*state).map_ready && s::map_free(state, c) {
                s::map_mark(state, c, true);
                if (*state).free > 0 {
                    (*state).free -= 1;
                }
            } else if !(*state).map_ready && (*state).free > 0 {
                (*state).free -= 1;
            }
            if c >= (*state).next {
                (*state).next = next_free(state, c + 1);
            }
        }
    }
}
#[inline(never)]
pub(super) unsafe fn allocated_range(v: Volume, first: u32, count: usize) {
    unsafe {
        if first < 2 || count == 0 {
            return;
        }
        let last = first + u32::try_from(count - 1).expect("allocation range");
        let h = hint_for(v);
        if last >= (*h).next {
            (*h).next = last + 1;
        }
        if let Some(state) = s::runtime_for(v, false) {
            let mut used = 0u32;
            for i in 0..count {
                let c = first + u32::try_from(i).expect("allocation offset");
                if (*state).map_ready {
                    if s::map_free(state, c) {
                        s::map_mark(state, c, true);
                        used += 1;
                    }
                } else {
                    used += 1;
                }
            }
            (*state).free = (*state).free.saturating_sub(used);
            if last >= (*state).next {
                (*state).next = next_free(state, last + 1);
            }
        }
    }
}
#[inline(never)]
pub(super) unsafe fn freed(v: Volume, c: u32) {
    unsafe {
        let h = hint_for(v);
        if c >= 2 && c < (*h).next {
            (*h).next = c;
        }
        if let Some(state) = s::runtime_for(v, false) {
            if (*state).map_ready {
                if !s::map_free(state, c) {
                    s::map_mark(state, c, false);
                    (*state).free = (*state).free.wrapping_add(1);
                }
            } else {
                (*state).free = (*state).free.wrapping_add(1);
            }
            if c >= 2 && c < (*state).next {
                (*state).next = c;
            }
        }
    }
}
#[inline(never)]
pub(super) unsafe fn write_runtime_info(v: Volume) -> bool {
    unsafe { s::runtime_for(v, false).is_none_or(|state| s::write_fsinfo(v, state)) }
}
#[inline(never)]
pub(super) unsafe fn free(v: Volume, start: u32) -> bool {
    unsafe {
        let mut c = start;
        let mut guard = 0;
        let mut steps = 0;
        while c >= 2 && c < s::alloc_end(v) && guard < v.total_clusters() {
            let Some(next) = s::fat_entry(v, c) else {
                return false;
            };
            if next < EOC && (next < 2 || next >= s::alloc_end(v)) {
                return false;
            }
            if !write_fat(v, c, 0) {
                return false;
            }
            freed(v, c);
            s::cooperate(&mut steps);
            if next >= EOC {
                return write_runtime_info(v);
            }
            STATS.cluster_walk_steps.fetch_add(1, Relaxed);
            c = next;
            guard += 1;
        }
        false
    }
}
#[inline(never)]
#[allow(dead_code)] // Original dormant single-cluster search path.
pub(super) unsafe fn find_one(v: Volume, start: u32) -> Option<u32> {
    unsafe {
        s::ensure_scanned(v);
        let end = s::alloc_end(v);
        let normal = if start >= 2 && start < end { start } else { 3 };
        let mut c = next_start(v, normal);
        let mut steps = 0;
        while c < end {
            STATS.alloc_search_steps.fetch_add(1, Relaxed);
            s::cooperate(&mut steps);
            if s::fat_entry(v, c)? == 0 {
                allocated_one(v, c);
                return Some(c);
            }
            c += 1;
        }
        if c != normal {
            c = normal;
            let stop = next_start(v, normal);
            while c < stop && c < end {
                STATS.alloc_search_steps.fetch_add(1, Relaxed);
                s::cooperate(&mut steps);
                if s::fat_entry(v, c)? == 0 {
                    allocated_one(v, c);
                    return Some(c);
                }
                c += 1;
            }
        }
        None
    }
}
unsafe fn scan_map(v: Volume, start: u32, stop: u32, wanted: usize, mut best: Run) -> Option<Run> {
    unsafe {
        let Some(state) = s::runtime_for(v, false) else {
            STATS.inusemap_alloc_misses.fetch_add(1, Relaxed);
            return None;
        };
        if !(*state).map_ready || wanted == 0 || start >= stop {
            STATS.inusemap_alloc_misses.fetch_add(1, Relaxed);
            return None;
        }
        let mut first = 0;
        let mut count = 0;
        let mut steps = 0;
        for c in start..stop {
            STATS.alloc_search_steps.fetch_add(1, Relaxed);
            s::cooperate(&mut steps);
            if s::map_free(state, c) {
                if count == 0 {
                    first = c;
                }
                count += 1;
                if count >= wanted {
                    STATS.inusemap_alloc_hits.fetch_add(1, Relaxed);
                    return Some(Run {
                        first,
                        count: wanted,
                    });
                }
            } else {
                if count > best.count {
                    best = Run { first, count };
                }
                count = 0;
            }
        }
        if count > best.count {
            best = Run { first, count };
        }
        if best.count > 0 {
            STATS.inusemap_alloc_hits.fetch_add(1, Relaxed);
            Some(best)
        } else {
            STATS.inusemap_alloc_misses.fetch_add(1, Relaxed);
            None
        }
    }
}
unsafe fn scan_fat(v: Volume, start: u32, stop: u32, wanted: usize, mut best: Run) -> Option<Run> {
    unsafe {
        if wanted == 0 || start >= stop {
            return if best.count > 0 { Some(best) } else { None };
        }
        let mut first = 0;
        let mut count = 0;
        let mut c = start;
        let mut steps = 0;
        while c < stop {
            let mut data = [0; SECTOR];
            if !s::read(
                v.device_index,
                u64::from(v.fat_lba(c)),
                1,
                Buffer::of(&mut data),
            ) {
                return None;
            }
            let mut off = v.fat_offset(c);
            while c < stop && off + 4 <= SECTOR {
                STATS.fat_reads.fetch_add(1, Relaxed);
                STATS.alloc_search_steps.fetch_add(1, Relaxed);
                s::cooperate(&mut steps);
                if le32(&data[off..off + 4]) & 0x0fff_ffff == 0 {
                    if count == 0 {
                        first = c;
                    }
                    count += 1;
                    if count >= wanted {
                        return Some(Run {
                            first,
                            count: wanted,
                        });
                    }
                } else {
                    if count > best.count {
                        best = Run { first, count };
                    }
                    count = 0;
                }
                c += 1;
                off += 4;
            }
        }
        if count > best.count {
            best = Run { first, count };
        }
        if best.count > 0 { Some(best) } else { None }
    }
}
#[inline(never)]
pub(super) unsafe fn find_run(v: Volume, start: u32, wanted: usize) -> Option<Run> {
    unsafe {
        if wanted == 0 {
            return None;
        }
        s::ensure_scanned(v);
        let end = s::alloc_end(v);
        if end <= 2 {
            return None;
        }
        let normal = if start >= 2 && start < end { start } else { 3 };
        let hinted = next_start(v, normal);
        let hinted = if hinted >= 2 && hinted < end {
            hinted
        } else {
            normal
        };
        if let Some(first) = scan_map(v, hinted, end, wanted, Run::default()) {
            if first.count >= wanted {
                return Some(first);
            }
            if hinted != normal
                && let Some(second) = scan_map(v, normal, hinted, wanted, first)
            {
                return Some(second);
            }
            return Some(first);
        }
        if let Some(first) = scan_fat(v, hinted, end, wanted, Run::default()) {
            if first.count >= wanted {
                return Some(first);
            }
            if hinted != normal
                && let Some(second) = scan_fat(v, normal, hinted, wanted, first)
            {
                return Some(second);
            }
            return Some(first);
        }
        if hinted != normal {
            scan_fat(v, normal, hinted, wanted, Run::default())
        } else {
            None
        }
    }
}
#[inline(never)]
pub(super) unsafe fn allocate_from(v: Volume, count: usize, start: u32) -> Option<Allocation> {
    unsafe {
        if count == 0 {
            return Some(Allocation::default());
        }
        STATS.alloc_chain_calls.fetch_add(1, Relaxed);
        let mut first = 0;
        let mut prev = 0;
        let mut allocated = 0;
        let mut search = if start >= 2 { start } else { 3 };
        let mut steps = 0;
        while allocated < count {
            let Some(run) = find_run(v, search, count - allocated) else {
                if first >= 2 {
                    free(v, first);
                }
                return None;
            };
            if run.first < 2 || run.count == 0 {
                if first >= 2 {
                    free(v, first);
                }
                return None;
            }
            if !write_run(v, run.first, run.count, EOC_MARK) {
                if first >= 2 {
                    free(v, first);
                }
                return None;
            }
            if prev >= 2 && !write_fat(v, prev, run.first) {
                free(v, first);
                free(v, run.first);
                return None;
            }
            if first == 0 {
                first = run.first;
            }
            prev = run.first + u32::try_from(run.count - 1).expect("allocation run");
            allocated += run.count;
            allocated_range(v, run.first, run.count);
            STATS.alloc_runs.fetch_add(1, Relaxed);
            STATS
                .alloc_run_clusters
                .fetch_add(run.count as u64, Relaxed);
            STATS
                .alloc_run_max_clusters
                .fetch_max(run.count as u64, Relaxed);
            search = prev + 1;
            s::cooperate(&mut steps);
        }
        STATS.alloc_clusters.fetch_add(count as u64, Relaxed);
        if !write_runtime_info(v) {
            if first >= 2 {
                free(v, first);
            }
            return None;
        }
        Some(Allocation { first, last: prev })
    }
}
#[inline(never)]
pub(super) unsafe fn allocate(v: Volume, count: usize) -> Option<u32> {
    unsafe { allocate_from(v, count, 3).map(|a| a.first) }
}
#[inline(never)]
pub(super) unsafe fn prefer(v: Volume, count: usize, start: u32) -> Option<Allocation> {
    unsafe {
        let normal = if (2..EOC).contains(&start) { start } else { 3 };
        if let Some(a) = allocate_from(v, count, normal) {
            return Some(a);
        }
        if normal != 3 {
            allocate_from(v, count, 3)
        } else {
            None
        }
    }
}
#[allow(clippy::manual_div_ceil)] // Retain original checked-add overflow boundary.
pub fn clusters_for(v: Volume, bytes: usize) -> usize {
    let size = usize::from(v.bytes_per_sector) * usize::from(v.sectors_per_cluster);
    (bytes + size - 1) / size
}
#[allow(clippy::manual_div_ceil)] // Retain original checked-add overflow boundary.
pub fn file_clusters(e: Entry, size: usize) -> usize {
    if e.size == 0 || size == 0 {
        0
    } else {
        (e.size as usize + size - 1) / size
    }
}
#[inline(never)]
pub(super) unsafe fn nth(v: Volume, start: u32, index: usize) -> Option<u32> {
    unsafe {
        if index >= v.total_clusters() as usize {
            return None;
        }
        let mut c = start;
        let mut i = 0;
        let mut steps = 0;
        while i < index && c >= 2 && c < s::alloc_end(v) {
            c = s::fat_entry(v, c)?;
            STATS.cluster_walk_steps.fetch_add(1, Relaxed);
            s::cooperate(&mut steps);
            i += 1;
        }
        if i != index || c < 2 || c >= s::alloc_end(v) {
            None
        } else {
            Some(c)
        }
    }
}
pub struct DirChain {
    v: Volume,
    pub cluster: u32,
    pub steps: usize,
    pub error: bool,
}
impl DirChain {
    pub fn new(v: Volume, start: u32) -> Self {
        Self {
            v,
            cluster: start,
            steps: 0,
            error: !s::valid_cluster(v, start),
        }
    }
    #[inline(never)]
    pub(super) unsafe fn next(&mut self) -> Option<u32> {
        unsafe {
            if self.error || is_eoc(self.cluster) {
                return None;
            }
            if !s::valid_cluster(self.v, self.cluster) {
                self.error = true;
                return None;
            }
            if self.steps >= DIR_LIMIT {
                s::DIR_TRUNCATIONS.fetch_add(1, Relaxed);
                self.error = true;
                s::log(b"FAT32 WARN: dir cluster chain > ");
                s::decimal(DIR_LIMIT as u64);
                s::log(b" (loop/corruption?), scan truncated\r\n");
                return None;
            }
            let current = self.cluster;
            self.steps += 1;
            if let Some(next) = s::fat_entry(self.v, current) {
                if is_eoc(next) {
                    self.cluster = EOC;
                } else if s::valid_cluster(self.v, next) {
                    self.cluster = next;
                } else {
                    self.error = true;
                    self.cluster = EOC;
                }
            } else {
                self.error = true;
                self.cluster = EOC;
            }
            Some(current)
        }
    }
}
