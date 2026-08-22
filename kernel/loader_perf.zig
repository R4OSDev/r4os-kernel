const timer = @import("timer.zig");
const time_core = @import("../platform/time.zig");

pub const service_boot_not_attempted: u32 = 0;
pub const service_boot_ran: u32 = 1;
pub const service_boot_not_found: u32 = 2;
pub const service_boot_failed: u32 = 3;

const audited_boot_critical_count: u32 = 8;
const audited_lazy_candidate_count: u32 = 9;

pub const Summary = struct {
    initialized: u32 = 0,
    loader_started: u32 = 0,
    loader_completed: u32 = 0,
    r4p_runtime_started: u32 = 0,
    r4p_runtime_completed: u32 = 0,
    service_boot_status: u32 = service_boot_not_attempted,
    boot_critical_count: u32 = audited_boot_critical_count,
    lazy_candidate_count: u32 = audited_lazy_candidate_count,

    loader_total_ticks: u64 = 0,
    r4p_runtime_total_ticks: u64 = 0,
    service_boot_ticks: u64 = 0,

    config_load_ticks: u64 = 0,
    config_bytes: u64 = 0,
    config_driver_count: u32 = 0,
    config_disabled_count: u32 = 0,
    config_option_count: u32 = 0,
    config_reserved0: u32 = 0,

    r4l_scan_entries: u64 = 0,
    r4l_candidates: u64 = 0,
    r4l_loaded: u64 = 0,
    r4l_failed: u64 = 0,
    r4l_scan_ticks: u64 = 0,
    r4l_read_ticks: u64 = 0,
    r4l_resolve_ticks: u64 = 0,
    r4l_load_ticks: u64 = 0,

    r4d_scan_entries: u64 = 0,
    r4d_candidates: u64 = 0,
    r4d_discovered: u64 = 0,
    r4d_failed: u64 = 0,
    r4d_scan_ticks: u64 = 0,
    r4d_read_ticks: u64 = 0,
    r4d_probe_ticks: u64 = 0,

    r4p_scan_entries: u64 = 0,
    r4p_candidates: u64 = 0,
    r4p_discovered: u64 = 0,
    r4p_active: u64 = 0,
    r4p_blocked: u64 = 0,
    r4p_failed: u64 = 0,
    r4p_scan_ticks: u64 = 0,
    r4p_read_ticks: u64 = 0,
    r4p_resolve_ticks: u64 = 0,
    r4p_init_ticks: u64 = 0,

    clock_flags: u32 = 0,
    clock_source: u32 = 0,
    clock_generation: u32 = 0,
    timing_valid_spans: u32 = 0,
    timing_unavailable_spans: u32 = 0,
    timing_reserved0: u32 = 0,
    clock_resolution_ns: u64 = 0,
    loader_total_ns: u64 = 0,
    r4p_runtime_total_ns: u64 = 0,
    service_boot_ns: u64 = 0,
    config_load_ns: u64 = 0,
};

var summary: Summary = .{};
var filesystem_loader_start_tick: u64 = 0;
var r4p_runtime_start_tick: u64 = 0;
var filesystem_loader_start_stamp: time_core.MonotonicStamp = .{};
var r4p_runtime_start_stamp: time_core.MonotonicStamp = .{};
var service_boot_start_stamp: time_core.MonotonicStamp = .{};
var config_load_start_stamp: time_core.MonotonicStamp = .{};

pub fn init() void {
    summary = .{
        .initialized = 1,
        .service_boot_status = service_boot_not_attempted,
        .boot_critical_count = audited_boot_critical_count,
        .lazy_candidate_count = audited_lazy_candidate_count,
    };
    filesystem_loader_start_tick = 0;
    r4p_runtime_start_tick = 0;
    filesystem_loader_start_stamp = .{};
    r4p_runtime_start_stamp = .{};
    service_boot_start_stamp = .{};
    config_load_start_stamp = .{};
}

pub fn snapshot() Summary {
    ensure();
    var out = summary;
    const clock = time_core.monotonicSnapshot();
    out.clock_flags = clock.flags;
    out.clock_source = @intFromEnum(clock.source);
    out.clock_generation = clock.generation;
    out.clock_resolution_ns = clock.resolution_ns;
    return out;
}

pub fn now() u64 {
    return timer.tickCount();
}

pub fn elapsedSince(start_tick: u64) u64 {
    const end_tick = now();
    return if (end_tick >= start_tick) end_tick - start_tick else 0;
}

pub fn beginFilesystemLoader() void {
    ensure();
    summary.loader_started = 1;
    filesystem_loader_start_tick = now();
    filesystem_loader_start_stamp = time_core.monotonicCapture();
}

pub fn finishFilesystemLoader() void {
    ensure();
    summary.loader_completed = 1;
    summary.loader_total_ticks +%= elapsedSince(filesystem_loader_start_tick);
    finishHighResolutionSpan(filesystem_loader_start_stamp, &summary.loader_total_ns);
}

pub fn beginR4pRuntime() u64 {
    ensure();
    summary.r4p_runtime_started = 1;
    r4p_runtime_start_tick = now();
    r4p_runtime_start_stamp = time_core.monotonicCapture();
    return r4p_runtime_start_tick;
}

pub fn finishR4pRuntime(start_tick: u64) void {
    ensure();
    summary.r4p_runtime_completed = 1;
    summary.r4p_runtime_total_ticks +%= elapsedSince(start_tick);
    finishHighResolutionSpan(r4p_runtime_start_stamp, &summary.r4p_runtime_total_ns);
}

pub fn beginServiceBoot() u64 {
    ensure();
    summary.service_boot_status = service_boot_not_attempted;
    service_boot_start_stamp = time_core.monotonicCapture();
    return now();
}

pub fn finishServiceBoot(start_tick: u64, status: u32) void {
    ensure();
    summary.service_boot_ticks +%= elapsedSince(start_tick);
    finishHighResolutionSpan(service_boot_start_stamp, &summary.service_boot_ns);
    summary.service_boot_status = status;
}

pub fn beginConfigLoad() u64 {
    ensure();
    config_load_start_stamp = time_core.monotonicCapture();
    return now();
}

pub fn recordConfigLoad(start_tick: u64, bytes: usize, driver_count: usize, disabled_count: usize, option_count: usize) void {
    ensure();
    summary.config_load_ticks +%= elapsedSince(start_tick);
    finishHighResolutionSpan(config_load_start_stamp, &summary.config_load_ns);
    summary.config_bytes = saturatingU64(bytes);
    summary.config_driver_count = saturatingU32(driver_count);
    summary.config_disabled_count = saturatingU32(disabled_count);
    summary.config_option_count = saturatingU32(option_count);
}

pub fn recordR4lScanEntry() void {
    ensure();
    summary.r4l_scan_entries +%= 1;
}

pub fn recordR4lCandidate() void {
    ensure();
    summary.r4l_candidates +%= 1;
}

pub fn addR4lScanTicks(start_tick: u64) void {
    ensure();
    summary.r4l_scan_ticks +%= elapsedSince(start_tick);
}

pub fn addR4lReadTicks(start_tick: u64) void {
    ensure();
    summary.r4l_read_ticks +%= elapsedSince(start_tick);
}

pub fn addR4lResolveTicks(start_tick: u64) void {
    ensure();
    summary.r4l_resolve_ticks +%= elapsedSince(start_tick);
}

pub fn addR4lLoadTicks(start_tick: u64) void {
    ensure();
    summary.r4l_load_ticks +%= elapsedSince(start_tick);
}

pub fn recordR4lResults(loaded: usize, total_candidates: usize) void {
    ensure();
    summary.r4l_loaded = saturatingU64(loaded);
    summary.r4l_failed = if (total_candidates > loaded) saturatingU64(total_candidates - loaded) else 0;
}

pub fn recordR4dScanEntry() void {
    ensure();
    summary.r4d_scan_entries +%= 1;
}

pub fn recordR4dCandidate() void {
    ensure();
    summary.r4d_candidates +%= 1;
}

pub fn addR4dScanTicks(start_tick: u64) void {
    ensure();
    summary.r4d_scan_ticks +%= elapsedSince(start_tick);
}

pub fn addR4dReadTicks(start_tick: u64) void {
    ensure();
    summary.r4d_read_ticks +%= elapsedSince(start_tick);
}

pub fn addR4dProbeTicks(start_tick: u64) void {
    ensure();
    summary.r4d_probe_ticks +%= elapsedSince(start_tick);
}

pub fn recordR4dResults(discovered: usize) void {
    ensure();
    summary.r4d_discovered = saturatingU64(discovered);
    summary.r4d_failed = if (summary.r4d_candidates > summary.r4d_discovered) summary.r4d_candidates - summary.r4d_discovered else 0;
}

pub fn recordR4pScanEntry() void {
    ensure();
    summary.r4p_scan_entries +%= 1;
}

pub fn recordR4pCandidate() void {
    ensure();
    summary.r4p_candidates +%= 1;
}

pub fn addR4pScanTicks(start_tick: u64) void {
    ensure();
    summary.r4p_scan_ticks +%= elapsedSince(start_tick);
}

pub fn addR4pReadTicks(start_tick: u64) void {
    ensure();
    summary.r4p_read_ticks +%= elapsedSince(start_tick);
}

pub fn addR4pResolveTicks(start_tick: u64) void {
    ensure();
    summary.r4p_resolve_ticks +%= elapsedSince(start_tick);
}

pub fn addR4pInitTicks(start_tick: u64) void {
    ensure();
    summary.r4p_init_ticks +%= elapsedSince(start_tick);
}

pub fn recordR4pResults(discovered: usize, active: usize, blocked: usize, failed: usize) void {
    ensure();
    summary.r4p_discovered = saturatingU64(discovered);
    summary.r4p_active = saturatingU64(active);
    summary.r4p_blocked = saturatingU64(blocked);
    summary.r4p_failed = saturatingU64(failed);
}

fn ensure() void {
    if (summary.initialized == 0) init();
}

fn finishHighResolutionSpan(start: time_core.MonotonicStamp, destination: *u64) void {
    const clock = time_core.monotonicSnapshot();
    if ((clock.flags & time_core.monotonic_flag_high_resolution) == 0) {
        summary.timing_unavailable_spans +|= 1;
        return;
    }
    const elapsed = time_core.monotonicElapsedSince(start) orelse {
        summary.timing_unavailable_spans +|= 1;
        return;
    };
    destination.* +|= elapsed;
    summary.timing_valid_spans +|= 1;
}

fn saturatingU32(value: usize) u32 {
    return @intCast(@min(value, @as(usize, 0xFFFF_FFFF)));
}

fn saturatingU64(value: usize) u64 {
    return @intCast(value);
}
