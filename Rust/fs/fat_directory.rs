//! Original validated directory traversal, physical cursors and namespace I/O.
use super::{
    fat_chain::{self as chain, DirChain},
    fat_names as n, fat_state as s,
    fat_types::*,
};
use core::{ffi::c_void, mem::MaybeUninit, ptr, sync::atomic::Ordering::Relaxed};
use s::STATS;
#[derive(PartialEq, Eq)]
enum ClusterLookup {
    Found,
    Next,
    End,
    Io,
}
unsafe fn in_cluster(
    v: Volume,
    cluster: u32,
    name: Bytes,
    out: *mut Entry,
    lfn: &mut n::Lfn,
) -> ClusterLookup {
    unsafe {
        STATS.dir_scans.fetch_add(1, Relaxed);
        let mut data = [0; SECTOR];
        let mut steps = 0;
        for sector in 0..v.sectors_per_cluster {
            let lba = v.cluster_lba(cluster) + u32::from(sector);
            if !s::read(v.device_index, u64::from(lba), 1, Buffer::of(&mut data)) {
                return ClusterLookup::Io;
            }
            for off in (0..SECTOR).step_by(32) {
                STATS.dir_entries_scanned.fetch_add(1, Relaxed);
                s::cooperate(&mut steps);
                let raw = &data[off..off + 32];
                if raw[0] == 0 {
                    lfn.reset();
                    return ClusterLookup::End;
                }
                if raw[0] == 0xe5 {
                    lfn.reset();
                    continue;
                }
                if raw[11] == 15 {
                    if !lfn.consume(raw) {
                        return ClusterLookup::Io;
                    }
                    continue;
                }
                if raw[11] & 15 == 15 {
                    return ClusterLookup::Io;
                }
                if raw[0] == b'.' || raw[11] & 8 != 0 {
                    if lfn.active {
                        return ClusterLookup::Io;
                    }
                    continue;
                }
                if lfn.active && !lfn.complete(raw) {
                    return ClusterLookup::Io;
                }
                let found = n::make_entry(raw, lfn, u64::from(lba), off as u16);
                if !s::entry_cluster_valid(v, found) {
                    return ClusterLookup::Io;
                }
                lfn.reset();
                if n::equal_ascii(&found.name[..found.name_len], name.view())
                    || n::short_equals(&raw[..11], name.view())
                {
                    out.write(found);
                    return ClusterLookup::Found;
                }
            }
        }
        ClusterLookup::Next
    }
}
#[inline(never)]
pub(super) unsafe fn find(v: Volume, start: u32, name: Bytes, out: *mut Entry) -> Lookup {
    unsafe {
        if !n::valid(name.view()) {
            return Lookup::Io;
        }
        let mut chain = DirChain::new(v, start);
        let mut lfn = n::Lfn::EMPTY;
        while let Some(c) = chain.next() {
            if chain.error {
                return Lookup::Io;
            }
            match in_cluster(v, c, name, out, &mut lfn) {
                ClusterLookup::Found => return Lookup::Found,
                ClusterLookup::Next => {}
                ClusterLookup::End => return Lookup::NotFound,
                ClusterLookup::Io => return Lookup::Io,
            }
        }
        if chain.error || lfn.active {
            Lookup::Io
        } else {
            Lookup::NotFound
        }
    }
}
#[inline(never)]
pub(super) unsafe fn find_location(
    v: Volume,
    start: u32,
    name: Bytes,
    out: *mut Location,
) -> Lookup {
    unsafe {
        if !n::valid(name.view()) {
            return Lookup::Io;
        }
        STATS.dir_scans.fetch_add(1, Relaxed);
        let mut steps = 0;
        let mut chain = DirChain::new(v, start);
        let mut lfn = n::Lfn::EMPTY;
        let mut slots = [Slot::default(); LFN_MAX];
        let mut count = 0;
        while let Some(c) = chain.next() {
            if chain.error {
                return Lookup::Io;
            }
            let mut data = [0; SECTOR];
            for sector in 0..v.sectors_per_cluster {
                let lba = v.cluster_lba(c) + u32::from(sector);
                if !s::read(v.device_index, u64::from(lba), 1, Buffer::of(&mut data)) {
                    return Lookup::Io;
                }
                for off in (0..SECTOR).step_by(32) {
                    STATS.dir_entries_scanned.fetch_add(1, Relaxed);
                    s::cooperate(&mut steps);
                    let raw = &data[off..off + 32];
                    if raw[0] == 0 {
                        return Lookup::NotFound;
                    }
                    if raw[0] == 0xe5 {
                        lfn.reset();
                        count = 0;
                        continue;
                    }
                    if raw[11] == 15 {
                        if !lfn.consume(raw) || count >= LFN_MAX {
                            return Lookup::Io;
                        }
                        slots[count] = Slot { lba, offset: off };
                        count += 1;
                        continue;
                    }
                    if raw[11] & 15 == 15 {
                        return Lookup::Io;
                    }
                    if raw[0] == b'.' || raw[11] & 8 != 0 {
                        if lfn.active {
                            return Lookup::Io;
                        }
                        continue;
                    }
                    if lfn.active && (!lfn.complete(raw) || count != usize::from(lfn.total)) {
                        return Lookup::Io;
                    }
                    let found = n::make_entry(raw, &lfn, u64::from(lba), off as u16);
                    if !s::entry_cluster_valid(v, found) {
                        return Lookup::Io;
                    }
                    if n::equal_ascii(&found.name[..found.name_len], name.view())
                        || n::short_equals(&raw[..11], name.view())
                    {
                        let mut loc = Location {
                            lba,
                            offset: off,
                            entry: found,
                            lfn: [Slot::default(); LFN_MAX],
                            lfn_count: count,
                        };
                        loc.lfn[..count].copy_from_slice(&slots[..count]);
                        out.write(loc);
                        return Lookup::Found;
                    }
                    lfn.reset();
                    count = 0;
                }
            }
        }
        if chain.error || lfn.active {
            Lookup::Io
        } else {
            Lookup::NotFound
        }
    }
}
#[inline(never)]
pub(super) unsafe fn optional_location(
    v: Volume,
    start: u32,
    name: Bytes,
) -> Result<Option<Location>, ()> {
    unsafe {
        let mut out = MaybeUninit::uninit();
        match find_location(v, start, name, out.as_mut_ptr()) {
            Lookup::Found => Ok(Some(out.assume_init())),
            Lookup::NotFound => Ok(None),
            Lookup::Io => Err(()),
        }
    }
}
#[inline(never)]
pub(super) unsafe fn resolve(v: Volume, path: Bytes, out: *mut Entry) -> Lookup {
    unsafe {
        if !s::valid_cluster(v, v.root_cluster) {
            return Lookup::Io;
        }
        let mut cluster = v.root_cluster;
        let mut start = 0;
        let mut result = None;
        while start < path.len {
            while start < path.len && matches!(path.get(start), b'\\' | b'/') {
                start += 1;
            }
            if start >= path.len {
                break;
            }
            let mut end = start;
            while end < path.len && !matches!(path.get(end), b'\\' | b'/') {
                end += 1;
            }
            let segment = path.sub(start, end - start);
            let mut e = MaybeUninit::uninit();
            match find(v, cluster, segment, e.as_mut_ptr()) {
                Lookup::Found => {}
                Lookup::NotFound => return Lookup::NotFound,
                Lookup::Io => return Lookup::Io,
            }
            let e = e.assume_init();
            result = Some(e);
            start = end;
            if start < path.len {
                if !e.is_dir() {
                    return Lookup::NotFound;
                }
                cluster = e.first_cluster;
            }
        }
        if let Some(e) = result {
            out.write(e);
            Lookup::Found
        } else {
            Lookup::NotFound
        }
    }
}
#[inline(never)]
pub(super) unsafe fn resolve_path(v: Volume, path: Bytes, out: *mut u32) -> Lookup {
    unsafe {
        if !s::valid_cluster(v, v.root_cluster) {
            return Lookup::Io;
        }
        let mut cluster = v.root_cluster;
        let mut start = 0;
        while start < path.len {
            while start < path.len && matches!(path.get(start), b'\\' | b'/') {
                start += 1;
            }
            if start >= path.len {
                break;
            }
            let mut end = start;
            while end < path.len && !matches!(path.get(end), b'\\' | b'/') {
                end += 1;
            }
            let segment = path.sub(start, end - start);
            if segment.len != 0 {
                let mut e = MaybeUninit::uninit();
                match find(v, cluster, segment, e.as_mut_ptr()) {
                    Lookup::Found => {
                        let e = e.assume_init();
                        if !e.is_dir() {
                            return Lookup::NotFound;
                        }
                        cluster = e.first_cluster;
                    }
                    Lookup::NotFound => return Lookup::NotFound,
                    Lookup::Io => return Lookup::Io,
                }
            }
            start = end;
        }
        out.write(cluster);
        Lookup::Found
    }
}
pub fn print_entry(e: Entry) {
    s::log(b"        ");
    s::log(&e.name[..e.name_len]);
    if e.is_dir() {
        s::log(b" <DIR>");
    } else {
        s::log(b" ");
        s::decimal(u64::from(e.size));
        s::log(b" bytes");
    }
    s::log(b" cluster=");
    s::decimal(u64::from(e.first_cluster));
    s::log(b"\r\n");
}
unsafe fn append(out: Buffer, cursor: &mut usize, bytes: &[u8]) -> bool {
    if *cursor + bytes.len() >= out.len {
        return false;
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out.data.add(*cursor), bytes.len());
    }
    *cursor += bytes.len();
    true
}
unsafe fn entry_line(out: Buffer, cursor: &mut usize, e: Entry) -> bool {
    unsafe {
        append(out, cursor, if e.is_dir() { b"<DIR> " } else { b"      " })
            && append(out, cursor, &e.name[..e.name_len])
            && append(out, cursor, b"\r\n")
    }
}
// Preserve the distinct original filters: diagnostic listing includes dot
// names, text/ordinal enumeration omits them. None uses lookup's stricter
// data-cluster validation as a silent change to the enumeration contract.
#[allow(clippy::too_many_arguments)] // Distinct listing/ordinal/output contracts stay explicit.
unsafe fn enumerate(
    v: Volume,
    start: u32,
    mode: u8,
    wanted: usize,
    out: Buffer,
    entry_out: *mut Entry,
    max: usize,
    cursor: &mut usize,
) -> Lookup {
    unsafe {
        let mut seen = 0;
        let mut lfn = n::Lfn::EMPTY;
        let mut chain = DirChain::new(v, start);
        while let Some(c) = chain.next() {
            if chain.error {
                return Lookup::Io;
            }
            STATS.dir_scans.fetch_add(1, Relaxed);
            let mut data = [0; SECTOR];
            let mut steps = 0;
            for sector in 0..v.sectors_per_cluster {
                let lba = v.cluster_lba(c) + u32::from(sector);
                if !s::read(v.device_index, u64::from(lba), 1, Buffer::of(&mut data)) {
                    return Lookup::Io;
                }
                for off in (0..SECTOR).step_by(32) {
                    STATS.dir_entries_scanned.fetch_add(1, Relaxed);
                    s::cooperate(&mut steps);
                    let raw = &data[off..off + 32];
                    if raw[0] == 0 {
                        lfn.reset();
                        return Lookup::NotFound;
                    }
                    if raw[0] == 0xe5 {
                        lfn.reset();
                        continue;
                    }
                    if raw[11] == 15 {
                        if !lfn.consume(raw) {
                            return Lookup::Io;
                        }
                        continue;
                    }
                    if raw[11] & 8 != 0 || (mode != 0 && raw[0] == b'.') {
                        if lfn.active {
                            return Lookup::Io;
                        }
                        continue;
                    }
                    if lfn.active && !lfn.complete(raw) {
                        return Lookup::Io;
                    }
                    let e = n::make_entry(raw, &lfn, u64::from(lba), off as u16);
                    lfn.reset();
                    match mode {
                        0 => print_entry(e),
                        1 => {
                            if !entry_line(out, cursor, e) {
                                return Lookup::Io;
                            }
                        }
                        2 => {
                            if seen == wanted {
                                if e.name_len + 1 > out.len {
                                    return Lookup::Io;
                                }
                                ptr::copy_nonoverlapping(e.name.as_ptr(), out.data, e.name_len);
                                out.put(e.name_len, 0);
                                entry_out.write(e);
                                return Lookup::Found;
                            }
                        }
                        _ => unreachable!(),
                    }
                    seen += 1;
                    if mode != 2 && seen >= max {
                        return Lookup::Found;
                    }
                }
            }
        }
        if chain.error || lfn.active {
            Lookup::Io
        } else {
            Lookup::NotFound
        }
    }
}
#[inline(never)]
pub(super) unsafe fn list(v: Volume, start: u32, max: usize) -> bool {
    unsafe {
        enumerate(
            v,
            start,
            0,
            0,
            Buffer::default(),
            ptr::null_mut(),
            max,
            &mut 0,
        ) != Lookup::Io
    }
}
#[inline(never)]
pub(super) unsafe fn read_directory(v: Volume, start: u32, out: Buffer) -> Option<usize> {
    unsafe {
        let mut cursor = 0;
        if !append(out, &mut cursor, b".\r\n") || !append(out, &mut cursor, b"..\r\n") {
            return None;
        }
        if enumerate(v, start, 1, 0, out, ptr::null_mut(), 64, &mut cursor) == Lookup::Io {
            return None;
        }
        if cursor < out.len {
            out.put(cursor, 0);
        }
        Some(cursor)
    }
}
#[inline(never)]
pub(super) unsafe fn read_entry(
    v: Volume,
    start: u32,
    index: usize,
    out: Buffer,
    entry_out: *mut Entry,
) -> Lookup {
    unsafe {
        if index < 2 {
            if out.len < 3 {
                return Lookup::Io;
            }
            let mut e = Entry {
                name_len: index + 1,
                attr: 0x10,
                first_cluster: start,
                ..Entry::EMPTY
            };
            e.name[0] = b'.';
            out.put(0, b'.');
            if index == 1 {
                e.name[1] = b'.';
                out.put(1, b'.');
            }
            out.put(e.name_len, 0);
            entry_out.write(e);
            return Lookup::Found;
        }
        enumerate(v, start, 2, index - 2, out, entry_out, 0, &mut 0)
    }
}
struct CursorLfn {
    cursor: *mut DirectoryCursor,
    lfn: n::Lfn,
}
impl Drop for CursorLfn {
    fn drop(&mut self) {
        unsafe {
            let c = self.cursor;
            ptr::copy_nonoverlapping(
                self.lfn.units.as_ptr(),
                ptr::addr_of_mut!((*c).units).cast::<u16>(),
                UNITS,
            );
            (*c).lfn_len = self.lfn.len as u32;
            (*c).lfn_active = u8::from(self.lfn.active);
            (*c).lfn_total = self.lfn.total;
            (*c).lfn_expected = self.lfn.expected;
            (*c).lfn_checksum = self.lfn.checksum;
        }
    }
}
#[inline(never)]
pub(super) unsafe fn next(
    v: Volume,
    start: u32,
    cursor: *mut DirectoryCursor,
    out: *mut Entry,
) -> Next {
    unsafe {
        let c = cursor;
        if (*c).state > 2
            || (*c).lfn_active > 1
            || (*c).lfn_len > UNITS as u32
            || (*c).lfn_total > LFN_MAX as u8
            || (*c).lfn_expected > (*c).lfn_total
        {
            return Next::Io;
        }
        if (*c).state == 2 {
            return Next::NotFound;
        }
        if (*c).state == 0 {
            c.write(DirectoryCursor {
                cluster: start,
                state: 1,
                ..DirectoryCursor::EMPTY
            });
        }
        let bytes = u32::from(v.sectors_per_cluster) * SECTOR as u32;
        if (*c).offset > bytes || !(*c).offset.is_multiple_of(32) {
            return Next::Io;
        }
        let mut guard = CursorLfn {
            cursor: c,
            lfn: n::Lfn {
                units: (*c).units,
                len: (*c).lfn_len as usize,
                active: (*c).lfn_active != 0,
                total: (*c).lfn_total,
                expected: (*c).lfn_expected,
                checksum: (*c).lfn_checksum,
            },
        };
        let mut examined = 0;
        let mut data = [0; SECTOR];
        while examined < 256 {
            if (*c).offset == bytes && bytes != 0 {
                (*c).cluster = (*c).successor;
                (*c).offset = 0;
            }
            if is_eoc((*c).cluster) {
                if guard.lfn.active {
                    return Next::Io;
                }
                (*c).state = 2;
                return Next::NotFound;
            }
            if !s::valid_cluster(v, (*c).cluster) {
                return Next::Io;
            }
            if (*c).offset == 0 {
                if (*c).steps >= DIR_LIMIT as u32 {
                    return Next::Io;
                }
                let Some(successor) = s::fat_entry(v, (*c).cluster) else {
                    return Next::Io;
                };
                (*c).successor = successor;
                if !is_eoc(successor) && !s::valid_cluster(v, successor) {
                    return Next::Io;
                }
                (*c).steps += 1;
                STATS.dir_scans.fetch_add(1, Relaxed);
            }
            let lba = v.cluster_lba((*c).cluster) + (*c).offset / SECTOR as u32;
            if !s::read(v.device_index, u64::from(lba), 1, Buffer::of(&mut data)) {
                return Next::Io;
            }
            let mut off = (*c).offset as usize % SECTOR;
            while off < SECTOR && examined < 256 {
                examined += 1;
                STATS.dir_entries_scanned.fetch_add(1, Relaxed);
                (*c).offset += 32;
                let raw = &data[off..off + 32];
                off += 32;
                if raw[0] == 0 {
                    guard.lfn.reset();
                    (*c).state = 2;
                    return Next::NotFound;
                }
                if raw[0] == 0xe5 {
                    guard.lfn.reset();
                    continue;
                }
                if raw[11] == 15 {
                    if !guard.lfn.consume(raw) {
                        return Next::Io;
                    }
                    continue;
                }
                if raw[0] == b'.' || raw[11] & 8 != 0 {
                    if guard.lfn.active {
                        return Next::Io;
                    }
                    continue;
                }
                if guard.lfn.active && !guard.lfn.complete(raw) {
                    return Next::Io;
                }
                out.write(n::make_entry(
                    raw,
                    &guard.lfn,
                    u64::from(lba),
                    (off - 32) as u16,
                ));
                guard.lfn.reset();
                return Next::Found;
            }
        }
        Next::Again
    }
}
#[inline(never)]
pub(super) unsafe fn raw_short_used(v: Volume, start: u32, short: [u8; 11]) -> bool {
    unsafe {
        STATS.dir_scans.fetch_add(1, Relaxed);
        let mut steps = 0;
        let mut chain = DirChain::new(v, start);
        while let Some(c) = chain.next() {
            if chain.error {
                return true;
            }
            let mut data = [0; SECTOR];
            for sector in 0..v.sectors_per_cluster {
                if !s::read(
                    v.device_index,
                    u64::from(v.cluster_lba(c) + u32::from(sector)),
                    1,
                    Buffer::of(&mut data),
                ) {
                    return true;
                }
                for off in (0..SECTOR).step_by(32) {
                    STATS.dir_entries_scanned.fetch_add(1, Relaxed);
                    s::cooperate(&mut steps);
                    let raw = &data[off..off + 32];
                    if raw[0] == 0 {
                        return false;
                    }
                    if raw[0] == 0xe5 || raw[11] == 15 {
                        continue;
                    }
                    if raw[..11] == short {
                        return true;
                    }
                }
            }
        }
        chain.error || chain.steps >= DIR_LIMIT
    }
}
unsafe fn unique_alias(v: Volume, parent: u32, name: Bytes, out: &mut [u8; 11]) -> bool {
    unsafe {
        if !n::valid_long(name.view()) {
            return false;
        }
        for suffix in 1..1000 {
            if !n::short_alias(name.view(), suffix, out) {
                return false;
            }
            if !raw_short_used(v, parent, *out) {
                return true;
            }
        }
        false
    }
}
#[inline(never)]
pub(super) unsafe fn build_entries(
    v: Volume,
    parent: u32,
    name: Bytes,
    attr: u8,
    first: u32,
    size: u32,
    out: &mut [u8],
) -> Option<usize> {
    unsafe {
        if out.len() < SLOT_MAX * 32 {
            return None;
        }
        let mut short = [0; 11];
        let short_only = n::build_short(name.view(), &mut short);
        if !short_only && !unique_alias(v, parent, name, &mut short) {
            return None;
        }
        let mut offset = 0;
        if !short_only {
            let mut units = [0; UNITS];
            let count = n::utf8_units(name.view(), &mut units)?;
            let sum = n::checksum(short);
            let lfn_count = count.div_ceil(13);
            if lfn_count == 0 || lfn_count > LFN_MAX {
                return None;
            }
            for seq in (1..=lfn_count).rev() {
                n::write_lfn(
                    &mut out[offset..offset + 32],
                    &units[..count],
                    seq,
                    lfn_count,
                    sum,
                );
                offset += 32;
            }
        }
        let mut raw = [0; 32];
        raw[..11].copy_from_slice(&short);
        raw[11] = attr;
        s::stamp(&mut raw);
        put_cluster(&mut raw, first);
        put32(&mut raw[28..32], size);
        out[offset..offset + 32].copy_from_slice(&raw);
        Some(offset + 32)
    }
}
#[inline(never)]
pub(super) unsafe fn mark_deleted(v: Volume, slot: Slot) -> bool {
    unsafe {
        let mut data = [0; SECTOR];
        if !s::read(
            v.device_index,
            u64::from(slot.lba),
            1,
            Buffer::of(&mut data),
        ) {
            return false;
        }
        data[slot.offset] = 0xe5;
        if !s::write(v, u64::from(slot.lba), 1, Bytes::of(&data)) {
            return false;
        }
        STATS.dir_entry_updates.fetch_add(1, Relaxed);
        true
    }
}
#[inline(never)]
pub(super) unsafe fn detach(v: Volume, loc: Location) -> bool {
    unsafe {
        if !mark_deleted(
            v,
            Slot {
                lba: loc.lba,
                offset: loc.offset,
            },
        ) {
            return false;
        }
        if loc.lfn_count == 0 {
            return true;
        }
        if !s::flush_volume(v) {
            return false;
        }
        for i in (0..loc.lfn_count).rev() {
            if !mark_deleted(v, loc.lfn[i]) || !s::flush_volume(v) {
                return false;
            }
        }
        true
    }
}
#[inline(never)]
pub(super) unsafe fn delete_at(v: Volume, loc: Location) -> bool {
    unsafe {
        if !detach(v, loc) || !s::flush_volume(v) {
            return false;
        }
        loc.entry.first_cluster == 0 || chain::free(v, loc.entry.first_cluster)
    }
}
struct LfnGuard {
    lfn: n::Lfn,
    slots: [Slot; LFN_MAX],
    count: usize,
}
impl LfnGuard {
    fn new() -> Self {
        Self {
            lfn: n::Lfn::EMPTY,
            slots: [Slot::default(); LFN_MAX],
            count: 0,
        }
    }
    unsafe fn visit(&mut self, v: Volume, raw: &[u8], slot: Slot) -> bool {
        unsafe {
            if raw[0] == 0xe5 || raw[0] == 0 {
                while self.count > 0 {
                    self.count -= 1;
                    if !mark_deleted(v, self.slots[self.count]) || !s::flush_volume(v) {
                        return false;
                    }
                }
                self.lfn.reset();
                return true;
            }
            if raw[11] == 15 {
                if !self.lfn.consume(raw) || self.count == LFN_MAX {
                    return false;
                }
                self.slots[self.count] = slot;
                self.count += 1;
                return true;
            }
            if self.lfn.active && !self.lfn.complete(raw) {
                return false;
            }
            self.lfn.reset();
            self.count = 0;
            true
        }
    }
}
#[derive(Clone, Copy)]
struct Growth {
    volume: Volume,
    tail: u32,
}
unsafe extern "C" fn grow_allocate(ctx: *mut c_void, count: usize, out: *mut u32) -> bool {
    unsafe {
        let g = ctx.cast::<Growth>().read();
        let Some(a) = chain::prefer(g.volume, count, g.tail + 1) else {
            return false;
        };
        out.write(a.first);
        true
    }
}
unsafe extern "C" fn grow_initialize(ctx: *mut c_void, first: u32, count: usize) -> bool {
    unsafe {
        let g = ctx.cast::<Growth>().read();
        let mut cluster = first;
        let zero = [0u8; SECTOR];
        let mut steps = 0;
        for i in 0..count {
            if !s::valid_cluster(g.volume, cluster) {
                return false;
            }
            for sector in 0..g.volume.sectors_per_cluster {
                if !s::write(
                    g.volume,
                    u64::from(g.volume.cluster_lba(cluster) + u32::from(sector)),
                    1,
                    Bytes::of(&zero),
                ) {
                    return false;
                }
                s::cooperate(&mut steps);
            }
            let Some(next) = s::fat_entry(g.volume, cluster) else {
                return false;
            };
            if i + 1 == count {
                return is_eoc(next);
            }
            cluster = next;
        }
        false
    }
}
unsafe extern "C" fn grow_flush(ctx: *mut c_void) -> bool {
    unsafe { s::flush_volume((*ctx.cast::<Growth>()).volume) }
}
unsafe extern "C" fn grow_tail(ctx: *mut c_void) -> bool {
    unsafe {
        let g = ctx.cast::<Growth>().read();
        s::fat_entry(g.volume, g.tail).is_some_and(is_eoc)
    }
}
unsafe extern "C" fn grow_publish(ctx: *mut c_void, first: u32) -> bool {
    unsafe {
        let g = ctx.cast::<Growth>().read();
        chain::write_fat(g.volume, g.tail, first)
    }
}
unsafe extern "C" fn grow_discard(ctx: *mut c_void, first: u32) {
    unsafe {
        let v = (*ctx.cast::<Growth>()).volume;
        chain::free(v, first);
        s::flush_volume(v);
    }
}
#[inline(never)]
pub(super) unsafe fn write_slots(v: Volume, slots: &[Slot], raw: Bytes) -> bool {
    unsafe {
        let mut data = [0; SECTOR];
        for (i, slot) in slots.iter().enumerate() {
            if !s::read(
                v.device_index,
                u64::from(slot.lba),
                1,
                Buffer::of(&mut data),
            ) {
                return false;
            }
            raw.sub(i * 32, 32).copy(data.as_mut_ptr().add(slot.offset));
            if !s::write(v, u64::from(slot.lba), 1, Bytes::of(&data)) {
                return false;
            }
        }
        STATS
            .dir_entry_updates
            .fetch_add(slots.len() as u64, Relaxed);
        true
    }
}
#[inline(never)]
pub(super) unsafe fn write_entries(v: Volume, directory: u32, raw: Bytes) -> bool {
    unsafe {
        if raw.len == 0 || !raw.len.is_multiple_of(32) {
            return false;
        }
        let needed = raw.len / 32;
        if needed > SLOT_MAX {
            return false;
        }
        STATS.dir_scans.fetch_add(1, Relaxed);
        let mut slots = [Slot::default(); SLOT_MAX];
        let mut run = 0;
        let mut tail = 0;
        let mut guard = LfnGuard::new();
        let mut steps = 0;
        let mut chain = DirChain::new(v, directory);
        while let Some(c) = chain.next() {
            if chain.error {
                return false;
            }
            tail = c;
            let mut data = [0; SECTOR];
            for sector in 0..v.sectors_per_cluster {
                let lba = v.cluster_lba(c) + u32::from(sector);
                if !s::read(v.device_index, u64::from(lba), 1, Buffer::of(&mut data)) {
                    return false;
                }
                for off in (0..SECTOR).step_by(32) {
                    STATS.dir_entries_scanned.fetch_add(1, Relaxed);
                    s::cooperate(&mut steps);
                    if !guard.visit(v, &data[off..off + 32], Slot { lba, offset: off }) {
                        return false;
                    }
                    if data[off] == 0 || data[off] == 0xe5 {
                        slots[run] = Slot { lba, offset: off };
                        run += 1;
                        if run == needed {
                            return write_slots(v, &slots[..needed], raw);
                        }
                    } else {
                        run = 0;
                    }
                }
            }
        }
        if chain.error || !is_eoc(chain.cluster) || guard.count != 0 || tail == 0 {
            return false;
        }
        let mut context = Growth { volume: v, tail };
        let io = super::directory_growth::Io {
            context: ptr::addr_of_mut!(context).cast(),
            allocate: grow_allocate,
            initialize: grow_initialize,
            flush: grow_flush,
            tail_available: grow_tail,
            publish: grow_publish,
            discard: grow_discard,
        };
        let mut extension = MaybeUninit::<super::directory_growth::Extension>::uninit();
        if super::directory_growth::r4os_rust_fs_directory_extend(
            &io,
            v.cluster_bytes() as usize,
            needed - run,
            DIR_LIMIT - chain.steps,
            extension.as_mut_ptr(),
        ) != 0
        {
            return false;
        }
        let extension = extension.assume_init();
        let mut cluster = extension.first;
        for index in 0..extension.count {
            if !s::valid_cluster(v, cluster) {
                return false;
            }
            for sector in 0..v.sectors_per_cluster {
                let lba = v.cluster_lba(cluster) + u32::from(sector);
                for off in (0..SECTOR).step_by(32) {
                    slots[run] = Slot { lba, offset: off };
                    run += 1;
                    if run == needed {
                        return write_slots(v, &slots[..needed], raw);
                    }
                }
            }
            if index + 1 < extension.count {
                let Some(next) = s::fat_entry(v, cluster) else {
                    return false;
                };
                cluster = next;
            }
        }
        false
    }
}
#[inline(never)]
pub(super) unsafe fn is_empty(v: Volume, start: u32) -> bool {
    unsafe {
        STATS.dir_scans.fetch_add(1, Relaxed);
        let mut steps = 0;
        let mut chain = DirChain::new(v, start);
        while let Some(c) = chain.next() {
            if chain.error {
                return false;
            }
            let mut data = [0; SECTOR];
            for sector in 0..v.sectors_per_cluster {
                if !s::read(
                    v.device_index,
                    u64::from(v.cluster_lba(c) + u32::from(sector)),
                    1,
                    Buffer::of(&mut data),
                ) {
                    return false;
                }
                for off in (0..SECTOR).step_by(32) {
                    STATS.dir_entries_scanned.fetch_add(1, Relaxed);
                    s::cooperate(&mut steps);
                    let raw = &data[off..off + 32];
                    if raw[0] == 0 {
                        return true;
                    }
                    if raw[0] == 0xe5 || raw[11] == 15 || raw[0] == b'.' {
                        continue;
                    }
                    return false;
                }
            }
        }
        !(chain.error || chain.steps >= DIR_LIMIT)
    }
}
#[inline(never)]
pub(super) unsafe fn init_cluster(v: Volume, cluster: u32, parent: u32) -> bool {
    unsafe {
        let mut data = [0u8; SECTOR];
        let mut dot = [b' '; 11];
        dot[0] = b'.';
        let mut dotdot = [b' '; 11];
        dotdot[0] = b'.';
        dotdot[1] = b'.';
        data[..11].copy_from_slice(&dot);
        data[11] = 0x10;
        put_cluster(&mut data[..32], cluster);
        data[32..43].copy_from_slice(&dotdot);
        data[43] = 0x10;
        put_cluster(&mut data[32..64], parent);
        let mut steps = 0;
        for sector in 0..v.sectors_per_cluster {
            if !s::write(
                v,
                u64::from(v.cluster_lba(cluster) + u32::from(sector)),
                1,
                Bytes::of(&data),
            ) {
                return false;
            }
            data.fill(0);
            s::cooperate(&mut steps);
        }
        true
    }
}
#[inline(never)]
pub(super) unsafe fn create_empty(v: Volume, parent: u32, name: Bytes) -> bool {
    unsafe {
        let mut short = [0; 11];
        if !n::build_short(name.view(), &mut short) {
            return false;
        }
        let mut raw = [0; 32];
        raw[..11].copy_from_slice(&short);
        raw[11] = 0x20;
        s::stamp(&mut raw);
        put_cluster(&mut raw, 0);
        put32(&mut raw[28..32], 0);
        write_entries(v, parent, Bytes::of(&raw))
    }
}
#[inline(never)]
pub(super) unsafe fn update_location(
    v: Volume,
    lba: u32,
    offset: usize,
    first: u32,
    size: u32,
) -> bool {
    unsafe {
        let mut data = [0; SECTOR];
        if !s::read(v.device_index, u64::from(lba), 1, Buffer::of(&mut data)) {
            return false;
        }
        let raw = &mut data[offset..offset + 32];
        put_cluster(raw, first);
        put32(&mut raw[28..32], size);
        let (date, time) = s::timestamp();
        put16(&mut raw[18..20], date);
        put16(&mut raw[22..24], time);
        put16(&mut raw[24..26], date);
        if !s::write(v, u64::from(lba), 1, Bytes::of(&data)) {
            return false;
        }
        STATS.dir_entry_updates.fetch_add(1, Relaxed);
        true
    }
}
#[inline(never)]
pub(super) unsafe fn alias(v: Volume, parent: u32, name: Bytes, source: Entry) -> bool {
    unsafe {
        if !matches!(optional_location(v, parent, name), Ok(None)) {
            return false;
        }
        let mut raw = [0; SLOT_MAX * 32];
        let Some(len) = build_entries(
            v,
            parent,
            name,
            source.attr,
            source.first_cluster,
            source.size,
            &mut raw,
        ) else {
            return false;
        };
        let off = len - 32;
        put16(&mut raw[off + 14..off + 16], source.created_time);
        put16(&mut raw[off + 16..off + 18], source.created_date);
        put16(&mut raw[off + 18..off + 20], source.access_date);
        put16(&mut raw[off + 22..off + 24], source.modified_time);
        put16(&mut raw[off + 24..off + 26], source.modified_date);
        write_entries(v, parent, Bytes::of(&raw[..len]))
    }
}
