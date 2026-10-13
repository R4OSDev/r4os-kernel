use core::fmt::Write;

pub mod config {
    include!(concat!(env!("OUT_DIR"), "/kernel_config.rs"));
}

#[repr(C)]
pub struct BuildInfo {
    magic: u32,
    size: u32,
    flags: u64,
    smp_fail_ap_index: u32,
    kernel_major: u32,
    kernel_minor: u32,
    kernel_patch: u32,
}

const _: () = {
    assert!(core::mem::size_of::<BuildInfo>() == 32);
    assert!(core::mem::align_of::<BuildInfo>() == 8);
    assert!(core::mem::offset_of!(BuildInfo, flags) == 8);
    assert!(core::mem::offset_of!(BuildInfo, smp_fail_ap_index) == 16);
};

#[unsafe(no_mangle)]
/// # Safety
/// `output` must be a writable, aligned caller-owned BuildInfo for this call.
pub unsafe extern "C" fn r4os_rust_build_info(output: *mut BuildInfo) {
    let value = BuildInfo {
        magic: 0x5234_4231,
        size: core::mem::size_of::<BuildInfo>() as u32,
        flags: config::BUILD_FLAGS,
        smp_fail_ap_index: config::SMP_FAIL_AP_INDEX,
        kernel_major: config::KERNEL_MAJOR,
        kernel_minor: config::KERNEL_MINOR,
        kernel_patch: config::KERNEL_PATCH,
    };
    // SAFETY: the private Zig bridge retains exclusive ownership of the output.
    unsafe { output.write(value) };
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_probe() {
    if config::RUST_PANIC_TEST {
        panic!("Rust panic crash test");
    }
    if config::RUST_OVERFLOW_TEST {
        let maximum = core::hint::black_box(u32::MAX);
        let _ = core::hint::black_box(maximum + 1);
        panic!("Rust release overflow checks were disabled");
    }
}

unsafe extern "C" {
    fn r4os_zig_rust_fatal(message: *const u8, len: usize) -> !;
}

struct PanicText {
    bytes: [u8; 256],
    len: usize,
}

impl Write for PanicText {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        let count = core::cmp::min(text.len(), self.bytes.len() - self.len);
        self.bytes[self.len..self.len + count].copy_from_slice(&text.as_bytes()[..count]);
        self.len += count;
        if count != text.len() {
            Err(core::fmt::Error)
        } else {
            Ok(())
        }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    let mut message = PanicText {
        bytes: [0; 256],
        len: 0,
    };
    let _ = write!(&mut message, "Rust panic: {}", info.message());
    if let Some(location) = info.location() {
        let _ = write!(&mut message, " at {}:{}", location.file(), location.line());
    }
    // SAFETY: the existing fatal owner consumes these bounded bytes synchronously
    // and never returns; no reference or unwinding crosses the native C boundary.
    unsafe { r4os_zig_rust_fatal(message.bytes.as_ptr(), message.len) }
}
