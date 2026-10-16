//! Existing bounded boot option view: borrowed immutable configuration input,
//! native reusable NUL output with the exact original64-byte payload limit.
use core::{cell::UnsafeCell, ptr, slice};
#[repr(C)]
struct Span {
    pointer: *const u8,
    len: usize,
}
#[repr(C)]
struct OptionView {
    driver: Span,
    key: Span,
    value: Span,
}

struct Buffer(UnsafeCell<[u8; 65]>);
// SAFETY: the original R4D synchronous option lookup serializes this reusable
// result; consumers retain no permission to use it after another query.
unsafe impl Sync for Buffer {}
static VALUE: Buffer = Buffer(UnsafeCell::new([0; 65]));
static EMPTY: [u8; 1] = [0];
unsafe fn matches(z: *const u8, span: Span) -> bool {
    let supplied = unsafe { slice::from_raw_parts(z, span.len) };
    let expected = unsafe { slice::from_raw_parts(span.pointer, span.len) };
    supplied.eq_ignore_ascii_case(expected) && unsafe { *z.add(span.len) == 0 }
}
/// # Safety
/// Zero-terminated names are readable under the original serialized driver
/// admission; the shared result buffer remains borrowed until the next lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_option(driver: *const u8, key: *const u8) -> *const u8 {
    let count = unsafe { driver_option_count() };
    for i in 0..count {
        let mut option = core::mem::MaybeUninit::uninit();
        unsafe { driver_option(i, option.as_mut_ptr()) };
        let option = unsafe { option.assume_init() };
        if !unsafe { matches(driver, option.driver) } || !unsafe { matches(key, option.key) } {
            continue;
        }
        let destination = VALUE.0.get().cast::<u8>();
        unsafe {
            ptr::write_bytes(destination, 0, 65);
            let length = option.value.len.min(64);
            if length != 0 {
                ptr::copy_nonoverlapping(option.value.pointer, destination, length)
            }
        }
        return destination;
    }
    EMPTY.as_ptr()
}

unsafe fn driver_option_count() -> usize {
    unsafe { (*crate::boot::config::get()).option_count }
}

unsafe fn driver_option(index: usize, out: *mut OptionView) {
    unsafe {
        let config = crate::boot::config::get();
        let opt = core::ptr::addr_of!((*config).options)
            .cast::<crate::boot::config::OptionRecord>()
            .add(index);
        out.write(OptionView {
            driver: Span {
                pointer: core::ptr::addr_of!((*opt).driver).cast(),
                len: (*opt).driver_len,
            },
            key: Span {
                pointer: core::ptr::addr_of!((*opt).key).cast(),
                len: (*opt).key_len,
            },
            value: Span {
                pointer: core::ptr::addr_of!((*opt).value).cast(),
                len: (*opt).value_len,
            },
        });
    }
}
