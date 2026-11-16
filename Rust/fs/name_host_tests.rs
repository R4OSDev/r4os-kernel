//! Software-only tests of the actual resident FAT/NTFS name owners.
#![allow(dead_code)]
#[path = "../storage/access_state.rs"]
pub mod actual_access_state;
mod storage {
    pub use crate::actual_access_state as access_state;
}
mod fat_names;
mod fat_types;
mod ntfs_format;
mod ntfs_format_lznt1;
mod ntfs_format_names;
mod ntfs_format_records;
mod ntfs_format_runs;
mod ntfs_format_types;
#[path = "../program/system/path.rs"]
mod program_path;

#[test]
fn strict_names_reject_aliases_odd_spans_and_preserve_outputs() {
    use ntfs_format_names as nt;
    for bad in [
        &b"\xc1\xa1"[..],
        &b"\xe0\x80\xaf"[..],
        &b"\xed\xa0\x80"[..],
        &b"\xf4\x90\x80\x80"[..],
        b"a\0b",
    ] {
        assert!(!nt::valid_name(bad));
        assert!(!fat_names::valid(bad));
        let mut out = [0x5a; 16];
        assert_eq!(nt::to_utf16(bad, &mut out), None);
        assert_eq!(out, [0x5a; 16]);
    }
    for bad in [
        &b"a"[..],
        &b"\x00\xd8"[..],
        &b"\x00\xdc"[..],
        &b"\x00\xdc\x00\xd8"[..],
        &b"a\0\0"[..],
        &b"\0\0"[..],
    ] {
        let mut out = [0x5a; 16];
        assert_eq!(nt::to_utf8(bad, &mut out), None);
        assert_eq!(out, [0x5a; 16]);
    }
    assert!(!nt::ascii_matches(&[], b"\xc4", &[0xc4, 0]));
    assert_eq!(nt::to_ascii(&[65, 0, 1], &mut [0; 8]), None);
    for bad in [b"ok/\xc1\xa1".as_slice(), b"ok/\xe0\x80\xaf/x", b"ok/\0x"] {
        assert!(!nt::valid_path(bad));
        assert!(!fat_names::valid_path(bad));
        assert!(!program_path::valid_unicode(bad));
    }
}

#[test]
fn exact_name_units_capacity_and_ntfs_foreign_records() {
    use ntfs_format_names as nt;
    let name = format!("{}\u{1f642}", "\u{20ac}".repeat(253));
    assert!(nt::valid_name(name.as_bytes()));
    assert!(fat_names::valid(name.as_bytes()));
    let mut wire = [0x5a; 510];
    assert_eq!(nt::to_utf16(name.as_bytes(), &mut wire), Some(510));
    let mut too_short = [0x5a; 509];
    assert_eq!(nt::to_utf16(name.as_bytes(), &mut too_short), None);
    assert_eq!(too_short, [0x5a; 509]);
    let mut text = [0x5a; 768];
    assert_eq!(nt::to_utf8(&wire, &mut text), Some(name.len()));
    assert_eq!(&text[..name.len()], name.as_bytes());
    assert!(text[name.len()..].iter().all(|b| *b == 0x5a));
    assert_eq!(program_path::unicode_error(name.as_bytes()), None);
    assert_eq!(
        program_path::unicode_error(format!("C:\\{name}x").as_bytes()),
        Some(program_path::UnicodeError::TooLong)
    );
    assert_eq!(
        program_path::unicode_error(b"C:\\bad\xc1\xa1"),
        Some(program_path::UnicodeError::Malformed)
    );
    assert!(fat_names::valid(b"N27-LONG-STAGING-0123456789.TMP"));
    assert!(!fat_names::valid_short(b"N27-LONG-STAGING-0123456789.TMP"));
    assert!(fat_names::valid(b"N27+STAG.TMP"));
    assert!(!fat_names::valid(b".N27-stage.tmp"));
    assert!(nt::valid_name(b".N27-stage.tmp"));
    assert!(fat_names::valid_short(b"A.")); // existing short alias, not a valid LFN
    assert!(!fat_names::valid_long(b"A."));
    assert!(!nt::valid_name(format!("{name}x").as_bytes()));
    assert!(!fat_names::valid(format!("{name}x").as_bytes()));
    assert!(!fat_names::valid("\u{ffff}".as_bytes())); // VFAT padding marker
    let mut raw = [0u8; 0x42 + 510];
    raw[0x40] = 255;
    raw[0x42..].copy_from_slice(&wire);
    assert!(ntfs_format_records::file_name(&raw).is_some());
    let original = raw;
    raw[0x42 + 508..].copy_from_slice(&[b'x', 0]); // lone high surrogate
    let malformed = raw;
    assert!(ntfs_format_records::file_name(&raw).is_none());
    assert_eq!(raw, malformed); // foreign raw bytes are never repaired
    assert_ne!(raw, original);
}

fn lfn_entry(name: &str) -> Option<fat_types::Entry> {
    let mut units = [0; fat_types::UNITS];
    let count = fat_names::utf8_units(name.as_bytes(), &mut units)?;
    let short = *b"UNICOD~1TXT";
    let checksum = fat_names::checksum(short);
    let slots = count.div_ceil(13);
    let mut lfn = fat_names::Lfn::EMPTY;
    for seq in (1..=slots).rev() {
        let mut raw = [0; 32];
        fat_names::write_lfn(&mut raw, &units[..count], seq, slots, checksum);
        assert!(lfn.consume(&raw));
    }
    let mut raw = [0; 32];
    raw[..11].copy_from_slice(&short);
    raw[11] = 0x20;
    fat_names::make_entry(&raw, &lfn, 123, 64)
}

#[test]
fn vfat_pairs_cross_slots_and_long_names_never_become_aliases() {
    for prefix in [0, 12, 25, 38, 253] {
        let name = format!("{}\u{1f642}", "a".repeat(prefix));
        let entry = lfn_entry(&name).expect("complete valid pair across LFNs");
        assert_eq!(&entry.name[..entry.name_len], name.as_bytes());
    }
    let mut raw = [0; 32];
    raw[..11].copy_from_slice(b"UNICOD~1TXT");
    raw[11] = 0x20;
    let mut lfn = fat_names::Lfn::EMPTY;
    lfn.active = true;
    lfn.expected = 0;
    lfn.len = 1;
    lfn.units[0] = 0xd800;
    lfn.checksum = fat_names::checksum(*b"UNICOD~1TXT");
    assert!(fat_names::make_entry(&raw, &lfn, 0, 0).is_none());
    lfn.units[0] = 0x20ac;
    lfn.checksum ^= 1;
    assert!(fat_names::make_entry(&raw, &lfn, 0, 0).is_none());
    lfn.expected = 1;
    assert!(fat_names::make_entry(&raw, &lfn, 0, 0).is_none());
    raw[0] = 0xc3;
    raw[1] = 0xa4; // OEM bytes must not alias UTF8 ae
    assert!(!fat_names::short_equals(&raw, "\u{e4}".as_bytes()));
    assert!(fat_names::make_entry(&raw, &fat_names::Lfn::EMPTY, 0, 0).is_none());
}

fn index_put16(s: &mut [u8], at: usize, value: u16) {
    s[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn index_put32(s: &mut [u8], at: usize, value: u32) {
    s[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn index_entries(units: usize, children: bool) -> Vec<u8> {
    let key = 0x42 + units * 2;
    let length = (16 + key + 7) & !7;
    let length = length + if children { 8 } else { 0 };
    let mut s = vec![0; length + if children { 24 } else { 16 }];
    index_put16(&mut s, 8, length as u16);
    index_put16(&mut s, 10, key as u16);
    index_put16(&mut s, 12, u16::from(children));
    s[0x50] = units as u8;
    for i in 0..units {
        s[0x52 + i * 2] = b'A';
    }
    index_put16(&mut s, length + 8, if children { 24 } else { 16 });
    index_put16(&mut s, length + 12, if children { 3 } else { 2 });
    s
}

#[test]
fn ntfs_index_entries_require_progress_whole_names_and_terminal_tail() {
    use ntfs_format_records as fmt;
    for children in [false, true] {
        for units in [1, 255] {
            let s = index_entries(units, children);
            assert!(fmt::directory_index_entries(&s, children));
            assert!(!fmt::directory_index_entries(&s, !children));
            for cut in 0..s.len() {
                assert!(!fmt::directory_index_entries(&s[..cut], children));
            }
        }
    }
    let original = index_entries(1, false);
    for (at, value) in [
        (8, 0),
        (8, 8),
        (8, 87),
        (8, 65528),
        (10, 0),
        (10, 65),
        (10, 89),
        (12, 4),
        (12, 1),
        (88 + 8, 0),
        (88 + 10, 1),
        (88 + 12, 0),
    ] {
        let mut bad = original.clone();
        index_put16(&mut bad, at, value);
        let unchanged = bad.clone();
        assert!(
            !fmt::directory_index_entries(&bad, false),
            "offset {at}, value {value}"
        );
        assert_eq!(bad, unchanged);
    }
    let mut bad = original.clone();
    bad[0x50] = 255;
    assert!(!fmt::directory_index_entries(&bad, false));
    bad[0x50] = 0;
    assert!(!fmt::directory_index_entries(&bad, false));
    // A valid first match cannot hide a malformed later entry.
    let mut tail = original[..88].to_vec();
    tail.extend_from_slice(&bad);
    assert!(!fmt::directory_index_entries(&tail, false));
    // The child VCN is not part of the key, even when both fit the entry.
    let mut child = index_entries(1, true);
    index_put16(&mut child, 10, 80);
    assert!(!fmt::directory_index_entries(&child, true));
    let mut it = ntfs_format_types::IteratorState {
        offset: usize::MAX,
        ..Default::default()
    };
    assert!(fmt::index_entry(&original, &mut it).is_none());
    assert!(it.done);
}

#[test]
fn ntfs_index_headers_and_fixed_up_block_end_stay_bounded() {
    use ntfs_format_records as fmt;
    let entries = index_entries(1, false);
    let mut root = vec![0; 32 + entries.len()];
    index_put32(&mut root, 0, 0x30);
    index_put32(&mut root, 4, 1);
    index_put32(&mut root, 8, 4096);
    index_put32(&mut root, 16, 16);
    index_put32(&mut root, 20, 16 + entries.len() as u32);
    index_put32(&mut root, 24, 16 + entries.len() as u32);
    root[32..].copy_from_slice(&entries);
    assert!(fmt::index_root(&root).is_some());
    for cut in 0..root.len() {
        assert!(fmt::index_root(&root[..cut]).is_none());
    }
    for (at, value) in [
        (16, 0),
        (16, 17),
        (16, u32::MAX),
        (20, 8),
        (20, 121),
        (20, u32::MAX),
        (24, 119),
        (24, u32::MAX),
        (28, 2),
    ] {
        let mut bad = root.clone();
        index_put32(&mut bad, at, value);
        assert!(fmt::index_root(&bad).is_none());
    }
    let mut block = vec![0; 4096];
    let start = block.len() - entries.len();
    index_put32(&mut block, 0, 0x58444e49);
    index_put16(&mut block, 4, 0x28);
    index_put16(&mut block, 6, 9);
    index_put32(&mut block, 24, (start - 24) as u32);
    index_put32(&mut block, 28, 4096 - 24);
    index_put32(&mut block, 32, 4096 - 24);
    block[start..].copy_from_slice(&entries);
    for corruption in [0, 1, 2] {
        let mut wire = block.clone();
        if corruption == 1 {
            index_put16(&mut wire, start + 8, 0);
        }
        if corruption == 2 {
            wire[start + 0x50] = 255;
        }
        assert_eq!(fmt::fixup(&mut wire, Some(1)), 0);
        assert_eq!(fmt::fixup(&mut wire, None), 0); // valid USA, deliberately bad index
        let parsed = fmt::index_block(&wire).expect("bounded INDX header");
        assert_eq!(
            fmt::directory_index_entries(unsafe { parsed.entries.get() }, false),
            corruption == 0
        );
    }
}
