// Private fixed C diagnostic copies; no Task layout crosses this boundary.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub initialized: bool,
    pub current_index: u32,
    pub yields: u64,
    pub sleeps: u64,
    pub wakes: u64,
    pub idle_waits: u64,
    pub object_waits: u64,
    pub object_wakes: u64,
    pub object_timeouts: u64,
    pub object_cancels: u64,
    pub ticks: u64,
    pub preemption_supported: u32,
    pub preemption_enabled: u32,
    pub preemption_test_mode: u32,
    pub preempt_disable_depth: u32,
    pub preempt_disable_max_depth: u32,
    pub preempt_disable_calls: u64,
    pub preempt_enable_calls: u64,
    pub preempt_disable_underflows: u64,
    pub preemption_simulation_ticks: u64,
    pub preemption_eligible_ticks: u64,
    pub preemption_deferred_disabled: u64,
    pub preemption_deferred_critical: u64,
    pub preemption_deferred_no_task: u64,
    pub preemption_deferred_no_ready: u64,
    pub preemption_deferred_quantum: u64,
    pub preemption_deferred_kernel_ip: u64,
    pub preemption_switch_ticks: u64,
    /// Base/maximum slice. Effective interactive dispatch ranks use8ms.
    pub preemption_quantum_ticks: u32,
    pub preemption_quantum_expired: u64,
    pub preemption_app_code_ticks: u64,
    pub long_running_task_warnings: u64,
    pub starvation_warnings: u64,
    pub ready_latency_samples: u64,
    pub ready_latency_total_ticks: u64,
    pub ready_latency_max_ticks: u64,
    pub ready_latency_last_ticks: u64,
    pub ready_waiting_max_ticks: u64,
    pub wait_object_total_ticks: u64,
    pub wait_object_max_ticks: u64,
    pub wait_object_last_ticks: u64,
    pub run_without_switch_max_ticks: u64,
    pub quantum_overrun_count: u64,
    pub quantum_overrun_max_ticks: u64,
    pub preemption_deferred_max_ticks: u64,
    pub long_running_warn_threshold_ticks: u64,
    pub starvation_warn_threshold_ticks: u64,
    pub priority_selects: u64,
    pub priority_picks_high: u64,
    pub priority_picks_normal: u64,
    pub priority_picks_low: u64,
    pub priority_rr_picks: u64,
    pub role_picks: [u64; 4],
    pub ready_latency_role_samples: [u64; 4],
    pub ready_latency_role_buckets: [[u64; 5]; 4],
    pub safe_reschedule_points: u64,
    pub safe_reschedule_switches: u64,
    pub safe_reschedule_deferred_irq: u64,
    pub safe_reschedule_deferred_owner: u64,
}
impl Stats {
    pub const EMPTY: Self = Self {
        initialized: false,
        current_index: 0,
        yields: 0,
        sleeps: 0,
        wakes: 0,
        idle_waits: 0,
        object_waits: 0,
        object_wakes: 0,
        object_timeouts: 0,
        object_cancels: 0,
        ticks: 0,
        preemption_supported: 0,
        preemption_enabled: 0,
        preemption_test_mode: 0,
        preempt_disable_depth: 0,
        preempt_disable_max_depth: 0,
        preempt_disable_calls: 0,
        preempt_enable_calls: 0,
        preempt_disable_underflows: 0,
        preemption_simulation_ticks: 0,
        preemption_eligible_ticks: 0,
        preemption_deferred_disabled: 0,
        preemption_deferred_critical: 0,
        preemption_deferred_no_task: 0,
        preemption_deferred_no_ready: 0,
        preemption_deferred_quantum: 0,
        preemption_deferred_kernel_ip: 0,
        preemption_switch_ticks: 0,
        preemption_quantum_ticks: 0,
        preemption_quantum_expired: 0,
        preemption_app_code_ticks: 0,
        long_running_task_warnings: 0,
        starvation_warnings: 0,
        ready_latency_samples: 0,
        ready_latency_total_ticks: 0,
        ready_latency_max_ticks: 0,
        ready_latency_last_ticks: 0,
        ready_waiting_max_ticks: 0,
        wait_object_total_ticks: 0,
        wait_object_max_ticks: 0,
        wait_object_last_ticks: 0,
        run_without_switch_max_ticks: 0,
        quantum_overrun_count: 0,
        quantum_overrun_max_ticks: 0,
        preemption_deferred_max_ticks: 0,
        long_running_warn_threshold_ticks: 0,
        starvation_warn_threshold_ticks: 0,
        priority_selects: 0,
        priority_picks_high: 0,
        priority_picks_normal: 0,
        priority_picks_low: 0,
        priority_rr_picks: 0,
        role_picks: [0; 4],
        ready_latency_role_samples: [0; 4],
        ready_latency_role_buckets: [[0; 5]; 4],
        safe_reschedule_points: 0,
        safe_reschedule_switches: 0,
        safe_reschedule_deferred_irq: 0,
        safe_reschedule_deferred_owner: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CpuPreemptionStats {
    pub timer_switches: u64,
    pub reschedule_ipi_switches: u64,
}
impl CpuPreemptionStats {
    pub const EMPTY: Self = Self {
        timer_switches: 0,
        reschedule_ipi_switches: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StructureStats {
    pub ready_candidate_visits: u64,
    pub timeout_candidate_visits: u64,
    pub warning_candidate_visits: u64,
    pub timeout_storm_deferrals: u64,
    pub wakeup_reschedule_requests: u64,
    pub wakeup_preemption_switches: u64,
    pub safe_reschedule_points: u64,
    pub safe_reschedule_switches: u64,
    pub safe_reschedule_deferred_irq: u64,
    pub safe_reschedule_deferred_owner: u64,
    pub reschedule_pending: bool,
}
impl StructureStats {
    pub const EMPTY: Self = Self {
        ready_candidate_visits: 0,
        timeout_candidate_visits: 0,
        warning_candidate_visits: 0,
        timeout_storm_deferrals: 0,
        wakeup_reschedule_requests: 0,
        wakeup_preemption_switches: 0,
        safe_reschedule_points: 0,
        safe_reschedule_switches: 0,
        safe_reschedule_deferred_irq: 0,
        safe_reschedule_deferred_owner: 0,
        reschedule_pending: false,
    };
}

const _: () = {
    assert!(core::mem::size_of::<Stats>() == 648);
    assert!(core::mem::offset_of!(Stats, initialized) == 0);
    assert!(core::mem::offset_of!(Stats, current_index) == 4);
    assert!(core::mem::offset_of!(Stats, yields) == 8);
    assert!(core::mem::offset_of!(Stats, sleeps) == 16);
    assert!(core::mem::offset_of!(Stats, wakes) == 24);
    assert!(core::mem::offset_of!(Stats, idle_waits) == 32);
    assert!(core::mem::offset_of!(Stats, object_waits) == 40);
    assert!(core::mem::offset_of!(Stats, object_wakes) == 48);
    assert!(core::mem::offset_of!(Stats, object_timeouts) == 56);
    assert!(core::mem::offset_of!(Stats, object_cancels) == 64);
    assert!(core::mem::offset_of!(Stats, ticks) == 72);
    assert!(core::mem::offset_of!(Stats, preemption_supported) == 80);
    assert!(core::mem::offset_of!(Stats, preemption_enabled) == 84);
    assert!(core::mem::offset_of!(Stats, preemption_test_mode) == 88);
    assert!(core::mem::offset_of!(Stats, preempt_disable_depth) == 92);
    assert!(core::mem::offset_of!(Stats, preempt_disable_max_depth) == 96);
    assert!(core::mem::offset_of!(Stats, preempt_disable_calls) == 104);
    assert!(core::mem::offset_of!(Stats, preempt_enable_calls) == 112);
    assert!(core::mem::offset_of!(Stats, preempt_disable_underflows) == 120);
    assert!(core::mem::offset_of!(Stats, preemption_simulation_ticks) == 128);
    assert!(core::mem::offset_of!(Stats, preemption_eligible_ticks) == 136);
    assert!(core::mem::offset_of!(Stats, preemption_deferred_disabled) == 144);
    assert!(core::mem::offset_of!(Stats, preemption_deferred_critical) == 152);
    assert!(core::mem::offset_of!(Stats, preemption_deferred_no_task) == 160);
    assert!(core::mem::offset_of!(Stats, preemption_deferred_no_ready) == 168);
    assert!(core::mem::offset_of!(Stats, preemption_deferred_quantum) == 176);
    assert!(core::mem::offset_of!(Stats, preemption_deferred_kernel_ip) == 184);
    assert!(core::mem::offset_of!(Stats, preemption_switch_ticks) == 192);
    assert!(core::mem::offset_of!(Stats, preemption_quantum_ticks) == 200);
    assert!(core::mem::offset_of!(Stats, preemption_quantum_expired) == 208);
    assert!(core::mem::offset_of!(Stats, preemption_app_code_ticks) == 216);
    assert!(core::mem::offset_of!(Stats, long_running_task_warnings) == 224);
    assert!(core::mem::offset_of!(Stats, starvation_warnings) == 232);
    assert!(core::mem::offset_of!(Stats, ready_latency_samples) == 240);
    assert!(core::mem::offset_of!(Stats, ready_latency_total_ticks) == 248);
    assert!(core::mem::offset_of!(Stats, ready_latency_max_ticks) == 256);
    assert!(core::mem::offset_of!(Stats, ready_latency_last_ticks) == 264);
    assert!(core::mem::offset_of!(Stats, ready_waiting_max_ticks) == 272);
    assert!(core::mem::offset_of!(Stats, wait_object_total_ticks) == 280);
    assert!(core::mem::offset_of!(Stats, wait_object_max_ticks) == 288);
    assert!(core::mem::offset_of!(Stats, wait_object_last_ticks) == 296);
    assert!(core::mem::offset_of!(Stats, run_without_switch_max_ticks) == 304);
    assert!(core::mem::offset_of!(Stats, quantum_overrun_count) == 312);
    assert!(core::mem::offset_of!(Stats, quantum_overrun_max_ticks) == 320);
    assert!(core::mem::offset_of!(Stats, preemption_deferred_max_ticks) == 328);
    assert!(core::mem::offset_of!(Stats, long_running_warn_threshold_ticks) == 336);
    assert!(core::mem::offset_of!(Stats, starvation_warn_threshold_ticks) == 344);
    assert!(core::mem::offset_of!(Stats, priority_selects) == 352);
    assert!(core::mem::offset_of!(Stats, priority_picks_high) == 360);
    assert!(core::mem::offset_of!(Stats, priority_picks_normal) == 368);
    assert!(core::mem::offset_of!(Stats, priority_picks_low) == 376);
    assert!(core::mem::offset_of!(Stats, priority_rr_picks) == 384);
    assert!(core::mem::offset_of!(Stats, role_picks) == 392);
    assert!(core::mem::offset_of!(Stats, ready_latency_role_samples) == 424);
    assert!(core::mem::offset_of!(Stats, ready_latency_role_buckets) == 456);
    assert!(core::mem::offset_of!(Stats, safe_reschedule_points) == 616);
    assert!(core::mem::offset_of!(Stats, safe_reschedule_switches) == 624);
    assert!(core::mem::offset_of!(Stats, safe_reschedule_deferred_irq) == 632);
    assert!(core::mem::offset_of!(Stats, safe_reschedule_deferred_owner) == 640);
    assert!(core::mem::size_of::<CpuPreemptionStats>() == 16);
    assert!(core::mem::offset_of!(CpuPreemptionStats, timer_switches) == 0);
    assert!(core::mem::offset_of!(CpuPreemptionStats, reschedule_ipi_switches) == 8);
    assert!(core::mem::size_of::<StructureStats>() == 88);
    assert!(core::mem::offset_of!(StructureStats, ready_candidate_visits) == 0);
    assert!(core::mem::offset_of!(StructureStats, timeout_candidate_visits) == 8);
    assert!(core::mem::offset_of!(StructureStats, warning_candidate_visits) == 16);
    assert!(core::mem::offset_of!(StructureStats, timeout_storm_deferrals) == 24);
    assert!(core::mem::offset_of!(StructureStats, wakeup_reschedule_requests) == 32);
    assert!(core::mem::offset_of!(StructureStats, wakeup_preemption_switches) == 40);
    assert!(core::mem::offset_of!(StructureStats, safe_reschedule_points) == 48);
    assert!(core::mem::offset_of!(StructureStats, safe_reschedule_switches) == 56);
    assert!(core::mem::offset_of!(StructureStats, safe_reschedule_deferred_irq) == 64);
    assert!(core::mem::offset_of!(StructureStats, safe_reschedule_deferred_owner) == 72);
    assert!(core::mem::offset_of!(StructureStats, reschedule_pending) == 80);
};
