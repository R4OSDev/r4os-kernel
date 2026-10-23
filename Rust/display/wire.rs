//! Admission of complete current graphics records. Read only the initialized
//! header until its full current capacity is proven; future tails stay untouched.
use crate::contract as a;
use core::{mem::size_of, ptr};
pub trait QueueRecord: Copy + Default {}
impl QueueRecord for a::GfxBackendRegistration {}
impl QueueRecord for a::GfxBackendInfo {}
impl QueueRecord for a::GfxSubmission {}
impl QueueRecord for a::GfxDriverJob {}
impl QueueRecord for a::GfxOutputColorState {}

/// # Safety
/// The pointer retains a readable initialized eight-byte header. No complete
/// record or missing field is accessed before this capacity check succeeds.
pub unsafe fn capacity<T: QueueRecord>(value: *const T) -> Option<u32> {
    if value.is_null() || !value.is_aligned() {
        return None;
    }
    let version = unsafe { value.cast::<u32>().read() };
    let bytes = unsafe { value.cast::<u32>().add(1).read() };
    (version == 1 && bytes >= size_of::<T>() as u32).then_some(size_of::<T>() as u32)
}
/// # Safety
/// Caller retains the initialized header and its advertised complete current
/// record. Larger capacity does not grant a different record interpretation.
pub unsafe fn read<T: QueueRecord>(input: *const T) -> Option<T> {
    let bytes = unsafe { capacity(input) }?;
    let mut value = unsafe { input.read() };
    unsafe { ptr::from_mut(&mut value).cast::<u32>().add(1).write(bytes) };
    Some(value)
}
/// # Safety
/// The previously admitted complete writable record is disjoint from the
/// retained snapshot. No bytes beyond the current record are written.
pub unsafe fn write<T: QueueRecord>(out: *mut T, bytes: u32, mut value: T) {
    assert_eq!(bytes, size_of::<T>() as u32);
    unsafe {
        ptr::from_mut(&mut value).cast::<u32>().add(1).write(bytes);
        out.write(value);
    }
}
/// # Safety
/// Same initialized-header/current-capacity contract as capacity().
pub unsafe fn color_capacity(value: *const a::GfxOutputColorState) -> Option<u32> {
    unsafe { capacity(value) }
}
/// # Safety
/// Same initialized-header/current-capacity contract as read().
pub unsafe fn color_read(value: *const a::GfxOutputColorState) -> Option<a::GfxOutputColorState> {
    unsafe { read(value) }
}
const _: () = {
    assert!(size_of::<a::GfxBackendRegistration>() == 48);
    assert!(size_of::<a::GfxBackendInfo>() == 152);
    assert!(size_of::<a::GfxSubmission>() == 512);
    assert!(size_of::<a::GfxDriverJob>() == 296);
    assert!(size_of::<a::GfxOutputColorState>() == 192);
};

#[cfg(test)]
mod tests {
    use super::*;
    fn record<T: QueueRecord>() {
        let size = size_of::<T>();
        assert!(size <= 512 && core::mem::align_of::<T>() == 8);
        let mut allocation = [0xA5A5_A5A5_A5A5_A5A5u64; 66];
        let value = allocation.as_mut_ptr().cast::<T>();
        for bytes in 0..size as u32 {
            unsafe {
                value.cast::<u32>().write(1);
                value.cast::<u32>().add(1).write(bytes)
            };
            assert_eq!(unsafe { capacity(value) }, None);
            assert!(unsafe { read(value) }.is_none());
            let tail = unsafe {
                core::slice::from_raw_parts(value.cast::<u8>().add(8), allocation.len() * 8 - 8)
            };
            assert!(tail.iter().all(|byte| *byte == 0xA5));
        }
        for bytes in [size as u32, size as u32 + 8] {
            unsafe {
                value.cast::<u32>().write(0);
                value.cast::<u32>().add(1).write(bytes)
            };
            assert_eq!(unsafe { capacity(value) }, None);
            unsafe { value.cast::<u32>().write(1) };
            let admitted = unsafe { capacity(value) }.unwrap();
            assert_eq!(admitted, size as u32);
            unsafe { write(value, admitted, T::default()) };
            assert!(unsafe { read(value) }.is_some());
            assert_eq!(unsafe { value.cast::<u32>().add(1).read() }, size as u32);
            let canary = unsafe {
                core::slice::from_raw_parts(
                    value.cast::<u8>().add(size),
                    allocation.len() * 8 - size,
                )
            };
            assert!(canary.iter().all(|byte| *byte == 0xA5));
        }
        assert_eq!(unsafe { capacity::<T>(ptr::null()) }, None);
        assert_eq!(
            unsafe { capacity(value.cast::<u8>().add(1).cast::<T>()) },
            None
        );
    }
    #[test]
    fn current_records_reject_all_short_capacities_and_keep_future_tail_canaries() {
        record::<a::GfxBackendRegistration>();
        record::<a::GfxBackendInfo>();
        record::<a::GfxSubmission>();
        record::<a::GfxDriverJob>();
        record::<a::GfxOutputColorState>();
    }
}
