//! Bounded wire/storage conversions only. UTF8 decoding, editing and ANSI
//! policy belong to the importing userland owners, never to this module.
use super::codepoint_queue::Queue;
use crate::contract as c;

const BYTE: u32 = 1 << 30;
const KEY: u32 = 1 << 31;
const TAGS: u32 = BYTE | KEY;
pub const BYTE_BUDGET: u32 = 4095;

pub fn pack(event: c::ConsoleInputEvent) -> Option<u32> {
    if event.version != 1 || event.flags != 0 {
        return None;
    }
    match event.kind {
        c::console_input_event_text => char::from_u32(event.value).map(|_| event.value),
        c::console_input_event_key if event.value <= 255 => Some(KEY | event.value),
        c::console_input_event_utf8_byte if event.value <= 255 => Some(BYTE | event.value),
        _ => None,
    }
}
pub fn unpack(word: u32) -> Option<c::ConsoleInputEvent> {
    let (kind, value) = match word & TAGS {
        0 => (c::console_input_event_text, word),
        KEY => (c::console_input_event_key, word & !KEY),
        BYTE => (c::console_input_event_utf8_byte, word & !BYTE),
        _ => return None,
    };
    let event = c::ConsoleInputEvent {
        version: 1,
        kind,
        value,
        flags: 0,
    };
    pack(event).map(|_| event)
}
pub fn text_word(value: u32) -> Option<u32> {
    pack(c::ConsoleInputEvent {
        kind: c::console_input_event_text,
        value,
        ..Default::default()
    })
}
pub const fn byte_word(value: u8) -> u32 {
    BYTE | value as u32
}

/// Keyboard layout's existing C0 and7F..9F values are logical keys. Tag them
/// at that sole native producer. Pure Unicode values never contain tag bits.
pub fn native_word(value: u32) -> Option<u32> {
    if value < 0x20 || (0x7f..=0x9f).contains(&value) {
        Some(KEY | value)
    } else {
        text_word(value)
    }
}
pub fn legacy_value(word: u32) -> Option<u32> {
    unpack(word).map(|event| event.value)
}
pub fn byte_cost(word: u32) -> Option<u32> {
    let event = unpack(word)?;
    if event.kind == c::console_input_event_text {
        Some(char::from_u32(event.value)?.len_utf8() as u32)
    } else {
        Some(1)
    }
}
pub fn admits(pending: u32, word: u32) -> bool {
    byte_cost(word).is_some_and(|cost| pending <= BYTE_BUDGET && cost <= BYTE_BUDGET - pending)
}

/// One owner holds the queue. Peek and test representation before removal:
/// an older byte reader must not eat a scalar it cannot return this call.
pub fn take_legacy(queue: &mut Queue, byte: bool) -> Option<u32> {
    let value = legacy_value(queue.peek()?)?;
    if byte && value > 255 {
        return None;
    }
    queue.pop();
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_control_and_continuations_are_disjoint_and_exact() {
        for value in [
            0, 0x80, 0x81, 0xfc, 0x20ac, 0x401, 0x10000, 0x1f600, 0x10ffff,
        ] {
            let event = c::ConsoleInputEvent {
                kind: c::console_input_event_text,
                value,
                ..Default::default()
            };
            let word = pack(event).unwrap();
            let decoded = unpack(word).unwrap();
            assert_eq!(
                (decoded.version, decoded.kind, decoded.value, decoded.flags),
                (event.version, event.kind, event.value, event.flags)
            );
            assert_ne!(word, byte_word(0x80));
        }
        let key = c::ConsoleInputEvent {
            kind: c::console_input_event_key,
            value: 0x81,
            ..Default::default()
        };
        assert_ne!(pack(key), Some(byte_word(0x81)));
        assert_ne!(pack(key), text_word(0x81));
        assert_eq!(
            unpack(native_word(0x20ac).unwrap()).unwrap().kind,
            c::console_input_event_text
        );
        assert_eq!(
            unpack(native_word(0x80).unwrap()).unwrap().kind,
            c::console_input_event_key
        );
    }
    #[test]
    fn invalid_wire_values_never_enter_storage() {
        for value in [0xd800, 0xdfff, 0x110000, u32::MAX] {
            assert!(text_word(value).is_none());
        }
        for event in [
            c::ConsoleInputEvent {
                version: 0,
                kind: 1,
                ..Default::default()
            },
            c::ConsoleInputEvent {
                flags: 1,
                kind: 1,
                ..Default::default()
            },
            c::ConsoleInputEvent {
                kind: 4,
                ..Default::default()
            },
            c::ConsoleInputEvent {
                kind: 2,
                value: 256,
                ..Default::default()
            },
            c::ConsoleInputEvent {
                kind: 3,
                value: 256,
                ..Default::default()
            },
        ] {
            assert!(pack(event).is_none());
        }
        assert!(unpack(TAGS).is_none());
        assert!(unpack(KEY | 256).is_none());
        assert!(unpack(BYTE | 256).is_none());
    }
    #[test]
    fn native_unrepresentable_byte_remains_queued_until_real_scalar_read() {
        let mut queue = Queue::EMPTY;
        assert!(queue.try_push(native_word(0x20ac).unwrap()));
        assert!(queue.try_push(native_word(0xfc).unwrap()));
        assert_eq!(take_legacy(&mut queue, true), None);
        assert_eq!(queue.pending_count(), 2);
        assert_eq!(take_legacy(&mut queue, false), Some(0x20ac));
        assert_eq!(take_legacy(&mut queue, true), Some(0xfc));
        assert!(!queue.pending());
    }
    #[test]
    fn full_queue_and_full_byte_budget_keep_whole_events() {
        let mut queue = Queue::EMPTY;
        let euro = native_word(0x20ac).unwrap();
        for _ in 0..super::super::codepoint_queue::CAPACITY {
            assert!(queue.try_push(euro));
        }
        assert!(!queue.try_push(byte_word(b'A')));
        assert_eq!(queue.pending_count(), 255);
        for _ in 0..255 {
            assert_eq!(take_legacy(&mut queue, true), None);
            assert_eq!(take_legacy(&mut queue, false), Some(0x20ac));
        }
        assert_eq!(byte_cost(euro), Some(3));
        assert!(admits(4092, euro));
        assert!(!admits(4093, euro));
        assert!(admits(4094, byte_word(0x81)));
        assert!(!admits(4095, byte_word(0x81)));
        assert!(!admits(u32::MAX, euro));
    }
}
