const bootlog = @import("../kernel/bootlog.zig");

pub const MAX_NAME: usize = 32;
pub const MAX_DRIVERS: usize = 16;

pub const State = enum(u8) {
    empty,
    loaded,
    initialized,
    active,
    shutdown,
    unloaded,
    failed,
    quarantined,
};

pub const Source = enum(u8) {
    builtin,
    r4d,
    preload,
};

pub const Entry = struct {
    used: bool = false,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    driver_type: u16 = 0,
    version: u32 = 0,
    source: Source = .builtin,
    state: State = .empty,
};

var entries: [MAX_DRIVERS]Entry = .{Entry{}} ** MAX_DRIVERS;

pub fn init() void {
    entries = .{Entry{}} ** MAX_DRIVERS;
}

pub fn beginLoad(name: []const u8, driver_type: u16, version: u32) ?usize {
    return beginLoadSource(name, driver_type, version, .builtin);
}

pub fn beginLoadR4d(name: []const u8, driver_type: u16, version: u32) ?usize {
    return beginLoadSource(name, driver_type, version, .r4d);
}

pub fn beginLoadPreloadR4d(name: []const u8, driver_type: u16, version: u32) ?usize {
    return beginLoadSource(name, driver_type, version, .preload);
}

fn beginLoadSource(name: []const u8, driver_type: u16, version: u32, source: Source) ?usize {
    const slot = freeSlot() orelse return null;
    const e = &entries[slot];
    e.* = .{
        .used = true,
        .driver_type = driver_type,
        .version = version,
        .source = source,
        .state = .loaded,
    };
    e.name_len = if (name.len < MAX_NAME) name.len else MAX_NAME - 1;
    if (e.name_len > 0) @memcpy(e.name[0..e.name_len], name[0..e.name_len]);
    logState(slot);
    return slot;
}

pub fn setState(slot: usize, state: State) void {
    if (slot >= entries.len or !entries[slot].used) return;
    entries[slot].state = state;
    logState(slot);
}

pub fn markUnloaded(slot: usize) void {
    setState(slot, .unloaded);
}

pub fn get(slot: usize) ?Entry {
    if (slot >= entries.len or !entries[slot].used) return null;
    return entries[slot];
}

pub fn countUsed() usize {
    var count: usize = 0;
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        if (entries[i].used) count += 1;
    }
    return count;
}

pub fn entryAt(index: usize) ?*const Entry {
    if (index >= entries.len or !entries[index].used) return null;
    return &entries[index];
}

pub fn findByName(name: []const u8) ?usize {
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used) continue;
        if (nameEq(e.name[0..e.name_len], name)) return i;
    }
    return null;
}

pub fn logRegistryToBootlog() void {
    bootlog.puts("[DRIVER] registry:\r\n");
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used) continue;
        bootlog.puts("[DRIVER]   ");
        bootlog.puts(e.name[0..e.name_len]);
        bootlog.puts(" src=");
        bootlog.puts(sourceName(e.source));
        bootlog.puts(" type=");
        bootlog.puts(typeName(e.driver_type));
        bootlog.puts(" v=");
        bootlog.putDec(e.version);
        bootlog.puts(" state=");
        bootlog.puts(stateName(e.state));
        bootlog.puts("\r\n");
    }
}

fn nameEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}

fn freeSlot() ?usize {
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        if (!entries[i].used or entries[i].state == .unloaded or entries[i].state == .failed) return i;
    }
    return null;
}

fn logState(slot: usize) void {
    const e = &entries[slot];
    bootlog.puts("[DRIVER] ");
    bootlog.puts(e.name[0..e.name_len]);
    bootlog.puts(" (");
    bootlog.puts(sourceName(e.source));
    bootlog.puts("/");
    bootlog.puts(typeName(e.driver_type));
    bootlog.puts(")");
    bootlog.puts(" -> ");
    bootlog.puts(stateName(e.state));
    bootlog.puts("\r\n");
}

pub fn stateName(state: State) []const u8 {
    return switch (state) {
        .empty => "EMPTY",
        .loaded => "GELADEN",
        .initialized => "INITIALISIERT",
        .active => "AKTIV",
        .shutdown => "SHUTDOWN",
        .unloaded => "ENTLADEN",
        .failed => "FAILED",
        .quarantined => "QUARANTINED",
    };
}

pub fn sourceName(source: Source) []const u8 {
    return switch (source) {
        .builtin => "builtin",
        .r4d => "r4d",
        .preload => "preload",
    };
}

pub fn typeName(driver_type: u16) []const u8 {
    return switch (driver_type) {
        1 => "audio",
        2 => "storage",
        3 => "input",
        4 => "synth",
        5 => "net",
        6 => "display",
        255 => "misc",
        else => "unknown",
    };
}
