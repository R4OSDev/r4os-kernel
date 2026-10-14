//! Intrusive wait linkage. The exact task and queue remain stable until detach;
//! callers hold the existing scheduler runtime owner. No allocation/callback.
use core::ptr::null_mut;

#[repr(C)]
pub struct Node {
    pub owner: *mut core::ffi::c_void,
    pub owner_generation: u64,
    pub queue: *mut QueueCore,
    pub prev: *mut Node,
    pub next: *mut Node,
}

#[repr(C)]
pub struct QueueCore {
    pub head: *mut Node,
    pub tail: *mut Node,
    pub count: usize,
    pub closing: bool,
    pub generation: u64,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            owner: null_mut(),
            owner_generation: 0,
            queue: null_mut(),
            prev: null_mut(),
            next: null_mut(),
        }
    }
}

impl Default for QueueCore {
    fn default() -> Self {
        Self {
            head: null_mut(),
            tail: null_mut(),
            count: 0,
            closing: false,
            generation: 1,
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
    unsafe {
        if (*queue).closing || owner_generation == 0 || !(*node).queue.is_null() {
            return false;
        }
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
            || (*queue).count != 0
            || !(*queue).head.is_null()
            || !(*queue).tail.is_null()
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
        if !(*node).queue.is_null() {
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
    assert!(core::mem::size_of::<Node>() == 40);
    assert!(core::mem::size_of::<QueueCore>() == 40);
    assert!(core::mem::offset_of!(Node, queue) == 16);
    assert!(core::mem::offset_of!(QueueCore, closing) == 24);
    assert!(core::mem::offset_of!(QueueCore, generation) == 32);
};
