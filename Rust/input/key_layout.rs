//! Canonical immutable keyboard layouts and original name/translation policy.
use core::{ptr, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Text {
    pub pointer: *const u8,
    pub len: usize,
}
unsafe impl Sync for Text {}
impl Text {
    const fn literal(v: &'static [u8]) -> Self {
        Self {
            pointer: v.as_ptr(),
            len: v.len(),
        }
    }
    fn bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.pointer, self.len) }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KeyChar {
    pub codepoint: u32,
    pub ascii: u8,
}
impl KeyChar {
    pub const EMPTY: Self = Self {
        codepoint: 0,
        ascii: 0,
    };
    pub fn has_character(self) -> bool {
        self.codepoint != 0 || self.ascii != 0
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KeyEntry {
    pub scancode: u8,
    pub normal: KeyChar,
    pub shift: KeyChar,
    pub altgr: KeyChar,
    pub shift_altgr: KeyChar,
}
#[repr(C)]
pub struct Definition {
    pub id: u32,
    pub name: Text,
    pub display: Text,
    pub aliases: *const Text,
    pub alias_count: usize,
    pub entries: *const KeyEntry,
    pub entry_count: usize,
}
// Immutable kernel data: every pointed-to array is static and never mutated.
unsafe impl Sync for Definition {}
static EN_EN_ALIASES: [Text; 7] = [
    Text::literal(b"EN"),
    Text::literal(b"EN_EN"),
    Text::literal(b"EN-US"),
    Text::literal(b"EN_US"),
    Text::literal(b"US"),
    Text::literal(b"USA"),
    Text::literal(b"QWERTY"),
];
static EN_EN_ENTRIES: [KeyEntry; 53] = [
    KeyEntry {
        scancode: 1,
        normal: KeyChar {
            codepoint: 27,
            ascii: 27,
        },
        shift: KeyChar {
            codepoint: 27,
            ascii: 27,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 2,
        normal: KeyChar {
            codepoint: 49,
            ascii: 49,
        },
        shift: KeyChar {
            codepoint: 33,
            ascii: 33,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 3,
        normal: KeyChar {
            codepoint: 50,
            ascii: 50,
        },
        shift: KeyChar {
            codepoint: 64,
            ascii: 64,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 4,
        normal: KeyChar {
            codepoint: 51,
            ascii: 51,
        },
        shift: KeyChar {
            codepoint: 35,
            ascii: 35,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 5,
        normal: KeyChar {
            codepoint: 52,
            ascii: 52,
        },
        shift: KeyChar {
            codepoint: 36,
            ascii: 36,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 6,
        normal: KeyChar {
            codepoint: 53,
            ascii: 53,
        },
        shift: KeyChar {
            codepoint: 37,
            ascii: 37,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 7,
        normal: KeyChar {
            codepoint: 54,
            ascii: 54,
        },
        shift: KeyChar {
            codepoint: 94,
            ascii: 94,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 8,
        normal: KeyChar {
            codepoint: 55,
            ascii: 55,
        },
        shift: KeyChar {
            codepoint: 38,
            ascii: 38,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 9,
        normal: KeyChar {
            codepoint: 56,
            ascii: 56,
        },
        shift: KeyChar {
            codepoint: 42,
            ascii: 42,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 10,
        normal: KeyChar {
            codepoint: 57,
            ascii: 57,
        },
        shift: KeyChar {
            codepoint: 40,
            ascii: 40,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 11,
        normal: KeyChar {
            codepoint: 48,
            ascii: 48,
        },
        shift: KeyChar {
            codepoint: 41,
            ascii: 41,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 12,
        normal: KeyChar {
            codepoint: 45,
            ascii: 45,
        },
        shift: KeyChar {
            codepoint: 95,
            ascii: 95,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 13,
        normal: KeyChar {
            codepoint: 61,
            ascii: 61,
        },
        shift: KeyChar {
            codepoint: 43,
            ascii: 43,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 14,
        normal: KeyChar {
            codepoint: 8,
            ascii: 8,
        },
        shift: KeyChar {
            codepoint: 8,
            ascii: 8,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 15,
        normal: KeyChar {
            codepoint: 9,
            ascii: 9,
        },
        shift: KeyChar {
            codepoint: 9,
            ascii: 9,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 16,
        normal: KeyChar {
            codepoint: 113,
            ascii: 113,
        },
        shift: KeyChar {
            codepoint: 81,
            ascii: 81,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 17,
        normal: KeyChar {
            codepoint: 119,
            ascii: 119,
        },
        shift: KeyChar {
            codepoint: 87,
            ascii: 87,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 18,
        normal: KeyChar {
            codepoint: 101,
            ascii: 101,
        },
        shift: KeyChar {
            codepoint: 69,
            ascii: 69,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 19,
        normal: KeyChar {
            codepoint: 114,
            ascii: 114,
        },
        shift: KeyChar {
            codepoint: 82,
            ascii: 82,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 20,
        normal: KeyChar {
            codepoint: 116,
            ascii: 116,
        },
        shift: KeyChar {
            codepoint: 84,
            ascii: 84,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 21,
        normal: KeyChar {
            codepoint: 121,
            ascii: 121,
        },
        shift: KeyChar {
            codepoint: 89,
            ascii: 89,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 22,
        normal: KeyChar {
            codepoint: 117,
            ascii: 117,
        },
        shift: KeyChar {
            codepoint: 85,
            ascii: 85,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 23,
        normal: KeyChar {
            codepoint: 105,
            ascii: 105,
        },
        shift: KeyChar {
            codepoint: 73,
            ascii: 73,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 24,
        normal: KeyChar {
            codepoint: 111,
            ascii: 111,
        },
        shift: KeyChar {
            codepoint: 79,
            ascii: 79,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 25,
        normal: KeyChar {
            codepoint: 112,
            ascii: 112,
        },
        shift: KeyChar {
            codepoint: 80,
            ascii: 80,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 26,
        normal: KeyChar {
            codepoint: 91,
            ascii: 91,
        },
        shift: KeyChar {
            codepoint: 123,
            ascii: 123,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 27,
        normal: KeyChar {
            codepoint: 93,
            ascii: 93,
        },
        shift: KeyChar {
            codepoint: 125,
            ascii: 125,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 28,
        normal: KeyChar {
            codepoint: 10,
            ascii: 10,
        },
        shift: KeyChar {
            codepoint: 10,
            ascii: 10,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 30,
        normal: KeyChar {
            codepoint: 97,
            ascii: 97,
        },
        shift: KeyChar {
            codepoint: 65,
            ascii: 65,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 31,
        normal: KeyChar {
            codepoint: 115,
            ascii: 115,
        },
        shift: KeyChar {
            codepoint: 83,
            ascii: 83,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 32,
        normal: KeyChar {
            codepoint: 100,
            ascii: 100,
        },
        shift: KeyChar {
            codepoint: 68,
            ascii: 68,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 33,
        normal: KeyChar {
            codepoint: 102,
            ascii: 102,
        },
        shift: KeyChar {
            codepoint: 70,
            ascii: 70,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 34,
        normal: KeyChar {
            codepoint: 103,
            ascii: 103,
        },
        shift: KeyChar {
            codepoint: 71,
            ascii: 71,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 35,
        normal: KeyChar {
            codepoint: 104,
            ascii: 104,
        },
        shift: KeyChar {
            codepoint: 72,
            ascii: 72,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 36,
        normal: KeyChar {
            codepoint: 106,
            ascii: 106,
        },
        shift: KeyChar {
            codepoint: 74,
            ascii: 74,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 37,
        normal: KeyChar {
            codepoint: 107,
            ascii: 107,
        },
        shift: KeyChar {
            codepoint: 75,
            ascii: 75,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 38,
        normal: KeyChar {
            codepoint: 108,
            ascii: 108,
        },
        shift: KeyChar {
            codepoint: 76,
            ascii: 76,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 39,
        normal: KeyChar {
            codepoint: 59,
            ascii: 59,
        },
        shift: KeyChar {
            codepoint: 58,
            ascii: 58,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 40,
        normal: KeyChar {
            codepoint: 39,
            ascii: 39,
        },
        shift: KeyChar {
            codepoint: 34,
            ascii: 34,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 41,
        normal: KeyChar {
            codepoint: 96,
            ascii: 96,
        },
        shift: KeyChar {
            codepoint: 126,
            ascii: 126,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 43,
        normal: KeyChar {
            codepoint: 92,
            ascii: 92,
        },
        shift: KeyChar {
            codepoint: 124,
            ascii: 124,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 44,
        normal: KeyChar {
            codepoint: 122,
            ascii: 122,
        },
        shift: KeyChar {
            codepoint: 90,
            ascii: 90,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 45,
        normal: KeyChar {
            codepoint: 120,
            ascii: 120,
        },
        shift: KeyChar {
            codepoint: 88,
            ascii: 88,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 46,
        normal: KeyChar {
            codepoint: 99,
            ascii: 99,
        },
        shift: KeyChar {
            codepoint: 67,
            ascii: 67,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 47,
        normal: KeyChar {
            codepoint: 118,
            ascii: 118,
        },
        shift: KeyChar {
            codepoint: 86,
            ascii: 86,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 48,
        normal: KeyChar {
            codepoint: 98,
            ascii: 98,
        },
        shift: KeyChar {
            codepoint: 66,
            ascii: 66,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 49,
        normal: KeyChar {
            codepoint: 110,
            ascii: 110,
        },
        shift: KeyChar {
            codepoint: 78,
            ascii: 78,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 50,
        normal: KeyChar {
            codepoint: 109,
            ascii: 109,
        },
        shift: KeyChar {
            codepoint: 77,
            ascii: 77,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 51,
        normal: KeyChar {
            codepoint: 44,
            ascii: 44,
        },
        shift: KeyChar {
            codepoint: 60,
            ascii: 60,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 52,
        normal: KeyChar {
            codepoint: 46,
            ascii: 46,
        },
        shift: KeyChar {
            codepoint: 62,
            ascii: 62,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 53,
        normal: KeyChar {
            codepoint: 47,
            ascii: 47,
        },
        shift: KeyChar {
            codepoint: 63,
            ascii: 63,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 57,
        normal: KeyChar {
            codepoint: 32,
            ascii: 32,
        },
        shift: KeyChar {
            codepoint: 32,
            ascii: 32,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 86,
        normal: KeyChar {
            codepoint: 92,
            ascii: 92,
        },
        shift: KeyChar {
            codepoint: 124,
            ascii: 124,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
];
static EN_EN: Definition = Definition {
    id: 0,
    name: Text::literal(b"en_en"),
    display: Text::literal(b"EN"),
    aliases: EN_EN_ALIASES.as_ptr(),
    alias_count: 7,
    entries: EN_EN_ENTRIES.as_ptr(),
    entry_count: 53,
};
static DE_DE_ALIASES: [Text; 6] = [
    Text::literal(b"DE"),
    Text::literal(b"DE_DE"),
    Text::literal(b"DE-DE"),
    Text::literal(b"GERMAN"),
    Text::literal(b"GERMANY"),
    Text::literal(b"QWERTZ"),
];
static DE_DE_ENTRIES: [KeyEntry; 53] = [
    KeyEntry {
        scancode: 1,
        normal: KeyChar {
            codepoint: 27,
            ascii: 27,
        },
        shift: KeyChar {
            codepoint: 27,
            ascii: 27,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 2,
        normal: KeyChar {
            codepoint: 49,
            ascii: 49,
        },
        shift: KeyChar {
            codepoint: 33,
            ascii: 33,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 3,
        normal: KeyChar {
            codepoint: 50,
            ascii: 50,
        },
        shift: KeyChar {
            codepoint: 34,
            ascii: 34,
        },
        altgr: KeyChar {
            codepoint: 178,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 4,
        normal: KeyChar {
            codepoint: 51,
            ascii: 51,
        },
        shift: KeyChar {
            codepoint: 167,
            ascii: 0,
        },
        altgr: KeyChar {
            codepoint: 179,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 5,
        normal: KeyChar {
            codepoint: 52,
            ascii: 52,
        },
        shift: KeyChar {
            codepoint: 36,
            ascii: 36,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 6,
        normal: KeyChar {
            codepoint: 53,
            ascii: 53,
        },
        shift: KeyChar {
            codepoint: 37,
            ascii: 37,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 7,
        normal: KeyChar {
            codepoint: 54,
            ascii: 54,
        },
        shift: KeyChar {
            codepoint: 38,
            ascii: 38,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 8,
        normal: KeyChar {
            codepoint: 55,
            ascii: 55,
        },
        shift: KeyChar {
            codepoint: 47,
            ascii: 47,
        },
        altgr: KeyChar {
            codepoint: 123,
            ascii: 123,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 9,
        normal: KeyChar {
            codepoint: 56,
            ascii: 56,
        },
        shift: KeyChar {
            codepoint: 40,
            ascii: 40,
        },
        altgr: KeyChar {
            codepoint: 91,
            ascii: 91,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 10,
        normal: KeyChar {
            codepoint: 57,
            ascii: 57,
        },
        shift: KeyChar {
            codepoint: 41,
            ascii: 41,
        },
        altgr: KeyChar {
            codepoint: 93,
            ascii: 93,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 11,
        normal: KeyChar {
            codepoint: 48,
            ascii: 48,
        },
        shift: KeyChar {
            codepoint: 61,
            ascii: 61,
        },
        altgr: KeyChar {
            codepoint: 125,
            ascii: 125,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 12,
        normal: KeyChar {
            codepoint: 223,
            ascii: 0,
        },
        shift: KeyChar {
            codepoint: 63,
            ascii: 63,
        },
        altgr: KeyChar {
            codepoint: 92,
            ascii: 92,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 13,
        normal: KeyChar {
            codepoint: 180,
            ascii: 39,
        },
        shift: KeyChar {
            codepoint: 96,
            ascii: 96,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 14,
        normal: KeyChar {
            codepoint: 8,
            ascii: 8,
        },
        shift: KeyChar {
            codepoint: 8,
            ascii: 8,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 15,
        normal: KeyChar {
            codepoint: 9,
            ascii: 9,
        },
        shift: KeyChar {
            codepoint: 9,
            ascii: 9,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 16,
        normal: KeyChar {
            codepoint: 113,
            ascii: 113,
        },
        shift: KeyChar {
            codepoint: 81,
            ascii: 81,
        },
        altgr: KeyChar {
            codepoint: 64,
            ascii: 64,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 17,
        normal: KeyChar {
            codepoint: 119,
            ascii: 119,
        },
        shift: KeyChar {
            codepoint: 87,
            ascii: 87,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 18,
        normal: KeyChar {
            codepoint: 101,
            ascii: 101,
        },
        shift: KeyChar {
            codepoint: 69,
            ascii: 69,
        },
        altgr: KeyChar {
            codepoint: 8364,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 19,
        normal: KeyChar {
            codepoint: 114,
            ascii: 114,
        },
        shift: KeyChar {
            codepoint: 82,
            ascii: 82,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 20,
        normal: KeyChar {
            codepoint: 116,
            ascii: 116,
        },
        shift: KeyChar {
            codepoint: 84,
            ascii: 84,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 21,
        normal: KeyChar {
            codepoint: 122,
            ascii: 122,
        },
        shift: KeyChar {
            codepoint: 90,
            ascii: 90,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 22,
        normal: KeyChar {
            codepoint: 117,
            ascii: 117,
        },
        shift: KeyChar {
            codepoint: 85,
            ascii: 85,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 23,
        normal: KeyChar {
            codepoint: 105,
            ascii: 105,
        },
        shift: KeyChar {
            codepoint: 73,
            ascii: 73,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 24,
        normal: KeyChar {
            codepoint: 111,
            ascii: 111,
        },
        shift: KeyChar {
            codepoint: 79,
            ascii: 79,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 25,
        normal: KeyChar {
            codepoint: 112,
            ascii: 112,
        },
        shift: KeyChar {
            codepoint: 80,
            ascii: 80,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 26,
        normal: KeyChar {
            codepoint: 252,
            ascii: 0,
        },
        shift: KeyChar {
            codepoint: 220,
            ascii: 0,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 27,
        normal: KeyChar {
            codepoint: 43,
            ascii: 43,
        },
        shift: KeyChar {
            codepoint: 42,
            ascii: 42,
        },
        altgr: KeyChar {
            codepoint: 126,
            ascii: 126,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 28,
        normal: KeyChar {
            codepoint: 10,
            ascii: 10,
        },
        shift: KeyChar {
            codepoint: 10,
            ascii: 10,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 30,
        normal: KeyChar {
            codepoint: 97,
            ascii: 97,
        },
        shift: KeyChar {
            codepoint: 65,
            ascii: 65,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 31,
        normal: KeyChar {
            codepoint: 115,
            ascii: 115,
        },
        shift: KeyChar {
            codepoint: 83,
            ascii: 83,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 32,
        normal: KeyChar {
            codepoint: 100,
            ascii: 100,
        },
        shift: KeyChar {
            codepoint: 68,
            ascii: 68,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 33,
        normal: KeyChar {
            codepoint: 102,
            ascii: 102,
        },
        shift: KeyChar {
            codepoint: 70,
            ascii: 70,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 34,
        normal: KeyChar {
            codepoint: 103,
            ascii: 103,
        },
        shift: KeyChar {
            codepoint: 71,
            ascii: 71,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 35,
        normal: KeyChar {
            codepoint: 104,
            ascii: 104,
        },
        shift: KeyChar {
            codepoint: 72,
            ascii: 72,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 36,
        normal: KeyChar {
            codepoint: 106,
            ascii: 106,
        },
        shift: KeyChar {
            codepoint: 74,
            ascii: 74,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 37,
        normal: KeyChar {
            codepoint: 107,
            ascii: 107,
        },
        shift: KeyChar {
            codepoint: 75,
            ascii: 75,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 38,
        normal: KeyChar {
            codepoint: 108,
            ascii: 108,
        },
        shift: KeyChar {
            codepoint: 76,
            ascii: 76,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 39,
        normal: KeyChar {
            codepoint: 246,
            ascii: 0,
        },
        shift: KeyChar {
            codepoint: 214,
            ascii: 0,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 40,
        normal: KeyChar {
            codepoint: 228,
            ascii: 0,
        },
        shift: KeyChar {
            codepoint: 196,
            ascii: 0,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 41,
        normal: KeyChar {
            codepoint: 94,
            ascii: 94,
        },
        shift: KeyChar {
            codepoint: 176,
            ascii: 0,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 43,
        normal: KeyChar {
            codepoint: 35,
            ascii: 35,
        },
        shift: KeyChar {
            codepoint: 39,
            ascii: 39,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 44,
        normal: KeyChar {
            codepoint: 121,
            ascii: 121,
        },
        shift: KeyChar {
            codepoint: 89,
            ascii: 89,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 45,
        normal: KeyChar {
            codepoint: 120,
            ascii: 120,
        },
        shift: KeyChar {
            codepoint: 88,
            ascii: 88,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 46,
        normal: KeyChar {
            codepoint: 99,
            ascii: 99,
        },
        shift: KeyChar {
            codepoint: 67,
            ascii: 67,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 47,
        normal: KeyChar {
            codepoint: 118,
            ascii: 118,
        },
        shift: KeyChar {
            codepoint: 86,
            ascii: 86,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 48,
        normal: KeyChar {
            codepoint: 98,
            ascii: 98,
        },
        shift: KeyChar {
            codepoint: 66,
            ascii: 66,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 49,
        normal: KeyChar {
            codepoint: 110,
            ascii: 110,
        },
        shift: KeyChar {
            codepoint: 78,
            ascii: 78,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 50,
        normal: KeyChar {
            codepoint: 109,
            ascii: 109,
        },
        shift: KeyChar {
            codepoint: 77,
            ascii: 77,
        },
        altgr: KeyChar {
            codepoint: 181,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 51,
        normal: KeyChar {
            codepoint: 44,
            ascii: 44,
        },
        shift: KeyChar {
            codepoint: 59,
            ascii: 59,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 52,
        normal: KeyChar {
            codepoint: 46,
            ascii: 46,
        },
        shift: KeyChar {
            codepoint: 58,
            ascii: 58,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 53,
        normal: KeyChar {
            codepoint: 45,
            ascii: 45,
        },
        shift: KeyChar {
            codepoint: 95,
            ascii: 95,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 57,
        normal: KeyChar {
            codepoint: 32,
            ascii: 32,
        },
        shift: KeyChar {
            codepoint: 32,
            ascii: 32,
        },
        altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
    KeyEntry {
        scancode: 86,
        normal: KeyChar {
            codepoint: 60,
            ascii: 60,
        },
        shift: KeyChar {
            codepoint: 62,
            ascii: 62,
        },
        altgr: KeyChar {
            codepoint: 124,
            ascii: 124,
        },
        shift_altgr: KeyChar {
            codepoint: 0,
            ascii: 0,
        },
    },
];
static DE_DE: Definition = Definition {
    id: 1,
    name: Text::literal(b"de_de"),
    display: Text::literal(b"DE"),
    aliases: DE_DE_ALIASES.as_ptr(),
    alias_count: 6,
    entries: DE_DE_ENTRIES.as_ptr(),
    entry_count: 53,
};
pub fn definition(id: u32) -> &'static Definition {
    match id {
        0 => &EN_EN,
        1 => &DE_DE,
        _ => panic!("Original keyboard layout enum"),
    }
}
fn equal_name(a: &[u8], b: &[u8]) -> bool {
    fn clean(b: u8) -> bool {
        !matches!(b, b'_' | b'-' | b' ' | b'\t')
    }
    a.iter()
        .copied()
        .filter(|&b| clean(b))
        .map(|b| b.to_ascii_uppercase())
        .eq(b
            .iter()
            .copied()
            .filter(|&b| clean(b))
            .map(|b| b.to_ascii_uppercase()))
}
pub fn parse_name(value: &[u8]) -> Option<u32> {
    let trimmed = value.iter().all(|b| matches!(b, b' ' | b'\t'));
    if trimmed {
        return None;
    }
    for id in 0..2 {
        let d = definition(id);
        if equal_name(value, d.name.bytes()) || equal_name(value, d.display.bytes()) {
            return Some(id);
        }
        for alias in unsafe { slice::from_raw_parts(d.aliases, d.alias_count) } {
            if equal_name(value, alias.bytes()) {
                return Some(id);
            }
        }
    }
    None
}
pub fn translate(id: u32, scancode: u8, shift: bool, altgr: bool) -> KeyChar {
    let d = definition(id);
    let entries = unsafe { slice::from_raw_parts(d.entries, d.entry_count) };
    let Some(e) = entries.iter().find(|e| e.scancode == scancode) else {
        return KeyChar::EMPTY;
    };
    if altgr && shift && e.shift_altgr.has_character() {
        e.shift_altgr
    } else if altgr && e.altgr.has_character() {
        e.altgr
    } else if shift && e.shift.has_character() {
        e.shift
    } else {
        e.normal
    }
}
pub fn ctrl_char(id: u32, scancode: u8) -> Option<u8> {
    let ch = translate(id, scancode, false, false).ascii;
    if ch.is_ascii_lowercase() {
        Some(ch - b'a' + 1)
    } else if ch.is_ascii_uppercase() {
        Some(ch - b'A' + 1)
    } else {
        None
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_layout_definition(id: u32) -> *const Definition {
    definition(id)
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable name bytes and owns aligned character/definition outputs; immutable returned names keep their static lifetime.
pub unsafe extern "C" fn r4os_rust_input_layout_parse(p: *const u8, l: usize) -> u32 {
    parse_name(if l == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p, l) }
    })
    .unwrap_or(u32::MAX)
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains readable name bytes and owns aligned character/definition outputs; immutable returned names keep their static lifetime.
pub unsafe extern "C" fn r4os_rust_input_layout_translate(
    id: u32,
    scan: u8,
    shift: bool,
    altgr: bool,
    o: *mut KeyChar,
) {
    unsafe { ptr::write(o, translate(id, scan, shift, altgr)) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_layout_ctrl(id: u32, scan: u8) -> u16 {
    ctrl_char(id, scan).map(u16::from).unwrap_or(256)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_layout_count() -> usize {
    2
}
const _: () = {
    assert!(core::mem::size_of::<KeyChar>() == 8);
    assert!(core::mem::align_of::<KeyChar>() == 4);
    assert!(core::mem::offset_of!(KeyChar, ascii) == 4);
    assert!(core::mem::size_of::<KeyEntry>() == 36);
    assert!(core::mem::align_of::<KeyEntry>() == 4);
    assert!(core::mem::offset_of!(KeyEntry, normal) == 4);
    assert!(core::mem::offset_of!(KeyEntry, shift) == 12);
    assert!(core::mem::offset_of!(KeyEntry, altgr) == 20);
    assert!(core::mem::offset_of!(KeyEntry, shift_altgr) == 28);
    assert!(core::mem::size_of::<Definition>() == 72);
    assert!(core::mem::offset_of!(Definition, name) == 8);
    assert!(core::mem::offset_of!(Definition, display) == 24);
    assert!(core::mem::offset_of!(Definition, aliases) == 40);
    assert!(core::mem::offset_of!(Definition, entries) == 56);
};
