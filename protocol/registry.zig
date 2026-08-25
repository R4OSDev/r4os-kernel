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

pub const Entry = struct {
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

var entries: [MAX_PROTOCOLS]Entry = .{Entry{}} ** MAX_PROTOCOLS;

pub fn init() void {
    entries = .{Entry{}} ** MAX_PROTOCOLS;
}

pub fn registerBuiltin(name: []const u8, role: []const u8, category_value: u16, note: []const u8) void {
    if (findBuiltinByRole(role) != null) return;
    const slot = freeSlot() orelse return;
    writeEntry(slot, .builtin, .fallback, name, role, category_value, 1, 1, 0, note);
}

pub fn beginLoadR4p(name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) ?usize {
    return beginLoadModule(.r4p, name, role, category_value, version, api_version);
}

pub fn catalogR4p(name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) ?usize {
    if (findModuleByRole(role) != null) return null;
    const slot = freeSlot() orelse return null;
    writeEntry(slot, .r4p, .loaded, name, role, category_value, version, api_version, 0, "installed; lazy");
    return slot;
}

pub fn selectCatalogR4p(slot: usize, name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) bool {
    if (slot >= entries.len or !entries[slot].used) return false;
    const entry = &entries[slot];
    if (entry.source != .r4p or !nameEq(entry.role[0..entry.role_len], role)) return false;
    entry.name = .{0} ** MAX_NAME;
    entry.name_len = copy(name, entry.name[0..]);
    entry.category = category_value;
    entry.version = version;
    entry.api_version = api_version;
    entry.last_error = 0;
    entry.state = .loaded;
    setNote(slot, "loading on demand");
    return true;
}

pub fn beginLoadPreload(name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) ?usize {
    return beginLoadModule(.preload, name, role, category_value, version, api_version);
}

fn beginLoadModule(source: Source, name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32) ?usize {
    if (findModuleByRole(role) != null) return null;
    const slot = freeSlot() orelse return null;
    writeEntry(slot, source, .loaded, name, role, category_value, version, api_version, 0, "loaded");
    return slot;
}

pub fn setState(slot: usize, state: State) void {
    if (slot >= entries.len or !entries[slot].used) return;
    entries[slot].state = state;
}

pub fn setError(slot: usize, err: i32, note: []const u8) void {
    if (slot >= entries.len or !entries[slot].used) return;
    entries[slot].last_error = err;
    entries[slot].state = .err;
    setNote(slot, note);
}

pub fn setBlocked(slot: usize, err: i32, note: []const u8) void {
    if (slot >= entries.len or !entries[slot].used) return;
    entries[slot].last_error = err;
    entries[slot].state = .blocked;
    setNote(slot, note);
}

pub fn setNote(slot: usize, note: []const u8) void {
    if (slot >= entries.len or !entries[slot].used) return;
    const e = &entries[slot];
    e.note = .{0} ** MAX_NOTE;
    e.note_len = copy(note, e.note[0..]);
}

pub fn setStatus(slot: usize, state_value: u32, note: []const u8) void {
    if (slot >= entries.len or !entries[slot].used) return;
    entries[slot].state = stateFromProtocolValue(state_value);
    setNote(slot, note);
}

pub fn activeRole(role: []const u8) bool {
    if (findRolePreferred(role)) |slot| {
        const state = entries[slot].state;
        return state == .active or state == .fallback;
    }
    return false;
}

pub fn r4pEntryForRole(role: []const u8) ?*const Entry {
    const slot = findModuleByRole(role) orelse return null;
    return &entries[slot];
}

pub fn moduleEntryForRole(role: []const u8) ?*const Entry {
    const slot = findModuleByRole(role) orelse return null;
    return &entries[slot];
}

pub fn builtinEntryForRole(role: []const u8) ?*const Entry {
    const slot = findBuiltinByRole(role) orelse return null;
    return &entries[slot];
}

pub fn activeR4pRole(role: []const u8) bool {
    const e = moduleEntryForRole(role) orelse return false;
    return e.state == .active;
}

pub fn dependencyStatus(role: []const u8) i32 {
    if (findRolePreferred(role)) |slot| {
        return switch (entries[slot].state) {
            .active, .fallback => 1,
            .loaded => 0,
            .blocked => -5,
            .err => -1,
            .disabled => -6,
            else => 0,
        };
    }
    return -5;
}

pub fn roleMatches(slot: usize, role: []const u8) bool {
    if (slot >= entries.len or !entries[slot].used) return false;
    const e = &entries[slot];
    return nameEq(e.role[0..e.role_len], role);
}

pub fn get(slot: usize) ?*const Entry {
    if (slot >= entries.len or !entries[slot].used) return null;
    return &entries[slot];
}

pub fn entryAt(index: usize) ?*const Entry {
    if (index >= entries.len or !entries[index].used) return null;
    return &entries[index];
}

pub fn countUsed() usize {
    var count: usize = 0;
    for (entries) |entry| {
        if (entry.used) count += 1;
    }
    return count;
}

pub fn dumpStatus() void {
    k.puts("Protocol registry\r\n");
    var i: usize = 0;
    var shown: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used) continue;
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

fn writeEntry(slot: usize, source: Source, state: State, name: []const u8, role: []const u8, category_value: u16, version: u16, api_version: u32, last_error: i32, note: []const u8) void {
    const e = &entries[slot];
    e.* = .{
        .used = true,
        .source = source,
        .state = state,
        .category = category_value,
        .version = version,
        .api_version = api_version,
        .last_error = last_error,
    };
    e.name_len = copy(name, e.name[0..]);
    e.role_len = copy(role, e.role[0..]);
    e.note_len = copy(note, e.note[0..]);
}

fn freeSlot() ?usize {
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        if (!entries[i].used) return i;
    }
    return null;
}

fn findBuiltinByRole(role: []const u8) ?usize {
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used or e.source != .builtin) continue;
        if (nameEq(e.role[0..e.role_len], role)) return i;
    }
    return null;
}

fn findR4pByRole(role: []const u8) ?usize {
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used or e.source != .r4p) continue;
        if (nameEq(e.role[0..e.role_len], role)) return i;
    }
    return null;
}

fn findModuleByRole(role: []const u8) ?usize {
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used or (e.source != .r4p and e.source != .preload)) continue;
        if (nameEq(e.role[0..e.role_len], role)) return i;
    }
    return null;
}

fn findRolePreferred(role: []const u8) ?usize {
    var fallback: ?usize = null;
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const e = &entries[i];
        if (!e.used or !nameEq(e.role[0..e.role_len], role)) continue;
        if ((e.source == .r4p or e.source == .preload) and e.state == .active) return i;
        if (fallback == null and (e.state == .active or e.state == .fallback)) fallback = i;
    }
    return fallback;
}

fn stateFromProtocolValue(value: u32) State {
    return switch (value) {
        1 => .loaded,
        2 => .active,
        3 => .fallback,
        4 => .blocked,
        5 => .err,
        6 => .disabled,
        else => .loaded,
    };
}

fn copy(src: []const u8, dst: []u8) usize {
    @memset(dst, 0);
    const len = @min(src.len, dst.len - 1);
    if (len > 0) @memcpy(dst[0..len], src[0..len]);
    return len;
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

fn putSignedDec(value: i32) void {
    if (value < 0) {
        k.puts("-");
        k.putDec(@intCast(-value));
    } else {
        k.putDec(@intCast(value));
    }
}
