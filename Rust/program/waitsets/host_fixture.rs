//! Host seams for the actual WaitSet/notification owners. This replaces only
//! scheduler, heap and current-instance hardware boundaries. Resource admission
//! seams test the actual new channel/Set protocol, not native IO/service owners.
#![allow(dead_code)]
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};
thread_local! {
    static SOURCE: Cell<*mut program::wait_sources::Channel> = const { Cell::new(core::ptr::null_mut()) };
    static SOURCE_READY: Cell<bool> = const { Cell::new(false) };
    static DEPTH: Cell<u64> = const { Cell::new(0) };
    static INSTANCE: Cell<*mut program::context::Instance> = const { Cell::new(core::ptr::null_mut()) };
    static CORE: Cell<*mut sched::task::task_registry::Core> = const { Cell::new(core::ptr::null_mut()) };
    static CLOCK: Cell<platform::monotonic::Snapshot> = const { Cell::new(platform::monotonic::Snapshot { flags: 1, source: 1, generation: 1, event_backend: 1, instant_ns: 10, event: platform::monotonic::Event { requested_hz: 1000 } }) };
    static LIVE: RefCell<HashMap<usize, std::alloc::Layout>> = RefCell::new(HashMap::new());
    static FAIL_FREE: Cell<bool> = const { Cell::new(false) };
    static PINS: Cell<usize> = const { Cell::new(0) };
    static PARK: RefCell<Option<Box<dyn FnMut(u64)>>> = const { RefCell::new(None) };
    static PARK_QUEUE: Cell<*mut sched::sync_types::WaitQueue> = const { Cell::new(core::ptr::null_mut()) };
    static WAKE: Cell<u32> = const { Cell::new(0) };
}
mod arch {
    pub mod runtime {
        pub fn r4os_rust_runtime_control(_: u8, _: u64) -> u64 {
            super::super::DEPTH.with(|d| u64::from(d.get() != 0))
        }
        pub fn save_runtime() -> u64 {
            super::super::DEPTH.with(|d| {
                let old = d.get();
                d.set(old + 1);
                old
            })
        }
        pub fn restore_runtime(flags: u64) {
            super::super::DEPTH.with(|d| {
                assert_eq!(d.get(), flags + 1);
                d.set(flags);
            });
        }
        pub struct RuntimeGuard(u64);
        impl RuntimeGuard {
            pub fn acquire() -> Self {
                Self(save_runtime())
            }
        }
        impl Drop for RuntimeGuard {
            fn drop(&mut self) {
                restore_runtime(self.0);
            }
        }
    }
}
mod memory {
    pub mod heap {
        pub unsafe fn r4os_rust_heap_alloc(size: usize, alignment: usize) -> *mut u8 {
            super::super::DEPTH.with(|d| assert_eq!(d.get(), 0));
            let layout = std::alloc::Layout::from_size_align(size, alignment).unwrap();
            let result = unsafe { std::alloc::alloc(layout) };
            assert!(!result.is_null());
            super::super::LIVE
                .with(|m| assert!(m.borrow_mut().insert(result.addr(), layout).is_none()));
            result
        }
        pub unsafe fn r4os_rust_heap_free(pointer: *mut u8, size: usize) -> i32 {
            super::super::DEPTH.with(|d| assert_eq!(d.get(), 0));
            if super::super::FAIL_FREE.with(|f| f.replace(false)) {
                return -1;
            }
            let layout =
                super::super::LIVE.with(|m| m.borrow_mut().remove(&pointer.addr()).unwrap());
            assert_eq!(layout.size(), size);
            unsafe {
                pointer.write_bytes(0xdd, size);
                std::alloc::dealloc(pointer, layout);
            }
            0
        }
    }
}
mod platform {
    pub mod monotonic {
        #[derive(Clone, Copy)]
        pub struct Event {
            pub requested_hz: u32,
        }
        #[derive(Clone, Copy)]
        pub struct Snapshot {
            pub flags: u32,
            pub source: u32,
            pub generation: u32,
            pub event_backend: u32,
            pub instant_ns: u64,
            pub event: Event,
        }
        pub fn snapshot() -> Snapshot {
            super::super::CLOCK.with(|c| c.get())
        }
    }
}
mod sched {
    pub mod scheduler {
        pub fn current() -> *mut super::task::task_registry::Core {
            super::super::CORE.with(|c| c.get())
        }
    }
    pub mod task {
        pub mod task_registry {
            pub struct Core {
                pub id: u32,
                pub generation: u64,
                pub preempt_disable_depth: u32,
                pub held_lock_count: u32,
            }
            pub unsafe fn core(raw: *mut Core) -> *mut Core {
                raw
            }
        }
    }
    pub mod task_context {
        #[derive(Clone, Copy)]
        pub struct Unwind;
        impl Unwind {
            pub fn admitted(self) -> bool {
                true
            }
        }
        pub unsafe fn enter() -> Unwind {
            super::super::PINS.with(|c| c.set(c.get() + 1));
            Unwind
        }
        pub unsafe fn leave(_: Unwind) -> bool {
            super::super::PINS.with(|c| {
                assert!(c.get() > 0);
                c.set(c.get() - 1);
            });
            true
        }
    }
    pub mod sync_types {
        use core::ffi::c_void;
        pub struct Text;
        impl Text {
            pub fn literal(_: &'static [u8]) -> Self {
                Self
            }
        }
        pub struct QueueCore {
            pub count: u32,
        }
        pub struct WaitQueue {
            pub core: QueueCore,
            pub closed: bool,
        }
        impl WaitQueue {
            pub const EMPTY: Self = Self {
                core: QueueCore { count: 0 },
                closed: false,
            };
        }
        pub struct WaitRequest {
            pub timeout: u64,
            pub reason: Text,
            pub predicate: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
            pub context: *mut c_void,
            pub release: Option<unsafe extern "C" fn(*mut c_void)>,
            pub release_context: *mut c_void,
            pub flags: u64,
            pub consume_runtime: bool,
        }
        impl WaitRequest {
            pub fn predicate(
                timeout: u64,
                reason: Text,
                predicate: unsafe extern "C" fn(*mut c_void) -> bool,
                context: *mut c_void,
            ) -> Self {
                Self {
                    timeout,
                    reason,
                    predicate: Some(predicate),
                    context,
                    release: None,
                    release_context: core::ptr::null_mut(),
                    flags: 0,
                    consume_runtime: false,
                }
            }
        }
    }
    pub mod sync_wait {
        use super::sync_types::*;
        pub unsafe fn r4os_rust_sync_queue(
            queue: *mut WaitQueue,
            operation: u8,
            _: *mut core::ffi::c_void,
            _: u32,
            _: *mut core::ffi::c_void,
        ) -> u32 {
            super::super::DEPTH.with(|d| assert!(d.get() > 0));
            unsafe {
                let count = (*queue).core.count;
                if operation == 4 {
                    (*queue).closed = true;
                }
                (*queue).core.count = 0;
                super::super::PARK_QUEUE.with(|q| {
                    if q.get() == queue {
                        super::super::WAKE.with(|w| w.set(if operation == 4 { 3 } else { 1 }));
                    }
                });
                count
            }
        }
        pub unsafe fn r4os_rust_sync_wait(queue: *mut WaitQueue, request: &WaitRequest) -> u32 {
            assert!(request.consume_runtime);
            super::super::DEPTH.with(|d| assert_eq!(d.get(), 1));
            super::super::PINS.with(|c| assert_eq!(c.get(), 0, "no unwind pin across park"));
            if let Some(predicate) = request.predicate {
                if !unsafe { predicate(request.context) } {
                    crate::arch::runtime::restore_runtime(request.flags);
                    return 1;
                }
            }
            unsafe {
                (*queue).core.count = 1;
            }
            super::super::PARK_QUEUE.with(|q| q.set(queue));
            super::super::WAKE.with(|w| w.set(0));
            crate::arch::runtime::restore_runtime(request.flags);
            let mut hook = super::super::PARK.with(|p| {
                p.borrow_mut()
                    .take()
                    .expect("test must arrange actual park boundary")
            });
            hook(request.timeout);
            let result = super::super::WAKE.with(|w| w.get());
            super::super::PARK_QUEUE.with(|q| q.set(core::ptr::null_mut()));
            // Close may have freed queue backing in hook. Never read it here.
            if result == 0 {
                panic!("test hook must wake or detach original queue");
            }
            result
        }
    }
}
// The launcher supplies only these three absolute source paths; the tested
// modules below are the production owners, never textual substitutions.
mod driver {
    #[path = "TREAP_SOURCE"]
    pub mod treap;
}
mod contract {
    pub use r4os_contract::*;
}
fn resource<T>(
    input: program::wait_sources::Identity,
    action: impl FnOnce(*mut program::wait_sources::Channel) -> Result<T, i32>,
) -> Result<T, i32> {
    DEPTH.with(|d| assert_eq!(d.get(), 0, "source owner must precede Runtime"));
    let channel = SOURCE.with(|v| v.get());
    if channel.is_null() {
        return Err(contract::waitset_error_stale);
    }
    unsafe { input.locked(channel, SOURCE_READY.with(|v| v.get()), action) }
}
mod service {
    pub mod endpoint {
        pub(crate) unsafe fn waitset_source<T>(
            input: crate::program::wait_sources::Identity,
            _: crate::contract::ProgramProcessHandle,
            action: impl FnOnce(*mut crate::program::wait_sources::Channel) -> Result<T, i32>,
        ) -> Result<T, i32> {
            crate::resource(input, action)
        }
    }
}
mod program {
    pub mod gui_activity {
        pub(super) unsafe fn waitset_membership(_: *mut super::wait_sources::Channel, _: bool) {}
        pub(super) unsafe fn waitset_source<T>(
            input: super::wait_sources::Identity,
            action: impl FnOnce(*mut super::wait_sources::Channel) -> Result<T, i32>,
        ) -> Result<T, i32> {
            crate::resource(input, action)
        }
    }
    pub mod async_io {
        pub(super) unsafe fn waitset_source<T>(
            input: super::wait_sources::Identity,
            action: impl FnOnce(*mut super::wait_sources::Channel) -> Result<T, i32>,
        ) -> Result<T, i32> {
            crate::resource(input, action)
        }
    }
    pub mod completions {
        pub(super) unsafe fn waitset_source<T>(
            input: super::wait_sources::Identity,
            action: impl FnOnce(*mut super::wait_sources::Channel) -> Result<T, i32>,
        ) -> Result<T, i32> {
            crate::resource(input, action)
        }
    }
    pub mod threads {
        pub(super) unsafe fn waitset_source<T>(
            input: super::wait_sources::Identity,
            action: impl FnOnce(*mut super::wait_sources::Channel) -> Result<T, i32>,
        ) -> Result<T, i32> {
            crate::resource(input, action)
        }
    }
    #[path = "WAIT_SOURCES_SOURCE"]
    pub(crate) mod wait_sources;
    pub mod context {
        pub struct Instance {
            pub runtime_payload: *mut core::ffi::c_void,
            pub notifications: super::notifications::Owner,
        }
        pub unsafe fn handle() -> Option<crate::contract::ProgramProcessHandle> {
            Some(crate::contract::ProgramProcessHandle {
                instance_id: 1,
                generation: 1,
                ..Default::default()
            })
        }
        pub unsafe fn current() -> *mut Instance {
            super::super::INSTANCE.with(|i| i.get())
        }
    }
    pub mod payload_types {
        pub struct Runtime {
            pub wait_sets: super::waitsets::Owner,
        }
    }
    #[path = "CASES_SOURCE"]
    mod cases;
    #[path = "NOTIFICATIONS_SOURCE"]
    pub mod notifications;
    #[path = "WAITSETS_SOURCE"]
    pub mod waitsets;
}
