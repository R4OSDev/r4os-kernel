const boot_info = @import("../bootloader/boot_info.zig");
const fb = @import("../display/framebuffer.zig");
const crash = @import("crash.zig");
const diag_screen = @import("diag_screen.zig");
const font = @import("font.zig");
const log = @import("log.zig");
const task = @import("../sched/task.zig");

const BLUE: u32 = 0x0000AA;
const WHITE: u32 = 0xFFFFFF;
const DIM_WHITE: u32 = 0xC0C0C0;
const MAX_BODY_COLS: u32 = 76;

pub const RenderResult = enum(u8) {
    framebuffer = 0,
    serial_only = 1,
};

pub fn render(report: *const crash.CrashReport) RenderResult {
    serialMirror(report);

    var framebuf = bootFramebuffer() orelse return .serial_only;
    if (!fb.supportsRgb32(&framebuf)) return .serial_only;

    drawFramebuffer(&framebuf, report);
    return .framebuffer;
}

pub fn renderToFramebuffer(framebuf: *fb.Framebuffer, report: *const crash.CrashReport) bool {
    if (!fb.supportsRgb32(framebuf)) return false;
    drawFramebuffer(framebuf, report);
    return true;
}

pub fn serialMirror(report: *const crash.CrashReport) void {
    // COM1-TX-Ring erst leeren, dann synchron weiter. Ein toter UART darf
    // den eigentlichen Framebuffer-Crashreport nicht aufhalten.
    log.serialEnterSync();
    serialPuts("[CRASH] cause=");
    serialPuts(crash.causeName(report.cause));
    serialPuts(" stop=0x");
    serialHex(report.stop_code, 8);
    serialPuts(" phase=");
    serialPuts(crash.bootPhaseName(report.boot_phase));
    serialPuts(" ticks=");
    serialDec(report.ticks);
    serialPuts("\r\n");

    if ((report.flags & crash.FLAG_MESSAGE) != 0) {
        serialPuts("[CRASH] message=");
        serialPuts(report.message.slice());
        serialPuts("\r\n");
    }

    if ((report.flags & crash.FLAG_CPU) != 0) {
        serialPuts("[CRASH] vector=");
        serialDec(report.cpu.vector);
        serialPuts(" exception=");
        serialPuts(crash.exceptionName(report.cpu.kind));
        serialPuts(" error=0x");
        serialHex(report.cpu.error_code, 16);
        serialPuts(" rip=0x");
        serialHex(report.cpu.rip, 16);
        serialPuts(" rsp=0x");
        serialHex(report.cpu.rsp, 16);
        serialPuts("\r\n");
    }

    if ((report.flags & crash.FLAG_PAGE_FAULT) != 0) {
        serialPuts("[CRASH] cr2=0x");
        serialHex(report.page_fault.fault_address, 16);
        serialPuts(" pf=");
        serialPutFlag("p", report.page_fault.present);
        serialPutFlag("w", report.page_fault.write);
        serialPutFlag("u", report.page_fault.user);
        serialPutFlag("rsvd", report.page_fault.reserved_bit);
        serialPutFlag("ifetch", report.page_fault.instruction_fetch);
        serialPuts("\r\n");
        // 0.56.15: Fault in einer Kernel-Stack-Guard-Page? Dann ist das ein
        // Stack-Overflow eines Kernel-Tasks - beim Namen nennen.
        if (task.stackGuardHit(report.page_fault.fault_address)) |hit| {
            serialPuts("[CRASH] STACK GUARD HIT task=");
            serialPuts(hit.name);
            serialPuts(" id=");
            serialDec(hit.id);
            serialPuts(" stack_base=0x");
            serialHex(hit.stack_base, 16);
            serialPuts("\r\n");
        }
    }

    if ((report.flags & crash.FLAG_CONTEXT) != 0) {
        serialPuts("[CRASH] context r4x=");
        serialDec(report.context.r4x_instance_id);
        serialPuts(" task=");
        serialDec(report.context.task_id);
        serialPuts(" owner=");
        serialDec(report.context.owner_id);
        serialPuts(" image=0x");
        serialHex(report.context.image_base, 16);
        serialPuts(" size=");
        serialDec(report.context.image_size);
        serialPuts(" ip_offset=0x");
        serialHex(report.context.instruction_offset, 16);
        const tag = fixedZ(report.context.tag[0..]);
        if (tag.len != 0) {
            serialPuts(" tag=");
            serialPuts(tag);
        }
        serialPuts("\r\n");
    }

    const incident = diag_screen.incidentText();
    if (incident.len != 0) {
        serialPuts("[CRASH] first diagnostic incident follows\r\n");
        for (incident) |ch| {
            if (ch == '\n') {
                serialPuts("\r\n");
            } else if (ch != '\r') {
                log.serialPutcRaw(ch);
            }
        }
        if (incident[incident.len - 1] != '\n') serialPuts("\r\n");
    }
}

fn drawFramebuffer(framebuf: *fb.Framebuffer, report: *const crash.CrashReport) void {
    var r = Renderer.init(framebuf);
    if (!r.usable()) return;

    fb.fill(framebuf, BLUE);
    r.centerLine(1, "R4OS");
    r.row = 4;
    r.putLine("A fatal system error has occurred.");
    r.putLine("The system has been stopped to protect R4OS.");
    r.blank();

    r.puts("STOP: 0x");
    r.putHex(report.stop_code, 8);
    r.puts("  ");
    r.putLine(crash.causeName(report.cause));

    r.puts("Phase: ");
    r.puts(crash.bootPhaseName(report.boot_phase));
    r.puts("    Ticks: ");
    r.putDec(report.ticks);
    r.newline();

    if ((report.flags & crash.FLAG_MESSAGE) != 0) {
        r.puts("Message: ");
        r.putLine(report.message.slice());
    }
    r.blank();

    if ((report.flags & crash.FLAG_CPU) != 0) {
        r.puts("Exception: ");
        r.puts(crash.exceptionName(report.cpu.kind));
        r.puts("  Vector: ");
        r.putDec(report.cpu.vector);
        r.puts("  Error: 0x");
        r.putHex(report.cpu.error_code, 16);
        r.newline();

        r.puts("RIP: 0x");
        r.putHex(report.cpu.rip, 16);
        r.puts("  RSP: 0x");
        r.putHex(report.cpu.rsp, 16);
        r.newline();

        r.puts("RBP: 0x");
        r.putHex(report.cpu.rbp, 16);
        r.puts("  RFLAGS: 0x");
        r.putHex(report.cpu.rflags, 16);
        r.newline();

        r.puts("RAX: 0x");
        r.putHex(report.cpu.registers.rax, 16);
        r.puts("  RBX: 0x");
        r.putHex(report.cpu.registers.rbx, 16);
        r.newline();

        r.puts("RCX: 0x");
        r.putHex(report.cpu.registers.rcx, 16);
        r.puts("  RDX: 0x");
        r.putHex(report.cpu.registers.rdx, 16);
        r.newline();
    }

    if ((report.flags & crash.FLAG_PAGE_FAULT) != 0) {
        r.blank();
        r.puts("Fault address: 0x");
        r.putHex(report.page_fault.fault_address, 16);
        r.newline();
        r.puts("Page fault: ");
        r.putFlag("present", report.page_fault.present);
        r.putFlag("write", report.page_fault.write);
        r.putFlag("user", report.page_fault.user);
        r.putFlag("rsvd", report.page_fault.reserved_bit);
        r.putFlag("ifetch", report.page_fault.instruction_fetch);
        r.newline();
    }

    if ((report.flags & crash.FLAG_MEMORY) != 0) {
        r.blank();
        r.puts("Memory: ");
        switch (report.memory.state) {
            .tracked => {
                r.puts(crash.memoryOwnerName(report.memory.owner));
                r.puts("#");
                r.putDec(report.memory.owner_id);
                r.puts("  ");
                r.puts(crash.memoryKindName(report.memory.kind));
                r.puts("  ");
                r.putLine(crash.memoryStatusName(report.memory.status));
            },
            .untracked => r.putLine("untracked"),
            .unavailable => r.putLine("unavailable"),
        }
    }

    if ((report.flags & crash.FLAG_CONTEXT) != 0) {
        r.blank();
        r.puts("Context: r4x=");
        r.putDec(report.context.r4x_instance_id);
        r.puts(" task=");
        r.putDec(report.context.task_id);
        r.puts(" owner=");
        r.putDec(report.context.owner_id);
        if (report.context.image_base != 0) {
            r.puts(" image=0x");
            r.putHex(report.context.image_base, 16);
            r.puts(" off=0x");
            r.putHex(report.context.instruction_offset, 16);
        }
        const tag = fixedZ(report.context.tag[0..]);
        if (tag.len != 0) {
            r.puts(" tag=");
            r.puts(tag);
        }
        r.newline();
    }

    const incident = diag_screen.incidentText();
    const incident_last_row = r.incidentLastRow();
    if (incident.len != 0 and
        r.row < incident_last_row and
        incident_last_row - r.row > 2)
    {
        r.blank();
        r.putDimLine("First diagnostic incident:");
        r.putWrappedLimited(incident, incident_last_row);
    }

    r.bottomLine("Restart the system. If this repeats, use diagnostics with the data above.");
}

fn bootFramebuffer() ?fb.Framebuffer {
    const src = boot_info.framebuffer() orelse return null;
    return .{
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
}

const Renderer = struct {
    framebuffer: *fb.Framebuffer,
    scale: u32,
    cols: u32,
    rows: u32,
    left: u32,
    right: u32,
    row: u32 = 0,
    col: u32 = 0,
    fg: u32,
    dim: u32,

    fn init(framebuffer: *fb.Framebuffer) Renderer {
        const scale = scaleFor(framebuffer);
        const cell_w = @as(u64, font.GLYPH_W) * scale;
        const cell_h = @as(u64, font.GLYPH_H) * scale;
        const cols: u32 = if (cell_w == 0) 0 else @intCast(framebuffer.width / cell_w);
        const rows: u32 = if (cell_h == 0) 0 else @intCast(framebuffer.height / cell_h);
        const body_cols = if (cols > 4) minU32(cols - 4, MAX_BODY_COLS) else cols;
        const left = if (cols > body_cols) (cols - body_cols) / 2 else 0;
        return .{
            .framebuffer = framebuffer,
            .scale = scale,
            .cols = cols,
            .rows = rows,
            .left = left,
            .right = left + body_cols,
            .row = 0,
            .col = left,
            .fg = fb.packRgb(framebuffer, WHITE),
            .dim = fb.packRgb(framebuffer, DIM_WHITE),
        };
    }

    fn usable(self: *const Renderer) bool {
        return self.cols > 0 and self.rows > 0 and self.right > self.left;
    }

    fn centerLine(self: *Renderer, row: u32, text: []const u8) void {
        if (row >= self.rows) return;
        const width: u32 = @intCast(min(text.len, self.cols));
        self.row = row;
        self.col = if (self.cols > width) (self.cols - width) / 2 else 0;
        self.putLine(text);
        self.col = self.left;
    }

    fn bottomLine(self: *Renderer, text: []const u8) void {
        if (self.rows == 0) return;
        self.row = self.footerRow();
        self.col = self.left;
        self.putDimLine(text);
    }

    fn footerRow(self: *const Renderer) u32 {
        if (self.rows == 0) return 0;
        return if (self.rows > 3) self.rows - 3 else self.rows - 1;
    }

    // Exclusive body limit. Keep one untouched row between retained incident
    // evidence and the fixed restart footer so wrapping can never overwrite
    // the footer on short or unusually shaped framebuffers.
    fn incidentLastRow(self: *const Renderer) u32 {
        const footer = self.footerRow();
        return if (footer > 0) footer - 1 else 0;
    }

    fn blank(self: *Renderer) void {
        self.newline();
    }

    fn putLine(self: *Renderer, text: []const u8) void {
        self.puts(text);
        self.newline();
    }

    fn putDimLine(self: *Renderer, text: []const u8) void {
        const old = self.fg;
        self.fg = self.dim;
        self.putLine(text);
        self.fg = old;
    }

    fn puts(self: *Renderer, text: []const u8) void {
        for (text) |ch| self.putc(ch);
    }

    fn putWrappedLimited(self: *Renderer, text: []const u8, last_row: u32) void {
        for (text) |ch| {
            if (self.row >= last_row) return;
            if (ch == '\r') continue;
            if (ch == '\n') {
                self.newline();
                continue;
            }
            if (self.col >= self.right) {
                self.newline();
                if (self.row >= last_row) return;
            }
            self.putc(ch);
        }
        if (self.col != self.left and self.row < last_row) self.newline();
    }

    fn putc(self: *Renderer, ch: u8) void {
        if (self.row >= self.rows or self.col >= self.right) return;
        if (ch == '\r') {
            self.col = self.left;
            return;
        }
        if (ch == '\n') {
            self.newline();
            return;
        }
        self.drawGlyph(self.col, self.row, if (ch >= 0x20 and ch <= 0x7e) ch else '?');
        self.col += 1;
    }

    fn newline(self: *Renderer) void {
        if (self.row < self.rows) self.row += 1;
        self.col = self.left;
    }

    fn putHex(self: *Renderer, value: u64, digits: u8) void {
        const hex = "0123456789ABCDEF";
        var i: u8 = digits;
        while (i > 0) {
            i -= 1;
            const shift: u6 = @intCast(@as(u32, i) * 4);
            const nibble: u4 = @truncate(value >> shift);
            self.putc(hex[nibble]);
        }
    }

    fn putDec(self: *Renderer, value: u64) void {
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

    fn putFlag(self: *Renderer, name: []const u8, value: u8) void {
        self.puts(name);
        self.putc('=');
        self.putc(if (value != 0) '1' else '0');
        self.putc(' ');
    }

    fn drawGlyph(self: *Renderer, cell_x: u32, cell_y: u32, ch: u8) void {
        const glyph = font.glyph(ch);
        const px0 = @as(u64, cell_x) * font.GLYPH_W * self.scale;
        const py0 = @as(u64, cell_y) * font.GLYPH_H * self.scale;
        var row: u32 = 0;
        while (row < font.GLYPH_H) : (row += 1) {
            const bits = glyph[row];
            var col: u32 = 0;
            while (col < font.GLYPH_W) : (col += 1) {
                const shift: u3 = @intCast(font.GLYPH_W - 1 - col);
                if (((bits >> shift) & 1) == 0) continue;
                var sy: u32 = 0;
                while (sy < self.scale) : (sy += 1) {
                    var sx: u32 = 0;
                    while (sx < self.scale) : (sx += 1) {
                        fb.putPacked32(
                            self.framebuffer,
                            px0 + col * self.scale + sx,
                            py0 + row * self.scale + sy,
                            self.fg,
                        );
                    }
                }
            }
        }
    }
};

fn scaleFor(framebuffer: *const fb.Framebuffer) u32 {
    if (framebuffer.width >= 960 and framebuffer.height >= 400) return 2;
    return 1;
}

fn serialPuts(text: []const u8) void {
    for (text) |ch| log.serialPutcRaw(ch);
}

fn fixedZ(bytes: []const u8) []const u8 {
    var len: usize = 0;
    while (len < bytes.len and bytes[len] != 0) : (len += 1) {}
    return bytes[0..len];
}

fn serialHex(value: u64, digits: u8) void {
    const hex = "0123456789ABCDEF";
    var i: u8 = digits;
    while (i > 0) {
        i -= 1;
        const shift: u6 = @intCast(@as(u32, i) * 4);
        const nibble: u4 = @truncate(value >> shift);
        log.serialPutcRaw(hex[nibble]);
    }
}

fn serialDec(value: u64) void {
    if (value == 0) {
        log.serialPutcRaw('0');
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
    serialPuts(buf[i..]);
}

fn serialPutFlag(name: []const u8, value: u8) void {
    serialPuts(name);
    log.serialPutcRaw('=');
    log.serialPutcRaw(if (value != 0) '1' else '0');
    log.serialPutcRaw(' ');
}

fn min(a: usize, b: usize) usize {
    return if (a < b) a else b;
}

fn minU32(a: u32, b: u32) u32 {
    return if (a < b) a else b;
}
