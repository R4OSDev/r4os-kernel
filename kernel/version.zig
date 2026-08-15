const std = @import("std");

pub const metadata_section = ".r4os.kernel.meta";
pub const metadata_magic = [8]u8{ 'R', '4', 'O', 'S', 'K', 'R', 'N', '1' };
pub const metadata_format_version: u32 = 1;
pub const metadata_text_capacity: usize = 16;

pub const SemanticVersion = struct {
    major: u32,
    minor: u32,
    patch: u32,
};

pub const KernelMetadata = extern struct {
    magic: [8]u8,
    format_version: u32,
    size: u32,
    major: u32,
    minor: u32,
    patch: u32,
    version_text: [metadata_text_capacity]u8,
};

pub const current = parseVersionSource(@embedFile("../VERSION.R4S"));
pub const text = std.fmt.comptimePrint("{d}.{d}.{d}", .{ current.major, current.minor, current.patch });

pub export var r4os_kernel_metadata: KernelMetadata linksection(metadata_section) = .{
    .magic = metadata_magic,
    .format_version = metadata_format_version,
    .size = @sizeOf(KernelMetadata),
    .major = current.major,
    .minor = current.minor,
    .patch = current.patch,
    .version_text = metadataText(),
};

pub inline fn keepMetadata() void {
    const first_byte: *volatile const u8 = @ptrCast(&r4os_kernel_metadata.magic[0]);
    _ = first_byte.*;
}

fn metadataText() [metadata_text_capacity]u8 {
    if (text.len >= metadata_text_capacity) @compileError("Kernel version text exceeds ELF metadata capacity");
    var result = [_]u8{0} ** metadata_text_capacity;
    @memcpy(result[0..text.len], text);
    return result;
}

fn parseVersionSource(comptime source: []const u8) SemanticVersion {
    const without_bom = if (source.len >= 3 and source[0] == 0xEF and source[1] == 0xBB and source[2] == 0xBF)
        source[3..]
    else
        source;
    const line = std.mem.trim(u8, without_bom, " \t\r\n");
    const prefix = "KERNEL_VERSION=";
    if (!std.mem.startsWith(u8, line, prefix)) @compileError("Code/Kernel/VERSION.R4S must contain KERNEL_VERSION=<MAJOR.MINOR.PATCH>");
    const value = line[prefix.len..];
    if (value.len == 0 or std.mem.indexOfAny(u8, value, " \t\r\n") != null) @compileError("Kernel version must be one semantic version without whitespace");

    var parts = std.mem.splitScalar(u8, value, '.');
    const major_text = parts.next() orelse @compileError("Kernel version lacks major component");
    const minor_text = parts.next() orelse @compileError("Kernel version lacks minor component");
    const patch_text = parts.next() orelse @compileError("Kernel version lacks patch component");
    if (parts.next() != null) @compileError("Kernel version must have exactly three components");

    return .{
        .major = parseComponent(major_text),
        .minor = parseComponent(minor_text),
        .patch = parseComponent(patch_text),
    };
}

fn parseComponent(comptime value: []const u8) u32 {
    if (value.len == 0) @compileError("Kernel version contains an empty component");
    if (value.len > 1 and value[0] == '0') @compileError("Kernel version components must not have leading zeroes");
    for (value) |byte| {
        if (byte < '0' or byte > '9') @compileError("Kernel version components must be decimal numbers");
    }
    return std.fmt.parseInt(u32, value, 10) catch @compileError("Kernel version component exceeds u32");
}

comptime {
    if (@sizeOf(KernelMetadata) != 44 or @alignOf(KernelMetadata) != 4)
        @compileError("Kernel ELF metadata layout drift");
}
