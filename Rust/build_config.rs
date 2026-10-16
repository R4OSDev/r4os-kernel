//! Checked build constants and terminal Rust panic diagnostics.
use core::fmt::Write;

pub mod config {
    include!(concat!(env!("OUT_DIR"), "/kernel_config.rs"));
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
    crate::boot::fatal::rust_panic(&message.bytes[..message.len])
}
