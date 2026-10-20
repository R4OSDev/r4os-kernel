//! Resident bounded records are assembled before their single sink call. No
//! formatter callback, pageable caller span or UART owner survives assembly.
pub(crate) struct Record {
    bytes: [u8; 512],
    len: usize,
    overflow: bool,
}
impl Record {
    pub(crate) fn new() -> Self {
        Self {
            bytes: [0; 512],
            len: 0,
            overflow: false,
        }
    }
    pub(crate) fn text(&mut self, text: &[u8]) {
        if text.len() > self.bytes.len() - self.len {
            self.overflow = true;
            return;
        }
        self.bytes[self.len..self.len + text.len()].copy_from_slice(text);
        self.len += text.len();
    }
    pub(crate) fn decimal(&mut self, mut value: u64) {
        let mut digits = [0u8; 20];
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.text(&digits[start..]);
    }
    pub(crate) fn hex(&mut self, value: u64) {
        let mut digits = [0u8; 16];
        for (i, byte) in digits.iter_mut().enumerate() {
            *byte = b"0123456789ABCDEF"[((value >> ((15 - i) * 4)) & 15) as usize];
        }
        self.text(&digits);
    }
    pub(crate) fn as_bytes(&self) -> &[u8] {
        if self.overflow {
            b"[LOG] bounded record overflow\r\n"
        } else {
            &self.bytes[..self.len]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_resident_record_or_explicit_whole_record_failure() {
        let mut record = Record::new();
        record.text(b"[MOD] generation=");
        record.decimal(u64::MAX);
        record.text(b" image=0x");
        record.hex(0x0123456789abcdef);
        record.text(b"\r\n");
        assert_eq!(
            record.as_bytes(),
            b"[MOD] generation=18446744073709551615 image=0x0123456789ABCDEF\r\n"
        );
        record.text(&[b'A'; 512]);
        assert_eq!(record.as_bytes(), b"[LOG] bounded record overflow\r\n");
    }
}
