//! Exact mounted file/stream/content identity for immutable decoded units.
use super::{
    ntfs_format_types::View,
    ntfs_volume_cache::{self, Cache},
    ntfs_volume_io::Io,
    ntfs_volume_types::AttrScratch,
};
use crate::storage::access_state::Reference;
use core::ptr;
/// Borrowed only by this synchronous read's exact task frame.
pub struct Identity {
    pub reference: Reference,
    pub record: u64,
    pub stream: View,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    pub cache: *mut Cache,
    reference: Reference,
    mount_epoch: u64,
    revision: u64,
    record: u64,
    name_len: usize,
    name: [u8; 510],
    unit: u64,
    pub bytes: usize,
    data_size: u64,
    initialized: u64,
    allocated: u64,
    flags: u16,
    compression: u8,
}
impl Key {
    pub const EMPTY: Self = Self {
        cache: ptr::null_mut(),
        reference: Reference {
            slot: 0,
            generation: 0,
        },
        mount_epoch: 0,
        revision: 0,
        record: 0,
        name_len: 0,
        name: [0; 510],
        unit: 0,
        bytes: 0,
        data_size: 0,
        initialized: 0,
        allocated: 0,
        flags: 0,
        compression: 0,
    };
    /// Caller owns a copied attribute and exact mounted admission. Stage stream
    /// bytes before metadata admission; no external pointer enters the pool.
    pub unsafe fn new(io: Io, attr: *const AttrScratch, unit: u64, bytes: usize) -> Option<Self> {
        if io.cache.is_null() || io.read_identity.is_null() || bytes == 0 || bytes > 65536 {
            return None;
        }
        let identity = unsafe { &*io.read_identity };
        if identity.reference.generation == 0 || identity.stream.len > 510 {
            return None;
        }
        let mut key = Self {
            cache: io.cache,
            reference: identity.reference,
            record: identity.record,
            name_len: identity.stream.len,
            unit,
            bytes,
            data_size: unsafe { (*attr).data_size },
            initialized: unsafe { (*attr).initialized_size },
            allocated: unsafe { (*attr).alloc_size },
            flags: unsafe { (*attr).flags },
            compression: unsafe { (*attr).compression_unit },
            ..Self::EMPTY
        };
        if key.name_len != 0 {
            unsafe {
                ptr::copy_nonoverlapping(identity.stream.data, key.name.as_mut_ptr(), key.name_len);
            }
        }
        let (epoch, revision) =
            unsafe { ntfs_volume_cache::with(io.cache, |cache| cache.content_stamp()) }?;
        key.mount_epoch = epoch;
        key.revision = revision;
        Some(key)
    }
    pub unsafe fn current(self) -> bool {
        !self.cache.is_null()
            && unsafe { ntfs_volume_cache::with(self.cache, |cache| cache.content_stamp()) }
                == Some((self.mount_epoch, self.revision))
    }
}
