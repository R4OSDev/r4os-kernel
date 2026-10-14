//! Resident intrusive treap. The original x86_64 Zig treap's rotations,
//! address-seeded xorshift priority and deletion tie rule are preserved.
use core::ptr;
#[repr(C)]
pub struct Node {
    pub key: u64,
    pub priority: usize,
    pub parent: *mut Node,
    pub children: [*mut Node; 2],
}
#[repr(C)]
pub struct Tree {
    pub root: *mut Node,
    pub xorshift: usize,
}
impl Default for Tree {
    fn default() -> Self {
        Self {
            root: ptr::null_mut(),
            xorshift: 0,
        }
    }
}
impl Tree {
    /// # Safety
    /// The caller serializes the tree and keeps all indexed nodes resident.
    pub unsafe fn find(&self, key: u64) -> *mut Node {
        let mut n = self.root;
        while !n.is_null() {
            unsafe {
                if (*n).key == key {
                    break;
                }
                n = (*n).children[usize::from(key > (*n).key)]
            }
        }
        n
    }
    /// # Safety
    /// The caller serializes the tree and keeps all indexed nodes resident.
    pub unsafe fn min(&self) -> *mut Node {
        let mut n = self.root;
        if n.is_null() {
            return n;
        }
        unsafe {
            while !(*n).children[0].is_null() {
                n = (*n).children[0]
            }
        }
        n
    }
    unsafe fn rotate(&mut self, n: *mut Node, right: bool) {
        unsafe {
            let parent = (*n).parent;
            let target = (*n).children[usize::from(!right)];
            assert!(!target.is_null());
            let adjacent = (*target).children[usize::from(right)];
            (*target).children[usize::from(right)] = n;
            (*n).children[usize::from(!right)] = adjacent;
            (*n).parent = target;
            (*target).parent = parent;
            if !adjacent.is_null() {
                (*adjacent).parent = n
            }
            let link = if parent.is_null() {
                &mut self.root
            } else {
                &mut (*parent).children[usize::from((*parent).children[1] == n)]
            };
            assert!(*link == n);
            *link = target;
        }
    }
    /// # Safety
    /// The key is absent and `n` is exclusive resident node storage, disjoint
    /// from the tree and all indexed nodes, until its confirmed removal.
    pub unsafe fn insert(&mut self, key: u64, n: *mut Node) {
        unsafe {
            let mut parent = ptr::null_mut();
            let mut current = self.root;
            while !current.is_null() {
                assert!((*current).key != key);
                parent = current;
                current = (*current).children[usize::from(key > (*current).key)]
            }
            if self.xorshift == 0 {
                self.xorshift = n as usize
            }
            self.xorshift ^= self.xorshift >> 13;
            self.xorshift ^= self.xorshift << 7;
            self.xorshift ^= self.xorshift >> 17;
            assert!(self.xorshift != 0);
            ptr::write(
                n,
                Node {
                    key,
                    priority: self.xorshift,
                    parent,
                    children: [ptr::null_mut(); 2],
                },
            );
            let link = if parent.is_null() {
                &mut self.root
            } else {
                &mut (*parent).children[usize::from(key > (*parent).key)]
            };
            assert!(link.is_null());
            *link = n;
            while !(*n).parent.is_null() {
                let p = (*n).parent;
                if (*p).priority <= (*n).priority {
                    break;
                }
                let is_right = (*p).children[1] == n;
                assert!((*p).children[usize::from(is_right)] == n);
                self.rotate(p, !is_right)
            }
        }
    }
    /// # Safety
    /// The caller serializes the tree; `n` is a live node in this exact tree.
    pub unsafe fn remove(&mut self, n: *mut Node) {
        unsafe {
            assert!((*n).priority != 0);
            while !(*n).children[0].is_null() || !(*n).children[1].is_null() {
                let left = (*n).children[0];
                let right = (*n).children[1];
                let rotate_right = if right.is_null() {
                    true
                } else if left.is_null() {
                    false
                } else {
                    (*left).priority < (*right).priority
                };
                self.rotate(n, rotate_right);
            }
            let parent = (*n).parent;
            let link = if parent.is_null() {
                &mut self.root
            } else {
                &mut (*parent).children[usize::from((*parent).children[1] == n)]
            };
            assert!(*link == n);
            *link = ptr::null_mut();
            (*n).priority = 0;
            (*n).parent = ptr::null_mut();
            (*n).children = [ptr::null_mut(); 2];
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<Node>() == 40);
    assert!(core::mem::size_of::<Tree>() == 16);
};
