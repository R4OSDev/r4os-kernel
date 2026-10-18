//! Pure standard PAT/MTRR interpretation. Hardware capture and early CPU
//! publication belong to pat.rs; mapping owners retain their alias lifetimes.
pub(crate) const UC: u8 = 0;
pub(crate) const WC: u8 = 1;
pub(crate) const WT: u8 = 4;
pub(crate) const WP: u8 = 5;
pub(crate) const WB: u8 = 6;
pub(crate) const UC_MINUS: u8 = 7;
pub(crate) fn valid_type(value: u8) -> bool {
    matches!(value, UC | WC | WT | WP | WB | UC_MINUS)
}
pub(crate) fn valid_pat(table: u64) -> bool {
    (0..8).all(|i| valid_type((table >> (i * 8)) as u8))
}
pub(crate) fn selector(table: u64, kind: u8) -> Option<u8> {
    if !valid_pat(table) || !valid_type(kind) {
        return None;
    }
    (0..8).find(|i| (table >> (i * 8)) as u8 == kind)
}
pub(crate) fn encode(index: u8, huge: bool) -> Option<u64> {
    if index >= 8 {
        return None;
    }
    Some(u64::from(index & 3) << 3 | u64::from(index >> 2) << if huge { 12 } else { 7 })
}
pub(crate) fn decode(flags: u64, huge: bool) -> u8 {
    ((flags >> 3) & 3) as u8 | (((flags >> if huge { 12 } else { 7 }) & 1) as u8) << 2
}
/// Intel SDM table13-7 / AMD APM2 table7-11. CD is normalized to UC for
/// the existing kernel UC/WC policy; this is not a new memory ordering API.
pub(crate) fn effective(pat: u8, mtrr: u8) -> Option<u8> {
    if !valid_type(pat) || !valid_type(mtrr) || mtrr == UC_MINUS {
        return None;
    }
    Some(match pat {
        UC => UC,
        WC => WC,
        UC_MINUS => {
            if mtrr == WC {
                WC
            } else {
                UC
            }
        }
        WB => mtrr,
        WT => match mtrr {
            WB | WT => WT,
            _ => UC,
        },
        // WT/WP has implementation-dependent behavior across vendor/model
        // tables. It must not manufacture a common cache-type grant.
        WP => match mtrr {
            WB | WP => WP,
            WT => return None,
            _ => UC,
        },
        _ => return None,
    })
}
pub(crate) fn cacheable(kind: u8) -> bool {
    matches!(kind, WB | WT | WP)
}
pub(crate) const MAX_VARIABLE: usize = 255;
#[derive(Clone, Copy, Default)]
pub(crate) struct Range {
    pub start: u64,
    pub end: u64,
    pub kind: u8,
}
#[derive(Clone, Copy)]
pub(crate) struct Mtrrs {
    pub limit: u64,
    pub default: u8,
    pub enabled: bool,
    pub fixed_enabled: bool,
    pub fixed: [u8; 88],
    pub ranges: [Range; MAX_VARIABLE],
    pub count: usize,
}
impl Mtrrs {
    pub(crate) const EMPTY: Self = Self {
        limit: 0,
        default: UC,
        enabled: false,
        fixed_enabled: false,
        fixed: [0; 88],
        ranges: [Range {
            start: 0,
            end: 0,
            kind: UC,
        }; MAX_VARIABLE],
        count: 0,
    };
    pub(crate) fn append(&mut self, base: u64, mask: u64) -> bool {
        if mask & (1 << 11) == 0 || !self.enabled {
            return true;
        }
        let kind = base as u8;
        if !valid_type(kind) || kind == UC_MINUS || self.limit < 4096 || self.count == MAX_VARIABLE
        {
            return false;
        }
        let address_mask = (self.limit - 1) & !4095;
        let mask = mask & address_mask;
        let bytes = (!mask & address_mask) + 4096;
        let start = base & address_mask;
        if !bytes.is_power_of_two()
            || start & (bytes - 1) != 0
            || mask != (address_mask & !(bytes - 1))
            || start + bytes > self.limit
        {
            return false;
        }
        self.ranges[self.count] = Range {
            start,
            end: start + bytes,
            kind,
        };
        self.count += 1;
        true
    }
    pub(crate) fn at(&self, address: u64) -> Option<u8> {
        if address >= self.limit {
            return None;
        }
        if !self.enabled {
            return Some(UC);
        }
        if self.fixed_enabled && address < 1024 * 1024 {
            let slot = if address < 512 * 1024 {
                address / (64 * 1024)
            } else if address < 768 * 1024 {
                8 + (address - 512 * 1024) / (16 * 1024)
            } else {
                24 + (address - 768 * 1024) / 4096
            };
            return Some(self.fixed[slot as usize]);
        }
        let mut kinds = 0u8;
        for range in &self.ranges[..self.count] {
            if range.start <= address && address < range.end {
                kinds |= 1 << range.kind;
            }
        }
        if kinds & 1 != 0 {
            return Some(UC);
        }
        if kinds == 0 {
            return Some(self.default);
        }
        if kinds.count_ones() == 1 {
            return Some(kinds.trailing_zeros() as u8);
        }
        if kinds == (1 << WB) | (1 << WT) {
            return Some(WT);
        }
        None // Architecturally undefined overlap is not a cache grant.
    }
    /// Compare effective characterization, not register count/order or
    /// disabled entries. Wider AP addressability is harmless to this root.
    pub(crate) fn equivalent(&self, other: &Self) -> bool {
        if self.limit == 0 || other.limit < self.limit {
            return false;
        }
        let mut points = [0u64; 1200];
        let mut count = 2;
        points[1] = self.limit;
        for snapshot in [self, other] {
            if snapshot.enabled {
                for range in &snapshot.ranges[..snapshot.count] {
                    for point in [range.start, range.end] {
                        if point < self.limit {
                            points[count] = point;
                            count += 1;
                        }
                    }
                }
                if snapshot.fixed_enabled {
                    for slot in 0..=88 {
                        let point = if slot <= 8 {
                            slot * 64 * 1024
                        } else if slot <= 24 {
                            512 * 1024 + (slot - 8) * 16 * 1024
                        } else {
                            768 * 1024 + (slot - 24) * 4096
                        };
                        if point < self.limit {
                            points[count] = point;
                            count += 1;
                        }
                    }
                }
            }
        }
        points[..count].sort_unstable();
        points[..count].iter().all(|&point| {
            point == self.limit
                || self
                    .at(point)
                    .is_some_and(|kind| Some(kind) == other.at(point))
        })
    }
}
