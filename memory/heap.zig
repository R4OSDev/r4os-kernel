// Kernel-Heap v3: Rust-Besitzer unter Rust/memory/heap.rs; diese Fassade
// behaelt die bisherigen Aufrufer und Diagnosefaelle.
//
// Ersetzt die flache Block-Tabelle (8192 Slots, lineare Scans, Coalesce mit
// Restart-Schleife) durch klassische in-band Boundary-Tags:
//   - Jeder Block traegt Header (16 B) und Footer (8 B) im Heap selbst;
//     es gibt keine MAX_BLOCKS-Grenze mehr.
//   - Freie Bloecke haengen in 24 groessenklassierten Listen (Bins);
//     alloc ist good-fit (kurzer Scan im passenden Bin, sonst Kopf eines
//     hoeheren Bins), free koalesziert in O(1) ueber die Footer.
//   - Committed waechst bedarfsweise (virt.commit) und ist RAM-abhaengig
//     gedeckelt (usable/4, min 32 MB, max Fenstergroesse 1 GB) statt der
//     alten harten 32-MB-Grenze.
//   - Geometrisches Commit und RAM-/druckabhaengige Trailing-Hysterese
//     vermeiden Commit/Uncommit-Thrash bei begrenztem freien Randbestand.
//
// SMP-INVARIANTE: Nur Metadaten liegen unter dem reentranten Heap-Owner.
// Wachstum reserviert den unveroeffentlichten Rand und gibt den Owner fuer
// die begrenzten VM-Schritte frei. Konkurrierendes Wachstum wartet nur in
// einem schlaffaehigen Taskkontext; IRQ-/No-Sleep-Aufrufer warten nie.
// Der Reentry-Waechter (control.in_heap) erkennt damit nur noch echte
// Rekursion auf derselben CPU und nicht erlaubte parallele Nutzung.
//
// Block-Layout (alle Offsets relativ HEAP_BASE, Granularitaet 16 Byte):
//   [start+0]  word0: Blockgroesse (inkl. Header/Footer) | bit0 = used
//   [start+8]  word1: used -> requested_size (fuer size_mismatch-Pruefung)
//                     free -> FREE_MAGIC
//   [start+16] used -> Payload            free -> next-Offset der Bin-Liste
//   [start+24]                            free -> prev-Offset der Bin-Liste
//   [start+size-8] Footer: Blockgroesse | bit0 = used (fuer Rueckwaerts-
//                  Koaleszenz vom Nachfolger aus)

const paging = @import("paging.zig");
const virt = @import("virt.zig");
const blocks = @import("blocks.zig");
const heap_policy = @import("heap_policy.zig");
const phys = @import("phys.zig");
const k = @import("../kernel/log.zig");
const task_context = @import("../sched/task_context.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");

// Read-only C-layout default for callers; the active allocator is Rust.
// The reservation cut checks this projection against the window owner.
const HEAP_BASE: u64 = 0xffff_ff10_0000_0000;
const PAGE_SIZE: usize = 4096;
const WINDOW_BYTES: usize = 1024 * 1024 * 1024;
const CAP_MIN_BYTES: usize = 32 * 1024 * 1024;
const MIN_COMMITTED_PAGES: usize = 1;

const GRANULE: usize = 16;
const HEADER_SIZE: usize = 16;
const FOOTER_SIZE: usize = 8;
const OVERHEAD: usize = HEADER_SIZE + FOOTER_SIZE;
const MIN_BLOCK: usize = 64;
const BIN_COUNT: usize = 24;
const NONE: u64 = ~@as(u64, 0);
const FREE_MAGIC: u64 = 0xF6EE_B10C_F6EE_B10C;
const FIT_SCAN_LIMIT: usize = 16;

pub const FreeResult = enum {
    ok,
    invalid_pointer,
    double_free,
    size_mismatch,
};

pub const Stats = extern struct {
    base: u64 = HEAP_BASE,
    pages: usize = 0,
    capacity_bytes: usize = 0,
    reserved_bytes: usize = 0,
    committed_bytes: usize = 0,
    used_bytes: usize = 0,
    free_bytes: usize = 0,
    active_blocks: usize = 0,
    free_blocks: usize = 0,
    largest_free: usize = 0,
    allocation_errors: u64 = 0,
    invalid_free_errors: u64 = 0,
    double_free_errors: u64 = 0,
    size_mismatch_errors: u64 = 0,
    oom_errors: u64 = 0,
    reentry_errors: u64 = 0,
    next_growth_pages: usize = heap_policy.min_growth_pages,
    pending_uncommit_bytes: usize = 0,
    commit_calls: u64 = 0,
    commit_failures: u64 = 0,
    committed_pages_total: u64 = 0,
    uncommit_calls: u64 = 0,
    uncommit_failures: u64 = 0,
    uncommitted_pages_total: u64 = 0,
    release_suppressed: u64 = 0,
    pressure_releases: u64 = 0,
    poison_bytes: u64 = 0,
    retained_tail_pages: usize = 0,
    fragmentation_hint: bool = false,
    bootstrap_bump_active: bool = false,
};

pub const MetadataRange = extern struct {
    base: u64 = 0,
    len: u64 = 0,
};

const RawUnwind = extern struct {
    counter: ?*u32 = null,
    context_present: bool = false,
    active: bool = false,
};
extern fn r4os_rust_heap_init() bool;
extern fn r4os_rust_heap_alloc(size: usize, alignment: usize) ?[*]u8;
extern fn r4os_rust_heap_free(pointer: [*]u8, len: usize) u8;
extern fn r4os_rust_heap_realloc(pointer: [*]u8, len: usize, size: usize, alignment: usize) ?[*]u8;
extern fn r4os_rust_heap_stats(output: *Stats) void;
extern fn r4os_rust_heap_metadata(output: *MetadataRange) void;
extern fn r4os_rust_heap_boot_invariant() bool;
extern fn r4os_rust_heap_growing() bool;
extern fn r4os_rust_heap_initialized() bool;
extern fn r4os_rust_heap_allocator_probe() bool;

// These cuts retain the old task/scheduler/wait and VM owners. No Rust
// control reference crosses them; blocking growth runs after owner release.
var growth_waiters: sync.WaitQueue = sync.WaitQueue.init();
var growth_predicate_context: u8 = 0;
export fn r4os_zig_heap_unwind_enter(output: *RawUnwind) callconv(.c) void {
    const token = task_context.enterUnwind();
    output.* = .{ .counter = token.counter, .context_present = token.context_present, .active = token.active };
}
export fn r4os_zig_heap_unwind_leave(token: *const RawUnwind) callconv(.c) bool {
    return task_context.leaveUnwind(.{ .counter = token.counter, .context_present = token.context_present, .active = token.active });
}
export fn r4os_zig_heap_can_wait() callconv(.c) bool {
    const current = scheduler.current() orelse return false;
    return current.preempt_disable_depth == 0 and current.held_lock_count == 0;
}
fn growthStillActive(_: *anyopaque) bool {
    return r4os_rust_heap_growing();
}
export fn r4os_zig_heap_wait_growth() callconv(.c) bool {
    if (@import("builtin").is_test) @panic("kernel heap wait requested by host fixture");
    return growth_waiters.waitUnless(sync.WAIT_FOREVER, "heap-growth", growthStillActive, &growth_predicate_context) == .signaled;
}
export fn r4os_zig_heap_wake_growth() callconv(.c) void {
    if (@import("builtin").is_test) @panic("kernel heap wake requested by host fixture");
    _ = growth_waiters.wakeAll();
}
export fn r4os_zig_heap_reserve(base: u64, bytes: u64) callconv(.c) u32 {
    if (@import("builtin").is_test) @panic("kernel heap VM reserve requested by host fixture");
    if (base != virt.windowBase(.kernel_heap)) return 0;
    return virt.reserveAt(.{ .window = .kernel_heap, .base = base, .kind = .kernel_heap, .owner = .kernel, .name = "kernel-heap", .len = bytes, .flags = paging.WRITABLE }) catch 0;
}
export fn r4os_zig_heap_commit(id: u32, offset: u64, bytes: u64) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("kernel heap VM commit requested by host fixture");
    virt.commit(id, offset, bytes) catch return false;
    return true;
}
export fn r4os_zig_heap_uncommit(id: u32, offset: u64, bytes: u64) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("kernel heap VM uncommit requested by host fixture");
    virt.uncommit(id, offset, bytes) catch return false;
    return true;
}
export fn r4os_zig_heap_fault(kind: u8, offset: u64) callconv(.c) void {
    if (kind == 0) {
        k.puts("HEAP REENTRY detected (non-preempt invariant violated)\r\n");
        return;
    }
    k.puts("HEAP CORRUPT kind=");
    k.puts(switch (kind) {
        1 => "unlink-header",
        2 => "unlink-next",
        3 => "unlink-prev",
        else => "unknown",
    });
    k.puts(" off=0x");
    k.putHex(offset, 12);
    k.puts("\r\n");
}
pub fn init() bool {
    return r4os_rust_heap_init();
}
pub fn alloc(size: usize, alignment: usize) ?[]u8 {
    const pointer = r4os_rust_heap_alloc(size, alignment) orelse return null;
    return pointer[0..size];
}
pub fn allocBytes(size: usize) ?[]u8 {
    return alloc(size, 1);
}
pub fn free(mem: []u8) FreeResult {
    return switch (r4os_rust_heap_free(mem.ptr, mem.len)) {
        0 => .ok,
        1 => .invalid_pointer,
        2 => .double_free,
        3 => .size_mismatch,
        else => unreachable,
    };
}
pub fn realloc(mem: []u8, new_size: usize, alignment: usize) ?[]u8 {
    const pointer = r4os_rust_heap_realloc(mem.ptr, mem.len, new_size, alignment) orelse return null;
    return pointer[0..new_size];
}
pub fn stats() Stats {
    var result: Stats = undefined;
    r4os_rust_heap_stats(&result);
    return result;
}
pub fn metadataRange() MetadataRange {
    var result: MetadataRange = undefined;
    r4os_rust_heap_metadata(&result);
    return result;
}
pub fn bootInvariant() bool {
    return r4os_rust_heap_boot_invariant();
}
comptime {
    if (@sizeOf(RawUnwind) != 16 or @sizeOf(Stats) != 232 or @sizeOf(MetadataRange) != 16)
        @compileError("Rust kernel heap C layout mismatch");
}

pub fn dumpStats() void {
    const s = stats();
    k.puts("  Kernel heap: v3 pages=");
    k.putDec(@intCast(s.pages));
    k.puts(" base=0x");
    k.putHex(s.base, 16);
    k.puts(" used=");
    k.putDec(@intCast(s.used_bytes));
    k.puts(" free=");
    k.putDec(@intCast(s.free_bytes));
    k.puts(" committed=");
    k.putDec(@intCast(s.committed_bytes));
    k.puts(" cap=");
    k.putDec(@intCast(s.reserved_bytes));
    k.puts("\r\n");

    k.puts("  Kernel heap VM: commit_calls=");
    k.putDec(s.commit_calls);
    k.puts(" pages=");
    k.putDec(s.committed_pages_total);
    k.puts(" failures=");
    k.putDec(s.commit_failures);
    k.puts(" uncommit_calls=");
    k.putDec(s.uncommit_calls);
    k.puts(" pages=");
    k.putDec(s.uncommitted_pages_total);
    k.puts(" failures=");
    k.putDec(s.uncommit_failures);
    k.puts(" retained=");
    k.putDec(@intCast(s.retained_tail_pages));
    k.puts(" suppressed=");
    k.putDec(s.release_suppressed);
    k.puts(" pressure=");
    k.putDec(s.pressure_releases);
    k.puts(" poison_bytes=");
    k.putDec(s.poison_bytes);
    k.puts("\r\n");

    k.puts("  Kernel heap blocks: active=");
    k.putDec(@intCast(s.active_blocks));
    k.puts(" free=");
    k.putDec(@intCast(s.free_blocks));
    k.puts(" largest_free=");
    k.putDec(@intCast(s.largest_free));
    k.puts(" frag=");
    k.puts(if (s.fragmentation_hint) "yes" else "no");
    k.puts("\r\n");

    k.puts("  Kernel heap errors: alloc=");
    k.putDec(s.allocation_errors);
    k.puts(" oom=");
    k.putDec(s.oom_errors);
    k.puts(" invalid_free=");
    k.putDec(s.invalid_free_errors);
    k.puts(" double_free=");
    k.putDec(s.double_free_errors);
    k.puts(" size_mismatch=");
    k.putDec(s.size_mismatch_errors);
    k.puts(" reentry=");
    k.putDec(s.reentry_errors);
    k.puts("\r\n");
}

// ---------------------------------------------------------------------------
// Interna: Wort-Zugriff, Bins, Platzierung, Wachstum
// ---------------------------------------------------------------------------

pub const AcceptanceProbe = struct {
    ok: bool = false,
    iterations: u64 = 0,
    commit_calls: u64 = 0,
    commit_pages: u64 = 0,
    uncommit_calls: u64 = 0,
    release_suppressed: u64 = 0,
    poison_bytes: u64 = 0,
    retained_pages: u64 = 0,
    block_claims: u64 = 0,
    extent_allocations: u64 = 0,
    map_batches: u64 = 0,
    unmap_batches: u64 = 0,
    under_pressure: bool = false,
};

/// Short SMP-profile acceptance workload for the grow/release policy. It
/// deliberately keeps only one medium allocation live at a time, modelling
/// the former exact-grow/exact-trim churn without becoming a long-run test.
pub fn acceptanceProbe() AcceptanceProbe {
    const iterations: u64 = 16;
    const request_bytes: usize = 192 * 1024;
    const before = stats();
    const blocks_before = blocks.hotPathStats();
    const phys_before = phys.stats();
    const paging_before = paging.stats();
    var completed: u64 = 0;
    while (completed < iterations) : (completed += 1) {
        const mem = alloc(request_bytes, GRANULE) orelse break;
        @memset(mem, @as(u8, @truncate(0x51 + completed)));
        if (mem[0] != @as(u8, @truncate(0x51 + completed)) or
            mem[mem.len - 1] != @as(u8, @truncate(0x51 + completed)))
        {
            _ = free(mem);
            break;
        }
        if (free(mem) != .ok) break;
    }
    const after = stats();
    const blocks_after = blocks.hotPathStats();
    const phys_after = phys.stats();
    const paging_after = paging.stats();
    const commit_delta = after.commit_calls -| before.commit_calls;
    const commit_pages_delta = after.committed_pages_total -| before.committed_pages_total;
    const uncommit_delta = after.uncommit_calls -| before.uncommit_calls;
    const suppressed_delta = after.release_suppressed -| before.release_suppressed;
    const poison_delta = after.poison_bytes -| before.poison_bytes;
    const pressure = after.retained_tail_pages == 0;
    const block_claim_delta = blocks_after.claim_transactions -| blocks_before.claim_transactions;
    const extent_allocation_delta = phys_after.extent_allocations -| phys_before.extent_allocations;
    const map_batch_delta = paging_after.map_batches -| paging_before.map_batches;
    const unmap_batch_delta = paging_after.unmap_batches -| paging_before.unmap_batches;
    const state_restored = after.used_bytes == before.used_bytes and
        after.active_blocks == before.active_blocks;
    const bounded_retention = after.pages <= before.pages + heap_policy.max_growth_pages;
    const churn_bounded = if (pressure)
        commit_delta <= iterations and uncommit_delta <= iterations
    else
        commit_delta <= 2 and uncommit_delta <= 1 and suppressed_delta + uncommit_delta >= iterations;
    const batch_accounting = paging_after.map_batch_rollbacks == paging_before.map_batch_rollbacks and
        blocks_after.claim_rollbacks == blocks_before.claim_rollbacks and
        ((commit_pages_delta == 0 and extent_allocation_delta == 0 and map_batch_delta == 0) or
            (commit_pages_delta > 0 and extent_allocation_delta > 0 and map_batch_delta > 0 and
                block_claim_delta <= commit_pages_delta));
    const ok = completed == iterations and state_restored and bounded_retention and churn_bounded and
        batch_accounting and
        after.commit_failures == before.commit_failures and
        after.uncommit_failures == before.uncommit_failures and
        poison_delta >= @as(u64, @intCast(request_bytes)) * iterations and residentCommitProbe();
    return .{
        .ok = ok,
        .iterations = completed,
        .commit_calls = commit_delta,
        .commit_pages = commit_pages_delta,
        .uncommit_calls = uncommit_delta,
        .release_suppressed = suppressed_delta,
        .poison_bytes = poison_delta,
        .retained_pages = after.retained_tail_pages,
        .block_claims = block_claim_delta,
        .extent_allocations = extent_allocation_delta,
        .map_batches = map_batch_delta,
        .unmap_batches = unmap_batch_delta,
        .under_pressure = pressure,
    };
}

// Extends the existing HEAPPROBE with a real late commit failure: four
// completed batches must roll back without changing the pre-existing page.
// The same range must then support a full zeroed commit and ordinary release.
fn residentCommitProbe() bool {
    const bytes = 257 * PAGE_SIZE;
    const before = virt.hotPathStats();
    const id = virt.reserve(.{ .window = .temp_kernel, .len = bytes, .name = "resident-commit-probe" }) catch return false;
    var live = true;
    defer if (live) {
        virt.release(id) catch {};
    };
    const info = virt.rangeInfo(id) orelse return false;
    const memory: [*]u8 = @ptrFromInt(info.base);
    virt.commit(id, bytes - PAGE_SIZE, PAGE_SIZE) catch return false;
    memory[bytes - 1] = 0xA5;
    if (virt.commit(id, 0, bytes)) |_| {
        return false;
    } else |err| {
        if (err != virt.Error.AlreadyCommitted) return false;
    }
    const partial = virt.rangeInfo(id) orelse return false;
    if (partial.committed_bytes != PAGE_SIZE or partial.resident_bytes != PAGE_SIZE or memory[bytes - 1] != 0xA5) return false;
    var offset: usize = 0;
    while (offset < bytes - PAGE_SIZE) : (offset += PAGE_SIZE) {
        if (paging.isMapped(info.base + offset)) return false;
    }
    virt.uncommit(id, bytes - PAGE_SIZE, PAGE_SIZE) catch return false;
    virt.commit(id, 0, bytes) catch return false;
    for (memory[0..bytes]) |byte| {
        if (byte != 0) return false;
    }
    const complete = virt.rangeInfo(id) orelse return false;
    if (complete.committed_bytes != bytes or complete.resident_bytes != bytes) return false;
    virt.release(id) catch return false;
    live = false;
    const after = virt.hotPathStats();
    if (after.resident_commit_batches - before.resident_commit_batches < 6 or
        after.resident_commit_max_batch_pages > 64 or
        after.resident_commit_unlocked_zero_bytes - before.resident_commit_unlocked_zero_bytes < bytes or
        after.resident_rollback_batches == before.resident_rollback_batches) return false;

    // Reserve more pages than can be backed. The app reserve check must
    // reject the whole operation without publishing a successful prefix.
    const impossible = (phys.stats().free_frames + 1) * PAGE_SIZE;
    const oom_id = virt.reserve(.{ .window = .app_stack, .len = impossible, .owner = .r4x_instance, .name = "resident-oom-probe" }) catch return false;
    defer virt.release(oom_id) catch {};
    if (virt.commit(oom_id, 0, impossible)) |_| {
        return false;
    } else |err| {
        if (err != virt.Error.OutOfMemory) return false;
    }
    const empty = virt.rangeInfo(oom_id) orelse return false;
    return empty.committed_bytes == 0 and empty.resident_bytes == 0;
}

// ---------------------------------------------------------------------------
// Boot-Self-Test + Mikrobenchmark (0.56.6). Ausgabe ist EIN COM1-Marker:
//   HEAPCHECK OK ... alloc_avg=<cyc> free_avg=<cyc> churn=<cyc> ...
//   HEAPCHECK FAIL reason=...
// Der Benchmark (rdtsc-Zyklen) dient dem Vorher/Nachher-Vergleich der
// Allokator-Generationen; die Werte sind virtualisierungsabhaengig und nur
// relativ zueinander aussagekraeftig.
// Vorher-Baseline (alter Tabellen-Heap, QEMU 2026-07-03):
//   HEAPCHECK OK ops=2000 alloc_avg=546210 free_avg=13459653
//   churn=14334174960 committed=4096 largest_free=4096
// ---------------------------------------------------------------------------

const SELFTEST_SLOTS: usize = 200;
const SELFTEST_OPS: usize = 2000;

pub fn selfTest() bool {
    if (!r4os_rust_heap_initialized()) return selfTestFail("not-initialized");
    if (!r4os_rust_heap_allocator_probe()) return selfTestFail("rust-allocator");

    // 1) Funktional: drei Groessen, Muster schreiben/lesen, Free ok.
    const sizes = [_]usize{ 24, 400, 6000 };
    var held: [sizes.len][]u8 = undefined;
    for (sizes, 0..) |sz, i| {
        const mem = alloc(sz, 8) orelse return selfTestFail("alloc-basic");
        if (mem.len != sz) return selfTestFail("alloc-len");
        @memset(mem, @intCast(0xA0 + i));
        held[i] = mem;
    }
    for (held, 0..) |mem, i| {
        for (mem) |b| {
            if (b != @as(u8, @intCast(0xA0 + i))) return selfTestFail("pattern");
        }
        if (free(mem) != .ok) return selfTestFail("free-basic");
    }

    // 2) Alignment.
    const aligned = alloc(100, 4096) orelse return selfTestFail("alloc-aligned");
    if ((@intFromPtr(aligned.ptr) & 4095) != 0) return selfTestFail("alignment");
    if (free(aligned) != .ok) return selfTestFail("free-aligned");

    // 3) Fehlererkennung: Double-Free muss als solcher erkannt werden
    //    (Zaehler-Delta exakt 1, kein stiller Erfolg).
    const df_before = stats().double_free_errors;
    const probe = alloc(64, 1) orelse return selfTestFail("alloc-df");
    const probe_copy = probe;
    if (free(probe) != .ok) return selfTestFail("free-df1");
    if (free(probe_copy) == .ok) return selfTestFail("double-free-accepted");
    if (stats().double_free_errors != df_before + 1) return selfTestFail("double-free-count");

    // 4) Churn-Benchmark: pseudo-zufaellige alloc/free-Folge (fester Seed).
    var slots: [SELFTEST_SLOTS]?[]u8 = .{null} ** SELFTEST_SLOTS;
    var rng: u64 = 0x9E3779B97F4A7C15;
    var alloc_cycles: u64 = 0;
    var free_cycles: u64 = 0;
    var alloc_ops: u64 = 0;
    var free_ops: u64 = 0;
    const churn_start = rdtsc();
    var op: usize = 0;
    while (op < SELFTEST_OPS) : (op += 1) {
        rng = rng *% 6364136223846793005 +% 1442695040888963407;
        const slot = @as(usize, @intCast((rng >> 33) % SELFTEST_SLOTS));
        if (slots[slot]) |mem| {
            const t0 = rdtsc();
            const rc = free(mem);
            free_cycles +%= rdtsc() -% t0;
            free_ops += 1;
            if (rc != .ok) return selfTestFail("churn-free");
            slots[slot] = null;
        } else {
            const size = 16 + @as(usize, @intCast((rng >> 8) & 0x7FF));
            const t0 = rdtsc();
            const mem = alloc(size, 8);
            alloc_cycles +%= rdtsc() -% t0;
            alloc_ops += 1;
            slots[slot] = mem orelse return selfTestFail("churn-alloc");
        }
    }
    for (&slots) |*entry| {
        if (entry.*) |mem| {
            if (free(mem) != .ok) return selfTestFail("churn-cleanup");
            entry.* = null;
        }
    }
    const churn_total = rdtsc() -% churn_start;

    // 5) Endzustand: alles wieder frei (keine Leaks durch den Test selbst).
    const s = stats();
    if (s.used_bytes != 0 or s.active_blocks != 0) return selfTestFail("leak");

    k.puts("HEAPCHECK OK ops=");
    k.putDec(@intCast(SELFTEST_OPS));
    k.puts(" alloc_avg=");
    k.putDec(if (alloc_ops == 0) 0 else alloc_cycles / alloc_ops);
    k.puts(" free_avg=");
    k.putDec(if (free_ops == 0) 0 else free_cycles / free_ops);
    k.puts(" churn=");
    k.putDec(churn_total);
    k.puts(" committed=");
    k.putDec(@intCast(s.committed_bytes));
    k.puts(" largest_free=");
    k.putDec(@intCast(s.largest_free));
    k.puts("\r\n");
    return true;
}

fn selfTestFail(reason: []const u8) bool {
    k.puts("HEAPCHECK FAIL reason=");
    k.puts(reason);
    k.puts("\r\n");
    return false;
}

fn rdtsc() u64 {
    var lo: u32 = undefined;
    var hi: u32 = undefined;
    asm volatile ("rdtsc"
        : [lo] "={eax}" (lo),
          [hi] "={edx}" (hi),
    );
    return (@as(u64, hi) << 32) | lo;
}

// ---------------------------------------------------------------------------
// Kleinkram
// ---------------------------------------------------------------------------
