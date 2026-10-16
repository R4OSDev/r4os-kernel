pub const Layout = enum(u32) {
    en_en,
    de_de,
};

pub const KeyChar = extern struct {
    codepoint: u32 = 0,
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

pub const KeyEntry = extern struct {
    scancode: u8,
    normal: KeyChar = .{},
    shift: KeyChar = .{},
    altgr: KeyChar = .{},
    shift_altgr: KeyChar = .{},
};

pub const Text = extern struct {
    pointer: [*]const u8,
    len: usize,
    pub fn slice(self: Text) []const u8 {
        return self.pointer[0..self.len];
    }
};
pub const LayoutDefinition = extern struct { id: Layout, name: Text, display: Text, aliases: [*]const Text, alias_count: usize, entries: [*]const KeyEntry, entry_count: usize };
comptime {
    if (@sizeOf(KeyChar) != 8 or @alignOf(KeyChar) != 4 or @offsetOf(KeyChar, "ascii") != 4 or @sizeOf(KeyEntry) != 36 or @alignOf(KeyEntry) != 4 or @offsetOf(KeyEntry, "normal") != 4 or @offsetOf(KeyEntry, "shift") != 12 or @offsetOf(KeyEntry, "altgr") != 20 or @offsetOf(KeyEntry, "shift_altgr") != 28 or @sizeOf(LayoutDefinition) != 72 or @offsetOf(LayoutDefinition, "name") != 8 or @offsetOf(LayoutDefinition, "display") != 24 or @offsetOf(LayoutDefinition, "aliases") != 40 or @offsetOf(LayoutDefinition, "entries") != 56) @compileError("Native keyboard layout C records");
}
