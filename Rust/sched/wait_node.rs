//! Intrusive wait linkage. The exact task and queue remain stable until detach;
//! callers hold the existing scheduler runtime owner. No allocation/callback.
#[cfg(test)]
#[path = "wait_node/host_tests.rs"]
mod host_tests;
use crate::sched::task_index as index;
use core::{
    mem::offset_of,
    ptr::{addr_of_mut, null_mut},
};

#[repr(C)]
pub struct Node {
    pub owner: *mut core::ffi::c_void,
    pub owner_generation: u64,
    pub queue: *mut QueueCore,
    pub prev: *mut Node,
    pub next: *mut Node,
    priority_links: index::Links,
    order: u64,
    rank: u8,
    indexed: bool,
}

#[repr(C)]
pub struct QueueCore {
    pub head: *mut Node,
    pub tail: *mut Node,
    pub count: usize,
    pub closing: bool,
    draining: bool,
    pub generation: u64,
    priority: index::Index,
    next_order: u64,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            owner: null_mut(),
            owner_generation: 0,
            queue: null_mut(),
            prev: null_mut(),
            next: null_mut(),
            priority_links: index::Links::default(),
            order: 0,
            rank: 2,
            indexed: false,
        }
    }
}

impl Default for QueueCore {
    fn default() -> Self {
        Self::EMPTY
    }
}
impl QueueCore {
    pub const EMPTY: Self = Self {
        head: null_mut(),
        tail: null_mut(),
        count: 0,
        closing: false,
        draining: false,
        generation: 1,
        priority: index::Index {
            root: null_mut(),
            first: null_mut(),
            count: 0,
            visits: 0,
        },
        next_order: 0,
    };
}

const PRIORITY_OFFSET: usize = offset_of!(Node, priority_links);
unsafe extern "C" fn before(a: *const core::ffi::c_void, b: *const core::ffi::c_void) -> bool {
    unsafe {
        let a = a.cast::<Node>();
        let b = b.cast::<Node>();
        ((*a).rank, (*a).order) < ((*b).rank, (*b).order)
    }
}
// FIFO-relative order is unchanged by compacting every linked key together.
// No rotation, observer, allocation, callback or wait occurs at this rare wrap.
unsafe fn next_order(queue: *mut QueueCore) -> Option<u64> {
    unsafe {
        if (*queue).count == 0 {
            (*queue).next_order = 0;
        } else if (*queue).next_order == u64::MAX {
            let mut cursor = (*queue).head;
            let mut order = 0u64;
            while !cursor.is_null() {
                (*cursor).order = order;
                order = order.checked_add(1)?;
                cursor = (*cursor).next;
            }
            (*queue).next_order = order;
        }
        let value = (*queue).next_order;
        (*queue).next_order = value.checked_add(1)?;
        Some(value)
    }
}

/// # Safety
/// Runtime retains the linked node/queue. The owner publishes every actual
/// wake-rank change here, including changes with an unchanged dispatch rank.
pub(crate) unsafe fn set_rank(node: *mut Node, rank: u8) {
    assert!(rank < 4);
    unsafe {
        if (*node).rank == rank {
            return;
        }
        let queue = (*node).queue;
        if !queue.is_null() && (*node).indexed && !(*queue).draining {
            index::remove_quiet(
                addr_of_mut!((*queue).priority),
                node.cast(),
                PRIORITY_OFFSET,
            );
            (*node).rank = rank;
            index::insert_quiet(
                addr_of_mut!((*queue).priority),
                node.cast(),
                PRIORITY_OFFSET,
                before,
            );
        } else {
            (*node).rank = rank;
        }
    }
}

/// # Safety
/// Caller retains the same Runtime-owned queue and exact node lifetimes.
pub(crate) unsafe fn first_ranked(queue: *mut QueueCore) -> *mut Node {
    unsafe { (*queue).priority.first.cast() }
}

/// # Safety
/// Exclusive Runtime-owned full drain. Waking cannot dispatch or enroll a
/// producer in this queue before end_drain. Keep broadcast removal linear.
pub(crate) unsafe fn begin_drain(queue: *mut QueueCore) -> bool {
    unsafe {
        if (*queue).count <= 1 {
            return false;
        }
        assert!(!(*queue).draining);
        (*queue).draining = true;
        (*queue).priority = index::Index::default();
        true
    }
}
/// # Safety
/// Matching exclusive drain has removed every FIFO node without dispatch.
pub(crate) unsafe fn end_drain(queue: *mut QueueCore, bulk: bool) {
    if bulk {
        unsafe {
            assert!((*queue).count == 0 && (*queue).priority.count == 0);
            (*queue).draining = false;
        }
    }
}

fn advance_generation(queue: &mut QueueCore) {
    queue.generation = queue.generation.wrapping_add(1);
    if queue.generation == 0 {
        queue.generation = 1;
    }
}

/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
pub unsafe fn link(
    queue: *mut QueueCore,
    node: *mut Node,
    owner: *mut core::ffi::c_void,
    owner_generation: u64,
) -> bool {
    unsafe { link_ranked(queue, node, owner, owner_generation, 2) }
}

/// # Safety
/// Same resident FIFO enrollment contract. Task wait owners supply their
/// current wake rank and retain the publication edge until exact detach.
pub(crate) unsafe fn link_ranked(
    queue: *mut QueueCore,
    node: *mut Node,
    owner: *mut core::ffi::c_void,
    owner_generation: u64,
    rank: u8,
) -> bool {
    unsafe {
        if (*queue).closing
            || (*queue).draining
            || owner_generation == 0
            || rank >= 4
            || !(*node).queue.is_null()
            || (*node).indexed
        {
            return false;
        }
        let Some(order) = next_order(queue) else {
            return false;
        };
        (*node).owner = owner;
        (*node).owner_generation = owner_generation;
        (*node).queue = queue;
        (*node).prev = (*queue).tail;
        (*node).next = null_mut();
        if (*queue).tail.is_null() {
            (*queue).head = node;
        } else {
            (*(*queue).tail).next = node;
        }
        (*queue).tail = node;
        (*queue).count += 1;
        (*node).order = order;
        (*node).rank = rank;
        index::insert_quiet(
            addr_of_mut!((*queue).priority),
            node.cast(),
            PRIORITY_OFFSET,
            before,
        );
        (*node).indexed = true;
        true
    }
}

/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
pub unsafe fn detach(node: *mut Node) -> bool {
    unsafe {
        let queue = (*node).queue;
        if queue.is_null() {
            return false;
        }
        if (*node).indexed {
            if !(*queue).draining {
                index::remove_quiet(
                    addr_of_mut!((*queue).priority),
                    node.cast(),
                    PRIORITY_OFFSET,
                );
            } else {
                (*node).priority_links = index::Links::default();
            }
            (*node).indexed = false;
        }
        if (*node).prev.is_null() {
            (*queue).head = (*node).next;
        } else {
            (*(*node).prev).next = (*node).next;
        }
        if (*node).next.is_null() {
            (*queue).tail = (*node).prev;
        } else {
            (*(*node).next).prev = (*node).prev;
        }
        if (*queue).count != 0 {
            (*queue).count -= 1;
        }
        (*node).queue = null_mut();
        (*node).prev = null_mut();
        (*node).next = null_mut();
        true
    }
}

/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
pub unsafe fn pop_front(queue: *mut QueueCore) -> *mut Node {
    unsafe {
        let node = (*queue).head;
        if !node.is_null() {
            detach(node);
        }
        node
    }
}

/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
pub unsafe fn close(queue: *mut QueueCore) {
    unsafe {
        if !(*queue).closing {
            (*queue).closing = true;
            advance_generation(&mut *queue);
        }
    }
}

/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
pub unsafe fn reopen(queue: *mut QueueCore) -> bool {
    unsafe {
        if !(*queue).closing
            || (*queue).draining
            || (*queue).count != 0
            || !(*queue).head.is_null()
            || !(*queue).tail.is_null()
            || !(*queue).priority.root.is_null()
            || !(*queue).priority.first.is_null()
            || (*queue).priority.count != 0
        {
            return false;
        }
        (*queue).closing = false;
        advance_generation(&mut *queue);
        true
    }
}

/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
pub unsafe fn clear_owner(node: *mut Node) -> bool {
    unsafe {
        if !(*node).queue.is_null() || (*node).indexed {
            return false;
        }
        (*node).owner_generation = 0;
        (*node).owner = null_mut();
        (*node).prev = null_mut();
        (*node).next = null_mut();
        true
    }
}

/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wait_node_link(
    queue: *mut QueueCore,
    node: *mut Node,
    owner: *mut core::ffi::c_void,
    generation: u64,
) -> bool {
    unsafe { link(queue, node, owner, generation) }
}
/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wait_node_detach(node: *mut Node) -> bool {
    unsafe { detach(node) }
}
/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wait_node_pop_front(queue: *mut QueueCore) -> *mut Node {
    unsafe { pop_front(queue) }
}
/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wait_node_close(queue: *mut QueueCore) {
    unsafe { close(queue) }
}
/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wait_node_reopen(queue: *mut QueueCore) -> bool {
    unsafe { reopen(queue) }
}
/// # Safety
/// Caller serializes aligned resident queue/node storage. Linked owner and
/// generation remain live until detach; enrollment supplies a non-null owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_wait_node_clear_owner(node: *mut Node) -> bool {
    unsafe { clear_owner(node) }
}

const _: () = {
    assert!(core::mem::size_of::<Node>() == 88);
    assert!(core::mem::size_of::<QueueCore>() == 80);
    assert!(core::mem::offset_of!(Node, queue) == 16);
    assert!(core::mem::offset_of!(QueueCore, closing) == 24);
    assert!(core::mem::offset_of!(QueueCore, generation) == 32);
};
