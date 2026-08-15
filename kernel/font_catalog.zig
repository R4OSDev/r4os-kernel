// Runtime catalogue for installed R4F system fonts.
//
// The catalogue owns neither a persistent copy of the font files nor a
// second font format.  It scans C:\R4OS\FONTS, validates each file through
// kernel/font.zig and keeps only the bounded glyph cache used by R4DRAW.

const font = @import("font.zig");
const fs_request = @import("../fs/request.zig");
const vfs = @import("../fs/vfs.zig");

pub const MAX_R4F_FILE_BYTES: usize = 64 * 1024;

pub const ReloadResult = struct {
    scanned: u32 = 0,
    registered: u32 = 0,
    rejected: u32 = 0,
    unavailable: bool = false,
};

var file_buffer: [MAX_R4F_FILE_BYTES]u8 = undefined;

pub fn reloadInstalled() ReloadResult {
    var result = ReloadResult{};
    const volume = vfs.volumeForDrive('C') orelse {
        result.unavailable = true;
        return result;
    };

    var resolve_request = fs_request.begin(.file_read, 'C') orelse {
        result.unavailable = true;
        return result;
    };
    const directory = vfs.resolvePath(volume, "/R4OS/FONTS");
    fs_request.finish(&resolve_request, directory != null);
    const dir = directory orelse {
        result.unavailable = true;
        return result;
    };

    font.resetCatalog();
    var index: usize = 0;
    var name_buffer: [vfs.NAME_MAX]u8 = .{0} ** vfs.NAME_MAX;
    while (true) : (index += 1) {
        var entry_request = fs_request.begin(.file_read, 'C') orelse {
            result.unavailable = true;
            break;
        };
        const maybe_entry = vfs.readDirectoryEntry(volume, dir, index, name_buffer[0..]);
        fs_request.finish(&entry_request, maybe_entry != null);
        const entry = maybe_entry orelse break;
        const name = zName(name_buffer[0..]);
        if (entry.isDir() or !hasR4fExtension(name)) continue;
        result.scanned += 1;
        if (entry.size == 0 or entry.size > MAX_R4F_FILE_BYTES) {
            result.rejected += 1;
            continue;
        }

        var read_request = fs_request.begin(.file_read, 'C') orelse {
            result.unavailable = true;
            break;
        };
        const want: usize = @intCast(entry.size);
        const got = vfs.readFile(volume, entry, file_buffer[0..want]);
        const read_ok = got != null and got.? == want;
        fs_request.finish(&read_request, read_ok);
        if (!read_ok) {
            result.rejected += 1;
            continue;
        }

        var path_buffer: [font.MAX_FONT_PATH]u8 = .{0} ** font.MAX_FONT_PATH;
        const path = systemPath(name, path_buffer[0..]) orelse {
            result.rejected += 1;
            continue;
        };
        if (font.registerR4F(path, file_buffer[0..want])) {
            result.registered += 1;
        } else {
            result.rejected += 1;
        }
    }
    return result;
}

fn systemPath(name: []const u8, out: []u8) ?[]const u8 {
    const prefix = "C:\\R4OS\\FONTS\\";
    if (prefix.len + name.len > out.len) return null;
    @memcpy(out[0..prefix.len], prefix);
    @memcpy(out[prefix.len .. prefix.len + name.len], name);
    return out[0 .. prefix.len + name.len];
}

fn hasR4fExtension(name: []const u8) bool {
    return name.len >= 4 and
        asciiUpper(name[name.len - 4]) == '.' and
        asciiUpper(name[name.len - 3]) == 'R' and
        asciiUpper(name[name.len - 2]) == '4' and
        asciiUpper(name[name.len - 1]) == 'F';
}

fn zName(bytes: []const u8) []const u8 {
    var len: usize = 0;
    while (len < bytes.len and bytes[len] != 0) : (len += 1) {}
    return bytes[0..len];
}

fn asciiUpper(ch: u8) u8 {
    return if (ch >= 'a' and ch <= 'z') ch - 32 else ch;
}
