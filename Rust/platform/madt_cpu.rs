//! Complete bounded MADT CPU selection. Firmware records are not CPU slots:
//! disabled/duplicate records never consume the 64-slot execution capacity.
use crate::arch::smp_policy::{Candidate, MAX_CPUS, Plan};

const HEADER: usize = 44;
const MAX_BYTES: usize = 1024 * 1024;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Cpu {
    pub acpi_uid: u32,
    pub apic_id: u32,
    pub flags: u32,
    pub x2apic: bool,
}
impl Cpu {
    const EMPTY: Self = Self {
        acpi_uid: 0,
        apic_id: 0,
        flags: 0,
        x2apic: false,
    };
    fn eligible(self) -> bool {
        self.flags & 3 != 0 && self.apic_id != u32::MAX
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Catalog {
    pub entries: [Cpu; MAX_CPUS],
    pub plan: Plan,
    pub present: u64,
    pub uid_conflicts: u64,
    pub requires_x2apic: bool,
}
impl Catalog {
    pub const EMPTY: Self = Self {
        entries: [Cpu::EMPTY; MAX_CPUS],
        plan: Plan::EMPTY,
        present: 0,
        uid_conflicts: 0,
        requires_x2apic: false,
    };
    pub fn firmware_uid(&self, apic: u32) -> Result<Option<u32>, ()> {
        let Some(index) = self.plan.apic_ids[..self.plan.count as usize]
            .iter()
            .position(|&id| id == apic)
        else {
            return Ok(None);
        };
        let bit = 1u64 << index;
        if self.uid_conflicts & bit != 0 {
            Err(())
        } else if self.present & bit != 0 {
            Ok(Some(self.entries[index].acpi_uid))
        } else {
            // The BSP can be executable without an enabled firmware record.
            Ok(None)
        }
    }
}

#[derive(Clone)]
struct Entries<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl Iterator for Entries<'_> {
    type Item = Cpu;
    fn next(&mut self) -> Option<Cpu> {
        while self.offset < self.bytes.len() {
            let offset = self.offset;
            let bytes = self.bytes;
            self.offset += usize::from(bytes[offset + 1]);
            match bytes[offset] {
                0 => {
                    return Some(Cpu {
                        acpi_uid: u32::from(bytes[offset + 2]),
                        apic_id: u32::from(bytes[offset + 3]),
                        flags: read32(bytes, offset + 4),
                        x2apic: false,
                    });
                }
                9 => {
                    return Some(Cpu {
                        acpi_uid: read32(bytes, offset + 12),
                        apic_id: read32(bytes, offset + 4),
                        flags: read32(bytes, offset + 8),
                        x2apic: true,
                    });
                }
                _ => {}
            }
        }
        None
    }
}
fn read32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
pub(crate) fn decode(bytes: &[u8], bsp: u32) -> Option<Catalog> {
    if bsp == u32::MAX
        || bytes.len() < HEADER
        || bytes.len() > MAX_BYTES
        || &bytes[..4] != b"APIC"
        || read32(bytes, 4) as usize != bytes.len()
        || bytes.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)) != 0
    {
        return None;
    }
    // Validate the whole record chain before retaining any executable slot.
    // One trailing byte, an undersized known CPU record or zero length rejects
    // the catalog, rather than publishing a successful truncated prefix.
    let mut offset = HEADER;
    while offset < bytes.len() {
        if bytes.len() - offset < 2 {
            return None;
        }
        let length = usize::from(bytes[offset + 1]);
        if length < 2
            || length > bytes.len() - offset
            || (bytes[offset] == 0 && length < 8)
            || (bytes[offset] == 9 && length < 16)
        {
            return None;
        }
        offset += length;
    }
    let entries = Entries {
        bytes,
        offset: HEADER,
    };
    let mut result = Catalog {
        plan: Plan::new(bsp),
        ..Catalog::EMPTY
    };
    for entry in entries.clone() {
        result.requires_x2apic |= entry.eligible() && entry.apic_id >= 0xff;
        result.plan.consider(Candidate {
            apic_id: entry.apic_id,
            enabled: entry.flags & 1 != 0,
            online_capable: entry.flags & 2 != 0,
        });
        if !entry.eligible() {
            continue;
        }
        if let Some(index) = result.plan.apic_ids[..result.plan.count as usize]
            .iter()
            .position(|&id| id == entry.apic_id)
        {
            let bit = 1u64 << index;
            if result.present & bit != 0 {
                if result.entries[index].acpi_uid != entry.acpi_uid {
                    result.uid_conflicts |= bit;
                }
            } else {
                result.entries[index] = entry;
                result.present |= bit;
            }
        }
    }
    // Also detect UID aliases in omitted records, including aliases preceding
    // a late BSP record. Overflow never turns ambiguous firmware into a UID.
    for entry in entries.filter(|e| e.eligible()) {
        for (index, kept) in result
            .entries
            .iter()
            .enumerate()
            .take(result.plan.count as usize)
        {
            let bit = 1u64 << index;
            if result.present & bit != 0
                && kept.acpi_uid == entry.acpi_uid
                && kept.apic_id != entry.apic_id
            {
                result.uid_conflicts |= bit;
            }
        }
    }
    Some(result)
}

const _: () = {
    assert!(core::mem::size_of::<Cpu>() == 16);
    assert!(core::mem::offset_of!(Cpu, apic_id) == 4);
    assert!(core::mem::offset_of!(Cpu, flags) == 8);
    assert!(core::mem::offset_of!(Cpu, x2apic) == 12);
    assert!(core::mem::size_of::<Catalog>() == 1328);
};
