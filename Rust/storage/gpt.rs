//! Exact original GPT geometry, CRC and partition decoding, without I/O or allocation.
use core::slice;
pub const SECTOR: usize = 512;
pub const ESP: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
pub const BASIC: [u8; 16] = [
    0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7,
];
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    pub disk_guid: [u8; 16],
    pub current_lba: u64,
    pub backup_lba: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub entries_lba: u64,
    pub entry_count: u32,
    pub entry_size: u32,
    pub entries_crc32: u32,
}
impl Header {
    pub fn entry_bytes(self) -> u64 {
        u64::from(self.entry_count) * u64::from(self.entry_size)
    }
    pub fn entry_sectors(self) -> u64 {
        self.entry_bytes().div_ceil(SECTOR as u64)
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Partition {
    pub type_guid: [u8; 16],
    pub unique_guid: [u8; 16],
    pub name_utf16: [u16; 36],
    pub partition_type: u8,
    pub first_lba: u64,
    pub last_lba: u64,
    pub attributes: u64,
}
impl Partition {
    pub fn sector_count(self) -> u64 {
        self.last_lba - self.first_lba + 1
    }
    pub fn boot_candidate(self) -> bool {
        self.partition_type == 0 || self.attributes & 4 != 0
    }
}
#[derive(Clone, Copy)]
#[repr(i32)]
pub enum Error {
    ShortSector = 1,
    BadSignature = 2,
    UnsupportedRevision = 3,
    BadHeaderSize = 4,
    NonzeroReserved = 5,
    BadHeaderCrc = 6,
    WrongHeaderLba = 7,
    BadBackupLba = 8,
    BadUsableRange = 9,
    UnsupportedEntryLayout = 10,
    EntryArrayOutOfRange = 11,
    ShortEntry = 12,
    BadPartitionRange = 13,
}
pub fn le32(p: &[u8]) -> u32 {
    u32::from_le_bytes(p[..4].try_into().unwrap())
}
pub fn le64(p: &[u8]) -> u64 {
    u64::from_le_bytes(p[..8].try_into().unwrap())
}
pub struct Crc32 {
    pub state: u32,
}
impl Default for Crc32 {
    fn default() -> Self {
        Self { state: u32::MAX }
    }
}
impl Crc32 {
    pub fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u32::from(*byte);
            for _ in 0..8 {
                self.state = (self.state >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(self.state & 1));
            }
        }
    }
    pub fn finish(&self) -> u32 {
        !self.state
    }
}
pub fn parse_header(sector: &[u8], header_lba: u64, device_sectors: u64) -> Result<Header, Error> {
    if sector.len() < SECTOR {
        return Err(Error::ShortSector);
    }
    if &sector[..8] != b"EFI PART" {
        return Err(Error::BadSignature);
    }
    if le32(&sector[8..12]) != 0x0001_0000 {
        return Err(Error::UnsupportedRevision);
    }
    let header_size = le32(&sector[12..16]) as usize;
    if !(92..=SECTOR).contains(&header_size) {
        return Err(Error::BadHeaderSize);
    }
    if le32(&sector[20..24]) != 0 {
        return Err(Error::NonzeroReserved);
    }
    let mut crc = Crc32::default();
    crc.update(&sector[..16]);
    crc.update(&[0; 4]);
    crc.update(&sector[20..header_size]);
    if crc.finish() != le32(&sector[16..20]) {
        return Err(Error::BadHeaderCrc);
    }
    let current_lba = le64(&sector[24..32]);
    let backup_lba = le64(&sector[32..40]);
    let first_usable_lba = le64(&sector[40..48]);
    let last_usable_lba = le64(&sector[48..56]);
    let entries_lba = le64(&sector[72..80]);
    let entry_count = le32(&sector[80..84]);
    let entry_size = le32(&sector[84..88]);
    if device_sectors < 3 || current_lba != header_lba {
        return Err(Error::WrongHeaderLba);
    }
    if backup_lba >= device_sectors || backup_lba == current_lba {
        return Err(Error::BadBackupLba);
    }
    if first_usable_lba > last_usable_lba || last_usable_lba >= device_sectors {
        return Err(Error::BadUsableRange);
    }
    if entry_count == 0
        || entry_count > 4096
        || !(128..=512).contains(&entry_size)
        || !entry_size.is_multiple_of(128)
    {
        return Err(Error::UnsupportedEntryLayout);
    }
    let header = Header {
        disk_guid: sector[56..72].try_into().unwrap(),
        current_lba,
        backup_lba,
        first_usable_lba,
        last_usable_lba,
        entries_lba,
        entry_count,
        entry_size,
        entries_crc32: le32(&sector[88..92]),
    };
    let entry_sectors = header.entry_sectors();
    if entry_sectors == 0
        || entries_lba >= device_sectors
        || entry_sectors > device_sectors - entries_lba
    {
        return Err(Error::EntryArrayOutOfRange);
    }
    let entries_end = entries_lba + entry_sectors;
    if header_lba == 1 {
        if backup_lba != device_sectors - 1
            || entries_lba <= header_lba
            || entries_end > first_usable_lba
        {
            return Err(Error::EntryArrayOutOfRange);
        }
    } else if header_lba == device_sectors - 1 {
        if backup_lba != 1 || entries_lba <= last_usable_lba || entries_end > header_lba {
            return Err(Error::EntryArrayOutOfRange);
        }
    } else {
        return Err(Error::WrongHeaderLba);
    }
    Ok(header)
}
pub fn parse_partition(raw: &[u8], header: Header) -> Result<Option<Partition>, Error> {
    if raw.len() < header.entry_size as usize || raw.len() < 128 {
        return Err(Error::ShortEntry);
    }
    let type_guid: [u8; 16] = raw[..16].try_into().unwrap();
    if type_guid == [0; 16] {
        return Ok(None);
    }
    let first_lba = le64(&raw[32..40]);
    let last_lba = le64(&raw[40..48]);
    if first_lba < header.first_usable_lba
        || first_lba > last_lba
        || last_lba > header.last_usable_lba
    {
        return Err(Error::BadPartitionRange);
    }
    let mut name_utf16 = [0; 36];
    for (i, unit) in name_utf16.iter_mut().enumerate() {
        *unit = u16::from_le_bytes(raw[56 + i * 2..58 + i * 2].try_into().unwrap());
    }
    Ok(Some(Partition {
        type_guid,
        unique_guid: raw[16..32].try_into().unwrap(),
        name_utf16,
        partition_type: if type_guid == ESP {
            0
        } else if type_guid == BASIC {
            1
        } else {
            2
        },
        first_lba,
        last_lba,
        attributes: le64(&raw[48..56]),
    }))
}
unsafe fn bytes<'a>(data: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, len) }
    }
}
/// # Safety
/// Input contains len resident readable bytes. Output is exclusive aligned
/// Header storage, disjoint from the input and written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gpt_header(
    data: *const u8,
    len: usize,
    lba: u64,
    sectors: u64,
    out: *mut Header,
) -> i32 {
    match parse_header(unsafe { bytes(data, len) }, lba, sectors) {
        Ok(h) => {
            unsafe { out.write(h) };
            0
        }
        Err(e) => e as i32,
    }
}
/// # Safety
/// Header is an initialized readable C value, input owns len readable bytes;
/// partition output is exclusive, aligned and disjoint, written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gpt_partition(
    data: *const u8,
    len: usize,
    header: *const Header,
    out: *mut Partition,
) -> i32 {
    match parse_partition(unsafe { bytes(data, len) }, unsafe { header.read() }) {
        Ok(Some(p)) => {
            unsafe { out.write(p) };
            0
        }
        Ok(None) => -1,
        Err(e) => e as i32,
    }
}
/// # Safety
/// Initialized caller-owned CRC word is exclusive for this call; input bytes
/// are resident and disjoint. No buffer or borrow is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gpt_crc(state: *mut u32, data: *const u8, len: usize) {
    let mut crc = Crc32 {
        state: unsafe { state.read() },
    };
    crc.update(unsafe { bytes(data, len) });
    unsafe { state.write(crc.state) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gpt_crc_finish(state: u32) -> u32 {
    !state
}
/// # Safety
/// Readable initialized C Header; no reference escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gpt_header_value(h: *const Header, sectors: bool) -> u64 {
    let h = unsafe { h.read() };
    if sectors {
        h.entry_sectors()
    } else {
        h.entry_bytes()
    }
}
/// # Safety
/// Readable initialized C Partition; kind is one of the original three values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gpt_partition_value(p: *const Partition, boot: bool) -> u64 {
    let p = unsafe { p.read() };
    if boot {
        u64::from(p.boot_candidate())
    } else {
        p.sector_count()
    }
}
#[repr(C)]
pub struct Label {
    pub data: *const u8,
    pub len: usize,
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gpt_label(partition_type: bool, code: u8) -> Label {
    let text: &[u8] = if partition_type {
        match code {
            0 => b"EFI-system",
            1 => b"Microsoft-basic-data",
            2 => b"unsupported-guid",
            _ => unreachable!(),
        }
    } else {
        match code {
            1 => b"short-sector",
            2 => b"bad-signature",
            3 => b"unsupported-revision",
            4 => b"bad-header-size",
            5 => b"reserved-not-zero",
            6 => b"header-crc",
            7 => b"header-lba",
            8 => b"backup-lba",
            9 => b"usable-range",
            10 => b"entry-layout",
            11 => b"entry-array-range",
            12 => b"short-entry",
            13 => b"partition-range",
            _ => unreachable!(),
        }
    };
    Label {
        data: text.as_ptr(),
        len: text.len(),
    }
}
const _: () = {
    assert!(core::mem::size_of::<Header>() == 72);
    assert!(core::mem::offset_of!(Header, entries_crc32) == 64);
    assert!(core::mem::size_of::<Partition>() == 136);
    assert!(core::mem::offset_of!(Partition, first_lba) == 112);
};
