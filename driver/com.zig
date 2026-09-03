// 16550-kompatibler UART (PC COM-Ports).

const io = @import("../arch/x86_64/io.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");

pub const COM1: u16 = 0x3F8;
pub const COM2: u16 = 0x2F8;
const TX_READY_TIMEOUT: usize = 1_000_000;
const UART_FIFO_DEPTH: usize = 16;

pub const Port = struct {
    base: u16,

    pub fn init(self: Port) void {
        io.outb(self.base + 1, 0x00); // IRQs aus
        io.outb(self.base + 3, 0x80); // DLAB on
        io.outb(self.base + 0, 0x03); // 38400 baud (low)
        io.outb(self.base + 1, 0x00); // (high)
        io.outb(self.base + 3, 0x03); // 8N1
        io.outb(self.base + 2, 0xC7); // FIFO an
        io.outb(self.base + 4, 0x0B); // RTS/DSR
    }

    pub fn isPresent(self: Port) bool {
        io.outb(self.base + 7, 0x5A);
        if (io.inb(self.base + 7) != 0x5A) return false;
        io.outb(self.base + 7, 0xA5);
        return io.inb(self.base + 7) == 0xA5;
    }

    pub fn putc(self: Port, c: u8) void {
        var spins: usize = 0;
        while ((io.inb(self.base + 5) & 0x20) == 0) : (spins += 1) {
            if (spins >= TX_READY_TIMEOUT) return;
        }
        io.outb(self.base, c);
    }

    // Nonblocking runtime path for COM2/Serial-Link. Once THR is empty the
    // enabled 16550 FIFO accepts one bounded burst; callers own retry and
    // deadline policy and therefore never hide a stalled byte as success.
    pub fn writeAvailable(self: Port, data: []const u8) usize {
        if (data.len == 0 or (io.inb(self.base + 5) & 0x20) == 0) return 0;
        const count = @min(data.len, UART_FIFO_DEPTH);
        for (data[0..count]) |byte| io.outb(self.base, byte);
        return count;
    }

    pub fn hasData(self: Port) bool {
        return (io.inb(self.base + 5) & 0x01) != 0;
    }

    pub fn getc(self: Port) ?u8 {
        if (!self.hasData()) return null;
        return io.inb(self.base);
    }

    pub fn puts(self: Port, s: []const u8) void {
        for (s) |c| self.putc(c);
    }
};

// Default ports for convenience.
pub var com1: Port = .{ .base = COM1 };
pub var com2: Port = .{ .base = COM2 };

pub fn init() void {
    com1.init();
}

pub fn puts(s: []const u8) void {
    // Ueber den TX-Ring, damit Direktnutzer die Ring-Reihenfolge nicht
    // ueberholen (COM1-Log ist EIN Strom).
    for (s) |c| logPutc(c);
}

// --- COM1 TX-Ring fuer den Kernel-Log-Pfad (0.56.15) ---
// putc schreibt nur in den Ring; gedraint wird opportunistisch (nach jedem
// Zeichen, wenn der UART-THR frei ist) sowie per Timer-Tick. Der Crash-/
// Poweroff-/Crash-Pfad schaltet mit logEnterSyncMode() auf synchrone
// Direktausgabe um. Auch dort ist ein nicht antwortender UART strikt
// begrenzt; COM2 (Serial-Link) bleibt bewusst roh/synchron.
const TX_RING_SIZE: usize = 8192; // Zweierpotenz (Index-Maskierung)

var tx_ring: [TX_RING_SIZE]u8 = undefined;
var tx_head: usize = 0; // monoton, Index = head & (SIZE-1)
var tx_tail: usize = 0;
var tx_sync_mode: bool = false;
var tx_ring_bytes: u64 = 0;
var tx_sync_bytes: u64 = 0;
var tx_full_stalls: u64 = 0;
var tx_dropped_bytes: u64 = 0;
var tx_sync_failures: u64 = 0;
var tx_sync_uart_available: bool = true;

pub const LogTxStats = struct {
    ring_size: u64 = TX_RING_SIZE,
    pending: u64 = 0,
    ring_bytes: u64 = 0,
    sync_bytes: u64 = 0,
    full_stalls: u64 = 0,
    dropped_bytes: u64 = 0,
    sync_failures: u64 = 0,
    sync_mode: bool = false,
    sync_uart_available: bool = true,
};

pub fn logTxStats() LogTxStats {
    return .{
        .pending = tx_head -% tx_tail,
        .ring_bytes = tx_ring_bytes,
        .sync_bytes = tx_sync_bytes,
        .full_stalls = tx_full_stalls,
        .dropped_bytes = tx_dropped_bytes,
        .sync_failures = tx_sync_failures,
        .sync_mode = tx_sync_mode,
        .sync_uart_available = tx_sync_uart_available,
    };
}

pub fn logPutc(c: u8) void {
    if (tx_sync_mode) {
        if (writeSyncByte(c)) {
            tx_sync_bytes +%= 1;
        } else {
            tx_dropped_bytes +%= 1;
        }
        return;
    }
    const flags = interrupts.saveAndDisableFor(.driver);
    defer interrupts.restore(flags);
    if (tx_head -% tx_tail >= TX_RING_SIZE) {
        // Normaler Runtime-Log darf niemals mit deaktivierten IRQs auf
        // Hardware warten. Erst genau einen nichtblockierenden Drain
        // versuchen, dann bei weiter voller Queue das aelteste Byte opfern.
        tx_full_stalls +%= 1;
        drainLocked();
        if (tx_head -% tx_tail >= TX_RING_SIZE) {
            tx_tail +%= 1;
            tx_dropped_bytes +%= 1;
        }
    }
    tx_ring[tx_head & (TX_RING_SIZE - 1)] = c;
    tx_head +%= 1;
    tx_ring_bytes +%= 1;
    drainLocked();
}

// Opportunistischer Drain fuer Timer-Tick/Idle: nicht blockierend.
pub fn logDrain() void {
    if (tx_tail == tx_head) return;
    const flags = interrupts.saveAndDisableFor(.driver);
    defer interrupts.restore(flags);
    drainLocked();
}

// Synchron leeren (Poweroff-/Abschluss-Pfad), solange der UART fortschreitet.
// Ein einmal erkannter Hardwarestillstand verwirft den Rest statt zu haengen.
pub fn logFlushSync() void {
    const flags = interrupts.saveAndDisableFor(.driver);
    defer interrupts.restore(flags);
    while (tx_tail != tx_head) {
        if (!drainSyncBurst()) {
            tx_dropped_bytes +%= tx_head -% tx_tail;
            tx_tail = tx_head; // UART tot: aufgeben statt haengen
            return;
        }
    }
}

// Crash-Pfad: erst Ring leeren, danach jede Ausgabe direkt/synchron.
pub fn logEnterSyncMode() void {
    tx_sync_mode = true;
    logFlushSync();
}

fn drainLocked() void {
    if (tx_tail == tx_head) return;
    if ((io.inb(COM1 + 5) & 0x20) == 0) return; // THR nicht frei
    var burst: usize = 0;
    while (burst < UART_FIFO_DEPTH and tx_tail != tx_head) : (burst += 1) {
        io.outb(COM1, tx_ring[tx_tail & (TX_RING_SIZE - 1)]);
        tx_tail +%= 1;
    }
}

fn drainSyncBurst() bool {
    if (!tx_sync_uart_available) return false;
    var spins: usize = 0;
    while ((io.inb(COM1 + 5) & 0x20) == 0) : (spins += 1) {
        if (spins >= TX_READY_TIMEOUT) {
            tx_sync_uart_available = false;
            tx_sync_failures +%= 1;
            return false;
        }
    }
    var burst: usize = 0;
    while (burst < UART_FIFO_DEPTH and tx_tail != tx_head) : (burst += 1) {
        io.outb(COM1, tx_ring[tx_tail & (TX_RING_SIZE - 1)]);
        tx_tail +%= 1;
    }
    return true;
}

fn writeSyncByte(c: u8) bool {
    if (!tx_sync_uart_available) return false;
    var spins: usize = 0;
    while ((io.inb(COM1 + 5) & 0x20) == 0) : (spins += 1) {
        if (spins >= TX_READY_TIMEOUT) {
            tx_sync_uart_available = false;
            tx_sync_failures +%= 1;
            return false;
        }
    }
    io.outb(COM1, c);
    return true;
}
