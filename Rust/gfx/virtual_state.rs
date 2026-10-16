//! Original cross-owner VA/binding lifetime. Resident caller nodes; hardware,
//! allocation and notification stay outside this no-sleep metadata owner.
use super::types::{Error, Handle, Owner};
use crate::{
    contract as a,
    driver::treap::{Node, Tree},
};
use core::{ffi::c_void, mem::size_of, ptr};
#[repr(C)]
pub struct Entry {
    pub index: Node,
    pub handle: Handle,
    pub owner: Owner,
    pub provider: Handle,
    pub request: a::GfxVirtualRequest,
    pub parent: *mut Entry,
    pub first_child: *mut Entry,
    pub next_child: *mut Entry,
    pub previous_child: *mut Entry,
    pub live_children: usize,
    pub executions: usize,
    pub open: bool,
    pub closing: bool,
    pub retired: bool,
    pub has_claim: bool,
    pub claim: u32,
    pub token: a::GfxVirtualToken,
    pub address: u64,
    pub result: i32,
    pub notifications: u8,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            index: Node {
                key: 0,
                priority: 0,
                parent: ptr::null_mut(),
                children: [ptr::null_mut(); 2],
            },
            handle: Handle::EMPTY,
            owner: Owner {
                kind: 1,
                id: 0,
                generation: 0,
            },
            provider: Handle::EMPTY,
            request: a::GfxVirtualRequest::default(),
            parent: ptr::null_mut(),
            first_child: ptr::null_mut(),
            next_child: ptr::null_mut(),
            previous_child: ptr::null_mut(),
            live_children: 0,
            executions: 0,
            open: true,
            closing: false,
            retired: false,
            has_claim: false,
            claim: 0,
            token: a::GfxVirtualToken::default(),
            address: 0,
            result: 0,
            notifications: 0,
        }
    }
}
pub fn public_handle(value: Handle) -> a::GfxBufferHandle {
    a::GfxBufferHandle {
        id: value.id,
        reserved0: 0,
        generation: value.generation,
    }
}
fn empty_handle(value: a::GfxBufferHandle) -> bool {
    value.id == 0 && value.generation == 0 && value.reserved0 == 0
}
pub fn zero_token(value: a::GfxVirtualToken) -> bool {
    value.opaque0 == 0 && value.opaque1 == 0 && value.opaque2 == 0
}
fn valid_handle(value: a::GfxBufferHandle) -> bool {
    value.id != 0 && value.generation != 0 && value.reserved0 == 0
}
fn token_equal(x: a::GfxVirtualToken, y: a::GfxVirtualToken) -> bool {
    x.opaque0 == y.opaque0 && x.opaque1 == y.opaque1 && x.opaque2 == y.opaque2
}
impl Entry {
    pub fn backend(&self) -> bool {
        !zero_token(self.token)
    }
    pub fn status(&self) -> a::GfxVirtualStatus {
        a::GfxVirtualStatus {
            resource: public_handle(self.handle),
            parent: self.request.parent,
            kind: self.request.kind,
            flags: u32::from(self.result != 0)
                | (u32::from(self.retired) << 1)
                | (u32::from(self.closing) << 2)
                | (u32::from(self.has_claim) << 3),
            result: self.result,
            address: if self.result == 1 && !self.closing {
                self.address
            } else {
                0
            },
            byte_length: self.request.byte_length,
            deadline_ns: self.request.deadline_ns,
            ..a::GfxVirtualStatus::default()
        }
    }
    pub fn settled(&self, until: u32) -> bool {
        if until == 0 {
            self.result != 0
        } else {
            self.retired
        }
    }
    fn finish_result(&mut self, result: i32) {
        if self.result != 0 {
            return;
        }
        self.result = result;
        self.notifications |= 1;
    }
    pub fn pending(&self) -> Option<u32> {
        if self.has_claim || self.retired {
            return None;
        }
        if self.closing {
            return if self.backend() && self.live_children == 0 && self.executions == 0 {
                Some(1)
            } else {
                None
            };
        }
        if self.result == 0 { Some(0) } else { None }
    }
    pub fn retain_execution(&mut self) -> Result<(), Error> {
        if !self.open || self.closing || self.retired {
            return Err(Error::Closed);
        }
        if self.request.kind != 2 || self.result != 1 || !self.backend() || self.has_claim {
            return Err(Error::Busy);
        }
        self.executions = self.executions.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(())
    }
}
// Use raw links for recursive retirement: no exclusive Entry reference spans a
// recursive parent/child operation, and no node aliases a live Rust borrow.
unsafe fn settle_retirement(e: *mut Entry) {
    unsafe {
        if (*e).retired
            || !(*e).closing
            || (*e).has_claim
            || (*e).backend()
            || (*e).live_children != 0
            || (*e).executions != 0
        {
            return;
        }
        (*e).retired = true;
        (*e).notifications |= 2;
        let parent = (*e).parent;
        if !parent.is_null() {
            assert!((*parent).live_children != 0);
            (*parent).live_children -= 1;
            settle_retirement(parent);
        }
    }
}
pub(crate) unsafe fn close(e: *mut Entry, release: bool, reason: i32) {
    unsafe {
        if release {
            (*e).open = false;
        }
        (*e).closing = true;
        (&mut *e).finish_result(reason);
        let mut child = (*e).first_child;
        while !child.is_null() {
            close(child, release, reason);
            child = (*child).next_child;
        }
        settle_retirement(e);
    }
}
/// # Safety
/// Exact retained execution and linked parent nodes remain resident under the
/// original shared metadata owner until the retirement transition finishes.
pub(crate) unsafe fn release_execution(e: *mut Entry) {
    unsafe {
        assert!((*e).executions != 0 && !(*e).retired);
        (*e).executions -= 1;
        settle_retirement(e);
    }
}
pub fn validate(input: a::GfxVirtualRequest, instant: u64) -> Result<(), Error> {
    if input.version != 1
        || input.size < size_of::<a::GfxVirtualRequest>() as u32
        || input.adapter_id == 0
        || input.memory_generation == 0
        || input.reserved0 != 0
        || input.reserved1 != 0
        || instant == 0
        || input.deadline_ns <= instant
        || input.deadline_ns == u64::MAX
        || input.byte_length == 0
        || input.byte_length & 4095 != 0
    {
        return Err(Error::Invalid);
    }
    match input.kind {
        1 => {
            let layout_flags = a::gfx_virtual_flag_blocklinear | a::gfx_virtual_layout_mask;
            if input.flags & !layout_flags != 0 || input.location > 1 {
                return Err(Error::Unsupported);
            }
            if input.flags & a::gfx_virtual_layout_mask != 0
                && input.flags & a::gfx_virtual_flag_blocklinear == 0
            {
                return Err(Error::Invalid);
            }
            if !empty_handle(input.parent)
                || !empty_handle(input.reference)
                || input.byte_offset != 0
                || input.virtual_offset != 0
                || input.alignment < 4096
                || !input.alignment.is_power_of_two()
                || !input.fixed_address.is_multiple_of(input.alignment)
                || input.fixed_address > u64::MAX - input.byte_length
            {
                return Err(Error::Invalid);
            }
        }
        2 => {
            if input.flags != 0 || input.location != 0 {
                return Err(Error::Unsupported);
            }
            if !valid_handle(input.parent)
                || !valid_handle(input.reference)
                || input.alignment != 0
                || input.fixed_address != 0
                || (input.byte_offset | input.virtual_offset) & 4095 != 0
                || input.byte_offset > u64::MAX - input.byte_length
                || input.virtual_offset > u64::MAX - input.byte_length
            {
                return Err(Error::Invalid);
            }
        }
        _ => return Err(Error::Unsupported),
    }
    Ok(())
}
pub(crate) unsafe fn next(mut n: *mut Node) -> *mut Node {
    unsafe {
        if !(*n).children[1].is_null() {
            n = (*n).children[1];
            while !(*n).children[0].is_null() {
                n = (*n).children[0];
            }
            return n;
        }
        let mut parent = (*n).parent;
        while !parent.is_null() && (*parent).children[1] == n {
            n = parent;
            parent = (*n).parent;
        }
        parent
    }
}
#[repr(C)]
#[derive(Default)]
pub struct Store {
    pub tree: Tree,
    pub serial: u64,
}
impl Store {
    pub(crate) unsafe fn find(&self, h: Handle) -> Result<*mut Entry, Error> {
        if h.id != 1 || h.generation == 0 {
            return Err(Error::Stale);
        }
        let n = unsafe { self.tree.find(h.generation) };
        if n.is_null() {
            Err(Error::Stale)
        } else {
            Ok(n.cast::<Entry>())
        }
    }
    pub(crate) unsafe fn owned(&self, h: Handle, o: Owner) -> Result<*mut Entry, Error> {
        let e = unsafe { self.find(h)? };
        if unsafe { (*e).owner } != o {
            return Err(Error::WrongOwner);
        }
        if !unsafe { (*e).open } {
            return Err(Error::Closed);
        }
        Ok(e)
    }
    pub(crate) unsafe fn parent(
        &self,
        o: Owner,
        p: Handle,
        input: a::GfxVirtualRequest,
    ) -> Result<*mut Entry, Error> {
        unsafe {
            if input.kind == 1 {
                return Ok(ptr::null_mut());
            }
            let e = self.owned(
                Handle {
                    id: input.parent.id,
                    generation: input.parent.generation,
                },
                o,
            )?;
            if (*e).request.kind != 1
                || (*e).provider != p
                || (*e).request.adapter_id != input.adapter_id
                || (*e).request.memory_generation != input.memory_generation
            {
                return Err(Error::Stale);
            }
            if (*e).closing || (*e).result < 0 {
                return Err(Error::Closed);
            }
            if (*e).result != 1 || !(*e).backend() || (*e).has_claim {
                return Err(Error::Busy);
            }
            if input.byte_length > (*e).request.byte_length
                || input.virtual_offset > (*e).request.byte_length - input.byte_length
            {
                return Err(Error::Invalid);
            }
            let mut child = (*e).first_child;
            while !child.is_null() {
                if !(*child).retired
                    && input.virtual_offset
                        < (*child).request.virtual_offset + (*child).request.byte_length
                    && (*child).request.virtual_offset < input.virtual_offset + input.byte_length
                {
                    return Err(Error::Busy);
                }
                child = (*child).next_child;
            }
            Ok(e)
        }
    }
    pub(crate) unsafe fn start(
        &mut self,
        e: *mut Entry,
        o: Owner,
        p: Handle,
        input: a::GfxVirtualRequest,
        instant: u64,
    ) -> Result<(), Error> {
        unsafe {
            validate(input, instant)?;
            if (*e).handle.id != 0 || !o.valid() || p.id == 0 || p.generation == 0 {
                return Err(Error::Invalid);
            }
            let enclosing = self.parent(o, p, input)?;
            if self.serial == u64::MAX
                || (!enclosing.is_null() && (*enclosing).live_children == usize::MAX)
            {
                return Err(Error::Exhausted);
            }
            self.serial += 1;
            e.write(Entry {
                handle: Handle {
                    id: 1,
                    generation: self.serial,
                },
                owner: o,
                provider: p,
                request: a::GfxVirtualRequest {
                    size: size_of::<a::GfxVirtualRequest>() as u32,
                    ..input
                },
                parent: enclosing,
                ..Entry::default()
            });
            self.tree.insert(self.serial, ptr::addr_of_mut!((*e).index));
            if !enclosing.is_null() {
                (*e).next_child = (*enclosing).first_child;
                if !(*enclosing).first_child.is_null() {
                    (*(*enclosing).first_child).previous_child = e;
                }
                (*enclosing).first_child = e;
                (*enclosing).live_children += 1;
            }
            Ok(())
        }
    }
    pub(crate) unsafe fn expire(&mut self, instant: u64) {
        unsafe {
            let mut n = self.tree.min();
            while !n.is_null() {
                let e = n.cast::<Entry>();
                if (*e).result == 0 && instant >= (*e).request.deadline_ns {
                    close(e, false, a::gfx_queue_error_wait_timeout);
                }
                n = next(n);
            }
        }
    }
    pub(crate) unsafe fn take(&mut self, p: Handle, instant: u64) -> *mut Entry {
        unsafe {
            self.expire(instant);
            let mut n = self.tree.min();
            while !n.is_null() {
                let e = n.cast::<Entry>();
                if (*e).provider == p
                    && let Some(operation) = (*e).pending()
                {
                    (*e).has_claim = true;
                    (*e).claim = operation;
                    return e;
                }
                n = next(n);
            }
            ptr::null_mut()
        }
    }
    pub(crate) unsafe fn complete(
        &mut self,
        p: Handle,
        c: a::GfxVirtualCompletion,
    ) -> Result<(), Error> {
        unsafe {
            if c.version != 1
                || c.size < size_of::<a::GfxVirtualCompletion>() as u32
                || !valid_handle(c.resource)
                || c.operation > 1
                || c.result == 0
                || c.result > 1
            {
                return Err(Error::Invalid);
            }
            let e = self.find(Handle {
                id: c.resource.id,
                generation: c.resource.generation,
            })?;
            if (*e).provider != p {
                return Err(Error::WrongOwner);
            }
            if !(*e).has_claim {
                return Err(Error::Closed);
            }
            let operation = (*e).claim;
            if operation != c.operation {
                return Err(Error::Stale);
            }
            if operation == 0 {
                if c.result == 1 {
                    if zero_token(c.token)
                        || c.address == 0
                        || c.address & 4095 != 0
                        || c.address > u64::MAX - (*e).request.byte_length
                    {
                        return Err(Error::Invalid);
                    }
                    let parent = (*e).parent;
                    if !parent.is_null() {
                        if c.address != (*parent).address + (*e).request.virtual_offset {
                            return Err(Error::Invalid);
                        }
                    } else if !c.address.is_multiple_of((*e).request.alignment)
                        || ((*e).request.fixed_address != 0
                            && c.address != (*e).request.fixed_address)
                    {
                        return Err(Error::Invalid);
                    }
                    (*e).token = c.token;
                    (*e).address = c.address;
                } else {
                    if !zero_token(c.token) || c.address != 0 {
                        return Err(Error::Invalid);
                    }
                    (*e).closing = true;
                }
                (&mut *e).finish_result(c.result);
            } else {
                if c.result != 1 || c.address != 0 || !token_equal(c.token, (*e).token) {
                    return Err(Error::Invalid);
                }
                (*e).token = a::GfxVirtualToken::default();
            }
            (*e).has_claim = false;
            (*e).claim = 0;
            settle_retirement(e);
            Ok(())
        }
    }
    pub(crate) unsafe fn close_provider(&mut self, p: Handle) {
        unsafe {
            let mut n = self.tree.min();
            while !n.is_null() {
                let e = n.cast::<Entry>();
                if (*e).provider == p {
                    close(e, false, a::gfx_queue_error_device_lost);
                }
                n = next(n);
            }
        }
    }
    pub(crate) unsafe fn stopped(&mut self, o: Owner) {
        unsafe {
            let mut n = self.tree.min();
            while !n.is_null() {
                let e = n.cast::<Entry>();
                if (*e).owner == o {
                    close(e, true, a::gfx_queue_error_wait_cancelled);
                }
                n = next(n);
            }
        }
    }
    pub fn removable(e: &Entry) -> bool {
        !e.open && e.retired && e.first_child.is_null() && e.notifications == 0
    }
    pub(crate) unsafe fn remove(&mut self, e: *mut Entry) {
        unsafe {
            assert!(Self::removable(&*e));
            let parent = (*e).parent;
            if !parent.is_null() {
                let previous = (*e).previous_child;
                let next = (*e).next_child;
                if !previous.is_null() {
                    (*previous).next_child = next;
                } else {
                    (*parent).first_child = next;
                }
                if !next.is_null() {
                    (*next).previous_child = previous;
                }
            }
            self.tree.remove(ptr::addr_of_mut!((*e).index));
            e.write(Entry::default());
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Args {
    pub owner: Owner,
    pub provider: Handle,
    pub handle: Handle,
    pub input: *const a::GfxVirtualRequest,
    pub completion: *const a::GfxVirtualCompletion,
    pub entry: *mut Entry,
    pub instant: u64,
}
/// # Safety
/// Caller serializes this exact store and all resident linked nodes under the
/// original metadata owner. Inputs/outputs are initialized, exact and disjoint;
/// returned node pointers are only borrowed within that owner. No wait/callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_virtual_state_run(
    store: *mut Store,
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { args.read() };
    let result = (|| -> Result<(), Error> {
        unsafe {
            // Pure queries do not create exclusive references to constant storage.
            match op {
                0 => out.cast::<*mut Entry>().write((&*store).find(args.handle)?),
                1 => out
                    .cast::<*mut Entry>()
                    .write((&*store).owned(args.handle, args.owner)?),
                2 => out.cast::<*mut Entry>().write((&*store).parent(
                    args.owner,
                    args.provider,
                    args.input.read(),
                )?),
                3 => (&mut *store).start(
                    args.entry,
                    args.owner,
                    args.provider,
                    args.input.read(),
                    args.instant,
                )?,
                4 => (&mut *store).expire(args.instant),
                5 => out
                    .cast::<*mut Entry>()
                    .write((&mut *store).take(args.provider, args.instant)),
                6 => (&mut *store).complete(args.provider, args.completion.read())?,
                7 => (&mut *store).close_provider(args.provider),
                8 => (&mut *store).stopped(args.owner),
                9 => (&mut *store).remove(args.entry),
                _ => panic!("Invalid VA store operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
/// # Safety
/// Exact resident node and its linked relatives are protected by the original
/// metadata owner. Out points to the operation's disjoint resident output type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_virtual_entry(
    e: *mut Entry,
    op: u32,
    value: u32,
    reason: i32,
    out: *mut c_void,
) -> i32 {
    let result = (|| -> Result<(), Error> {
        unsafe {
            match op {
                0 => out.cast::<bool>().write((&*e).backend()),
                1 => out.cast::<a::GfxVirtualStatus>().write((&*e).status()),
                2 => out.cast::<bool>().write((&*e).settled(value)),
                3 => close(e, value != 0, reason),
                4 => out.cast::<u32>().write((&*e).pending().unwrap_or(2)),
                5 => (&mut *e).retain_execution()?,
                6 => release_execution(e),
                7 => out.cast::<bool>().write(Store::removable(&*e)),
                _ => panic!("Invalid VA entry operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
/// # Safety
/// Complete initialized resident contract input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_virtual_validate(
    input: *const a::GfxVirtualRequest,
    instant: u64,
) -> i32 {
    match validate(unsafe { input.read() }, instant) {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
/// # Safety
/// Read-only serialized resident tree header; no borrow survives the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_virtual_tree_first(tree: *const Tree) -> *mut Node {
    unsafe { (&*tree).min() }
}
/// # Safety
/// Node belongs to the exact serialized resident tree until traversal completes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_virtual_node_next(node: *mut Node) -> *mut Node {
    unsafe { next(node) }
}
/// # Safety
/// Complete initialized resident token input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_virtual_zero_token(
    token: *const a::GfxVirtualToken,
) -> bool {
    zero_token(unsafe { token.read() })
}

const _: () = {
    assert!(core::mem::size_of::<Node>() == 40 && core::mem::align_of::<Node>() == 8);
    assert!(core::mem::offset_of!(Node, key) == 0);
    assert!(core::mem::offset_of!(Node, priority) == 8);
    assert!(core::mem::offset_of!(Node, parent) == 16);
    assert!(core::mem::offset_of!(Node, children) == 24);
    assert!(core::mem::size_of::<Tree>() == 16 && core::mem::align_of::<Tree>() == 8);
    assert!(core::mem::offset_of!(Tree, root) == 0);
    assert!(core::mem::offset_of!(Tree, xorshift) == 8);
    assert!(core::mem::size_of::<Entry>() == 312 && core::mem::align_of::<Entry>() == 8);
    assert!(core::mem::offset_of!(Entry, index) == 0);
    assert!(core::mem::offset_of!(Entry, handle) == 40);
    assert!(core::mem::offset_of!(Entry, owner) == 56);
    assert!(core::mem::offset_of!(Entry, provider) == 80);
    assert!(core::mem::offset_of!(Entry, request) == 96);
    assert!(core::mem::offset_of!(Entry, parent) == 216);
    assert!(core::mem::offset_of!(Entry, first_child) == 224);
    assert!(core::mem::offset_of!(Entry, next_child) == 232);
    assert!(core::mem::offset_of!(Entry, previous_child) == 240);
    assert!(core::mem::offset_of!(Entry, live_children) == 248);
    assert!(core::mem::offset_of!(Entry, executions) == 256);
    assert!(core::mem::offset_of!(Entry, open) == 264);
    assert!(core::mem::offset_of!(Entry, closing) == 265);
    assert!(core::mem::offset_of!(Entry, retired) == 266);
    assert!(core::mem::offset_of!(Entry, has_claim) == 267);
    assert!(core::mem::offset_of!(Entry, claim) == 268);
    assert!(core::mem::offset_of!(Entry, token) == 272);
    assert!(core::mem::offset_of!(Entry, address) == 296);
    assert!(core::mem::offset_of!(Entry, result) == 304);
    assert!(core::mem::offset_of!(Entry, notifications) == 308);
    assert!(core::mem::size_of::<Store>() == 24 && core::mem::align_of::<Store>() == 8);
    assert!(core::mem::offset_of!(Store, tree) == 0);
    assert!(core::mem::offset_of!(Store, serial) == 16);
    assert!(core::mem::size_of::<Args>() == 88 && core::mem::align_of::<Args>() == 8);
    assert!(core::mem::offset_of!(Args, owner) == 0);
    assert!(core::mem::offset_of!(Args, provider) == 24);
    assert!(core::mem::offset_of!(Args, handle) == 40);
    assert!(core::mem::offset_of!(Args, input) == 56);
    assert!(core::mem::offset_of!(Args, completion) == 64);
    assert!(core::mem::offset_of!(Args, entry) == 72);
    assert!(core::mem::offset_of!(Args, instant) == 80);
};
