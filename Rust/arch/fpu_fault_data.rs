// Pure bounded decoder. Raw capture never acknowledges or repairs an exception.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Environment {
    pub cw: u16,
    pub sw: u16,
    pub tags: u8,
    pub top: u8,
    pub mxcsr: u32,
    pub mxcsr_mask: u32,
    pub unmasked_x87: u8,
    pub unmasked_simd: u8,
}
pub fn decode(raw: &[u8; 512]) -> Environment {
    let u16at = |p| u16::from_le_bytes([raw[p], raw[p + 1]]);
    let u32at = |p| u32::from_le_bytes([raw[p], raw[p + 1], raw[p + 2], raw[p + 3]]);
    let cw = u16at(0);
    let sw = u16at(2);
    let csr = u32at(24);
    Environment {
        cw,
        sw,
        tags: raw[4],
        top: ((sw >> 11) & 7) as u8,
        mxcsr: csr,
        mxcsr_mask: u32at(28),
        unmasked_x87: (sw & !cw & 0x3f) as u8,
        unmasked_simd: (csr & !(csr >> 7) & 0x3f) as u8,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_image() {
        let mut b = [0xa5; 512];
        b[..2].copy_from_slice(&0x37fu16.to_le_bytes());
        b[2..4].copy_from_slice(&0u16.to_le_bytes());
        b[4] = 0;
        b[24..28].copy_from_slice(&0x1f80u32.to_le_bytes());
        let e = decode(&b);
        assert_eq!(
            (
                e.cw,
                e.sw,
                e.tags,
                e.top,
                e.mxcsr,
                e.unmasked_x87,
                e.unmasked_simd
            ),
            (0x37f, 0, 0, 0, 0x1f80, 0, 0)
        );
    }
    #[test]
    fn raw_top_tags_and_masks() {
        let mut b = [0; 512];
        b[..2].copy_from_slice(&0x37au16.to_le_bytes());
        b[2..4].copy_from_slice(&0x2885u16.to_le_bytes());
        b[4] = 0xe0;
        b[24..28].copy_from_slice(&0x1d85u32.to_le_bytes());
        b[28..32].copy_from_slice(&0xffbfu32.to_le_bytes());
        let e = decode(&b);
        assert_eq!(
            (e.top, e.tags, e.unmasked_x87, e.unmasked_simd, e.mxcsr_mask),
            (5, 0xe0, 5, 4, 0xffbf)
        );
    }
    #[test]
    fn no_stale_data_interpretation() {
        let a = decode(&[0; 512]);
        let mut b = [0; 512];
        b[32..].fill(0xff);
        assert_eq!(a, decode(&b));
    }
}
