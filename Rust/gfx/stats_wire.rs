//! Complete current BO statistics. No projection to an old statistics prefix.
use crate::contract::GfxBufferStats;
use core::mem::size_of;
/// # Safety
/// Caller retains an initialized resident eight-byte header; no complete
/// record is accessed until its current writable capacity has been admitted.
pub unsafe fn capacity(p: *const GfxBufferStats) -> u32 {
    if p.is_null() || !p.is_aligned() {
        return 0;
    }
    let version = unsafe { p.cast::<u32>().read() };
    let bytes = unsafe { p.cast::<u32>().add(1).read() };
    if version == 1 && bytes >= size_of::<GfxBufferStats>() as u32 {
        size_of::<GfxBufferStats>() as u32
    } else {
        0
    }
}
/// # Safety
/// Previously admitted complete writable output and disjoint complete source.
pub unsafe fn write(out: *mut GfxBufferStats, count: u32, value: *const GfxBufferStats) {
    assert_eq!(count, size_of::<GfxBufferStats>() as u32);
    let mut value = unsafe { value.read() };
    value.size = count;
    unsafe { out.write(value) };
}
const _: () = {
    assert!(size_of::<GfxBufferStats>() == 136);
};

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;
    #[test]
    fn current_statistics_refuse_short_headers_and_preserve_future_capacity() {
        let mut memory = [0xA5A5_A5A5_A5A5_A5A5u64; 19];
        let out = memory.as_mut_ptr().cast::<GfxBufferStats>();
        for bytes in 0..136 {
            unsafe {
                out.cast::<u32>().write(1);
                out.cast::<u32>().add(1).write(bytes)
            };
            assert_eq!(unsafe { capacity(out) }, 0);
            let untouched = unsafe { core::slice::from_raw_parts(out.cast::<u8>().add(8), 144) };
            assert!(untouched.iter().all(|byte| *byte == 0xA5));
        }
        for bytes in [136, 144] {
            unsafe {
                out.cast::<u32>().write(1);
                out.cast::<u32>().add(1).write(bytes)
            };
            let count = unsafe { capacity(out) };
            assert_eq!(count, 136);
            let value = GfxBufferStats {
                committed_bytes: 0x100000003,
                system_bytes: 17,
                ..Default::default()
            };
            unsafe { write(out, count, &value) };
            assert_eq!(unsafe { (*out).system_bytes }, 17);
            assert_eq!(unsafe { (*out).committed_bytes }, 0x100000003);
            assert_eq!(memory[17], 0xA5A5_A5A5_A5A5_A5A5);
            assert_eq!(memory[18], 0xA5A5_A5A5_A5A5_A5A5);
        }
        assert_eq!(unsafe { capacity(ptr::null()) }, 0);
        assert_eq!(unsafe { capacity(out.cast::<u8>().add(1).cast()) }, 0);
    }
}
