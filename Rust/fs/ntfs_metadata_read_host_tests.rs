//! Actual metadata-read orchestration against modeled heap/I/O/admission.
//! Native NTFS structures, IRQs and scheduler are qualified separately.
#![allow(dead_code)]
use std::sync::Mutex;
static TEST: Mutex<()> = Mutex::new(());
#[derive(Clone, Copy)]
struct Stats {
    held: bool,
    allow: bool,
    fail_alloc: bool,
    fail_io: bool,
    mutate: bool,
    cache: usize,
    io: usize,
    loans: usize,
    returns: usize,
    allocs: usize,
    frees: usize,
    fallback: usize,
    checkpoints: usize,
    shared_tag: u64,
}
const INITIAL: Stats = Stats {
    held: true,
    allow: true,
    fail_alloc: false,
    fail_io: false,
    mutate: false,
    cache: 0,
    io: 0,
    loans: 0,
    returns: 0,
    allocs: 0,
    frees: 0,
    fallback: 0,
    checkpoints: 0,
    shared_tag: 7,
};
static S: Mutex<Stats> = Mutex::new(INITIAL);
mod storage {
    pub mod access_state {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub struct Reference {
            pub slot: u32,
            pub generation: u64,
        }
    }
}
mod memory {
    pub mod heap {
        pub fn r4os_rust_heap_alloc(size: usize, align: usize) -> *mut u8 {
            let mut s = crate::S.lock().unwrap();
            if s.fail_alloc {
                return core::ptr::null_mut();
            }
            s.allocs += 1;
            drop(s);
            unsafe { std::alloc::alloc(std::alloc::Layout::from_size_align(size, align).unwrap()) }
        }
        pub unsafe fn r4os_rust_heap_free(p: *mut u8, size: usize) {
            let mut s = crate::S.lock().unwrap();
            assert!(s.held);
            s.frees += 1;
            drop(s);
            unsafe {
                std::alloc::dealloc(p, std::alloc::Layout::from_size_align(size, 8).unwrap());
            }
        }
    }
}
mod sched {
    pub mod scheduler {
        pub fn safe_reschedule() {
            let mut s = crate::S.lock().unwrap();
            assert!(s.held);
            s.checkpoints += 1;
        }
    }
}
mod ntfs_format_types {
    #[derive(Default)]
    pub struct View;
    pub struct Buffer {
        pub data: *mut u8,
        pub len: usize,
    }
}
mod ntfs_volume_types {
    #[repr(C)]
    pub struct AttrScratch {
        pub tag: u64,
        pub resident: bool,
        pub data_size: u64,
        pub initialized_size: u64,
    }
    #[derive(PartialEq, Eq)]
    pub enum Lookup {
        Found,
    }
    #[derive(Clone, Copy)]
    pub struct Geometry {
        pub total: u64,
    }
    impl Geometry {
        pub fn total_clusters(self) -> u64 {
            self.total
        }
    }
    #[derive(Clone, Copy)]
    pub struct Volume {
        pub io: super::ntfs_volume_io::Io,
    }
    impl Volume {
        pub fn geometry(self) -> Geometry {
            self.io.geometry
        }
        pub fn cache(self) -> *mut super::ntfs_volume_cache::Cache {
            self.io.cache
        }
    }
}
mod ntfs_volume_cache {
    pub struct Cache {
        pub stamp: (u64, u64),
        pub cached: Option<u64>,
        pub stored: Option<u64>,
    }
    impl Cache {
        pub fn free_clusters(&self, _: u64) -> Option<u64> {
            self.cached
        }
        pub fn content_stamp(&self) -> Option<(u64, u64)> {
            Some(self.stamp)
        }
        pub fn store_free_clusters(&mut self, _: u64, free: u64) {
            self.stored = Some(free);
        }
    }
    pub unsafe fn with<R>(c: *mut Cache, call: impl FnOnce(&mut Cache) -> R) -> R {
        unsafe { call(&mut *c) }
    }
}
mod request {
    use super::storage::access_state::Reference;
    pub struct Guard;
    pub struct PayloadBorrow;
    impl PayloadBorrow {
        pub unsafe fn can_start(_: *mut Guard, r: Reference) -> bool {
            let s = crate::S.lock().unwrap();
            s.allow && s.held && r.slot == 1 && r.generation == 9
        }
        pub unsafe fn begin(g: *mut Guard, r: Reference) -> Option<Self> {
            if !unsafe { Self::can_start(g, r) } {
                return None;
            }
            let mut s = crate::S.lock().unwrap();
            s.held = false;
            s.loans += 1;
            Some(Self)
        }
    }
    impl Drop for PayloadBorrow {
        fn drop(&mut self) {
            let mut s = crate::S.lock().unwrap();
            assert!(!s.held);
            s.held = true;
            s.returns += 1;
        }
    }
}
mod ntfs_volume_io {
    use super::{
        ntfs_format_types::Buffer,
        ntfs_volume_cache::Cache,
        ntfs_volume_types::{AttrScratch, Geometry},
    };
    #[repr(C)]
    pub struct ReadScratch {
        pub sector: [u8; 512],
        pub input: *mut u8,
        pub output: *mut u8,
    }
    #[derive(Clone, Copy)]
    pub struct Io {
        pub geometry: Geometry,
        pub scratch: *mut u8,
        pub cache: *mut Cache,
        pub work_counters: bool,
        pub read_identity: *const u8,
        pub read_scratch: *mut ReadScratch,
    }
    pub struct Runs {
        pub tag: u64,
    }
    pub unsafe fn runs(
        io: Io,
        runs: Runs,
        _: u64,
        b: Buffer,
        _: Option<()>,
        _: *mut usize,
    ) -> bool {
        let mut s = crate::S.lock().unwrap();
        assert!(!s.held);
        assert!(
            io.scratch.is_null()
                && io.cache.is_null()
                && !io.read_scratch.is_null()
                && !io.work_counters
                && io.read_identity.is_null()
        );
        assert_eq!(runs.tag, 7);
        s.shared_tag = 999;
        s.io += 1;
        if s.mutate {
            unsafe {
                (*(s.cache as *mut Cache)).stamp.1 += 1;
            }
        }
        if s.fail_io {
            return false;
        }
        drop(s);
        unsafe {
            core::ptr::write_bytes(b.data, 0, b.len);
            b.data.write(0xff);
        }
        true
    }
    pub unsafe fn tag(a: *const AttrScratch) -> Runs {
        Runs {
            tag: unsafe { (*a).tag },
        }
    }
}
mod ntfs_volume_records {
    use super::{ntfs_format_types::View, ntfs_volume_io, ntfs_volume_types::*};
    pub unsafe fn collect(v: Volume, _: u64, _: u32, _: View, out: *mut AttrScratch) -> Lookup {
        let s = crate::S.lock().unwrap();
        assert!(s.held);
        let n = v.geometry().total_clusters().div_ceil(8);
        unsafe {
            out.write(AttrScratch {
                tag: s.shared_tag,
                resident: false,
                data_size: n,
                initialized_size: n,
            });
        }
        Lookup::Found
    }
    pub unsafe fn attr_runs(a: *const AttrScratch) -> ntfs_volume_io::Runs {
        unsafe { ntfs_volume_io::tag(a) }
    }
}
mod ntfs_volume_clusters {
    use super::ntfs_volume_types::Volume;
    pub unsafe fn free_count(_: Volume) -> Option<u64> {
        let mut s = crate::S.lock().unwrap();
        assert!(s.held);
        s.fallback += 1;
        Some(123)
    }
    // Independent bit-by-bit oracle, not the product's word-popcount loop.
    pub fn count_bits(bytes: &[u8], bits: u64) -> u64 {
        (0..bits)
            .filter(|&b| bytes[b as usize / 8] & (1 << (b % 8)) == 0)
            .count() as u64
    }
}
#[path = "ntfs_metadata_read.rs"]
mod actual;
fn setup() -> (Box<ntfs_volume_cache::Cache>, ntfs_volume_types::Volume) {
    let mut c = Box::new(ntfs_volume_cache::Cache {
        stamp: (5, 7),
        cached: None,
        stored: None,
    });
    *S.lock().unwrap() = Stats {
        cache: core::ptr::from_mut(&mut *c) as usize,
        ..INITIAL
    };
    let v = ntfs_volume_types::Volume {
        io: ntfs_volume_io::Io {
            geometry: ntfs_volume_types::Geometry { total: 1_048_577 },
            scratch: core::ptr::dangling_mut(),
            cache: core::ptr::from_mut(&mut *c),
            work_counters: false,
            read_identity: core::ptr::null(),
            read_scratch: core::ptr::null_mut(),
        },
    };
    (c, v)
}
fn run(v: ntfs_volume_types::Volume) -> Option<u64> {
    unsafe {
        actual::free_count(
            v,
            core::ptr::dangling_mut(),
            storage::access_state::Reference {
                slot: 1,
                generation: 9,
            },
        )
    }
}
#[test]
fn three_bounded_chunks_survive_foreign_shared_scratch_poison_and_count_tail() {
    let _test = TEST.lock().unwrap();
    let (c, v) = setup();
    let answer = run(v);
    assert_eq!(answer, Some(1_048_577 - 8 - 8 - 1));
    assert_eq!(c.stored, answer);
    let s = *S.lock().unwrap();
    assert_eq!(
        (s.io, s.loans, s.returns, s.allocs, s.frees, s.checkpoints),
        (3, 3, 3, 1, 1, 3)
    );
    assert!(s.held);
    assert_eq!(s.shared_tag, 999);
}
#[test]
fn changed_generation_refuses_stale_store_after_exact_backend_return() {
    let _test = TEST.lock().unwrap();
    let (c, v) = setup();
    S.lock().unwrap().mutate = true;
    assert_eq!(run(v), None);
    assert_eq!(c.stored, None);
    let s = *S.lock().unwrap();
    assert_eq!((s.io, s.returns, s.frees), (1, 1, 1));
    assert!(s.held);
}
#[test]
fn io_failure_returns_loan_before_freeing_owned_scratch() {
    let _test = TEST.lock().unwrap();
    let (c, v) = setup();
    S.lock().unwrap().fail_io = true;
    assert_eq!(run(v), None);
    assert_eq!(c.stored, None);
    let s = *S.lock().unwrap();
    assert_eq!((s.loans, s.returns, s.allocs, s.frees), (1, 1, 1, 1));
    assert!(s.held);
}
#[test]
fn warm_cache_has_no_allocation_or_io() {
    let _test = TEST.lock().unwrap();
    let (mut c, v) = setup();
    c.cached = Some(77);
    assert_eq!(run(v), Some(77));
    let s = *S.lock().unwrap();
    assert_eq!((s.allocs, s.io, s.loans), (0, 0, 0));
}
#[test]
fn memory_or_admission_failure_keeps_original_serialized_owner() {
    let _test = TEST.lock().unwrap();
    let (_c, v) = setup();
    S.lock().unwrap().fail_alloc = true;
    assert_eq!(run(v), Some(123));
    let s = *S.lock().unwrap();
    assert_eq!((s.io, s.loans, s.fallback), (0, 0, 1));
    let (_c, v) = setup();
    S.lock().unwrap().allow = false;
    assert_eq!(run(v), Some(123));
    let s = *S.lock().unwrap();
    assert_eq!((s.allocs, s.io, s.loans, s.fallback), (0, 0, 0, 1));
}
