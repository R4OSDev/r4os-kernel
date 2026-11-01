//! Bounded boot-time SMBIOS reader. Only the validated system UUID survives.
use crate::{
    boot::{info, limine},
    memory::{paging, phys},
};
use core::{
    cell::UnsafeCell,
    slice,
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_TABLE: usize = 1024 * 1024;
const MAX_PHYSICAL: u64 = 1 << 52;

struct Published(UnsafeCell<[u8; 16]>);
// One BSP write precedes the release publication; later readers only copy.
unsafe impl Sync for Published {}
static UUID: Published = Published(UnsafeCell::new([0; 16]));
static VALID: AtomicBool = AtomicBool::new(false);

pub fn system_uuid() -> Option<[u8; 16]> {
    if !VALID.load(Ordering::Acquire) {
        return None;
    }
    Some(unsafe { *UUID.0.get() })
}

pub fn inspect() {
    let response = limine::r4os_rust_limine_response(6).cast::<limine::SmbiosResponse>();
    if response.is_null() {
        return;
    }
    // Limine responses are bootloader-owned. Inspect while the BSP still owns
    // boot startup and copy only the final 16 bytes into permanent storage.
    let response = unsafe { &*response };
    let value = match (response.entry_64 != 0, response.entry_32 != 0) {
        (true, true) => match (entry64(response.entry_64), entry32(response.entry_32)) {
            (Some(a), Some(b)) if a == b => Some(a),
            _ => None,
        },
        (true, false) => entry64(response.entry_64),
        (false, true) => entry32(response.entry_32),
        (false, false) => None,
    };
    if let Some(value) = value {
        unsafe { UUID.0.get().write(value) };
        VALID.store(true, Ordering::Release);
    }
}

fn physical(address: u64) -> Option<u64> {
    if address == 0 {
        return None;
    }
    let snapshot = unsafe { &*info::r4os_rust_boot_info() };
    let value = if snapshot.flags & 4 != 0 {
        address.checked_sub(snapshot.hhdm).unwrap_or(address)
    } else {
        address
    };
    (value < MAX_PHYSICAL).then_some(value)
}

fn mapped(address: u64, length: usize) -> Option<&'static [u8]> {
    let start = physical(address)?;
    if length == 0 || length > MAX_TABLE {
        return None;
    }
    let end = start.checked_add(length as u64)?;
    if end > MAX_PHYSICAL {
        return None;
    }
    let mut page = start & !4095;
    let last = end.checked_add(4095)? & !4095;
    while page < last {
        let virtual_address = phys::r4os_rust_phys_to_virt(page);
        let mut mapped_physical = 0;
        if !unsafe {
            paging::r4os_rust_paging_read(virtual_address, 3, false, &mut mapped_physical)
        } && !paging::r4os_rust_paging_map(virtual_address, page, 1 << 63, false)
        {
            return None;
        }
        page += 4096;
    }
    Some(unsafe { slice::from_raw_parts(phys::r4os_rust_phys_to_virt(start) as *const u8, length) })
}

fn checksum(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn entry64(address: u64) -> Option<[u8; 16]> {
    let header = mapped(address, 24)?;
    if &header[..5] != b"_SM3_" || header[7] < 3 {
        return None;
    }
    let length = header[6] as usize;
    if !(24..=32).contains(&length) || !checksum(mapped(address, length)?) {
        return None;
    }
    let table_length = u32::from_le_bytes(header[12..16].try_into().ok()?) as usize;
    let table_address = u64::from_le_bytes(header[16..24].try_into().ok()?);
    parse_table(mapped(table_address, table_length)?)
}

fn entry32(address: u64) -> Option<[u8; 16]> {
    let header = mapped(address, 31)?;
    if &header[..4] != b"_SM_"
        || &header[16..21] != b"_DMI_"
        || header[6] < 2
        || (header[6] == 2 && header[7] < 6)
    {
        return None;
    }
    let length = header[5] as usize;
    if !(31..=32).contains(&length)
        || !checksum(mapped(address, length)?)
        || !checksum(&header[16..31])
    {
        return None;
    }
    let table_length = u16::from_le_bytes(header[22..24].try_into().ok()?) as usize;
    let table_address = u32::from_le_bytes(header[24..28].try_into().ok()?) as u64;
    parse_table(mapped(table_address, table_length)?)
}

fn parse_table(table: &[u8]) -> Option<[u8; 16]> {
    let mut at: usize = 0;
    let mut found = None;
    while at.checked_add(4)? <= table.len() {
        let kind = table[at];
        let length = table[at + 1] as usize;
        if length < 4 || at.checked_add(length)? > table.len() {
            return None;
        }
        if kind == 1 {
            if length < 24 || found.is_some() {
                return None;
            }
            let mut uuid: [u8; 16] = table[at + 8..at + 24].try_into().ok()?;
            if uuid.iter().all(|byte| *byte == uuid[0]) {
                return None;
            }
            uuid[..4].reverse();
            uuid[4..6].reverse();
            uuid[6..8].reverse();
            found = Some(uuid);
        }
        let mut end = at + length;
        while end + 1 < table.len() && (table[end] != 0 || table[end + 1] != 0) {
            end += 1;
        }
        if end + 1 >= table.len() {
            return None;
        }
        if kind == 127 {
            return found;
        }
        at = end + 2;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_table;

    #[test]
    fn normalizes_only_one_bounded_type_one_uuid() {
        let mut table = [0u8; 32];
        table[0] = 1;
        table[1] = 24;
        table[8..24].copy_from_slice(&[
            0x67, 0x45, 0x3e, 0x12, 0x9b, 0xe8, 0xd3, 0x12, 0xa4, 0x56, 0x42, 0x66, 0x14, 0x17,
            0x40, 0x00,
        ]);
        table[26] = 127;
        table[27] = 4;
        assert_eq!(
            parse_table(&table),
            Some([
                0x12, 0x3e, 0x45, 0x67, 0xe8, 0x9b, 0x12, 0xd3, 0xa4, 0x56, 0x42, 0x66, 0x14, 0x17,
                0x40, 0x00,
            ])
        );
        table[8..24].fill(0);
        assert_eq!(parse_table(&table), None);
        table[8..24].fill(0xff);
        assert_eq!(parse_table(&table), None);
        table[8..24].fill(0x11);
        assert_eq!(parse_table(&table), None);
        table[1] = 23;
        assert_eq!(parse_table(&table), None);
    }
}
