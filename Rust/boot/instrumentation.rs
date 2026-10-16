//! Compile-time optional work only. Functional clocks, deadlines, owner
//! accounting, guards and resource cleanup never depend on this capture.
use crate::{build_config::config, platform::monotonic};
pub const DETAILED: bool = config::ENABLE_METRICS;
pub const WORK_CRITICAL_STRIDE: u32 = 64;
pub fn capture() -> monotonic::Stamp {
    if DETAILED {
        monotonic::capture()
    } else {
        monotonic::Stamp::default()
    }
}
/// # Safety
/// Caller owns one aligned writable private C stamp for this bounded capture.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_instrumentation_capture(out: *mut monotonic::Stamp) {
    unsafe { out.write(capture()) };
}
