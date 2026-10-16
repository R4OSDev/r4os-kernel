//! Sole DHCP client, lease statistics, link coordinator and worker owner.
//! Only small metadata changes hold the common network owner. The original
//! sleepable operation mutex spans explicit operations; peer/R4P calls and
//! pageable copies retain no resident Rust reference or CPU owner.
use super::{
    adapters, builders, config,
    config_writer::Addresses,
    dhcp, dhcp_messages,
    dhcp_runtime::{self, Action, Coordinator, Operation, State},
    protocol_runtime, timing, transmit,
    udp_sockets::Info,
};
use crate::{
    boot::foundation,
    memory::owner::Guard,
    sched::{
        scheduler, sync_locks,
        sync_types::{Mutex, Text, WaitQueue},
        task::{self, task_resources, task_types},
    },
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr, slice};

const MAX_PACKET: usize = 1536;
const MAX_LOOPS: usize = 200_000;
const POLL_ROUNDS: usize = 512;
const ATTEMPTS: u8 = 3;
const BROADCAST: [u8; 4] = [255; 4];
const ZERO_IP: [u8; 4] = [0; 4];

#[repr(C)]
pub struct Peers {
    pub refresh: unsafe extern "C" fn(usize),
    pub poll: unsafe extern "C" fn(usize),
    pub idle: unsafe extern "C" fn(),
    pub bind: unsafe extern "C" fn(u16) -> i32,
    pub close: unsafe extern "C" fn(u32) -> i32,
    pub recv: unsafe extern "C" fn(u32, *mut Info, *mut u8, usize) -> i32,
    pub send:
        unsafe extern "C" fn(usize, u32, *const [u8; 4], *const [u8; 4], *const u8, usize) -> u8,
    pub arp_flush: unsafe extern "C" fn(),
}
struct Metadata {
    stats: dhcp::Stats,
    coordinator: Coordinator,
    task_started: bool,
    task_id: u32,
    task_generation: u64,
    task_retry_requested: bool,
    task_next_retry_tick: u64,
    last_logged_state: State,
    last_logged_link_generation: u32,
    peers: *const Peers,
}
impl Metadata {
    const EMPTY: Self = Self {
        stats: dhcp::Stats::EMPTY,
        coordinator: Coordinator::EMPTY,
        task_started: false,
        task_id: 0,
        task_generation: 0,
        task_retry_requested: false,
        task_next_retry_tick: 0,
        last_logged_state: State::Disabled,
        last_logged_link_generation: 0,
        peers: ptr::null(),
    };
}
struct Storage(UnsafeCell<Metadata>);
// SAFETY: all metadata access is a bounded operation under the network owner.
unsafe impl Sync for Storage {}
static STORAGE: Storage = Storage(UnsafeCell::new(Metadata::EMPTY));
const EMPTY_MUTEX: Mutex = Mutex {
    owner: 0,
    owner_generation: 0,
    depth: 0,
    rank: 10,
    mode: 0,
    name: Text::literal(b"dhcp-operation"),
    queue: WaitQueue::EMPTY,
    donation_rank: 255,
    handoff_pending: false,
};
struct OperationLock(UnsafeCell<Mutex>);
// SAFETY: mutex fields and waiters belong exclusively to the native sync owner;
// client code passes stable raw storage, never a borrow spanning a task switch.
unsafe impl Sync for OperationLock {}
static OPERATION: OperationLock = OperationLock(UnsafeCell::new(EMPTY_MUTEX));
fn metadata<T>(f: impl FnOnce(&mut Metadata) -> T) -> T {
    let _guard = Guard::acquire(2);
    unsafe { f(&mut *STORAGE.0.get()) }
}
fn peers() -> &'static Peers {
    let p = metadata(|s| s.peers);
    assert!(!p.is_null(), "DHCP peers installed before worker admission");
    // SAFETY: singleton bootstrap installs one immutable kernel-lifetime table.
    unsafe { &*p }
}
pub(super) fn stats() -> dhcp::Stats {
    metadata(|s| s.stats)
}
fn error(reason: &'static [u8]) {
    metadata(|s| s.stats.last_error = Text::literal(reason));
}
fn error_result(reason: &'static [u8], result: u8) -> u8 {
    error(reason);
    result
}
fn clear_bound() {
    metadata(|s| {
        s.stats.lease.bound = false;
        s.stats.lease_acquired_tick = 0;
    });
}
fn clear_expectation() {
    let e = dhcp_messages::DhcpExpectation {
        expected: 0,
        xid: 0,
        mac: [0; 6],
    };
    unsafe { dhcp_messages::r4os_rust_dhcp_expectation_set(&e) };
}
fn expect(expected: u8, xid: u32, mac: [u8; 6]) {
    let e = dhcp_messages::DhcpExpectation { expected, xid, mac };
    unsafe { dhcp_messages::r4os_rust_dhcp_expectation_set(&e) };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DhcpLeaseTiming {
    pub bound: bool,
    pub acquired_tick: u64,
    pub elapsed_seconds: u32,
    pub remaining_seconds: u32,
    pub renew_in_seconds: u32,
    pub rebind_in_seconds: u32,
}
fn lease_timing() -> DhcpLeaseTiming {
    let s = stats();
    if !s.lease.bound {
        return DhcpLeaseTiming {
            bound: false,
            acquired_tick: 0,
            elapsed_seconds: 0,
            remaining_seconds: 0,
            renew_in_seconds: 0,
            rebind_in_seconds: 0,
        };
    }
    let elapsed = timing::elapsed_seconds_since(s.lease_acquired_tick);
    let remaining = |seconds| {
        if s.lease.lease_seconds == dhcp::INFINITE_SECONDS {
            dhcp::INFINITE_SECONDS
        } else {
            timing::seconds_remaining(seconds, elapsed)
        }
    };
    DhcpLeaseTiming {
        bound: true,
        acquired_tick: s.lease_acquired_tick,
        elapsed_seconds: elapsed,
        remaining_seconds: remaining(s.lease.lease_seconds),
        renew_in_seconds: remaining(s.lease.renew_seconds),
        rebind_in_seconds: remaining(s.lease.rebind_seconds),
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DhcpRuntimeStatus {
    pub state: State,
    pub desired_dhcp: bool,
    pub link_up: bool,
    pub task_started: bool,
    pub task_id: u32,
    pub task_generation: u64,
    pub link_generation: u32,
    pub operation_generation: u32,
    pub transition_tick: u64,
    pub next_retry_tick: u64,
    pub last_timeout_tick: u64,
    pub retry_round: u8,
    pub operation_active: bool,
    pub recoveries: u64,
    pub starts: u64,
    pub cancels: u64,
}
fn runtime_status() -> DhcpRuntimeStatus {
    let desired_dhcp = config::settings().dhcp_enabled;
    let link_up = adapters::snapshot(0).is_some_and(|a| a.link == 2);
    metadata(|s| DhcpRuntimeStatus {
        state: s.coordinator.state,
        desired_dhcp,
        link_up,
        task_started: s.task_started,
        task_id: s.task_id,
        task_generation: s.task_generation,
        link_generation: s.coordinator.link_generation,
        operation_generation: s.coordinator.operation_generation,
        transition_tick: s.coordinator.transition_tick,
        next_retry_tick: s.coordinator.next_retry_tick,
        last_timeout_tick: s.coordinator.last_timeout_tick,
        retry_round: s.coordinator.retry_round,
        operation_active: s.coordinator.operation_active,
        recoveries: s.coordinator.recoveries,
        starts: s.coordinator.starts,
        cancels: s.coordinator.cancels,
    })
}
fn text(value: Text) {
    if value.len != 0 {
        // SAFETY: all recorded labels retain the original kernel-string lifetime.
        foundation::puts(unsafe { slice::from_raw_parts(value.pointer, value.len) });
    }
}
fn log_ip(ip: [u8; 4]) {
    for (index, byte) in ip.into_iter().enumerate() {
        if index != 0 {
            foundation::puts(b".");
        }
        foundation::dec(u64::from(byte));
    }
}
fn log_runtime(before: Option<State>, reason: Text) {
    let snapshot = metadata(|s| {
        let c = &s.coordinator;
        if before.is_some_and(|b| b == c.state)
            && s.last_logged_state == c.state
            && s.last_logged_link_generation == c.link_generation
        {
            return None;
        }
        s.last_logged_state = c.state;
        s.last_logged_link_generation = c.link_generation;
        Some((
            c.state,
            c.link_generation,
            c.operation_generation,
            c.retry_round,
        ))
    });
    let Some((state, link, operation, retry)) = snapshot else {
        return;
    };
    let cfg = config::settings();
    foundation::puts(b"DHCP05913 state=");
    text(dhcp_runtime::state_name(state));
    foundation::puts(b" link_gen=");
    foundation::dec(u64::from(link));
    foundation::puts(b" op_gen=");
    foundation::dec(u64::from(operation));
    foundation::puts(b" retry=");
    foundation::dec(u64::from(retry));
    foundation::puts(b" source=");
    text(config::source_name());
    foundation::puts(b" ip=");
    log_ip(cfg.local_ip);
    foundation::puts(b" mask=");
    log_ip(cfg.netmask);
    foundation::puts(b" gateway=");
    log_ip(cfg.gateway_ip);
    foundation::puts(b" dns=");
    log_ip(cfg.dns_ip);
    foundation::puts(b" error=");
    text(reason);
    foundation::puts(b"\r\n");
}
fn cancel_coordinator(keep_desired: bool) -> State {
    let now = timing::now_ticks();
    metadata(|s| {
        let before = s.coordinator.state;
        s.coordinator.cancel(now, keep_desired);
        before
    })
}
fn cancel(reason: Text) -> bool {
    let was_pending = metadata(|s| s.stats.operation_pending || s.coordinator.operation_active);
    clear_expectation();
    metadata(|s| {
        s.stats.operation_pending = false;
        s.stats.pending_label = Text::literal(b"idle");
        if was_pending {
            s.stats.last_error = reason;
        }
    });
    let before = cancel_coordinator(config::settings().dhcp_enabled);
    log_runtime(Some(before), reason);
    was_pending
}
fn begin(label: &'static [u8], operation: Option<Operation>) -> bool {
    if !unsafe { sync_locks::mutex_lock(OPERATION.0.get(), u64::MAX) } {
        error(b"operation-lock");
        return false;
    }
    let now = timing::now_ticks();
    let (before, started) = metadata(|s| {
        let before = s.coordinator.state;
        (
            before,
            operation.is_none_or(|kind| s.coordinator.start_operation(now, kind)),
        )
    });
    if !started {
        unsafe {
            sync_locks::mutex_unlock(OPERATION.0.get());
        }
        error(b"operation-busy");
        return false;
    }
    metadata(|s| s.stats.operation_pending = true);
    clear_expectation();
    metadata(|s| s.stats.pending_label = Text::literal(label));
    log_runtime(Some(before), Text::literal(label));
    true
}
fn finish(result: u8, coordinator_operation: bool) {
    clear_expectation();
    let now = timing::now_ticks();
    let (before, reason) = metadata(|s| {
        s.stats.operation_pending = false;
        s.stats.pending_label = Text::literal(b"idle");
        let before = s.coordinator.state;
        if coordinator_operation {
            let reason = s.stats.last_error;
            // SAFETY: error labels are resident immutable kernel strings.
            let timeout = reason.len >= 7
                && unsafe { slice::from_raw_parts(reason.pointer, reason.len) }
                    .windows(7)
                    .any(|w| w == b"timeout");
            s.coordinator.finish_operation(
                now,
                result == 0,
                timeout,
                timing::ms_to_ticks(250),
                timing::ms_to_ticks(4000),
            );
        }
        (before, s.stats.last_error)
    });
    if coordinator_operation {
        log_runtime(Some(before), reason);
    }
    unsafe {
        sync_locks::mutex_unlock(OPERATION.0.get());
    }
}
struct Socket(u32);
impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            (peers().close)(self.0);
        }
    }
}
fn bind() -> Option<Socket> {
    let socket = unsafe { (peers().bind)(dhcp::CLIENT_PORT) };
    if socket <= 0 {
        error(b"socket-bind");
        None
    } else {
        Some(Socket(socket as u32))
    }
}
fn xid(mac: [u8; 6]) -> u32 {
    let value = timing::now_ticks() as u32
        ^ u32::from_be_bytes([mac[0], mac[1], mac[2], mac[3]])
        ^ ((u32::from(mac[4]) << 8) | u32::from(mac[5]));
    if value == 0 { 0x5244_4843 } else { value }
}
fn mac(index: usize) -> [u8; 6] {
    adapters::snapshot(index).map_or([0; 6], |a| a.mac)
}
fn count() -> usize {
    adapters::r4os_rust_net_adapter_count()
}
fn link_down(index: usize) -> bool {
    adapters::snapshot(index).is_some_and(|a| a.link == 1)
}
fn expired(deadline: Option<u64>) -> bool {
    deadline.is_some_and(|d| timing::now_ticks() >= d)
}
fn wait_budget(deadline: Option<u64>, ticks: u64) -> u64 {
    deadline.map_or(ticks, |d| d.saturating_sub(timing::now_ticks()).min(ticks))
}
fn deadline_result(reason: &'static [u8]) -> u8 {
    metadata(|s| {
        s.stats.timeouts += 1;
        s.stats.last_error = Text::literal(reason);
    });
    6
}
fn retry(reason: &'static [u8], attempt: u8) {
    metadata(|s| {
        s.stats.timeouts += 1;
        if attempt < ATTEMPTS {
            s.stats.retries += 1;
        }
        s.stats.last_error = Text::literal(reason);
    });
}
fn can_continue(index: usize) -> bool {
    if !stats().operation_pending || index >= count() {
        return false;
    }
    unsafe {
        (peers().refresh)(index);
    }
    stats().operation_pending && !link_down(index)
}
fn cancelled(index: usize) -> u8 {
    if index >= count() {
        error_result(b"adapter-gone", 1)
    } else if link_down(index) {
        error_result(b"link-down", 2)
    } else {
        error_result(b"cancelled", 6)
    }
}
unsafe fn handle_message(data: *const u8, len: usize) -> bool {
    let mut op = MaybeUninit::uninit();
    let mut report = MaybeUninit::uninit();
    unsafe {
        dhcp_messages::r4os_rust_dhcp_message(
            stats().operation_pending,
            data,
            len,
            op.as_mut_ptr(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    let accepted = metadata(|s| unsafe {
        dhcp_messages::r4os_rust_dhcp_message_apply(&mut s.stats, op.as_ptr(), &report)
    });
    unsafe {
        dhcp_messages::r4os_rust_dhcp_message_log(&report);
    }
    protocol_runtime::r4os_rust_net_note_dhcp_dispatch_failures(report.dispatch_failures);
    if accepted {
        protocol_runtime::r4os_rust_net_note_dhcp_r4p_rx(1);
    }
    accepted
}
fn drain(socket: u32) {
    let mut payload = [0; 1472];
    loop {
        let mut info = MaybeUninit::<Info>::uninit();
        let got = unsafe {
            (peers().recv)(
                socket,
                info.as_mut_ptr(),
                payload.as_mut_ptr(),
                payload.len(),
            )
        };
        if got < 0 {
            error(b"socket-recv");
            return;
        }
        if got == 0 {
            return;
        }
        assert!(got as usize <= payload.len(), "Exact UDP receive bound");
        let info = unsafe { info.assume_init() };
        if info.source_port == dhcp::SERVER_PORT && info.dest_port == dhcp::CLIENT_PORT {
            unsafe {
                handle_message(payload.as_ptr(), got as usize);
            }
        }
    }
}
enum Response {
    Offer { offer: u64, ack: u64 },
    Ack { ack: u64, nak: u64 },
}
impl Response {
    fn received(&self) -> bool {
        let s = stats();
        match *self {
            Self::Offer { offer, ack } => s.offer_rx >= offer && s.ack_rx >= ack,
            Self::Ack { ack, nak } => s.ack_rx >= ack || s.nak_rx >= nak,
        }
    }
}
fn wait(index: usize, socket: u32, response: Response, ticks: u64) -> bool {
    let deadline = timing::Deadline::start(ticks, MAX_LOOPS);
    for _ in 0..MAX_LOOPS {
        if !can_continue(index) {
            return false;
        }
        unsafe {
            (peers().poll)(POLL_ROUNDS);
        }
        drain(socket);
        if response.received() {
            return true;
        }
        if deadline.expired_now() {
            return false;
        }
        unsafe {
            (peers().idle)();
        }
    }
    response.received()
}
fn send(index: usize, socket: u32, source: [u8; 4], dest: [u8; 4], payload: &[u8]) -> u8 {
    unsafe {
        (peers().send)(
            index,
            socket,
            &source,
            &dest,
            payload.as_ptr(),
            payload.len(),
        )
    }
}
fn offer_retry(index: usize, socket: u32, payload: &[u8], deadline: Option<u64>) -> u8 {
    for attempt in 1..=ATTEMPTS {
        let ticks = wait_budget(
            deadline,
            timing::DEFAULT_DHCP_TIMEOUT_TICKS * u64::from(attempt),
        );
        if ticks == 0 {
            return deadline_result(b"offer-timeout");
        }
        if !can_continue(index) {
            return cancelled(index);
        }
        let offer = metadata(|s| {
            s.stats.last_attempt = attempt;
            s.stats.offer_rx + 1
        });
        let result = send(index, socket, ZERO_IP, BROADCAST, payload);
        if result != 0 {
            return error_result(transmit::result_name(result), result);
        }
        // The original ACK target is sampled after sending/polling, not before.
        if wait(
            index,
            socket,
            Response::Offer {
                offer,
                ack: stats().ack_rx,
            },
            ticks,
        ) {
            return 0;
        }
        if !can_continue(index) {
            return cancelled(index);
        }
        retry(b"offer-timeout", attempt);
    }
    error_result(b"offer-timeout", 6)
}
struct Request<'a> {
    index: usize,
    socket: u32,
    source: [u8; 4],
    dest: [u8; 4],
    payload: &'a [u8],
}
fn ack_retry(request: Request<'_>, label: &'static [u8], deadline: Option<u64>) -> u8 {
    for attempt in 1..=ATTEMPTS {
        let ticks = wait_budget(
            deadline,
            timing::DEFAULT_DHCP_TIMEOUT_TICKS * u64::from(attempt),
        );
        if ticks == 0 {
            return deadline_result(label);
        }
        if !can_continue(request.index) {
            return cancelled(request.index);
        }
        let (ack, nak_before) = metadata(|s| {
            s.stats.last_attempt = attempt;
            (s.stats.ack_rx + 1, s.stats.nak_rx)
        });
        let result = send(
            request.index,
            request.socket,
            request.source,
            request.dest,
            request.payload,
        );
        if result != 0 {
            return error_result(transmit::result_name(result), result);
        }
        if wait(
            request.index,
            request.socket,
            Response::Ack {
                ack,
                nak: nak_before + 1,
            },
            ticks,
        ) {
            if stats().nak_rx > nak_before {
                metadata(|s| s.stats.lease.bound = false);
                config::clear_dhcp_lease();
                unsafe {
                    (peers().arp_flush)();
                }
                return error_result(b"nak", 6);
            }
            return 0;
        }
        if !can_continue(request.index) {
            return cancelled(request.index);
        }
        retry(label, attempt);
    }
    error_result(label, 6)
}
enum Build {
    Discover,
    Request,
    Release,
}
fn build(kind: Build, params: builders::WireDhcpBuild, out: &mut [u8]) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        match kind {
            Build::Discover => builders::r4os_rust_wire_build_dhcp_discover(
                &params,
                out.as_mut_ptr(),
                out.len(),
                report.as_mut_ptr(),
            ),
            Build::Request => builders::r4os_rust_wire_build_dhcp_request(
                &params,
                out.as_mut_ptr(),
                out.len(),
                report.as_mut_ptr(),
            ),
            Build::Release => builders::r4os_rust_wire_build_dhcp_release(
                &params,
                out.as_mut_ptr(),
                out.len(),
                report.as_mut_ptr(),
            ),
        }
    }
    let report = unsafe { report.assume_init() };
    protocol_runtime::r4os_rust_net_note_dhcp_dispatch_failures(report.dispatch_failures);
    metadata(|s| {
        if report.reason.len != 0 {
            s.stats.last_error = report.reason;
        }
        if report.length != usize::MAX {
            dhcp::apply_build_metadata(&mut s.stats, report.flags, report.message_type);
        }
    });
    if report.length == usize::MAX {
        return None;
    }
    protocol_runtime::r4os_rust_net_note_dhcp_r4p_build(1);
    Some(report.length)
}
fn params(xid: u32, mac: [u8; 6]) -> builders::WireDhcpBuild {
    builders::WireDhcpBuild {
        xid,
        mac,
        requested_ip: ZERO_IP,
        server_ip: ZERO_IP,
        client_ip: ZERO_IP,
        state: 0,
    }
}
fn normalize(mut lease: dhcp::Lease) -> dhcp::Lease {
    if lease.netmask == ZERO_IP {
        lease.netmask = [255, 255, 255, 0];
    }
    if lease.lease_seconds == dhcp::INFINITE_SECONDS {
        lease.renew_seconds = dhcp::INFINITE_SECONDS;
        lease.rebind_seconds = dhcp::INFINITE_SECONDS;
    } else if lease.lease_seconds >= 3 {
        let renew = lease.lease_seconds / 2;
        let rebind = ((u64::from(lease.lease_seconds) * 7) / 8) as u32;
        if lease.renew_seconds == 0 {
            lease.renew_seconds = renew;
        }
        if lease.rebind_seconds == 0 {
            lease.rebind_seconds = rebind;
        }
        if lease.renew_seconds >= lease.rebind_seconds
            || lease.rebind_seconds >= lease.lease_seconds
        {
            lease.renew_seconds = renew;
            lease.rebind_seconds = rebind;
        }
    } else {
        lease.renew_seconds = 0;
        lease.rebind_seconds = 0;
    }
    lease
}
fn apply_lease() {
    let now = timing::now_ticks();
    let lease = metadata(|s| {
        s.stats.lease = normalize(s.stats.lease);
        s.stats.lease_acquired_tick = now;
        s.stats.lease
    });
    config::apply_addresses(
        Addresses {
            local_ip: lease.offered_ip,
            netmask: lease.netmask,
            gateway_ip: lease.gateway_ip,
            dns_ip: lease.dns_ip,
            dns_configured: lease.dns_configured,
        },
        true,
    );
}
fn acquire_operation(index: usize, deadline: Option<u64>) -> u8 {
    let Some(socket) = bind() else {
        return 6;
    };
    let xid = xid(mac(index));
    metadata(|s| {
        s.stats.lease = dhcp::Lease {
            xid,
            ..dhcp::Lease::EMPTY
        };
        s.stats.lease_acquired_tick = 0;
        s.stats.last_error = Text::literal(b"discover");
    });
    let mut payload = [0; MAX_PACKET];
    let Some(length) = build(Build::Discover, params(xid, mac(index)), &mut payload) else {
        return error_result(b"discover-build", 4);
    };
    expect(1, xid, mac(index));
    let result = offer_retry(index, socket.0, &payload[..length], deadline);
    if result != 0 {
        return result;
    }
    let lease = stats().lease;
    if lease.xid != 0 && lease.xid != xid {
        return error_result(b"xid-mismatch", 6);
    }
    if lease.offered_ip == ZERO_IP {
        return error_result(b"no-offer-ip", 6);
    }
    error(b"request");
    let mut p = params(xid, mac(index));
    p.requested_ip = lease.offered_ip;
    p.server_ip = lease.server_ip;
    let Some(length) = build(Build::Request, p, &mut payload) else {
        return error_result(b"request-build", 4);
    };
    expect(2, xid, mac(index));
    let result = ack_retry(
        Request {
            index,
            socket: socket.0,
            source: ZERO_IP,
            dest: BROADCAST,
            payload: &payload[..length],
        },
        b"ack-timeout",
        deadline,
    );
    if result != 0 {
        return result;
    }
    let lease = stats().lease;
    if !lease.bound || lease.offered_ip == ZERO_IP {
        return error_result(b"not-bound", 6);
    }
    apply_lease();
    unsafe {
        (peers().arp_flush)();
    }
    error_result(b"bound", 0)
}
fn acquire(index: usize, deadline: Option<u64>) -> u8 {
    if index >= count() {
        return error_result(b"no-adapter", 1);
    }
    if link_down(index) {
        return error_result(b"link-down", 2);
    }
    if expired(deadline) {
        return deadline_result(b"acquire-timeout");
    }
    if !begin(b"acquire", Some(Operation::Acquire)) {
        return 3;
    }
    let result = acquire_operation(index, deadline);
    finish(result, true);
    result
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestState {
    Renew,
    Rebind,
}
fn renew_operation(index: usize, deadline: Option<u64>, state: RequestState) -> u8 {
    let Some(socket) = bind() else {
        return 6;
    };
    let previous = stats();
    let xid = xid(mac(index));
    metadata(|s| {
        s.stats.lease.xid = xid;
        s.stats.last_error = Text::literal(b"renew");
    });
    let mut payload = [0; MAX_PACKET];
    let mut p = params(xid, mac(index));
    p.requested_ip = previous.lease.offered_ip;
    p.server_ip = previous.lease.server_ip;
    p.state = if state == RequestState::Rebind { 2 } else { 1 };
    let Some(length) = build(Build::Request, p, &mut payload) else {
        return error_result(b"renew-build", 4);
    };
    expect(2, xid, mac(index));
    let nak_before = stats().nak_rx;
    let dest = if state == RequestState::Rebind {
        BROADCAST
    } else {
        previous.lease.server_ip
    };
    let result = ack_retry(
        Request {
            index,
            socket: socket.0,
            source: previous.lease.offered_ip,
            dest,
            payload: &payload[..length],
        },
        b"renew-timeout",
        deadline,
    );
    if result != 0 {
        metadata(|s| {
            if s.stats.nak_rx <= nak_before {
                s.stats.lease = previous.lease;
                s.stats.lease_acquired_tick = previous.lease_acquired_tick;
            }
        });
        return result;
    }
    if !stats().lease.bound {
        metadata(|s| {
            s.stats.lease = previous.lease;
            s.stats.lease_acquired_tick = previous.lease_acquired_tick;
        });
        return error_result(b"not-bound", 6);
    }
    apply_lease();
    error_result(b"renewed", 0)
}
fn renew(index: usize, requested: Option<u64>) -> u8 {
    if index >= count() {
        return error_result(b"no-adapter", 1);
    }
    let s = stats();
    if !s.lease.bound || s.lease.offered_ip == ZERO_IP {
        return error_result(b"no-lease", 6);
    }
    if expired(requested) {
        return deadline_result(b"renew-timeout");
    }
    let elapsed = lease_timing().elapsed_seconds;
    let s = stats();
    if s.lease.lease_seconds != dhcp::INFINITE_SECONDS && elapsed >= s.lease.lease_seconds {
        return error_result(b"lease-expired", 6);
    }
    let state = if s.lease.server_ip == ZERO_IP
        || (s.lease.lease_seconds != dhcp::INFINITE_SECONDS
            && s.lease.rebind_seconds != 0
            && elapsed >= s.lease.rebind_seconds)
    {
        RequestState::Rebind
    } else {
        RequestState::Renew
    };
    let deadline = if s.lease.lease_seconds == dhcp::INFINITE_SECONDS {
        requested
    } else {
        let seconds = if state == RequestState::Renew && s.lease.rebind_seconds != 0 {
            s.lease.rebind_seconds
        } else {
            s.lease.lease_seconds
        };
        let boundary = s
            .lease_acquired_tick
            .saturating_add(u64::from(seconds).saturating_mul(timing::tick_hz()));
        Some(requested.map_or(boundary, |r| r.min(boundary)))
    };
    let (label, operation): (&'static [u8], _) = if state == RequestState::Rebind {
        (b"rebind", Operation::Rebind)
    } else {
        (b"renew", Operation::Renew)
    };
    if !begin(label, Some(operation)) {
        return 3;
    }
    let result = renew_operation(index, deadline, state);
    finish(result, true);
    result
}
fn release_operation(index: usize) -> u8 {
    let Some(socket) = bind() else {
        return 6;
    };
    let lease = stats().lease;
    let xid = xid(mac(index));
    let mut payload = [0; MAX_PACKET];
    let mut p = params(xid, mac(index));
    p.client_ip = lease.offered_ip;
    p.server_ip = lease.server_ip;
    let Some(length) = build(Build::Release, p, &mut payload) else {
        return error_result(b"release-build", 4);
    };
    let dest = if lease.server_ip == ZERO_IP {
        BROADCAST
    } else {
        lease.server_ip
    };
    let result = send(index, socket.0, lease.offered_ip, dest, &payload[..length]);
    clear_bound();
    config::clear_dhcp_lease();
    unsafe {
        (peers().arp_flush)();
    }
    if result != 0 {
        metadata(|s| s.stats.release_errors += 1);
        return error_result(transmit::result_name(result), result);
    }
    error_result(b"released", 0)
}
fn released() {
    let before = cancel_coordinator(false);
    log_runtime(Some(before), Text::literal(b"released"));
}
fn release(index: usize) -> u8 {
    if index >= count() {
        return error_result(b"no-adapter", 1);
    }
    let lease = stats().lease;
    if !lease.bound || lease.offered_ip == ZERO_IP {
        error(b"no-lease");
        metadata(|s| s.stats.lease_acquired_tick = 0);
        config::clear_dhcp_lease();
        released();
        return 0;
    }
    if !begin(b"release", None) {
        return 3;
    }
    let result = release_operation(index);
    finish(result, false);
    released();
    result
}
fn drive() {
    let now = timing::now_ticks();
    let present = count() != 0;
    if present {
        unsafe {
            (peers().refresh)(0);
        }
    }
    let link_up = present && adapters::snapshot(0).is_some_and(|a| a.link == 2);
    let elapsed = lease_timing().elapsed_seconds;
    let cfg = config::settings();
    let (before, action, reason) = metadata(|s| {
        let lease = s.stats.lease;
        let bound = lease.bound && cfg.dhcp_bound;
        let finite = bound && lease.lease_seconds != dhcp::INFINITE_SECONDS;
        let before = s.coordinator.state;
        let action = s.coordinator.observe(dhcp_runtime::Input {
            now,
            desired_dhcp: cfg.dhcp_enabled,
            adapter_present: present,
            link_up,
            lease_bound: bound,
            renew_due: finite && lease.renew_seconds != 0 && elapsed >= lease.renew_seconds,
            rebind_due: finite && lease.rebind_seconds != 0 && elapsed >= lease.rebind_seconds,
            lease_expired: finite && lease.lease_seconds != 0 && elapsed >= lease.lease_seconds,
        });
        (before, action, s.stats.last_error)
    });
    log_runtime(Some(before), reason);
    match action {
        Action::None => {}
        Action::ClearLease => {
            clear_bound();
            unsafe {
                config::clear_preserving_mode(Text::literal(b"lease-lost"));
                (peers().arp_flush)();
            }
            log_runtime(None, Text::literal(b"lease-lost"));
        }
        Action::Acquire => {
            acquire(0, None);
        }
        Action::Renew | Action::Rebind => {
            renew(0, None);
        }
    }
}
unsafe extern "C" fn task_main() {
    loop {
        drive();
        unsafe {
            scheduler::sleep(timing::ms_to_ticks(100), b"dhcp-link-idle".as_ptr(), 14);
        }
    }
}
fn start() -> bool {
    if metadata(|s| s.task_started) {
        return true;
    }
    let mut failure = task_types::CreateFailure::None;
    let worker = unsafe {
        task_resources::create(
            task_resources::CreateArgs {
                name: task_types::Bytes {
                    pointer: b"dhcp-link".as_ptr(),
                    len: 9,
                },
                state: task_types::State::Ready,
                needs_fpu: true,
                entry: Some(task_main),
                role: task_types::Role::Batch,
                smp_eligible: false,
                smp_r4x: false,
                owner_kind: task_types::ExecutionOwnerKind::None,
                owner_context: ptr::null_mut(),
            },
            &mut failure,
        )
    };
    if worker.is_null() {
        let next = timing::now_ticks().saturating_add(timing::ms_to_ticks(1000));
        metadata(|s| {
            s.task_retry_requested = true;
            s.task_next_retry_tick = next;
        });
        foundation::puts(b"DHCP05913 task=create-failed\r\n");
        return false;
    }
    let (id, generation) = unsafe { task::immutable_identity(worker.cast()) };
    metadata(|s| {
        s.task_started = true;
        s.task_retry_requested = false;
        s.task_next_retry_tick = 0;
        s.task_id = id;
        s.task_generation = generation;
    });
    foundation::puts(b"DHCP05913 task=started id=");
    foundation::dec(u64::from(id));
    foundation::puts(b" generation=");
    foundation::dec(generation);
    foundation::puts(b"\r\n");
    true
}
/// # Safety
/// Quiescent singleton network initialization; immutable peers live forever.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_client_reset(input: *const Peers) {
    metadata(|s| {
        *s = Metadata {
            peers: input,
            ..Metadata::EMPTY
        };
    });
    unsafe {
        OPERATION.0.get().write(EMPTY_MUTEX);
    }
    clear_expectation();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_start() -> bool {
    start()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_retry_task() {
    let due = metadata(|s| {
        s.task_retry_requested && !s.task_started && timing::now_ticks() >= s.task_next_retry_tick
    });
    if due {
        start();
    }
}
/// # Safety
/// Resident immutable kernel label retains its original lifetime through logs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_client_cancel(reason: Text) -> bool {
    cancel(reason)
}
/// # Safety
/// Diagnostic-only pending injection; label is resident kernel text. No lease
/// or other statistics are overwritten from a stale diagnostic snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_client_set_pending(pending: bool, label: *const Text) {
    let label = if label.is_null() {
        None
    } else {
        Some(unsafe { *label })
    };
    metadata(|s| {
        s.stats.operation_pending = pending;
        if let Some(label) = label {
            s.stats.pending_label = label;
        }
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_invalidate_lease() {
    clear_bound();
}
/// # Safety
/// Exclusive exact paired output, written after metadata admission is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_client_stats(out: *mut dhcp::Stats) {
    unsafe {
        out.write(stats());
    }
}
/// # Safety
/// Exclusive exact paired output, written after metadata admission is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_client_runtime(out: *mut DhcpRuntimeStatus) {
    unsafe {
        out.write(runtime_status());
    }
}
/// # Safety
/// Exclusive exact paired output, written after metadata admission is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_client_lease_timing(out: *mut DhcpLeaseTiming) {
    unsafe {
        out.write(lease_timing());
    }
}
/// # Safety
/// Readable retained packet throughout protocol dispatch, no CPU owner spans it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_client_message(data: *const u8, len: usize) -> bool {
    unsafe { handle_message(data, len) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_acquire(index: usize, bounded: bool, tick: u64) -> u8 {
    acquire(index, if bounded { Some(tick) } else { None })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_renew(index: usize, bounded: bool, tick: u64) -> u8 {
    renew(index, if bounded { Some(tick) } else { None })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_release(index: usize) -> u8 {
    release(index)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_acquire_default(bounded: bool, tick: u64) -> u8 {
    config::enable_dhcp();
    if count() == 0 {
        return error_result(b"no-adapter", 1);
    }
    acquire(0, if bounded { Some(tick) } else { None })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_renew_default(bounded: bool, tick: u64) -> u8 {
    if count() == 0 {
        return error_result(b"no-adapter", 1);
    }
    renew(0, if bounded { Some(tick) } else { None })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_client_release_default() -> u8 {
    if count() == 0 {
        error(b"no-adapter");
        clear_bound();
        config::clear_dhcp_lease();
        released();
        return 1;
    }
    release(0)
}

const _: () = {
    assert!(core::mem::size_of::<DhcpLeaseTiming>() == 32);
    assert!(core::mem::align_of::<DhcpLeaseTiming>() == 8);
    assert!(core::mem::offset_of!(DhcpLeaseTiming, bound) == 0);
    assert!(core::mem::offset_of!(DhcpLeaseTiming, acquired_tick) == 8);
    assert!(core::mem::offset_of!(DhcpLeaseTiming, elapsed_seconds) == 16);
    assert!(core::mem::offset_of!(DhcpLeaseTiming, remaining_seconds) == 20);
    assert!(core::mem::offset_of!(DhcpLeaseTiming, renew_in_seconds) == 24);
    assert!(core::mem::offset_of!(DhcpLeaseTiming, rebind_in_seconds) == 28);
};
const _: () = {
    assert!(core::mem::size_of::<DhcpRuntimeStatus>() == 88);
    assert!(core::mem::align_of::<DhcpRuntimeStatus>() == 8);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, state) == 0);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, desired_dhcp) == 2);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, link_up) == 3);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, task_started) == 4);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, task_id) == 8);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, task_generation) == 16);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, link_generation) == 24);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, operation_generation) == 28);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, transition_tick) == 32);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, next_retry_tick) == 40);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, last_timeout_tick) == 48);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, retry_round) == 56);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, operation_active) == 57);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, recoveries) == 64);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, starts) == 72);
    assert!(core::mem::offset_of!(DhcpRuntimeStatus, cancels) == 80);
};

const _: () = {
    assert!(core::mem::size_of::<Peers>() == 64);
    assert!(core::mem::align_of::<Peers>() == 8);
    assert!(core::mem::offset_of!(Peers, refresh) == 0);
    assert!(core::mem::offset_of!(Peers, poll) == 8);
    assert!(core::mem::offset_of!(Peers, idle) == 16);
    assert!(core::mem::offset_of!(Peers, bind) == 24);
    assert!(core::mem::offset_of!(Peers, close) == 32);
    assert!(core::mem::offset_of!(Peers, recv) == 40);
    assert!(core::mem::offset_of!(Peers, send) == 48);
    assert!(core::mem::offset_of!(Peers, arp_flush) == 56);
};
