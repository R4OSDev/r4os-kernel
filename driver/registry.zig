const bootlog = @import("../kernel/bootlog.zig");

pub const MAX_NAME: usize = 32;
// Full AMD images contain 17 R4D modules, alongside built-in owners.
// All per-driver runtime/resource tables derive their capacity here.
pub const MAX_DRIVERS: usize = 32;

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

// Native registry snapshots are caller-owned; no reusable slot pointer escapes.
const RawEntry = extern struct {
    used: bool,
    name: [MAX_NAME]u8,
    name_len: usize,
    driver_type: u16,
    version: u32,
    source: Source,
    state: State,
};
comptime {
    if (@sizeOf(RawEntry) != 64 or @offsetOf(RawEntry, "name_len") != 40 or
        @offsetOf(RawEntry, "version") != 52 or @offsetOf(RawEntry, "state") != 57)
        @compileError("Native driver registry C layout drift");
}
extern fn r4os_rust_driver_registry_reset() void;
extern fn r4os_rust_driver_registry_begin(name: [*]const u8, len: usize, kind: u16, version: u32, source: Source) usize;
extern fn r4os_rust_driver_registry_set_state(slot: usize, state: State) void;
extern fn r4os_rust_driver_registry_get(slot: usize, output: *RawEntry) bool;
extern fn r4os_rust_driver_registry_count() usize;
extern fn r4os_rust_driver_registry_find(name: [*]const u8, len: usize) usize;
pub fn init() void {
    r4os_rust_driver_registry_reset();
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
    const slot = r4os_rust_driver_registry_begin(name.ptr, name.len, driver_type, version, source);
    if (slot == @import("std").math.maxInt(usize)) return null;
    logState(slot);
    return slot;
}
pub fn setState(slot: usize, state: State) void {
    r4os_rust_driver_registry_set_state(slot, state);
    logState(slot);
}
pub fn markUnloaded(slot: usize) void {
    setState(slot, .unloaded);
}
pub fn get(slot: usize) ?Entry {
    var value: RawEntry = undefined;
    if (!r4os_rust_driver_registry_get(slot, &value)) return null;
    return .{ .used = value.used, .name = value.name, .name_len = value.name_len, .driver_type = value.driver_type, .version = value.version, .source = value.source, .state = value.state };
}
pub fn entryAt(index: usize) ?Entry {
    return get(index);
}
pub fn countUsed() usize {
    return r4os_rust_driver_registry_count();
}
pub fn findByName(name: []const u8) ?usize {
    const slot = r4os_rust_driver_registry_find(name.ptr, name.len);
    return if (slot == @import("std").math.maxInt(usize)) null else slot;
}
pub fn logRegistryToBootlog() void {
    bootlog.puts("[DRIVER] registry:\r\n");
    var i: usize = 0;
    while (i < MAX_DRIVERS) : (i += 1) {
        const e = get(i) orelse continue;
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

fn logState(slot: usize) void {
    const e = get(slot) orelse return;
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
