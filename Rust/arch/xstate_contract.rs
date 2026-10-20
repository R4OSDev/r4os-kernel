//! Pure standard-layout storage contract; no CPU instruction or mutable owner.
pub const CAPACITY: usize = 4096;
pub const ALIGNMENT: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contract {
    pub backend: u32,
    pub mask: u64,
    pub bytes: u32,
}
impl Contract {
    pub const DISABLED: Self = Self {
        backend: 0,
        mask: 0,
        bytes: 0,
    };
    pub const fn valid(self) -> bool {
        match self.backend {
            1 => self.mask == 0 && self.bytes == 512,
            2 => {
                (self.mask == 3 || self.mask == 7)
                    && self.bytes >= if self.mask == 7 { 832 } else { 576 }
                    && (self.bytes as usize) <= CAPACITY
            }
            _ => false,
        }
    }
    pub const fn buffer(self, address: usize, len: usize) -> bool {
        self.valid()
            && address != 0
            && address & (ALIGNMENT - 1) == 0
            && len >= self.bytes as usize
            && address.checked_add(self.bytes as usize).is_some()
    }
    pub const fn standard(self) -> bool {
        self.backend == 2
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_exact() {
        let c = Contract {
            backend: 1,
            mask: 0,
            bytes: 512,
        };
        assert!(c.valid());
        assert!(!Contract { bytes: 511, ..c }.valid());
        assert!(!Contract { mask: 3, ..c }.valid());
    }
    #[test]
    fn standard_bounds() {
        for mask in [3, 7] {
            assert!(
                Contract {
                    backend: 2,
                    mask,
                    bytes: if mask == 7 { 832 } else { 576 }
                }
                .valid()
            );
            assert!(
                Contract {
                    backend: 2,
                    mask,
                    bytes: 4096
                }
                .valid()
            );
            assert!(
                !Contract {
                    backend: 2,
                    mask,
                    bytes: 4097
                }
                .valid()
            );
            assert!(
                !Contract {
                    backend: 2,
                    mask,
                    bytes: 575
                }
                .valid()
            );
        }
        for mask in [0, 1, 2, 4, 5, 6, 15, u64::MAX] {
            assert!(
                !Contract {
                    backend: 2,
                    mask,
                    bytes: 832
                }
                .valid()
            );
        }
    }
    #[test]
    fn rejects_without_memory_access() {
        let c = Contract {
            backend: 2,
            mask: 7,
            bytes: 832,
        };
        assert!(!c.buffer(0, 4096));
        assert!(!c.buffer(65, 4096));
        assert!(!c.buffer(64, 831));
        assert!(!c.buffer(usize::MAX & !(ALIGNMENT - 1), 4096));
        assert!(c.buffer(64, 832));
        assert!(c.buffer(64, 4096));
        assert!(!Contract::DISABLED.buffer(64, 4096));
    }
    #[test]
    fn fixed_capacity_alignment() {
        assert_eq!(CAPACITY % ALIGNMENT, 0);
        assert!(ALIGNMENT.is_power_of_two());
        assert!(
            !Contract {
                backend: 9,
                mask: 7,
                bytes: 832
            }
            .valid()
        );
    }
}
