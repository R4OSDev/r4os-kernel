//! One owned diagnostic range and its two exact physical frame claims. Every
//! partial acquisition stays visible; only acknowledged returns clear it.
use super::{NEW, OLD, PAGE, claim_probe_frame, release_probe_frame};
use crate::{
    arch::tlb,
    memory::{paging, phys, vm_query as query, vm_ranges as ranges, vm_transactions as vm},
};
use core::ptr;
pub(super) struct Mapping {
    pub range_id: u32,
    pub address: u64,
    pub old_frame: u64,
    pub new_frame: u64,
    pub mapped_frame: u64,
    pub last_unmap: tlb::Result,
    pub retained: bool,
}
impl Mapping {
    pub const EMPTY: Self = Self {
        range_id: 0,
        address: 0,
        old_frame: 0,
        new_frame: 0,
        mapped_frame: 0,
        last_unmap: tlb::Result {
            ok: false,
            generation: 0,
            target_mask: 0,
            acknowledgement_mask: 0,
            offline_mask: 0,
            spins: 0,
        },
        retained: false,
    };
    /// Caller retains this resident object on any failure and retires every
    /// reader before cleanup. No worker receives a borrow of this metadata.
    pub fn acquire(&mut self) -> bool {
        if self.range_id != 0 || self.old_frame != 0 || self.new_frame != 0 || self.retained {
            return false;
        }
        let request = ranges::Reserve {
            eviction_allowed: false,
            window: 0,
            kind: 4,
            owner: 0,
            owner_id: 0,
            name_ptr: b"tlb-acceptance".as_ptr(),
            name_len: 14,
            flags: 2 | (1 << 63),
            base: 0,
            len: PAGE,
        };
        if unsafe { ranges::r4os_rust_vm_reserve(&request, PAGE, &mut self.range_id) } != 0 {
            return false;
        }
        let mut range = core::mem::MaybeUninit::uninit();
        if !unsafe { query::r4os_rust_vm_range_info(self.range_id, range.as_mut_ptr()) } {
            return false;
        }
        self.address = unsafe { range.assume_init() }.base;
        let Some(old) = claim_probe_frame(b"tlb-probe-old") else {
            return false;
        };
        self.old_frame = old;
        let Some(new) = claim_probe_frame(b"tlb-probe-new") else {
            return false;
        };
        self.new_frame = new;
        unsafe {
            ptr::with_exposed_provenance_mut::<u64>(phys::r4os_rust_phys_to_virt(old) as usize)
                .write_volatile(OLD);
            ptr::with_exposed_provenance_mut::<u64>(phys::r4os_rust_phys_to_virt(new) as usize)
                .write_volatile(NEW);
        }
        if !paging::r4os_rust_paging_map(self.address, old, 2 | (1 << 63), false) {
            return false;
        }
        self.mapped_frame = old;
        true
    }
    pub fn replace(&mut self) -> bool {
        let (ok, receipt) = paging::unmap_with_receipt(self.address);
        self.last_unmap = receipt;
        if !ok {
            return false;
        }
        self.mapped_frame = 0;
        if paging::r4os_rust_paging_map(self.address, self.new_frame, 2 | (1 << 63), false) {
            self.mapped_frame = self.new_frame;
            return true;
        }
        if paging::r4os_rust_paging_map(self.address, self.old_frame, 2 | (1 << 63), false) {
            self.mapped_frame = self.old_frame;
        }
        false
    }
    /// Requires actual reader task retirement, not just phase/done flags.
    /// An unacknowledged return is sticky and never retried via a raw address.
    pub fn cleanup(&mut self) -> bool {
        if self.retained {
            return false;
        }
        if self.mapped_frame != 0 {
            let mut mapped = 0;
            let exact =
                unsafe { paging::r4os_rust_paging_read(self.address, 0, false, &mut mapped) }
                    && mapped == self.mapped_frame;
            if !exact || !paging::r4os_rust_paging_unmap(self.address, false) {
                self.retained = true;
                return false;
            }
            self.mapped_frame = 0;
        }
        for frame in [&mut self.old_frame, &mut self.new_frame] {
            if *frame != 0 {
                if !release_probe_frame(*frame) {
                    self.retained = true;
                    return false;
                }
                *frame = 0;
            }
        }
        if self.range_id != 0 {
            if vm::r4os_rust_vm_release(self.range_id) != 0 {
                self.retained = true;
                return false;
            }
            self.range_id = 0;
            self.address = 0;
        }
        true
    }
    pub fn live_frames(&self) -> u64 {
        u64::from(self.old_frame != 0) + u64::from(self.new_frame != 0)
    }
}
