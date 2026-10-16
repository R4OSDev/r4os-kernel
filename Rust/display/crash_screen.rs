//! Original fatal report, fixed restart footer, and synchronous serial mirror.
//! Rendering uses only the immutable built-in font, never the reloadable catalog.
use super::{
    diag_screen, firmware,
    framebuffer::{self as fb, Framebuffer},
};
use crate::{
    boot::crash::{self, CrashReport},
    font::builtin,
    sched::task::task_api,
};
use core::{mem::MaybeUninit, slice};
unsafe extern "C" {
    fn r4os_zig_crash_screen_sync();
    fn r4os_zig_crash_screen_putc(ch: u8);
}
const FLAG_MESSAGE: u16 = 1 << 4;
const FLAG_CPU: u16 = 1 << 0;
const FLAG_PAGE_FAULT: u16 = 1 << 1;
const FLAG_MEMORY: u16 = 1 << 2;
const FLAG_CONTEXT: u16 = 1 << 3;
const BLUE: u32 = 0x0000aa;
const WHITE: u32 = 0xffffff;
const DIM: u32 = 0xc0c0c0;
fn fixed_z(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len())]
}
fn serial_putc(ch: u8) {
    unsafe { r4os_zig_crash_screen_putc(ch) }
}
fn serial_puts(text: &[u8]) {
    for &ch in text {
        serial_putc(ch)
    }
}
fn serial_hex(value: u64, digits: u8) {
    for i in (0..digits).rev() {
        serial_putc(b"0123456789ABCDEF"[((value >> (u32::from(i) * 4)) & 15) as usize]);
    }
}
fn decimal(mut value: u64, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    &buf[i..]
}
fn serial_dec(value: u64) {
    let mut buf = [0; 20];
    serial_puts(decimal(value, &mut buf));
}
fn serial_flag(name: &[u8], value: u8) {
    serial_puts(name);
    serial_putc(b'=');
    serial_putc(if value != 0 { b'1' } else { b'0' });
    serial_putc(b' ');
}
fn serial_mirror(r: &CrashReport) {
    unsafe { r4os_zig_crash_screen_sync() }
    serial_puts(b"[CRASH] cause=");
    serial_puts(crash::cause_name(r.cause));
    serial_puts(b" stop=0x");
    serial_hex(u64::from(r.stop_code), 8);
    serial_puts(b" phase=");
    serial_puts(crash::phase_name(r.boot_phase));
    serial_puts(b" ticks=");
    serial_dec(r.ticks);
    serial_puts(b"\r\n");
    if r.flags & FLAG_MESSAGE != 0 {
        serial_puts(b"[CRASH] message=");
        serial_puts(r.message.bytes());
        serial_puts(b"\r\n");
    }
    if r.flags & FLAG_CPU != 0 {
        serial_puts(b"[CRASH] vector=");
        serial_dec(u64::from(r.cpu.vector));
        serial_puts(b" exception=");
        serial_puts(crash::exception_name(r.cpu.kind));
        serial_puts(b" error=0x");
        serial_hex(r.cpu.error_code, 16);
        serial_puts(b" rip=0x");
        serial_hex(r.cpu.rip, 16);
        serial_puts(b" rsp=0x");
        serial_hex(r.cpu.rsp, 16);
        serial_puts(b"\r\n");
    }
    if r.flags & FLAG_PAGE_FAULT != 0 {
        serial_puts(b"[CRASH] cr2=0x");
        serial_hex(r.page_fault.fault_address, 16);
        serial_puts(b" pf=");
        serial_flag(b"p", r.page_fault.present);
        serial_flag(b"w", r.page_fault.write);
        serial_flag(b"u", r.page_fault.user);
        serial_flag(b"rsvd", r.page_fault.reserved_bit);
        serial_flag(b"ifetch", r.page_fault.instruction_fetch);
        serial_puts(b"\r\n");
        let mut hit = MaybeUninit::uninit();
        if unsafe {
            task_api::r4os_rust_task_guard_hit(r.page_fault.fault_address, hit.as_mut_ptr())
        } {
            let hit = unsafe { hit.assume_init() };
            serial_puts(b"[CRASH] STACK GUARD HIT task=");
            let name = if hit.name.len == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(hit.name.pointer, hit.name.len) }
            };
            serial_puts(name);
            serial_puts(b" id=");
            serial_dec(u64::from(hit.id));
            serial_puts(b" stack_base=0x");
            serial_hex(hit.stack_base, 16);
            serial_puts(b"\r\n");
        }
    }
    if r.flags & FLAG_CONTEXT != 0 {
        serial_puts(b"[CRASH] context r4x=");
        serial_dec(u64::from(r.context.r4x_instance_id));
        serial_puts(b" task=");
        serial_dec(u64::from(r.context.task_id));
        serial_puts(b" owner=");
        serial_dec(r.context.owner_id);
        serial_puts(b" image=0x");
        serial_hex(r.context.image_base, 16);
        serial_puts(b" size=");
        serial_dec(r.context.image_size);
        serial_puts(b" ip_offset=0x");
        serial_hex(r.context.instruction_offset, 16);
        let tag = fixed_z(&r.context.tag);
        if !tag.is_empty() {
            serial_puts(b" tag=");
            serial_puts(tag);
        }
        serial_puts(b"\r\n");
    }
    let incident = unsafe { diag_screen::incident_text() };
    if !incident.is_empty() {
        serial_puts(b"[CRASH] first diagnostic incident follows\r\n");
        for &ch in incident {
            if ch == b'\n' {
                serial_puts(b"\r\n")
            } else if ch != b'\r' {
                serial_putc(ch)
            }
        }
        if incident[incident.len() - 1] != b'\n' {
            serial_puts(b"\r\n")
        }
    }
}
struct Renderer<'a> {
    frame: &'a Framebuffer,
    scale: u32,
    cols: u32,
    rows: u32,
    left: u32,
    right: u32,
    row: u32,
    col: u32,
    fg: u32,
    dim: u32,
}
impl<'a> Renderer<'a> {
    fn new(f: &'a Framebuffer) -> Self {
        let scale = if f.width >= 960 && f.height >= 400 {
            2
        } else {
            1
        };
        let cols = u32::try_from(f.width / (8 * u64::from(scale))).expect("Original fatal columns");
        let rows = u32::try_from(f.height / (8 * u64::from(scale))).expect("Original fatal rows");
        let body = if cols > 4 { (cols - 4).min(76) } else { cols };
        let left = if cols > body { (cols - body) / 2 } else { 0 };
        Self {
            frame: f,
            scale,
            cols,
            rows,
            left,
            right: left + body,
            row: 0,
            col: left,
            fg: fb::pack_rgb(f, WHITE),
            dim: fb::pack_rgb(f, DIM),
        }
    }
    fn usable(&self) -> bool {
        self.cols > 0 && self.rows > 0 && self.right > self.left
    }
    fn footer_row(&self) -> u32 {
        if self.rows == 0 {
            0
        } else if self.rows > 3 {
            self.rows - 3
        } else {
            self.rows - 1
        }
    }
    fn incident_last_row(&self) -> u32 {
        self.footer_row().saturating_sub(1)
    }
    fn center_line(&mut self, row: u32, text: &[u8]) {
        if row >= self.rows {
            return;
        }
        let width = text.len().min(self.cols as usize) as u32;
        self.row = row;
        self.col = if self.cols > width {
            (self.cols - width) / 2
        } else {
            0
        };
        self.put_line(text);
        self.col = self.left;
    }
    fn bottom_line(&mut self, text: &[u8]) {
        if self.rows == 0 {
            return;
        }
        self.row = self.footer_row();
        self.col = self.left;
        self.dim_line(text)
    }
    fn newline(&mut self) {
        if self.row < self.rows {
            self.row += 1
        }
        self.col = self.left;
    }
    fn puts(&mut self, text: &[u8]) {
        for &ch in text {
            self.putc(ch)
        }
    }
    fn put_line(&mut self, text: &[u8]) {
        self.puts(text);
        self.newline();
    }
    fn dim_line(&mut self, text: &[u8]) {
        let old = self.fg;
        self.fg = self.dim;
        self.put_line(text);
        self.fg = old;
    }
    fn wrapped(&mut self, text: &[u8], last: u32) {
        for &ch in text {
            if self.row >= last {
                return;
            }
            if ch == b'\r' {
                continue;
            }
            if ch == b'\n' {
                self.newline();
                continue;
            }
            if self.col >= self.right {
                self.newline();
                if self.row >= last {
                    return;
                }
            }
            self.putc(ch)
        }
        if self.col != self.left && self.row < last {
            self.newline()
        }
    }
    fn putc(&mut self, ch: u8) {
        if self.row >= self.rows || self.col >= self.right {
            return;
        }
        if ch == b'\r' {
            self.col = self.left;
            return;
        }
        if ch == b'\n' {
            self.newline();
            return;
        }
        self.draw_glyph(if (0x20..=0x7e).contains(&ch) {
            ch
        } else {
            b'?'
        });
        self.col += 1;
    }
    fn hex(&mut self, value: u64, digits: u8) {
        for i in (0..digits).rev() {
            self.putc(b"0123456789ABCDEF"[((value >> (u32::from(i) * 4)) & 15) as usize]);
        }
    }
    fn dec(&mut self, value: u64) {
        let mut buf = [0; 20];
        self.puts(decimal(value, &mut buf));
    }
    fn flag(&mut self, name: &[u8], value: u8) {
        self.puts(name);
        self.putc(b'=');
        self.putc(if value != 0 { b'1' } else { b'0' });
        self.putc(b' ');
    }
    fn draw_glyph(&self, ch: u8) {
        let glyph = builtin::glyph(u32::from(ch));
        let x = u64::from(self.col) * 8 * u64::from(self.scale);
        let y = u64::from(self.row) * 8 * u64::from(self.scale);
        for (row, &bits) in glyph.iter().enumerate() {
            for col in 0..8u32 {
                if bits >> (7 - col) & 1 == 0 {
                    continue;
                }
                for sy in 0..self.scale {
                    for sx in 0..self.scale {
                        unsafe {
                            fb::put_packed(
                                self.frame,
                                x + u64::from(col * self.scale + sx),
                                y + row as u64 * u64::from(self.scale) + u64::from(sy),
                                self.fg,
                            );
                        }
                    }
                }
            }
        }
    }
}
unsafe fn draw_framebuffer(f: &Framebuffer, report: &CrashReport) {
    let mut r = Renderer::new(f);
    if !r.usable() {
        return;
    }
    unsafe { fb::fill(f, BLUE) }
    r.center_line(1, b"R4OS");
    r.row = 4;
    r.put_line(b"A fatal system error has occurred.");
    r.put_line(b"The system has been stopped to protect R4OS.");
    r.newline();
    r.puts(b"STOP: 0x");
    r.hex(u64::from(report.stop_code), 8);
    r.puts(b"  ");
    r.put_line(crash::cause_name(report.cause));
    r.puts(b"Phase: ");
    r.puts(crash::phase_name(report.boot_phase));
    r.puts(b"    Ticks: ");
    r.dec(report.ticks);
    r.newline();
    if report.flags & FLAG_MESSAGE != 0 {
        r.puts(b"Message: ");
        r.put_line(report.message.bytes());
    }
    r.newline();
    if report.flags & FLAG_CPU != 0 {
        r.puts(b"Exception: ");
        r.puts(crash::exception_name(report.cpu.kind));
        r.puts(b"  Vector: ");
        r.dec(u64::from(report.cpu.vector));
        r.puts(b"  Error: 0x");
        r.hex(report.cpu.error_code, 16);
        r.newline();
        r.puts(b"RIP: 0x");
        r.hex(report.cpu.rip, 16);
        r.puts(b"  RSP: 0x");
        r.hex(report.cpu.rsp, 16);
        r.newline();
        r.puts(b"RBP: 0x");
        r.hex(report.cpu.rbp, 16);
        r.puts(b"  RFLAGS: 0x");
        r.hex(report.cpu.rflags, 16);
        r.newline();
        r.puts(b"RAX: 0x");
        r.hex(report.cpu.registers.rax, 16);
        r.puts(b"  RBX: 0x");
        r.hex(report.cpu.registers.rbx, 16);
        r.newline();
        r.puts(b"RCX: 0x");
        r.hex(report.cpu.registers.rcx, 16);
        r.puts(b"  RDX: 0x");
        r.hex(report.cpu.registers.rdx, 16);
        r.newline();
    }
    if report.flags & FLAG_PAGE_FAULT != 0 {
        r.newline();
        r.puts(b"Fault address: 0x");
        r.hex(report.page_fault.fault_address, 16);
        r.newline();
        r.puts(b"Page fault: ");
        r.flag(b"present", report.page_fault.present);
        r.flag(b"write", report.page_fault.write);
        r.flag(b"user", report.page_fault.user);
        r.flag(b"rsvd", report.page_fault.reserved_bit);
        r.flag(b"ifetch", report.page_fault.instruction_fetch);
        r.newline();
    }
    if report.flags & FLAG_MEMORY != 0 {
        r.newline();
        r.puts(b"Memory: ");
        match report.memory.state {
            2 => {
                r.puts(crash::memory_owner_name(report.memory.owner));
                r.puts(b"#");
                r.dec(report.memory.owner_id);
                r.puts(b"  ");
                r.puts(crash::memory_kind_name(report.memory.kind));
                r.puts(b"  ");
                r.put_line(crash::memory_status_name(report.memory.status));
            }
            1 => r.put_line(b"untracked"),
            _ => r.put_line(b"unavailable"),
        }
    }
    if report.flags & FLAG_CONTEXT != 0 {
        r.newline();
        r.puts(b"Context: r4x=");
        r.dec(u64::from(report.context.r4x_instance_id));
        r.puts(b" task=");
        r.dec(u64::from(report.context.task_id));
        r.puts(b" owner=");
        r.dec(report.context.owner_id);
        if report.context.image_base != 0 {
            r.puts(b" image=0x");
            r.hex(report.context.image_base, 16);
            r.puts(b" off=0x");
            r.hex(report.context.instruction_offset, 16);
        }
        let tag = fixed_z(&report.context.tag);
        if !tag.is_empty() {
            r.puts(b" tag=");
            r.puts(tag);
        }
        r.newline();
    }
    let incident = unsafe { diag_screen::incident_text() };
    let last = r.incident_last_row();
    if !incident.is_empty() && r.row < last && last - r.row > 2 {
        r.newline();
        r.dim_line(b"First diagnostic incident:");
        r.wrapped(incident, last);
    }
    r.bottom_line(b"Restart the system. If this repeats, use diagnostics with the data above.");
}
/// # Safety
/// Caller freezes the report and retained diagnostic evidence for fatal output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_crash_screen_render(report: *const CrashReport) -> u8 {
    let report = unsafe { &*report };
    serial_mirror(report);
    let Some(_lease) = firmware::Lease::acquire() else {
        return 1;
    };
    let Some(f) = fb::boot_framebuffer() else {
        return 1;
    };
    if !fb::supports_rgb32(&f) {
        return 1;
    }
    unsafe { draw_framebuffer(&f, report) }
    0
}
/// # Safety
/// Caller owns the complete framebuffer writer and immutable report/evidence.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_crash_screen_frame(
    f: *const Framebuffer,
    r: *const CrashReport,
) -> bool {
    let f = unsafe { &*f };
    if !fb::supports_rgb32(f) {
        return false;
    }
    unsafe { draw_framebuffer(f, &*r) }
    true
}
/// # Safety
/// Caller freezes the report and retained diagnostic evidence for serial output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_crash_screen_serial(r: *const CrashReport) {
    serial_mirror(unsafe { &*r })
}
