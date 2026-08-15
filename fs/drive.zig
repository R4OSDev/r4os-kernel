const k = @import("../kernel/log.zig");

const MAX_DRIVES: usize = 26;
pub const MAX_PATH: usize = 1024; // contract file_path_max_bytes + NUL (0.60.19)
pub const DRIVE_COUNT: usize = MAX_DRIVES;

pub const Kind = enum(u8) {
    none,
    ram,
    fat32,
    ntfs,
};

pub const Role = enum(u8) {
    none,
    system,
    data,
    ram,
};

pub const Drive = struct {
    mounted: bool = false,
    letter: u8 = 0,
    kind: Kind = .none,
    role: Role = .none,
    name: []const u8 = "",
    bytes: usize = 0,
    block_device_index: ?usize = null,
    cwd: [MAX_PATH]u8 = .{0} ** MAX_PATH,
    cwd_len: usize = 0,
};

var drives: [MAX_DRIVES]Drive = .{Drive{}} ** MAX_DRIVES;
var current_index: usize = letterToIndex('C');
var initialized = false;

pub fn init() void {
    drives = .{Drive{}} ** MAX_DRIVES;
    current_index = letterToIndex('C');
    initialized = true;
}

pub fn mount(letter: u8, kind: Kind, name: []const u8, bytes: usize) bool {
    return mountWithBlock(letter, kind, name, bytes, null);
}

pub fn mountBlock(letter: u8, kind: Kind, name: []const u8, bytes: usize, block_device_index: usize) bool {
    return mountWithBlock(letter, kind, name, bytes, block_device_index);
}

pub fn mountBlockRole(letter: u8, kind: Kind, role: Role, name: []const u8, bytes: usize, block_device_index: usize) bool {
    return mountWithBlockRole(letter, kind, role, name, bytes, block_device_index);
}

fn mountWithBlock(letter: u8, kind: Kind, name: []const u8, bytes: usize, block_device_index: ?usize) bool {
    return mountWithBlockRole(letter, kind, roleForLetter(upperLetter(letter), kind), name, bytes, block_device_index);
}

fn mountWithBlockRole(letter: u8, kind: Kind, role: Role, name: []const u8, bytes: usize, block_device_index: ?usize) bool {
    if (!initialized) init();
    const upper = upperLetter(letter);
    if (upper < 'A' or upper > 'Z') return false;

    const index = letterToIndex(upper);
    drives[index] = .{
        .mounted = true,
        .letter = upper,
        .kind = kind,
        .role = role,
        .name = name,
        .bytes = bytes,
        .block_device_index = block_device_index,
        .cwd = .{0} ** MAX_PATH,
        .cwd_len = 1,
    };
    drives[index].cwd[0] = '\\';
    if (!drives[current_index].mounted) current_index = index;
    return true;
}

pub fn setCurrent(letter: u8) bool {
    const upper = upperLetter(letter);
    if (upper < 'A' or upper > 'Z') return false;

    const index = letterToIndex(upper);
    if (!drives[index].mounted) return false;
    current_index = index;
    return true;
}

pub fn current() ?*Drive {
    if (!initialized or !drives[current_index].mounted) return null;
    return &drives[current_index];
}

pub fn get(letter: u8) ?*Drive {
    const upper = upperLetter(letter);
    if (upper < 'A' or upper > 'Z') return null;
    const index = letterToIndex(upper);
    if (!initialized or !drives[index].mounted) return null;
    return &drives[index];
}

pub fn atIndex(index: usize) ?*const Drive {
    if (!initialized or index >= drives.len) return null;
    if (!drives[index].mounted) return null;
    return &drives[index];
}

pub fn currentLetter() u8 {
    if (!initialized or !drives[current_index].mounted) return 0;
    return drives[current_index].letter;
}

pub fn setCwdRoot(d: *Drive) void {
    d.cwd = .{0} ** MAX_PATH;
    d.cwd[0] = '\\';
    d.cwd_len = 1;
}

pub fn setCwdPath(d: *Drive, path: []const u8) bool {
    if (path.len == 0 or path[0] != '\\') return false;
    if (path.len >= MAX_PATH) return false;
    d.cwd = .{0} ** MAX_PATH;
    var i: usize = 0;
    while (i < path.len) : (i += 1) {
        d.cwd[i] = upperLetter(path[i]);
    }
    d.cwd_len = path.len;
    return true;
}

pub fn appendCwd(d: *Drive, segment: []const u8) bool {
    if (segment.len == 0) return true;

    var needed = d.cwd_len + segment.len;
    if (d.cwd_len > 1) needed += 1;
    if (needed >= MAX_PATH) return false;

    if (d.cwd_len > 1) {
        d.cwd[d.cwd_len] = '\\';
        d.cwd_len += 1;
    }

    var i: usize = 0;
    while (i < segment.len) : (i += 1) {
        d.cwd[d.cwd_len] = upperLetter(segment[i]);
        d.cwd_len += 1;
    }
    d.cwd[d.cwd_len] = 0;
    return true;
}

pub fn popCwd(d: *Drive) void {
    if (d.cwd_len <= 1) return;
    var end = d.cwd_len;
    while (end > 1 and d.cwd[end - 1] != '\\') : (end -= 1) {}
    d.cwd_len = if (end <= 1) 1 else end - 1;
    d.cwd[d.cwd_len] = 0;
}

pub fn printMounted() void {
    k.puts("  Drives:\r\n");
    var i: usize = 0;
    while (i < drives.len) : (i += 1) {
        const d = &drives[i];
        if (!d.mounted) continue;

        k.puts("    ");
        k.putc(d.letter);
        k.puts(": ");
        k.puts(kindName(d.kind));
        k.puts(" ");
        k.puts(roleName(d.role));
        k.puts(" ");
        k.puts(d.name);
        k.puts(" ");
        k.putDec(d.bytes / 1024);
        k.puts(" KB");
        if (d.block_device_index) |block_index| {
            k.puts(" block=#");
            k.putDec(block_index);
        }
        k.puts("\r\n");
    }
}

pub fn printLettersForBlock(block_device_index: usize) void {
    var printed = false;
    var i: usize = 0;
    while (i < drives.len) : (i += 1) {
        const d = &drives[i];
        if (!d.mounted) continue;
        if (d.block_device_index == null or d.block_device_index.? != block_device_index) continue;
        if (printed) k.puts(",");
        k.putc(d.letter);
        k.puts(":");
        printed = true;
    }
    if (!printed) k.puts("none");
}

pub fn printPrompt() void {
    if (current()) |d| {
        k.putc(d.letter);
        k.puts(":");
        k.puts(d.cwd[0..d.cwd_len]);
        k.puts(">");
    } else {
        k.puts("?:\\>");
    }
}

pub fn kindName(kind: Kind) []const u8 {
    return switch (kind) {
        .none => "NONE",
        .ram => "RAM",
        .fat32 => "FAT32",
        .ntfs => "NTFS",
    };
}

pub fn roleName(role: Role) []const u8 {
    return switch (role) {
        .none => "general",
        .system => "system",
        .data => "data",
        .ram => "ram",
    };
}

fn roleForLetter(letter: u8, kind: Kind) Role {
    if (letter == 'C' and kind == .fat32) return .system;
    if (letter == 'D' and kind == .fat32) return .data;
    return .none;
}

fn letterToIndex(letter: u8) usize {
    return @as(usize, upperLetter(letter) - 'A');
}

fn upperLetter(letter: u8) u8 {
    if (letter >= 'a' and letter <= 'z') return letter - ('a' - 'A');
    return letter;
}
