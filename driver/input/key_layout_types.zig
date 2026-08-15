pub const Layout = enum {
    en_en,
    de_de,
};

pub const KeyChar = struct {
    codepoint: u21 = 0,
    ascii: u8 = 0,

    pub fn none() KeyChar {
        return .{};
    }

    pub fn asciiChar(ch: u8) KeyChar {
        return .{ .codepoint = ch, .ascii = ch };
    }

    pub fn unicode(cp: u21) KeyChar {
        return .{ .codepoint = cp };
    }

    pub fn unicodeAscii(cp: u21, ascii_fallback: u8) KeyChar {
        return .{ .codepoint = cp, .ascii = ascii_fallback };
    }

    pub fn hasCharacter(self: KeyChar) bool {
        return self.codepoint != 0 or self.ascii != 0;
    }

    pub fn canEmitLegacyAscii(self: KeyChar) bool {
        return self.ascii >= 0x20 and self.ascii < 0x80;
    }
};

pub const KeyEntry = struct {
    scancode: u8,
    normal: KeyChar = .{},
    shift: KeyChar = .{},
    altgr: KeyChar = .{},
    shift_altgr: KeyChar = .{},
};

pub const LayoutDefinition = struct {
    id: Layout,
    name: []const u8,
    display: []const u8,
    aliases: []const []const u8,
    entries: []const KeyEntry,
};
