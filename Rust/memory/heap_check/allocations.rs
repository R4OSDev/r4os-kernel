//! Exact diagnostic allocation identities; failed releases stay resident and
//! are never retried against a possibly reused allocator address.
use super::{c, heap};
use crate::memory::{vm_ranges, vm_transactions as tx};
use core::{alloc::Layout, ptr};

#[derive(Clone, Copy)]
struct Slot {
    pointer: *mut u8,
    len: usize,
    alignment: usize,
    rust: bool,
    retained: bool,
}
impl Slot {
    const EMPTY: Self = Self {
        pointer: ptr::null_mut(),
        len: 0,
        alignment: 0,
        rust: false,
        retained: false,
    };
}
pub(super) struct State {
    slots: [Slot; 200],
    reservations: [u32; 2],
    retained_reservations: [bool; 2],
    rust_failed_history: [usize; 2],
    pub(super) report: c::ProgramKernelHeapCheck,
}
impl State {
    // The fixed record contains only integers; zero is valid before begin().
    pub(super) const EMPTY: Self = Self {
        slots: [Slot::EMPTY; 200],
        reservations: [0; 2],
        retained_reservations: [false; 2],
        rust_failed_history: [0; 2],
        report: unsafe { core::mem::zeroed() },
    };

    pub(super) fn begin(&mut self, id: u64) {
        assert!(self.rust_failed_history == [0; 2]);
        assert!(self.slots.iter().all(|s| s.pointer.is_null()) && self.reservations == [0; 2]);
        self.report = c::ProgramKernelHeapCheck {
            run_id: id,
            flags: c::kernel_heap_check_flag_initialized,
            ..Default::default()
        };
    }
    pub(super) fn pointer(&self, index: usize) -> *mut u8 {
        self.slots[index].pointer
    }
    pub(super) fn occupied(&self, index: usize) -> bool {
        !self.pointer(index).is_null()
    }
    fn account(&mut self, index: usize, slot: Slot) {
        assert!(!self.occupied(index));
        self.slots[index] = slot;
        self.report.owned_allocations += 1;
        self.report.owned_live_blocks += 1;
        self.report.owned_live_bytes += slot.len as u64;
        self.report.owned_allocation_bytes += slot.len as u64;
        self.report.peak_owned_bytes = self
            .report
            .peak_owned_bytes
            .max(self.report.owned_live_bytes);
    }
    pub(super) fn allocate(&mut self, index: usize, len: usize, alignment: usize) -> bool {
        assert!(!self.occupied(index));
        let pointer = heap::r4os_rust_heap_alloc(len, alignment);
        if pointer.is_null() {
            return false;
        }
        self.account(
            index,
            Slot {
                pointer,
                len,
                alignment,
                rust: false,
                retained: false,
            },
        );
        true
    }
    pub(super) fn rust_allocate(&mut self, index: usize, len: usize, alignment: usize) -> bool {
        assert!(!self.occupied(index));
        let Ok(layout) = Layout::from_size_align(len, alignment) else {
            return false;
        };
        let pointer = unsafe { crate::alloc::alloc::alloc(layout) };
        if pointer.is_null() {
            return false;
        }
        self.account(
            index,
            Slot {
                pointer,
                len,
                alignment,
                rust: true,
                retained: false,
            },
        );
        true
    }
    pub(super) fn rust_resize(&mut self, index: usize, len: usize) -> bool {
        let old = self.slots[index];
        assert!(old.rust && !old.pointer.is_null() && !old.retained);
        let Ok(layout) = Layout::from_size_align(old.len, old.alignment) else {
            return false;
        };
        let pointer = unsafe { crate::alloc::alloc::realloc(old.pointer, layout, len) };
        if pointer.is_null() {
            return false;
        }
        self.slots[index].pointer = pointer;
        self.slots[index].len = len;
        self.report.owned_live_bytes = self.report.owned_live_bytes - old.len as u64 + len as u64;
        self.report.owned_allocation_bytes += len.saturating_sub(old.len) as u64;
        self.report.peak_owned_bytes = self
            .report
            .peak_owned_bytes
            .max(self.report.owned_live_bytes);
        true
    }
    fn returned(&mut self, index: usize) {
        self.report.owned_releases += 1;
        self.report.owned_live_blocks -= 1;
        self.report.owned_live_bytes -= self.slots[index].len as u64;
        self.slots[index] = Slot::EMPTY;
    }
    pub(super) fn release(&mut self, index: usize) -> bool {
        let slot = self.slots[index];
        assert!(!slot.pointer.is_null());
        if slot.retained {
            return false;
        }
        let ok = if slot.rust {
            unsafe { heap::rust_deallocate_probe(slot.pointer, slot.len, slot.alignment) }
        } else {
            unsafe { heap::r4os_rust_heap_free(slot.pointer, slot.len) == 0 }
        };
        if ok {
            self.returned(index);
        } else {
            self.slots[index].retained = true;
            self.report.cleanup_errors += 1;
        }
        ok
    }
    pub(super) fn retain_rust_balance_failure(&mut self, original: *mut u8, resized: *mut u8) {
        // Layouts are the fixed 64/64 and 160/64 Rust case. Preserve both
        // historical addresses under this run ID; never retry either free.
        self.rust_failed_history = [original.addr(), resized.addr()];
        self.report.cleanup_errors = self.report.cleanup_errors.max(1);
    }
    pub(super) fn duplicate_free(&mut self, index: usize) -> bool {
        let slot = self.slots[index];
        assert!(!slot.pointer.is_null() && !slot.rust && !slot.retained);
        let (released, rejected) = unsafe { heap::duplicate_free_probe(slot.pointer, slot.len) };
        if released {
            self.returned(index);
        } else {
            self.slots[index].retained = true;
            self.report.cleanup_errors += 1;
        }
        if rejected {
            self.report.expected_double_free_errors += 1;
        }
        released && rejected
    }
    pub(super) fn reserve(&mut self, slot: usize, window: u8, owner: u8, len: u64) -> bool {
        assert!(self.reservations[slot] == 0);
        let input = vm_ranges::Reserve {
            eviction_allowed: true,
            window,
            kind: 4,
            owner,
            owner_id: 0,
            name_ptr: b"kernel-heap-check".as_ptr(),
            name_len: b"kernel-heap-check".len(),
            flags: 2 | (1 << 63),
            base: 0,
            len,
        };
        let mut id = 0;
        if unsafe { vm_ranges::r4os_rust_vm_reserve(&input, 4096, &mut id) } != 0 {
            return false;
        }
        self.reservations[slot] = id;
        self.report.vm_reservations += 1;
        self.report.vm_live_reservations += 1;
        true
    }
    pub(super) fn reservation(&self, slot: usize) -> u32 {
        self.reservations[slot]
    }
    pub(super) fn release_reservation(&mut self, slot: usize) -> bool {
        assert!(self.reservations[slot] != 0);
        if self.retained_reservations[slot] {
            return false;
        }
        if tx::r4os_rust_vm_release(self.reservations[slot]) != 0 {
            self.retained_reservations[slot] = true;
            self.report.cleanup_errors += 1;
            return false;
        }
        self.reservations[slot] = 0;
        self.report.vm_releases += 1;
        self.report.vm_live_reservations -= 1;
        true
    }
    pub(super) fn cleanup(&mut self) {
        for index in 0..self.slots.len() {
            if self.occupied(index) && !self.slots[index].retained {
                self.release(index);
            }
        }
        for index in 0..self.reservations.len() {
            if self.reservations[index] != 0 && !self.retained_reservations[index] {
                self.release_reservation(index);
            }
        }
    }
}
