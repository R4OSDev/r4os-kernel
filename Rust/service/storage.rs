//! Address-stable service records. Allocation and destruction run outside
//! registry/runtime admission; intrusive indexes never move their payloads.
use super::registry_types::{Endpoint, Entry};
use crate::{
    arch::runtime::RuntimeGuard,
    driver::treap::{Node, Tree},
    memory::heap,
    sched::task_context,
};
use core::{
    mem::{align_of, offset_of, size_of},
    ptr,
    sync::atomic::AtomicUsize,
};

const EMPTY_TREE: Tree = Tree {
    root: ptr::null_mut(),
    xorshift: 0,
};

pub(super) struct Operation(task_context::Unwind);
impl Operation {
    pub(super) fn enter() -> Option<Self> {
        let _guard = RuntimeGuard::acquire();
        let token = unsafe { task_context::enter() };
        if token.admitted() {
            Some(Self(token))
        } else {
            None
        }
    }
    pub(super) fn release(&mut self) {
        let _guard = RuntimeGuard::acquire();
        assert!(unsafe { task_context::leave(self.0) });
        self.0 = task_context::Unwind::EMPTY;
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.release();
    }
}

/// Unpublished zeroed resident storage, retained through publication or rollback.
pub(super) struct Prepared<T> {
    pointer: *mut T,
    _operation: Operation,
}
impl<T> Prepared<T> {
    /// T must permit all-zero initialization and own no unpublished resources.
    pub(super) unsafe fn new() -> Option<Self> {
        let operation = Operation::enter()?;
        let pointer = heap::r4os_rust_heap_alloc(size_of::<T>(), align_of::<T>()).cast::<T>();
        if pointer.is_null() {
            return None;
        }
        // T is one of the two private all-zero-valid records below. In-place
        // initialization avoids an Endpoint-sized temporary on a 64-KB stack.
        unsafe { ptr::write_bytes(pointer, 0, 1) };
        Some(Self {
            pointer,
            _operation: operation,
        })
    }
    pub(super) fn pointer(&self) -> *mut T {
        self.pointer
    }
    pub(super) fn publish(&mut self) -> *mut T {
        core::mem::replace(&mut self.pointer, ptr::null_mut())
    }
}
impl<T> Drop for Prepared<T> {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            assert_eq!(
                unsafe { heap::r4os_rust_heap_free(self.pointer.cast(), size_of::<T>()) },
                0,
                "Unpublished service allocation release failed"
            );
        }
    }
}

#[repr(C)]
pub(super) struct ServiceNode {
    pub index: Node,
    name_index: Node,
    same_hash: *mut ServiceNode,
    pub retired_next: *mut ServiceNode,
    pub entry: Entry,
}

pub(super) struct Catalog {
    ids: Tree,
    names: Tree,
    pub count: usize,
    pub serial: usize,
    pub retired: *mut ServiceNode,
}
impl Catalog {
    pub const EMPTY: Self = Self {
        ids: EMPTY_TREE,
        names: EMPTY_TREE,
        count: 0,
        serial: 0,
        retired: ptr::null_mut(),
    };
    pub unsafe fn get(&self, id: usize) -> *mut ServiceNode {
        unsafe { self.ids.find(id as u64).cast() }
    }
    pub unsafe fn after(&self, id: usize) -> *mut ServiceNode {
        unsafe { after(&self.ids, id as u64).cast() }
    }
    pub unsafe fn at(&self, mut index: u32) -> *mut ServiceNode {
        let mut node = unsafe { self.after(0) };
        while !node.is_null() && index != 0 {
            index -= 1;
            node = unsafe { self.after((*node).index.key as usize) };
        }
        node
    }
    pub unsafe fn find(&self, name: &[u8]) -> *mut ServiceNode {
        if name.is_empty() {
            return ptr::null_mut();
        }
        unsafe {
            let index = self.names.find(name_hash(name));
            if index.is_null() {
                return ptr::null_mut();
            }
            let mut node = index
                .cast::<u8>()
                .sub(offset_of!(ServiceNode, name_index))
                .cast::<ServiceNode>();
            while !node.is_null() {
                let e = &(*node).entry;
                if e.name[..e.name_len].eq_ignore_ascii_case(name) {
                    return node;
                }
                node = (*node).same_hash;
            }
            ptr::null_mut()
        }
    }
    pub unsafe fn insert(&mut self, node: *mut ServiceNode) -> Option<usize> {
        let id = self.serial.checked_add(1)?;
        let count = self.count.checked_add(1)?;
        unsafe {
            let e = &(*node).entry;
            let hash = name_hash(&e.name[..e.name_len]);
            let head_index = self.names.find(hash);
            if head_index.is_null() {
                self.names
                    .insert(hash, ptr::addr_of_mut!((*node).name_index));
            } else {
                let head = head_index
                    .cast::<u8>()
                    .sub(offset_of!(ServiceNode, name_index))
                    .cast::<ServiceNode>();
                (*node).same_hash = (*head).same_hash;
                (*head).same_hash = node;
            }
            self.ids.insert(id as u64, ptr::addr_of_mut!((*node).index));
        }
        self.serial = id;
        self.count = count;
        Some(id)
    }
    pub unsafe fn remove(&mut self, id: usize) -> bool {
        unsafe {
            let node = self.get(id);
            if node.is_null() {
                return false;
            }
            let e = &(*node).entry;
            let hash = name_hash(&e.name[..e.name_len]);
            if (*node).name_index.priority != 0 {
                self.names.remove(ptr::addr_of_mut!((*node).name_index));
                if !(*node).same_hash.is_null() {
                    self.names
                        .insert(hash, ptr::addr_of_mut!((*(*node).same_hash).name_index));
                }
            } else {
                let index = self.names.find(hash);
                assert!(!index.is_null());
                let mut previous = index
                    .cast::<u8>()
                    .sub(offset_of!(ServiceNode, name_index))
                    .cast::<ServiceNode>();
                while (*previous).same_hash != node {
                    previous = (*previous).same_hash;
                    assert!(!previous.is_null());
                }
                (*previous).same_hash = (*node).same_hash;
            }
            self.ids.remove(ptr::addr_of_mut!((*node).index));
            (*node).entry.used = false;
            (*node).retired_next = self.retired;
            self.retired = node;
            self.count -= 1;
            true
        }
    }
}

#[repr(C)]
pub(super) struct EndpointNode {
    // Endpoint address is also the allocation address; private fixture layouts
    // of Endpoint and its embedded synchronization records remain unchanged.
    pub endpoint: Endpoint,
    pub index: Node,
    pub pins: AtomicUsize,
    pub retired_next: *mut EndpointNode,
}

pub(super) struct Endpoints {
    active: Tree,
    pub retired: *mut EndpointNode,
    pub count: usize,
    pub allocated: usize,
    pub serial: u32,
}
impl Endpoints {
    pub const EMPTY: Self = Self {
        active: EMPTY_TREE,
        retired: ptr::null_mut(),
        count: 0,
        allocated: 0,
        serial: 0,
    };
    pub unsafe fn get(&self, handle: u32) -> *mut EndpointNode {
        unsafe { endpoint_node(self.active.find(u64::from(handle))) }
    }
    pub unsafe fn after(&self, handle: u32) -> *mut EndpointNode {
        unsafe { endpoint_node(after(&self.active, u64::from(handle))) }
    }
    pub unsafe fn for_service(&self, service: usize) -> *mut EndpointNode {
        let mut node = unsafe { self.after(0) };
        while !node.is_null() {
            unsafe {
                if (*node).endpoint.service_slot == service {
                    return node;
                }
                node = self.after((*node).endpoint.handle);
            }
        }
        ptr::null_mut()
    }
    pub unsafe fn insert(&mut self, node: *mut EndpointNode) -> Option<u32> {
        let handle = self.serial.checked_add(1)?;
        let count = self.count.checked_add(1)?;
        let allocated = self.allocated.checked_add(1)?;
        unsafe {
            (*node).endpoint.handle = handle;
            (*node).endpoint.generation = u64::from(handle);
            (*node).endpoint.used = true;
            self.active
                .insert(u64::from(handle), ptr::addr_of_mut!((*node).index));
        }
        self.serial = handle;
        self.count = count;
        self.allocated = allocated;
        Some(handle)
    }
    pub unsafe fn remove(&mut self, endpoint: *mut Endpoint) {
        unsafe {
            let node = endpoint.cast::<EndpointNode>();
            assert!(self.get((*endpoint).handle) == node);
            self.active.remove(ptr::addr_of_mut!((*node).index));
            (*endpoint).used = false;
            (*node).retired_next = self.retired;
            self.retired = node;
            self.count -= 1;
        }
    }
}

unsafe fn endpoint_node(index: *mut Node) -> *mut EndpointNode {
    if index.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        index
            .cast::<u8>()
            .sub(offset_of!(EndpointNode, index))
            .cast()
    }
}

/// Upper bound by original registration/endpoint identity; holes are not rows.
unsafe fn after(tree: &Tree, key: u64) -> *mut Node {
    let mut node = tree.root;
    let mut result = ptr::null_mut();
    while !node.is_null() {
        unsafe {
            if (*node).key > key {
                result = node;
                node = (*node).children[0];
            } else {
                node = (*node).children[1];
            }
        }
    }
    result
}

fn name_hash(name: &[u8]) -> u64 {
    name.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x100_0000_01b3)
    })
}
