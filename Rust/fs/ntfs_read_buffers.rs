//! Prepared data reads borrow only the backend and isolate their scratch.
//! Metadata collection remains with the original per-mount NTFS scratch.
use super::{
    ntfs_format_types::Buffer,
    ntfs_volume_io::{Io, ReadScratch},
    ntfs_volume_read,
    ntfs_volume_types::AttrScratch,
    request::{Guard, PayloadBorrow},
};
use crate::{memory::heap, storage::access_state::Reference};
use core::ptr;
const UNIT: usize = 65536;
struct Buffers {
    scratch: ReadScratch,
    compressed: *mut u8,
}
impl Buffers {
    unsafe fn new(compressed: bool) -> Option<Self> {
        let storage = if compressed {
            heap::r4os_rust_heap_alloc(2 * UNIT, 16)
        } else {
            ptr::null_mut()
        };
        if compressed && storage.is_null() {
            return None;
        }
        Some(Self {
            scratch: ReadScratch {
                sector: [0; 512],
                input: storage,
                output: if compressed {
                    unsafe { storage.add(UNIT) }
                } else {
                    ptr::null_mut()
                },
            },
            compressed: storage,
        })
    }
}
impl Drop for Buffers {
    fn drop(&mut self) {
        if !self.compressed.is_null() {
            unsafe {
                heap::r4os_rust_heap_free(self.compressed, 2 * UNIT);
            }
        }
    }
}
pub(super) unsafe fn read(
    io: Io,
    attr: *const AttrScratch,
    offset: usize,
    out: Buffer,
    guard: *mut Guard,
    reference: Reference,
) -> Option<usize> {
    unsafe {
        if !PayloadBorrow::can_start(guard, reference) {
            return ntfs_volume_read::attribute(io, attr, offset, out);
        }
        let Some(mut buffers) = Buffers::new((*attr).flags & 1 != 0) else {
            // Existing serialized resident scratch remains available under the
            // still held backend; memory pressure never aliases another reader.
            return ntfs_volume_read::attribute(io, attr, offset, out);
        };
        let Some(_payload) = PayloadBorrow::begin(guard, reference) else {
            return ntfs_volume_read::attribute(io, attr, offset, out);
        };
        let mut data_io = io;
        data_io.read_scratch = ptr::addr_of_mut!(buffers.scratch);
        // _payload restores the backend before buffers are released and before
        // any caller metadata continuation. Both stay on this exact taskstack.
        ntfs_volume_read::attribute(data_io, attr, offset, out)
    }
}
