//! Validate the byte rectangle while the complete source BO remains retained.
pub fn valid(
    width: u32,
    height: u32,
    pitch: u64,
    offset: u64,
    bytes: u64,
    rows: u32,
    partial: bool,
) -> bool {
    let row = u64::from(width) * 4;
    if width == 0 || height == 0 || pitch < row || bytes == 0 || rows == 0 || bytes & 3 != 0 {
        return false;
    }
    if (offset % pitch) & 3 != 0 {
        return false;
    }
    if !partial {
        return offset == 0 && bytes == row && rows == height;
    }
    let y = offset / pitch;
    let x = offset % pitch;
    y < u64::from(height) && x < row && bytes <= row - x && u64::from(rows) <= u64::from(height) - y
}

#[cfg(test)]
mod tests {
    use super::valid;
    #[test]
    fn complete_images_and_padded_rectangles_preserve_bounds() {
        assert!(valid(8, 5, 36, 0, 32, 5, false));
        assert!(!valid(8, 5, 36, 0, 28, 5, false));
        assert!(valid(8, 5, 36, 4 * 36 + 28, 4, 1, true));
        for (offset, bytes, rows) in [
            (1, 4, 1),
            (32, 4, 1),
            (35, 4, 1),
            (4 * 36 + 28, 8, 1),
            (4 * 36, 4, 2),
            (0, 0, 1),
            (0, 4, 0),
            (u64::MAX, 4, 1),
            (0, u64::MAX, 1),
        ] {
            assert!(!valid(8, 5, 36, offset, bytes, rows, true));
        }
        assert!(valid(8, 5, 37, 37 + 4, 4, 1, true));
        assert!(!valid(8, 5, 0, 0, 4, 1, true));
        assert!(!valid(8, 5, 28, 0, 4, 1, true));
        for y in 0..7u64 {
            for x in 0..10u64 {
                for h in 0..7u32 {
                    for w in 0..10u64 {
                        let expected =
                            x < 8 && y < 5 && w > 0 && h > 0 && x + w <= 8 && y + u64::from(h) <= 5;
                        assert_eq!(
                            valid(8, 5, 48, y * 48 + x * 4, w * 4, h, true),
                            expected,
                            "{x},{y} {w}x{h}"
                        );
                    }
                }
            }
        }
    }
}
