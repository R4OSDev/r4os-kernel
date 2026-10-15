//! Original sector-exact write-batch ownership, under the cache caller's owner.

const SECTOR_COUNT: usize = 8;

/// # Safety
/// Owners points to eight initialized u64 records, protected by the existing
/// cache owner throughout the synchronous read. No I/O or wait occurs here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_batch_mask(
    dirty_mask: u8,
    owners: *const u64,
    owner: u64,
) -> u8 {
    let mut mask = 0;
    for sector in 0..SECTOR_COUNT {
        let bit = 1u8 << sector;
        if dirty_mask & bit != 0 && unsafe { owners.add(sector).read() } == owner {
            mask |= bit;
        }
    }
    mask
}

/// # Safety
/// Both input arrays contain eight initialized u64 records under the exact
/// cache owner. Output is aligned writable storage and is written only when
/// an owned dirty sector exists; no borrow survives this synchronous query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_batch_oldest(
    dirty_mask: u8,
    owners: *const u64,
    sequences: *const u64,
    owner: u64,
    output: *mut u64,
) -> bool {
    let mut oldest = None;
    for sector in 0..SECTOR_COUNT {
        let bit = 1u8 << sector;
        if dirty_mask & bit == 0 || unsafe { owners.add(sector).read() } != owner {
            continue;
        }
        let sequence = unsafe { sequences.add(sector).read() };
        if oldest.is_none_or(|oldest| sequence < oldest) {
            oldest = Some(sequence);
        }
    }
    if let Some(oldest) = oldest {
        unsafe { output.write(oldest) };
        true
    } else {
        false
    }
}

/// # Safety
/// Owners and sequences each expose eight aligned writable records under the
/// caller's cache owner. Raw scalar writes preserve the original ordered
/// clearing even if the arrays alias; there is no second batch state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_batch_clear(owners: *mut u64, sequences: *mut u64, bits: u8) {
    for sector in 0..SECTOR_COUNT {
        if bits & (1u8 << sector) == 0 {
            continue;
        }
        unsafe {
            owners.add(sector).write(0);
            sequences.add(sector).write(0);
        }
    }
}
