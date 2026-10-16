//! Original program registry admission, lists, pins and bounded capacity work.
//! Every raw record has a stable address; no Rust reference spans a peer call.
use super::{memory, registry_state::state, registry_types::*};
use crate::{
    contract::ProgramProcessHandle as Handle,
    memory::heap,
    platform::timer,
    sched::{sync_locks, sync_objects},
};
use core::{ffi::c_void, mem, ptr};

const FREE: u8 = 0;
const CREATE: u8 = 1;
const PUBLISH: u8 = 2;
const RUN: u8 = 3;
const EXIT: u8 = 4;
const DONE: u8 = 5;
const RETIRE: u8 = 6;
const REAP: u8 = 7;
const PRIVATE: u8 = 0;
const PENDING: u8 = 1;
const CONSUMED: u8 = 3;
const IDLE: u8 = 0;
const COMPLETED: u8 = 1;
const DEFERRED: u8 = 2;

pub fn visible(v: u8) -> bool {
    matches!(v, PUBLISH | RUN | EXIT | DONE)
}
pub fn owns_payload(v: u8) -> bool {
    !matches!(v, FREE | CREATE | REAP)
}
pub fn running(v: u8) -> bool {
    matches!(v, PUBLISH | RUN)
}
pub fn valid(h: Handle) -> bool {
    h.instance_id != 0 && h.generation != 0 && h.reserved == 0
}
pub fn equal(a: Handle, b: Handle) -> bool {
    a.instance_id == b.instance_id && a.reserved == b.reserved && a.generation == b.generation
}
pub fn lock() -> bool {
    unsafe { sync_locks::r4os_rust_sync_mutex((*state()).mutex, 1, u64::MAX) }
}
pub fn unlock() {
    unsafe { sync_locks::r4os_rust_sync_mutex((*state()).mutex, 2, 0) };
}
struct Guard;
impl Guard {
    fn acquire() -> Option<Self> {
        if lock() { Some(Self) } else { None }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        unlock();
    }
}
fn signal() {
    unsafe { sync_objects::r4os_rust_sync_event((*state()).event, 0, 0) };
}
unsafe fn stats() -> *mut ProgramRegistryStats {
    unsafe { (*state()).stats }
}
pub(super) unsafe fn handle_for_slot(slot: *const ProgramRegistrySlot) -> Handle {
    unsafe {
        Handle {
            instance_id: (*slot).public_id,
            reserved: 0,
            generation: (*slot).generation,
        }
    }
}
pub(super) unsafe fn handle_for_reservation(r: *const ProgramInstanceReservation) -> Handle {
    unsafe {
        Handle {
            instance_id: (*r).id,
            reserved: 0,
            generation: (*r).generation,
        }
    }
}
unsafe fn chunk_empty(chunk: *const ProgramRegistryChunk) -> bool {
    unsafe {
        for slot in &(*chunk).slots {
            if slot.state != FREE || slot.pin_count != 0 {
                return false;
            }
        }
        true
    }
}
unsafe fn free_slot() -> *mut ProgramRegistrySlot {
    unsafe {
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            for i in 0..16 {
                let slot = ptr::addr_of_mut!((*chunk).slots[i]);
                if (*slot).state == FREE && (*slot).pin_count == 0 {
                    return slot;
                }
            }
            chunk = (*chunk).next;
        }
        ptr::null_mut()
    }
}
unsafe fn chunk_for_slot(wanted: *const ProgramRegistrySlot) -> *mut ProgramRegistryChunk {
    unsafe {
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            for i in 0..16 {
                if ptr::addr_of!((*chunk).slots[i]) == wanted {
                    return chunk;
                }
            }
            chunk = (*chunk).next;
        }
        ptr::null_mut()
    }
}
unsafe fn slot_for_instance(wanted: *const ProgramInstance) -> *mut ProgramRegistrySlot {
    unsafe {
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            for i in 0..16 {
                let slot = ptr::addr_of_mut!((*chunk).slots[i]);
                if ptr::addr_of!((*slot).instance) == wanted {
                    return slot;
                }
            }
            chunk = (*chunk).next;
        }
        ptr::null_mut()
    }
}
/// # Safety
/// The caller holds the registry mutex; the returned slot is borrowed only until
/// unlock unless the caller acquires an exact generation pin under that mutex.
pub unsafe fn lookup_id(id: u32, retiring: bool) -> *mut ProgramRegistrySlot {
    if id == 0 {
        return ptr::null_mut();
    }
    unsafe {
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            for i in 0..16 {
                let slot = ptr::addr_of_mut!((*chunk).slots[i]);
                if (visible((*slot).state) || (retiring && matches!((*slot).state, RETIRE | REAP)))
                    && (*slot).public_id == id
                    && (*slot).instance.id == id
                {
                    return slot;
                }
            }
            chunk = (*chunk).next;
        }
        ptr::null_mut()
    }
}
/// # Safety
/// The caller holds the registry mutex and retains it, or pins this exact slot
/// and generation before using the returned pointer after unlock.
pub unsafe fn lookup_handle(h: Handle, retiring: bool) -> *mut ProgramRegistrySlot {
    if !valid(h) {
        return ptr::null_mut();
    }
    let slot = unsafe { lookup_id(h.instance_id, retiring) };
    if slot.is_null() || unsafe { (*slot).generation } != h.generation {
        ptr::null_mut()
    } else {
        slot
    }
}
unsafe fn has_id(id: u32) -> bool {
    if id == 0 {
        return false;
    }
    unsafe {
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            for slot in &(*chunk).slots {
                if slot.state != FREE && slot.public_id == id {
                    return true;
                }
            }
            chunk = (*chunk).next;
        }
        false
    }
}
/// # Safety
/// The caller holds the registry mutex. The returned node remains borrowed
/// under that mutex; this lookup does not extend its lifetime past unlinking.
pub unsafe fn completion_handle(h: Handle) -> *mut ProgramCompletionNode {
    unsafe {
        let mut node = (*state()).completion_head;
        while !node.is_null() {
            if (*node).state != CONSUMED && equal((*node).handle, h) {
                return node;
            }
            node = (*node).next;
        }
        ptr::null_mut()
    }
}
pub(super) unsafe fn completion_id(id: u32) -> *mut ProgramCompletionNode {
    if id == 0 {
        return ptr::null_mut();
    }
    unsafe {
        let mut node = (*state()).completion_head;
        while !node.is_null() {
            if (*node).state != CONSUMED && (*node).handle.instance_id == id {
                return node;
            }
            node = (*node).next;
        }
        ptr::null_mut()
    }
}
unsafe fn id_in_use(id: u32) -> bool {
    id == 0 || unsafe { has_id(id) || !completion_id(id).is_null() }
}
/// # Safety
/// The caller holds the registry mutex for both the slot and completion scans.
pub unsafe fn missing(h: Handle) -> i32 {
    if !valid(h) {
        return -1;
    }
    if unsafe { has_id(h.instance_id) || !completion_id(h.instance_id).is_null() } {
        -3
    } else {
        -2
    }
}
unsafe fn published_count() -> u64 {
    unsafe {
        let mut count = 0u64;
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            for slot in &(*chunk).slots {
                if visible(slot.state) {
                    count = count.wrapping_add(1);
                }
            }
            chunk = (*chunk).next;
        }
        count
    }
}
/// # Safety
/// The caller holds the registry mutex and has exclusive mutation admission.
pub unsafe fn epoch() {
    unsafe {
        (*state()).epoch = (*state()).epoch.wrapping_add(1);
        if (*state()).epoch == 0 {
            (*state()).epoch = 1;
        }
    }
}
unsafe fn generation() -> u64 {
    unsafe {
        let s = state();
        if (*s).generation_exhausted || (*s).next_generation == 0 {
            (*stats()).last_admission_error = -11;
            return 0;
        }
        let value = (*s).next_generation;
        if value == u64::MAX {
            (*s).next_generation = 0;
            (*s).generation_exhausted = true;
        } else {
            (*s).next_generation = value + 1;
        }
        (*stats()).next_generation = (*s).next_generation;
        value
    }
}
unsafe fn allocate_id() -> u32 {
    unsafe {
        let s = state();
        let t = stats();
        let blockers = (u64::from((*t).chunk_count) * 16)
            .saturating_add((*t).completion_pending)
            .saturating_add((*t).completion_ready);
        let mut remaining = blockers.saturating_add(1).min(u64::from(u32::MAX));
        while remaining != 0 {
            remaining -= 1;
            let mut candidate = (*s).next_instance_id;
            if candidate == 0 {
                candidate = 1;
            }
            (*s).next_instance_id = candidate.wrapping_add(1);
            if (*s).next_instance_id == 0 {
                (*s).next_instance_id = 1;
                (*t).id_wraps = (*t).id_wraps.wrapping_add(1);
            }
            if !id_in_use(candidate) {
                return candidate;
            }
            (*t).id_collisions = (*t).id_collisions.wrapping_add(1);
        }
        (*t).last_admission_error = -2;
        0
    }
}
unsafe fn should_fail_growth() -> bool {
    unsafe {
        let s = state();
        if (*s).next_growth_failure {
            (*s).next_growth_failure = false;
            (*stats()).forced_failures = (*stats()).forced_failures.wrapping_add(1);
            return true;
        }
        if !(*s).failure_after_present {
            return false;
        }
        let cursor = (*s).failure_cursor;
        (*s).failure_cursor = cursor.wrapping_add(1);
        cursor == (*s).failure_after
    }
}
unsafe fn init_core(core: *mut ProgramInstance, id: u32) {
    unsafe {
        ptr::write_bytes(core.cast::<u8>(), 0, mem::size_of::<ProgramInstance>());
        (*core).memory_limits = memory::Limits {
            vm_reserve_limit: 1024 * 1024 * 1024,
            vm_commit_limit: 256 * 1024 * 1024,
            resident_limit: 256 * 1024 * 1024,
            stack_reserve: 4 * 1024 * 1024,
            stack_initial_commit: 64 * 1024,
        };
        (*core).app_class = 1;
        (*core).memory_profile = 2;
        (*core).id = id;
    }
}
unsafe fn init_chunk(chunk: *mut ProgramRegistryChunk) {
    unsafe {
        ptr::write_bytes(
            chunk.cast::<u8>(),
            0,
            mem::size_of::<ProgramRegistryChunk>(),
        );
        for i in 0..16 {
            init_core(ptr::addr_of_mut!((*chunk).slots[i].instance), 0);
        }
    }
}
// Candidate statuses:0 chunk;1 injected;2 allocation;3 lock failure.
unsafe fn candidate(out: *mut *mut ProgramRegistryChunk) -> u8 {
    if !lock() {
        return 3;
    }
    let injected = unsafe {
        (*stats()).growth_attempts = (*stats()).growth_attempts.wrapping_add(1);
        let fail = should_fail_growth();
        if fail {
            (*stats()).growth_failures = (*stats()).growth_failures.wrapping_add(1);
            (*stats()).last_admission_error = -1;
        }
        fail
    };
    unlock();
    if injected {
        return 1;
    }
    let memory = heap::r4os_rust_heap_alloc(
        mem::size_of::<ProgramRegistryChunk>(),
        mem::align_of::<ProgramRegistryChunk>(),
    );
    if memory.is_null() {
        if lock() {
            unsafe {
                (*stats()).growth_failures = (*stats()).growth_failures.wrapping_add(1);
                (*stats()).last_admission_error = -1;
            }
            unlock();
        }
        return 2;
    }
    let chunk = memory.cast();
    unsafe {
        init_chunk(chunk);
        out.write(chunk);
    }
    0
}
unsafe fn free_chunk(chunk: *mut ProgramRegistryChunk) -> bool {
    unsafe { heap::r4os_rust_heap_free(chunk.cast(), mem::size_of::<ProgramRegistryChunk>()) == 0 }
}
unsafe fn link_chunk(chunk: *mut ProgramRegistryChunk) {
    unsafe {
        let s = state();
        let t = stats();
        (*chunk).next = ptr::null_mut();
        (*chunk).serial = (*s).next_chunk_serial;
        (*s).next_chunk_serial = (*s).next_chunk_serial.wrapping_add(1);
        if (*s).next_chunk_serial == 0 {
            (*s).next_chunk_serial = 1;
        }
        (*chunk).empty_since_epoch = (*s).epoch;
        if !(*s).tail.is_null() {
            (*(*s).tail).next = chunk;
        } else {
            (*s).head = chunk;
        }
        (*s).tail = chunk;
        (*t).chunk_count = (*t).chunk_count.wrapping_add(1);
        (*t).slot_capacity = (*t).slot_capacity.wrapping_add(16);
        if u64::from((*t).chunk_count) > (*t).peak_chunks {
            (*t).peak_chunks = u64::from((*t).chunk_count);
        }
        super::payload::registry_telemetry(0, ptr::null_mut());
    }
}
// Grow statuses:0 grown;1 concurrent capacity;2 injected;3 allocation;4 lock.
unsafe fn grow() -> u8 {
    let mut chunk = ptr::null_mut();
    let result = unsafe { candidate(&mut chunk) };
    if result != 0 {
        return match result {
            1 => 2,
            2 => 3,
            3 => 4,
            _ => unreachable!(),
        };
    }
    let mut linked = false;
    let locked = lock();
    if locked {
        unsafe {
            if free_slot().is_null() {
                link_chunk(chunk);
                linked = true;
            }
        }
        unlock();
    }
    if !linked {
        if !unsafe { free_chunk(chunk) } && lock() {
            unsafe {
                link_chunk(chunk);
            }
            linked = true;
            unlock();
        }
        if !locked && !linked {
            return 4;
        }
    }
    if linked { 0 } else { 1 }
}
pub(super) unsafe fn clear_slot(slot: *mut ProgramRegistrySlot) {
    unsafe {
        (*slot).state = FREE;
        (*slot).public_id = 0;
        (*slot).pin_count = 0;
        (*slot).reclaim_pending = false;
        (*slot).retire_queued = false;
        (*slot).retire_in_progress = false;
        (*slot).cancel_pending = false;
        (*slot).retire_phase = 0;
        (*slot).retire_output_detached = false;
        (*slot).retire_storage_released = false;
        (*slot).retire_image_stack_released = false;
        (*slot).retire_owner_released = false;
        (*slot).retire_attempts = 0;
        (*slot).retire_next = ptr::null_mut();
        (*slot).completion = ptr::null_mut();
        init_core(ptr::addr_of_mut!((*slot).instance), 0);
        epoch();
        let chunk = chunk_for_slot(slot);
        if !chunk.is_null() && chunk_empty(chunk) {
            (*chunk).empty_since_epoch = (*state()).epoch;
        }
    }
}
pub(super) unsafe fn enqueue(slot: *mut ProgramRegistrySlot) -> bool {
    unsafe {
        if (*slot).retire_queued || (*slot).retire_in_progress {
            return false;
        }
        (*slot).retire_queued = true;
        (*slot).retire_next = ptr::null_mut();
        if !(*state()).retire_tail.is_null() {
            (*(*state()).retire_tail).retire_next = slot;
        } else {
            (*state()).retire_head = slot;
        }
        (*state()).retire_tail = slot;
        true
    }
}
pub(super) unsafe fn take() -> *mut ProgramRegistrySlot {
    unsafe {
        let s = state();
        let slot = (*s).retire_head;
        if slot.is_null() {
            return slot;
        }
        (*s).retire_head = (*slot).retire_next;
        if (*s).retire_head.is_null() {
            (*s).retire_tail = ptr::null_mut();
        }
        (*slot).retire_next = ptr::null_mut();
        (*slot).retire_queued = false;
        (*slot).retire_in_progress = true;
        slot
    }
}
unsafe fn unlink_chunk(wanted: *mut ProgramRegistryChunk) -> bool {
    unsafe {
        let mut previous: *mut ProgramRegistryChunk = ptr::null_mut();
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            if chunk != wanted {
                previous = chunk;
                chunk = (*chunk).next;
                continue;
            }
            if !previous.is_null() {
                (*previous).next = (*chunk).next;
            } else {
                (*state()).head = (*chunk).next;
            }
            if (*state()).tail == chunk {
                (*state()).tail = previous;
            }
            (*chunk).next = ptr::null_mut();
            if (*stats()).chunk_count != 0 {
                (*stats()).chunk_count -= 1;
            }
            if (*stats()).slot_capacity >= 16 {
                (*stats()).slot_capacity -= 16;
            }
            super::payload::registry_telemetry(1, ptr::null_mut());
            return true;
        }
        false
    }
}
pub(super) unsafe fn shrink(pressure: bool) {
    loop {
        if !lock() {
            return;
        }
        let mut removed = ptr::null_mut();
        unsafe {
            if (*stats()).chunk_count > 2 {
                let now = (*state()).epoch;
                let age = if pressure { 8 } else { 64 };
                let mut chunk = (*state()).head;
                while !chunk.is_null() {
                    if chunk_empty(chunk)
                        && (*chunk).empty_since_epoch != 0
                        && now.wrapping_sub((*chunk).empty_since_epoch) >= age
                    {
                        if unlink_chunk(chunk) {
                            removed = chunk;
                        }
                        break;
                    }
                    chunk = (*chunk).next;
                }
            }
        }
        unlock();
        if removed.is_null() {
            return;
        }
        if unsafe { free_chunk(removed) } {
            if lock() {
                unsafe {
                    (*stats()).shrink_count = (*stats()).shrink_count.wrapping_add(1);
                }
                unlock();
            }
            continue;
        }
        if lock() {
            unsafe {
                link_chunk(removed);
            }
            unlock();
        }
        return;
    }
}
unsafe fn reserve(out: *mut ProgramInstanceReservation) -> u8 {
    let mut recovered = false;
    loop {
        if !lock() {
            return 1;
        }
        let slot = unsafe { free_slot() };
        if !slot.is_null() {
            unsafe {
                let id = allocate_id();
                if id == 0 {
                    unlock();
                    return 1;
                }
                let generation = generation();
                if generation == 0 {
                    unlock();
                    return 2;
                }
                (*slot).generation = generation;
                (*slot).public_id = id;
                (*slot).pin_count = 0;
                (*slot).reclaim_pending = false;
                (*slot).retire_queued = false;
                (*slot).retire_in_progress = false;
                (*slot).cancel_pending = false;
                (*slot).retire_phase = 0;
                (*slot).retire_output_detached = false;
                (*slot).retire_storage_released = false;
                (*slot).retire_image_stack_released = false;
                (*slot).retire_owner_released = false;
                (*slot).retire_attempts = 0;
                (*slot).retire_next = ptr::null_mut();
                (*slot).completion = ptr::null_mut();
                init_core(ptr::addr_of_mut!((*slot).instance), id);
                (*slot).state = CREATE;
                let chunk = chunk_for_slot(slot);
                if !chunk.is_null() {
                    (*chunk).empty_since_epoch = 0;
                }
                epoch();
                (*stats()).last_admission_error = 0;
                let mut value: ProgramInstanceReservation = mem::zeroed();
                value.slot = slot;
                value.id = id;
                value.generation = generation;
                out.write(value);
            }
            unlock();
            return 0;
        }
        unlock();
        match unsafe { grow() } {
            0 | 1 => {}
            2 | 4 => return 1,
            3 => {
                if recovered {
                    return 1;
                }
                recovered = true;
                unsafe { super::process::r4os_rust_program_process_pressure() };
            }
            _ => unreachable!(),
        }
    }
}
#[repr(C)]
pub struct CompletionRequest {
    pub reservation: *const ProgramInstanceReservation,
    pub owner: bool,
    pub override_present: bool,
    pub owner_handle: Handle,
    pub legacy_id: bool,
    pub retain_output: bool,
}
unsafe fn allocate_completion(r: &CompletionRequest) -> bool {
    let memory = heap::r4os_rust_heap_alloc(
        mem::size_of::<ProgramCompletionNode>(),
        mem::align_of::<ProgramCompletionNode>(),
    );
    if memory.is_null() {
        return false;
    }
    unsafe {
        let node = memory.cast::<ProgramCompletionNode>();
        ptr::write_bytes(memory, 0, mem::size_of::<ProgramCompletionNode>());
        (*node).handle = handle_for_reservation(r.reservation);
        (*node).owner = r.owner;
        if r.owner {
            if r.override_present {
                (*node).owner_handle = r.owner_handle;
            } else {
                if let Some(h) = super::context::handle() {
                    (*node).owner_handle = h;
                }
            }
        }
        (*node).state = PRIVATE;
        (*node).legacy_id = r.legacy_id;
        (*node).retain_output = r.retain_output;
        (*node).start_tick = timer::tick_count();
        (*node).flags = if r.owner { 8 } else { 0 };
        (*node).console_state = Default::default();
        if lock() {
            let slot = (*r.reservation).slot;
            if (*slot).state == CREATE
                && (*slot).public_id == (*r.reservation).id
                && (*slot).generation == (*r.reservation).generation
                && (*slot).completion.is_null()
            {
                (*slot).completion = node;
                (*node).slot_attached = true;
                unlock();
                return true;
            }
            unlock();
        }
        free_completion(node);
        false
    }
}
pub(super) unsafe fn free_completion(node: *mut ProgramCompletionNode) {
    unsafe { heap::r4os_rust_heap_free(node.cast(), mem::size_of::<ProgramCompletionNode>()) };
}
unsafe fn link_completion(node: *mut ProgramCompletionNode) {
    unsafe {
        (*node).next = ptr::null_mut();
        (*node).state = PENDING;
        if !(*state()).completion_tail.is_null() {
            (*(*state()).completion_tail).next = node;
        } else {
            (*state()).completion_head = node;
        }
        (*state()).completion_tail = node;
        (*stats()).completion_pending = (*stats()).completion_pending.wrapping_add(1);
        let total = (*stats())
            .completion_pending
            .wrapping_add((*stats()).completion_ready);
        if total > (*stats()).completion_peak {
            (*stats()).completion_peak = total;
        }
        epoch();
    }
}
pub(super) unsafe fn unlink_completion(wanted: *mut ProgramCompletionNode) -> bool {
    unsafe {
        let mut previous: *mut ProgramCompletionNode = ptr::null_mut();
        let mut node = (*state()).completion_head;
        while !node.is_null() {
            if node != wanted {
                previous = node;
                node = (*node).next;
                continue;
            }
            if !previous.is_null() {
                (*previous).next = (*node).next;
            } else {
                (*state()).completion_head = (*node).next;
            }
            if (*state()).completion_tail == node {
                (*state()).completion_tail = previous;
            }
            (*node).next = ptr::null_mut();
            epoch();
            return true;
        }
        false
    }
}
pub(super) unsafe fn finish_cancel(r: *const ProgramInstanceReservation) -> u8 {
    unsafe {
        let slot = (*r).slot;
        let complete = super::peer_lifetime::cleanup(ptr::addr_of_mut!((*slot).instance));
        if !lock() {
            return DEFERRED;
        }
        if (*slot).state != CREATE
            || (*slot).public_id != (*r).id
            || (*slot).generation != (*r).generation
            || !(*slot).cancel_pending
            || !(*slot).retire_in_progress
        {
            unlock();
            return IDLE;
        }
        if !complete {
            (*slot).retire_in_progress = false;
            (*slot).retire_attempts = (*slot).retire_attempts.wrapping_add(1);
            (*stats()).retire_deferred = (*stats()).retire_deferred.wrapping_add(1);
            enqueue(slot);
            unlock();
            signal();
            return DEFERRED;
        }
        let node = (*slot).completion;
        (*slot).completion = ptr::null_mut();
        if !node.is_null() {
            (*node).slot_attached = false;
        }
        clear_slot(slot);
        (*stats()).rollback_count = (*stats()).rollback_count.wrapping_add(1);
        unlock();
        if !node.is_null() {
            free_completion(node);
        }
        shrink(false);
        COMPLETED
    }
}
unsafe fn cancel(r: *const ProgramInstanceReservation) {
    if !lock() {
        return;
    }
    unsafe {
        let slot = (*r).slot;
        if (*slot).state != CREATE
            || (*slot).public_id != (*r).id
            || (*slot).generation != (*r).generation
            || (*slot).cancel_pending
            || (*slot).retire_queued
            || (*slot).retire_in_progress
        {
            unlock();
            return;
        }
        (*slot).cancel_pending = true;
        (*slot).retire_in_progress = true;
    }
    unlock();
    unsafe { finish_cancel(r) };
}
unsafe fn publish(r: *const ProgramInstanceReservation) -> bool {
    let Some(_guard) = Guard::acquire() else {
        return false;
    };
    unsafe {
        let slot = (*r).slot;
        if (*slot).state != CREATE
            || (*slot).public_id != (*r).id
            || (*slot).generation != (*r).generation
            || (*slot).instance.id != (*r).id
        {
            return false;
        }
        let node = (*slot).completion;
        if node.is_null() {
            return false;
        }
        (*slot).instance.used = true;
        (*slot).state = PUBLISH;
        link_completion(node);
        (*stats()).publish_count = (*stats()).publish_count.wrapping_add(1);
        let count = published_count();
        if count > (*stats()).peak_live {
            (*stats()).peak_live = count;
        }
        super::payload::registry_telemetry(2, ptr::addr_of_mut!((*slot).instance));
        true
    }
}
unsafe fn pin(h: Handle, done: bool, out: *mut ProgramInstanceLease) -> bool {
    let Some(_guard) = Guard::acquire() else {
        return false;
    };
    unsafe {
        let slot = lookup_handle(h, false);
        if slot.is_null() {
            return false;
        }
        if !running((*slot).state) && !(done && matches!((*slot).state, EXIT | DONE)) {
            return false;
        }
        if (*slot).pin_count == u32::MAX {
            return false;
        }
        (*slot).pin_count += 1;
        let mut lease: ProgramInstanceLease = mem::zeroed();
        lease.slot = slot;
        lease.instance = ptr::addr_of_mut!((*slot).instance);
        lease.id = h.instance_id;
        lease.generation = h.generation;
        out.write(lease);
        true
    }
}
unsafe fn pin_id(id: u32, out: *mut ProgramInstanceLease) -> bool {
    if !lock() {
        return false;
    }
    let slot = unsafe { lookup_id(id, false) };
    if slot.is_null() {
        unlock();
        return false;
    }
    let h = unsafe { handle_for_slot(slot) };
    unlock();
    unsafe { pin(h, true, out) }
}
pub(super) unsafe fn unpin(lease: *const ProgramInstanceLease) {
    if !lock() {
        return;
    }
    let wake = unsafe {
        let slot = (*lease).slot;
        if (*slot).generation != (*lease).generation
            || (*slot).public_id != (*lease).id
            || (*slot).pin_count == 0
        {
            (*stats()).stale_lease_rejections = (*stats()).stale_lease_rejections.wrapping_add(1);
            false
        } else {
            (*slot).pin_count -= 1;
            (*slot).pin_count == 0 && (*slot).state == RETIRE
        }
    };
    unlock();
    if wake {
        signal();
    }
}
fn mix(value: u64) -> u64 {
    let mut v = value.wrapping_add(0x9e3779b97f4a7c15);
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d049bb133111eb);
    v ^ (v >> 31)
}
unsafe fn snapshot(out: *mut ProgramRegistryStats) {
    let locked = lock();
    unsafe {
        let mut result = stats().read();
        if !locked {
            out.write(result);
            return;
        }
        result.failure_armed = (*state()).next_growth_failure;
        result.free_slots = 0;
        result.reserved_slots = 0;
        result.live_slots = 0;
        result.done_slots = 0;
        result.retiring_slots = 0;
        result.pinned_slots = 0;
        result.live_id_hash = 0;
        result.live_address_hash = 0;
        let mut chunk = (*state()).head;
        while !chunk.is_null() {
            for i in 0..16 {
                let slot = ptr::addr_of!((*chunk).slots[i]);
                match (*slot).state {
                    FREE => result.free_slots = result.free_slots.wrapping_add(1),
                    CREATE => result.reserved_slots = result.reserved_slots.wrapping_add(1),
                    PUBLISH | RUN | EXIT => result.live_slots = result.live_slots.wrapping_add(1),
                    DONE => result.done_slots = result.done_slots.wrapping_add(1),
                    RETIRE | REAP => result.retiring_slots = result.retiring_slots.wrapping_add(1),
                    _ => unreachable!(),
                }
                if (*slot).pin_count != 0 {
                    result.pinned_slots = result.pinned_slots.wrapping_add(1);
                }
                if running((*slot).state) {
                    result.live_id_hash ^= mix(u64::from((*slot).public_id));
                    result.live_address_hash ^= mix(ptr::addr_of!((*slot).instance).addr() as u64);
                }
            }
            chunk = (*chunk).next;
        }
        result.next_generation = (*state()).next_generation;
        result.retire_queued = 0;
        let mut slot = (*state()).retire_head;
        while !slot.is_null() {
            result.retire_queued = result.retire_queued.wrapping_add(1);
            slot = (*slot).retire_next;
        }
        result.history_head = (*state()).history_head as u64;
        result.history_count = (*state()).history_count as u64;
        result.history_sequence = (*state()).last_exit_sequence;
        out.write(result);
    }
    unlock();
}

/// # Safety
/// Pure ordinal/handle projections; selected C pointer input/output is resident
/// and aligned. Locked variants retain their original registry admission.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_registry_state_test(op: u8, value: u8) -> bool {
    match op {
        0 => visible(value),
        1 => owns_payload(value),
        2 => running(value),
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_handle_test(op: u8, a: Handle, b: Handle) -> bool {
    match op {
        0 => valid(a),
        1 => equal(a, b),
        _ => unreachable!(),
    }
}
/// # Safety
/// Caller holds the original registry mutex/short stable-record borrow. Object
/// input matches op1 slot/op2 instance; all returned pointers remain borrowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_lookup(
    op: u8,
    id: u32,
    h: Handle,
    object: *mut c_void,
    flag: bool,
) -> *mut c_void {
    unsafe {
        match op {
            0 => free_slot().cast(),
            1 => chunk_for_slot(object.cast()).cast(),
            2 => slot_for_instance(object.cast()).cast(),
            3 => lookup_id(id, flag).cast(),
            4 => lookup_handle(h, flag).cast(),
            5 => completion_handle(h).cast(),
            6 => completion_id(id).cast(),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Ops0..3 retain caller's registry admission; op0 also needs exact chunk.
/// Op4 acquires and releases the registry itself, matching the original API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_predicate(
    op: u8,
    id: u32,
    chunk: *const ProgramRegistryChunk,
) -> bool {
    unsafe {
        match op {
            0 => chunk_empty(chunk),
            1 => has_id(id),
            2 => !completion_id(id).is_null(),
            3 => id_in_use(id),
            4 => {
                let Some(_guard) = Guard::acquire() else {
                    return false;
                };
                let slot = lookup_id(id, true);
                !slot.is_null() && owns_payload((*slot).state)
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Ops0/1 need exact slot/reservation pointer under original lifetime; op2 an
/// instance pointer pinned by caller. Output is exclusive aligned handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_handle(
    op: u8,
    object: *const c_void,
    id: u32,
    out: *mut Handle,
) -> bool {
    unsafe {
        let value = match op {
            0 => handle_for_slot(object.cast()),
            1 => handle_for_reservation(object.cast()),
            2 | 3 => {
                let Some(_guard) = Guard::acquire() else {
                    return false;
                };
                let slot = if op == 2 {
                    slot_for_instance(object.cast())
                } else {
                    lookup_id(id, true)
                };
                if slot.is_null() || (op == 2 && matches!((*slot).state, FREE | REAP)) {
                    return false;
                }
                handle_for_slot(slot)
            }
            _ => unreachable!(),
        };
        out.write(value);
        true
    }
}
/// # Safety
/// Ops3..6/12 need original caller-held registry admission. Failure seam setters
/// acquire it themselves. Value7 uses MAX for absent, otherwise exact u32.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_control(op: u8, value: u64) -> u64 {
    unsafe {
        match op {
            0 => u64::from(lock()),
            1 => {
                unlock();
                0
            }
            2 => u64::from(sync_locks::r4os_rust_sync_mutex((*state()).mutex, 0, 0)),
            3 => generation(),
            4 => u64::from(allocate_id()),
            5 => published_count(),
            6 => {
                epoch();
                0
            }
            7 => {
                let Some(_guard) = Guard::acquire() else {
                    return 0;
                };
                (*state()).failure_after_present = value != u64::MAX;
                (*state()).failure_after = if value == u64::MAX {
                    0
                } else {
                    u32::try_from(value).unwrap()
                };
                (*state()).failure_cursor = 0;
                0
            }
            8 => {
                let Some(_guard) = Guard::acquire() else {
                    return 0;
                };
                (*state()).next_instance_id = if value == 0 {
                    1
                } else {
                    u32::try_from(value).unwrap()
                };
                0
            }
            9 => {
                let Some(_guard) = Guard::acquire() else {
                    return 0;
                };
                (*state()).next_generation = value;
                (*state()).generation_exhausted = value == 0;
                (*stats()).next_generation = value;
                0
            }
            10 => {
                let Some(_guard) = Guard::acquire() else {
                    return 0;
                };
                if (*state()).next_growth_failure {
                    return 0;
                }
                (*state()).next_growth_failure = true;
                1
            }
            11 => {
                let Some(_guard) = Guard::acquire() else {
                    return 0;
                };
                (*state()).next_growth_failure = false;
                0
            }
            12 => u64::from(should_fail_growth()),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Selected operation retains its original resident/pinned input and C output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_candidate(
    out: *mut *mut ProgramRegistryChunk,
) -> u8 {
    unsafe { candidate(out) }
}
/// # Safety
/// Same original registry admission; lower pressure cleanup runs without lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_grow() -> u8 {
    unsafe { grow() }
}
/// # Safety
/// Exact live chunk. Link/unlink ops0/1 hold registry; free op2 holds no lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_chunk(
    op: u8,
    chunk: *mut ProgramRegistryChunk,
) -> bool {
    unsafe {
        match op {
            0 => {
                link_chunk(chunk);
                true
            }
            1 => unlink_chunk(chunk),
            2 => free_chunk(chunk),
            _ => unreachable!(),
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_registry_shrink(pressure: bool) {
    unsafe { shrink(pressure) }
}
/// # Safety
/// Aligned exclusive reservation output; does not publish on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_reserve(
    out: *mut ProgramInstanceReservation,
) -> u8 {
    unsafe { reserve(out) }
}
/// # Safety
/// Exact stable reservation identity and exclusive original teardown admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_reservation(
    op: u8,
    r: *const ProgramInstanceReservation,
) -> u8 {
    unsafe {
        match op {
            0 => {
                cancel(r);
                0
            }
            1 => finish_cancel(r),
            2 => u8::from(publish(r)),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exact stable slot; original caller holds registry admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_clear(slot: *mut ProgramRegistrySlot) {
    unsafe { clear_slot(slot) }
}
/// # Safety
/// Resident request/reservation; the original caller keeps its spawn unwind and
/// exact resource ownership while allocation/current-owner lookup may wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_completion_allocate(
    r: *const CompletionRequest,
) -> bool {
    unsafe { allocate_completion(&*r) }
}
/// # Safety
/// Exact resident completion. Link/unlink ops0/1 hold registry; free op2 is only
/// after original detachment/ref acknowledgments and holds no registry lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_completion(
    op: u8,
    node: *mut ProgramCompletionNode,
) -> bool {
    unsafe {
        match op {
            0 => {
                link_completion(node);
                true
            }
            1 => unlink_completion(node),
            2 => {
                free_completion(node);
                true
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Stable pinned instance; registry is acquired for exact identity validation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_done(instance: *mut ProgramInstance) {
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        let slot = slot_for_instance(instance);
        if !slot.is_null()
            && matches!((*slot).state, PUBLISH | RUN | EXIT)
            && (*slot).public_id == (*instance).id
        {
            (*slot).state = DONE;
            epoch();
        }
    }
}
/// # Safety
/// Out is exclusive aligned lease; returned slot/instance retain one exact pin.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_pin(
    h: Handle,
    include_done: bool,
    out: *mut ProgramInstanceLease,
) -> bool {
    unsafe { pin(h, include_done, out) }
}
/// # Safety
/// Exclusive aligned output; ID lookup is revalidated by generation after lock
/// release, preserving the original bounded two-step pin transaction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_pin_id(
    id: u32,
    out: *mut ProgramInstanceLease,
) -> bool {
    unsafe { pin_id(id, out) }
}
/// # Safety
/// Exact live lease token; slot remains resident until this pin is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_unpin(lease: *const ProgramInstanceLease) {
    unsafe { unpin(lease) }
}
/// # Safety
/// Ops0/1 caller holds registry; op0 needs stable slot. Queue op2/run op3 acquire
/// their own registry admission; events are signaled only after unlock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_retire(
    op: u8,
    slot: *mut ProgramRegistrySlot,
    h: Handle,
) -> *mut ProgramRegistrySlot {
    unsafe {
        match op {
            0 => {
                if enqueue(slot) {
                    slot
                } else {
                    ptr::null_mut()
                }
            }
            1 => take(),
            2 => {
                if !lock() {
                    return ptr::null_mut();
                }
                let slot = lookup_handle(h, true);
                let queued = !slot.is_null() && (*slot).state == RETIRE && enqueue(slot);
                unlock();
                if queued {
                    signal();
                    slot
                } else {
                    ptr::null_mut()
                }
            }
            3 => {
                let Some(_guard) = Guard::acquire() else {
                    return ptr::null_mut();
                };
                let slot = lookup_handle(h, false);
                if !slot.is_null() && (*slot).state == PUBLISH {
                    (*slot).state = RUN;
                    epoch();
                }
                ptr::null_mut()
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Caller holds original registry admission and provides exact C handle value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_missing(h: Handle) -> i32 {
    unsafe { missing(h) }
}
/// # Safety
/// Exclusive aligned statistics output; no pointer/lease escapes this snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_snapshot(out: *mut ProgramRegistryStats) {
    unsafe { snapshot(out) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_registry_mix(value: u64) -> u64 {
    mix(value)
}
#[repr(C)]
pub struct Iterator {
    pub chunk: *mut ProgramRegistryChunk,
    pub slot_index: usize,
    pub include_done: bool,
}
/// # Safety
/// Same short cooperative/registry-pinned scan as original: no yield, wait,
/// reclaim or mutation may invalidate the borrowed chunk/slot during next.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_iterator(
    it: *mut Iterator,
) -> *mut ProgramInstance {
    unsafe {
        while !(*it).chunk.is_null() {
            let chunk = (*it).chunk;
            while (*it).slot_index < 16 {
                let slot = ptr::addr_of_mut!((*chunk).slots[(*it).slot_index]);
                (*it).slot_index += 1;
                if running((*slot).state)
                    || ((*it).include_done && matches!((*slot).state, EXIT | DONE))
                {
                    return ptr::addr_of_mut!((*slot).instance);
                }
            }
            (*it).chunk = (*chunk).next;
            (*it).slot_index = 0;
        }
        ptr::null_mut()
    }
}
const _: () = {
    assert!(mem::size_of::<CompletionRequest>() == 40);
    assert!(mem::offset_of!(CompletionRequest, owner_handle) == 16);
    assert!(mem::size_of::<Iterator>() == 24);
};

/// # Safety
/// Exclusive resident iterator output. Its original short cooperative scan
/// cannot retain a borrowed chunk through reclaim, waits or registry mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_registry_iterator_new(
    include_done: bool,
    out: *mut Iterator,
) {
    unsafe {
        out.write(Iterator {
            chunk: (*state()).head,
            slot_index: 0,
            include_done,
        });
    }
}
