//! Original released56/136-byte statistics prefixes. Read only the caller's
//! validated header, stage every field, and write exactly the admitted prefix.
use crate::contract::GfxBufferStats;
use core::ptr;
/// # Safety
/// Non-null aligned caller pointer retains at least the initialized eight-byte
/// header; no full136-byte reference is formed for a legacy56-byte allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_stats_capacity(p: *const GfxBufferStats) -> u32 {
    if p.is_null() || !p.is_aligned() {
        return 0;
    }
    let version = unsafe { ptr::addr_of!((*p).version).read() };
    let size = unsafe { ptr::addr_of!((*p).size).read() };
    if version != 1 || size < 56 {
        0
    } else if size >= 136 {
        136
    } else {
        56
    }
}
/// # Safety
/// Caller owns exactly count writable bytes after capacity validation and a
/// complete initialized readable source record; admission stays with its owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_stats_write(
    out: *mut GfxBufferStats,
    count: u32,
    value: *const GfxBufferStats,
) {
    assert!(matches!(count, 56 | 136));
    let value = unsafe { value.read() };
    let mut bytes = [0u8; 136];
    for (i, v) in [
        value.version,
        count,
        value.objects,
        value.references,
        value.leases,
        value.reserved0,
    ]
    .into_iter()
    .enumerate()
    {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in [
        value.committed_bytes,
        value.retained_bytes,
        value.budget_bytes,
        value.producer_budget_bytes,
        value.system_bytes,
        value.device_bytes,
        value.system_backed_bytes,
        value.device_backed_bytes,
        value.system_pinned_bytes,
        value.device_pinned_bytes,
        value.scanout_pinned_bytes,
        value.device_mapped_bytes,
        value.allocating_bytes,
        value.destroying_bytes,
    ]
    .into_iter()
    .enumerate()
    {
        let start = 24 + i * 8;
        bytes[start..start + 8].copy_from_slice(&v.to_le_bytes());
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast(), count as usize);
    }
}
const _: () = {
    assert!(core::mem::size_of::<GfxBufferStats>() == 136);
    assert!(core::mem::align_of::<GfxBufferStats>() == 8);
    assert!(core::mem::offset_of!(GfxBufferStats, version) == 0);
    assert!(core::mem::offset_of!(GfxBufferStats, size) == 4);
    assert!(core::mem::offset_of!(GfxBufferStats, objects) == 8);
    assert!(core::mem::offset_of!(GfxBufferStats, committed_bytes) == 24);
    assert!(core::mem::offset_of!(GfxBufferStats, system_bytes) == 56);
};
