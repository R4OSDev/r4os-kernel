const std = @import("std");

// These package companions are ordinary files, never executable components.
// The same scope must be understood by packer, updater and early recovery.
pub const CompanionKind = enum { license, source };
pub const companion_recovery_kernel = "0.1.199";

pub fn companionKind(path: []const u8) ?CompanionKind {
    const scopes = .{
        .{ "C:/R4OS/LICENSES/", CompanionKind.license },
        .{ "C:/R4OS/SOURCES/", CompanionKind.source },
    };
    inline for (scopes) |scope| {
        const prefix = scope[0];
        if (path.len > prefix.len and samePath(path[0..prefix.len], prefix)) {
            var parts = std.mem.tokenizeAny(u8, path[prefix.len..], "/\\");
            while (parts.next()) |part| {
                if (std.mem.eql(u8, part, ".") or std.mem.eql(u8, part, "..") or
                    part[part.len - 1] == '.' or part[part.len - 1] == ' ') return null;
                for (part) |byte| {
                    if (byte < 0x20 or byte >= 0x7f or
                        std.mem.indexOfScalar(u8, "\"*:;<>?|", byte) != null) return null;
                }
            }
            if (path[path.len - 1] == '/' or path[path.len - 1] == '\\') return null;
            return scope[1];
        }
    }
    return null;
}

fn samePath(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (a, b) |left, right| {
        const l = if (left == '\\') '/' else std.ascii.toUpper(left);
        const r = if (right == '\\') '/' else std.ascii.toUpper(right);
        if (l != r) return false;
    }
    return true;
}
