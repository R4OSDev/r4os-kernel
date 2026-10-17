//! One bounded extension of the existing TCP performance-contract diagnostic.
//! It never resets a foreign slot/queue/counter. Failed retirement retains
//! this resident ledger and its exclusive gate, rather than inventing success.
use super::*;
use crate::{
    platform::timer,
    sched::{
        scheduler,
        task::{
            diagnostic::{self, Ledger},
            task_registry,
            task_types::{Role, State, Task},
        },
        task_context::Admission,
    },
};
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32};
const WORKERS: usize = 11;
struct Cell<T>(UnsafeCell<T>);
// One admitted controller owns each ledger/fixture. Worker request slots are
// immutable from release publication until the exact Task has been reaped.
unsafe impl<T> Sync for Cell<T> {}
static BUSY: AtomicBool = AtomicBool::new(false);
static LEDGER: Cell<Ledger<WORKERS>> = Cell(UnsafeCell::new(Ledger::EMPTY));
static REQUESTS: Cell<[Option<(Target, u64)>; WORKERS]> = Cell(UnsafeCell::new([None; WORKERS]));
static TASKS: [AtomicPtr<Task>; WORKERS] = [const { AtomicPtr::new(ptr::null_mut()) }; WORKERS];
static DONE: AtomicU32 = AtomicU32::new(0);
static CURRENT: AtomicU32 = AtomicU32::new(0);
static PHASE: AtomicU32 = AtomicU32::new(0);
static CONNECTIONS_OWNED: Cell<[Option<ConnectionIdentity>; 6]> = Cell(UnsafeCell::new([None; 6]));
static LISTENERS_OWNED: Cell<[Option<tcp::ListenerIdentity>; 2]> = Cell(UnsafeCell::new([None; 2]));
fn owned<T>(f: impl FnOnce() -> T) -> Option<T> {
    let _owner = tcp_owner::Guard::acquire()?;
    Some(f())
}
fn connection(index: usize, syn: bool) -> Option<(u32, Target)> {
    let identity = owned(|| unsafe {
        let id = if syn {
            tcp::begin_live_connect([198, 51, 100, 28], 65098, 0x1100)
        } else {
            tcp::self_test_connect([198, 51, 100, 28], 65096 + index as u16)
        };
        if id <= 0 {
            return None;
        }
        tcp::connection_identity(id as u32)
    })??;
    unsafe {
        (*CONNECTIONS_OWNED.0.get())[index] = Some(identity);
    }
    Some((
        identity.connection_id,
        Target::connection(identity.connection_id, Need::Readable)?,
    ))
}
fn spawn(index: usize, target: Target) -> bool {
    unsafe {
        (*REQUESTS.0.get())[index] = Some((target, 5000));
    }
    let t = unsafe {
        (&mut *LEDGER.0.get()).create(b"tcp-wait-check", worker, Role::Interactive, None)
    };
    if t.is_null() {
        return false;
    }
    TASKS[index].store(t, Ordering::Release);
    diagnostic::start(t)
}
unsafe extern "C" fn worker() {
    let t = scheduler::current().cast::<Task>();
    let Some(index) = TASKS.iter().position(|p| p.load(Ordering::Acquire) == t) else {
        return;
    };
    let Some(_admission) = Admission::acquire(true) else {
        return;
    };
    let Some((target, timeout)) = (unsafe { (*REQUESTS.0.get())[index] }) else {
        return;
    };
    if target.wait(timeout) {
        CURRENT.fetch_or(1 << index, Ordering::Relaxed);
    }
    DONE.fetch_or(1 << index, Ordering::Release);
}
fn enrolled(index: usize) -> bool {
    let _runtime = RuntimeGuard::acquire();
    let t = TASKS[index].load(Ordering::Acquire);
    if t.is_null() || DONE.load(Ordering::Acquire) & (1 << index) != 0 {
        return false;
    }
    unsafe {
        let Some((target, _)) = (*REQUESTS.0.get())[index] else {
            return false;
        };
        let c = task_registry::core(t);
        (*c).state == State::Blocked
            && (*c).wait_node.queue
                == ptr::addr_of_mut!((*target.channel().queue(target.need.index())).core)
    }
}
fn await_enrolled(index: usize) -> bool {
    let end = timer::tick_count().saturating_add(1000);
    while !enrolled(index) {
        if DONE.load(Ordering::Acquire) & (1 << index) != 0 || timer::tick_count() >= end {
            return false;
        }
        diagnostic::sleep(1, b"tcp-wait-enrollment");
    }
    true
}
fn join(index: usize, current: bool) -> bool {
    let t = TASKS[index].load(Ordering::Acquire);
    let ok = unsafe { (&mut *LEDGER.0.get()).finish_task(t, false) };
    if ok {
        TASKS[index].store(ptr::null_mut(), Ordering::Release);
    }
    ok && DONE.load(Ordering::Acquire) & (1 << index) != 0
        && (CURRENT.load(Ordering::Acquire) & (1 << index) != 0) == current
}
fn write(id: u32, data: &[u8]) -> bool {
    owned(|| unsafe { tcp::loopback_write(id, data.as_ptr(), data.len()) == data.len() as i32 })
        == Some(true)
}
fn read(id: u32, expected: &[u8]) -> bool {
    let mut bytes = [0; 8];
    owned(|| unsafe {
        tcp::read(id, &mut bytes) == expected.len() as i32 && bytes[..expected.len()] == *expected
    }) == Some(true)
}
fn listen(port: u16, index: usize) -> Option<Target> {
    let identity = owned(|| unsafe {
        if tcp::has_listener(port) || !tcp::listen(port) {
            return None;
        }
        tcp::listener_identity(port)
    })??;
    unsafe {
        (*LISTENERS_OWNED.0.get())[index] = Some(identity);
    }
    Target::listener(port, Need::PortConnection)
}
fn incoming(port: u16, remote_port: u16, index: usize) -> Option<u32> {
    owned(|| unsafe {
        let mut view = tcp::SegmentView {
            source_ip: [198, 51, 100, 29],
            dest_ip: [192, 0, 2, 1],
            source_port: remote_port,
            dest_port: port,
            seq: 0x3000,
            ack: 0,
            flags: tcp::FLAG_SYN,
            window: 4096,
            mss: 1460,
            window_scale: 0,
            window_scale_present: false,
            payload: Text::literal(b""),
        };
        let id = tcp::accept_inbound(view, 0x4000)?;
        (*CONNECTIONS_OWNED.0.get())[index] = tcp::connection_identity(id);
        if !tcp::commit_sent(id, tcp::FLAG_SYN | tcp::FLAG_ACK, b"", timer::tick_count()) {
            return None;
        }
        view.flags = tcp::FLAG_ACK;
        view.seq = 0x3001;
        view.ack = 0x4001;
        tcp::apply_rx_view(view)?;
        tcp::established(id).then_some(id)
    })?
}
fn body() -> Option<()> {
    PHASE.store(1, Ordering::Relaxed);
    let (a, ta) = connection(0, false)?;
    let (b, tb) = connection(1, false)?;
    if !spawn(0, ta) || !spawn(1, tb) || !await_enrolled(0) || !await_enrolled(1) {
        return None;
    }
    if !write(a, b"A") || !join(0, true) || !read(a, b"A") || !enrolled(1) {
        return None;
    }
    PHASE.store(2, Ordering::Relaxed);
    let closed = Target::connection(b, Need::Closed)?;
    if !spawn(2, closed) || !await_enrolled(2) {
        return None;
    }
    owned(|| unsafe {
        tcp::abort(b, Text::literal(b"tcp-wait-check"));
    })?;
    if !join(1, false) || !join(2, false) {
        return None;
    }
    owned(|| unsafe {
        tcp::close(b);
    })?;
    PHASE.store(3, Ordering::Relaxed);
    let (c, tc) = connection(2, false)?;
    let (Endpoint::Connection(old), Endpoint::Connection(new)) = (tb.endpoint, tc.endpoint) else {
        return None;
    };
    if old.slot != new.slot || old.generation == new.generation || tb.current() {
        return None;
    }
    if !spawn(3, tc) || !await_enrolled(3) {
        return None;
    }
    // Readiness on A, including an already-ready pre-enrollment transition,
    // never resumes the independently enrolled replacement connection C.
    if !write(a, b"pre") || !spawn(8, ta) || !join(8, true) || !read(a, b"pre") || !enrolled(3) {
        return None;
    }
    if !write(c, b"C") || !join(3, true) || !read(c, b"C") {
        return None;
    }
    let before = timer::tick_count();
    if !ta.wait(2) || timer::tick_count().saturating_sub(before) < 2 {
        return None;
    }
    let port = 65099;
    PHASE.store(4, Ordering::Relaxed);
    let old_listener = listen(port, 0)?;
    let old_data = Target::listener(port, Need::PortData)?;
    if !spawn(4, old_listener) || !spawn(5, old_data) || !await_enrolled(4) || !await_enrolled(5) {
        return None;
    }
    let old_server = incoming(port, 54028, 3)?;
    if !join(4, true) || !enrolled(5) {
        return None;
    }
    if !write(old_server, b"L") || !join(5, true) || !read(old_server, b"L") {
        return None;
    }
    if !spawn(6, old_data) || !await_enrolled(6) {
        return None;
    }
    PHASE.store(5, Ordering::Relaxed);
    tcp::finish_listener(old_listener.listener_identity(), false);
    let new_listener = listen(port, 1)?;
    if new_listener.identity() == old_listener.identity()
        || !join(6, false)
        || tcp::query_listener(old_listener.listener_identity(), false).is_ok()
        || tcp::query_listener(new_listener.listener_identity(), false) != Ok(None)
    {
        return None;
    }
    if !spawn(7, new_listener) || !await_enrolled(7) {
        return None;
    }
    if !write(old_server, b"old") || !enrolled(7) {
        return None;
    }
    // An old operation's late cleanup must not close the new listener.
    tcp::finish_listener(old_listener.listener_identity(), true);
    if !new_listener.current() || !enrolled(7) {
        return None;
    }
    incoming(port, 54029, 4)?;
    if !join(7, true) {
        return None;
    }
    PHASE.store(6, Ordering::Relaxed);
    let (e, _) = connection(5, true)?;
    let established = Target::connection(e, Need::Established)?;
    if !spawn(9, established) || !await_enrolled(9) {
        return None;
    }
    owned(|| unsafe {
        let identity = tcp::connection_identity(e)?;
        // Copy the admitted endpoint into a wire view; no table borrow spans
        // the actual state application or publication at owner release.
        let info = tcp::connection_info(identity.slot as u32)?;
        let view = tcp::SegmentView {
            source_ip: info.remote_ip,
            dest_ip: [192, 0, 2, 1],
            source_port: info.remote_port,
            dest_port: info.local_port,
            seq: 0x2200,
            ack: 0x1101,
            flags: tcp::FLAG_SYN | tcp::FLAG_ACK,
            window: 4096,
            mss: 1460,
            window_scale: 0,
            window_scale_present: false,
            payload: Text::literal(b""),
        };
        tcp::apply_rx_view(view)
    })??;
    if !join(9, true) {
        return None;
    }
    PHASE.store(7, Ordering::Relaxed);
    let closed = Target::connection(e, Need::Closed)?;
    if !spawn(10, closed) || !await_enrolled(10) {
        return None;
    }
    owned(|| unsafe {
        tcp::close(e);
    })?;
    if !join(10, false) {
        return None;
    }
    Some(())
}
fn cleanup() -> bool {
    // Cancel only these exact pinned tasks, never a shared connection queue.
    for p in &TASKS {
        let t = p.load(Ordering::Acquire);
        if !t.is_null() {
            unsafe {
                scheduler::r4os_rust_scheduler_task(
                    2,
                    t.cast(),
                    WaitResult::Cancelled as u64,
                    0,
                    ptr::null(),
                    0,
                );
            }
        }
    }
    let mut clean = true;
    for p in &TASKS {
        let t = p.load(Ordering::Acquire);
        if !t.is_null() {
            if unsafe { (&mut *LEDGER.0.get()).finish_task(t, true) } {
                p.store(ptr::null_mut(), Ordering::Release);
            } else {
                clean = false;
            }
        }
    }
    for identity in unsafe { &*LISTENERS_OWNED.0.get() }.iter().flatten() {
        tcp::finish_listener(*identity, true);
        clean &= owned(|| unsafe { tcp::listener_identity(identity.port) != Some(*identity) })
            == Some(true);
    }
    for identity in unsafe { &*CONNECTIONS_OWNED.0.get() }.iter().flatten() {
        clean &= owned(|| unsafe {
            !tcp::identity_active(*identity) || tcp::close(identity.connection_id) == 0
        }) == Some(true);
    }
    clean && unsafe { (&*LEDGER.0.get()).summary().2 == 0 }
}
pub(crate) fn run() -> bool {
    if BUSY
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return false;
    }
    let Some(_admission) = Admission::acquire(true) else {
        BUSY.store(false, Ordering::Release);
        return false;
    };
    unsafe {
        LEDGER.0.get().write(Ledger::EMPTY);
        REQUESTS.0.get().write([None; WORKERS]);
        CONNECTIONS_OWNED.0.get().write([None; 6]);
        LISTENERS_OWNED.0.get().write([None; 2]);
    }
    DONE.store(0, Ordering::Relaxed);
    CURRENT.store(0, Ordering::Relaxed);
    let passed = body().is_some();
    let clean = cleanup();
    if clean {
        BUSY.store(false, Ordering::Release);
    }
    if passed && clean {
        crate::boot::foundation::puts(b"TCPWAIT08428 PASS own-tasks=11 retired=11 isolated=A/B readiness=before-enrollment listener=generations close=abort+retire timeout=actual\r\n");
    } else {
        crate::boot::foundation::puts(b"TCPWAIT08428 FAILED phase=");
        crate::boot::log_route::dec(u64::from(PHASE.load(Ordering::Relaxed)));
        crate::boot::foundation::puts(if clean {
            b" cleanup=complete\r\n"
        } else {
            b" cleanup=retained\r\n"
        });
    }
    passed && clean
}
