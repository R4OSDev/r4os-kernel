//! Bounded vendor-format validation. No instruction, pointer submission or
//! allocation occurs here. Whole-file origin hashes are checked by the early
//! owner independently; a checksum is never authority to load arbitrary data.
pub(crate) const MAX_FILE: usize = 131_072;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Truncated,
    Format,
    Checksum,
    Ambiguous,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Patch {
    pub offset: usize,
    pub bytes: usize,
    pub revision: u32,
}
pub(crate) fn u32_at(bytes: &[u8], at: usize) -> Result<u32, Error> {
    let end = at.checked_add(4).ok_or(Error::Truncated)?;
    let word = bytes.get(at..end).ok_or(Error::Truncated)?;
    Ok(u32::from_le_bytes(word.try_into().unwrap()))
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, Error> {
    let word = bytes
        .get(at..at.checked_add(2).ok_or(Error::Truncated)?)
        .ok_or(Error::Truncated)?;
    Ok(u16::from_le_bytes(word.try_into().unwrap()))
}
fn sum(bytes: &[u8]) -> Result<u32, Error> {
    if bytes.len() & 3 != 0 {
        return Err(Error::Format);
    }
    Ok(bytes.as_chunks::<4>().0.iter().fold(0u32, |total, word| {
        total.wrapping_add(u32::from_le_bytes(*word))
    }))
}
fn select(best: &mut Option<Patch>, patch: Patch, bytes: &[u8]) -> Result<(), Error> {
    if let Some(old) = *best {
        if old.revision > patch.revision {
            return Ok(());
        }
        if old.revision == patch.revision {
            if bytes[old.offset..old.offset + old.bytes]
                != bytes[patch.offset..patch.offset + patch.bytes]
            {
                return Err(Error::Ambiguous);
            }
            return Ok(());
        }
    }
    *best = Some(patch);
    Ok(())
}
/// Validate every record before returning any match. Platform is the exact
/// one-hot bit obtained from this CPU's IA32_PLATFORM_ID, not a model guess.
pub(crate) fn intel(bytes: &[u8], signature: u32, platform: u32) -> Result<Option<Patch>, Error> {
    if bytes.is_empty() || bytes.len() > MAX_FILE || !platform.is_power_of_two() || platform > 128 {
        return Err(Error::Format);
    }
    let mut at = 0;
    let mut count = 0;
    let mut best = None;
    while at < bytes.len() {
        count += 1;
        if count > 64 {
            return Err(Error::Format);
        }
        let remain = &bytes[at..];
        if remain.len() < 48 {
            return Err(Error::Truncated);
        }
        let revision = u32_at(remain, 4)?;
        if u32_at(remain, 0)? != 1
            || u32_at(remain, 20)? != 1
            || revision == 0
            || remain[36..48] != [0; 12]
        {
            return Err(Error::Format);
        }
        let data_raw = u32_at(remain, 28)? as usize;
        let total_raw = u32_at(remain, 32)? as usize;
        let data = if data_raw == 0 { 2000 } else { data_raw };
        let total = if total_raw == 0 { 2048 } else { total_raw };
        let base = data.checked_add(48).ok_or(Error::Format)?;
        if data & 3 != 0 || total & 1023 != 0 || total < base || (data_raw == 0 && total != 2048) {
            return Err(Error::Format);
        }
        let record = remain.get(..total).ok_or(Error::Truncated)?;
        if sum(&record[..base])? != 0 {
            return Err(Error::Checksum);
        }
        let original_signature = u32_at(record, 12)?;
        let original_flags = u32_at(record, 24)?;
        if original_flags & !255 != 0 {
            return Err(Error::Format);
        }
        let mut matches = original_signature == signature && original_flags & platform != 0;
        let ext = &record[base..];
        if !ext.is_empty() {
            if ext.len() < 20 {
                return Err(Error::Truncated);
            }
            let count = u32_at(ext, 0)? as usize;
            if count > 256
                || count.checked_mul(12).and_then(|n| n.checked_add(20)) != Some(ext.len())
                || ext[8..20] != [0; 12]
            {
                return Err(Error::Format);
            }
            if sum(ext)? != 0 {
                return Err(Error::Checksum);
            }
            let original_sum = original_signature
                .wrapping_add(original_flags)
                .wrapping_add(u32_at(record, 16)?);
            for entry in ext[20..].as_chunks::<12>().0 {
                let sig = u32_at(entry, 0)?;
                let flags = u32_at(entry, 4)?;
                if flags & !255 != 0
                    || sig.wrapping_add(flags).wrapping_add(u32_at(entry, 8)?) != original_sum
                {
                    return Err(Error::Checksum);
                }
                matches |= sig == signature && flags & platform != 0;
            }
        }
        if matches {
            select(
                &mut best,
                Patch {
                    offset: at + 48,
                    bytes: data,
                    revision,
                },
                bytes,
            )?;
        }
        at += total;
    }
    Ok(best)
}
pub(crate) fn amd_signature(revision: u32) -> u32 {
    ((revision >> 8) & 15)
        | ((revision >> 8) & 0xf0)
        | 0xf00
        | ((revision >> 4) & 0xf0000)
        | ((revision >> 4) & 0xff00000)
}
fn family(signature: u32) -> u32 {
    let base = (signature >> 8) & 15;
    base + if base == 15 {
        (signature >> 20) & 255
    } else {
        0
    }
}
/// AMD containers use magic0x00414d44, an equivalence table and type1 patch
/// sections. Zen matches the full CPUID encoded in patch_id; it does not
/// substitute the legacy table's processor_rev_id for that signature.
/// Modern payload integrity is the pinned SHA256, not an invented legacy sum.
pub(crate) fn amd(bytes: &[u8], signature: u32) -> Result<Option<Patch>, Error> {
    if bytes.is_empty() || bytes.len() > MAX_FILE {
        return Err(Error::Format);
    }
    let mut at = 0;
    let mut records = 0;
    let mut best = None;
    while at < bytes.len() {
        if u32_at(bytes, at)? != 0x0041_4d44 || u32_at(bytes, at + 4)? != 0 {
            return Err(Error::Format);
        }
        let len = u32_at(bytes, at + 8)? as usize;
        if len < 16 || len & 15 != 0 || len > 4096 {
            return Err(Error::Format);
        }
        let table_start = at.checked_add(12).ok_or(Error::Truncated)?;
        let table_end = table_start.checked_add(len).ok_or(Error::Truncated)?;
        let table = bytes.get(table_start..table_end).ok_or(Error::Truncated)?;
        if table[len - 16..] != [0; 16] {
            return Err(Error::Format);
        }
        let mut equivalent = None;
        for row in table[..len - 16].as_chunks::<16>().0 {
            let installed = u32_at(row, 0)?;
            let equiv = u16_at(row, 12)?;
            if installed == 0 || u16_at(row, 14)? != 0 || equiv == 0 {
                return Err(Error::Format);
            }
            if installed == signature {
                // An errata-qualified legacy mapping is not silently assumed.
                if u32_at(row, 4)? != 0 || u32_at(row, 8)? != 0 {
                    return Err(Error::Format);
                }
                if equivalent.is_some_and(|old| old != equiv) {
                    return Err(Error::Ambiguous);
                }
                equivalent = Some(equiv);
            }
        }
        at = table_end;
        let mut sections = 0;
        while at < bytes.len() && u32_at(bytes, at)? != 0x0041_4d44 {
            records += 1;
            sections += 1;
            if records > 128 || u32_at(bytes, at)? != 1 {
                return Err(Error::Format);
            }
            let size = u32_at(bytes, at + 4)? as usize;
            if !(64..=32_768).contains(&size) {
                return Err(Error::Format);
            }
            let offset = at.checked_add(8).ok_or(Error::Truncated)?;
            let end = offset.checked_add(size).ok_or(Error::Truncated)?;
            let patch = bytes.get(offset..end).ok_or(Error::Truncated)?;
            let revision = u32_at(patch, 4)?;
            let processor = u16_at(patch, 24)?;
            if revision == 0 || u32_at(patch, 16)? != 0 || u32_at(patch, 20)? != 0 {
                return Err(Error::Format);
            }
            let matches = if family(signature) >= 0x17 {
                amd_signature(revision) == signature
                    && 0xf + u32::from(processor >> 12) == family(signature)
            } else {
                equivalent == Some(processor)
            };
            if matches {
                select(
                    &mut best,
                    Patch {
                        offset,
                        bytes: size,
                        revision,
                    },
                    bytes,
                )?;
            }
            at = end;
        }
        if sections == 0 {
            return Err(Error::Format);
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn intel_fixture() -> [u8; 2048] {
        let mut b = [0; 2048];
        put(&mut b, 0, 1);
        put(&mut b, 4, 0x28);
        put(&mut b, 12, 0x306c3);
        put(&mut b, 20, 1);
        put(&mut b, 24, 0x32);
        let check = 0u32.wrapping_sub(sum(&b).unwrap());
        put(&mut b, 16, check);
        b
    }
    fn amd_fixture() -> [u8; 100] {
        let mut b = [0; 100];
        put(&mut b, 0, 0x00414d44);
        put(&mut b, 8, 16);
        put(&mut b, 28, 1);
        put(&mut b, 32, 64);
        put(&mut b, 40, 0x0a0011de);
        b[60..62].copy_from_slice(&0xa011u16.to_le_bytes());
        b
    }
    #[test]
    fn intel_exact_platform_and_signature() {
        let b = intel_fixture();
        assert_eq!(intel(&b, 0x306c3, 2).unwrap().unwrap().revision, 0x28);
        assert_eq!(intel(&b, 0x306c3, 1), Ok(None));
        assert_eq!(intel(&b, 0x306c2, 2), Ok(None));
    }
    #[test]
    fn intel_checksum_truncation_and_tail() {
        let mut b = intel_fixture();
        b[99] ^= 1;
        assert_eq!(intel(&b, 0x306c3, 2), Err(Error::Checksum));
        let b = intel_fixture();
        assert_eq!(intel(&b[..2047], 0x306c3, 2), Err(Error::Truncated));
        let mut whole = b.to_vec();
        whole.extend_from_slice(&[0; 4]);
        assert_eq!(intel(&whole, 0x306c3, 2), Err(Error::Truncated));
    }
    #[test]
    fn intel_unknown_header_reserved_and_bad_sizes() {
        for off in [0, 20, 36, 40, 44] {
            let mut b = intel_fixture();
            put(&mut b, off, 2);
            assert_eq!(intel(&b, 0x306c3, 2), Err(Error::Format));
        }
        let mut b = intel_fixture();
        put(&mut b, 28, u32::MAX);
        assert_eq!(intel(&b, 0x306c3, 2), Err(Error::Format));
        assert_eq!(intel(&intel_fixture(), 0x306c3, 3), Err(Error::Format));
    }
    #[test]
    fn intel_valid_prefix_does_not_hide_bad_later_record() {
        let mut b = intel_fixture().to_vec();
        let mut second = intel_fixture();
        second[100] = 1;
        b.extend_from_slice(&second);
        assert_eq!(intel(&b, 0x306c3, 2), Err(Error::Checksum));
    }
    fn extended() -> [u8; 3072] {
        let mut b = [0; 3072];
        b[..48].copy_from_slice(&intel_fixture()[..48]);
        put(&mut b, 16, 0);
        put(&mut b, 28, 2992);
        put(&mut b, 32, 3072);
        let base_check = 0u32.wrapping_sub(sum(&b[..3040]).unwrap());
        put(&mut b, 16, base_check);
        put(&mut b, 3040, 1);
        put(&mut b, 3060, 0x306c4);
        put(&mut b, 3064, 2);
        let replacement = 0x306c3u32
            .wrapping_add(0x32)
            .wrapping_add(base_check)
            .wrapping_sub(0x306c4)
            .wrapping_sub(2);
        put(&mut b, 3068, replacement);
        let ext_check = 0u32.wrapping_sub(sum(&b[3040..]).unwrap());
        put(&mut b, 3044, ext_check);
        b
    }
    #[test]
    fn intel_extended_signature_and_both_checksums() {
        let b = extended();
        assert!(intel(&b, 0x306c4, 2).unwrap().is_some());
        assert_eq!(intel(&b, 0x306c4, 1), Ok(None));
        let mut bad = b;
        bad[3068] ^= 1;
        assert_eq!(intel(&bad, 0x306c4, 2), Err(Error::Checksum));
        put(&mut bad, 3044, 0);
        let c = 0u32.wrapping_sub(sum(&bad[3040..]).unwrap());
        put(&mut bad, 3044, c);
        assert_eq!(intel(&bad, 0x306c4, 2), Err(Error::Checksum));
        let mut bad = b;
        put(&mut bad, 3040, 2);
        assert_eq!(intel(&bad, 0x306c4, 2), Err(Error::Format));
    }
    #[test]
    fn amd_zen_full_identity_not_equivalence_guess() {
        let b = amd_fixture();
        assert_eq!(amd_signature(0x0a0011de), 0x00a00f11);
        assert_eq!(amd(&b, 0x00a00f11).unwrap().unwrap().revision, 0x0a0011de);
        assert_eq!(amd(&b, 0x00a00f12), Ok(None));
    }
    #[test]
    fn amd_truncated_section_and_wrong_types() {
        let b = amd_fixture();
        assert_eq!(amd(&b[..99], 0x00a00f11), Err(Error::Truncated));
        for off in [0, 4, 28] {
            let mut b = amd_fixture();
            put(&mut b, off, 2);
            assert_eq!(amd(&b, 0x00a00f11), Err(Error::Format));
        }
    }
    #[test]
    fn amd_bad_table_chipset_and_size() {
        for off in [24, 52, 56] {
            let mut b = amd_fixture();
            put(&mut b, off, 1);
            assert_eq!(amd(&b, 0x00a00f11), Err(Error::Format));
        }
        let mut b = amd_fixture();
        put(&mut b, 32, 33_000);
        assert_eq!(amd(&b, 0x00a00f11), Err(Error::Format));
    }
    #[test]
    fn amd_no_successful_prefix_over_truncated_tail() {
        let mut b = amd_fixture().to_vec();
        b.push(1);
        assert_eq!(amd(&b, 0x00a00f11), Err(Error::Truncated));
    }
    #[test]
    fn amd_legacy_equivalence_and_errata_are_separate() {
        let mut b = [0; 116];
        put(&mut b, 0, 0x00414d44);
        put(&mut b, 8, 32);
        put(&mut b, 12, 0x00600f12);
        b[24..26].copy_from_slice(&0x6012u16.to_le_bytes());
        put(&mut b, 44, 1);
        put(&mut b, 48, 64);
        put(&mut b, 56, 0x06000852);
        b[76..78].copy_from_slice(&0x6012u16.to_le_bytes());
        assert!(amd(&b, 0x00600f12).unwrap().is_some());
        assert_eq!(amd(&b, 0x00600f11), Ok(None));
        put(&mut b, 16, 1);
        assert_eq!(amd(&b, 0x00600f12), Err(Error::Format));
    }
}
