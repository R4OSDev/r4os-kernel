// Shared x86_64 hardware clock source.
//
// The normal path is an adjusted TSC read followed by a precomputed
// multiply/shift conversion. HPET is retained as calibration reference,
// periodic watchdog, and fail-safe common fallback.

const std = @import("std");
const hpet = @import("../arch/x86_64/hpet.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const cpu = @import("cpu.zig");
const math = @import("monotonic_math.zig");

pub const Source = enum(u32) {
    unavailable = 0,
    tsc = 1,
    hpet = 2,
};

pub const FallbackReason = enum(u32) {
    none = 0,
    no_tsc = 1,
    non_invariant_without_reference = 2,
    frequency_unavailable = 3,
    calibration_unstable = 4,
    frequency_mismatch = 5,
    cpu_skew = 6,
    tsc_discontinuity = 7,
    hpet_unavailable = 8,
};

pub const StampKind = enum(u8) {
    unavailable = 0,
    tsc = 1,
    hpet = 2,
};

pub const Stamp = struct {
    raw: u64 = 0,
    kind: StampKind = .unavailable,
    generation: u32 = 0,
};

pub const Status = struct {
    source: Source = .unavailable,
    generation: u32 = 0,
    frequency_hz: u64 = 0,
    resolution_ns: u64 = 0,
    tsc_present: bool = false,
    tsc_invariant: bool = false,
    tsc_hpet_calibrated: bool = false,
    hpet_available: bool = false,
    registered_cpu_mask: u64 = 0,
    max_cpu_skew_ns: u64 = 0,
    calibration_error_ppm: u64 = 0,
    fallback_reason: FallbackReason = .none,
};

const min_plausible_tsc_hz: u64 = 1_000_000;
const max_plausible_tsc_hz: u64 = 10_000_000_000;
const calibration_samples: usize = 3;
const calibration_hpet_divisor: u64 = 200; // 5 ms per sample.
const calibration_tolerance_ppm: u64 = 20_000;
const cpu_offset_samples: usize = 5;
const max_cpu_offset_ns: u64 = 1_000_000_000;
const max_cpu_offset_spread_ns: u64 = 250_000;
const watchdog_base_tolerance_ns: u64 = 2_000_000;
const watchdog_drift_tolerance_ppm: u64 = 5_000;

const Correlation = struct {
    hpet_raw: u64 = 0,
    tsc_raw: u64 = 0,
    bracket_cycles: u64 = 0,
};

const Calibration = struct {
    ok: bool = false,
    frequency_hz: u64 = 0,
    error_ppm: u64 = 0,
    reference: Correlation = .{},
    failure: FallbackReason = .calibration_unstable,
};

var early_initialized = false;
var early_tsc_origin: u64 = 0;
var tsc_present = false;
var tsc_invariant = false;
var tsc_frequency_hz: u64 = 0;
var tsc_scale: math.FixedScale = .{};
var tsc_hpet_calibrated = false;
var calibration_error_ppm: u64 = 0;
var hpet_configured = false;
var hpet_frequency_hz: u64 = 0;
var hpet_scale: math.FixedScale = .{};
var hpet_origin_raw: u64 = 0;
var hpet_epoch_ns: u64 = 0;
var reference_hpet_raw: u64 = 0;
var reference_tsc_raw: u64 = 0;
var last_validation_tsc: u64 = 0;
var cpu_corrections: [percpu.max_cpus]i64 = .{0} ** percpu.max_cpus;
var registered_cpu_mask: u64 = 0;
var max_cpu_skew_ns: u64 = 0;
var active_source_raw: u32 = @intFromEnum(Source.unavailable);
var active_generation: u32 = 0;
var published_ns: u64 = 0;
var fallback_reason_raw: u32 = @intFromEnum(FallbackReason.none);

pub fn earlyInit() void {
    if (early_initialized) return;
    early_tsc_origin = readTsc();
    early_initialized = true;
}

pub fn configureCpuClock() void {
    if (!early_initialized) earlyInit();
    const info = cpu.status();
    tsc_present = info.features.tsc;
    tsc_invariant = info.features.invariant_tsc;
    cpu_corrections = .{0} ** percpu.max_cpus;
    @atomicStore(u64, &registered_cpu_mask, if (tsc_present) 1 else 0, .release);

    if (!tsc_present) {
        setFallbackReason(.no_tsc);
        return;
    }
    const exact_frequency = exactTscFrequencyHz(info);
    if (exact_frequency != 0) installTscFrequency(exact_frequency);

    const choice = math.chooseHardwareClock(
        tsc_present,
        tsc_scale.valid(),
        tsc_invariant,
        false,
        false,
        true,
    );
    if (choice == .tsc) {
        activate(.tsc);
        setFallbackReason(.none);
    } else if (!tsc_invariant) {
        setFallbackReason(.non_invariant_without_reference);
    } else {
        setFallbackReason(.frequency_unavailable);
    }
}

/// Attaches HPET after ACPI mapping. The caller supplies its already
/// published periodic/hardware epoch so a fallback cannot move time back.
pub fn attachHpetClock(prior_ns: u64) void {
    const hpet_status = hpet.status();
    if (!hpet_status.mapped or !hpet_status.enabled or hpet_status.frequency_hz == 0) {
        if (activeSource() == .unavailable) setFallbackReason(.hpet_unavailable);
        return;
    }

    hpet_frequency_hz = hpet_status.frequency_hz;
    hpet_scale = math.FixedScale.initFloor(math.nanoseconds_per_second, hpet_frequency_hz);
    if (!hpet_scale.valid()) {
        if (activeSource() == .unavailable) setFallbackReason(.hpet_unavailable);
        return;
    }
    hpet_origin_raw = hpet.readExtendedMainCounter();
    hpet_epoch_ns = publish(prior_ns);
    hpet_configured = true;

    if (tsc_present) {
        const exact_frequency = tsc_frequency_hz;
        const calibration = calibrateTsc(exact_frequency);
        if (calibration.ok) {
            if (exact_frequency == 0) installTscFrequency(calibration.frequency_hz);
            calibration_error_ppm = calibration.error_ppm;
            tsc_hpet_calibrated = true;
            reference_hpet_raw = calibration.reference.hpet_raw;
            reference_tsc_raw = calibration.reference.tsc_raw;
            last_validation_tsc = calibration.reference.tsc_raw;
            @atomicStore(u64, &registered_cpu_mask, 1, .release);
            if (math.chooseHardwareClock(true, tsc_scale.valid(), tsc_invariant, true, true, true) == .tsc) {
                activate(.tsc);
                setFallbackReason(.none);
                _ = nowNanoseconds();
                return;
            }
        } else {
            setFallbackReason(calibration.failure);
        }
    } else {
        setFallbackReason(.no_tsc);
    }

    activate(.hpet);
    _ = nowNanoseconds();
}

/// Called by an AP after installing its GS-local CPU index and before it is
/// made schedulable. A common HPET correlation supplies the per-CPU offset.
pub fn registerCurrentCpu(index: u32) bool {
    if (index >= percpu.max_cpus) return false;
    if (index == 0) {
        _ = @atomicRmw(u64, &registered_cpu_mask, .Or, 1, .acq_rel);
        return true;
    }
    if (activeSource() != .tsc) {
        _ = @atomicRmw(u64, &registered_cpu_mask, .Or, @as(u64, 1) << @intCast(index), .acq_rel);
        return true;
    }
    if (!hpet_configured or !tsc_scale.valid() or reference_hpet_raw == 0) return false;

    var best = Correlation{};
    var best_correction: i64 = 0;
    var minimum_correction: i64 = std.math.maxInt(i64);
    var maximum_correction: i64 = std.math.minInt(i64);
    var sample_index: usize = 0;
    while (sample_index < cpu_offset_samples) : (sample_index += 1) {
        const sample = correlatedSample();
        const expected = reference_tsc_raw +% math.scaleFloor(
            sample.hpet_raw -% reference_hpet_raw,
            tsc_frequency_hz,
            hpet_frequency_hz,
        );
        const correction = math.signedCounterCorrection(expected, sample.tsc_raw) orelse return false;
        if (correction < minimum_correction) minimum_correction = correction;
        if (correction > maximum_correction) maximum_correction = correction;
        if (sample_index == 0 or sample.bracket_cycles < best.bracket_cycles) {
            best = sample;
            best_correction = correction;
        }
        asm volatile ("pause");
    }

    const absolute_ns = tsc_scale.apply(math.absoluteCorrection(best_correction));
    const spread_cycles: u64 = @intCast(@as(i128, maximum_correction) - @as(i128, minimum_correction));
    const spread_ns = tsc_scale.apply(spread_cycles);
    const maximum_bracket = @max(@as(u64, 1), tsc_frequency_hz / 500);
    if (absolute_ns > max_cpu_offset_ns or spread_ns > max_cpu_offset_spread_ns or
        best.bracket_cycles > maximum_bracket) return false;

    cpu_corrections[index] = best_correction;
    _ = @atomicRmw(u64, &max_cpu_skew_ns, .Max, absolute_ns, .acq_rel);
    _ = @atomicRmw(u64, &registered_cpu_mask, .Or, @as(u64, 1) << @intCast(index), .acq_rel);
    return true;
}

/// BSP-side publication barrier after an AP reaches `parked`. A failed AP
/// clock qualification demotes all CPUs to HPET rather than rejecting the AP.
pub fn finalizeCpuRegistration(index: u32) bool {
    if (index >= percpu.max_cpus) return false;
    const bit = @as(u64, 1) << @intCast(index);
    if ((@atomicLoad(u64, &registered_cpu_mask, .acquire) & bit) != 0) return true;
    fallbackToHpet(.cpu_skew);
    if (activeSource() == .hpet) {
        _ = @atomicRmw(u64, &registered_cpu_mask, .Or, bit, .acq_rel);
    }
    return false;
}

/// Low-frequency BSP watchdog. The timer owner calls this at a bounded IRQ
/// cadence; normal clock reads never touch HPET.
pub fn periodicValidate() bool {
    if (percpu.currentIndex() != 0 or activeSource() != .tsc or !hpet_configured or
        !tsc_hpet_calibrated) return true;

    const current_tsc = adjustedTscRaw();
    const previous_tsc = last_validation_tsc;
    if (previous_tsc != 0 and !math.counterAdvanced(previous_tsc, current_tsc)) {
        fallbackToHpet(.tsc_discontinuity);
        return false;
    }
    last_validation_tsc = current_tsc;

    const current_hpet = hpet.readExtendedMainCounter();
    const tsc_elapsed_ns = tsc_scale.apply(current_tsc -% reference_tsc_raw);
    const hpet_elapsed_ns = hpet_scale.apply(current_hpet -% reference_hpet_raw);
    const difference = if (tsc_elapsed_ns >= hpet_elapsed_ns)
        tsc_elapsed_ns - hpet_elapsed_ns
    else
        hpet_elapsed_ns - tsc_elapsed_ns;
    const allowed = watchdog_base_tolerance_ns +| math.scaleFloor(
        hpet_elapsed_ns,
        watchdog_drift_tolerance_ppm,
        1_000_000,
    );
    if (difference > allowed) {
        fallbackToHpet(.tsc_discontinuity);
        return false;
    }
    return true;
}

pub fn capture() Stamp {
    const source = activeSource();
    return switch (source) {
        .tsc => .{ .raw = adjustedTscRaw(), .kind = .tsc, .generation = generation() },
        .hpet => .{ .raw = hpet.readExtendedMainCounter(), .kind = .hpet, .generation = generation() },
        .unavailable => if (early_initialized)
            .{ .raw = readTsc(), .kind = .tsc, .generation = generation() }
        else
            .{},
    };
}

pub fn resolve(stamp: Stamp) ?u64 {
    return switch (stamp.kind) {
        .unavailable => null,
        .tsc => if (tsc_scale.valid())
            tsc_scale.apply(stamp.raw -% early_tsc_origin)
        else
            null,
        .hpet => if (hpet_configured)
            hpet_epoch_ns +| hpet_scale.apply(stamp.raw -% hpet_origin_raw)
        else
            null,
    };
}

pub fn nowNanoseconds() ?u64 {
    const candidate = switch (activeSource()) {
        .tsc => tsc_scale.apply(adjustedTscRaw() -% early_tsc_origin),
        .hpet => hpet_epoch_ns +| hpet_scale.apply(hpet.readExtendedMainCounter() -% hpet_origin_raw),
        .unavailable => return null,
    };
    return publish(candidate);
}

pub fn publishExternal(candidate: u64) u64 {
    return publish(candidate);
}

pub fn lastPublishedNanoseconds() u64 {
    return @atomicLoad(u64, &published_ns, .acquire);
}

pub fn activeSource() Source {
    return @enumFromInt(@atomicLoad(u32, &active_source_raw, .acquire));
}

pub fn generation() u32 {
    return @atomicLoad(u32, &active_generation, .acquire);
}

pub fn status() Status {
    const source = activeSource();
    const frequency = switch (source) {
        .tsc => tsc_frequency_hz,
        .hpet => hpet_frequency_hz,
        .unavailable => 0,
    };
    return .{
        .source = source,
        .generation = generation(),
        .frequency_hz = frequency,
        .resolution_ns = math.resolutionNanoseconds(frequency, 1),
        .tsc_present = tsc_present,
        .tsc_invariant = tsc_invariant,
        .tsc_hpet_calibrated = tsc_hpet_calibrated,
        .hpet_available = hpet_configured,
        .registered_cpu_mask = @atomicLoad(u64, &registered_cpu_mask, .acquire),
        .max_cpu_skew_ns = @atomicLoad(u64, &max_cpu_skew_ns, .acquire),
        .calibration_error_ppm = calibration_error_ppm,
        .fallback_reason = @enumFromInt(@atomicLoad(u32, &fallback_reason_raw, .acquire)),
    };
}

pub fn sourceName(source: Source) []const u8 {
    return switch (source) {
        .unavailable => "unavailable",
        .tsc => "TSC",
        .hpet => "HPET main counter",
    };
}

pub fn fallbackReasonName(reason: FallbackReason) []const u8 {
    return switch (reason) {
        .none => "none",
        .no_tsc => "no-tsc",
        .non_invariant_without_reference => "non-invariant-without-reference",
        .frequency_unavailable => "frequency-unavailable",
        .calibration_unstable => "calibration-unstable",
        .frequency_mismatch => "frequency-mismatch",
        .cpu_skew => "cpu-skew",
        .tsc_discontinuity => "tsc-discontinuity",
        .hpet_unavailable => "hpet-unavailable",
    };
}

fn installTscFrequency(frequency_hz: u64) void {
    if (frequency_hz < min_plausible_tsc_hz or frequency_hz > max_plausible_tsc_hz) return;
    const scale = math.FixedScale.initFloor(math.nanoseconds_per_second, frequency_hz);
    if (!scale.valid()) return;
    tsc_frequency_hz = frequency_hz;
    tsc_scale = scale;
}

fn activate(source: Source) void {
    if (activeSource() == source) return;
    _ = @atomicRmw(u32, &active_generation, .Add, 1, .acq_rel);
    @atomicStore(u32, &active_source_raw, @intFromEnum(source), .release);
}

fn fallbackToHpet(reason: FallbackReason) void {
    const prior = nowNanoseconds() orelse lastPublishedNanoseconds();
    setFallbackReason(reason);
    if (hpet_configured) {
        hpet_origin_raw = hpet.readExtendedMainCounter();
        hpet_epoch_ns = publish(prior);
        activate(.hpet);
        _ = nowNanoseconds();
    } else {
        activate(.unavailable);
    }
}

fn calibrateTsc(exact_frequency_hz: u64) Calibration {
    if (!hpet_configured or !tsc_present) return .{};
    const target_hpet_ticks = @max(@as(u64, 1), hpet_frequency_hz / calibration_hpet_divisor);
    var frequencies: [calibration_samples]u64 = .{0} ** calibration_samples;
    var best_reference = Correlation{};
    var index: usize = 0;
    while (index < calibration_samples) : (index += 1) {
        const start = correlatedSample();
        while (hpet.readExtendedMainCounter() -% start.hpet_raw < target_hpet_ticks) {
            asm volatile ("pause");
        }
        const finish = correlatedSample();
        const hpet_delta = finish.hpet_raw -% start.hpet_raw;
        const tsc_delta = finish.tsc_raw -% start.tsc_raw;
        if (hpet_delta == 0 or tsc_delta == 0) return .{};
        frequencies[index] = math.scaleFloor(tsc_delta, hpet_frequency_hz, hpet_delta);
        if (index == 0 or finish.bracket_cycles < best_reference.bracket_cycles) best_reference = finish;
    }
    sortThree(&frequencies);
    const measured_frequency = frequencies[1];
    if (measured_frequency < min_plausible_tsc_hz or measured_frequency > max_plausible_tsc_hz) return .{};
    if (math.frequencyErrorPpm(frequencies[0], frequencies[2]) > calibration_tolerance_ppm) return .{};

    const sample_spread_ppm = math.frequencyErrorPpm(frequencies[0], frequencies[2]);
    var error_ppm = sample_spread_ppm;
    if (exact_frequency_hz != 0) {
        const exact_error_ppm = math.frequencyErrorPpm(measured_frequency, exact_frequency_hz);
        error_ppm = @max(sample_spread_ppm, exact_error_ppm);
        if (exact_error_ppm > calibration_tolerance_ppm) {
            return .{ .frequency_hz = measured_frequency, .error_ppm = error_ppm, .failure = .frequency_mismatch };
        }
    }
    return .{
        .ok = true,
        .frequency_hz = if (exact_frequency_hz != 0) exact_frequency_hz else measured_frequency,
        .error_ppm = error_ppm,
        .reference = best_reference,
        .failure = .none,
    };
}

fn correlatedSample() Correlation {
    const before = readTsc();
    const hpet_raw = hpet.readExtendedMainCounter();
    const after = readTsc();
    const bracket = after -% before;
    return .{
        .hpet_raw = hpet_raw,
        .tsc_raw = before +% bracket / 2,
        .bracket_cycles = bracket,
    };
}

fn sortThree(values: *[calibration_samples]u64) void {
    if (values[0] > values[1]) std.mem.swap(u64, &values[0], &values[1]);
    if (values[1] > values[2]) std.mem.swap(u64, &values[1], &values[2]);
    if (values[0] > values[1]) std.mem.swap(u64, &values[0], &values[1]);
}

fn exactTscFrequencyHz(info: cpu.Status) u64 {
    if (info.tsc_denominator == 0 or info.tsc_numerator == 0 or info.crystal_hz == 0) return 0;
    const frequency = (@as(u128, info.crystal_hz) * info.tsc_numerator) / info.tsc_denominator;
    if (frequency < min_plausible_tsc_hz or frequency > max_plausible_tsc_hz) return 0;
    return @intCast(frequency);
}

fn adjustedTscRaw() u64 {
    const raw = readTsc();
    const index = percpu.currentIndex();
    if (index >= percpu.max_cpus) return raw;
    return math.applySignedCorrection(raw, cpu_corrections[index]);
}

fn publish(candidate: u64) u64 {
    var current = @atomicLoad(u64, &published_ns, .acquire);
    while (candidate > current) {
        if (@cmpxchgWeak(u64, &published_ns, current, candidate, .acq_rel, .acquire)) |actual| {
            current = actual;
        } else {
            return candidate;
        }
    }
    return current;
}

fn setFallbackReason(reason: FallbackReason) void {
    @atomicStore(u32, &fallback_reason_raw, @intFromEnum(reason), .release);
}

fn readTsc() u64 {
    var low: u32 = undefined;
    var high: u32 = undefined;
    asm volatile ("lfence");
    asm volatile ("rdtsc"
        : [low] "={eax}" (low),
          [high] "={edx}" (high),
    );
    return (@as(u64, high) << 32) | low;
}
