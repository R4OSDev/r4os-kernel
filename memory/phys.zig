const boot_info = @import("../bootloader/boot_info.zig");
const blocks = @import("blocks.zig");
const mem_map = @import("map.zig");
const k = @import("../kernel/log.zig");

pub const FRAME_SIZE: u64 = 4096;

pub const Stats = struct {
    total_frames: u64 = 0,
    free_frames: u64 = 0,
    used_frames: u64 = 0,
    bitmap_bytes: u64 = 0,
    bitmap_base: u64 = 0,
    double_free_errors: u64 = 0,
    bad_free_errors: u64 = 0,
};

// 0.56.9: PMM-Wachhunde. Ein Double-Free markiert einen IN BENUTZUNG
// befindlichen Frame als frei -> die naechste Vergabe teilt ihn doppelt
// aus und der zweite Besitzer zerschreibt via HHDM fremden RAM (Top-
// Kandidat fuer die rip=0-Crashklasse). Vorher wurden Double-Frees hier
// STUMM geschluckt; jetzt zaehlen sie und melden sich einmalig auf COM1
// (der Gate-serial-markers-Subtest greift die Marker auf).
var double_free_errors: u64 = 0;
var bad_free_errors: u64 = 0;
var pmm_fault_reported: bool = false;

fn reportPmmFault(kind: []const u8, addr: u64) void {
    if (pmm_fault_reported) return;
    pmm_fault_reported = true;
    k.puts("PMM FAULT ");
    k.puts(kind);
    k.puts(" frame=0x");
    k.putHex(addr, 16);
    k.puts("\r\n");
}

var bitmap: [*]u8 = undefined;
var bitmap_bytes: u64 = 0;
var total_frames: u64 = 0;
var free_frames: u64 = 0;
var next_hint: u64 = 0;
var hhdm_offset: u64 = 0;
var initialized = false;

pub fn init() bool {
    const entries = boot_info.memoryMap();
    if (entries.len == 0) return false;
    hhdm_offset = boot_info.hhdmOffset() orelse return false;
    const summary = mem_map.summarize();
    if (summary.largest_usable_len == 0) return false;

    const max_addr = highestUsableAddress(entries);
    total_frames = (alignUpChecked(max_addr, FRAME_SIZE) orelse return false) / FRAME_SIZE;
    bitmap_bytes = (alignUpChecked(total_frames, 8) orelse return false) / 8;

    const bitmap_phys = summary.largest_usable_base;
    const bitmap_end = checkedAdd(bitmap_phys, bitmap_bytes) orelse return false;
    const largest_end = checkedAdd(summary.largest_usable_base, summary.largest_usable_len) orelse return false;
    if (bitmap_end > largest_end) return false;

    bitmap = @ptrFromInt(physToVirt(bitmap_phys));
    @memset(bitmap[0..@intCast(bitmap_bytes)], 0xFF);
    free_frames = 0;
    next_hint = 0;

    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const entry = entries[i];
        if (entry.kind == .usable) {
            markRange(entry.usable_base, entry.usable_len, false);
        }
    }

    markRange(0, FRAME_SIZE, true);
    markRange(bitmap_phys, bitmap_bytes, true);
    const bitmap_claim_base = alignDown(bitmap_phys, FRAME_SIZE);
    const bitmap_claim_end = alignUpChecked(bitmap_end, FRAME_SIZE) orelse return false;
    _ = blocks.claimPhysicalRange(bitmap_claim_base, bitmap_claim_end - bitmap_claim_base, .kernel, .kernel, 0, "pmm-bitmap") catch return false;
    initialized = true;
    return true;
}

// 0.56.12: Bitmap wortweise als u64 lesen, wenn die Basis 8-Byte-
// ausgerichtet ist (HHDM ist seitenausgerichtet, in der Praxis immer der
// Fall). Volle Woerter (0xFF..) werden mit EINEM Vergleich uebersprungen
// statt 64 Einzelbit-Tests - der Hotpath allocFrame skaliert dann mit der
// Zahl der belegten WOERTER, nicht der Bits.
fn bitmapWords() ?[*]u64 {
    if ((@intFromPtr(bitmap) & 7) != 0) return null;
    return @ptrCast(@alignCast(bitmap));
}

fn wordCount() u64 {
    return (bitmap_bytes + 7) / 8;
}

pub fn allocFrame() ?u64 {
    if (!initialized) return null;
    const words = bitmapWords() orelse return allocFrameLinear();

    const wc = wordCount();
    var scanned: u64 = 0;
    var wi = next_hint / 64;
    var start_bit: u6 = @intCast(next_hint % 64);
    while (scanned <= wc) : (scanned += 1) {
        if (wi >= wc) {
            wi = 0;
            start_bit = 0;
        }
        var word = words[wi];
        // Bits unterhalb des Starthinweises als belegt maskieren.
        if (start_bit != 0) word |= (@as(u64, 1) << start_bit) - 1;
        if (word != ~@as(u64, 0)) {
            const bit: u6 = @intCast(@ctz(~word));
            const frame = wi * 64 + bit;
            if (frame == 0) {
                // Frame 0 ist reserviert: maskieren, dasselbe Wort erneut.
                start_bit = 1;
                continue;
            }
            if (frame < total_frames) {
                words[wi] |= @as(u64, 1) << bit;
                free_frames -= 1;
                next_hint = frame + 1;
                return frame * FRAME_SIZE;
            }
            // frame >= total_frames: Tail-Padding-Bits, naechstes Wort.
        }
        wi += 1;
        start_bit = 0;
    }
    return null;
}

fn allocFrameLinear() ?u64 {
    var scanned: u64 = 0;
    var frame = next_hint;
    while (scanned < total_frames) : (scanned += 1) {
        if (frame >= total_frames) frame = 0;
        if (frame == 0) {
            frame = 1;
            continue;
        }
        if (!isUsed(frame)) {
            setUsed(frame, true);
            free_frames -= 1;
            next_hint = frame + 1;
            return frame * FRAME_SIZE;
        }
        frame += 1;
    }
    return null;
}

pub fn allocContiguousFrames(count: u64) ?u64 {
    if (!initialized or count == 0) return null;
    const words = bitmapWords();
    var start: u64 = 1;
    while (start + count <= total_frames) : (start += 1) {
        // 0.56.12: volle Woerter beim Fehlschlag ueberspringen.
        if (words) |w| {
            if (isUsed(start) and (start & 63) == 0 and w[start / 64] == ~@as(u64, 0)) {
                start += 63;
                continue;
            }
        }
        var offset: u64 = 0;
        while (offset < count and !isUsed(start + offset)) : (offset += 1) {}
        if (offset == count) {
            var mark: u64 = 0;
            while (mark < count) : (mark += 1) setUsed(start + mark, true);
            free_frames -= count;
            next_hint = start + count;
            return start * FRAME_SIZE;
        }
        start += offset;
    }
    return null;
}

pub fn freeContiguousFrames(addr: u64, count: u64) void {
    if (!initialized or addr % FRAME_SIZE != 0 or count == 0) return;
    const start = addr / FRAME_SIZE;
    if (start + count > total_frames) return;
    var offset: u64 = 0;
    while (offset < count) : (offset += 1) {
        const frame = start + offset;
        if (!isUsed(frame)) continue;
        setUsed(frame, false);
        free_frames += 1;
    }
    if (start < next_hint) next_hint = start;
}

pub fn freeFrame(addr: u64) void {
    if (!initialized) return;
    if (addr % FRAME_SIZE != 0) {
        bad_free_errors +%= 1;
        reportPmmFault("BAD-FREE-ALIGN", addr);
        return;
    }
    const frame = addr / FRAME_SIZE;
    if (frame >= total_frames) {
        bad_free_errors +%= 1;
        reportPmmFault("BAD-FREE-RANGE", addr);
        return;
    }
    if (!isUsed(frame)) {
        double_free_errors +%= 1;
        reportPmmFault("DOUBLE-FREE", addr);
        return;
    }
    setUsed(frame, false);
    free_frames += 1;
    if (frame < next_hint) next_hint = frame;
}

pub fn stats() Stats {
    return .{
        .total_frames = total_frames,
        .free_frames = free_frames,
        .used_frames = total_frames - free_frames,
        .bitmap_bytes = bitmap_bytes,
        .bitmap_base = if (initialized) virtToPhys(@intFromPtr(bitmap)) else 0,
        .double_free_errors = double_free_errors,
        .bad_free_errors = bad_free_errors,
    };
}

pub fn dumpStats() void {
    const s = stats();
    k.puts("  Physical frames: total=");
    k.putDec(s.total_frames);
    k.puts(" free=");
    k.putDec(s.free_frames);
    k.puts(" used=");
    k.putDec(s.used_frames);
    k.puts("\r\n");
    k.puts("  PMM bitmap: base=0x");
    k.putHex(s.bitmap_base, 16);
    k.puts(" bytes=");
    k.putDec(s.bitmap_bytes);
    k.puts("\r\n");
}

fn highestUsableAddress(entries: []const boot_info.MemoryMapEntry) u64 {
    var high: u64 = 0;
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const entry = entries[i];
        if (entry.kind != .usable or entry.usable_len == 0) continue;
        const end = entry.usable_base + entry.usable_len;
        if (end > high) high = end;
    }
    return high;
}

pub fn physToVirt(addr: u64) u64 {
    return checkedAdd(addr, hhdm_offset) orelse 0;
}

pub fn virtToPhys(addr: u64) u64 {
    if (addr < hhdm_offset) return 0;
    return addr - hhdm_offset;
}

fn markRange(base: u64, len: u64, used: bool) void {
    if (len == 0) return;
    const range_end = checkedAdd(base, len) orelse return;
    const start_addr = if (used) alignDown(base, FRAME_SIZE) else (alignUpChecked(base, FRAME_SIZE) orelse return);
    const end_addr = if (used) (alignUpChecked(range_end, FRAME_SIZE) orelse return) else alignDown(range_end, FRAME_SIZE);
    const start = start_addr / FRAME_SIZE;
    const end = end_addr / FRAME_SIZE;
    var frame = start;
    while (frame < end) : (frame += 1) {
        if (frame >= total_frames) break;
        const was_used = isUsed(frame);
        if (was_used != used) {
            setUsed(frame, used);
            if (used) {
                free_frames -= 1;
            } else {
                free_frames += 1;
            }
        }
    }
}

fn isUsed(frame: u64) bool {
    const byte_index: usize = @intCast(frame / 8);
    const bit: u3 = @intCast(frame % 8);
    return (bitmap[byte_index] & (@as(u8, 1) << bit)) != 0;
}

fn setUsed(frame: u64, used: bool) void {
    const byte_index: usize = @intCast(frame / 8);
    const bit: u3 = @intCast(frame % 8);
    if (used) {
        bitmap[byte_index] |= @as(u8, 1) << bit;
    } else {
        bitmap[byte_index] &= ~(@as(u8, 1) << bit);
    }
}

fn alignDown(value: u64, alignment: u64) u64 {
    return value & ~(alignment - 1);
}

fn alignUpChecked(value: u64, alignment: u64) ?u64 {
    const adjusted = checkedAdd(value, alignment - 1) orelse return null;
    return alignDown(adjusted, alignment);
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    const result = a +% b;
    if (result < a) return null;
    return result;
}
