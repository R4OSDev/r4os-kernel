//! Intrusive AVL owner: nodes stay at stable addresses; rotations change only
//! links. Registry runtime admission is supplied by the caller, no allocation.
use core::{cmp::Ordering, ffi::c_void, ptr::null_mut};
type Node = *mut c_void;
type Less = unsafe extern "C" fn(*const c_void, *const c_void) -> bool;

#[repr(C)]
pub struct Links {
    pub parent: Node,
    pub left: Node,
    pub right: Node,
    pub height: u8,
}
#[repr(C)]
#[derive(Default)]
pub struct Index {
    pub root: Node,
    pub first: Node,
    pub count: usize,
    pub visits: u64,
}
#[repr(C)]
#[derive(Default)]
pub struct Neighbors {
    pub previous: Node,
    pub following: Node,
}
impl Default for Links {
    fn default() -> Self {
        Self {
            parent: null_mut(),
            left: null_mut(),
            right: null_mut(),
            height: 1,
        }
    }
}
unsafe fn links(node: Node, offset: usize) -> *mut Links {
    unsafe { node.cast::<u8>().add(offset).cast() }
}
unsafe fn height(node: Node, offset: usize) -> u8 {
    if node.is_null() {
        0
    } else {
        unsafe { (*links(node, offset)).height }
    }
}
unsafe fn update(node: Node, offset: usize) {
    unsafe {
        let link = links(node, offset);
        (*link).height = 1 + height((*link).left, offset).max(height((*link).right, offset));
    }
}
unsafe fn balance(node: Node, offset: usize) -> i16 {
    unsafe {
        let link = links(node, offset);
        i16::from(height((*link).left, offset)) - i16::from(height((*link).right, offset))
    }
}
unsafe fn replace(index: *mut Index, old: Node, replacement: Node, offset: usize) {
    unsafe {
        let parent = (*links(old, offset)).parent;
        if parent.is_null() {
            (*index).root = replacement;
        } else if (*links(parent, offset)).left == old {
            (*links(parent, offset)).left = replacement;
        } else {
            (*links(parent, offset)).right = replacement;
        }
        if !replacement.is_null() {
            (*links(replacement, offset)).parent = parent;
        }
    }
}
unsafe fn rotate_left(index: *mut Index, node: Node, offset: usize) -> Node {
    unsafe {
        let pivot = (*links(node, offset)).right;
        let middle = (*links(pivot, offset)).left;
        replace(index, node, pivot, offset);
        (*links(pivot, offset)).left = node;
        (*links(node, offset)).parent = pivot;
        (*links(node, offset)).right = middle;
        if !middle.is_null() {
            (*links(middle, offset)).parent = node;
        }
        update(node, offset);
        update(pivot, offset);
        pivot
    }
}
unsafe fn rotate_right(index: *mut Index, node: Node, offset: usize) -> Node {
    unsafe {
        let pivot = (*links(node, offset)).left;
        let middle = (*links(pivot, offset)).right;
        replace(index, node, pivot, offset);
        (*links(pivot, offset)).right = node;
        (*links(node, offset)).parent = pivot;
        (*links(node, offset)).left = middle;
        if !middle.is_null() {
            (*links(middle, offset)).parent = node;
        }
        update(node, offset);
        update(pivot, offset);
        pivot
    }
}
unsafe fn rebalance(index: *mut Index, start: Node, offset: usize) {
    unsafe {
        let mut cursor = start;
        while !cursor.is_null() {
            let node = cursor;
            (*index).visits = (*index).visits.wrapping_add(1);
            update(node, offset);
            let mut top = node;
            if balance(node, offset) > 1 {
                if balance((*links(node, offset)).left, offset) < 0 {
                    rotate_left(index, (*links(node, offset)).left, offset);
                }
                top = rotate_right(index, node, offset);
            } else if balance(node, offset) < -1 {
                if balance((*links(node, offset)).right, offset) > 0 {
                    rotate_right(index, (*links(node, offset)).right, offset);
                }
                top = rotate_left(index, node, offset);
            }
            cursor = (*links(top, offset)).parent;
        }
    }
}
/// Search an existing stable index by a scalar key, without constructing or
/// dereferencing a foreign probe record. `compare` orders each resident node
/// against the key. Last-equal selects the newest generation of an ID prefix.
/// # Safety
/// Caller retains the index owner and live nodes with Links at `offset`.
/// Key comparison is pure, respects the index ordering and cannot reschedule.
pub(crate) unsafe fn lookup(
    index: *mut Index,
    offset: usize,
    mut compare: impl FnMut(Node) -> Ordering,
    last_equal: bool,
) -> Node {
    unsafe {
        let mut result = null_mut();
        let mut cursor = (*index).root;
        while !cursor.is_null() {
            (*index).visits = (*index).visits.wrapping_add(1);
            match compare(cursor) {
                Ordering::Less => cursor = (*links(cursor, offset)).right,
                Ordering::Greater => cursor = (*links(cursor, offset)).left,
                Ordering::Equal => {
                    if !last_equal {
                        return cursor;
                    }
                    result = cursor;
                    cursor = (*links(cursor, offset)).right;
                }
            }
        }
        result
    }
}
/// # Safety
/// Caller retains the index owner and stable ordered records. The probe need
/// not belong to this index. Search one AVL path, without touching membership.
pub(crate) unsafe fn neighbors(
    index: *mut Index,
    probe: Node,
    offset: usize,
    less: Less,
) -> Neighbors {
    unsafe {
        let mut result = Neighbors::default();
        let mut cursor = (*index).root;
        while !cursor.is_null() {
            (*index).visits = (*index).visits.wrapping_add(1);
            if less(probe, cursor) {
                result.following = cursor;
                cursor = (*links(cursor, offset)).left;
            } else {
                result.previous = cursor;
                cursor = (*links(cursor, offset)).right;
            }
        }
        result
    }
}
/// # Safety
/// Caller serializes aligned stable records and valid memberships. The offset
/// selects Links in each node; ordering is pure and cannot wait or reschedule.
pub unsafe fn insert(index: *mut Index, node: Node, offset: usize, less: Less) -> Neighbors {
    unsafe {
        links(node, offset).write(Links::default());
        let mut parent = null_mut();
        let mut cursor = (*index).root;
        let mut neighbors = Neighbors::default();
        while !cursor.is_null() {
            let candidate = cursor;
            (*index).visits = (*index).visits.wrapping_add(1);
            parent = candidate;
            if less(node, candidate) {
                neighbors.following = candidate;
                cursor = (*links(candidate, offset)).left;
            } else {
                neighbors.previous = candidate;
                cursor = (*links(candidate, offset)).right;
            }
        }
        (*links(node, offset)).parent = parent;
        if parent.is_null() {
            (*index).root = node;
        } else if less(node, parent) {
            (*links(parent, offset)).left = node;
        } else {
            (*links(parent, offset)).right = node;
        }
        if neighbors.previous.is_null() {
            (*index).first = node;
        }
        (*index).count += 1;
        rebalance(index, parent, offset);
        neighbors
    }
}
unsafe fn minimum(index: *mut Index, node: Node, offset: usize) -> Node {
    unsafe {
        let mut cursor = node;
        while !(*links(cursor, offset)).left.is_null() {
            (*index).visits = (*index).visits.wrapping_add(1);
            cursor = (*links(cursor, offset)).left;
        }
        cursor
    }
}
/// # Safety
/// Caller serializes aligned stable records and valid memberships. The offset
/// selects Links in each node; ordering is pure and cannot wait or reschedule.
pub unsafe fn next(index: *mut Index, node: Node, offset: usize) -> Node {
    unsafe {
        if !(*links(node, offset)).right.is_null() {
            return minimum(index, (*links(node, offset)).right, offset);
        }
        let mut child = node;
        let mut parent = (*links(child, offset)).parent;
        while !parent.is_null() {
            (*index).visits = (*index).visits.wrapping_add(1);
            if (*links(parent, offset)).left == child {
                return parent;
            }
            child = parent;
            parent = (*links(parent, offset)).parent;
        }
        null_mut()
    }
}
/// # Safety
/// Caller serializes aligned stable records and valid memberships. The offset
/// selects Links in each node; ordering is pure and cannot wait or reschedule.
pub unsafe fn remove(index: *mut Index, node: Node, offset: usize) {
    unsafe {
        if (*index).first == node {
            (*index).first = next(index, node, offset);
        }
        let mut start = (*links(node, offset)).parent;
        if (*links(node, offset)).left.is_null() {
            replace(index, node, (*links(node, offset)).right, offset);
        } else if (*links(node, offset)).right.is_null() {
            replace(index, node, (*links(node, offset)).left, offset);
        } else {
            let successor = minimum(index, (*links(node, offset)).right, offset);
            if (*links(successor, offset)).parent != node {
                start = (*links(successor, offset)).parent;
                replace(index, successor, (*links(successor, offset)).right, offset);
                (*links(successor, offset)).right = (*links(node, offset)).right;
                (*links((*links(successor, offset)).right, offset)).parent = successor;
            } else {
                start = successor;
            }
            replace(index, node, successor, offset);
            (*links(successor, offset)).left = (*links(node, offset)).left;
            (*links((*links(successor, offset)).left, offset)).parent = successor;
            update(successor, offset);
        }
        links(node, offset).write(Links::default());
        (*index).count -= 1;
        rebalance(index, start, offset);
    }
}
/// # Safety
/// Caller serializes aligned stable records and valid memberships. The offset
/// selects Links in each node; ordering is pure and cannot wait or reschedule.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_index_insert(
    index: *mut Index,
    node: Node,
    offset: usize,
    less: Less,
    output: *mut Neighbors,
) {
    unsafe { output.write(insert(index, node, offset, less)) };
}
/// # Safety
/// Caller serializes aligned stable records and valid memberships. The offset
/// selects Links in each node; ordering is pure and cannot wait or reschedule.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_index_next(
    index: *mut Index,
    node: Node,
    offset: usize,
) -> Node {
    unsafe { next(index, node, offset) }
}
/// # Safety
/// Caller serializes aligned stable records and valid memberships. The offset
/// selects Links in each node; ordering is pure and cannot wait or reschedule.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_task_index_remove(index: *mut Index, node: Node, offset: usize) {
    unsafe { remove(index, node, offset) }
}
const _: () = {
    assert!(core::mem::size_of::<Links>() == 32);
    assert!(core::mem::size_of::<Index>() == 32);
    assert!(core::mem::size_of::<Neighbors>() == 16);
};

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    struct Record {
        id: u32,
        generation: u64,
        identity: Links,
        address: Links,
    }
    const ID: usize = offset_of!(Record, identity);
    const ADDRESS: usize = offset_of!(Record, address);
    fn record(id: u32, generation: u64) -> Box<Record> {
        Box::new(Record {
            id,
            generation,
            identity: Links::default(),
            address: Links::default(),
        })
    }
    fn node(r: &mut Record) -> Node {
        core::ptr::from_mut(r).cast()
    }
    unsafe extern "C" fn id_before(a: *const c_void, b: *const c_void) -> bool {
        unsafe {
            let x = &*a.cast::<Record>();
            let y = &*b.cast::<Record>();
            (x.id, x.generation, a.addr()) < (y.id, y.generation, b.addr())
        }
    }
    unsafe extern "C" fn address_before(a: *const c_void, b: *const c_void) -> bool {
        a.addr() < b.addr()
    }
    unsafe fn identity(index: *mut Index, id: u32, generation: u64) -> Node {
        unsafe {
            lookup(
                index,
                ID,
                |p| {
                    let r = &*p.cast::<Record>();
                    (r.id, r.generation).cmp(&(id, generation))
                },
                false,
            )
        }
    }

    #[test]
    fn scalar_keys_cover_all_records_without_linear_visits() {
        let mut index = Index::default();
        let mut records: Vec<_> = (1..=257).map(|i| record(i, 1)).collect();
        unsafe {
            for r in &mut records {
                insert(&mut index, node(r), ID, id_before);
            }
            assert_eq!(index.count, records.len());
            for r in &mut records {
                let before = index.visits;
                assert_eq!(identity(&mut index, r.id, r.generation), node(r));
                assert!(index.visits - before <= 10);
            }
            assert!(identity(&mut index, 0, 1).is_null());
            assert!(identity(&mut index, 258, 1).is_null());
            assert!(identity(&mut index, 128, 2).is_null());
            let mut probe = record(128, 2);
            let next = neighbors(&mut index, node(&mut probe), ID, id_before);
            assert_eq!(next.previous, node(&mut records[127]));
            assert_eq!(next.following, node(&mut records[128]));
            for i in (0..records.len()).step_by(2) {
                remove(&mut index, node(&mut records[i]), ID);
            }
            for (i, r) in records.iter_mut().enumerate() {
                assert_eq!(identity(&mut index, r.id, 1).is_null(), i % 2 == 0);
            }
        }
    }

    #[test]
    fn same_address_reuse_rejects_old_generation_and_selects_newest_id() {
        let mut index = Index::default();
        let mut old = record(u32::MAX, 9);
        let mut new = record(u32::MAX, 10);
        unsafe {
            let old_pointer = node(&mut old);
            insert(&mut index, old_pointer, ID, id_before);
            insert(&mut index, node(&mut new), ID, id_before);
            let latest = lookup(
                &mut index,
                ID,
                |p| (*p.cast::<Record>()).id.cmp(&u32::MAX),
                true,
            );
            assert_eq!(latest, node(&mut new));
            assert_eq!(identity(&mut index, u32::MAX, 9), old_pointer);
            remove(&mut index, old_pointer, ID);
            assert!(identity(&mut index, u32::MAX, 9).is_null());
            old.generation = 11;
            insert(&mut index, old_pointer, ID, id_before);
            assert!(identity(&mut index, u32::MAX, 9).is_null());
            assert_eq!(identity(&mut index, u32::MAX, 11), old_pointer);
            assert_eq!(index.count, 2);
        }
    }

    #[test]
    fn independent_views_survive_remove_rollback_and_foreign_pointer_query() {
        let mut ids = Index::default();
        let mut addresses = Index::default();
        let mut records: Vec<_> = (1..=63).rev().map(|i| record(i, i as u64)).collect();
        unsafe {
            for r in &mut records {
                insert(&mut ids, node(r), ID, id_before);
                insert(&mut addresses, node(r), ADDRESS, address_before);
            }
            let candidate = 1usize;
            assert!(lookup(&mut addresses, ADDRESS, |p| p.addr().cmp(&candidate), false).is_null());
            for r in &mut records {
                let p = node(r);
                remove(&mut ids, p, ID);
                remove(&mut addresses, p, ADDRESS);
                assert_eq!(ids.count, addresses.count);
                assert!(identity(&mut ids, r.id, r.generation).is_null());
                assert!(
                    lookup(&mut addresses, ADDRESS, |n| n.addr().cmp(&p.addr()), false).is_null()
                );
                insert(&mut ids, p, ID, id_before);
                insert(&mut addresses, p, ADDRESS, address_before);
                assert_eq!(identity(&mut ids, r.id, r.generation), p);
            }
            for r in &mut records {
                remove(&mut ids, node(r), ID);
                remove(&mut addresses, node(r), ADDRESS);
            }
            assert_eq!(ids.count, 0);
            assert_eq!(addresses.count, 0);
            assert!(ids.root.is_null() && ids.first.is_null());
            assert!(addresses.root.is_null() && addresses.first.is_null());
        }
    }
}
