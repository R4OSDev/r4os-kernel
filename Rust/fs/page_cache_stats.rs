//! Original page-cache diagnostic fields/defaults. Scalar atomics permit
//! safe advisory counters at unlocked error/I/O edges; no table/entry/pin
//! is made accessible outside the original cache mutex by these snapshots.
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Summary {
    pub enabled: bool,
    pub sector_bytes: u32,
    pub capacity: u32,
    pub entries_used: u32,
    pub dirty_entries: u32,
    pub reads: u64,
    pub hits: u64,
    pub misses: u64,
    pub fills: u64,
    pub evictions: u64,
    pub invalidations: u64,
    pub write_through_requests: u64,
    pub write_through_updates: u64,
    pub flushes: u64,
    pub read_errors: u64,
    pub write_errors: u64,
    pub writeback_waits: u64,
    pub writeback_errors: u64,
    pub dirty_bytes: u64,
    pub dirty_high_water_entries: u32,
    pub writeback_queue_depth: u32,
    pub writeback_queue_high_water: u32,
    pub deferred_write_requests: u64,
    pub dirty_sector_updates: u64,
    pub writeback_drains: u64,
    pub writeback_sectors: u64,
    pub writeback_pressure_drains: u64,
    pub writeback_flush_drains: u64,
    pub writeback_total_ticks: u64,
    pub writeback_max_ticks: u64,
    pub writeback_last_ticks: u64,
    pub writeback_retries: u64,
    pub clean_reclaimable_entries: u32,
    pub dirty_non_reclaimable_entries: u32,
    pub clean_reclaimable_bytes: u64,
    pub dirty_non_reclaimable_bytes: u64,
    pub reclaim_scans: u64,
    pub reclaim_clean_entries: u64,
    pub reclaim_dirty_drains: u64,
    pub reclaim_failed_drains: u64,
    pub payload_frame_bytes: u32,
    pub payload_frames: u32,
    pub payload_bytes: u64,
    pub pmm_reclaimable_bytes: u64,
    pub pmm_dirty_bytes: u64,
    pub payload_allocations: u64,
    pub payload_allocation_failures: u64,
    pub payload_releases: u64,
    pub reclaim_returned_frames: u64,
    pub reclaim_returned_bytes: u64,
    pub lock_timeouts: u64,
    pub busy_waits: u64,
    pub bulk_write_requests: u64,
    pub bulk_write_sectors: u64,
    pub selective_flushes: u64,
    pub selective_writeback_sectors: u64,
    pub selective_foreign_dirty_sectors_skipped: u64,
    pub policy_version: u32,
    pub policy_device_capacity: u32,
    pub policy_dirty_high_pages: u32,
    pub policy_dirty_low_pages: u32,
    pub policy_max_dirty_age_ticks: u64,
    pub policy_background_page_budget: u32,
    pub policy_worker_started: u32,
    pub policy_worker_task_id: u32,
    pub policy_worker_wakeups: u64,
    pub policy_background_drains: u64,
    pub policy_background_sectors: u64,
    pub policy_background_pressure_drains: u64,
    pub policy_background_age_drains: u64,
    pub policy_background_errors: u64,
    pub policy_clean_device_probes: u64,
    pub policy_dirty_device_probes: u64,
    pub policy_full_scan_fallbacks: u64,
    pub policy_device_dirty_high_water: u32,
    pub read_ahead_requests: u64,
    pub read_ahead_issued: u64,
    pub read_ahead_hits: u64,
    pub read_ahead_cancellations: u64,
    pub read_ahead_budget_skips: u64,
    pub capacity_min_pages: u32,
    pub capacity_max_pages: u32,
    pub capacity_ram_limit_pages: u32,
    pub capacity_active_limit_pages: u32,
    pub capacity_pressure_level: u32,
    pub read_ahead_window_pages: u32,
    pub read_ahead_window_max_pages: u32,
    pub capacity_reserved0: u32,
    pub fill_run_requests: u64,
    pub fill_run_backend_requests: u64,
    pub fill_run_pages: u64,
    pub fill_run_sectors: u64,
    pub fill_run_bytes: u64,
    pub fill_run_failures: u64,
    pub fill_run_retries: u64,
    pub fill_run_max_pages: u64,
    pub fill_scatter_copy_bytes: u64,
    pub read_staging_copy_bytes: u64,
    pub read_caller_copy_bytes: u64,
    pub read_publish_lock_drops: u64,
    pub fill_lock_drops: u64,
    pub capacity_reductions: u64,
    pub capacity_trimmed_pages: u64,
    pub read_ahead_pages_scheduled: u64,
    pub read_ahead_pages_issued: u64,
    pub read_ahead_random_resets: u64,
    pub writeback_windows: u64,
    pub writeback_window_pages: u64,
    pub writeback_window_requests: u64,
    pub writeback_window_max_sectors: u64,
    pub writeback_window_pending_max: u64,
}
pub struct Counters {
    pub enabled: AtomicBool,
    pub sector_bytes: AtomicU32,
    pub capacity: AtomicU32,
    pub entries_used: AtomicU32,
    pub dirty_entries: AtomicU32,
    pub reads: AtomicU64,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub fills: AtomicU64,
    pub evictions: AtomicU64,
    pub invalidations: AtomicU64,
    pub write_through_requests: AtomicU64,
    pub write_through_updates: AtomicU64,
    pub flushes: AtomicU64,
    pub read_errors: AtomicU64,
    pub write_errors: AtomicU64,
    pub writeback_waits: AtomicU64,
    pub writeback_errors: AtomicU64,
    pub dirty_bytes: AtomicU64,
    pub dirty_high_water_entries: AtomicU32,
    pub writeback_queue_depth: AtomicU32,
    pub writeback_queue_high_water: AtomicU32,
    pub deferred_write_requests: AtomicU64,
    pub dirty_sector_updates: AtomicU64,
    pub writeback_drains: AtomicU64,
    pub writeback_sectors: AtomicU64,
    pub writeback_pressure_drains: AtomicU64,
    pub writeback_flush_drains: AtomicU64,
    pub writeback_total_ticks: AtomicU64,
    pub writeback_max_ticks: AtomicU64,
    pub writeback_last_ticks: AtomicU64,
    pub writeback_retries: AtomicU64,
    pub clean_reclaimable_entries: AtomicU32,
    pub dirty_non_reclaimable_entries: AtomicU32,
    pub clean_reclaimable_bytes: AtomicU64,
    pub dirty_non_reclaimable_bytes: AtomicU64,
    pub reclaim_scans: AtomicU64,
    pub reclaim_clean_entries: AtomicU64,
    pub reclaim_dirty_drains: AtomicU64,
    pub reclaim_failed_drains: AtomicU64,
    pub payload_frame_bytes: AtomicU32,
    pub payload_frames: AtomicU32,
    pub payload_bytes: AtomicU64,
    pub pmm_reclaimable_bytes: AtomicU64,
    pub pmm_dirty_bytes: AtomicU64,
    pub payload_allocations: AtomicU64,
    pub payload_allocation_failures: AtomicU64,
    pub payload_releases: AtomicU64,
    pub reclaim_returned_frames: AtomicU64,
    pub reclaim_returned_bytes: AtomicU64,
    pub lock_timeouts: AtomicU64,
    pub busy_waits: AtomicU64,
    pub bulk_write_requests: AtomicU64,
    pub bulk_write_sectors: AtomicU64,
    pub selective_flushes: AtomicU64,
    pub selective_writeback_sectors: AtomicU64,
    pub selective_foreign_dirty_sectors_skipped: AtomicU64,
    pub policy_version: AtomicU32,
    pub policy_device_capacity: AtomicU32,
    pub policy_dirty_high_pages: AtomicU32,
    pub policy_dirty_low_pages: AtomicU32,
    pub policy_max_dirty_age_ticks: AtomicU64,
    pub policy_background_page_budget: AtomicU32,
    pub policy_worker_started: AtomicU32,
    pub policy_worker_task_id: AtomicU32,
    pub policy_worker_wakeups: AtomicU64,
    pub policy_background_drains: AtomicU64,
    pub policy_background_sectors: AtomicU64,
    pub policy_background_pressure_drains: AtomicU64,
    pub policy_background_age_drains: AtomicU64,
    pub policy_background_errors: AtomicU64,
    pub policy_clean_device_probes: AtomicU64,
    pub policy_dirty_device_probes: AtomicU64,
    pub policy_full_scan_fallbacks: AtomicU64,
    pub policy_device_dirty_high_water: AtomicU32,
    pub read_ahead_requests: AtomicU64,
    pub read_ahead_issued: AtomicU64,
    pub read_ahead_hits: AtomicU64,
    pub read_ahead_cancellations: AtomicU64,
    pub read_ahead_budget_skips: AtomicU64,
    pub capacity_min_pages: AtomicU32,
    pub capacity_max_pages: AtomicU32,
    pub capacity_ram_limit_pages: AtomicU32,
    pub capacity_active_limit_pages: AtomicU32,
    pub capacity_pressure_level: AtomicU32,
    pub read_ahead_window_pages: AtomicU32,
    pub read_ahead_window_max_pages: AtomicU32,
    pub capacity_reserved0: AtomicU32,
    pub fill_run_requests: AtomicU64,
    pub fill_run_backend_requests: AtomicU64,
    pub fill_run_pages: AtomicU64,
    pub fill_run_sectors: AtomicU64,
    pub fill_run_bytes: AtomicU64,
    pub fill_run_failures: AtomicU64,
    pub fill_run_retries: AtomicU64,
    pub fill_run_max_pages: AtomicU64,
    pub fill_scatter_copy_bytes: AtomicU64,
    pub read_staging_copy_bytes: AtomicU64,
    pub read_caller_copy_bytes: AtomicU64,
    pub read_publish_lock_drops: AtomicU64,
    pub fill_lock_drops: AtomicU64,
    pub capacity_reductions: AtomicU64,
    pub capacity_trimmed_pages: AtomicU64,
    pub read_ahead_pages_scheduled: AtomicU64,
    pub read_ahead_pages_issued: AtomicU64,
    pub read_ahead_random_resets: AtomicU64,
    pub writeback_windows: AtomicU64,
    pub writeback_window_pages: AtomicU64,
    pub writeback_window_requests: AtomicU64,
    pub writeback_window_max_sectors: AtomicU64,
    pub writeback_window_pending_max: AtomicU64,
}
impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}
impl Counters {
    pub const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            sector_bytes: AtomicU32::new(512),
            capacity: AtomicU32::new(super::page_cache_policy::MAX_ENTRIES as u32),
            entries_used: AtomicU32::new(0),
            dirty_entries: AtomicU32::new(0),
            reads: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            fills: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            invalidations: AtomicU64::new(0),
            write_through_requests: AtomicU64::new(0),
            write_through_updates: AtomicU64::new(0),
            flushes: AtomicU64::new(0),
            read_errors: AtomicU64::new(0),
            write_errors: AtomicU64::new(0),
            writeback_waits: AtomicU64::new(0),
            writeback_errors: AtomicU64::new(0),
            dirty_bytes: AtomicU64::new(0),
            dirty_high_water_entries: AtomicU32::new(0),
            writeback_queue_depth: AtomicU32::new(0),
            writeback_queue_high_water: AtomicU32::new(0),
            deferred_write_requests: AtomicU64::new(0),
            dirty_sector_updates: AtomicU64::new(0),
            writeback_drains: AtomicU64::new(0),
            writeback_sectors: AtomicU64::new(0),
            writeback_pressure_drains: AtomicU64::new(0),
            writeback_flush_drains: AtomicU64::new(0),
            writeback_total_ticks: AtomicU64::new(0),
            writeback_max_ticks: AtomicU64::new(0),
            writeback_last_ticks: AtomicU64::new(0),
            writeback_retries: AtomicU64::new(0),
            clean_reclaimable_entries: AtomicU32::new(0),
            dirty_non_reclaimable_entries: AtomicU32::new(0),
            clean_reclaimable_bytes: AtomicU64::new(0),
            dirty_non_reclaimable_bytes: AtomicU64::new(0),
            reclaim_scans: AtomicU64::new(0),
            reclaim_clean_entries: AtomicU64::new(0),
            reclaim_dirty_drains: AtomicU64::new(0),
            reclaim_failed_drains: AtomicU64::new(0),
            payload_frame_bytes: AtomicU32::new(4096),
            payload_frames: AtomicU32::new(0),
            payload_bytes: AtomicU64::new(0),
            pmm_reclaimable_bytes: AtomicU64::new(0),
            pmm_dirty_bytes: AtomicU64::new(0),
            payload_allocations: AtomicU64::new(0),
            payload_allocation_failures: AtomicU64::new(0),
            payload_releases: AtomicU64::new(0),
            reclaim_returned_frames: AtomicU64::new(0),
            reclaim_returned_bytes: AtomicU64::new(0),
            lock_timeouts: AtomicU64::new(0),
            busy_waits: AtomicU64::new(0),
            bulk_write_requests: AtomicU64::new(0),
            bulk_write_sectors: AtomicU64::new(0),
            selective_flushes: AtomicU64::new(0),
            selective_writeback_sectors: AtomicU64::new(0),
            selective_foreign_dirty_sectors_skipped: AtomicU64::new(0),
            policy_version: AtomicU32::new(4),
            policy_device_capacity: AtomicU32::new(8),
            policy_dirty_high_pages: AtomicU32::new(64),
            policy_dirty_low_pages: AtomicU32::new(32),
            policy_max_dirty_age_ticks: AtomicU64::new(2 * crate::arch::pit::DEFAULT_HZ as u64),
            policy_background_page_budget: AtomicU32::new(4),
            policy_worker_started: AtomicU32::new(0),
            policy_worker_task_id: AtomicU32::new(0),
            policy_worker_wakeups: AtomicU64::new(0),
            policy_background_drains: AtomicU64::new(0),
            policy_background_sectors: AtomicU64::new(0),
            policy_background_pressure_drains: AtomicU64::new(0),
            policy_background_age_drains: AtomicU64::new(0),
            policy_background_errors: AtomicU64::new(0),
            policy_clean_device_probes: AtomicU64::new(0),
            policy_dirty_device_probes: AtomicU64::new(0),
            policy_full_scan_fallbacks: AtomicU64::new(0),
            policy_device_dirty_high_water: AtomicU32::new(0),
            read_ahead_requests: AtomicU64::new(0),
            read_ahead_issued: AtomicU64::new(0),
            read_ahead_hits: AtomicU64::new(0),
            read_ahead_cancellations: AtomicU64::new(0),
            read_ahead_budget_skips: AtomicU64::new(0),
            capacity_min_pages: AtomicU32::new(64),
            capacity_max_pages: AtomicU32::new(super::page_cache_policy::MAX_ENTRIES as u32),
            capacity_ram_limit_pages: AtomicU32::new(64),
            capacity_active_limit_pages: AtomicU32::new(64),
            capacity_pressure_level: AtomicU32::new(0),
            read_ahead_window_pages: AtomicU32::new(0),
            read_ahead_window_max_pages: AtomicU32::new(0),
            capacity_reserved0: AtomicU32::new(0),
            fill_run_requests: AtomicU64::new(0),
            fill_run_backend_requests: AtomicU64::new(0),
            fill_run_pages: AtomicU64::new(0),
            fill_run_sectors: AtomicU64::new(0),
            fill_run_bytes: AtomicU64::new(0),
            fill_run_failures: AtomicU64::new(0),
            fill_run_retries: AtomicU64::new(0),
            fill_run_max_pages: AtomicU64::new(0),
            fill_scatter_copy_bytes: AtomicU64::new(0),
            read_staging_copy_bytes: AtomicU64::new(0),
            read_caller_copy_bytes: AtomicU64::new(0),
            read_publish_lock_drops: AtomicU64::new(0),
            fill_lock_drops: AtomicU64::new(0),
            capacity_reductions: AtomicU64::new(0),
            capacity_trimmed_pages: AtomicU64::new(0),
            read_ahead_pages_scheduled: AtomicU64::new(0),
            read_ahead_pages_issued: AtomicU64::new(0),
            read_ahead_random_resets: AtomicU64::new(0),
            writeback_windows: AtomicU64::new(0),
            writeback_window_pages: AtomicU64::new(0),
            writeback_window_requests: AtomicU64::new(0),
            writeback_window_max_sectors: AtomicU64::new(0),
            writeback_window_pending_max: AtomicU64::new(0),
        }
    }
    pub fn snapshot(&self) -> Summary {
        Summary {
            enabled: self.enabled.load(Relaxed),
            sector_bytes: self.sector_bytes.load(Relaxed),
            capacity: self.capacity.load(Relaxed),
            entries_used: self.entries_used.load(Relaxed),
            dirty_entries: self.dirty_entries.load(Relaxed),
            reads: self.reads.load(Relaxed),
            hits: self.hits.load(Relaxed),
            misses: self.misses.load(Relaxed),
            fills: self.fills.load(Relaxed),
            evictions: self.evictions.load(Relaxed),
            invalidations: self.invalidations.load(Relaxed),
            write_through_requests: self.write_through_requests.load(Relaxed),
            write_through_updates: self.write_through_updates.load(Relaxed),
            flushes: self.flushes.load(Relaxed),
            read_errors: self.read_errors.load(Relaxed),
            write_errors: self.write_errors.load(Relaxed),
            writeback_waits: self.writeback_waits.load(Relaxed),
            writeback_errors: self.writeback_errors.load(Relaxed),
            dirty_bytes: self.dirty_bytes.load(Relaxed),
            dirty_high_water_entries: self.dirty_high_water_entries.load(Relaxed),
            writeback_queue_depth: self.writeback_queue_depth.load(Relaxed),
            writeback_queue_high_water: self.writeback_queue_high_water.load(Relaxed),
            deferred_write_requests: self.deferred_write_requests.load(Relaxed),
            dirty_sector_updates: self.dirty_sector_updates.load(Relaxed),
            writeback_drains: self.writeback_drains.load(Relaxed),
            writeback_sectors: self.writeback_sectors.load(Relaxed),
            writeback_pressure_drains: self.writeback_pressure_drains.load(Relaxed),
            writeback_flush_drains: self.writeback_flush_drains.load(Relaxed),
            writeback_total_ticks: self.writeback_total_ticks.load(Relaxed),
            writeback_max_ticks: self.writeback_max_ticks.load(Relaxed),
            writeback_last_ticks: self.writeback_last_ticks.load(Relaxed),
            writeback_retries: self.writeback_retries.load(Relaxed),
            clean_reclaimable_entries: self.clean_reclaimable_entries.load(Relaxed),
            dirty_non_reclaimable_entries: self.dirty_non_reclaimable_entries.load(Relaxed),
            clean_reclaimable_bytes: self.clean_reclaimable_bytes.load(Relaxed),
            dirty_non_reclaimable_bytes: self.dirty_non_reclaimable_bytes.load(Relaxed),
            reclaim_scans: self.reclaim_scans.load(Relaxed),
            reclaim_clean_entries: self.reclaim_clean_entries.load(Relaxed),
            reclaim_dirty_drains: self.reclaim_dirty_drains.load(Relaxed),
            reclaim_failed_drains: self.reclaim_failed_drains.load(Relaxed),
            payload_frame_bytes: self.payload_frame_bytes.load(Relaxed),
            payload_frames: self.payload_frames.load(Relaxed),
            payload_bytes: self.payload_bytes.load(Relaxed),
            pmm_reclaimable_bytes: self.pmm_reclaimable_bytes.load(Relaxed),
            pmm_dirty_bytes: self.pmm_dirty_bytes.load(Relaxed),
            payload_allocations: self.payload_allocations.load(Relaxed),
            payload_allocation_failures: self.payload_allocation_failures.load(Relaxed),
            payload_releases: self.payload_releases.load(Relaxed),
            reclaim_returned_frames: self.reclaim_returned_frames.load(Relaxed),
            reclaim_returned_bytes: self.reclaim_returned_bytes.load(Relaxed),
            lock_timeouts: self.lock_timeouts.load(Relaxed),
            busy_waits: self.busy_waits.load(Relaxed),
            bulk_write_requests: self.bulk_write_requests.load(Relaxed),
            bulk_write_sectors: self.bulk_write_sectors.load(Relaxed),
            selective_flushes: self.selective_flushes.load(Relaxed),
            selective_writeback_sectors: self.selective_writeback_sectors.load(Relaxed),
            selective_foreign_dirty_sectors_skipped: self
                .selective_foreign_dirty_sectors_skipped
                .load(Relaxed),
            policy_version: self.policy_version.load(Relaxed),
            policy_device_capacity: self.policy_device_capacity.load(Relaxed),
            policy_dirty_high_pages: self.policy_dirty_high_pages.load(Relaxed),
            policy_dirty_low_pages: self.policy_dirty_low_pages.load(Relaxed),
            policy_max_dirty_age_ticks: self.policy_max_dirty_age_ticks.load(Relaxed),
            policy_background_page_budget: self.policy_background_page_budget.load(Relaxed),
            policy_worker_started: self.policy_worker_started.load(Relaxed),
            policy_worker_task_id: self.policy_worker_task_id.load(Relaxed),
            policy_worker_wakeups: self.policy_worker_wakeups.load(Relaxed),
            policy_background_drains: self.policy_background_drains.load(Relaxed),
            policy_background_sectors: self.policy_background_sectors.load(Relaxed),
            policy_background_pressure_drains: self.policy_background_pressure_drains.load(Relaxed),
            policy_background_age_drains: self.policy_background_age_drains.load(Relaxed),
            policy_background_errors: self.policy_background_errors.load(Relaxed),
            policy_clean_device_probes: self.policy_clean_device_probes.load(Relaxed),
            policy_dirty_device_probes: self.policy_dirty_device_probes.load(Relaxed),
            policy_full_scan_fallbacks: self.policy_full_scan_fallbacks.load(Relaxed),
            policy_device_dirty_high_water: self.policy_device_dirty_high_water.load(Relaxed),
            read_ahead_requests: self.read_ahead_requests.load(Relaxed),
            read_ahead_issued: self.read_ahead_issued.load(Relaxed),
            read_ahead_hits: self.read_ahead_hits.load(Relaxed),
            read_ahead_cancellations: self.read_ahead_cancellations.load(Relaxed),
            read_ahead_budget_skips: self.read_ahead_budget_skips.load(Relaxed),
            capacity_min_pages: self.capacity_min_pages.load(Relaxed),
            capacity_max_pages: self.capacity_max_pages.load(Relaxed),
            capacity_ram_limit_pages: self.capacity_ram_limit_pages.load(Relaxed),
            capacity_active_limit_pages: self.capacity_active_limit_pages.load(Relaxed),
            capacity_pressure_level: self.capacity_pressure_level.load(Relaxed),
            read_ahead_window_pages: self.read_ahead_window_pages.load(Relaxed),
            read_ahead_window_max_pages: self.read_ahead_window_max_pages.load(Relaxed),
            capacity_reserved0: self.capacity_reserved0.load(Relaxed),
            fill_run_requests: self.fill_run_requests.load(Relaxed),
            fill_run_backend_requests: self.fill_run_backend_requests.load(Relaxed),
            fill_run_pages: self.fill_run_pages.load(Relaxed),
            fill_run_sectors: self.fill_run_sectors.load(Relaxed),
            fill_run_bytes: self.fill_run_bytes.load(Relaxed),
            fill_run_failures: self.fill_run_failures.load(Relaxed),
            fill_run_retries: self.fill_run_retries.load(Relaxed),
            fill_run_max_pages: self.fill_run_max_pages.load(Relaxed),
            fill_scatter_copy_bytes: self.fill_scatter_copy_bytes.load(Relaxed),
            read_staging_copy_bytes: self.read_staging_copy_bytes.load(Relaxed),
            read_caller_copy_bytes: self.read_caller_copy_bytes.load(Relaxed),
            read_publish_lock_drops: self.read_publish_lock_drops.load(Relaxed),
            fill_lock_drops: self.fill_lock_drops.load(Relaxed),
            capacity_reductions: self.capacity_reductions.load(Relaxed),
            capacity_trimmed_pages: self.capacity_trimmed_pages.load(Relaxed),
            read_ahead_pages_scheduled: self.read_ahead_pages_scheduled.load(Relaxed),
            read_ahead_pages_issued: self.read_ahead_pages_issued.load(Relaxed),
            read_ahead_random_resets: self.read_ahead_random_resets.load(Relaxed),
            writeback_windows: self.writeback_windows.load(Relaxed),
            writeback_window_pages: self.writeback_window_pages.load(Relaxed),
            writeback_window_requests: self.writeback_window_requests.load(Relaxed),
            writeback_window_max_sectors: self.writeback_window_max_sectors.load(Relaxed),
            writeback_window_pending_max: self.writeback_window_pending_max.load(Relaxed),
        }
    }
    pub fn clear(&self) {
        self.enabled.store(false, Relaxed);
        self.sector_bytes.store(512, Relaxed);
        self.capacity
            .store(super::page_cache_policy::MAX_ENTRIES as u32, Relaxed);
        self.entries_used.store(0, Relaxed);
        self.dirty_entries.store(0, Relaxed);
        self.reads.store(0, Relaxed);
        self.hits.store(0, Relaxed);
        self.misses.store(0, Relaxed);
        self.fills.store(0, Relaxed);
        self.evictions.store(0, Relaxed);
        self.invalidations.store(0, Relaxed);
        self.write_through_requests.store(0, Relaxed);
        self.write_through_updates.store(0, Relaxed);
        self.flushes.store(0, Relaxed);
        self.read_errors.store(0, Relaxed);
        self.write_errors.store(0, Relaxed);
        self.writeback_waits.store(0, Relaxed);
        self.writeback_errors.store(0, Relaxed);
        self.dirty_bytes.store(0, Relaxed);
        self.dirty_high_water_entries.store(0, Relaxed);
        self.writeback_queue_depth.store(0, Relaxed);
        self.writeback_queue_high_water.store(0, Relaxed);
        self.deferred_write_requests.store(0, Relaxed);
        self.dirty_sector_updates.store(0, Relaxed);
        self.writeback_drains.store(0, Relaxed);
        self.writeback_sectors.store(0, Relaxed);
        self.writeback_pressure_drains.store(0, Relaxed);
        self.writeback_flush_drains.store(0, Relaxed);
        self.writeback_total_ticks.store(0, Relaxed);
        self.writeback_max_ticks.store(0, Relaxed);
        self.writeback_last_ticks.store(0, Relaxed);
        self.writeback_retries.store(0, Relaxed);
        self.clean_reclaimable_entries.store(0, Relaxed);
        self.dirty_non_reclaimable_entries.store(0, Relaxed);
        self.clean_reclaimable_bytes.store(0, Relaxed);
        self.dirty_non_reclaimable_bytes.store(0, Relaxed);
        self.reclaim_scans.store(0, Relaxed);
        self.reclaim_clean_entries.store(0, Relaxed);
        self.reclaim_dirty_drains.store(0, Relaxed);
        self.reclaim_failed_drains.store(0, Relaxed);
        self.payload_frame_bytes.store(4096, Relaxed);
        self.payload_frames.store(0, Relaxed);
        self.payload_bytes.store(0, Relaxed);
        self.pmm_reclaimable_bytes.store(0, Relaxed);
        self.pmm_dirty_bytes.store(0, Relaxed);
        self.payload_allocations.store(0, Relaxed);
        self.payload_allocation_failures.store(0, Relaxed);
        self.payload_releases.store(0, Relaxed);
        self.reclaim_returned_frames.store(0, Relaxed);
        self.reclaim_returned_bytes.store(0, Relaxed);
        self.lock_timeouts.store(0, Relaxed);
        self.busy_waits.store(0, Relaxed);
        self.bulk_write_requests.store(0, Relaxed);
        self.bulk_write_sectors.store(0, Relaxed);
        self.selective_flushes.store(0, Relaxed);
        self.selective_writeback_sectors.store(0, Relaxed);
        self.selective_foreign_dirty_sectors_skipped
            .store(0, Relaxed);
        self.policy_version.store(4, Relaxed);
        self.policy_device_capacity.store(8, Relaxed);
        self.policy_dirty_high_pages.store(64, Relaxed);
        self.policy_dirty_low_pages.store(32, Relaxed);
        self.policy_max_dirty_age_ticks
            .store(2 * crate::arch::pit::DEFAULT_HZ as u64, Relaxed);
        self.policy_background_page_budget.store(4, Relaxed);
        self.policy_worker_started.store(0, Relaxed);
        self.policy_worker_task_id.store(0, Relaxed);
        self.policy_worker_wakeups.store(0, Relaxed);
        self.policy_background_drains.store(0, Relaxed);
        self.policy_background_sectors.store(0, Relaxed);
        self.policy_background_pressure_drains.store(0, Relaxed);
        self.policy_background_age_drains.store(0, Relaxed);
        self.policy_background_errors.store(0, Relaxed);
        self.policy_clean_device_probes.store(0, Relaxed);
        self.policy_dirty_device_probes.store(0, Relaxed);
        self.policy_full_scan_fallbacks.store(0, Relaxed);
        self.policy_device_dirty_high_water.store(0, Relaxed);
        self.read_ahead_requests.store(0, Relaxed);
        self.read_ahead_issued.store(0, Relaxed);
        self.read_ahead_hits.store(0, Relaxed);
        self.read_ahead_cancellations.store(0, Relaxed);
        self.read_ahead_budget_skips.store(0, Relaxed);
        self.capacity_min_pages.store(64, Relaxed);
        self.capacity_max_pages
            .store(super::page_cache_policy::MAX_ENTRIES as u32, Relaxed);
        self.capacity_ram_limit_pages.store(64, Relaxed);
        self.capacity_active_limit_pages.store(64, Relaxed);
        self.capacity_pressure_level.store(0, Relaxed);
        self.read_ahead_window_pages.store(0, Relaxed);
        self.read_ahead_window_max_pages.store(0, Relaxed);
        self.capacity_reserved0.store(0, Relaxed);
        self.fill_run_requests.store(0, Relaxed);
        self.fill_run_backend_requests.store(0, Relaxed);
        self.fill_run_pages.store(0, Relaxed);
        self.fill_run_sectors.store(0, Relaxed);
        self.fill_run_bytes.store(0, Relaxed);
        self.fill_run_failures.store(0, Relaxed);
        self.fill_run_retries.store(0, Relaxed);
        self.fill_run_max_pages.store(0, Relaxed);
        self.fill_scatter_copy_bytes.store(0, Relaxed);
        self.read_staging_copy_bytes.store(0, Relaxed);
        self.read_caller_copy_bytes.store(0, Relaxed);
        self.read_publish_lock_drops.store(0, Relaxed);
        self.fill_lock_drops.store(0, Relaxed);
        self.capacity_reductions.store(0, Relaxed);
        self.capacity_trimmed_pages.store(0, Relaxed);
        self.read_ahead_pages_scheduled.store(0, Relaxed);
        self.read_ahead_pages_issued.store(0, Relaxed);
        self.read_ahead_random_resets.store(0, Relaxed);
        self.writeback_windows.store(0, Relaxed);
        self.writeback_window_pages.store(0, Relaxed);
        self.writeback_window_requests.store(0, Relaxed);
        self.writeback_window_max_sectors.store(0, Relaxed);
        self.writeback_window_pending_max.store(0, Relaxed);
    }
}
