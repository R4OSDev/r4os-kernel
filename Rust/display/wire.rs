//! Complete extensible graphics payload prefixes. Only the admitted old
//! header/prefix is touched; missing field groups keep their exact defaults.
use crate::contract as a;
use core::{ffi::c_void, mem::size_of, ptr};
pub trait QueueRecord: Copy + Default {
    const PREFIXES: &'static [u32];
}
impl QueueRecord for a::GfxBackendRegistration {
    const PREFIXES: &'static [u32] = &[32, 40, 48];
}
impl QueueRecord for a::GfxBackendInfo {
    const PREFIXES: &'static [u32] = &[136, 144, 152];
}
impl QueueRecord for a::GfxSubmission {
    const PREFIXES: &'static [u32] = &[408, 432, 512];
}
impl QueueRecord for a::GfxDriverJob {
    const PREFIXES: &'static [u32] = &[112, 136, 224, 272, 296];
}
impl QueueRecord for a::GfxOutputColorState {
    const PREFIXES: &'static [u32] = &[128, 192];
}
pub fn prefix<T: QueueRecord>(available: u32) -> Option<u32> {
    assert_eq!(T::PREFIXES.last().copied(), Some(size_of::<T>() as u32));
    let mut size = None;
    for &n in T::PREFIXES {
        if n <= available {
            size = Some(n);
        }
    }
    size
}
/// # Safety
/// The pointer retains a readable original eight-byte contract header, with
/// the alignment promised by its original ABI. No complete T is read here.
pub unsafe fn capacity<T: QueueRecord>(value: *const T) -> Option<u32> {
    if value.is_null() || unsafe { value.cast::<u32>().read() } != 1 {
        return None;
    }
    prefix::<T>(unsafe { value.cast::<u32>().add(1).read() })
}
/// # Safety
/// Exact readable retained header and its declared complete released prefix.
pub unsafe fn read<T: QueueRecord>(input: *const T) -> Option<T> {
    let bytes = unsafe { capacity(input) }?;
    let mut value = T::default();
    unsafe {
        ptr::copy_nonoverlapping(
            input.cast::<u8>(),
            ptr::from_mut(&mut value).cast(),
            bytes as usize,
        );
        ptr::from_mut(&mut value).cast::<u32>().add(1).write(bytes);
    }
    Some(value)
}
/// # Safety
/// Exact writable previously admitted prefix; supplied complete snapshot is
/// retained and disjoint from that output. No partial field group is written.
pub unsafe fn write<T: QueueRecord>(out: *mut T, bytes: u32, mut value: T) {
    assert_eq!(prefix::<T>(bytes), Some(bytes));
    unsafe {
        ptr::from_mut(&mut value).cast::<u32>().add(1).write(bytes);
        ptr::copy_nonoverlapping(
            ptr::from_ref(&value).cast::<u8>(),
            out.cast(),
            bytes as usize,
        );
    }
}
/// # Safety
/// Same released-color header/prefix as capacity(), including legacy 128 bytes.
pub unsafe fn color_capacity(value: *const a::GfxOutputColorState) -> Option<u32> {
    if value.is_null() || !value.is_aligned() {
        None
    } else {
        unsafe { capacity(value) }
    }
}
/// # Safety
/// Exact retained color prefix. The new owner gets a complete representation.
pub unsafe fn color_read(value: *const a::GfxOutputColorState) -> Option<a::GfxOutputColorState> {
    let bytes = unsafe { color_capacity(value) }?;
    let mut snapshot = a::GfxOutputColorState::default();
    unsafe {
        ptr::copy_nonoverlapping(
            value.cast::<u8>(),
            ptr::from_mut(&mut snapshot).cast(),
            bytes as usize,
        )
    };
    snapshot.size = size_of::<a::GfxOutputColorState>() as u32;
    Some(snapshot)
}
unsafe fn dispatch<T: QueueRecord>(
    op: u32,
    input: *const c_void,
    out: *mut c_void,
    bytes: u32,
    count: *mut u32,
) -> bool {
    match op {
        0 => match prefix::<T>(bytes) {
            Some(value) => {
                unsafe { count.write(value) };
                true
            }
            None => false,
        },
        1 => match unsafe { read(input.cast::<T>()) } {
            Some(value) => {
                unsafe { out.cast::<T>().write(value) };
                true
            }
            None => false,
        },
        2 => match unsafe { capacity(input.cast::<T>()) } {
            Some(value) => {
                unsafe { count.write(value) };
                true
            }
            None => false,
        },
        3 => {
            unsafe { write(out.cast::<T>(), bytes, input.cast::<T>().read()) };
            true
        }
        _ => panic!("graphics wire operation"),
    }
}
/// # Safety
/// Kind selects its exact unchanged Contract record. Headers, released prefix,
/// full snapshots, output and scalar capacity obey that operation's lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_wire(
    op: u32,
    kind: u32,
    input: *const c_void,
    out: *mut c_void,
    bytes: u32,
    count: *mut u32,
) -> bool {
    match kind {
        0 => unsafe { dispatch::<a::GfxBackendRegistration>(op, input, out, bytes, count) },
        1 => unsafe { dispatch::<a::GfxBackendInfo>(op, input, out, bytes, count) },
        2 => unsafe { dispatch::<a::GfxSubmission>(op, input, out, bytes, count) },
        3 => unsafe { dispatch::<a::GfxDriverJob>(op, input, out, bytes, count) },
        4 => match op {
            0 => unsafe { dispatch::<a::GfxOutputColorState>(op, input, out, bytes, count) },
            1 => match unsafe { color_read(input.cast()) } {
                Some(value) => {
                    unsafe { out.cast::<a::GfxOutputColorState>().write(value) };
                    true
                }
                None => false,
            },
            2 => match unsafe { color_capacity(input.cast()) } {
                Some(value) => {
                    unsafe { count.write(value) };
                    true
                }
                None => false,
            },
            3 => {
                unsafe {
                    write(
                        out.cast::<a::GfxOutputColorState>(),
                        bytes,
                        input.cast::<a::GfxOutputColorState>().read(),
                    )
                };
                true
            }
            _ => panic!("color wire operation"),
        },
        _ => panic!("graphics wire record"),
    }
}
const _: () = {
    assert!(
        size_of::<a::GfxBackendRegistration>() == 48
            && size_of::<a::GfxBackendInfo>() == 152
            && size_of::<a::GfxSubmission>() == 512
            && size_of::<a::GfxDriverJob>() == 296
            && size_of::<a::GfxOutputColorState>() == 192
    );
    assert!(core::mem::offset_of!(a::GfxOutputColorState, link_kind) == 128);
};
