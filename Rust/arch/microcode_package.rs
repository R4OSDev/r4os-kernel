//! Optional R4U0 appendix after the unchanged legacy R4I total. All lengths,
//! origins and vendor records are validated before any native load can occur.
use super::{
    microcode_catalog::{PINS, Pin},
    microcode_format as format,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Preload,
    Appendix,
    Origin,
    Vendor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct File {
    pub offset: usize,
    pub bytes: usize,
}
fn word(bytes: &[u8], at: usize) -> Result<u32, Error> {
    format::u32_at(bytes, at).map_err(|_| Error::Appendix)
}
fn entry(bytes: &[u8], at: usize, pin: Pin, offset: usize) -> Result<File, Error> {
    let row = bytes.get(at..at + 64).ok_or(Error::Appendix)?;
    if [
        word(row, 0)?,
        word(row, 4)?,
        word(row, 8)?,
        word(row, 12)?,
        word(row, 16)?,
        word(row, 20)?,
        word(row, 24)?,
        word(row, 28)?,
    ] != [
        pin.vendor,
        pin.id,
        offset as u32,
        pin.bytes as u32,
        pin.signature,
        pin.revision,
        pin.minimum_revision,
        pin.platform_mask,
    ] || row[32..64] != pin.sha256
    {
        return Err(Error::Origin);
    }
    Ok(File {
        offset,
        bytes: pin.bytes,
    })
}
pub(crate) fn validate(
    image: &[u8],
    mut digest: impl FnMut(&[u8]) -> [u8; 32],
) -> Result<Option<[File; 2]>, Error> {
    if image.len() < 64 || &image[..8] != b"R4I0\x01\0\x40\0" {
        return Err(Error::Preload);
    }
    let count = word(image, 8)? as usize;
    let entries = word(image, 12)? as usize;
    let data = word(image, 16)? as usize;
    let base = word(image, 20)? as usize;
    if count > 32
        || entries < 64
        || entries
            .checked_add(count * 128)
            .is_none_or(|end| end > data)
        || data > base
        || base > image.len()
    {
        return Err(Error::Preload);
    }
    if base == image.len() {
        return Ok(None);
    }
    let tail = &image[base..];
    if tail.len() < 32
        || tail.len() > 262_144
        || &tail[..8] != b"R4U0\x01\0\x20\0"
        || word(tail, 8)? != 2
        || word(tail, 12)? as usize != tail.len()
        || word(tail, 16)? != 64
        || tail[20..32] != [0; 12]
    {
        return Err(Error::Appendix);
    }
    let mut files = [File {
        offset: 0,
        bytes: 0,
    }; 2];
    let mut offset = 160;
    for (index, pin) in PINS.into_iter().enumerate() {
        let file = entry(tail, 32 + 64 * index, pin, offset)?;
        let end = file.offset.checked_add(file.bytes).ok_or(Error::Appendix)?;
        let payload = tail.get(file.offset..end).ok_or(Error::Appendix)?;
        if digest(payload) != pin.sha256 {
            return Err(Error::Origin);
        }
        let matched = match pin.vendor {
            1 => format::intel(
                payload,
                pin.signature,
                1 << pin.platform_mask.trailing_zeros(),
            ),
            2 => format::amd(payload, pin.signature),
            _ => return Err(Error::Vendor),
        }
        .map_err(|_| Error::Vendor)?
        .ok_or(Error::Vendor)?;
        if matched.revision != pin.revision {
            return Err(Error::Vendor);
        }
        files[index] = File {
            offset: base + file.offset,
            bytes: file.bytes,
        };
        offset = end;
    }
    if offset != tail.len() {
        return Err(Error::Appendix);
    }
    Ok(Some(files))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn old() -> [u8; 64] {
        let mut bytes = [0; 64];
        bytes[..8].copy_from_slice(b"R4I0\x01\0\x40\0");
        for at in [12, 16, 20] {
            bytes[at..at + 4].copy_from_slice(&64u32.to_le_bytes());
        }
        bytes
    }
    #[test]
    fn legacy_preload_has_no_firmware_and_no_hash_work() {
        let b = old();
        assert_eq!(
            validate(&b, |_| panic!("No firmware bytes exist")),
            Ok(None)
        );
    }
    #[test]
    fn arbitrary_or_truncated_tail_is_not_silently_missing() {
        let mut b = old().to_vec();
        b.push(0);
        assert_eq!(validate(&b, |_| [0; 32]), Err(Error::Appendix));
        let mut b = old();
        b[20..24].copy_from_slice(&65u32.to_le_bytes());
        assert_eq!(validate(&b, |_| [0; 32]), Err(Error::Preload));
    }
    #[test]
    fn overlapping_r4i_table_is_refused_before_firmware() {
        let mut b = old();
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(validate(&b, |_| [0; 32]), Err(Error::Preload));
    }
}
