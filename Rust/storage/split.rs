//! Original ordered logical request splitting, without a new backend limit.
use core::ptr;
#[repr(C)]
pub struct Iterator {
    pub total_sectors: u16,
    pub max_sectors: u16,
    pub sector_size: usize,
    pub sector_offset: u16,
}
#[repr(C)]
pub struct Chunk {
    pub sector_offset: u16,
    pub sectors: u16,
    pub byte_offset: usize,
    pub byte_count: usize,
}
/// # Safety
/// Output is an aligned exclusive private C iterator. Rejected empty input
/// leaves its backing unchanged, as the original optional constructor did.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_split_init(
    total: u16,
    max: u16,
    size: usize,
    out: *mut Iterator,
) -> bool {
    if total == 0 || size == 0 {
        return false;
    }
    unsafe {
        ptr::write(
            out,
            Iterator {
                total_sectors: total,
                max_sectors: if max == 0 { total } else { max },
                sector_size: size,
                sector_offset: 0,
            },
        )
    };
    true
}
/// # Safety
/// Initialized iterator and output are aligned exclusive resident C records
/// under the original caller owner. No borrow survives the synchronous step.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_split_next(
    state: *mut Iterator,
    out: *mut Chunk,
) -> bool {
    let state = unsafe { &mut *state };
    if state.sector_offset >= state.total_sectors {
        return false;
    }
    let remaining = state.total_sectors - state.sector_offset;
    let sectors = remaining.min(state.max_sectors);
    let offset = state.sector_offset;
    state.sector_offset += sectors;
    unsafe {
        ptr::write(
            out,
            Chunk {
                sector_offset: offset,
                sectors,
                byte_offset: usize::from(offset) * state.sector_size,
                byte_count: usize::from(sectors) * state.sector_size,
            },
        )
    };
    true
}
const _: () = {
    assert!(core::mem::size_of::<Iterator>() == 24);
    assert!(core::mem::offset_of!(Iterator, sector_size) == 8);
    assert!(core::mem::size_of::<Chunk>() == 24);
};

/// Successful whole sectors plus a successful short device completion.
pub(crate) fn transferred_bytes(sectors: u64, sector_bytes: u32, additional: u64) -> Option<u64> {
    if sector_bytes == 0 {
        return None;
    }
    sectors.checked_mul(u64::from(sector_bytes))?.checked_add(additional)
}

#[cfg(test)]
mod transferred_tests {
    use super::transferred_bytes;
    #[test]
    fn device_sector_size_short_transfers_and_limits() {
        assert_eq!(transferred_bytes(2, 512, 37), Some(1061));
        assert_eq!(transferred_bytes(2, 4096, 37), Some(8229));
        assert_eq!(transferred_bytes(0, 4096, 0), Some(0));
        assert_eq!(transferred_bytes(1, 0, 0), None);
        assert_eq!(transferred_bytes(u64::MAX, 4096, 0), None);
        assert_eq!(transferred_bytes(1, 512, u64::MAX), None);
    }
}
