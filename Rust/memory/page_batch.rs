//! Bounded natural extents and fresh, value-only leaf translation.
pub const MAX_EXTENT_PAGES: u64 = 64;
pub const MAX_TRANSLATION_BYTES: u64 = 256 * 1024 * 1024;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Translation {
    pub virtual_base: u64,
    pub bytes: u64,
    pub consumed: u64,
    pub physical: u64,
    pub physical_present: bool,
}
impl Translation {
    pub fn new(virtual_base: u64, bytes: u64) -> Option<Self> {
        if bytes == 0
            || bytes > MAX_TRANSLATION_BYTES
            || virtual_base.checked_add(bytes - 1).is_none()
        {
            return None;
        }
        Some(Self {
            virtual_base,
            bytes,
            consumed: 0,
            physical: 0,
            physical_present: false,
        })
    }
    pub fn accept(&mut self, address: u64, remaining: u64) -> bool {
        if self.consumed >= self.bytes || remaining == 0 {
            return false;
        }
        let count = (self.bytes - self.consumed).min(remaining);
        if address.checked_add(count - 1).is_none() {
            return false;
        }
        if self.physical_present {
            if self.physical.checked_add(self.consumed) != Some(address) {
                return false;
            }
        } else {
            self.physical = address;
            self.physical_present = true;
        }
        self.consumed += count;
        true
    }
    pub fn result(&self) -> Option<u64> {
        if self.consumed == self.bytes && self.physical_present {
            Some(self.physical)
        } else {
            None
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_page_batch_bound(remaining: u64) -> u64 {
    remaining.min(MAX_EXTENT_PAGES)
}
/// # Safety
/// frames contains count readable acquired addresses, without ownership transfer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_page_batch_prefix(
    frames: *const u64,
    count: usize,
    page: u64,
) -> usize {
    if count == 0 || page == 0 {
        return 0;
    }
    let frames = unsafe { core::slice::from_raw_parts(frames, count) };
    let mut prefix = 1;
    while prefix < count {
        if frames[prefix] != frames[0].wrapping_add((prefix as u64).wrapping_mul(page)) {
            break;
        }
        prefix += 1;
    }
    prefix
}
/// # Safety
/// output is live exclusive C-layout storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_translation_init(
    virtual_base: u64,
    bytes: u64,
    output: *mut Translation,
) -> bool {
    if let Some(value) = Translation::new(virtual_base, bytes) {
        unsafe {
            output.write(value);
        }
        true
    } else {
        false
    }
}
/// # Safety
/// translation is an exclusive initialized value snapshot, without a retained page-table pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_translation_accept(
    translation: *mut Translation,
    address: u64,
    remaining: u64,
) -> bool {
    unsafe { (&mut *translation).accept(address, remaining) }
}
/// # Safety
/// translation/output are live C records; success writes the completed physical base.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_translation_result(
    translation: *const Translation,
    output: *mut u64,
) -> bool {
    if let Some(value) = unsafe { (&*translation).result() } {
        unsafe {
            output.write(value);
        }
        true
    } else {
        false
    }
}
const _: () = assert!(core::mem::size_of::<Translation>() == 40);
