//! Native original NTFS volume values and resident per-mount storage.
use super::ntfs_format_types::Run;
pub const NAME_MAX: usize = 768;
pub const UNITS: usize = 255;
pub const ATTR_RUNS: usize = 256;
pub const MFT_RUNS: usize = 64;
pub const ENTRY_MAX: usize = 0x10 + 0x42 + 510 + 8 + 8;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WorkStats {
    pub cluster_bitmap_probes: u64,
    pub record_bitmap_probes: u64,
    pub read_zero_bytes: u64,
    pub directory_entries: u64,
}
impl WorkStats {
    pub const EMPTY: Self = Self {
        cluster_bitmap_probes: 0,
        record_bitmap_probes: 0,
        read_zero_bytes: 0,
        directory_entries: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MetadataCacheSummary {
    pub version: u32,
    pub active_volumes: u32,
    pub slot_capacity: u32,
    pub bytes_per_volume: u32,
    pub record_capacity: u32,
    pub attribute_capacity: u32,
    pub index_capacity: u32,
    pub path_capacity: u32,
    pub mount_generation: u64,
    pub content_generation: u64,
    pub negative_ttl_ticks: u64,
    pub record_entries: u32,
    pub attribute_entries: u32,
    pub index_entries: u32,
    pub path_entries: u32,
    pub record_hits: u64,
    pub record_misses: u64,
    pub record_stores: u64,
    pub record_evictions: u64,
    pub attribute_hits: u64,
    pub attribute_misses: u64,
    pub attribute_stores: u64,
    pub attribute_evictions: u64,
    pub index_hits: u64,
    pub index_misses: u64,
    pub index_stores: u64,
    pub index_evictions: u64,
    pub path_queries: u64,
    pub path_positive_hits: u64,
    pub path_negative_hits: u64,
    pub path_misses: u64,
    pub path_positive_stores: u64,
    pub path_negative_stores: u64,
    pub path_expirations: u64,
    pub lookup_tree_walks: u64,
    pub recovery_cache_bypasses: u64,
    pub mount_invalidations: u64,
    pub mutation_invalidations: u64,
    pub external_invalidations: u64,
    pub invalidated_entries: u64,
    pub payload_write_retentions: u64,
    pub system_write_retentions: u64,
    pub targeted_invalidations: u64,
    pub targeted_record_invalidations: u64,
    pub targeted_attribute_invalidations: u64,
    pub targeted_directory_invalidations: u64,
    pub global_mutation_invalidations: u64,
    pub recovery_invalidations: u64,
    pub mutation_invalidated_record_entries: u64,
    pub mutation_invalidated_attribute_entries: u64,
    pub mutation_invalidated_index_entries: u64,
    pub mutation_invalidated_path_entries: u64,
    pub reclaim_requests: u64,
    pub reclaim_scans: u64,
    pub reclaimed_entries: u64,
}
impl MetadataCacheSummary {
    pub const EMPTY: Self = Self {
        version: 2,
        active_volumes: 0,
        slot_capacity: 22,
        bytes_per_volume: 0,
        record_capacity: 8,
        attribute_capacity: 4,
        index_capacity: 2,
        path_capacity: 8,
        mount_generation: 0,
        content_generation: 0,
        negative_ttl_ticks: 0,
        record_entries: 0,
        attribute_entries: 0,
        index_entries: 0,
        path_entries: 0,
        record_hits: 0,
        record_misses: 0,
        record_stores: 0,
        record_evictions: 0,
        attribute_hits: 0,
        attribute_misses: 0,
        attribute_stores: 0,
        attribute_evictions: 0,
        index_hits: 0,
        index_misses: 0,
        index_stores: 0,
        index_evictions: 0,
        path_queries: 0,
        path_positive_hits: 0,
        path_negative_hits: 0,
        path_misses: 0,
        path_positive_stores: 0,
        path_negative_stores: 0,
        path_expirations: 0,
        lookup_tree_walks: 0,
        recovery_cache_bypasses: 0,
        mount_invalidations: 0,
        mutation_invalidations: 0,
        external_invalidations: 0,
        invalidated_entries: 0,
        payload_write_retentions: 0,
        system_write_retentions: 0,
        targeted_invalidations: 0,
        targeted_record_invalidations: 0,
        targeted_attribute_invalidations: 0,
        targeted_directory_invalidations: 0,
        global_mutation_invalidations: 0,
        recovery_invalidations: 0,
        mutation_invalidated_record_entries: 0,
        mutation_invalidated_attribute_entries: 0,
        mutation_invalidated_index_entries: 0,
        mutation_invalidated_path_entries: 0,
        reclaim_requests: 0,
        reclaim_scans: 0,
        reclaimed_entries: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MetadataReclaimResult {
    pub requested_entries: u32,
    pub inspected_entries: u32,
    pub reclaimed_entries: u32,
}
impl MetadataReclaimResult {
    pub const EMPTY: Self = Self {
        requested_entries: 0,
        inspected_entries: 0,
        reclaimed_entries: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub name: [u8; NAME_MAX],
    pub name_len: usize,
    pub attr: u8,
    pub reparse: bool,
    pub record: u64,
    pub sequence: u16,
    pub size: u64,
    pub created_time_nt: u64,
    pub modified_time_nt: u64,
    pub access_time_nt: u64,
}
impl Entry {
    pub const EMPTY: Self = Self {
        name: [0; NAME_MAX],
        name_len: 0,
        attr: 0,
        reparse: false,
        record: 0,
        sequence: 0,
        size: 0,
        created_time_nt: 0,
        modified_time_nt: 0,
        access_time_nt: 0,
    };
    pub fn is_dir(self) -> bool {
        self.attr & 0x10 != 0
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LookupResult {
    pub record: u64,
    pub sequence: u16,
    pub entry: Entry,
    pub in_root: bool,
    pub block_vcn: u64,
    pub entry_offset: usize,
}
impl LookupResult {
    pub const EMPTY: Self = Self {
        record: 0,
        sequence: 0,
        entry: Entry::EMPTY,
        in_root: false,
        block_vcn: 0,
        entry_offset: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AttrScratch {
    pub runs: [Run; ATTR_RUNS],
    pub count: usize,
    pub data_size: u64,
    pub initialized_size: u64,
    pub alloc_size: u64,
    pub flags: u16,
    pub compression_unit: u8,
    pub resident: bool,
    pub resident_len: usize,
    pub resident_copy: [u8; 1024],
}
impl AttrScratch {
    pub const EMPTY: Self = Self {
        runs: [Run {
            length_clusters: 0,
            has_lcn: false,
            lcn: 0,
        }; ATTR_RUNS],
        count: 0,
        data_size: 0,
        initialized_size: 0,
        alloc_size: 0,
        flags: 0,
        compression_unit: 0,
        resident: false,
        resident_len: 0,
        resident_copy: [0; 1024],
    };
}
#[derive(Clone, Copy)]
pub enum Mutation {
    Payload,
    Record(u64),
    Attribute { record: u64, kind: u32 },
    Directory(u64),
    System,
    Unknown,
    Recovery,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Found,
    NotFound,
    Io,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PathLookup {
    Miss,
    Positive,
    Negative,
}
pub fn next_generation(current: u64) -> u64 {
    let n = current.wrapping_add(1);
    if n == 0 { 1 } else { n }
}

impl AttrScratch {
    pub fn clear(&mut self) {
        unsafe { core::ptr::write_bytes(self as *mut Self, 0, 1) }
    }
}

/// Original per-mount resident scratch. Native pointer access, never an
/// exclusive whole-structure borrow across I/O; initialization stays in place.
#[repr(C)]
pub struct Scratch {
    pub work: WorkStats,
    pub cluster_search_start: u64,
    pub record_search_start: u64,
    pub repair_required: bool,
    pub has_mirror_bootstrap_lcn: bool,
    pub mirror_bootstrap_lcn: u64,
    pub rollback_record: [u8; 4096],
    pub attr: AttrScratch,
    pub record: [u8; 4096],
    pub part_record: [u8; 4096],
    pub write_record: [u8; 4096],
    pub block: [u8; 4096 + ENTRY_MAX],
    pub block2: [u8; 4096],
    pub entry_a: [u8; ENTRY_MAX],
    pub entry_b: [u8; ENTRY_MAX],
    pub name_utf16: [u8; 510],
    pub sector: [u8; 512],
    pub attr_op: AttrScratch,
    pub attr_index: AttrScratch,
    pub attr_mgmt_a: AttrScratch,
    pub attr_mgmt_b: AttrScratch,
    pub attr_cluster: AttrScratch,
    pub attr_mirror: AttrScratch,
    pub mirror_buf: [u8; 4096],
    pub comp_in: [u8; 65536],
    pub comp_out: [u8; 65536],
}
impl Scratch {
    /// # Safety
    /// Exclusive aligned resident allocation; every native field has a valid
    /// zero representation. No old I/O/borrows may refer to the allocation.
    pub unsafe fn initialize(out: *mut Self) {
        unsafe {
            core::ptr::write_bytes(out, 0, 1);
            (*out).record_search_start = 16
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Geometry {
    pub partition_lba: u32,
    pub cluster_bytes: u32,
    pub total_sectors: u64,
}
impl Geometry {
    pub fn sectors_per_cluster(self) -> u32 {
        self.cluster_bytes / 512
    }
    pub fn total_clusters(self) -> u64 {
        if self.cluster_bytes == 0 || !self.cluster_bytes.is_multiple_of(512) {
            0
        } else {
            self.total_sectors / u64::from(self.sectors_per_cluster())
        }
    }
    pub fn sector_io_valid(self, lba: u64, count: u32, len: usize) -> bool {
        if count == 0 {
            return len == 0;
        }
        let Some(bytes) = u64::from(count).checked_mul(512) else {
            return false;
        };
        if bytes != len as u64 || lba < u64::from(self.partition_lba) {
            return false;
        }
        let relative = lba - u64::from(self.partition_lba);
        relative < self.total_sectors && u64::from(count) <= self.total_sectors - relative
    }
    pub fn mapped_run_valid(self, run: Run) -> bool {
        if run.length_clusters == 0
            || self.cluster_bytes == 0
            || !self.cluster_bytes.is_multiple_of(512)
            || self.total_sectors.checked_mul(512).is_none()
        {
            return false;
        }
        if !run.has_lcn {
            return true;
        }
        let total = self.total_clusters();
        run.lcn < total
            && run.length_clusters <= total - run.lcn
            && run.lcn.checked_mul(u64::from(self.cluster_bytes)).is_some()
            && run
                .length_clusters
                .checked_mul(u64::from(self.cluster_bytes))
                .is_some()
    }
    pub fn runlist_valid(self, runs: &[Run]) -> bool {
        if self.cluster_bytes == 0
            || !self.cluster_bytes.is_multiple_of(512)
            || self.total_sectors.checked_mul(512).is_none()
        {
            return false;
        }
        let mut logical = 0u64;
        for run in runs {
            if run.length_clusters == 0 {
                return false;
            }
            let Some(next) = logical.checked_add(run.length_clusters) else {
                return false;
            };
            logical = next;
            if !self.mapped_run_valid(*run) {
                return false;
            }
        }
        true
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DirectoryFrame {
    pub vcn: u64,
    pub offset: u32,
    pub child_done: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DirectoryCursor {
    pub state: u32,
    pub depth: u32,
    pub frames: [DirectoryFrame; 8],
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DirectoryNext {
    Found,
    NotFound,
    Again,
    Io,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RecordName {
    pub parent: u64,
    pub parent_sequence: u16,
    pub name: [u8; NAME_MAX],
    pub name_len: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResolvedEntry {
    pub entry: Entry,
    pub parent_record: u64,
}
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WriteStatus {
    Ok,
    Invalid,
    NotFound,
    Exists,
    Directory,
    NotDirectory,
    NotEmpty,
    ReadOnlyTarget,
    NoSpace,
    DirFull,
    RecordFull,
    Unsupported,
    OffsetMismatch,
    Io,
    CleanupFailed,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WriteProgress {
    pub status: WriteStatus,
    pub completed: usize,
    pub uncertain: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MountInfo {
    pub cluster_bytes: u32,
    pub record_bytes: u32,
    pub index_block_bytes: u32,
    pub total_sectors: u64,
    pub mft_lcn: u64,
    pub mftmirr_lcn: u64,
    pub mft_run_count: usize,
    pub used_mft_mirror: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Volume {
    pub io: super::ntfs_volume_io::Io,
    pub record_bytes: u32,
    pub index_block_bytes: u32,
    pub mft_runs: *mut Run,
    pub mft_capacity: usize,
    pub mft_count: *mut usize,
    pub upcase: super::ntfs_format_types::View,
    pub now_filetime: u64,
    pub security_id_file: u32,
    pub security_id_dir: u32,
    pub cache_now_ticks: u64,
}
impl Volume {
    pub fn geometry(self) -> Geometry {
        self.io.geometry
    }
    pub fn scratch(self) -> *mut Scratch {
        self.io.scratch
    }
    pub fn cache(self) -> *mut super::ntfs_volume_cache::Cache {
        self.io.cache
    }
    pub(super) unsafe fn record(self) -> super::ntfs_format_types::Buffer {
        unsafe {
            assert!(self.record_bytes <= 4096);
            super::ntfs_format_types::Buffer {
                data: core::ptr::addr_of_mut!((*self.scratch()).record).cast(),
                len: self.record_bytes as usize,
            }
        }
    }
    pub(super) unsafe fn write_record(self) -> super::ntfs_format_types::Buffer {
        unsafe {
            assert!(self.record_bytes <= 4096);
            super::ntfs_format_types::Buffer {
                data: core::ptr::addr_of_mut!((*self.scratch()).write_record).cast(),
                len: self.record_bytes as usize,
            }
        }
    }
    pub(super) unsafe fn part_record(self) -> super::ntfs_format_types::Buffer {
        unsafe {
            assert!(self.record_bytes <= 4096);
            super::ntfs_format_types::Buffer {
                data: core::ptr::addr_of_mut!((*self.scratch()).part_record).cast(),
                len: self.record_bytes as usize,
            }
        }
    }
}
#[derive(Clone, Copy)]
pub struct AllocationResult {
    pub status: WriteStatus,
    pub produced: usize,
}
impl AllocationResult {
    pub fn error(status: WriteStatus) -> Self {
        Self {
            status,
            produced: 0,
        }
    }
}
#[derive(Clone, Copy)]
pub struct FindRun {
    pub status: WriteStatus,
    pub run: Run,
}
impl FindRun {
    pub fn error(status: WriteStatus) -> Self {
        Self {
            status,
            run: Run {
                length_clusters: 0,
                has_lcn: false,
                lcn: 0,
            },
        }
    }
}
