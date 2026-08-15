// Direct-to-framebuffer diagnostic overlay (0.60.20).
//
// The normal kernel console sink is detached from presentation after the
// desktop takes the framebuffer (boot_status.releaseForUserSession ->
// invalidateConsoleBackingForExternalDisplay).  A runtime watchdog that
// only calls k.puts therefore paints into a shadow the frozen desktop
// never presents -- invisible on real hardware (proven on the Lenovo
// SSH-exec freeze: three console-based probes, all silent).
//
// This module retains text first, independently of framebuffer availability,
// then draws it STRAIGHT into the BootInfo framebuffer when possible, exactly
// like the crash screen.  A wedged system that no longer presents therefore
// still shows the diagnosis, while a later crash can replay the same evidence.
// It keeps its own line cursor inside a bounded top band.  The first/root-cause
// lines are never destroyed by wrapping: once the band is full, later visible
// lines are dropped until a new incident begins. It touches no lock/storage.

const boot_info = @import("../bootloader/boot_info.zig");
const fb = @import("../display/framebuffer.zig");
const font = @import("font.zig");

const BG: u32 = 0x101830; // dark blue band, distinct from a desktop
const FG: u32 = 0xF0F0F0;
const SCALE: u32 = 1;
const MAX_ROWS: u32 = 16;
const INCIDENT_LOG_BYTES: usize = 4096;

pub const IncidentToken = struct {
    generation: u64 = 0,

    pub fn valid(self: IncidentToken) bool {
        return self.generation != 0;
    }
};

var started = false;
var row: u32 = 0;
var col: u32 = 0;
var cols: u32 = 0;
var rows: u32 = 0;
var fb_storage: fb.Framebuffer = undefined;
var fb_ready = false;
var incident_active = false;
var visible_incident_active = false;
var saturated = false;
var next_incident_generation: u64 = 0;
var active_incident_generation: u64 = 0;
var resolvable_incident_generation: u64 = 0;
var retained_evidence: [INCIDENT_LOG_BYTES]u8 = .{0} ** INCIDENT_LOG_BYTES;
var retained_evidence_len: usize = 0;

fn resetRetainedEvidence() void {
    retained_evidence_len = 0;
}

fn captureByte(ch: u8) void {
    if (retained_evidence_len >= retained_evidence.len) return;
    retained_evidence[retained_evidence_len] = if (ch == '\n' or ch == '\r' or (ch >= 0x20 and ch <= 0x7e))
        ch
    else
        '?';
    retained_evidence_len += 1;
}

fn captureText(text: []const u8) void {
    for (text) |ch| {
        if (ch == '\r') continue;
        captureByte(ch);
    }
}

/// First/root-cause diagnostic text retained independently of framebuffer
/// contents.  The fatal screen and COM1 crash mirror use this after their
/// own full-screen redraw would otherwise destroy the xHCI/BOT evidence.
pub fn incidentText() []const u8 {
    return retained_evidence[0..retained_evidence_len];
}

fn framebuffer() ?*fb.Framebuffer {
    if (!fb_ready) {
        const src = boot_info.framebuffer() orelse return null;
        fb_storage = .{
            .address = src.address,
            .width = src.width,
            .height = src.height,
            .pitch = src.pitch,
            .bpp = src.bpp,
            .memory_model = src.memory_model,
            .red_mask_size = src.red_mask_size,
            .red_mask_shift = src.red_mask_shift,
            .green_mask_size = src.green_mask_size,
            .green_mask_shift = src.green_mask_shift,
            .blue_mask_size = src.blue_mask_size,
            .blue_mask_shift = src.blue_mask_shift,
            .unused = src.unused,
            .edid_size = src.edid_size,
            .edid = src.edid,
        };
        if (!fb.supportsRgb32(&fb_storage)) return null;
        fb_ready = true;
    }
    return &fb_storage;
}

fn ensureStarted(f: *fb.Framebuffer) void {
    if (started) return;
    started = true;
    const cell_w = @as(u64, font.GLYPH_W) * SCALE;
    const cell_h = @as(u64, font.GLYPH_H) * SCALE;
    cols = if (cell_w == 0) 0 else @intCast(f.width / cell_w);
    rows = if (cell_h == 0) 0 else @min(@as(u32, @intCast(f.height / cell_h)), MAX_ROWS);
    row = 0;
    col = 0;
    saturated = false;
}

fn clearBand(f: *fb.Framebuffer) void {
    if (rows == 0) return;
    const cell_h = @as(u64, font.GLYPH_H) * SCALE;
    fb.rect(f, 0, 0, f.width, @as(u64, rows) * cell_h, BG);
}

fn prepareWrite(f: *fb.Framebuffer) bool {
    ensureStarted(f);
    if (cols == 0 or rows == 0) return false;
    if (!visible_incident_active) {
        clearBand(f);
        visible_incident_active = true;
        row = 0;
        col = 0;
        saturated = false;
    }
    return !saturated;
}

fn drawGlyph(f: *fb.Framebuffer, cell_x: u32, cell_y: u32, ch: u8) void {
    const g = font.glyph(ch);
    const px0 = @as(u64, cell_x) * font.GLYPH_W * SCALE;
    const py0 = @as(u64, cell_y) * font.GLYPH_H * SCALE;
    const fg = fb.packRgb(f, FG);
    const bg = fb.packRgb(f, BG);
    var gy: u32 = 0;
    while (gy < font.GLYPH_H) : (gy += 1) {
        const bits = g[gy];
        var gx: u32 = 0;
        while (gx < font.GLYPH_W) : (gx += 1) {
            const shift: u3 = @intCast(font.GLYPH_W - 1 - gx);
            const color = if (((bits >> shift) & 1) != 0) fg else bg;
            var sy: u32 = 0;
            while (sy < SCALE) : (sy += 1) {
                var sx: u32 = 0;
                while (sx < SCALE) : (sx += 1) {
                    fb.putPacked32(f, px0 + gx * SCALE + sx, py0 + gy * SCALE + sy, color);
                }
            }
        }
    }
}

fn newline() bool {
    col = 0;
    if (rows == 0 or saturated) return false;
    if (row + 1 >= rows) {
        saturated = true;
        return false;
    }
    row += 1;
    return true;
}

fn startIncident(resolvable: bool) IncidentToken {
    if (incident_active) return .{};
    next_incident_generation +%= 1;
    if (next_incident_generation == 0) next_incident_generation = 1;
    incident_active = true;
    active_incident_generation = next_incident_generation;
    resolvable_incident_generation = if (resolvable) active_incident_generation else 0;
    resetRetainedEvidence();
    visible_incident_active = false;
    saturated = false;
    row = 0;
    col = 0;
    return if (resolvable)
        .{ .generation = active_incident_generation }
    else
        .{};
}

fn ensureIncident() void {
    if (!incident_active) _ = startIncident(false);
}

fn prepareIncidentOverlay() void {
    const f = framebuffer() orelse return;
    _ = prepareWrite(f);
}

/// Starts a diagnostic incident even when no framebuffer is available. The
/// first caller owns the retained evidence; later reporters append without
/// erasing it.
pub fn beginIncident() void {
    ensureIncident();
    // A fatal/deadman owner has no successful completion path. If it joins
    // an earlier recoverable incident, seal that generation against resolve.
    resolvable_incident_generation = 0;
    prepareIncidentOverlay();
}

/// Starts an incident which its owner may later resolve. If another incident
/// is already active, the returned token is invalid so an unrelated recovery
/// cannot clear the existing root cause.
pub fn beginResolvableIncident() IncidentToken {
    const token = startIncident(true);
    prepareIncidentOverlay();
    return token;
}

/// Resolves only the exact still-active incident. Retained evidence remains
/// available to a later crash screen; the next incident replaces it.
pub fn resolveIncident(token: IncidentToken) bool {
    if (!token.valid() or
        !incident_active or
        token.generation != active_incident_generation or
        token.generation != resolvable_incident_generation)
        return false;
    incident_active = false;
    active_incident_generation = 0;
    resolvable_incident_generation = 0;
    visible_incident_active = false;
    saturated = false;
    row = 0;
    col = 0;
    return true;
}

/// The desktop (or another external presenter) replaced the framebuffer
/// contents.  The next diagnostic write must therefore start with a fresh
/// band even if an early-boot incident initialized this module already.
/// Presentation handoff invalidates only the pixels.  It must not close the
/// reporter's live generation: a delayed completion holding that exact token
/// must still be able to resolve it, and a stale token must never manufacture
/// a new unresolvable incident on its next write.
pub fn resetForExternalDisplay() void {
    visible_incident_active = false;
    saturated = false;
    row = 0;
    col = 0;
}

/// Writes one line and advances the cursor.  Non-printable bytes render as
/// '?'.  Safe to call before any explicit init.
pub fn line(text: []const u8) void {
    write(text);
    endLine();
}

/// Appends a decimal number to the current line without a newline.
pub fn writeDec(value: u64) void {
    var buf: [20]u8 = undefined;
    var n = value;
    var i: usize = buf.len;
    if (n == 0) {
        buf[buf.len - 1] = '0';
        i = buf.len - 1;
    } else {
        while (n > 0) {
            i -= 1;
            buf[i] = @intCast('0' + (n % 10));
            n /= 10;
        }
    }
    write(buf[i..]);
}

/// Appends an unsigned hexadecimal number with a 0x prefix.
pub fn writeHex(value: u64) void {
    write("0x");
    const digits = "0123456789ABCDEF";
    var buf: [16]u8 = undefined;
    var n = value;
    var i: usize = buf.len;
    if (n == 0) {
        buf[buf.len - 1] = '0';
        i = buf.len - 1;
    } else {
        while (n != 0) {
            i -= 1;
            buf[i] = digits[@intCast(n & 0xF)];
            n >>= 4;
        }
    }
    write(buf[i..]);
}

/// Appends text to the current line without a trailing newline.
pub fn write(text: []const u8) void {
    ensureIncident();
    captureText(text);
    const f = framebuffer() orelse return;
    if (!prepareWrite(f)) return;
    for (text) |ch| {
        if (ch == '\n' or ch == '\r') continue;
        if (col >= cols and !newline()) return;
        drawGlyph(f, col, row, if (ch >= 0x20 and ch <= 0x7e) ch else '?');
        col += 1;
    }
}

/// Ends the current line.
pub fn endLine() void {
    ensureIncident();
    captureByte('\n');
    const f = framebuffer() orelse return;
    if (!prepareWrite(f)) return;
    _ = newline();
}
