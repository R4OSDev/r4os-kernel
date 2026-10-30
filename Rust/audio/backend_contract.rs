//! Original declared format limits and separate Idle/physical Stop receipts.
use core::ffi::c_void;
pub const FORMAT_S16LE: u16 = 1;
pub const FORMAT_U8: u16 = 2;
pub const FORMAT_FLAG_S16LE: u32 = 1;
pub const FORMAT_FLAG_U8: u32 = 2;
pub type EndCallback = unsafe extern "C" fn(*mut c_void) -> i32;
#[derive(Clone, Copy, PartialEq)]
pub enum EndMode {
    Idle,
    Stop,
}
// Keep the original Idle-first and closing/physical-Stop callback branches.
#[allow(clippy::collapsible_if)]
pub(crate) unsafe fn finish_pcm(
    mode: EndMode,
    closing: bool,
    context: *mut c_void,
    stop: Option<EndCallback>,
    idle: Option<EndCallback>,
) -> i32 {
    unsafe {
        if mode == EndMode::Idle && !closing {
            if let Some(callback) = idle {
                return callback(context);
            }
        }
        if let Some(callback) = stop {
            return callback(context);
        }
        0
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Limits {
    pub formats: u32,
    pub min_rate: u32,
    pub max_rate: u32,
    pub max_channels: u16,
}
impl Limits {
    pub fn accepts(self, rate: u32, channels: u16, format: u16) -> bool {
        if rate == 0 || channels == 0 || self.max_channels == 0 {
            return false;
        }
        if self.min_rate != 0 && rate < self.min_rate {
            return false;
        }
        if self.max_rate != 0 && rate > self.max_rate {
            return false;
        }
        if channels > self.max_channels {
            return false;
        }
        let Some(flag) = format_flag(format) else {
            return false;
        };
        self.formats & flag != 0
    }
}
pub fn format_flag(format: u16) -> Option<u32> {
    match format {
        FORMAT_S16LE => Some(FORMAT_FLAG_S16LE),
        FORMAT_U8 => Some(FORMAT_FLAG_U8),
        _ => None,
    }
}
/// # Safety
/// Callback identity/context and the copied optional fixed limit are live for the call.
#[cfg(test)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_audio_contract(
    op: u32,
    limits: *const Limits,
    rate: u32,
    channels: u16,
    format: u16,
    closing: bool,
    context: *mut c_void,
    stop: Option<EndCallback>,
    idle: Option<EndCallback>,
) -> i32 {
    unsafe {
        match op {
            0 => finish_pcm(EndMode::Idle, closing, context, stop, idle),
            1 => finish_pcm(EndMode::Stop, closing, context, stop, idle),
            2 => i32::from((*limits).accepts(rate, channels, format)),
            3 => format_flag(format).unwrap_or(0) as i32,
            _ => 0,
        }
    }
}

const _: () = {
    assert!(size_of::<Limits>() == 16);
    assert!(core::mem::offset_of!(Limits, max_channels) == 12);
};
