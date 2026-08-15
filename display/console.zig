// Text console on the framebuffer.
//
// Tracks cursor position, foreground/background color, and scrolling.

const fb = @import("../display/framebuffer.zig");
const font = @import("../kernel/font.zig");
const scroll_buffer = @import("console_scroll_buffer.zig");

// Ein einzelner frueher Framebuffer-ConsoleSink ist aktiv. Das begrenzte
// Zellraster liegt deshalb statisch im Kernel statt als grosse Stackstruktur.
// 65.536 Zellen decken den Default-2x-Textmodus bis einschliesslich 5K ab;
// groessere/ungewoehnliche Modi behalten den ausgerichteten FB-Fallback.
var backing_cells: [scroll_buffer.MAX_CELLS]scroll_buffer.Cell = undefined;

pub const Console = struct {
    framebuffer: *fb.Framebuffer,
    cols: u32,
    rows: u32,
    cur_x: u32 = 0,
    cur_y: u32 = 0,
    fg: u32 = 0xE0E0E0,
    bg: u32 = 0x001830,
    scale: u32 = 2, // 2x-Pixel-Skalierung -> 16x16 effektiv
    margin_left: u32 = 0,
    margin_top: u32 = 0,
    margin_right: u32 = 0,
    margin_bottom: u32 = 0,
    backing_cols: u32 = 0,
    backing_rows: u32 = 0,
    backing_valid: bool = false,

    pub fn init(framebuffer: *fb.Framebuffer) Console {
        const scale: u32 = 2;
        const gw: u32 = font.glyphWidth() * scale;
        const gh: u32 = font.glyphHeight() * scale;
        const cols: u32 = if (fb.supportsRgb32(framebuffer)) @intCast(framebuffer.width / gw) else 0;
        const rows: u32 = if (fb.supportsRgb32(framebuffer)) @intCast(framebuffer.height / gh) else 0;
        return .{
            .framebuffer = framebuffer,
            .cols = cols,
            .rows = rows,
            .scale = scale,
        };
    }

    pub fn setFontScale(self: *Console, scale: u32) void {
        self.scale = if (scale == 0) 1 else scale;
        self.recalculateGeometry();
        // Die vorhandenen Pixel besitzen noch die alte Zellgeometrie. Bis
        // zum naechsten expliziten clear bleibt deshalb nur der FB-Fallback.
        self.invalidateBacking();
    }

    pub fn setColors(self: *Console, fg: u32, bg: u32) void {
        self.fg = fg;
        self.bg = bg;
    }

    pub fn setMargins(self: *Console, left: u32, top: u32, right: u32, bottom: u32) void {
        self.margin_left = if (left < self.cols) left else 0;
        self.margin_top = if (top < self.rows) top else 0;
        self.margin_right = if (right < self.cols - self.margin_left) right else 0;
        self.margin_bottom = if (bottom < self.rows - self.margin_top) bottom else 0;
        self.cur_x = self.margin_left;
        self.cur_y = self.margin_top;
    }

    pub fn textCols(self: *const Console) u32 {
        if (self.cols <= self.margin_left + self.margin_right) return self.cols;
        return self.cols - self.margin_left - self.margin_right;
    }

    pub fn textRows(self: *const Console) u32 {
        if (self.rows <= self.margin_top + self.margin_bottom) return self.rows;
        return self.rows - self.margin_top - self.margin_bottom;
    }

    pub fn setCursor(self: *Console, x: u32, y: u32) void {
        const max_x = if (self.textCols() == 0) 0 else self.margin_left + self.textCols() - 1;
        const max_y = if (self.textRows() == 0) 0 else self.margin_top + self.textRows() - 1;
        self.cur_x = if (x <= max_x) x else max_x;
        self.cur_y = if (y <= max_y) y else max_y;
    }

    pub fn clear(self: *Console) void {
        if (!self.isUsable()) return;
        fb.fill(self.framebuffer, self.bg);
        self.cur_x = self.margin_left;
        self.cur_y = self.margin_top;
        self.resetBacking(self.bg, self.bg, false);
    }

    pub fn clearFramed(self: *Console, border: u32, inner: u32) void {
        if (!self.isUsable()) return;
        fb.fill(self.framebuffer, border);
        self.bg = inner;
        const cell_w = font.glyphWidth() * self.scale;
        const cell_h = font.glyphHeight() * self.scale;
        const x = self.margin_left * cell_w;
        const y = self.margin_top * cell_h;
        const w = self.textCols() * cell_w;
        const h = (self.rows - self.margin_top - self.margin_bottom) * cell_h;
        fb.rect(self.framebuffer, x, y, w, h, inner);
        self.cur_x = self.margin_left;
        self.cur_y = self.margin_top;
        self.resetBacking(border, inner, true);
    }

    pub fn invalidateBackingForExternalDisplay(self: *Console) void {
        // Splash, Desktop and crash renderers write outside this Console and
        // therefore make its logical cell image non-authoritative.
        self.invalidateBacking();
    }

    pub fn putc(self: *Console, c: u8) void {
        if (!self.isUsable()) return;
        switch (c) {
            '\n' => self.newline(),
            '\r' => self.cur_x = self.margin_left,
            0x08 => self.backspace(),
            '\t' => {
                const next = (self.cur_x + 4) & ~@as(u32, 3);
                while (self.cur_x < next and self.cur_x < self.textRight()) {
                    self.drawGlyphAtCursor(' ');
                    self.cur_x += 1;
                }
            },
            else => {
                if (self.cur_x >= self.textRight()) self.newline();
                self.drawGlyphAtCursor(c);
                self.cur_x += 1;
            },
        }
    }

    pub fn puts(self: *Console, s: []const u8) void {
        for (s) |c| self.putc(c);
    }

    pub fn putHex(self: *Console, value: u64, digits: u8) void {
        const hex = "0123456789ABCDEF";
        var i: u8 = digits;
        while (i > 0) {
            i -= 1;
            const shift: u6 = @intCast(@as(u32, i) * 4);
            const nibble: u4 = @truncate(value >> shift);
            self.putc(hex[nibble]);
        }
    }

    pub fn putDec(self: *Console, value: u64) void {
        if (value == 0) {
            self.putc('0');
            return;
        }
        var buf: [20]u8 = undefined;
        var n = value;
        var i: usize = buf.len;
        while (n > 0) {
            i -= 1;
            buf[i] = @intCast('0' + (n % 10));
            n /= 10;
        }
        self.puts(buf[i..]);
    }

    fn newline(self: *Console) void {
        if (!self.isUsable()) return;
        self.cur_x = self.margin_left;
        self.cur_y += 1;
        if (self.cur_y >= self.textBottom()) {
            self.scroll();
            self.cur_y = self.textBottom() - 1;
        }
    }

    fn backspace(self: *Console) void {
        if (!self.isUsable() or self.cur_x <= self.margin_left) return;
        self.cur_x -= 1;
        self.drawGlyphAtCursor(' ');
    }

    fn drawGlyphAtCursor(self: *Console, c: u8) void {
        const glyph_w = font.glyphWidth();
        const glyph_h = font.glyphHeight();
        const px = self.cur_x * glyph_w * self.scale;
        const py = self.cur_y * glyph_h * self.scale;
        const fg = fb.packRgb(self.framebuffer, self.fg);
        const bg = fb.packRgb(self.framebuffer, self.bg);
        self.rememberCell(self.cur_x, self.cur_y, c, fg, bg);
        var row: u32 = 0;
        while (row < glyph_h) : (row += 1) {
            const bits = font.glyphRow(c, row);
            var col: u32 = 0;
            while (col < glyph_w) : (col += 1) {
                const shift: u3 = @intCast(glyph_w - 1 - col);
                const on = (bits >> shift) & 1 == 1;
                const color = if (on) fg else bg;
                // Skalieren: 'scale x scale' Pixel-Block pro Font-Pixel.
                var sy: u32 = 0;
                while (sy < self.scale) : (sy += 1) {
                    var sx: u32 = 0;
                    while (sx < self.scale) : (sx += 1) {
                        fb.putPacked32(
                            self.framebuffer,
                            px + col * self.scale + sx,
                            py + row * self.scale + sy,
                            color,
                        );
                    }
                }
            }
        }
    }

    fn scroll(self: *Console) void {
        const f = self.framebuffer;
        const line_h = font.glyphHeight() * self.scale;
        const cell_w = font.glyphWidth() * self.scale;
        const visible_h: u64 = @as(u64, self.textBottom() - self.margin_top) * line_h;
        const visible_w: u64 = @as(u64, self.textCols()) * cell_w;
        if (visible_h <= line_h or visible_w == 0) return;
        const x0: u64 = @as(u64, self.margin_left) * cell_w;
        const dst_y0: u64 = @as(u64, self.margin_top) * line_h;
        const move_h: u64 = visible_h - line_h;

        // 0.59.4: Der Normalpfad liest keine PAT-WC-/GPU-Framebufferpixel
        // mehr zurueck. Zellinhalt und pro Zelle wirksame Farben wandern im
        // normalen RAM; danach wird der Textbereich streng zeilenweise neu
        // geschrieben, damit Write-Combining auf echter Hardware greifen kann.
        if (self.backingSlice()) |cells| {
            const blank = self.cell(' ');
            if (scroll_buffer.scrollUp(
                cells,
                self.cols,
                self.rows,
                self.margin_left,
                self.margin_top,
                self.textRight(),
                self.textBottom(),
                blank,
            )) {
                self.redrawTextAreaFromBacking();
                return;
            }
            self.invalidateBacking();
        }

        // Geometriewechsel oder ein Modus oberhalb des statischen Zelllimits:
        // Semantik bleibt durch den sichtbaren Move erhalten, aber ausgerichtete
        // 64-/32-Bit-Zugriffe ersetzen die fruehere Byte-fuer-Byte-Schleife.
        _ = fb.movePacked32RectUp(f, x0, dst_y0, visible_w, visible_h, line_h);
        // Fill the lower line with the background color.
        fb.rect(f, x0, dst_y0 + move_h, visible_w, line_h, self.bg);
    }

    fn resetBacking(self: *Console, outer_rgb: u32, inner_rgb: u32, framed: bool) void {
        const count = scroll_buffer.requiredCellCount(self.cols, self.rows) orelse {
            self.invalidateBacking();
            return;
        };
        const outer = scroll_buffer.Cell{
            .glyph = ' ',
            .foreground = fb.packRgb(self.framebuffer, self.fg),
            .background = fb.packRgb(self.framebuffer, outer_rgb),
        };
        const inner = scroll_buffer.Cell{
            .glyph = ' ',
            .foreground = outer.foreground,
            .background = fb.packRgb(self.framebuffer, inner_rgb),
        };

        scroll_buffer.fill(backing_cells[0..count], outer);
        if (framed) {
            var row = self.margin_top;
            while (row < self.textBottom()) : (row += 1) {
                var col = self.margin_left;
                while (col < self.textRight()) : (col += 1) {
                    backing_cells[self.backingIndex(row, col)] = inner;
                }
            }
        }
        self.backing_cols = self.cols;
        self.backing_rows = self.rows;
        self.backing_valid = true;
    }

    fn invalidateBacking(self: *Console) void {
        self.backing_valid = false;
        self.backing_cols = 0;
        self.backing_rows = 0;
    }

    fn backingSlice(self: *Console) ?[]scroll_buffer.Cell {
        if (!self.backing_valid or self.backing_cols != self.cols or self.backing_rows != self.rows) return null;
        const count = scroll_buffer.requiredCellCount(self.cols, self.rows) orelse return null;
        return backing_cells[0..count];
    }

    fn backingIndex(self: *const Console, row: u32, col: u32) usize {
        return @as(usize, row) * @as(usize, self.cols) + @as(usize, col);
    }

    fn cell(self: *const Console, glyph: u8) scroll_buffer.Cell {
        return .{
            .glyph = glyph,
            .foreground = fb.packRgb(self.framebuffer, self.fg),
            .background = fb.packRgb(self.framebuffer, self.bg),
        };
    }

    fn rememberCell(self: *Console, col: u32, row: u32, glyph: u8, foreground: u32, background: u32) void {
        if (self.backingSlice() == null or col >= self.cols or row >= self.rows) return;
        backing_cells[self.backingIndex(row, col)] = .{
            .glyph = glyph,
            .foreground = foreground,
            .background = background,
        };
    }

    fn redrawTextAreaFromBacking(self: *Console) void {
        const cells = self.backingSlice() orelse return;
        _ = cells;
        const glyph_w = font.glyphWidth();
        const glyph_h = font.glyphHeight();
        const cell_w = glyph_w * self.scale;
        const cell_h = glyph_h * self.scale;

        var cell_row = self.margin_top;
        while (cell_row < self.textBottom()) : (cell_row += 1) {
            var glyph_row: u32 = 0;
            while (glyph_row < glyph_h) : (glyph_row += 1) {
                var scale_y: u32 = 0;
                while (scale_y < self.scale) : (scale_y += 1) {
                    const pixel_y = cell_row * cell_h + glyph_row * self.scale + scale_y;
                    var cell_col = self.margin_left;
                    while (cell_col < self.textRight()) : (cell_col += 1) {
                        const stored = backing_cells[self.backingIndex(cell_row, cell_col)];
                        const bits = font.glyphRow(stored.glyph, glyph_row);
                        var glyph_col: u32 = 0;
                        while (glyph_col < glyph_w) : (glyph_col += 1) {
                            const shift: u3 = @intCast(glyph_w - 1 - glyph_col);
                            const color = if ((bits >> shift) & 1 == 1) stored.foreground else stored.background;
                            var scale_x: u32 = 0;
                            while (scale_x < self.scale) : (scale_x += 1) {
                                fb.putPacked32(
                                    self.framebuffer,
                                    cell_col * cell_w + glyph_col * self.scale + scale_x,
                                    pixel_y,
                                    color,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn isUsable(self: *const Console) bool {
        return self.cols > 0 and self.rows > 0 and fb.supportsRgb32(self.framebuffer);
    }

    fn recalculateGeometry(self: *Console) void {
        if (!fb.supportsRgb32(self.framebuffer)) {
            self.cols = 0;
            self.rows = 0;
            return;
        }
        const cell_w = font.glyphWidth() * self.scale;
        const cell_h = font.glyphHeight() * self.scale;
        self.cols = if (cell_w == 0) 0 else @intCast(self.framebuffer.width / cell_w);
        self.rows = if (cell_h == 0) 0 else @intCast(self.framebuffer.height / cell_h);
        self.margin_left = 0;
        self.margin_top = 0;
        self.margin_right = 0;
        self.margin_bottom = 0;
        self.cur_x = 0;
        self.cur_y = 0;
    }

    fn textRight(self: *const Console) u32 {
        return self.cols - self.margin_right;
    }

    fn textBottom(self: *const Console) u32 {
        return self.rows - self.margin_bottom;
    }
};
