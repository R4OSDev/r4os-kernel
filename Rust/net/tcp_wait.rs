//! Lifetime-bound TCP readiness queues. TCP admission publishes the complete
//! atomic projection and matching wakes under runtime admission. Predicates
//! never acquire the waitable TCP owner; no table pointer spans sleep.
use super::{tcp, tcp_owner, tcp_runtime::ConnectionIdentity};
use crate::{
    arch::runtime::RuntimeGuard,
    sched::{
        sync_objects,
        sync_types::{Text, WaitQueue},
        task::task_types::WaitResult,
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "tcp_wait_check.rs"]
mod check;
pub(super) use check::run as check;

struct Channel {
    identity: AtomicU64,
    flags: AtomicU64,
    queues: UnsafeCell<[WaitQueue; 3]>,
}
// Queue storage is kernel-lifetime resident. Runtime admission covers every
// enrollment, projection update and wake; the TCP owner supplies source truth.
unsafe impl Sync for Channel {}
impl Channel {
    const fn new() -> Self {
        Self {
            identity: AtomicU64::new(0),
            flags: AtomicU64::new(0),
            queues: UnsafeCell::new([const { WaitQueue::EMPTY }; 3]),
        }
    }
    fn queue(&self, index: usize) -> *mut WaitQueue {
        assert!(index < 3);
        unsafe { self.queues.get().cast::<WaitQueue>().add(index) }
    }
    unsafe fn publish(&self, identity: u64, flags: u64, masks: &[u64]) {
        let old_identity = self.identity.load(Ordering::Relaxed);
        let old_flags = self.flags.load(Ordering::Relaxed);
        if old_identity == identity && old_flags == flags {
            return;
        }
        if old_flags != flags {
            self.flags.store(flags, Ordering::Release);
        }
        if old_identity != identity {
            self.identity.store(identity, Ordering::Release);
        }
        for (index, mask) in masks.iter().enumerate() {
            if identity != old_identity || old_flags & mask == 0 && flags & mask != 0 {
                let woken = unsafe { sync_objects::queue_wake_all(self.queue(index)) };
                WOKEN.fetch_add(woken as u64, Ordering::Relaxed);
            }
        }
    }
}
static CONNECTIONS: [Channel; tcp::MAX_CONNECTIONS] =
    [const { Channel::new() }; tcp::MAX_CONNECTIONS];
static LISTENERS: [Channel; tcp::MAX_LISTENERS] = [const { Channel::new() }; tcp::MAX_LISTENERS];
static WOKEN: AtomicU64 = AtomicU64::new(0);

/// # Safety
/// Exact TCP task owner remains held; no mutable table borrow survives here.
/// This finite resident projection and wake never waits on a TCP operation.
pub(super) unsafe fn publish_owned() {
    let mut connections = [(0, 0); tcp::MAX_CONNECTIONS];
    let mut listeners = [(0, 0); tcp::MAX_LISTENERS];
    unsafe {
        tcp::wait_observations(&mut connections, &mut listeners);
    }
    let _runtime = RuntimeGuard::acquire();
    for (channel, &(identity, flags)) in CONNECTIONS.iter().zip(&connections) {
        // Establishment waits also end on close; readable waits must recheck
        // terminal state even when no further payload will arrive.
        unsafe {
            channel.publish(identity, flags, &[5, 6, 4]);
        }
    }
    for (channel, &(identity, flags)) in LISTENERS.iter().zip(&listeners) {
        unsafe {
            channel.publish(identity, flags, &[8, 16]);
        }
    }
}
pub(super) fn activity_count() -> u64 {
    WOKEN.load(Ordering::Relaxed)
}

#[derive(Clone, Copy)]
pub(super) enum Need {
    Established,
    Readable,
    Closed,
    PortData,
    PortConnection,
}
impl Need {
    fn index(self) -> usize {
        match self {
            Self::Established | Self::PortConnection => 0,
            Self::Readable | Self::PortData => 1,
            Self::Closed => 2,
        }
    }
    fn mask(self) -> u64 {
        match self {
            Self::Established => 5,
            Self::Readable => 2,
            Self::Closed => 4,
            Self::PortConnection => 8,
            Self::PortData => 16,
        }
    }
    fn reason(self) -> &'static [u8] {
        match self {
            Self::Established => b"tcp-est",
            Self::Readable => b"tcp-read",
            Self::Closed => b"tcp-close",
            Self::PortData | Self::PortConnection => b"tcp-accept",
        }
    }
}
#[derive(Clone, Copy)]
enum Endpoint {
    Connection(ConnectionIdentity),
    Listener(tcp::ListenerIdentity),
}
#[derive(Clone, Copy)]
pub(super) struct Target {
    endpoint: Endpoint,
    need: Need,
}
impl Target {
    pub(super) fn connection(id: u32, need: Need) -> Option<Self> {
        assert!(matches!(
            need,
            Need::Established | Need::Readable | Need::Closed
        ));
        let _owner = tcp_owner::Guard::acquire()?;
        Some(Self {
            endpoint: Endpoint::Connection(unsafe { tcp::connection_identity(id) }?),
            need,
        })
    }
    pub(super) fn listener(port: u16, need: Need) -> Option<Self> {
        assert!(matches!(need, Need::PortData | Need::PortConnection));
        let _owner = tcp_owner::Guard::acquire()?;
        Some(Self {
            endpoint: Endpoint::Listener(unsafe { tcp::listener_identity(port) }?),
            need,
        })
    }
    fn channel(self) -> &'static Channel {
        match self.endpoint {
            Endpoint::Connection(id) => &CONNECTIONS[id.slot],
            Endpoint::Listener(id) => &LISTENERS[id.slot],
        }
    }
    fn identity(self) -> u64 {
        match self.endpoint {
            Endpoint::Connection(id) => {
                u64::from(id.connection_id) | (u64::from(id.generation) << 32)
            }
            Endpoint::Listener(id) => id.generation,
        }
    }
    pub(super) fn listener_identity(self) -> tcp::ListenerIdentity {
        match self.endpoint {
            Endpoint::Listener(id) => id,
            _ => panic!("TCP listener wait identity"),
        }
    }
    pub(super) fn current(self) -> bool {
        self.channel().identity.load(Ordering::Acquire) == self.identity()
    }
    pub(super) fn wait(self, timeout: u64) -> bool {
        let mut context = self;
        let result = unsafe {
            sync_objects::queue_wait_unless(
                self.channel().queue(self.need.index()),
                timeout,
                Text::literal(self.need.reason()),
                needed,
                ptr::addr_of_mut!(context).cast(),
            )
        };
        result != WaitResult::Failed && self.current()
    }
}
unsafe extern "C" fn needed(raw: *mut c_void) -> bool {
    let context = unsafe { *raw.cast::<Target>() };
    context.current() && context.channel().flags.load(Ordering::Acquire) & context.need.mask() == 0
}
