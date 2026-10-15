//! Exact read-only GPT/MBR/EBR inventory; media I/O stays with the existing block owner.
use crate::storage::gpt;
use core::{ffi::c_void, ptr, slice};
pub const MAX_PARTITIONS: usize = 128;
pub const BIOS: [u8; 16] = [
    0x48, 0x61, 0x68, 0x21, 0x49, 0x64, 0x6f, 0x6e, 0x74, 0x4e, 0x65, 0x65, 0x64, 0x45, 0x46, 0x49,
];
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Reader {
    pub ctx: *mut c_void,
    pub read: unsafe extern "C" fn(*mut c_void, u64, *mut u8) -> bool,
    pub sector_bytes: u32,
    pub sectors: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Partition {
    pub number: u32,
    pub first_lba: u64,
    pub sector_count: u64,
    pub unique_guid: [u8; 16],
    pub type_guid: [u8; 16],
    pub mbr_type: u8,
    pub attributes: u64,
    pub name: [u16; 36],
}
impl Partition {
    pub const EMPTY: Self = Self {
        number: 0,
        first_lba: 0,
        sector_count: 0,
        unique_guid: [0; 16],
        type_guid: [0; 16],
        mbr_type: 0,
        attributes: 0,
        name: [0; 36],
    };
    pub fn end(self) -> u64 {
        self.first_lba + self.sector_count
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Label {
    pub data: *const u8,
    pub len: usize,
}
impl Label {
    pub const fn new(s: &'static [u8]) -> Self {
        Self {
            data: s.as_ptr(),
            len: s.len(),
        }
    }
}
#[repr(C)]
pub struct Table {
    pub kind: u8,
    pub valid: bool,
    pub reason: Label,
    pub disk_guid: [u8; 16],
    pub mbr_disk_id: u32,
    pub first_usable: u64,
    pub last_usable: u64,
    pub partitions: *mut Partition,
    pub count: usize,
}
impl Table {
    fn items(&self) -> &[Partition] {
        unsafe { slice::from_raw_parts(self.partitions, self.count) }
    }
}
enum Error {
    Gpt(gpt::Error),
    OutOfRange,
    ReadFailed,
    UnsupportedSectorSize,
    UnsupportedCapacity,
    NoPartitionTable,
    BadProtectiveMbr,
    HybridMbr,
    MultipleExtendedPartitions,
    PartitionOverlap,
    TooManyPartitions,
    ExtendedCycle,
    ExtendedMetadataOverlap,
    BadExtendedSignature,
    BadLogicalPartition,
    BadExtendedEntries,
    BadExtendedLink,
    MissingDiskGuid,
    GptCopiesDiffer,
    MissingPartitionGuid,
    BadEntriesCrc,
    DuplicatePartitionGuid,
    PartitionOutOfRange,
}
impl From<gpt::Error> for Error {
    fn from(e: gpt::Error) -> Self {
        Self::Gpt(e)
    }
}
impl Error {
    fn label(&self) -> &'static [u8] {
        match self {
            Self::Gpt(e) => match e {
                gpt::Error::ShortSector => b"ShortSector",
                gpt::Error::BadSignature => b"BadSignature",
                gpt::Error::UnsupportedRevision => b"UnsupportedRevision",
                gpt::Error::BadHeaderSize => b"BadHeaderSize",
                gpt::Error::NonzeroReserved => b"NonzeroReserved",
                gpt::Error::BadHeaderCrc => b"BadHeaderCrc",
                gpt::Error::WrongHeaderLba => b"WrongHeaderLba",
                gpt::Error::BadBackupLba => b"BadBackupLba",
                gpt::Error::BadUsableRange => b"BadUsableRange",
                gpt::Error::UnsupportedEntryLayout => b"UnsupportedEntryLayout",
                gpt::Error::EntryArrayOutOfRange => b"EntryArrayOutOfRange",
                gpt::Error::ShortEntry => b"ShortEntry",
                gpt::Error::BadPartitionRange => b"BadPartitionRange",
            },
            Self::OutOfRange => b"OutOfRange",
            Self::ReadFailed => b"ReadFailed",
            Self::UnsupportedSectorSize => b"UnsupportedSectorSize",
            Self::UnsupportedCapacity => b"UnsupportedCapacity",
            Self::NoPartitionTable => b"NoPartitionTable",
            Self::BadProtectiveMbr => b"BadProtectiveMbr",
            Self::HybridMbr => b"HybridMbr",
            Self::MultipleExtendedPartitions => b"MultipleExtendedPartitions",
            Self::PartitionOverlap => b"PartitionOverlap",
            Self::TooManyPartitions => b"TooManyPartitions",
            Self::ExtendedCycle => b"ExtendedCycle",
            Self::ExtendedMetadataOverlap => b"ExtendedMetadataOverlap",
            Self::BadExtendedSignature => b"BadExtendedSignature",
            Self::BadLogicalPartition => b"BadLogicalPartition",
            Self::BadExtendedEntries => b"BadExtendedEntries",
            Self::BadExtendedLink => b"BadExtendedLink",
            Self::MissingDiskGuid => b"MissingDiskGuid",
            Self::GptCopiesDiffer => b"GptCopiesDiffer",
            Self::MissingPartitionGuid => b"MissingPartitionGuid",
            Self::BadEntriesCrc => b"BadEntriesCrc",
            Self::DuplicatePartitionGuid => b"DuplicatePartitionGuid",
            Self::PartitionOutOfRange => b"PartitionOutOfRange",
        }
    }
}
impl Reader {
    fn sector(&self, lba: u64, out: &mut [u8; 512]) -> Result<(), Error> {
        if lba >= self.sectors {
            return Err(Error::OutOfRange);
        }
        if !unsafe { (self.read)(self.ctx, lba, out.as_mut_ptr()) } {
            return Err(Error::ReadFailed);
        }
        Ok(())
    }
}
fn extended(kind: u8) -> bool {
    kind == 5 || kind == 0x0f || kind == 0x85
}
fn bounds(p: Partition, sectors: u64) -> Result<(), Error> {
    if p.first_lba == 0
        || p.first_lba >= sectors
        || p.sector_count == 0
        || p.sector_count > sectors - p.first_lba
    {
        return Err(Error::PartitionOutOfRange);
    }
    Ok(())
}
unsafe fn append(out: *mut Table, p: Partition) -> Result<(), Error> {
    let table = unsafe { &mut *out };
    for old in table.items() {
        if p.first_lba < old.end() && p.end() > old.first_lba {
            return Err(Error::PartitionOverlap);
        }
        if p.unique_guid != [0; 16] && old.unique_guid == p.unique_guid {
            return Err(Error::DuplicatePartitionGuid);
        }
    }
    if table.count == MAX_PARTITIONS {
        return Err(Error::TooManyPartitions);
    }
    unsafe { table.partitions.add(table.count).write(p) };
    table.count += 1;
    Ok(())
}
unsafe fn scan_gpt(reader: Reader, out: *mut Table) -> Result<(), Error> {
    let mut a = [0; 512];
    let mut b = [0; 512];
    reader.sector(1, &mut a)?;
    let primary = gpt::parse_header(&a, 1, reader.sectors)?;
    if 512 % primary.entry_size != 0 {
        return Err(Error::Gpt(gpt::Error::UnsupportedEntryLayout));
    }
    unsafe {
        (*out).disk_guid = primary.disk_guid;
        (*out).first_usable = primary.first_usable_lba;
        (*out).last_usable = primary.last_usable_lba
    };
    if primary.disk_guid == [0; 16] {
        return Err(Error::MissingDiskGuid);
    }
    reader.sector(reader.sectors - 1, &mut b)?;
    let backup = gpt::parse_header(&b, reader.sectors - 1, reader.sectors)?;
    if primary.disk_guid != backup.disk_guid
        || primary.first_usable_lba != backup.first_usable_lba
        || primary.last_usable_lba != backup.last_usable_lba
        || primary.entry_count != backup.entry_count
        || primary.entry_size != backup.entry_size
        || primary.entries_crc32 != backup.entries_crc32
    {
        return Err(Error::GptCopiesDiffer);
    }
    let mut crc = gpt::Crc32::default();
    let mut remaining = primary.entry_bytes();
    let mut index = 0;
    let mut sector_index = 0;
    while remaining != 0 {
        reader.sector(primary.entries_lba + sector_index, &mut a)?;
        reader.sector(backup.entries_lba + sector_index, &mut b)?;
        let take = remaining.min(512) as usize;
        if a[..take] != b[..take] {
            return Err(Error::GptCopiesDiffer);
        }
        crc.update(&a[..take]);
        let mut offset = 0;
        while offset < take {
            index += 1;
            if let Some(parsed) =
                gpt::parse_partition(&a[offset..offset + primary.entry_size as usize], primary)?
            {
                if parsed.unique_guid == [0; 16] {
                    return Err(Error::MissingPartitionGuid);
                }
                unsafe {
                    append(
                        out,
                        Partition {
                            number: index,
                            first_lba: parsed.first_lba,
                            sector_count: parsed.sector_count(),
                            unique_guid: parsed.unique_guid,
                            type_guid: parsed.type_guid,
                            attributes: parsed.attributes,
                            name: parsed.name_utf16,
                            ..Partition::EMPTY
                        },
                    )
                }?;
            }
            offset += primary.entry_size as usize;
        }
        remaining -= take as u64;
        sector_index += 1;
    }
    if crc.finish() != primary.entries_crc32 {
        return Err(Error::BadEntriesCrc);
    }
    Ok(())
}
unsafe fn scan_checked(reader: Reader, out: *mut Table) -> Result<(), Error> {
    if reader.sector_bytes != 512 {
        return Err(Error::UnsupportedSectorSize);
    }
    if reader.sectors == 0 || reader.sectors > u64::MAX / 512 {
        return Err(Error::UnsupportedCapacity);
    }
    let mut mbr = [0; 512];
    reader.sector(0, &mut mbr)?;
    if mbr[510] != 0x55 || mbr[511] != 0xaa {
        return Err(Error::NoPartitionTable);
    }
    unsafe { (*out).mbr_disk_id = gpt::le32(&mbr[440..444]) };
    let mut protective = 0;
    let mut occupied = 0;
    for i in 0..4 {
        let entry = &mbr[446 + i * 16..462 + i * 16];
        if entry[4] == 0 {
            continue;
        }
        occupied += 1;
        if entry[4] == 0xee {
            protective += 1;
            if gpt::le32(&entry[8..12]) != 1
                || u64::from(gpt::le32(&entry[12..16]))
                    != (reader.sectors - 1).min(u64::from(u32::MAX))
            {
                return Err(Error::BadProtectiveMbr);
            }
        }
    }
    if protective != 0 {
        unsafe { (*out).kind = 2 };
        if protective != 1 || occupied != 1 {
            return Err(Error::HybridMbr);
        }
        return unsafe { scan_gpt(reader, out) };
    }
    unsafe {
        (*out).kind = 1;
        (*out).first_usable = 1;
        (*out).last_usable = reader.sectors - 1
    };
    let mut extended_base = 0;
    let mut extended_end = 0;
    for i in 0..4 {
        let raw = &mbr[446 + i * 16..462 + i * 16];
        if raw[4] == 0 {
            continue;
        }
        let p = Partition {
            number: (i + 1) as u32,
            mbr_type: raw[4],
            first_lba: u64::from(gpt::le32(&raw[8..12])),
            sector_count: u64::from(gpt::le32(&raw[12..16])),
            ..Partition::EMPTY
        };
        bounds(p, reader.sectors)?;
        if extended(p.mbr_type) {
            if extended_base != 0 {
                return Err(Error::MultipleExtendedPartitions);
            }
            extended_base = p.first_lba;
            extended_end = p.end();
        } else {
            unsafe { append(out, p) }?;
        }
    }
    if extended_base != 0 {
        for p in unsafe { (&*out).items() } {
            if p.first_lba < extended_end && p.end() > extended_base {
                return Err(Error::PartitionOverlap);
            }
        }
        let mut ebr_lba = extended_base;
        let mut seen = [0; MAX_PARTITIONS];
        let mut seen_count = 0;
        loop {
            if seen_count == MAX_PARTITIONS {
                return Err(Error::TooManyPartitions);
            }
            if seen[..seen_count].contains(&ebr_lba) {
                return Err(Error::ExtendedCycle);
            }
            for p in unsafe { (&*out).items() } {
                if ebr_lba >= p.first_lba && ebr_lba < p.end() {
                    return Err(Error::ExtendedMetadataOverlap);
                }
            }
            seen[seen_count] = ebr_lba;
            seen_count += 1;
            let mut ebr = [0; 512];
            reader.sector(ebr_lba, &mut ebr)?;
            if ebr[510] != 0x55 || ebr[511] != 0xaa {
                return Err(Error::BadExtendedSignature);
            }
            let first = &ebr[446..462];
            if first[4] == 0 || extended(first[4]) {
                return Err(Error::BadLogicalPartition);
            }
            let p = Partition {
                number: (4 + seen_count) as u32,
                mbr_type: first[4],
                first_lba: ebr_lba + u64::from(gpt::le32(&first[8..12])),
                sector_count: u64::from(gpt::le32(&first[12..16])),
                ..Partition::EMPTY
            };
            bounds(p, reader.sectors)?;
            if p.first_lba <= ebr_lba || p.end() > extended_end {
                return Err(Error::BadLogicalPartition);
            }
            unsafe { append(out, p) }?;
            if ebr[486] != 0 || ebr[502] != 0 {
                return Err(Error::BadExtendedEntries);
            }
            let next = &ebr[462..478];
            if next[4] == 0 {
                break;
            }
            if !extended(next[4]) {
                return Err(Error::BadExtendedLink);
            }
            ebr_lba = extended_base + u64::from(gpt::le32(&next[8..12]));
            if ebr_lba < extended_base || ebr_lba >= extended_end {
                return Err(Error::BadExtendedLink);
            }
        }
    }
    Ok(())
}
/// # Safety
/// Caller retains exclusive resident result through the entire scan. Reader
/// callback/context remain alive; each exact 512-byte output is written by the
/// unchanged backend before return. Only raw result identity spans I/O; no
/// table reference survives a callback. No inventory pointer escapes its owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_partition_scan(reader: *const Reader, out: *mut Table) {
    let reader = unsafe { reader.read() };
    let partitions = unsafe { (*out).partitions };
    unsafe {
        ptr::write_bytes(partitions, 0, MAX_PARTITIONS);
        ptr::write_bytes(out, 0, 1);
        (*out).partitions = partitions;
        (*out).reason = Label::new(b"not-scanned")
    };
    match unsafe { scan_checked(reader, out) } {
        Ok(()) => unsafe {
            (*out).valid = true;
            (*out).reason = Label::new(b"ok")
        },
        Err(e) => unsafe { (*out).reason = Label::new(e.label()) },
    }
}
/// # Safety
/// Partition is one readable initialized C record; no borrow is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_partition_end(p: *const Partition) -> u64 {
    unsafe { p.read() }.end()
}
const _: () = {
    assert!(core::mem::size_of::<Reader>() == 32);
    assert!(core::mem::size_of::<Partition>() == 144);
    assert!(core::mem::offset_of!(Partition, attributes) == 64);
    assert!(core::mem::size_of::<Table>() == 80);
    assert!(core::mem::offset_of!(Table, partitions) == 64);
};
