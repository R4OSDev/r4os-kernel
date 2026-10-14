const k = @import("../kernel/log.zig");

pub const MAX_NAME: usize = 32;
pub const MAX_ROLE: usize = 64;
pub const MAX_NOTE: usize = 96;
pub const MAX_PROTOCOLS: usize = 64;

pub const Source = enum(u8) {
    builtin,
    r4p,
    preload,
};

pub const State = enum(u8) {
    empty,
    loaded,
    active,
    fallback,
    blocked,
    err,
    disabled,
};

pub const Category = enum(u16) {
    net = 1,
    usb = 2,
    audio = 3,
    data = 4,
    misc = 255,
    unknown = 0,
};

pub const Entry = extern struct {
    used: bool = false,
    source: Source = .builtin,
    state: State = .empty,
    category: u16 = 0,
    version: u16 = 0,
    api_version: u32 = 0,
    last_error: i32 = 0,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    role: [MAX_ROLE]u8 = .{0} ** MAX_ROLE,
    role_len: usize = 0,
    note: [MAX_NOTE]u8 = .{0} ** MAX_NOTE,
    note_len: usize = 0,
};

const Registration = extern struct {
    name: [*]const u8,
    name_len: usize,
    role: [*]const u8,
    role_len: usize,
    note: [*]const u8,
    note_len: usize,
    category: u16,
    version: u16,
    api_version: u32,
};
comptime {
    if (@sizeOf(Entry) != 232 or @offsetOf(Entry, "name_len") != 48 or @offsetOf(Entry, "role_len") != 120 or
        @offsetOf(Entry, "note_len") != 224 or @sizeOf(Registration) != 56)
        @compileError("Native protocol registry C layout drift");
}
extern fn r4os_rust_protocol_registry_reset() void;
extern fn r4os_rust_protocol_registry_register(operation: u8, input: *const Registration) usize;
extern fn r4os_rust_protocol_registry_select(slot: usize, input: *const Registration) bool;
extern fn r4os_rust_protocol_registry_mutate(slot: usize, operation: u8, value: u32, err: i32, note: [*]const u8, len: usize) void;
extern fn r4os_rust_protocol_registry_entry(role: [*]const u8, len: usize, mode: u8, output: *Entry) bool;
extern fn r4os_rust_protocol_registry_get(slot: usize, output: *Entry) bool;
extern fn r4os_rust_protocol_registry_count() usize;
extern fn r4os_rust_protocol_registry_query(operation: u8, slot: usize, role: [*]const u8, len: usize) i32;
fn request(name: []const u8, role: []const u8, category: u16, version: u16, api_version: u32, note: []const u8) Registration {
    return .{ .name = name.ptr, .name_len = name.len, .role = role.ptr, .role_len = role.len, .note = note.ptr, .note_len = note.len, .category = category, .version = version, .api_version = api_version };
}
fn begin(operation: u8, input: Registration) ?usize {
    const slot = r4os_rust_protocol_registry_register(operation, &input);
    return if (slot == @import("std").math.maxInt(usize)) null else slot;
}
pub fn init() void {
    r4os_rust_protocol_registry_reset();
}
pub fn registerBuiltin(name: []const u8, role: []const u8, category_value: u16, note: []const u8) void {
    _ = begin(0, request(name, role, category_value, 1, 1, note));
}
pub fn beginLoadR4p(name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) ?usize {
    return begin(1, request(name, role, category_value, version, api_version, ""));
}
pub fn catalogR4p(name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) ?usize {
    return begin(2, request(name, role, category_value, version, api_version, ""));
}
pub fn selectCatalogR4p(slot: usize, name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) bool {
    const value = request(name, role, category_value, version, api_version, "");
    return r4os_rust_protocol_registry_select(slot, &value);
}
pub fn beginLoadPreload(name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) ?usize {
    return begin(3, request(name, role, category_value, version, api_version, ""));
}
pub fn setState(slot: usize, state: State) void {
    r4os_rust_protocol_registry_mutate(slot, 0, @intFromEnum(state), 0, "", 0);
}
pub fn setError(slot: usize, err: i32, note: []const u8) void {
    r4os_rust_protocol_registry_mutate(slot, 1, 0, err, note.ptr, note.len);
}
pub fn setBlocked(slot: usize, err: i32, note: []const u8) void {
    r4os_rust_protocol_registry_mutate(slot, 2, 0, err, note.ptr, note.len);
}
pub fn setNote(slot: usize, note: []const u8) void {
    r4os_rust_protocol_registry_mutate(slot, 3, 0, 0, note.ptr, note.len);
}
pub fn setStatus(slot: usize, state_value: u32, note: []const u8) void {
    r4os_rust_protocol_registry_mutate(slot, 4, state_value, 0, note.ptr, note.len);
}
pub fn activeRole(role: []const u8) bool {
    return r4os_rust_protocol_registry_query(0, 0, role.ptr, role.len) != 0;
}
fn entryForRole(role: []const u8, mode: u8) ?Entry {
    var entry: Entry = undefined;
    if (!r4os_rust_protocol_registry_entry(role.ptr, role.len, mode, &entry)) return null;
    return entry;
}
pub fn r4pEntryForRole(role: []const u8) ?Entry {
    return entryForRole(role, 1);
}
pub fn moduleEntryForRole(role: []const u8) ?Entry {
    return entryForRole(role, 1);
}
pub fn builtinEntryForRole(role: []const u8) ?Entry {
    return entryForRole(role, 0);
}
pub fn activeR4pRole(role: []const u8) bool {
    return r4os_rust_protocol_registry_query(1, 0, role.ptr, role.len) != 0;
}
pub fn dependencyStatus(role: []const u8) i32 {
    return r4os_rust_protocol_registry_query(2, 0, role.ptr, role.len);
}
pub fn roleMatches(slot: usize, role: []const u8) bool {
    return r4os_rust_protocol_registry_query(3, slot, role.ptr, role.len) != 0;
}
pub fn get(slot: usize) ?Entry {
    var entry: Entry = undefined;
    if (!r4os_rust_protocol_registry_get(slot, &entry)) return null;
    return entry;
}
pub fn entryAt(index: usize) ?Entry {
    return get(index);
}
pub fn countUsed() usize {
    return r4os_rust_protocol_registry_count();
}
pub fn dumpStatus() void {
    k.puts("Protocol registry\r\n");
    var i: usize = 0;
    var shown: usize = 0;
    while (i < MAX_PROTOCOLS) : (i += 1) {
        const e = get(i) orelse continue;
        shown += 1;
        k.puts("  ");
        k.puts(e.role[0..e.role_len]);
        k.puts(" name=");
        k.puts(e.name[0..e.name_len]);
        k.puts(" src=");
        k.puts(sourceName(e.source));
        k.puts(" cat=");
        k.puts(categoryName(e.category));
        k.puts(" state=");
        k.puts(stateName(e.state));
        if (e.last_error != 0) {
            k.puts(" err=");
            putSignedDec(e.last_error);
        }
        if (e.note_len > 0) {
            k.puts(" note=");
            k.puts(e.note[0..e.note_len]);
        }
        k.puts("\r\n");
    }
    if (shown == 0) k.puts("  none\r\n");
}

pub fn sourceName(source: Source) []const u8 {
    return switch (source) {
        .builtin => "builtin",
        .r4p => "r4p",
        .preload => "preload",
    };
}

pub fn stateName(state: State) []const u8 {
    return switch (state) {
        .empty => "empty",
        .loaded => "loaded",
        .active => "active",
        .fallback => "fallback",
        .blocked => "blocked",
        .err => "error",
        .disabled => "disabled",
    };
}

pub fn categoryName(category_value: u16) []const u8 {
    return switch (category_value) {
        1 => "net",
        2 => "usb",
        3 => "audio",
        4 => "data",
        255 => "misc",
        else => "unknown",
    };
}

pub fn categoryFromValue(value: u16) ?Category {
    return switch (value) {
        1 => .net,
        2 => .usb,
        3 => .audio,
        4 => .data,
        255 => .misc,
        else => null,
    };
}

fn putSignedDec(value: i32) void {
    if (value < 0) {
        k.puts("-");
        k.putDec(@intCast(-value));
    } else {
        k.putDec(@intCast(value));
    }
}
