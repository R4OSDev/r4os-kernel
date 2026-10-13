const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const min_growth_pages: usize = 16;
pub const max_growth_pages: usize = 256;
pub const min_retained_tail_pages: usize = 16;

extern fn r4os_rust_heap_growth(required: usize, hint: usize, remaining: usize) usize;
extern fn r4os_rust_heap_next_hint(committed: usize) usize;
extern fn r4os_rust_heap_retained(hint: usize, cap: usize, pressure: bool) usize;
extern fn r4os_rust_heap_release_tail(free: usize, retained: usize, pressure: bool) bool;

pub fn growthPages(required_pages: usize, previous_hint: usize, remaining_pages: usize) usize {
    return r4os_rust_heap_growth(required_pages, previous_hint, remaining_pages);
}
pub fn nextGrowthHint(committed_pages: usize) usize {
    return r4os_rust_heap_next_hint(committed_pages);
}
pub fn retainedTailPages(next_hint: usize, cap_pages: usize, under_pressure: bool) usize {
    return r4os_rust_heap_retained(next_hint, cap_pages, under_pressure);
}
pub fn shouldReleaseTail(free_tail_pages: usize, retained_pages: usize, under_pressure: bool) bool {
    return r4os_rust_heap_release_tail(free_tail_pages, retained_pages, under_pressure);
}

test "geometric growth is bounded by requirement and remaining capacity" {
    try std.testing.expectEqual(@as(usize, 16), growthPages(1, 0, 1000));
    try std.testing.expectEqual(@as(usize, 32), growthPages(20, 32, 1000));
    try std.testing.expectEqual(@as(usize, 300), growthPages(300, 256, 1000));
    try std.testing.expectEqual(@as(usize, 7), growthPages(1, 32, 7));
    try std.testing.expectEqual(@as(usize, 64), nextGrowthHint(32));
    try std.testing.expectEqual(max_growth_pages, nextGrowthHint(200));
}

test "tail hysteresis is RAM bounded and pressure overrides retention" {
    try std.testing.expectEqual(@as(usize, 16), retainedTailPages(128, 8192, false));
    try std.testing.expectEqual(@as(usize, 64), retainedTailPages(128, 65536, false));
    try std.testing.expectEqual(@as(usize, 0), retainedTailPages(128, 65536, true));
    try std.testing.expect(!shouldReleaseTail(128, 64, false));
    try std.testing.expect(shouldReleaseTail(129, 64, false));
    try std.testing.expect(shouldReleaseTail(1, 0, true));
}

test "repeated medium churn needs one geometric commit in the policy model" {
    const iterations: usize = 32;
    const required_pages: usize = 49;
    var committed_pages: usize = 16;
    var hint: usize = min_growth_pages;
    var new_commit_calls: usize = 0;
    var new_uncommit_calls: usize = 0;
    var i: usize = 0;
    while (i < iterations) : (i += 1) {
        if (committed_pages < required_pages) {
            const added = growthPages(required_pages - committed_pages, hint, 8192 - committed_pages);
            committed_pages += added;
            hint = nextGrowthHint(added);
            new_commit_calls += 1;
        }
        const retained = retainedTailPages(hint, 65536, false);
        if (shouldReleaseTail(committed_pages - 16, retained, false)) {
            committed_pages = 16 + retained;
            new_uncommit_calls += 1;
        }
    }
    try std.testing.expectEqual(@as(usize, 1), new_commit_calls);
    try std.testing.expectEqual(@as(usize, 0), new_uncommit_calls);

    // The former exact-grow/absolute-16-page-release policy performs both
    // operations in every iteration of the identical model.
    var legacy_committed_pages: usize = 16;
    var legacy_commit_calls: usize = 0;
    var legacy_uncommit_calls: usize = 0;
    i = 0;
    while (i < iterations) : (i += 1) {
        if (legacy_committed_pages < required_pages) {
            legacy_committed_pages += required_pages - legacy_committed_pages;
            legacy_commit_calls += 1;
        }
        if (legacy_committed_pages > min_retained_tail_pages) {
            legacy_committed_pages = min_retained_tail_pages;
            legacy_uncommit_calls += 1;
        }
    }
    try std.testing.expectEqual(iterations, legacy_commit_calls);
    try std.testing.expectEqual(iterations, legacy_uncommit_calls);
}
