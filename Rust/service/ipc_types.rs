//! Original channel limits and fixed private snapshot records.

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Lifetime {
    pub send_calls: u64,
    pub recv_calls: u64,
    pub request_calls: u64,
    pub handler_queued: u64,
    pub handler_started: u64,
    pub handler_completed: u64,
    pub handler_failures: u64,
    pub handler_direct: u64,
    pub handler_waits: u64,
    pub handler_wait_timeouts: u64,
    pub handler_queue_ns: u64,
    pub handler_queue_max_ns: u64,
    pub handler_run_ns: u64,
    pub handler_run_max_ns: u64,
    pub handler_e2e_ns: u64,
    pub handler_e2e_max_ns: u64,
    pub timing_unavailable: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub payload_copy_bytes: u64,
    pub payload_clear_bytes: u64,
    pub queue_full: u64,
    pub queue_empty: u64,
    pub admission_waits: u64,
    pub admission_timeouts: u64,
    pub recv_buffer_small: u64,
    pub response_searches: u64,
    pub response_search_slots: u64,
    pub stale_drops: u64,
    pub lock_contentions: u64,
}
impl Lifetime {
    pub const EMPTY: Self = Self {
        send_calls: 0,
        recv_calls: 0,
        request_calls: 0,
        handler_queued: 0,
        handler_started: 0,
        handler_completed: 0,
        handler_failures: 0,
        handler_direct: 0,
        handler_waits: 0,
        handler_wait_timeouts: 0,
        handler_queue_ns: 0,
        handler_queue_max_ns: 0,
        handler_run_ns: 0,
        handler_run_max_ns: 0,
        handler_e2e_ns: 0,
        handler_e2e_max_ns: 0,
        timing_unavailable: 0,
        request_bytes: 0,
        response_bytes: 0,
        payload_copy_bytes: 0,
        payload_clear_bytes: 0,
        queue_full: 0,
        queue_empty: 0,
        admission_waits: 0,
        admission_timeouts: 0,
        recv_buffer_small: 0,
        response_searches: 0,
        response_search_slots: 0,
        stale_drops: 0,
        lock_contentions: 0,
    };
}
impl PerformanceSummary {
    pub(super) fn add_lifetime(&mut self, value: Lifetime) {
        self.send_calls = self.send_calls.wrapping_add(value.send_calls);
        self.recv_calls = self.recv_calls.wrapping_add(value.recv_calls);
        self.request_calls = self.request_calls.wrapping_add(value.request_calls);
        self.handler_queued = self.handler_queued.wrapping_add(value.handler_queued);
        self.handler_started = self.handler_started.wrapping_add(value.handler_started);
        self.handler_completed = self.handler_completed.wrapping_add(value.handler_completed);
        self.handler_failures = self.handler_failures.wrapping_add(value.handler_failures);
        self.handler_direct = self.handler_direct.wrapping_add(value.handler_direct);
        self.handler_waits = self.handler_waits.wrapping_add(value.handler_waits);
        self.handler_wait_timeouts = self
            .handler_wait_timeouts
            .wrapping_add(value.handler_wait_timeouts);
        self.handler_queue_ns = self.handler_queue_ns.wrapping_add(value.handler_queue_ns);
        self.handler_queue_max_ns = self
            .handler_queue_max_ns
            .wrapping_add(value.handler_queue_max_ns);
        self.handler_run_ns = self.handler_run_ns.wrapping_add(value.handler_run_ns);
        self.handler_run_max_ns = self
            .handler_run_max_ns
            .wrapping_add(value.handler_run_max_ns);
        self.handler_e2e_ns = self.handler_e2e_ns.wrapping_add(value.handler_e2e_ns);
        self.handler_e2e_max_ns = self
            .handler_e2e_max_ns
            .wrapping_add(value.handler_e2e_max_ns);
        self.timing_unavailable = self
            .timing_unavailable
            .wrapping_add(value.timing_unavailable);
        self.request_bytes = self.request_bytes.wrapping_add(value.request_bytes);
        self.response_bytes = self.response_bytes.wrapping_add(value.response_bytes);
        self.payload_copy_bytes = self
            .payload_copy_bytes
            .wrapping_add(value.payload_copy_bytes);
        self.payload_clear_bytes = self
            .payload_clear_bytes
            .wrapping_add(value.payload_clear_bytes);
        self.queue_full = self.queue_full.wrapping_add(value.queue_full);
        self.queue_empty = self.queue_empty.wrapping_add(value.queue_empty);
        self.admission_waits = self.admission_waits.wrapping_add(value.admission_waits);
        self.admission_timeouts = self
            .admission_timeouts
            .wrapping_add(value.admission_timeouts);
        self.recv_buffer_small = self.recv_buffer_small.wrapping_add(value.recv_buffer_small);
        self.response_searches = self.response_searches.wrapping_add(value.response_searches);
        self.response_search_slots = self
            .response_search_slots
            .wrapping_add(value.response_search_slots);
        self.stale_drops = self.stale_drops.wrapping_add(value.stale_drops);
        self.lock_contentions = self.lock_contentions.wrapping_add(value.lock_contentions);
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Summary {
    pub max_channels: u32,
    pub active_channels: u32,
    pub max_message_size: u32,
    pub queue_depth: u32,
    pub sends: u64,
    pub receives: u64,
    pub drops: u64,
    pub errors: u64,
    pub echo_tests: u64,
}
impl Summary {
    pub const EMPTY: Self = Self {
        max_channels: 0,
        active_channels: 0,
        max_message_size: 0,
        queue_depth: 0,
        sends: 0,
        receives: 0,
        drops: 0,
        errors: 0,
        echo_tests: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ChannelInfo {
    pub id: u32,
    pub active: u32,
    pub queued: u32,
    pub queue_depth: u32,
    pub max_message_size: u32,
    pub has_handler: u32,
    pub reserved0: u32,
    pub reserved1: u32,
    pub opens: u64,
    pub closes: u64,
    pub sends: u64,
    pub receives: u64,
    pub drops: u64,
    pub name: [u8; 16],
}
impl ChannelInfo {
    pub const EMPTY: Self = Self {
        id: 0,
        active: 0,
        queued: 0,
        queue_depth: 0,
        max_message_size: 0,
        has_handler: 0,
        reserved0: 0,
        reserved1: 0,
        opens: 0,
        closes: 0,
        sends: 0,
        receives: 0,
        drops: 0,
        name: [0; 16],
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PerformanceSummary {
    pub worker_started: u32,
    pub worker_task_id: u32,
    pub active_channels: u32,
    pub queue_used: u32,
    pub queue_ready: u32,
    pub queue_running: u32,
    pub queue_limit: u32,
    pub reserved0: u32,
    pub send_calls: u64,
    pub recv_calls: u64,
    pub request_calls: u64,
    pub handler_queued: u64,
    pub handler_started: u64,
    pub handler_completed: u64,
    pub handler_failures: u64,
    pub handler_direct: u64,
    pub handler_waits: u64,
    pub handler_wait_timeouts: u64,
    pub handler_queue_ns: u64,
    pub handler_queue_max_ns: u64,
    pub handler_run_ns: u64,
    pub handler_run_max_ns: u64,
    pub handler_e2e_ns: u64,
    pub handler_e2e_max_ns: u64,
    pub timing_unavailable: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub payload_copy_bytes: u64,
    pub payload_clear_bytes: u64,
    pub queue_full: u64,
    pub queue_empty: u64,
    pub admission_waits: u64,
    pub admission_timeouts: u64,
    pub recv_buffer_small: u64,
    pub response_searches: u64,
    pub response_search_slots: u64,
    pub stale_drops: u64,
    pub lock_contentions: u64,
    pub irq_denied: u64,
    pub worker_idle_waits: u64,
    pub worker_wakes: u64,
}
impl PerformanceSummary {
    pub const EMPTY: Self = Self {
        worker_started: 0,
        worker_task_id: 0,
        active_channels: 0,
        queue_used: 0,
        queue_ready: 0,
        queue_running: 0,
        queue_limit: 0,
        reserved0: 0,
        send_calls: 0,
        recv_calls: 0,
        request_calls: 0,
        handler_queued: 0,
        handler_started: 0,
        handler_completed: 0,
        handler_failures: 0,
        handler_direct: 0,
        handler_waits: 0,
        handler_wait_timeouts: 0,
        handler_queue_ns: 0,
        handler_queue_max_ns: 0,
        handler_run_ns: 0,
        handler_run_max_ns: 0,
        handler_e2e_ns: 0,
        handler_e2e_max_ns: 0,
        timing_unavailable: 0,
        request_bytes: 0,
        response_bytes: 0,
        payload_copy_bytes: 0,
        payload_clear_bytes: 0,
        queue_full: 0,
        queue_empty: 0,
        admission_waits: 0,
        admission_timeouts: 0,
        recv_buffer_small: 0,
        response_searches: 0,
        response_search_slots: 0,
        stale_drops: 0,
        lock_contentions: 0,
        irq_denied: 0,
        worker_idle_waits: 0,
        worker_wakes: 0,
    };
}

const _: () = {
    assert!(core::mem::offset_of!(Summary, max_channels) == 0);
    assert!(core::mem::offset_of!(Summary, active_channels) == 4);
    assert!(core::mem::offset_of!(Summary, max_message_size) == 8);
    assert!(core::mem::offset_of!(Summary, queue_depth) == 12);
    assert!(core::mem::offset_of!(Summary, sends) == 16);
    assert!(core::mem::offset_of!(Summary, receives) == 24);
    assert!(core::mem::offset_of!(Summary, drops) == 32);
    assert!(core::mem::offset_of!(Summary, errors) == 40);
    assert!(core::mem::offset_of!(Summary, echo_tests) == 48);
    assert!(core::mem::size_of::<Summary>() == 56);
    assert!(core::mem::offset_of!(ChannelInfo, id) == 0);
    assert!(core::mem::offset_of!(ChannelInfo, active) == 4);
    assert!(core::mem::offset_of!(ChannelInfo, queued) == 8);
    assert!(core::mem::offset_of!(ChannelInfo, queue_depth) == 12);
    assert!(core::mem::offset_of!(ChannelInfo, max_message_size) == 16);
    assert!(core::mem::offset_of!(ChannelInfo, has_handler) == 20);
    assert!(core::mem::offset_of!(ChannelInfo, reserved0) == 24);
    assert!(core::mem::offset_of!(ChannelInfo, reserved1) == 28);
    assert!(core::mem::offset_of!(ChannelInfo, opens) == 32);
    assert!(core::mem::offset_of!(ChannelInfo, closes) == 40);
    assert!(core::mem::offset_of!(ChannelInfo, sends) == 48);
    assert!(core::mem::offset_of!(ChannelInfo, receives) == 56);
    assert!(core::mem::offset_of!(ChannelInfo, drops) == 64);
    assert!(core::mem::offset_of!(ChannelInfo, name) == 72);
    assert!(core::mem::size_of::<ChannelInfo>() == 88);
    assert!(core::mem::offset_of!(PerformanceSummary, worker_started) == 0);
    assert!(core::mem::offset_of!(PerformanceSummary, worker_task_id) == 4);
    assert!(core::mem::offset_of!(PerformanceSummary, active_channels) == 8);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_used) == 12);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_ready) == 16);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_running) == 20);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_limit) == 24);
    assert!(core::mem::offset_of!(PerformanceSummary, reserved0) == 28);
    assert!(core::mem::offset_of!(PerformanceSummary, send_calls) == 32);
    assert!(core::mem::offset_of!(PerformanceSummary, recv_calls) == 40);
    assert!(core::mem::offset_of!(PerformanceSummary, request_calls) == 48);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_queued) == 56);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_started) == 64);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_completed) == 72);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_failures) == 80);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_direct) == 88);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_waits) == 96);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_wait_timeouts) == 104);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_queue_ns) == 112);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_queue_max_ns) == 120);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_run_ns) == 128);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_run_max_ns) == 136);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_e2e_ns) == 144);
    assert!(core::mem::offset_of!(PerformanceSummary, handler_e2e_max_ns) == 152);
    assert!(core::mem::offset_of!(PerformanceSummary, timing_unavailable) == 160);
    assert!(core::mem::offset_of!(PerformanceSummary, request_bytes) == 168);
    assert!(core::mem::offset_of!(PerformanceSummary, response_bytes) == 176);
    assert!(core::mem::offset_of!(PerformanceSummary, payload_copy_bytes) == 184);
    assert!(core::mem::offset_of!(PerformanceSummary, payload_clear_bytes) == 192);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_full) == 200);
    assert!(core::mem::offset_of!(PerformanceSummary, queue_empty) == 208);
    assert!(core::mem::offset_of!(PerformanceSummary, admission_waits) == 216);
    assert!(core::mem::offset_of!(PerformanceSummary, admission_timeouts) == 224);
    assert!(core::mem::offset_of!(PerformanceSummary, recv_buffer_small) == 232);
    assert!(core::mem::offset_of!(PerformanceSummary, response_searches) == 240);
    assert!(core::mem::offset_of!(PerformanceSummary, response_search_slots) == 248);
    assert!(core::mem::offset_of!(PerformanceSummary, stale_drops) == 256);
    assert!(core::mem::offset_of!(PerformanceSummary, lock_contentions) == 264);
    assert!(core::mem::offset_of!(PerformanceSummary, irq_denied) == 272);
    assert!(core::mem::offset_of!(PerformanceSummary, worker_idle_waits) == 280);
    assert!(core::mem::offset_of!(PerformanceSummary, worker_wakes) == 288);
    assert!(core::mem::size_of::<PerformanceSummary>() == 296);
};
