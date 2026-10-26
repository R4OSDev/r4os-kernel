//! Cold NTFS bitmap count: prepare under metadata ownership, then borrow only
//! the backend for bounded reads into request-owned resident scratch.
use super::{
    ntfs_format_types::{Buffer, View},
    ntfs_volume_cache as cache, ntfs_volume_clusters as clusters,
    ntfs_volume_io::{self as io, ReadScratch},
    ntfs_volume_records as records,
    ntfs_volume_types::{AttrScratch, Lookup, Volume},
    request::{Guard, PayloadBorrow},
};
use crate::{memory::heap, sched::scheduler, storage::access_state::Reference};
use core::{
    mem::{align_of, size_of},
    ptr,
};
const CHUNK: usize = 65536;
#[repr(C)]
struct Owned {
    attribute: AttrScratch,
    read: ReadScratch,
    data: [u8; CHUNK],
}
struct Scratch(*mut Owned);
impl Scratch {
    unsafe fn new() -> Option<Self> {
        let p = heap::r4os_rust_heap_alloc(size_of::<Owned>(), align_of::<Owned>()).cast::<Owned>();
        if p.is_null() {
            None
        } else {
            unsafe { ptr::write_bytes(p, 0, 1) };
            Some(Self(p))
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        unsafe {
            heap::r4os_rust_heap_free(self.0.cast(), size_of::<Owned>());
        }
    }
}
/// The original request retains shared namespace admission and exact mounted
/// use throughout. Readers may update their own caches/scratch while I/O waits;
/// no pointer into the shared volume scratch is read after backend release.
pub(super) unsafe fn free_count(v: Volume, guard: *mut Guard, reference: Reference) -> Option<u64> {
    unsafe {
        let total = v.geometry().total_clusters();
        let cached = if v.cache().is_null() {
            None
        } else {
            cache::with(v.cache(), |c| c.free_clusters(total))
        };
        if cached.is_some() {
            return cached;
        }
        if v.cache().is_null() || !PayloadBorrow::can_start(guard, reference) {
            return clusters::free_count(v);
        }
        let Some(scratch) = Scratch::new() else {
            return clusters::free_count(v);
        };
        let attr = ptr::addr_of_mut!((*scratch.0).attribute);
        // Collection may use the original mount scratch; its backend is still held.
        if records::collect(v, 6, 0x80, View::default(), attr) != Lookup::Found || (*attr).resident
        {
            return None;
        }
        let needed = total / 8 + u64::from(!total.is_multiple_of(8));
        if needed > (*attr).data_size || needed > (*attr).initialized_size {
            return None;
        }
        let Some(stamp) = cache::with(v.cache(), |c| c.content_stamp()) else {
            return clusters::free_count(v);
        };
        let mut private = v.io;
        private.scratch = ptr::null_mut();
        private.cache = ptr::null_mut();
        private.work_counters = false;
        private.read_identity = ptr::null();
        private.read_scratch = ptr::addr_of_mut!((*scratch.0).read);
        let data = ptr::addr_of_mut!((*scratch.0).data).cast::<u8>();
        let mut cluster = 0;
        let mut free = 0;
        while cluster < total {
            if cache::with(v.cache(), |c| c.content_stamp()) != Some(stamp) {
                return None;
            }
            let bits = (total - cluster).min(CHUNK as u64 * 8);
            let bytes = (bits / 8 + u64::from(!bits.is_multiple_of(8))) as usize;
            let counted = {
                let _borrow = PayloadBorrow::begin(guard, reference)?;
                if !io::runs(
                    private,
                    records::attr_runs(attr),
                    cluster / 8,
                    Buffer { data, len: bytes },
                    None,
                    ptr::null_mut(),
                ) {
                    return None;
                }
                // Only owned bytes are read; a maximum64KB CPU section is bounded.
                clusters::count_bits(core::slice::from_raw_parts(data, bytes), bits)
            }; // Reacquire the exact backend before any metadata continuation.
            if !PayloadBorrow::can_start(guard, reference)
                || cache::with(v.cache(), |c| c.content_stamp()) != Some(stamp)
            {
                return None;
            }
            free += counted;
            cluster += bits;
            scheduler::safe_reschedule();
        }
        cache::with(v.cache(), |c| {
            if c.content_stamp() == Some(stamp) {
                c.store_free_clusters(total, free);
                Some(free)
            } else {
                None
            }
        })
    }
}
const _: () = assert!(size_of::<Owned>() <= 80 * 1024);
