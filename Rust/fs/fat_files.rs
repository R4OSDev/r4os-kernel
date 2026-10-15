//! Original payload reads/writes and exact append/extent acceleration state.
use super::{fat_chain as chain, fat_state as s, fat_types::*};
use core::sync::atomic::Ordering::Relaxed;
use s::STATS;
#[derive(Clone, Copy)]
pub struct ReadCursor {
    pub index: usize,
    pub cluster: u32,
    pub remaining: usize,
}
#[inline(never)]
pub(super) unsafe fn read_file(v: Volume, e: Entry, out: Buffer) -> Option<usize> {
    unsafe {
        let mut op = s::Operation::begin(3);
        if e.is_dir() || out.len < e.size as usize {
            return None;
        }
        let bytes = v.cluster_bytes();
        if bytes == 0 {
            return None;
        }
        let needed = chain::file_clusters(e, bytes as usize);
        if needed > v.total_clusters() as usize {
            return None;
        }
        let mut c = e.first_cluster;
        let mut remaining = e.size as usize;
        let mut copied = 0;
        let mut data = [0; SECTOR];
        let mut guard = 0;
        let mut steps = 0;
        while remaining > 0 && guard < needed {
            if !s::valid_cluster(v, c) {
                return None;
            }
            for sector in 0..v.sectors_per_cluster {
                if remaining == 0 {
                    break;
                }
                if !s::read(
                    v.device_index,
                    u64::from(v.cluster_lba(c) + u32::from(sector)),
                    1,
                    Buffer::of(&mut data),
                ) {
                    return None;
                }
                let count = remaining.min(SECTOR);
                Bytes::of(&data[..count]).copy(out.data.add(copied));
                copied += count;
                remaining -= count;
                s::cooperate(&mut steps);
            }
            if remaining == 0 {
                break;
            }
            let next = s::fat_entry(v, c)?;
            if next >= EOC {
                break;
            }
            STATS.cluster_walk_steps.fetch_add(1, Relaxed);
            c = next;
            guard += 1;
        }
        if remaining != 0 {
            return None;
        }
        op.ok = true;
        Some(copied)
    }
}
fn matches(e: s::Extent, v: Volume, file: Entry) -> bool {
    e.valid
        && e.device == v.device_index
        && e.partition == v.partition_lba
        && e.first == file.first_cluster
        && e.size == file.size
}
#[inline(never)]
pub(super) unsafe fn cached(v: Volume, file: Entry, target: usize) -> Option<ReadCursor> {
    unsafe {
        let mut best = None;
        let mut best_index = 0;
        for i in 0..s::EXTENTS {
            let e = s::extent(i).read();
            if !matches(e, v, file) || e.cluster < 2 || e.cluster >= EOC || e.count == 0 {
                continue;
            }
            let start = e.cluster_index;
            let end = start + e.count;
            if target >= start && target < end {
                let off = target - start;
                (*s::extent(i)).last_use = s::next_clock();
                STATS.read_extent_cache_hits.fetch_add(1, Relaxed);
                return Some(ReadCursor {
                    index: target,
                    cluster: e.cluster + u32::try_from(off).expect("extent offset"),
                    remaining: e.count - off,
                });
            }
            if end <= target && (best.is_none() || end - 1 > best_index) {
                let off = e.count - 1;
                best_index = end - 1;
                best = Some(ReadCursor {
                    index: best_index,
                    cluster: e.cluster + u32::try_from(off).expect("extent offset"),
                    remaining: 1,
                });
            }
        }
        if best.is_some() {
            STATS.read_extent_cache_hits.fetch_add(1, Relaxed);
        } else {
            STATS.read_extent_cache_misses.fetch_add(1, Relaxed);
        }
        best
    }
}
#[inline(never)]
pub(super) unsafe fn remember_extent(
    v: Volume,
    file: Entry,
    index: usize,
    cluster: u32,
    count: usize,
) {
    unsafe {
        if file.first_cluster < 2 || !(2..EOC).contains(&cluster) || count == 0 {
            return;
        }
        let mut target = None;
        for i in 0..s::EXTENTS {
            let e = s::extent(i).read();
            if matches(e, v, file) && e.cluster_index == index {
                target = Some(i);
                break;
            }
        }
        if target.is_none() {
            for i in 0..s::EXTENTS {
                if !(*s::extent(i)).valid {
                    target = Some(i);
                    break;
                }
            }
        }
        let i = target.unwrap_or_else(|| {
            let mut best = 0;
            let mut tick = (*s::extent(0)).last_use;
            for i in 1..s::EXTENTS {
                if (*s::extent(i)).last_use < tick {
                    tick = (*s::extent(i)).last_use;
                    best = i;
                }
            }
            best
        });
        s::extent(i).write(s::Extent {
            valid: true,
            device: v.device_index,
            partition: v.partition_lba,
            first: file.first_cluster,
            size: file.size,
            cluster_index: index,
            cluster,
            count,
            last_use: s::next_clock(),
        });
        STATS.read_extent_cache_stores.fetch_add(1, Relaxed);
        STATS
            .read_extent_cache_clusters
            .fetch_add(count as u64, Relaxed);
    }
}
#[inline(never)]
pub(super) unsafe fn extent_from(
    v: Volume,
    file: Entry,
    total: usize,
    index: usize,
    start: u32,
    steps: &mut u32,
) -> usize {
    unsafe {
        if file.first_cluster < 2 || !(2..EOC).contains(&start) || total == 0 || index >= total {
            return 1;
        }
        let max = (total - index).min(256);
        let mut count = 1;
        let mut cluster = start;
        let mut guard = 0;
        while count < max && (2..EOC).contains(&cluster) && guard < 256 {
            let Some(next) = s::fat_entry(v, cluster) else {
                break;
            };
            STATS.cluster_walk_steps.fetch_add(1, Relaxed);
            if next != cluster + 1 || next >= EOC {
                break;
            }
            cluster = next;
            count += 1;
            s::cooperate(steps);
            guard += 1;
        }
        remember_extent(v, file, index, start, count);
        count
    }
}
#[inline(never)]
pub(super) unsafe fn read_range(v: Volume, e: Entry, offset: usize, out: Buffer) -> Option<usize> {
    unsafe {
        let mut op = s::Operation::begin(4);
        if e.is_dir() || offset > e.size as usize {
            return None;
        }
        if out.len == 0 || offset == e.size as usize {
            op.ok = true;
            return Some(0);
        }
        let wanted = out.len.min(e.size as usize - offset);
        let size = v.cluster_bytes() as usize;
        let target = offset / size;
        let total = chain::file_clusters(e, size);
        let mut cluster = e.first_cluster;
        let mut index = 0;
        let mut contiguous = 1;
        if let Some(c) = cached(v, e, target) {
            cluster = c.cluster;
            index = c.index;
            contiguous = c.remaining;
        }
        if cluster < 2 {
            return if wanted == 0 { Some(0) } else { None };
        }
        let mut skip = target - index;
        let mut guard = 0;
        let mut steps = 0;
        while skip > 0 && (2..EOC).contains(&cluster) && guard < v.total_clusters() {
            if contiguous > 1 {
                let step = skip.min(contiguous - 1);
                cluster += u32::try_from(step).expect("extent skip");
                index += step;
                skip -= step;
                contiguous -= step;
            } else {
                cluster = s::fat_entry(v, cluster)?;
                STATS.cluster_walk_steps.fetch_add(1, Relaxed);
                index += 1;
                skip -= 1;
                contiguous = 1;
                s::cooperate(&mut steps);
            }
            guard += 1;
        }
        if skip != 0 || cluster < 2 || cluster >= s::alloc_end(v) {
            return None;
        }
        if contiguous <= 1 {
            contiguous = extent_from(v, e, total, index, cluster, &mut steps);
        }
        let mut inside = offset % size;
        let mut copied = 0;
        let mut data = [0; SECTOR];
        guard = 0;
        while copied < wanted
            && cluster >= 2
            && cluster < s::alloc_end(v)
            && guard < v.total_clusters()
        {
            let mut sector = u8::try_from(inside / SECTOR).expect("sector index");
            let mut sector_offset = inside % SECTOR;
            while sector < v.sectors_per_cluster && copied < wanted {
                if sector_offset == 0 && wanted - copied >= SECTOR {
                    let left = usize::from(v.sectors_per_cluster - sector);
                    let full = left.min((wanted - copied) / SECTOR);
                    if !s::read(
                        v.device_index,
                        u64::from(v.cluster_lba(cluster) + u32::from(sector)),
                        u16::try_from(full).expect("read sectors"),
                        out.sub(copied, full * SECTOR),
                    ) {
                        return None;
                    }
                    copied += full * SECTOR;
                    sector += u8::try_from(full).expect("read sectors");
                    s::cooperate(&mut steps);
                    continue;
                }
                if !s::read(
                    v.device_index,
                    u64::from(v.cluster_lba(cluster) + u32::from(sector)),
                    1,
                    Buffer::of(&mut data),
                ) {
                    return None;
                }
                let count = (SECTOR - sector_offset).min(wanted - copied);
                Bytes::of(&data[sector_offset..sector_offset + count]).copy(out.data.add(copied));
                copied += count;
                sector_offset = 0;
                sector += 1;
                s::cooperate(&mut steps);
            }
            if copied >= wanted {
                break;
            }
            if contiguous > 1 {
                cluster += 1;
                index += 1;
                contiguous -= 1;
            } else {
                cluster = s::fat_entry(v, cluster)?;
                STATS.cluster_walk_steps.fetch_add(1, Relaxed);
                index += 1;
                contiguous = extent_from(v, e, total, index, cluster, &mut steps);
            }
            inside = 0;
            guard += 1;
        }
        if copied != wanted {
            return None;
        }
        op.ok = true;
        Some(copied)
    }
}
#[inline(never)]
pub(super) unsafe fn write_cluster_data(v: Volume, start: u32, data: Bytes) -> bool {
    unsafe {
        let mut cluster = start;
        let mut written = 0;
        let mut sector = [0; SECTOR];
        let mut steps = 0;
        while (2..EOC).contains(&cluster) {
            for i in 0..v.sectors_per_cluster {
                sector.fill(0);
                if written < data.len {
                    let count = (data.len - written).min(SECTOR);
                    data.sub(written, count).copy(sector.as_mut_ptr());
                    written += count;
                }
                if !s::write(
                    v,
                    u64::from(v.cluster_lba(cluster) + u32::from(i)),
                    1,
                    Bytes::of(&sector),
                ) {
                    return false;
                }
                s::cooperate(&mut steps);
            }
            if written >= data.len {
                return true;
            }
            let Some(next) = s::fat_entry(v, cluster) else {
                return false;
            };
            if next >= EOC {
                return written >= data.len;
            }
            STATS.cluster_walk_steps.fetch_add(1, Relaxed);
            cluster = next;
        }
        written >= data.len
    }
}
#[inline(never)]
pub(super) unsafe fn copy_clusters(src: Volume, dst: Volume, e: Entry, dest: u32) -> bool {
    unsafe {
        if e.size == 0 {
            return true;
        }
        if e.first_cluster < 2 || dest < 2 {
            return false;
        }
        let mut sc = e.first_cluster;
        let mut dc = dest;
        let mut si = 0u8;
        let mut di = 0u8;
        let mut remaining = e.size as usize;
        let mut sector = [0; SECTOR];
        let max = remaining.div_ceil(SECTOR) + 8;
        let mut count = 0;
        let mut steps = 0;
        while remaining > 0 && count < max {
            if !s::read(
                src.device_index,
                u64::from(src.cluster_lba(sc) + u32::from(si)),
                1,
                Buffer::of(&mut sector),
            ) {
                return false;
            }
            let n = remaining.min(SECTOR);
            if n < SECTOR {
                sector[n..].fill(0);
            }
            if !s::write(
                dst,
                u64::from(dst.cluster_lba(dc) + u32::from(di)),
                1,
                Bytes::of(&sector),
            ) {
                return false;
            }
            remaining -= n;
            s::cooperate(&mut steps);
            if remaining == 0 {
                return true;
            }
            si += 1;
            if si >= src.sectors_per_cluster {
                si = 0;
                let Some(next) = s::fat_entry(src, sc) else {
                    return false;
                };
                if next >= EOC {
                    return false;
                }
                STATS.cluster_walk_steps.fetch_add(1, Relaxed);
                sc = next;
            }
            di += 1;
            if di >= dst.sectors_per_cluster {
                di = 0;
                let Some(next) = s::fat_entry(dst, dc) else {
                    return false;
                };
                if next >= EOC && remaining > 0 {
                    return false;
                }
                STATS.cluster_walk_steps.fetch_add(1, Relaxed);
                dc = next;
            }
            count += 1;
        }
        remaining == 0
    }
}
#[inline(never)]
pub(super) unsafe fn write_from(v: Volume, start: u32, in_cluster: usize, data: Bytes) -> bool {
    unsafe { write_spans(v, start, in_cluster, data) }
}
unsafe fn write_spans(v: Volume, start: u32, offset: usize, data: Bytes) -> bool {
    unsafe {
        let size = v.cluster_bytes() as usize;
        if offset >= size {
            return false;
        }
        let mut cluster = start;
        let mut inside = offset;
        let mut written = 0;
        let mut sector = [0; SECTOR];
        let mut guard = 0;
        let mut steps = 0;
        while written < data.len
            && cluster >= 2
            && cluster < s::alloc_end(v)
            && guard < v.total_clusters()
        {
            let mut si = u8::try_from(inside / SECTOR).expect("sector index");
            let mut so = inside % SECTOR;
            while si < v.sectors_per_cluster && written < data.len {
                let lba = v.cluster_lba(cluster) + u32::from(si);
                if so == 0 {
                    let full = ((data.len - written) / SECTOR)
                        .min(usize::from(v.sectors_per_cluster - si));
                    if full > 0 {
                        let bytes = full * SECTOR;
                        if !s::write(
                            v,
                            u64::from(lba),
                            u16::try_from(full).expect("write sectors"),
                            data.sub(written, bytes),
                        ) {
                            return false;
                        }
                        written += bytes;
                        si += u8::try_from(full).expect("write sectors");
                        s::cooperate(&mut steps);
                        continue;
                    }
                }
                let count = (SECTOR - so).min(data.len - written);
                if so == 0 && count == SECTOR {
                    if !s::write(v, u64::from(lba), 1, data.sub(written, SECTOR)) {
                        return false;
                    }
                } else {
                    if !s::read(v.device_index, u64::from(lba), 1, Buffer::of(&mut sector)) {
                        return false;
                    }
                    data.sub(written, count).copy(sector.as_mut_ptr().add(so));
                    if !s::write(v, u64::from(lba), 1, Bytes::of(&sector)) {
                        return false;
                    }
                }
                written += count;
                so = 0;
                si += 1;
                s::cooperate(&mut steps);
            }
            if written >= data.len {
                return true;
            }
            let Some(next) = s::fat_entry(v, cluster) else {
                return false;
            };
            cluster = next;
            STATS.cluster_walk_steps.fetch_add(1, Relaxed);
            inside = 0;
            guard += 1;
        }
        written >= data.len
    }
}
#[inline(never)]
pub(super) unsafe fn write_range(v: Volume, e: Entry, offset: usize, data: Bytes) -> Option<usize> {
    unsafe {
        let mut op = s::Operation::begin(5);
        let v = s::mutation(v)?;
        if e.is_dir() || e.read_only() || offset > e.size as usize {
            return None;
        }
        if data.len == 0 {
            op.ok = true;
            return Some(0);
        }
        s::invalidate_extents();
        if data.len > e.size as usize - offset || e.first_cluster < 2 {
            return None;
        }
        let size = v.cluster_bytes() as usize;
        let first = chain::nth(v, e.first_cluster, offset / size)?;
        if !write_spans(v, first, offset % size, data) || !s::flush_mutation(v) {
            return None;
        }
        STATS.file_write_ranges.fetch_add(1, Relaxed);
        STATS.file_write_bytes.fetch_add(data.len as u64, Relaxed);
        op.ok = true;
        Some(data.len)
    }
}
unsafe fn append_matches(v: Volume, parent: u32, name: Bytes) -> bool {
    unsafe {
        let a = s::append();
        if !(*a).valid
            || (*a).device != v.device_index
            || (*a).partition != v.partition_lba
            || (*a).parent != parent
            || (*a).name_len != name.len
            || name.len > NAME_MAX
        {
            return false;
        }
        for i in 0..name.len {
            if (*a).name[i] != name.get(i) {
                return false;
            }
        }
        true
    }
}
#[inline(never)]
pub(super) unsafe fn append_location(
    v: Volume,
    parent: u32,
    name: Bytes,
    expected: Option<u32>,
) -> Option<Location> {
    unsafe {
        if !append_matches(v, parent, name) {
            return None;
        }
        let a = s::append().read();
        if expected.is_some_and(|size| a.size != size) {
            return None;
        }
        let mut e = Entry {
            name_len: a.name_len,
            attr: a.attr,
            first_cluster: a.first,
            size: a.size,
            ..Entry::EMPTY
        };
        e.name[..e.name_len].copy_from_slice(&a.name[..e.name_len]);
        Some(Location {
            lba: a.lba,
            offset: a.offset,
            entry: e,
            ..Location::EMPTY
        })
    }
}
#[inline(never)]
pub(super) unsafe fn append_last(v: Volume, parent: u32, name: Bytes, size: u32) -> Option<u32> {
    unsafe {
        if !append_matches(v, parent, name) {
            return None;
        }
        let a = s::append().read();
        if a.size != size || a.last < 2 {
            None
        } else {
            Some(a.last)
        }
    }
}
#[inline(never)]
pub(super) unsafe fn remember_append(
    v: Volume,
    parent: u32,
    name: Bytes,
    loc: Location,
    first: u32,
    size: u32,
    last: u32,
) {
    unsafe {
        let a = s::append();
        a.write(s::AppendCache {
            valid: true,
            device: v.device_index,
            partition: v.partition_lba,
            parent,
            name_len: name.len.min(NAME_MAX),
            lba: loc.lba,
            offset: loc.offset,
            attr: loc.entry.attr,
            first,
            size,
            last,
            ..s::AppendCache::EMPTY
        });
        if (*a).name_len > 0 {
            name.sub(0, (*a).name_len)
                .copy(core::ptr::addr_of_mut!((*a).name).cast::<u8>());
        }
    }
}
#[inline(never)]
pub(super) unsafe fn print_file(v: Volume, e: Entry, max: usize) -> bool {
    unsafe {
        if e.is_dir() {
            return false;
        }
        let mut cluster = e.first_cluster;
        let mut remaining = e.size as usize;
        let mut printed = 0;
        let mut sector = [0; SECTOR];
        let mut guard = 0;
        let mut steps = 0;
        while (2..EOC).contains(&cluster) && remaining > 0 && printed < max && guard < 4096 {
            for i in 0..v.sectors_per_cluster {
                if remaining == 0 || printed >= max {
                    break;
                }
                if !s::read(
                    v.device_index,
                    u64::from(v.cluster_lba(cluster) + u32::from(i)),
                    1,
                    Buffer::of(&mut sector),
                ) {
                    return false;
                }
                let count = SECTOR.min(remaining).min(max - printed);
                for &c in &sector[..count] {
                    match c {
                        b'\n' => s::log(b"\r\n"),
                        b'\r' => {}
                        b'\t' => s::character(c),
                        _ => s::character(if (0x20..=0x7e).contains(&c) { c } else { b'.' }),
                    }
                }
                remaining -= count;
                printed += count;
                s::cooperate(&mut steps);
            }
            if remaining == 0 || printed >= max {
                break;
            }
            let Some(next) = s::fat_entry(v, cluster) else {
                return false;
            };
            if next >= EOC {
                break;
            }
            STATS.cluster_walk_steps.fetch_add(1, Relaxed);
            cluster = next;
            guard += 1;
        }
        if printed >= max && remaining > 0 {
            s::log(b"\r\n[output truncated]\r\n");
        }
        true
    }
}
