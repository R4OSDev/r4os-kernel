//! TCP activity queue and nonblocking scheduler predicates. TCP task admission
//! publishes copied readiness before enrollment and before waking; predicates
//! read only atomics while the scheduler runtime owner is held. No caller or
//! connection-table reference spans the sleep, and wakees recheck real state.
use super::{tcp, tcp_owner};
use crate::sched::{
    sync_objects,
    sync_types::{Text, WaitQueue},
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    sync::atomic::{AtomicU64, Ordering},
};
static WORDS: [AtomicU64; tcp::MAX_CONNECTIONS] =
    [const { AtomicU64::new(0) }; tcp::MAX_CONNECTIONS];
static SIGNALS: AtomicU64 = AtomicU64::new(0);
struct Activity(UnsafeCell<WaitQueue>);
// SAFETY: native WaitQueue runtime admission owns enrollment/wake; its storage
// is kernel-lifetime resident. No mutable Queue reference crosses a wait.
unsafe impl Sync for Activity {}
static ACTIVITY: Activity = Activity(UnsafeCell::new(WaitQueue::EMPTY));
/// # Safety
/// Exact TCP task owner remains held. This projection owns no connection,
/// pointer, buffer or lifecycle resource and performs no callbacks or waits.
pub(super) unsafe fn publish_owned() {
    for (slot, word) in WORDS.iter().enumerate() {
        let value = unsafe { tcp::wait_observation_word(slot) };
        if word.load(Ordering::Relaxed) != value {
            word.store(value, Ordering::Release);
        }
    }
}
fn refresh() -> bool {
    // Exact owner release publishes current readiness, including changes that
    // do not explicitly wake activity. A completed TCP operation is therefore
    // visible to a later scheduler predicate under runtime admission.
    let Some(_owner) = tcp_owner::Guard::acquire() else {
        return false;
    };
    true
}
fn flags_for(id: u32) -> u64 {
    for word in &WORDS {
        let value = word.load(Ordering::Acquire);
        if value as u32 == id && id != 0 {
            return value >> 48;
        }
    }
    6 /* original unknown connection is readable and closed */
}
fn port_ready(port: u16, flag: u64) -> bool {
    WORDS.iter().any(|word| {
        let value = word.load(Ordering::Acquire);
        value as u32 != 0 && ((value >> 32) as u16) == port && (value >> 48) & flag != 0
    })
}
#[derive(Clone, Copy)]
enum Need {
    Established,
    Readable,
    Closed,
    PortData,
    PortConnection,
}
struct Context {
    connection: u32,
    port: u16,
    need: Need,
}
unsafe extern "C" fn needed(raw: *mut c_void) -> bool {
    let c = unsafe { &*raw.cast::<Context>() };
    match c.need {
        Need::Established => {
            let flags = flags_for(c.connection);
            flags & 1 == 0 && flags & 4 == 0
        }
        Need::Readable => flags_for(c.connection) & 2 == 0,
        Need::Closed => flags_for(c.connection) & 4 == 0,
        Need::PortData => !port_ready(c.port, 16),
        Need::PortConnection => !port_ready(c.port, 8),
    }
}
fn wait(mut context: Context, timeout: u64, reason: &'static [u8]) {
    // Refresh is outside runtime admission and may wait for an in-flight foreign
    // TCP buffer copy. The following predicate is bounded and never takes that
    // owner. Producers publish before wake, closing the original lost-wake window.
    if !refresh() {
        return;
    }
    unsafe {
        sync_objects::queue_wait_unless(
            ACTIVITY.0.get(),
            timeout,
            Text::literal(reason),
            needed,
            (&raw mut context).cast(),
        );
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_activity_signal() {
    refresh();
    SIGNALS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        sync_objects::queue_wake_all(ACTIVITY.0.get());
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_activity_count() -> u64 {
    SIGNALS.load(Ordering::Relaxed)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wait_established(id: u32, timeout: u64) {
    wait(
        Context {
            connection: id,
            port: 0,
            need: Need::Established,
        },
        timeout,
        b"tcp-est",
    );
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wait_readable(id: u32, timeout: u64) {
    wait(
        Context {
            connection: id,
            port: 0,
            need: Need::Readable,
        },
        timeout,
        b"tcp-read",
    );
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wait_closed(id: u32, timeout: u64) {
    wait(
        Context {
            connection: id,
            port: 0,
            need: Need::Closed,
        },
        timeout,
        b"tcp-close",
    );
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wait_port_data(port: u16, timeout: u64) {
    wait(
        Context {
            connection: 0,
            port,
            need: Need::PortData,
        },
        timeout,
        b"tcp-accept",
    );
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tcp_wait_port_connection(port: u16, timeout: u64) {
    wait(
        Context {
            connection: 0,
            port,
            need: Need::PortConnection,
        },
        timeout,
        b"tcp-accept",
    );
}
