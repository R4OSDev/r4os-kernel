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
