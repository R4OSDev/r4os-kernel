//! Exact GUI payload channels. Runtime owns sequence, subscription links and
//! Task enrollment; program lifetime keeps the resident payload through wait.
//! Desktop input/GPU activity has no path into these program queues.
pub(super) mod probe;
use super::{console::Lease, context, gui_events, payload_types as t, threads};
use crate::{
    arch::runtime::{self, RuntimeGuard},
    contract as c,
    sched::{
        sync_objects,
        sync_types::{Text, WaitQueue, WaitRequest},
        sync_wait,
    },
    service::desktop_events,
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};

#[repr(C)]
pub struct Channel {
    queue: WaitQueue,
    sequence: u64,
    previous: *mut Channel,
    next: *mut Channel,
    subscriptions: u32,
}
impl Channel {
    pub const EMPTY: Self = Self {
        queue: WaitQueue::EMPTY,
        sequence: 1,
        previous: ptr::null_mut(),
        next: ptr::null_mut(),
        subscriptions: 0,
    };
}
struct Subscribers(UnsafeCell<*mut Channel>);
// Exact live GUI payloads only; insert/remove/publication all hold Runtime.
unsafe impl Sync for Subscribers {}
static SUBSCRIBERS: Subscribers = Subscribers(UnsafeCell::new(ptr::null_mut()));
unsafe fn unlink(channel: *mut Channel) {
    unsafe {
        if (*channel).subscriptions == 0 {
            return;
        }
        if (*channel).previous.is_null() {
            *SUBSCRIBERS.0.get() = (*channel).next;
        } else {
            (*(*channel).previous).next = (*channel).next;
        }
        if !(*channel).next.is_null() {
            (*(*channel).next).previous = (*channel).previous;
        }
        (*channel).previous = ptr::null_mut();
        (*channel).next = ptr::null_mut();
        (*channel).subscriptions = 0;
    }
}
unsafe fn publish(channel: *mut Channel) {
    unsafe {
        if !(*channel).queue.core.closing {
            sync_wait::r4os_rust_sync_queue(
                ptr::addr_of_mut!((*channel).queue),
                8,
                ptr::null_mut(),
                0,
                ptr::addr_of_mut!((*channel).sequence),
            );
        }
    }
}
unsafe fn subscribe(channel: *mut Channel, mask: u32) {
    unsafe {
        if (*channel).subscriptions == mask {
            return;
        }
        unlink(channel);
        if mask != 0 {
            let head = *SUBSCRIBERS.0.get();
            (*channel).next = head;
            if !head.is_null() {
                (*head).previous = channel;
            }
            *SUBSCRIBERS.0.get() = channel;
            (*channel).subscriptions = mask;
        }
        // Force a rescan across the first/changed subscription boundary.
        publish(channel);
    }
}
/// # Safety
/// Exact GUI retained by its producer/registry lease. No state-lock borrow
/// spans this independent Runtime admission.
pub(super) unsafe fn signal(gui: *mut t::Gui) {
    if gui.is_null() {
        return;
    }
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        publish(ptr::addr_of_mut!((*gui).activity));
    }
}
/// # Safety
/// Exact terminal/releasing GUI lifetime; unlink precedes payload reuse/free.
pub(super) unsafe fn close(gui: *mut t::Gui) {
    if gui.is_null() {
        return;
    }
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let channel = ptr::addr_of_mut!((*gui).activity);
        unlink(channel);
        sync_objects::queue_close_cancelled(ptr::addr_of_mut!((*channel).queue));
    }
}
pub(super) fn signal_handle(handle: c::ProgramProcessHandle) {
    let Some(lease) = (unsafe { Lease::handle(handle) }) else {
        return;
    };
    let gui = {
        let _owner = crate::memory::owner::Guard::acquire(0);
        unsafe { (*lease.0.instance).gui_payload.cast::<t::Gui>() }
    };
    unsafe {
        signal(gui);
    }
}
/// Shared publishers use their authoritative state, never a registry scan.
pub(crate) fn notify(mask: u32) -> i32 {
    if mask == 0 || mask & !c::gui_activity_shared_all != 0 {
        return -1;
    }
    {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            let mut channel = *SUBSCRIBERS.0.get();
            while !channel.is_null() {
                let next = (*channel).next;
                if (*channel).subscriptions & mask != 0 {
                    publish(channel);
                }
                channel = next;
            }
        }
    }
    desktop_events::r4os_rust_desktop_signal();
    0
}
struct Observation {
    channel: *mut Channel,
    sequence: u64,
}
unsafe extern "C" fn unchanged(raw: *mut c_void) -> bool {
    unsafe {
        let observation = &*raw.cast::<Observation>();
        !(*observation.channel).queue.core.closing
            && (*observation.channel).sequence == observation.sequence
    }
}
/// # Safety
/// Exact current main GUI execution keeps its payload resident through the
/// original unwind/cancel boundary; out is writable for this call only.
pub(crate) unsafe fn wait(last: u64, timeout: u64, mask: u32, out: *mut u64) -> i32 {
    if out.is_null() || mask & !c::gui_activity_shared_all != 0 {
        return -1;
    }
    let Some(_operation) = threads::Operation::enter() else {
        return -2;
    };
    let instance = unsafe { context::current() };
    if instance.is_null() || !unsafe { gui_events::allowed(instance) } {
        return -1;
    }
    let thread = unsafe { threads::current() };
    // A synchronous main entry can retain its outer caller's thread. An
    // actual worker of this instance must not replace the GUI subscription.
    if !thread.is_null()
        && unsafe { (*thread).owner_instance == instance && (*thread).flags & threads::MAIN == 0 }
    {
        return -1;
    }
    let gui = {
        let _owner = crate::memory::owner::Guard::acquire(0);
        unsafe { (*instance).gui_payload.cast::<t::Gui>() }
    };
    if gui.is_null() {
        return -1;
    }
    let channel = unsafe { ptr::addr_of_mut!((*gui).activity) };
    let mut observation = Observation {
        channel,
        sequence: last,
    };
    let flags = runtime::save_runtime();
    unsafe {
        if (*channel).queue.core.closing {
            runtime::restore_runtime(flags);
            return -2;
        }
        subscribe(channel, mask);
    }
    // Transfer this exact outer admission to native atomic enrollment/park;
    // no RuntimeGuard or GUI/Channel reference crosses the physical switch.
    let mut request = WaitRequest::predicate(
        timeout,
        Text::literal(b"gui-activity"),
        unchanged,
        ptr::from_mut(&mut observation).cast(),
    );
    request.flags = flags;
    request.consume_runtime = true;
    let result =
        unsafe { sync_wait::r4os_rust_sync_wait(ptr::addr_of_mut!((*channel).queue), &request) };
    let (closed, sequence) = {
        let _runtime = RuntimeGuard::acquire();
        unsafe { ((*channel).queue.core.closing, (*channel).sequence) }
    };
    if closed || result == 3 || result == 4 {
        return -2;
    }
    if result != 1 && result != 2 {
        return -3;
    }
    unsafe {
        out.write(sequence);
    }
    i32::from(sequence != last || result == 1)
}
const _: () = {
    assert!(core::mem::size_of::<Channel>() == 72);
};
