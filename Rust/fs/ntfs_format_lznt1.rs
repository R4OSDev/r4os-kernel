//! Exact original read-side LZNT1 chunk/back-reference decoder.
use super::ntfs_format_types::le16;
pub fn decompress(src: &[u8], dst: &mut [u8]) -> Option<usize> {
    let mut input = 0;
    let mut out = 0;
    while input + 2 <= src.len() && out < dst.len() {
        let h = le16(src, input);
        input += 2;
        if h == 0 {
            break;
        }
        if h & 0x7000 != 0x3000 {
            return None;
        }
        let n = usize::from(h & 0x0fff) + 1;
        let end = input + n;
        if end > src.len() {
            return None;
        }
        let chunk = out;
        if h & 0x8000 == 0 {
            if out + n > dst.len() {
                return None;
            }
            dst[out..out + n].copy_from_slice(&src[input..end]);
            out += n;
            input = end;
            continue;
        }
        while input < end && out < dst.len() {
            let mut flags = src[input];
            input += 1;
            let mut t = 0;
            while t < 8 && input < end {
                if flags & 1 == 0 {
                    if out + 1 > dst.len() {
                        return None;
                    }
                    dst[out] = src[input];
                    out += 1;
                    input += 1
                } else {
                    if input + 2 > end {
                        return None;
                    }
                    let tuple = le16(src, input);
                    input += 2;
                    let written = out - chunk;
                    if written == 0 {
                        return None;
                    }
                    let mut l = 0u8;
                    let mut u = written - 1;
                    while u >= 0x10 {
                        l = l
                            .checked_add(1)
                            .filter(|l| *l < 16)
                            .expect("NTFS LZNT1 u4 shift");
                        u >>= 1
                    }
                    let length = usize::from(tuple & (0x0fffu16 >> l)) + 3;
                    let shift = 12u16 - u16::from(l);
                    let offset = usize::from(tuple >> shift) + 1;
                    if offset > written || out + length > dst.len() {
                        return None;
                    }
                    for _ in 0..length {
                        dst[out] = dst[out - offset];
                        out += 1
                    }
                }
                flags >>= 1;
                if out >= dst.len() {
                    break;
                }
                t += 1
            }
        }
        input = end
    }
    Some(out)
}
