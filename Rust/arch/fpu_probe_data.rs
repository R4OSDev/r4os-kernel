// Pure integer oracle for architectural legacy images; empty x87 data and
// reserved/pointer bytes are deliberately unspecified. No FP instruction.
pub const OUTPUT_BYTES: usize = 2112;
#[repr(C, align(64))]
pub struct Pattern {
    pub bytes: [u8; 704],
}
#[repr(C, align(64))]
pub struct Output {
    pub bytes: [u8; OUTPUT_BYTES],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Control {
    pub cw: u16,
    pub reserved: u16,
    pub mxcsr: u32,
    pub avx: u32,
    pub phase: u32,
}
pub fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
pub fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
pub fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
pub fn mxcsr_mask(image: &[u8]) -> u32 {
    let mask = u32_at(image, 28) & 0xffff;
    if mask == 0 { 0xffbf } else { mask }
}
pub fn default_environment(image: &[u8]) -> bool {
    u16_at(image, 0) == 0x037f
        && u16_at(image, 2) & 0x38ff == 0
        && image[4] == 0
        && u32_at(image, 24) == 0x1f80
}
impl Pattern {
    pub fn new(seed: u32) -> Self {
        let mut out = Self { bytes: [0; 704] };
        for (i, b) in out.bytes[..512].iter_mut().enumerate() {
            *b = ((i as u32).wrapping_mul(73).wrapping_add(seed) ^ ((i as u32) >> 3)) as u8;
        }
        for i in 0..8 {
            let significand = 0x8000_0000_0000_0000u64
                | ((i as u64 + 1) * 0x0102_0304_0506_0708u64 + u64::from(seed));
            out.bytes[512 + i * 16..520 + i * 16].copy_from_slice(&significand.to_le_bytes());
            out.bytes[520 + i * 16..522 + i * 16]
                .copy_from_slice(&(0x3fffu16 | if i & 1 != 0 { 0x8000 } else { 0 }).to_le_bytes());
            let mm = 0x1928_3746_5546_3728u64
                ^ (u64::from(seed) << 24)
                ^ ((i as u64 + 1) * 0x0101_0101_0101_0101);
            out.bytes[640 + i * 8..648 + i * 8].copy_from_slice(&mm.to_le_bytes());
        }
        out
    }
}
impl Output {
    pub const fn new() -> Self {
        Self {
            bytes: [0; OUTPUT_BYTES],
        }
    }
}
impl Default for Output {
    fn default() -> Self {
        Self::new()
    }
}
impl Control {
    pub fn new(round: u32, extras: u32, avx: bool, phase: u32) -> Self {
        Self {
            cw: 0x037f | ((round as u16) << 10),
            reserved: 0,
            mxcsr: 0x1f80 | (round << 13) | extras,
            avx: avx as u32,
            phase,
        }
    }
}
pub fn validate(pattern: &Pattern, control: Control, output: &Output) -> Result<(), &'static [u8]> {
    let (top, tags, live) = match control.phase {
        0 => (2, 0xff, 8),
        1 => (0, 0xff, 8),
        2 => (5, 0xe0, 3),
        _ => return Err(b"phase"),
    };
    for offset in [0, 512] {
        let data = &output.bytes[offset..offset + 512];
        if u16_at(data, 0) != control.cw
            || u16_at(data, 2) != top << 11
            || data[4] != tags
            || u32_at(data, 24) != control.mxcsr
        {
            return Err(b"control-top-tags");
        }
        for lane in 0..live {
            let stored = 32 + lane * 16;
            if control.phase == 1 {
                if data[stored..stored + 8] != pattern.bytes[640 + lane * 8..648 + lane * 8]
                    || u16_at(data, stored + 8) != 0xffff
                {
                    return Err(b"mmx-alias");
                }
            } else {
                let source = if control.phase == 0 {
                    (13 - lane) % 8
                } else {
                    2 - lane
                };
                if data[stored..stored + 10] != pattern.bytes[512 + source * 16..522 + source * 16]
                {
                    return Err(b"x87-data");
                }
            }
        }
        for lane in 0..16 {
            if data[160 + lane * 16..176 + lane * 16] != pattern.bytes[lane * 32..lane * 32 + 16] {
                return Err(b"xmm-legacy");
            }
        }
    }
    let width = if control.avx != 0 { 32 } else { 16 };
    for lane in 0..16 {
        if output.bytes[1024 + lane * 32..1024 + lane * 32 + width]
            != pattern.bytes[lane * 32..lane * 32 + width]
        {
            return Err(b"all-vector-registers");
        }
    }
    let expected = [2u64, 1, 2, 1][((control.cw >> 10) & 3) as usize];
    if u64_at(&output.bytes, 1536) != expected || u64_at(&output.bytes, 1544) != expected {
        return Err(b"actual-rounding");
    }
    if output.bytes[1604] != 0 {
        return Err(b"emms-mode");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn architectural_zero_mask_does_not_authorize_daz() {
        let mut raw = [0u8; 512];
        assert_eq!(mxcsr_mask(&raw), 0xffbf);
        assert_eq!(mxcsr_mask(&raw) & 0x40, 0);
        raw[28..32].copy_from_slice(&0x1ffffu32.to_le_bytes());
        assert_eq!(mxcsr_mask(&raw), 0xffff);
    }
    #[test]
    fn fresh_empty_tags_do_not_promise_data_zero() {
        let mut raw = [0xa5u8; 512];
        raw[0..2].copy_from_slice(&0x37fu16.to_le_bytes());
        raw[2..4].copy_from_slice(&0u16.to_le_bytes());
        raw[4] = 0;
        raw[24..28].copy_from_slice(&0x1f80u32.to_le_bytes());
        assert!(default_environment(&raw));
        raw[2] = 1;
        assert!(!default_environment(&raw));
    }
    #[test]
    fn pattern_is_finite_distinct_and_complete() {
        let p = Pattern::new(19);
        for i in 0..8 {
            assert_ne!(u64_at(&p.bytes, 512 + i * 16) >> 63, 0);
            assert_eq!(u16_at(&p.bytes, 520 + i * 16) & 0x7fff, 0x3fff);
        }
        for i in 1..16 {
            assert_ne!(p.bytes[..32], p.bytes[i * 32..i * 32 + 32]);
        }
        assert_eq!(core::mem::size_of::<Pattern>(), 704);
        assert_eq!(core::mem::size_of::<Output>(), 2112);
        assert_eq!(core::mem::size_of::<Control>(), 16);
    }
}
