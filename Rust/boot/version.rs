//! The existing 44-byte kernel image metadata, independent of the compiler.
use crate::build_config::config;

#[repr(C)]
pub struct Metadata {
    magic: [u8; 8],
    format_version: u32,
    size: u32,
    major: u32,
    minor: u32,
    patch: u32,
    version_text: [u8; 16],
}
const fn text() -> [u8; 16] {
    let mut value = [0; 16];
    let mut i = 0;
    while i < config::KERNEL_VERSION_TEXT.len() {
        value[i] = config::KERNEL_VERSION_TEXT[i];
        i += 1;
    }
    value
}
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".r4os.kernel.meta")]
pub static r4os_kernel_metadata: Metadata = Metadata {
    magic: *b"R4OSKRN1",
    format_version: 1,
    size: 44,
    major: config::KERNEL_MAJOR,
    minor: config::KERNEL_MINOR,
    patch: config::KERNEL_PATCH,
    version_text: text(),
};
pub(super) fn keep() {
    unsafe {
        core::ptr::addr_of!(r4os_kernel_metadata)
            .cast::<u8>()
            .read_volatile();
    }
}
const _: () = {
    assert!(core::mem::size_of::<Metadata>() == 44);
    assert!(core::mem::align_of::<Metadata>() == 4);
};
