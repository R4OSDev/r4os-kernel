// Private stable C view of the single native bootstrap drive owner.
const k = @import("../kernel/log.zig");
pub const MAX_PATH: usize = 1024;
pub const DRIVE_COUNT: usize = 26;
pub const Kind = enum(u8) { none, ram, fat32, ntfs };
pub const Role = enum(u8) { none, system, data, ram };
pub const Label = extern struct {
    data: [*]const u8 = "".ptr,
    len: usize = 0,
    pub fn text(self: Label) []const u8 {
        return self.data[0..self.len];
    }
};
pub const Drive = extern struct {
    mounted: bool = false,
    letter: u8 = 0,
    kind: Kind = .none,
    role: Role = .none,
    name: Label = .{},
    bytes: usize = 0,
    block_index: usize = 0,
    has_block: bool = false,
    cwd: [MAX_PATH]u8 = .{0} ** MAX_PATH,
    cwd_len: usize = 0,
    pub fn blockDeviceIndex(self: *const Drive) ?usize {
        return if (self.has_block) self.block_index else null;
    }
};
extern fn r4os_rust_fs_drive_control(u8, u8) usize;
extern fn r4os_rust_fs_drive_lookup(u8, usize) ?*Drive;
extern fn r4os_rust_fs_drive_mount(u8, u8, u8, bool, [*]const u8, usize, usize, usize, bool) bool;
extern fn r4os_rust_fs_drive_cwd(*Drive, u8, ?[*]const u8, usize) bool;
extern fn r4os_rust_fs_drive_print(u8, usize) void;
extern fn r4os_rust_fs_drive_label(bool, u8, *Label) void;

pub fn init() void {
    _ = r4os_rust_fs_drive_control(0, 0);
}
pub fn mount(letter: u8, kind: Kind, name: []const u8, bytes: usize) bool {
    return r4os_rust_fs_drive_mount(letter, @intFromEnum(kind), 0, true, name.ptr, name.len, bytes, 0, false);
}
pub fn mountBlock(letter: u8, kind: Kind, name: []const u8, bytes: usize, block_device_index: usize) bool {
    return r4os_rust_fs_drive_mount(letter, @intFromEnum(kind), 0, true, name.ptr, name.len, bytes, block_device_index, true);
}
pub fn mountBlockRole(letter: u8, kind: Kind, role: Role, name: []const u8, bytes: usize, block_device_index: usize) bool {
    return r4os_rust_fs_drive_mount(letter, @intFromEnum(kind), @intFromEnum(role), false, name.ptr, name.len, bytes, block_device_index, true);
}
pub fn setCurrent(letter: u8) bool {
    return r4os_rust_fs_drive_control(1, letter) != 0;
}
pub fn unmountLocked(letter: u8) void {
    _ = r4os_rust_fs_drive_control(2, letter);
}
pub fn current() ?*Drive {
    return r4os_rust_fs_drive_lookup(0, 0);
}
pub fn get(letter: u8) ?*Drive {
    return r4os_rust_fs_drive_lookup(1, letter);
}
pub fn atIndex(index: usize) ?*const Drive {
    return r4os_rust_fs_drive_lookup(2, index);
}
pub fn currentLetter() u8 {
    return @intCast(r4os_rust_fs_drive_control(3, 0));
}
pub fn setCwdRoot(d: *Drive) void {
    _ = r4os_rust_fs_drive_cwd(d, 0, null, 0);
}
pub fn setCwdPath(d: *Drive, path: []const u8) bool {
    return r4os_rust_fs_drive_cwd(d, 1, path.ptr, path.len);
}
pub fn appendCwd(d: *Drive, segment: []const u8) bool {
    return r4os_rust_fs_drive_cwd(d, 2, segment.ptr, segment.len);
}
pub fn popCwd(d: *Drive) void {
    _ = r4os_rust_fs_drive_cwd(d, 3, null, 0);
}
pub fn printMounted() void {
    r4os_rust_fs_drive_print(0, 0);
}
pub fn printLettersForBlock(block_device_index: usize) void {
    r4os_rust_fs_drive_print(1, block_device_index);
}
pub fn printPrompt() void {
    r4os_rust_fs_drive_print(2, 0);
}
pub fn kindName(kind: Kind) []const u8 {
    var result: Label = undefined;
    r4os_rust_fs_drive_label(false, @intFromEnum(kind), &result);
    return result.text();
}
pub fn roleName(role: Role) []const u8 {
    var result: Label = undefined;
    r4os_rust_fs_drive_label(true, @intFromEnum(role), &result);
    return result.text();
}
export fn r4os_zig_fs_drive_text(data: [*]const u8, len: usize) void {
    k.puts(data[0..len]);
}
export fn r4os_zig_fs_drive_decimal(value: usize) void {
    k.putDec(value);
}
comptime {
    if (@sizeOf(Label) != 16 or @offsetOf(Drive, "name") != 8 or
        @offsetOf(Drive, "has_block") != 40 or @offsetOf(Drive, "cwd") != 41 or
        @offsetOf(Drive, "cwd_len") != 1072 or @sizeOf(Drive) != 1080)
        @compileError("Private native bootstrap-drive layout changed");
}
