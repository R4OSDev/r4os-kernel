const owner_locks = @import("../memory/owner_locks.zig");

pub const BUFFER_SIZE: usize = 65536;
pub const FLAG_WRAPPED: u32 = 1 << 0;

var buffer: [BUFFER_SIZE]u8 = .{0} ** BUFFER_SIZE;
var cursor: usize = 0;
var wrapped: bool = false;
var total_written: u64 = 0;

pub fn puts(text: []const u8) void {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    append(text);
}

pub fn putc(ch: u8) void {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    appendByte(ch);
}

pub fn putDec(value: u64) void {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    var tmp: [20]u8 = undefined;
    var n = value;
    var len: usize = 0;
    if (n == 0) {
        appendByte('0');
        return;
    }
    while (n > 0 and len < tmp.len) : (len += 1) {
        tmp[len] = @intCast('0' + (n % 10));
        n /= 10;
    }
    while (len > 0) {
        len -= 1;
        appendByte(tmp[len]);
    }
}

pub fn putHex(value: u64, width: usize) void {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    const digits = "0123456789ABCDEF";
    var i = width;
    while (i > 0) {
        i -= 1;
        const shift: u6 = @intCast(i * 4);
        const nibble: usize = @intCast((value >> shift) & 0xF);
        appendByte(digits[nibble]);
    }
}

pub fn snapshot(out: []u8) usize {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    if (!wrapped) {
        const len = if (cursor < out.len) cursor else out.len;
        if (len > 0) @memcpy(out[0..len], buffer[0..len]);
        return len;
    }

    var written: usize = 0;
    var i = cursor;
    while (i < BUFFER_SIZE and written < out.len) : (i += 1) {
        out[written] = buffer[i];
        written += 1;
    }
    i = 0;
    while (i < cursor and written < out.len) : (i += 1) {
        out[written] = buffer[i];
        written += 1;
    }
    return written;
}

pub fn capacity() usize {
    return BUFFER_SIZE;
}

pub fn length() usize {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    return if (wrapped) BUFFER_SIZE else cursor;
}

pub fn flags() u32 {
    const irq_flags = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_flags);
    return if (wrapped) FLAG_WRAPPED else 0;
}

pub fn totalWritten() u64 {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    return total_written;
}

pub fn droppedBytes() u64 {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    return if (total_written > BUFFER_SIZE) total_written - BUFFER_SIZE else 0;
}

pub fn read(offset: usize, out: []u8) usize {
    const irq_state = owner_locks.boot_log.acquire();
    defer owner_locks.boot_log.release(irq_state);
    const len = if (wrapped) BUFFER_SIZE else cursor;
    if (offset >= len or out.len == 0) return 0;
    const count = @min(out.len, len - offset);
    var i: usize = 0;
    while (i < count) : (i += 1) {
        out[i] = chronologicalByte(offset + i);
    }
    return count;
}

fn append(text: []const u8) void {
    for (text) |ch| appendByte(ch);
}

fn appendByte(ch: u8) void {
    buffer[cursor] = ch;
    cursor += 1;
    if (cursor >= BUFFER_SIZE) {
        cursor = 0;
        wrapped = true;
    }
    total_written +%= 1;
}

fn chronologicalByte(index: usize) u8 {
    if (!wrapped) return buffer[index];
    return buffer[(cursor + index) % BUFFER_SIZE];
}
