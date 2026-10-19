//! Finite local CPUID capture. Missing/vendor-inapplicable rows remain unknown;
//! no fallback leaf value is allowed to masquerade as a requested feature.
use core::arch::x86_64::CpuidResult;

pub(crate) const LEAVES: [(u32, u32); 21] = [
    (0, 0),
    (1, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (7, 1),
    (0x0a, 0),
    (0x0d, 0),
    (0x0d, 1),
    (0x15, 0),
    (0x16, 0),
    (0x8000_0000, 0),
    (0x8000_0001, 0),
    (0x8000_0007, 0),
    (0x8000_0008, 0),
    (0x8000_0022, 0),
    (0x4000_0000, 0),
    (0x4000_0001, 0),
    (0x8000_0002, 0),
    (0x8000_0003, 0),
    (0x8000_0004, 0),
];
pub(crate) const WORDS: usize = 2 + LEAVES.len() * 2;
pub(crate) const INVALID_BASIC: u64 = 1;
pub(crate) const INVALID_EXTENDED: u64 = 2;
pub(crate) const INVALID_HYPERVISOR: u64 = 4;
pub(crate) const LEGACY_KVM_MAXIMUM: u64 = 8;
const EMPTY: CpuidResult = CpuidResult {
    eax: 0,
    ebx: 0,
    ecx: 0,
    edx: 0,
};

#[derive(Clone, Copy)]
pub(crate) struct Raw {
    pub known: u64,
    pub issues: u64,
    pub rows: [CpuidResult; LEAVES.len()],
}
impl Raw {
    pub(crate) const EMPTY: Self = Self {
        known: 0,
        issues: 0,
        rows: [EMPTY; LEAVES.len()],
    };
    pub(crate) fn get(&self, leaf: u32, subleaf: u32) -> Option<CpuidResult> {
        LEAVES
            .iter()
            .position(|&row| row == (leaf, subleaf))
            .filter(|&i| self.known & (1u64 << i) != 0)
            .map(|i| self.rows[i])
    }
    fn put(&mut self, leaf: u32, subleaf: u32, value: CpuidResult) {
        let i = LEAVES
            .iter()
            .position(|&row| row == (leaf, subleaf))
            .expect("Defined finite CPUID row");
        self.rows[i] = value;
        self.known |= 1u64 << i;
    }
    pub(crate) fn vendor(&self) -> [u8; 12] {
        self.get(0, 0)
            .map_or([0; 12], |r| words([r.ebx, r.edx, r.ecx]))
    }
    pub(crate) fn hypervisor(&self) -> [u8; 12] {
        self.get(0x4000_0000, 0)
            .map_or([0; 12], |r| words([r.ebx, r.ecx, r.edx]))
    }
    pub(crate) fn hypervisor_maximum(&self) -> Option<u32> {
        let raw = self.get(0x4000_0000, 0)?.eax;
        if self.issues & INVALID_HYPERVISOR != 0 {
            None
        } else if self.issues & LEGACY_KVM_MAXIMUM != 0 {
            Some(0x4000_0001)
        } else {
            Some(raw)
        }
    }
    pub(crate) fn signature(&self) -> Option<(u32, u32, u32)> {
        let r = self.get(1, 0)?.eax;
        let base = (r >> 8) & 15;
        let family = base + if base == 15 { (r >> 20) & 255 } else { 0 };
        let model = ((r >> 4) & 15)
            + if base == 6 || base == 15 {
                ((r >> 16) & 15) << 4
            } else {
                0
            };
        Some((family, model, r & 15))
    }
    pub(crate) fn brand(&self) -> Option<[u8; 48]> {
        let mut result = [0; 48];
        for (i, leaf) in (0x8000_0002..=0x8000_0004).enumerate() {
            let r = self.get(leaf, 0)?;
            for (j, value) in [r.eax, r.ebx, r.ecx, r.edx].into_iter().enumerate() {
                result[i * 16 + j * 4..i * 16 + j * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        Some(result)
    }
    pub(crate) fn encode(&self) -> [u64; WORDS] {
        let mut result = [0; WORDS];
        result[0] = self.known;
        result[1] = self.issues;
        for (i, r) in self.rows.iter().enumerate() {
            result[2 + i * 2] = u64::from(r.eax) | (u64::from(r.ebx) << 32);
            result[3 + i * 2] = u64::from(r.ecx) | (u64::from(r.edx) << 32);
        }
        result
    }
    pub(crate) fn decode(words: [u64; WORDS]) -> Option<Self> {
        if words[0] >> LEAVES.len() != 0 || words[1] & !15 != 0 {
            return None;
        }
        let mut result = Self::EMPTY;
        result.known = words[0];
        result.issues = words[1];
        for (i, r) in result.rows.iter_mut().enumerate() {
            *r = CpuidResult {
                eax: words[2 + i * 2] as u32,
                ebx: (words[2 + i * 2] >> 32) as u32,
                ecx: words[3 + i * 2] as u32,
                edx: (words[3 + i * 2] >> 32) as u32,
            };
        }
        Some(result)
    }
}
fn words(values: [u32; 3]) -> [u8; 12] {
    let mut result = [0; 12];
    for (i, v) in values.into_iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    result
}

pub(crate) fn capture(mut query: impl FnMut(u32, u32) -> CpuidResult) -> Raw {
    let mut result = Raw::EMPTY;
    let base = query(0, 0);
    result.put(0, 0, base);
    let extended = query(0x8000_0000, 0);
    result.put(0x8000_0000, 0, extended);
    let basic_valid = base.eax < 0x4000_0000;
    let ext_valid = (0x8000_0000..0xc000_0000).contains(&extended.eax);
    if !basic_valid {
        result.issues |= INVALID_BASIC;
    }
    if extended.eax != 0 && !ext_valid {
        result.issues |= INVALID_EXTENDED;
    }
    let amd = result.vendor() == *b"AuthenticAMD";
    for &(leaf, subleaf) in &LEAVES {
        if result.get(leaf, subleaf).is_some() || leaf >= 0x4000_0000 {
            continue;
        }
        if !basic_valid || leaf > base.eax {
            continue;
        }
        if leaf == 7 && subleaf != 0 && result.get(7, 0).is_none_or(|r| subleaf > r.eax) {
            continue;
        }
        if leaf == 0x0d && result.get(1, 0).is_none_or(|r| r.ecx & (1 << 26) == 0) {
            continue;
        }
        // MONITOR/MWAIT's leaf is reserved when its enabling feature is absent.
        if leaf == 5 && result.get(1, 0).is_none_or(|r| r.ecx & (1 << 3) == 0) {
            continue;
        }
        // Architectural PMU leaf A is Intel's interface; AMD has its own row.
        if leaf == 0x0a && result.vendor() != *b"GenuineIntel" {
            continue;
        }
        result.put(leaf, subleaf, query(leaf, subleaf));
    }
    for &(leaf, subleaf) in &LEAVES {
        if leaf <= 0x8000_0000 || !ext_valid || leaf > extended.eax {
            continue;
        }
        if leaf == 0x8000_0022 && !amd {
            continue;
        }
        result.put(leaf, subleaf, query(leaf, subleaf));
    }
    if result.get(1, 0).is_some_and(|r| r.ecx & (1 << 31) != 0) {
        let hv = query(0x4000_0000, 0);
        result.put(0x4000_0000, 0, hv);
        if hv.eax == 0 && result.hypervisor() == *b"KVMKVMKVM\0\0\0" {
            result.issues |= LEGACY_KVM_MAXIMUM;
        } else if !(0x4000_0000..0x4001_0000).contains(&hv.eax) {
            result.issues |= INVALID_HYPERVISOR;
        }
        if result
            .hypervisor_maximum()
            .is_some_and(|maximum| maximum >= 0x4000_0001)
        {
            result.put(0x4000_0001, 0, query(0x4000_0001, 0));
        }
    }
    result
}

const _: () = {
    assert!(LEAVES.len() < 64);
};
