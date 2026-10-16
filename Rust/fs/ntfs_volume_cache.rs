//! Original bounded decoded metadata cache.
//! All methods are pure per-volume operations: no I/O, callbacks or waits.
use super::{ntfs_format_names as names, ntfs_format_types::Run, ntfs_volume_types::*};
#[repr(C)]
#[derive(Clone, Copy)]
struct RecordEntry {
    valid: bool,
    generation: u64,
    last_used: u64,
    number: u64,
    byte_len: u16,
    bytes: [u8; 4096],
}
#[repr(C)]
#[derive(Clone, Copy)]
struct AttributeEntry {
    valid: bool,
    generation: u64,
    last_used: u64,
    record_number: u64,
    attr_type: u32,
    name_len: u16,
    name: [u8; 510],
    runs: [Run; 256],
    count: u16,
    data_size: u64,
    initialized_size: u64,
    alloc_size: u64,
    flags: u16,
    compression_unit: u8,
    resident: bool,
    resident_len: u16,
    resident_copy: [u8; 1024],
}
#[repr(C)]
#[derive(Clone, Copy)]
struct IndexEntry {
    valid: bool,
    generation: u64,
    last_used: u64,
    directory_record: u64,
    vcn: u64,
    byte_len: u16,
    bytes: [u8; 4096],
}
#[repr(C)]
#[derive(Clone, Copy)]
struct PathEntry {
    valid: bool,
    generation: u64,
    last_used: u64,
    directory_record: u64,
    name_len: u16,
    name_utf16: [u8; 510],
    negative: bool,
    expires_at: u64,
    result: LookupResult,
}
#[derive(Default)]
struct Counts {
    records: u32,
    attributes: u32,
    indices: u32,
    paths: u32,
}
impl Counts {
    fn total(&self) -> u32 {
        self.records + self.attributes + self.indices + self.paths
    }
}
#[repr(C)]
pub struct Cache {
    records: [RecordEntry; 8],
    attributes: [AttributeEntry; 4],
    indices: [IndexEntry; 2],
    paths: [PathEntry; 8],
    generation: u64,
    mount_generation: u64,
    negative_ttl_ticks: u64,
    access_clock: u64,
    reclaim_cursor: usize,
    counters: MetadataCacheSummary,
    free_valid: bool,
    free_count: u64,
    free_total: u64,
}
macro_rules! inc {
    ($this:expr,$field:ident,$value:expr) => {
        $this.counters.$field = $this.counters.$field.wrapping_add($value as u64)
    };
}
impl Cache {
    /// # Safety
    /// Exclusive resident aligned cache, with no live views. All contained C
    /// scalars/arrays have valid zero representations; initialize in place to
    /// keep the complete84-KB state off bounded kernel task stacks.
    pub unsafe fn initialize(out: *mut Self) {
        unsafe {
            core::ptr::write_bytes(out, 0, 1);
            (*out).generation = 1;
            core::ptr::addr_of_mut!((*out).counters).write(MetadataCacheSummary::EMPTY)
        }
    }
    pub fn free_clusters(&self, total: u64) -> Option<u64> {
        if self.free_valid && self.free_total == total {
            Some(self.free_count)
        } else {
            None
        }
    }
    pub fn store_free_clusters(&mut self, total: u64, free: u64) {
        self.free_total = total;
        self.free_count = free;
        self.free_valid = free <= total
    }
    pub fn begin_mount(&mut self, ttl: u64) {
        self.free_valid = false;
        let n = self.clear_all().total();
        self.mount_generation = next_generation(self.mount_generation);
        self.generation = next_generation(self.generation);
        self.negative_ttl_ticks = ttl;
        inc!(self, mount_invalidations, 1);
        inc!(self, invalidated_entries, n)
    }
    pub fn invalidate_external(&mut self) {
        self.free_valid = false;
        let n = self.clear_all().total();
        self.generation = next_generation(self.generation);
        inc!(self, external_invalidations, 1);
        inc!(self, invalidated_entries, n)
    }
    pub fn invalidate(&mut self, mutation: Mutation) {
        match mutation {
            Mutation::Payload => inc!(self, payload_write_retentions, 1),
            Mutation::Record(number) => {
                if number == 6 {
                    self.free_valid = false
                }
                inc!(self, targeted_invalidations, 1);
                inc!(self, targeted_record_invalidations, 1);
                let n = self.clear_record(number);
                self.note_invalidation(n, false)
            }
            Mutation::Attribute { record, kind } => {
                if record == 6 {
                    self.free_valid = false
                }
                inc!(self, targeted_invalidations, 1);
                inc!(self, targeted_attribute_invalidations, 1);
                let n = self.clear_attribute(record, kind);
                self.note_invalidation(n, false)
            }
            Mutation::Directory(number) => {
                inc!(self, targeted_invalidations, 1);
                inc!(self, targeted_directory_invalidations, 1);
                let n = self.clear_directory(number);
                self.note_invalidation(n, false)
            }
            Mutation::System => inc!(self, system_write_retentions, 1),
            Mutation::Unknown | Mutation::Recovery => {
                self.free_valid = false;
                inc!(self, global_mutation_invalidations, 1);
                if matches!(mutation, Mutation::Recovery) {
                    inc!(self, recovery_invalidations, 1)
                }
                let n = self.clear_all();
                self.note_invalidation(n, true)
            }
        }
    }
    fn note_invalidation(&mut self, n: Counts, global: bool) {
        let total = n.total();
        if total == 0 {
            return;
        }
        if global {
            self.generation = next_generation(self.generation)
        }
        inc!(self, mutation_invalidations, 1);
        inc!(self, invalidated_entries, total);
        inc!(self, mutation_invalidated_record_entries, n.records);
        inc!(self, mutation_invalidated_attribute_entries, n.attributes);
        inc!(self, mutation_invalidated_index_entries, n.indices);
        inc!(self, mutation_invalidated_path_entries, n.paths)
    }
    fn touch(&mut self) -> u64 {
        self.access_clock = next_generation(self.access_clock);
        self.access_clock
    }
    pub fn lookup_record(&mut self, number: u64, out: &mut [u8]) -> bool {
        for i in 0..8 {
            let e = &self.records[i];
            if !e.valid
                || e.generation != self.generation
                || e.number != number
                || e.byte_len as usize != out.len()
            {
                continue;
            }
            out.copy_from_slice(&e.bytes[..out.len()]);
            let clock = self.touch();
            self.records[i].last_used = clock;
            inc!(self, record_hits, 1);
            return true;
        }
        inc!(self, record_misses, 1);
        false
    }
    pub fn store_record(&mut self, number: u64, bytes: &[u8]) {
        if bytes.is_empty() || bytes.len() > 4096 {
            return;
        }
        let mut target = 0;
        let mut oldest = u64::MAX;
        for (i, e) in self.records.iter().enumerate() {
            if !e.valid || e.generation != self.generation || e.number == number {
                target = i;
                oldest = 0;
                break;
            }
            if e.last_used < oldest {
                oldest = e.last_used;
                target = i
            }
        }
        if oldest != 0 && self.records[target].valid {
            inc!(self, record_evictions, 1)
        }
        let clock = self.touch();
        let e = &mut self.records[target];
        e.valid = true;
        e.generation = self.generation;
        e.last_used = clock;
        e.number = number;
        e.byte_len = bytes.len() as u16;
        e.bytes[..bytes.len()].copy_from_slice(bytes);
        inc!(self, record_stores, 1)
    }
    pub fn lookup_attribute(
        &mut self,
        number: u64,
        kind: u32,
        name: &[u8],
        out: &mut AttrScratch,
    ) -> bool {
        if name.len() > 510 {
            return false;
        }
        for i in 0..4 {
            let e = &self.attributes[i];
            if !e.valid
                || e.generation != self.generation
                || e.record_number != number
                || e.attr_type != kind
                || e.name_len as usize != name.len()
                || &e.name[..e.name_len as usize] != name
            {
                continue;
            }
            out.clear();
            out.count = e.count as usize;
            out.runs[..out.count].copy_from_slice(&e.runs[..out.count]);
            out.data_size = e.data_size;
            out.initialized_size = e.initialized_size;
            out.alloc_size = e.alloc_size;
            out.flags = e.flags;
            out.compression_unit = e.compression_unit;
            out.resident = e.resident;
            out.resident_len = e.resident_len as usize;
            out.resident_copy[..out.resident_len]
                .copy_from_slice(&e.resident_copy[..out.resident_len]);
            let clock = self.touch();
            self.attributes[i].last_used = clock;
            inc!(self, attribute_hits, 1);
            return true;
        }
        inc!(self, attribute_misses, 1);
        false
    }
    pub fn store_attribute(&mut self, number: u64, kind: u32, name: &[u8], value: &AttrScratch) {
        if name.len() > 510 || value.count > 256 || value.resident_len > 1024 {
            return;
        }
        let mut target = 0;
        let mut oldest = u64::MAX;
        for (i, e) in self.attributes.iter().enumerate() {
            if !e.valid
                || e.generation != self.generation
                || (e.record_number == number
                    && e.attr_type == kind
                    && e.name_len as usize == name.len()
                    && &e.name[..e.name_len as usize] == name)
            {
                target = i;
                oldest = 0;
                break;
            }
            if e.last_used < oldest {
                oldest = e.last_used;
                target = i
            }
        }
        if oldest != 0 && self.attributes[target].valid {
            inc!(self, attribute_evictions, 1)
        }
        let clock = self.touch();
        let e = &mut self.attributes[target];
        e.valid = true;
        e.generation = self.generation;
        e.last_used = clock;
        e.record_number = number;
        e.attr_type = kind;
        e.name_len = name.len() as u16;
        e.name[..name.len()].copy_from_slice(name);
        e.count = value.count as u16;
        e.runs[..value.count].copy_from_slice(&value.runs[..value.count]);
        e.data_size = value.data_size;
        e.initialized_size = value.initialized_size;
        e.alloc_size = value.alloc_size;
        e.flags = value.flags;
        e.compression_unit = value.compression_unit;
        e.resident = value.resident;
        e.resident_len = value.resident_len as u16;
        e.resident_copy[..value.resident_len]
            .copy_from_slice(&value.resident_copy[..value.resident_len]);
        inc!(self, attribute_stores, 1)
    }
    pub fn lookup_index(&mut self, number: u64, vcn: u64, out: &mut [u8]) -> bool {
        for i in 0..2 {
            let e = &self.indices[i];
            if !e.valid
                || e.generation != self.generation
                || e.directory_record != number
                || e.vcn != vcn
                || e.byte_len as usize != out.len()
            {
                continue;
            }
            out.copy_from_slice(&e.bytes[..out.len()]);
            let clock = self.touch();
            self.indices[i].last_used = clock;
            inc!(self, index_hits, 1);
            return true;
        }
        inc!(self, index_misses, 1);
        false
    }
    pub fn store_index(&mut self, number: u64, vcn: u64, bytes: &[u8]) {
        if bytes.is_empty() || bytes.len() > 4096 {
            return;
        }
        let mut target = 0;
        let mut oldest = u64::MAX;
        for (i, e) in self.indices.iter().enumerate() {
            if !e.valid
                || e.generation != self.generation
                || (e.directory_record == number && e.vcn == vcn)
            {
                target = i;
                oldest = 0;
                break;
            }
            if e.last_used < oldest {
                oldest = e.last_used;
                target = i
            }
        }
        if oldest != 0 && self.indices[target].valid {
            inc!(self, index_evictions, 1)
        }
        let clock = self.touch();
        let e = &mut self.indices[target];
        e.valid = true;
        e.generation = self.generation;
        e.last_used = clock;
        e.directory_record = number;
        e.vcn = vcn;
        e.byte_len = bytes.len() as u16;
        e.bytes[..bytes.len()].copy_from_slice(bytes);
        inc!(self, index_stores, 1)
    }
    pub fn lookup_path(
        &mut self,
        upcase: &[u8],
        number: u64,
        name: &[u8],
        now: u64,
        out: &mut LookupResult,
    ) -> PathLookup {
        inc!(self, path_queries, 1);
        for i in 0..8 {
            let e = &self.paths[i];
            if !e.valid
                || e.generation != self.generation
                || e.directory_record != number
                || names::compare(upcase, &e.name_utf16[..e.name_len as usize], name) != 0
            {
                continue;
            }
            if e.negative && now >= e.expires_at {
                self.paths[i].valid = false;
                inc!(self, path_expirations, 1);
                continue;
            }
            let clock = self.touch();
            let e = &mut self.paths[i];
            e.last_used = clock;
            if e.negative {
                inc!(self, path_negative_hits, 1);
                return PathLookup::Negative;
            }
            *out = e.result;
            inc!(self, path_positive_hits, 1);
            return PathLookup::Positive;
        }
        inc!(self, path_misses, 1);
        PathLookup::Miss
    }
    pub fn store_path(
        &mut self,
        number: u64,
        name: &[u8],
        status: Lookup,
        value: &LookupResult,
        now: u64,
    ) {
        if name.is_empty() || name.len() > 510 || status == Lookup::Io {
            return;
        }
        if status == Lookup::NotFound && self.negative_ttl_ticks == 0 {
            return;
        }
        let mut target = 0;
        let mut oldest = u64::MAX;
        for (i, e) in self.paths.iter().enumerate() {
            if !e.valid || e.generation != self.generation {
                target = i;
                oldest = 0;
                break;
            }
            if e.last_used < oldest {
                oldest = e.last_used;
                target = i
            }
        }
        let _ = oldest;
        let clock = self.touch();
        let e = &mut self.paths[target];
        e.valid = true;
        e.generation = self.generation;
        e.last_used = clock;
        e.directory_record = number;
        e.name_len = name.len() as u16;
        e.name_utf16[..name.len()].copy_from_slice(name);
        e.negative = status == Lookup::NotFound;
        e.expires_at = if e.negative {
            now.saturating_add(self.negative_ttl_ticks)
        } else {
            0
        };
        if status == Lookup::Found {
            e.result = *value;
            inc!(self, path_positive_stores, 1)
        } else {
            inc!(self, path_negative_stores, 1)
        }
    }
    pub fn note_tree_walk(&mut self) {
        inc!(self, lookup_tree_walks, 1)
    }
    pub fn note_recovery_bypass(&mut self) {
        inc!(self, recovery_cache_bypasses, 1)
    }
    pub fn reclaim(&mut self, requested: u32) -> MetadataReclaimResult {
        let mut result = MetadataReclaimResult {
            requested_entries: requested.min(22),
            ..MetadataReclaimResult::EMPTY
        };
        inc!(self, reclaim_requests, 1);
        while result.inspected_entries < 22 && result.reclaimed_entries < result.requested_entries {
            let slot = self.reclaim_cursor;
            self.reclaim_cursor = (self.reclaim_cursor + 1) % 22;
            result.inspected_entries += 1;
            if self.clear_slot(slot) {
                result.reclaimed_entries += 1
            }
        }
        inc!(self, reclaim_scans, result.inspected_entries);
        inc!(self, reclaimed_entries, result.reclaimed_entries);
        result
    }
    pub fn summary(&self) -> MetadataCacheSummary {
        let mut out = self.counters;
        out.version = 2;
        out.active_volumes = 1;
        out.slot_capacity = 22;
        out.bytes_per_volume = core::mem::size_of::<Self>() as u32;
        out.record_capacity = 8;
        out.attribute_capacity = 4;
        out.index_capacity = 2;
        out.path_capacity = 8;
        out.mount_generation = self.mount_generation;
        out.content_generation = self.generation;
        out.negative_ttl_ticks = self.negative_ttl_ticks;
        out.record_entries += self.records.iter().filter(|e| e.valid).count() as u32;
        out.attribute_entries += self.attributes.iter().filter(|e| e.valid).count() as u32;
        out.index_entries += self.indices.iter().filter(|e| e.valid).count() as u32;
        out.path_entries += self.paths.iter().filter(|e| e.valid).count() as u32;
        out
    }
    fn clear_record(&mut self, number: u64) -> Counts {
        let mut n = Counts::default();
        for e in &mut self.records {
            if e.valid && e.number == number {
                e.valid = false;
                n.records += 1
            }
        }
        for e in &mut self.attributes {
            if e.valid && e.record_number == number {
                e.valid = false;
                n.attributes += 1
            }
        }
        for e in &mut self.indices {
            if e.valid && e.directory_record == number {
                e.valid = false;
                n.indices += 1
            }
        }
        for e in &mut self.paths {
            if e.valid
                && (e.directory_record == number || (!e.negative && e.result.record == number))
            {
                e.valid = false;
                n.paths += 1
            }
        }
        n
    }
    fn clear_attribute(&mut self, number: u64, kind: u32) -> Counts {
        let mut n = Counts::default();
        for e in &mut self.attributes {
            if e.valid && e.record_number == number && e.attr_type == kind {
                e.valid = false;
                n.attributes += 1
            }
        }
        n
    }
    fn clear_directory(&mut self, number: u64) -> Counts {
        let mut n = Counts::default();
        for e in &mut self.indices {
            if e.valid && e.directory_record == number {
                e.valid = false;
                n.indices += 1
            }
        }
        for e in &mut self.paths {
            if e.valid && e.directory_record == number {
                e.valid = false;
                n.paths += 1
            }
        }
        n
    }
    fn clear_all(&mut self) -> Counts {
        let mut n = Counts::default();
        for e in &mut self.records {
            if e.valid {
                e.valid = false;
                n.records += 1
            }
        }
        for e in &mut self.attributes {
            if e.valid {
                e.valid = false;
                n.attributes += 1
            }
        }
        for e in &mut self.indices {
            if e.valid {
                e.valid = false;
                n.indices += 1
            }
        }
        for e in &mut self.paths {
            if e.valid {
                e.valid = false;
                n.paths += 1
            }
        }
        n
    }
    fn clear_slot(&mut self, slot: usize) -> bool {
        let valid = if slot < 8 {
            &mut self.records[slot].valid
        } else if slot < 12 {
            &mut self.attributes[slot - 8].valid
        } else if slot < 14 {
            &mut self.indices[slot - 12].valid
        } else {
            &mut self.paths[slot - 14].valid
        };
        if !*valid {
            false
        } else {
            *valid = false;
            true
        }
    }
}

/// # Safety
/// Exact initialized resident cache and disjoint scratch arguments outlive the
/// pure callback. It cannot allocate, wait, log, perform I/O or return a view
/// into the cache. Admission starts before any cache reference is constructed.
pub(crate) unsafe fn with<R>(cache: *mut Cache, call: impl FnOnce(&mut Cache) -> R) -> R {
    // Serialized original host models never execute privileged CPU admission;
    // the product and SMP4 guest always acquire the genuine storage owner.
    #[cfg(not(r4os_owner_fixture))]
    let _owner = crate::memory::owner::Guard::acquire(1);
    unsafe { call(&mut *cache) }
}
