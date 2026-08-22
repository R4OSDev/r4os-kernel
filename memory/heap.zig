// Kernel-Heap v3 (0.56.6): Boundary-Tags + segregierte Free-Listen.
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
//   - Trailing-Release gibt freie Seiten am Heap-Ende zurueck, behaelt aber
//     RELEASE_KEEP_PAGES als Hysterese gegen Commit/Uncommit-Thrash.
//
// NICHT-PREEMPTIERBARKEITS-INVARIANTE (Befund 1.3): Der Heap ist bewusst
// lockfrei. Das traegt nur, weil (a) der Scheduler kooperativ ist und
// Kernel-Code zwischen Yields nicht verdraengt wird, (b) KEINE Funktion in
// dieser Datei yieldet oder blockiert und (c) kein IRQ-Handler allokiert.
// Wer einen dieser Punkte aendert, MUSS einen Heap-Lock einfuehren. Ein
// Reentry-Waechter (control.in_heap) macht Verstoesse als Zaehler + einmalige
// COM1-Zeile sichtbar, statt still Metadaten zu zerstoeren.
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
const map = @import("map.zig");
const k = @import("../kernel/log.zig");
const task_context = @import("../sched/task_context.zig");

const HEAP_BASE: u64 = virt.windowBase(.kernel_heap);
const PAGE_SIZE: usize = 4096;
const WINDOW_BYTES: usize = 1024 * 1024 * 1024;
const CAP_MIN_BYTES: usize = 32 * 1024 * 1024;
const MIN_COMMITTED_PAGES: usize = 1;
const RELEASE_KEEP_PAGES: usize = 16;

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

pub const Stats = struct {
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
    fragmentation_hint: bool = false,
    bootstrap_bump_active: bool = false,
};

pub const MetadataRange = struct {
    base: u64 = 0,
    len: u64 = 0,
};

const Control = struct {
    bins: [BIN_COUNT]u64 = .{NONE} ** BIN_COUNT,
    heap_top: usize = 0,
    committed_pages: usize = 0,
    cap_pages: usize = 0,
    used_bytes: usize = 0,
    active_blocks: usize = 0,
    free_blocks: usize = 0,
    free_bytes: usize = 0,
    range_id: u32 = 0,
    initialized: bool = false,
    in_heap: bool = false,
    reentry_reported: bool = false,
    allocation_errors: u64 = 0,
    invalid_free_errors: u64 = 0,
    double_free_errors: u64 = 0,
    size_mismatch_errors: u64 = 0,
    oom_errors: u64 = 0,
    reentry_errors: u64 = 0,
};

var control: Control = .{};

pub fn init() bool {
    control = .{};
    control.range_id = virt.reserveAt(.{
        .window = .kernel_heap,
        .base = HEAP_BASE,
        .kind = .kernel_heap,
        .owner = .kernel,
        .name = "kernel-heap",
        .len = WINDOW_BYTES,
        .flags = paging.WRITABLE,
    }) catch 0;
    if (control.range_id == 0) return false;

    const summary = map.summarize();
    var cap_bytes: usize = @intCast(summary.usable_page_bytes / 4);
    if (cap_bytes < CAP_MIN_BYTES) cap_bytes = CAP_MIN_BYTES;
    if (cap_bytes > WINDOW_BYTES) cap_bytes = WINDOW_BYTES;
    control.cap_pages = cap_bytes / PAGE_SIZE;

    control.initialized = true;
    if (!growCommitted(MIN_COMMITTED_PAGES * PAGE_SIZE)) {
        control.initialized = false;
        return false;
    }
    return true;
}

pub fn alloc(size: usize, alignment: usize) ?[]u8 {
    const guard = enterHeap() orelse return allocFailure();
    defer leaveHeap(guard);
    if (!control.initialized or size == 0) return allocFailure();
    const align_value = normalizeAlignment(alignment) orelse return allocFailure();
    const need = blockSizeFor(size) orelse return oomFailure();
    if (need > control.cap_pages * PAGE_SIZE) return oomFailure();

    if (align_value <= GRANULE) {
        if (findFit(need)) |off| return placeBlock(off, need, size);
        const grow_need = need + MIN_BLOCK;
        if (!growCommitted(grow_need)) return oomFailure();
        if (findFit(need)) |off| return placeBlock(off, need, size);
        return oomFailure();
    }
    return allocAligned(size, need, align_value);
}

pub fn allocBytes(size: usize) ?[]u8 {
    return alloc(size, 1);
}

pub fn free(mem: []u8) FreeResult {
    const guard = enterHeap() orelse return .invalid_pointer;
    defer leaveHeap(guard);
    if (!control.initialized or mem.len == 0) return recordInvalidFree();
    const check = validateUsedBlock(mem.ptr, mem.len) orelse return recordInvalidFree();
    switch (check.kind) {
        .used_ok => {},
        .free_block => return recordDoubleFree(),
        .size_mismatch => return recordSizeMismatch(),
    }

    control.active_blocks -= 1;
    control.used_bytes -= check.requested;
    releaseBlock(check.offset, check.size);
    releaseTrailingPages();
    return .ok;
}

const InPlaceResult = union(enum) {
    done: []u8,
    failed,
    need_copy,
};

pub fn realloc(mem: []u8, new_size: usize, alignment: usize) ?[]u8 {
    if (!control.initialized) return null;
    if (new_size == 0) {
        _ = free(mem);
        return null;
    }
    if (mem.len == 0) return alloc(new_size, alignment);
    const align_value = normalizeAlignment(alignment) orelse {
        control.allocation_errors += 1;
        return null;
    };

    switch (reallocInPlace(mem, new_size, align_value)) {
        .done => |slice| return slice,
        .failed => return null,
        .need_copy => {},
    }

    // Copy-Pfad ausserhalb des Guards: alloc/free setzen ihn selbst.
    const new_mem = alloc(new_size, align_value) orelse return null;
    const copy_len = if (mem.len < new_mem.len) mem.len else new_mem.len;
    @memcpy(new_mem[0..copy_len], mem[0..copy_len]);
    if (free(mem) != .ok) return null;
    return new_mem;
}

fn reallocInPlace(mem: []u8, new_size: usize, align_value: usize) InPlaceResult {
    const guard = enterHeap() orelse return .failed;
    defer leaveHeap(guard);

    const check = validateUsedBlock(mem.ptr, mem.len) orelse {
        control.invalid_free_errors += 1;
        return .failed;
    };
    switch (check.kind) {
        .used_ok => {},
        .free_block => {
            control.invalid_free_errors += 1;
            return .failed;
        },
        .size_mismatch => {
            control.size_mismatch_errors += 1;
            return .failed;
        },
    }

    if (!isAligned(@intFromPtr(mem.ptr), align_value)) return .need_copy;

    const need = blockSizeFor(new_size) orelse {
        _ = oomFailure();
        return .failed;
    };
    const off = check.offset;
    var bsize = check.size;

    if (need <= bsize) {
        if (bsize - need >= MIN_BLOCK) {
            writeUsedBlock(off, need, new_size);
            releaseBlock(off + need, bsize - need);
            releaseTrailingPages();
        } else {
            writeUsedBlock(off, bsize, new_size);
        }
        control.used_bytes = control.used_bytes - check.requested + new_size;
        const ptr: [*]u8 = @ptrFromInt(HEAP_BASE + @as(u64, off) + HEADER_SIZE);
        return .{ .done = ptr[0..new_size] };
    }

    const next_off = off + bsize;
    if (next_off < control.heap_top) {
        const next_word = word(next_off).*;
        if ((next_word & 1) == 0) {
            const next_size = blockSize(next_word);
            if (bsize + next_size >= need) {
                unlinkFree(next_off, next_size);
                bsize += next_size;
                if (bsize - need >= MIN_BLOCK) {
                    writeUsedBlock(off, need, new_size);
                    insertFree(off + need, bsize - need);
                } else {
                    writeUsedBlock(off, bsize, new_size);
                }
                control.used_bytes = control.used_bytes - check.requested + new_size;
                const ptr: [*]u8 = @ptrFromInt(HEAP_BASE + @as(u64, off) + HEADER_SIZE);
                return .{ .done = ptr[0..new_size] };
            }
        }
    }

    return .need_copy;
}

pub fn stats() Stats {
    var s: Stats = .{
        .pages = control.committed_pages,
        .capacity_bytes = committedBytes(),
        .reserved_bytes = control.cap_pages * PAGE_SIZE,
        .committed_bytes = committedBytes(),
        .used_bytes = control.used_bytes,
        .free_bytes = control.free_bytes,
        .active_blocks = control.active_blocks,
        .free_blocks = control.free_blocks,
        .allocation_errors = control.allocation_errors,
        .invalid_free_errors = control.invalid_free_errors,
        .double_free_errors = control.double_free_errors,
        .size_mismatch_errors = control.size_mismatch_errors,
        .oom_errors = control.oom_errors,
        .reentry_errors = control.reentry_errors,
    };
    s.largest_free = largestFreeBlock();
    s.fragmentation_hint = s.free_blocks > 1 and s.free_bytes > s.largest_free;
    return s;
}

pub fn metadataRange() MetadataRange {
    return .{
        .base = @intFromPtr(&control),
        .len = @sizeOf(Control),
    };
}

// Product-boot invariant: validate only the structure created by init().
// Allocation, error-injection, alignment, and churn probes belong to the
// explicit -Dboot-selftests diagnostic kernel.
pub fn bootInvariant() bool {
    if (!control.initialized or control.range_id == 0) return false;
    if (control.committed_pages < MIN_COMMITTED_PAGES or control.committed_pages > control.cap_pages) return false;
    if (control.heap_top != committedBytes() or control.heap_top < MIN_BLOCK) return false;
    if (control.active_blocks != 0 or control.used_bytes != 0) return false;
    if (control.free_blocks != 1 or control.free_bytes != control.heap_top) return false;

    const first = word(0).*;
    if ((first & 1) != 0 or blockSize(first) != control.heap_top) return false;
    if (word(8).* != FREE_MAGIC or word(control.heap_top - FOOTER_SIZE).* != first) return false;
    if (word(16).* != NONE or word(24).* != NONE) return false;
    return control.bins[binIndex(control.heap_top)] == 0;
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

fn word(offset: usize) *u64 {
    return @ptrFromInt(HEAP_BASE + @as(u64, offset));
}

fn blockSize(word0: u64) usize {
    return @intCast(word0 & ~@as(u64, GRANULE - 1));
}

fn blockSizeFor(request: usize) ?usize {
    const raw = checkedAdd(request, OVERHEAD) orelse return null;
    const rounded = alignUp(raw, GRANULE) orelse return null;
    return if (rounded < MIN_BLOCK) MIN_BLOCK else rounded;
}

fn binIndex(size: usize) usize {
    if (size < 128) return 0;
    const lg: usize = 63 - @as(usize, @clz(@as(u64, size)));
    const idx = lg - 6;
    return if (idx >= BIN_COUNT) BIN_COUNT - 1 else idx;
}

fn insertFree(offset: usize, size: usize) void {
    word(offset).* = @as(u64, size);
    word(offset + 8).* = FREE_MAGIC;
    word(offset + size - FOOTER_SIZE).* = @as(u64, size);
    const idx = binIndex(size);
    const head = control.bins[idx];
    word(offset + 16).* = head;
    word(offset + 24).* = NONE;
    if (head != NONE) word(@intCast(head + 24)).* = @as(u64, offset);
    control.bins[idx] = @as(u64, offset);
    control.free_blocks += 1;
    control.free_bytes += size;
}

fn unlinkFree(offset: usize, size: usize) void {
    // Validierung gegen Metadaten-Korruption (z.B. Use-after-free eines
    // Konsumenten, der in einen freien Block geschrieben hat): kaputte
    // Nachbar-Offsets NICHT dereferenzieren, sondern laut markieren.
    const head_word = word(offset).*;
    if ((head_word & 1) != 0 or blockSize(head_word) != size or
        word(offset + 8).* != FREE_MAGIC)
    {
        reportCorruption("unlink-header", offset);
    }
    const next = word(offset + 16).*;
    const prev = word(offset + 24).*;
    if (next != NONE and !nodeOffsetPlausible(next)) {
        reportCorruption("unlink-next", offset);
    } else if (prev != NONE and !nodeOffsetPlausible(prev)) {
        reportCorruption("unlink-prev", offset);
    } else {
        if (prev != NONE) {
            word(@intCast(prev + 16)).* = next;
        } else {
            control.bins[binIndex(size)] = next;
        }
        if (next != NONE) word(@intCast(next + 24)).* = prev;
    }
    control.free_blocks -= 1;
    control.free_bytes -= size;
}

fn nodeOffsetPlausible(offset_word: u64) bool {
    if ((offset_word & (GRANULE - 1)) != 0) return false;
    const off: usize = @intCast(offset_word);
    return off + MIN_BLOCK <= control.heap_top;
}

var corruption_reported: bool = false;
var corruption_count: u64 = 0;

fn reportCorruption(kind: []const u8, offset: usize) void {
    corruption_count += 1;
    if (corruption_reported) return;
    corruption_reported = true;
    k.puts("HEAP CORRUPT kind=");
    k.puts(kind);
    k.puts(" off=0x");
    k.putHex(@as(u64, offset), 12);
    k.puts("\r\n");
}

fn writeUsedBlock(offset: usize, size: usize, requested: usize) void {
    word(offset).* = @as(u64, size) | 1;
    word(offset + 8).* = @as(u64, requested);
    word(offset + size - FOOTER_SIZE).* = @as(u64, size) | 1;
}

// Koalesziert den (nicht mehr verketteten) Block mit freien Nachbarn und
// haengt das Ergebnis in seinen Bin.
fn releaseBlock(offset_in: usize, size_in: usize) void {
    var offset = offset_in;
    var size = size_in;

    // Poison (0xDD) ueber die freigegebene Payload: macht Use-after-free
    // deterministisch sichtbar (Crash-Adressen/Zeiger werden 0xDDDD...),
    // statt zufaellig weiterzulaufen. Kostet einen memset pro Free.
    if (size >= OVERHEAD + GRANULE) {
        const body: [*]u8 = @ptrFromInt(HEAP_BASE + @as(u64, offset) + HEADER_SIZE);
        @memset(body[0 .. size - OVERHEAD], 0xDD);
    }

    const next_off = offset + size;
    if (next_off < control.heap_top) {
        const next_word = word(next_off).*;
        if ((next_word & 1) == 0) {
            const next_size = blockSize(next_word);
            unlinkFree(next_off, next_size);
            size += next_size;
        }
    }
    if (offset > 0) {
        const prev_footer = word(offset - FOOTER_SIZE).*;
        if ((prev_footer & 1) == 0) {
            const prev_size = blockSize(prev_footer);
            const prev_off = offset - prev_size;
            unlinkFree(prev_off, prev_size);
            offset = prev_off;
            size += prev_size;
        }
    }
    insertFree(offset, size);
}

fn findFit(need: usize) ?usize {
    var idx = binIndex(need);
    // Im exakten Bin kurz suchen (Groessen dort koennen < need sein) ...
    var cursor = control.bins[idx];
    var scanned: usize = 0;
    while (cursor != NONE and scanned < FIT_SCAN_LIMIT) : (scanned += 1) {
        const off: usize = @intCast(cursor);
        const size = blockSize(word(off).*);
        if (size >= need) {
            unlinkFree(off, size);
            return off;
        }
        cursor = word(off + 16).*;
    }
    // ... in hoeheren Bins passt jeder Block garantiert: Kopf nehmen.
    idx += 1;
    while (idx < BIN_COUNT) : (idx += 1) {
        const head = control.bins[idx];
        if (head == NONE) continue;
        const off: usize = @intCast(head);
        const size = blockSize(word(off).*);
        if (size >= need) {
            unlinkFree(off, size);
            return off;
        }
        // Nur moeglich im obersten Sammel-Bin: dort linear weiter.
        var walk = word(off + 16).*;
        while (walk != NONE) {
            const woff: usize = @intCast(walk);
            const wsize = blockSize(word(woff).*);
            if (wsize >= need) {
                unlinkFree(woff, wsize);
                return woff;
            }
            walk = word(woff + 16).*;
        }
    }
    return null;
}

fn placeBlock(offset: usize, need: usize, requested: usize) ?[]u8 {
    const total = blockSize(word(offset).*);
    if (total - need >= MIN_BLOCK) {
        writeUsedBlock(offset, need, requested);
        insertFree(offset + need, total - need);
    } else {
        writeUsedBlock(offset, total, requested);
    }
    control.active_blocks += 1;
    control.used_bytes += requested;
    const ptr: [*]u8 = @ptrFromInt(HEAP_BASE + @as(u64, offset) + HEADER_SIZE);
    return ptr[0..requested];
}

fn allocAligned(requested: usize, need: usize, alignment: usize) ?[]u8 {
    if (findAlignedFit(need, alignment)) |found| {
        return placeAligned(found.offset, found.payload, need, requested);
    }
    const grow_need = need + alignment + MIN_BLOCK;
    if (!growCommitted(grow_need)) return oomFailure();
    if (findAlignedFit(need, alignment)) |found| {
        return placeAligned(found.offset, found.payload, need, requested);
    }
    return oomFailure();
}

const AlignedFit = struct {
    offset: usize,
    payload: usize,
};

fn findAlignedFit(need: usize, alignment: usize) ?AlignedFit {
    var idx = binIndex(need);
    while (idx < BIN_COUNT) : (idx += 1) {
        var cursor = control.bins[idx];
        var scanned: usize = 0;
        while (cursor != NONE and scanned < FIT_SCAN_LIMIT) : (scanned += 1) {
            const off: usize = @intCast(cursor);
            const size = blockSize(word(off).*);
            if (alignedPayloadFor(off, size, need, alignment)) |payload| {
                unlinkFree(off, size);
                return .{ .offset = off, .payload = payload };
            }
            cursor = word(off + 16).*;
        }
    }
    return null;
}

// Liefert die Payload-Adresse (als Heap-Offset), wenn der freie Block
// [off, off+size) einen ausgerichteten Block der Groesse need aufnehmen
// kann. Prefix-Reste sind entweder 0 oder >= MIN_BLOCK.
fn alignedPayloadFor(off: usize, size: usize, need: usize, alignment: usize) ?usize {
    var payload = alignUp(off + HEADER_SIZE, alignment) orelse return null;
    while (true) {
        const prefix = payload - HEADER_SIZE - off;
        if (prefix == 0 or prefix >= MIN_BLOCK) break;
        payload = checkedAdd(payload, alignment) orelse return null;
    }
    const prefix = payload - HEADER_SIZE - off;
    const total_need = checkedAdd(prefix, need) orelse return null;
    if (total_need > size) return null;
    return payload;
}

fn placeAligned(offset: usize, payload: usize, need: usize, requested: usize) ?[]u8 {
    const total = blockSize(word(offset).*);
    const prefix = payload - HEADER_SIZE - offset;
    var used_off = offset;
    var used_size = total;
    if (prefix != 0) {
        insertFree(offset, prefix);
        used_off = offset + prefix;
        used_size = total - prefix;
    }
    if (used_size - need >= MIN_BLOCK) {
        writeUsedBlock(used_off, need, requested);
        insertFree(used_off + need, used_size - need);
    } else {
        writeUsedBlock(used_off, used_size, requested);
    }
    control.active_blocks += 1;
    control.used_bytes += requested;
    const ptr: [*]u8 = @ptrFromInt(HEAP_BASE + @as(u64, used_off) + HEADER_SIZE);
    return ptr[0..requested];
}

fn growCommitted(min_extra_bytes: usize) bool {
    const extra_pages_min = pagesForBytes(min_extra_bytes);
    if (extra_pages_min == 0) return true;
    if (control.committed_pages + extra_pages_min > control.cap_pages) return false;

    const start = committedBytes();
    const extra_bytes = extra_pages_min * PAGE_SIZE;
    virt.commit(control.range_id, @intCast(start), @intCast(extra_bytes)) catch return false;
    control.committed_pages += extra_pages_min;
    control.heap_top = committedBytes();

    // Neues Freistueck mit einem freien Vorgaenger am alten Heap-Ende
    // verschmelzen, damit die Kein-Nachbar-frei-Invariante haelt.
    var offset = start;
    var size = extra_bytes;
    if (offset > 0) {
        const prev_footer = word(offset - FOOTER_SIZE).*;
        if ((prev_footer & 1) == 0) {
            const prev_size = blockSize(prev_footer);
            const prev_off = offset - prev_size;
            unlinkFree(prev_off, prev_size);
            offset = prev_off;
            size += prev_size;
        }
    }
    insertFree(offset, size);
    return true;
}

fn releaseTrailingPages() void {
    const keep_min = keepMinBytes();
    if (control.heap_top <= keep_min) return;
    const last_footer = word(control.heap_top - FOOTER_SIZE).*;
    if ((last_footer & 1) != 0) return;
    const last_size = blockSize(last_footer);
    const last_start = control.heap_top - last_size;

    var uncommit_start = alignUp(last_start, PAGE_SIZE) orelse return;
    var remainder = uncommit_start - last_start;
    if (remainder != 0 and remainder < MIN_BLOCK) {
        uncommit_start += PAGE_SIZE;
        remainder += PAGE_SIZE;
    }
    if (uncommit_start < keep_min) {
        uncommit_start = keep_min;
        remainder = uncommit_start - last_start;
        if (remainder != 0 and remainder < MIN_BLOCK) {
            uncommit_start += PAGE_SIZE;
            remainder += PAGE_SIZE;
        }
    }
    if (uncommit_start >= control.heap_top) return;

    unlinkFree(last_start, last_size);
    const bytes = control.heap_top - uncommit_start;
    virt.uncommit(control.range_id, @intCast(uncommit_start), @intCast(bytes)) catch {
        insertFree(last_start, last_size);
        return;
    };
    control.committed_pages = uncommit_start / PAGE_SIZE;
    control.heap_top = uncommit_start;
    if (remainder != 0) insertFree(last_start, remainder);
}

fn keepMinBytes() usize {
    const keep = RELEASE_KEEP_PAGES * PAGE_SIZE;
    const floor = MIN_COMMITTED_PAGES * PAGE_SIZE;
    return if (keep < floor) floor else keep;
}

fn largestFreeBlock() usize {
    var idx: usize = BIN_COUNT;
    while (idx > 0) {
        idx -= 1;
        var cursor = control.bins[idx];
        if (cursor == NONE) continue;
        var best: usize = 0;
        while (cursor != NONE) {
            const off: usize = @intCast(cursor);
            const size = blockSize(word(off).*);
            if (size > best) best = size;
            cursor = word(off + 16).*;
        }
        return best;
    }
    return 0;
}

// ---------------------------------------------------------------------------
// Validierung eingehender Pointer (free/realloc)
// ---------------------------------------------------------------------------

const ValidationKind = enum {
    used_ok,
    free_block,
    size_mismatch,
};

const Validation = struct {
    kind: ValidationKind,
    offset: usize = 0,
    size: usize = 0,
    requested: usize = 0,
};

fn validateUsedBlock(ptr: [*]u8, len: usize) ?Validation {
    const addr: u64 = @intFromPtr(ptr);
    if (addr < HEAP_BASE + HEADER_SIZE) return null;
    const payload_off: usize = @intCast(addr - HEAP_BASE);
    if (payload_off >= control.heap_top) return null;
    if ((payload_off & (GRANULE - 1)) != 0) return null;
    const offset = payload_off - HEADER_SIZE;

    const word0 = word(offset).*;
    const size = blockSize(word0);
    const header_plausible = size >= MIN_BLOCK and size <= control.heap_top and
        offset + size <= control.heap_top and
        word(offset + size - FOOTER_SIZE).* == word0;
    if (!header_plausible) return classifyByWalk(offset);

    if ((word0 & 1) == 0) {
        if (word(offset + 8).* == FREE_MAGIC) {
            return .{ .kind = .free_block, .offset = offset, .size = size };
        }
        return classifyByWalk(offset);
    }

    const requested: usize = @intCast(word(offset + 8).*);
    if (requested > size - OVERHEAD) return classifyByWalk(offset);
    if (requested != len) {
        return .{ .kind = .size_mismatch, .offset = offset, .size = size, .requested = requested };
    }
    return .{ .kind = .used_ok, .offset = offset, .size = size, .requested = requested };
}

// Langsamer Fallback fuer die Fehlerpfade: physische Blockkette von vorn
// ablaufen und den Block bestimmen, der das Ziel enthaelt. Liegt das Ziel
// in einem FREIEN Block (z.B. weil der ehemals eigene Block beim Free mit
// Nachbarn koalesziert wurde), ist es ein Double-Free; sonst ungueltig.
fn classifyByWalk(target: usize) ?Validation {
    var off: usize = 0;
    while (off < control.heap_top) {
        const w0 = word(off).*;
        const size = blockSize(w0);
        if (size < MIN_BLOCK) return null;
        if (off + size > control.heap_top) return null;
        if (target < off + size) {
            if ((w0 & 1) == 0) {
                return .{ .kind = .free_block, .offset = off, .size = size };
            }
            return null;
        }
        off += size;
    }
    return null;
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
    if (!control.initialized) return selfTestFail("not-initialized");

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
    const df_before = control.double_free_errors;
    const probe = alloc(64, 1) orelse return selfTestFail("alloc-df");
    const probe_copy = probe;
    if (free(probe) != .ok) return selfTestFail("free-df1");
    if (free(probe_copy) == .ok) return selfTestFail("double-free-accepted");
    if (control.double_free_errors != df_before + 1) return selfTestFail("double-free-count");

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

const HeapGuard = struct {
    unwind: task_context.UnwindToken,
};

fn enterHeap() ?HeapGuard {
    const unwind = task_context.enterUnwind();
    if (!unwind.admitted()) {
        control.reentry_errors += 1;
        return null;
    }
    if (control.in_heap) {
        control.reentry_errors += 1;
        if (!control.reentry_reported) {
            control.reentry_reported = true;
            k.puts("HEAP REENTRY detected (non-preempt invariant violated)\r\n");
        }
        _ = task_context.leaveUnwind(unwind);
        return null;
    }
    control.in_heap = true;
    return .{ .unwind = unwind };
}

fn leaveHeap(guard: HeapGuard) void {
    control.in_heap = false;
    _ = task_context.leaveUnwind(guard.unwind);
}

fn committedBytes() usize {
    return control.committed_pages * PAGE_SIZE;
}

fn pagesForBytes(bytes: usize) usize {
    return (bytes + PAGE_SIZE - 1) / PAGE_SIZE;
}

fn normalizeAlignment(alignment: usize) ?usize {
    const value = if (alignment == 0) 1 else alignment;
    if (!isPowerOfTwo(value)) return null;
    return value;
}

fn checkedAdd(a: usize, b: usize) ?usize {
    const result = a +% b;
    if (result < a) return null;
    return result;
}

fn alignUp(value: usize, alignment: usize) ?usize {
    const add = alignment - 1;
    const sum = checkedAdd(value, add) orelse return null;
    return sum & ~add;
}

fn isAligned(value: usize, alignment: usize) bool {
    return (value & (alignment - 1)) == 0;
}

fn isPowerOfTwo(value: usize) bool {
    return value != 0 and (value & (value - 1)) == 0;
}

fn allocFailure() ?[]u8 {
    control.allocation_errors += 1;
    return null;
}

fn oomFailure() ?[]u8 {
    control.allocation_errors += 1;
    control.oom_errors += 1;
    return null;
}

fn recordInvalidFree() FreeResult {
    control.invalid_free_errors += 1;
    return .invalid_pointer;
}

fn recordDoubleFree() FreeResult {
    control.double_free_errors += 1;
    return .double_free;
}

fn recordSizeMismatch() FreeResult {
    control.size_mismatch_errors += 1;
    return .size_mismatch;
}
