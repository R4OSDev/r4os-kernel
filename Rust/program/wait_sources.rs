//! Observation links embedded in the original resource, never a second
//! resource registry. Source admission takes its real owner BEFORE Runtime.
//! Every reset/free closes links under Runtime before backing can disappear.
use super::waitsets::{self, Subscription};
use crate::{arch::runtime::RuntimeGuard, contract as c};
use core::{cell::UnsafeCell, ptr};

struct Epoch(UnsafeCell<u64>);
// Reserved only under the original Runtime owner; exhausted epochs never wrap.
unsafe impl Sync for Epoch {}
static EPOCH: Epoch = Epoch(UnsafeCell::new(0));

#[repr(C)]
pub(crate) struct Channel {
    head: *mut Subscription,
    epoch: u64,
    revision: u64,
    count: u32,
    ready: bool,
    closed: bool,
    gui: bool,
}
impl Channel {
    pub(crate) const EMPTY: Self = Self {
        head: ptr::null_mut(),
        epoch: 0,
        revision: 0,
        count: 0,
        ready: false,
        closed: false,
        gui: false,
    };
    pub(super) const GUI: Self = Self {
        gui: true,
        ..Self::EMPTY
    };
    pub(super) fn subscribed(&self) -> bool {
        self.count != 0
    }

    /// Real source owner held; no pageable copy, foreign call or allocation.
    pub(crate) unsafe fn publish(channel: *mut Self, ready: bool) {
        // Epoch is written under this SAME source owner. An unobserved source
        // has no links; its first Prepare snapshots the authoritative state.
        // Preserve the old hot path without an additional Runtime acquisition.
        if unsafe { (*channel).epoch == 0 } {
            return;
        }
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            Self::publish_locked(channel, ready);
        }
    }
    pub(super) unsafe fn publish_locked(channel: *mut Self, ready: bool) {
        unsafe {
            if (*channel).closed {
                return;
            }
            let changed = (*channel).gui || (*channel).ready != ready;
            (*channel).ready = ready;
            if !changed || (*channel).epoch == 0 {
                return;
            }
            if (*channel).revision == u64::MAX {
                Self::close_locked(channel);
                return;
            }
            (*channel).revision += 1;
            let mut link = (*channel).head;
            let mut count = 0;
            while !link.is_null() {
                waitsets::resource_changed(link, (*channel).revision, ready, false);
                link = (*link).next;
                count += 1;
                assert!(count <= c::waitset_max_source_subscribers);
            }
            assert!(count == (*channel).count);
        }
    }
    pub(crate) unsafe fn close(channel: *mut Self) {
        // The authoritative owner (or exclusive unlinked backing) serializes
        // this zero-epoch path with Prepare. No observer could have a link.
        if unsafe { (*channel).epoch == 0 } {
            unsafe {
                (*channel).closed = true;
            }
            return;
        }
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            Self::close_locked(channel);
        }
    }
    pub(super) unsafe fn close_locked(channel: *mut Self) {
        unsafe {
            (*channel).closed = true;
            let mut link = (*channel).head;
            let mut count = 0;
            while !link.is_null() {
                let next = (*link).next;
                (*link).linked = false;
                (*link).channel = ptr::null_mut();
                (*link).previous = ptr::null_mut();
                (*link).next = ptr::null_mut();
                waitsets::resource_changed(link, (*channel).revision, false, true);
                count += 1;
                assert!(count <= c::waitset_max_source_subscribers);
                link = next;
            }
            assert!(count == (*channel).count);
            (*channel).head = ptr::null_mut();
            (*channel).count = 0;
            if (*channel).gui {
                super::gui_activity::waitset_membership(channel);
            }
        }
    }
    unsafe fn epoch(channel: *mut Self, input: Identity, ready: bool) -> Result<(), i32> {
        unsafe {
            if (*channel).closed {
                return Err(c::waitset_error_closed);
            }
            match input {
                Identity::Original(_) => {
                    if (*channel).epoch == 0 {
                        let next = (*EPOCH.0.get())
                            .checked_add(1)
                            .ok_or(c::waitset_error_exhausted)?;
                        *EPOCH.0.get() = next;
                        (*channel).epoch = next;
                        (*channel).ready = ready;
                    }
                }
                Identity::Prepared(source) => {
                    if (*channel).epoch == 0 || source.generation != (*channel).epoch {
                        return Err(c::waitset_error_stale);
                    }
                }
            }
            assert!((*channel).ready == ready || (*channel).gui);
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Identity {
    Original(c::WaitSourceRequest),
    Prepared(c::WaitSource),
}
impl Identity {
    pub(super) fn kind(self) -> u32 {
        match self {
            Self::Original(r) => r.kind,
            Self::Prepared(r) => r.kind,
        }
    }
    pub(crate) fn object(self) -> u64 {
        match self {
            Self::Original(r) => r.object,
            Self::Prepared(r) => r.object,
        }
    }
    pub(crate) fn original(self) -> Option<c::WaitSourceRequest> {
        match self {
            Self::Original(r) => Some(r),
            Self::Prepared(_) => None,
        }
    }
    pub(crate) unsafe fn matches(self, channel: *const Channel) -> bool {
        match self {
            Self::Original(_) => true,
            Self::Prepared(s) => unsafe { s.generation == (*channel).epoch },
        }
    }
    /// The caller already retains the authoritative source owner/lifetime.
    pub(crate) unsafe fn locked<T>(
        self,
        channel: *mut Channel,
        ready: bool,
        action: impl FnOnce(*mut Channel) -> Result<T, i32>,
    ) -> Result<T, i32> {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            Channel::epoch(channel, self, ready)?;
        }
        action(channel)
    }
}
/// Original resource owners acquire their existing mutex/short pin before
/// invoking this bounded Runtime action. No source owner survives a Set wait.
pub(super) unsafe fn access<T>(
    input: Identity,
    action: impl FnOnce(*mut Channel) -> Result<T, i32>,
) -> Result<T, i32> {
    unsafe {
        match input.kind() {
            c::waitset_source_io => super::async_io::waitset_source(input, action),
            c::waitset_source_process => super::completions::waitset_source(input, action),
            c::waitset_source_thread => super::threads::waitset_source(input, action),
            c::waitset_source_service_endpoint => crate::service::endpoint::waitset_source(
                input,
                super::context::handle().ok_or(c::waitset_error_context)?,
                action,
            ),
            c::waitset_source_gui_activity => super::gui_activity::waitset_source(input, action),
            _ => Err(c::waitset_error_unsupported),
        }
    }
}
pub(super) unsafe fn subscribe(
    channel: *mut Channel,
    link: *mut Subscription,
    observed: u64,
) -> Result<(), i32> {
    unsafe {
        if (*channel).closed {
            return Err(c::waitset_error_closed);
        }
        if observed > (*channel).revision {
            return Err(c::waitset_error_invalid);
        }
        if (*channel).count == c::waitset_max_source_subscribers {
            return Err(c::waitset_error_capacity);
        }
        assert!(!(*link).linked);
        (*link).channel = channel;
        (*link).previous = ptr::null_mut();
        (*link).next = (*channel).head;
        if !(*link).next.is_null() {
            (*(*link).next).previous = link;
        }
        (*channel).head = link;
        (*channel).count += 1;
        (*link).linked = true;
        waitsets::resource_changed(link, (*channel).revision, (*channel).ready, false);
        if (*channel).gui {
            super::gui_activity::waitset_membership(channel);
        }
        Ok(())
    }
}
/// Runtime only: direct stable embedded list, no acquisition of source mutex.
/// Source Close clears every pointer before its resource resets/frees backing.
pub(super) unsafe fn unsubscribe(link: *mut Subscription) {
    unsafe {
        let channel = (*link).channel;
        if channel.is_null() {
            return;
        }
        assert!((*link).linked && (*channel).count != 0);
        if (*link).previous.is_null() {
            (*channel).head = (*link).next;
        } else {
            (*(*link).previous).next = (*link).next;
        }
        if !(*link).next.is_null() {
            (*(*link).next).previous = (*link).previous;
        }
        (*channel).count -= 1;
        (*link).linked = false;
        (*link).channel = ptr::null_mut();
        (*link).previous = ptr::null_mut();
        (*link).next = ptr::null_mut();
        if (*channel).gui {
            super::gui_activity::waitset_membership(channel);
        }
    }
}
pub(super) unsafe fn prepare(request: *const c::WaitSourceRequest, out: *mut c::WaitSource) -> i32 {
    if !waitsets::valid(request, 1)
        || !waitsets::valid(out.cast_const(), 1)
        || waitsets::overlaps(
            request.addr(),
            size_of::<c::WaitSourceRequest>(),
            out.addr(),
            size_of::<c::WaitSource>(),
        )
    {
        return c::waitset_error_invalid;
    }
    let request = unsafe { request.read() };
    if request.version != 1
        || request.size as usize != size_of::<c::WaitSourceRequest>()
        || request.flags != 0
        || (request.kind != c::waitset_source_gui_activity && request.observed_revision != 0)
        || (request.kind != c::waitset_source_thread
            && (request.scope_id != 0 || request.scope_generation != 0))
        || (!matches!(
            request.kind,
            c::waitset_source_process | c::waitset_source_thread
        ) && request.generation != 0)
    {
        return c::waitset_error_invalid;
    }
    let Some(_operation) = super::notifications::Operation::enter() else {
        return c::waitset_error_context;
    };
    if unsafe { super::context::current() }.is_null() {
        return c::waitset_error_context;
    }
    let result = unsafe {
        access(Identity::Original(request), |channel| {
            if request.observed_revision > (*channel).revision {
                return Err(c::waitset_error_invalid);
            }
            Ok(c::WaitSource {
                kind: request.kind,
                object: request.object,
                generation: (*channel).epoch,
                observed_revision: request.observed_revision,
                user_tag: request.user_tag,
                ..c::WaitSource::default()
            })
        })
    };
    match result {
        Ok(source) => {
            unsafe {
                out.write(source);
            }
            c::waitset_ok
        }
        Err(error) => error,
    }
}
