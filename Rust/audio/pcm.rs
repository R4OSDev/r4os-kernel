//! Original integer PCM conversion and exact streaming phase/tail contract.
#[cfg(test)]
use core::slice;
pub const TARGET_RATE: u32 = 48_000;
pub const TARGET_CHANNELS: u16 = 2;
pub const FORMAT_S16LE: u16 = 1;
pub const FORMAT_U8: u16 = 2;
pub const TARGET_FRAME_BYTES: usize = 4;
const PHASE_ONE: u64 = 65_536;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Work {
    pub sample_reads: u64,
    pub sample_writes: u64,
    pub interpolations: u64,
    pub direct_bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StereoFrame {
    pub left: i16,
    pub right: i16,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ResamplerState {
    pub rate: u32,
    pub channels: u16,
    pub format: u16,
    pub phase_q16: u64,
    pub phase_remainder: u32,
    pub previous: StereoFrame,
    pub previous_valid: bool,
    pub chunk_done: bool,
}
impl ResamplerState {
    pub fn reset(&mut self) {
        *self = Self::default()
    }
    pub fn reset_if_format_changed(&mut self, rate: u32, channels: u16, format: u16) {
        if self.rate == rate && self.channels == channels && self.format == format {
            return;
        }
        *self = Self {
            rate,
            channels,
            format,
            ..Self::default()
        };
    }
    pub fn begin_chunk(&mut self, rate: u32, channels: u16, format: u16) {
        self.reset_if_format_changed(rate, channels, format);
        self.chunk_done = false;
    }
}
pub fn source_frame_bytes(channels: u16, format: u16) -> Option<usize> {
    if channels == 0 || channels > 2 {
        return None;
    }
    match format {
        FORMAT_S16LE => Some(channels as usize * 2),
        FORMAT_U8 => Some(channels as usize),
        _ => None,
    }
}
// Preserve the original checked arithmetic and overflow boundary exactly.
#[allow(clippy::manual_div_ceil)]
pub fn output_frame_count(input_len: usize, rate: u32, channels: u16, format: u16) -> usize {
    if rate == 0 {
        return 0;
    }
    let Some(frame_bytes) = source_frame_bytes(channels, format) else {
        return 0;
    };
    let input_frames = input_len / frame_bytes;
    if input_frames == 0 {
        return 0;
    }
    ((input_frames as u64 * TARGET_RATE as u64 + rate as u64 - 1) / rate as u64)
        .try_into()
        .unwrap()
}
/// The returned span borrows only caller input; no native owner retains it.
pub(crate) unsafe fn take_direct_chunk<'a>(
    state: &mut ResamplerState,
    input: &'a [u8],
    rate: u32,
    channels: u16,
    format: u16,
    max_bytes: usize,
    work: *mut Work,
) -> Option<&'a [u8]> {
    unsafe {
        if rate != TARGET_RATE
            || channels != TARGET_CHANNELS
            || format != FORMAT_S16LE
            || input.len() < TARGET_FRAME_BYTES
        {
            return None;
        }
        state.reset_if_format_changed(rate, channels, format);
        if state.previous_valid || state.phase_q16 & (PHASE_ONE - 1) != 0 {
            return None;
        }
        if state.chunk_done {
            return Some(&input[..0]);
        }
        let frames = input.len() / TARGET_FRAME_BYTES;
        let limit = frames as u64 * PHASE_ONE;
        if state.phase_q16 >= limit {
            state.phase_q16 -= limit;
            state.chunk_done = true;
            return Some(&input[..0]);
        }
        let first: usize = (state.phase_q16 / PHASE_ONE).try_into().unwrap();
        let count = (frames - first).min(max_bytes / TARGET_FRAME_BYTES);
        state.phase_q16 += count as u64 * PHASE_ONE;
        if state.phase_q16 >= limit {
            state.phase_q16 -= limit;
            state.chunk_done = true;
        }
        if !work.is_null() {
            (*work).direct_bytes += (count * TARGET_FRAME_BYTES) as u64;
        }
        Some(
            &input[first * TARGET_FRAME_BYTES
                ..first * TARGET_FRAME_BYTES + count * TARGET_FRAME_BYTES],
        )
    }
}
#[cfg(test)]
pub(crate) unsafe fn convert_to_stereo_s16(
    input: &[u8],
    rate: u32,
    channels: u16,
    format: u16,
    output: &mut [u8],
    start_output_frame: usize,
    work: *mut Work,
) -> usize {
    unsafe {
        if rate == 0 {
            return 0;
        }
        let Some(frame_bytes) = source_frame_bytes(channels, format) else {
            return 0;
        };
        let input_frames = input.len() / frame_bytes;
        if input_frames == 0 {
            return 0;
        }
        let total = output_frame_count(input.len(), rate, channels, format);
        if start_output_frame >= total {
            return 0;
        }
        let mut out_pos = 0;
        let mut out_frame = start_output_frame;
        while out_pos + TARGET_FRAME_BYTES <= output.len() && out_frame < total {
            let scaled = out_frame as u64 * rate as u64 * 65_536 / TARGET_RATE as u64;
            let mut src_index: usize = (scaled >> 16).try_into().unwrap();
            if src_index >= input_frames {
                src_index = input_frames - 1;
            }
            let next_index = if src_index + 1 < input_frames {
                src_index + 1
            } else {
                src_index
            };
            let frac = (scaled & 0xffff) as u32;
            let a = read_frame(input, src_index, channels, format, work);
            let b = read_frame(input, next_index, channels, format, work);
            let left = lerp_s16(a.left, b.left, frac, work);
            let right = lerp_s16(a.right, b.right, frac, work);
            write_s16(output, out_pos, left, work);
            write_s16(output, out_pos + 2, right, work);
            out_pos += TARGET_FRAME_BYTES;
            out_frame += 1;
        }
        out_pos
    }
}
pub(crate) unsafe fn convert_streaming_to_stereo_s16(
    state: &mut ResamplerState,
    input: &[u8],
    rate: u32,
    channels: u16,
    format: u16,
    output: &mut [u8],
    work: *mut Work,
) -> usize {
    unsafe {
        if state.chunk_done || rate == 0 {
            return 0;
        }
        let Some(frame_bytes) = source_frame_bytes(channels, format) else {
            return 0;
        };
        let input_frames = input.len() / frame_bytes;
        if input_frames == 0 {
            return 0;
        }
        if let Some(direct) =
            take_direct_chunk(state, input, rate, channels, format, output.len(), work)
        {
            output[..direct.len()].copy_from_slice(direct);
            return direct.len();
        }
        state.reset_if_format_changed(rate, channels, format);
        let prefix = usize::from(state.previous_valid);
        let last_position = (input_frames + prefix - 1) as u64 * PHASE_ONE;
        let mut out_pos = 0;
        while out_pos + TARGET_FRAME_BYTES <= output.len() && state.phase_q16 <= last_position {
            let src_index: usize = (state.phase_q16 / PHASE_ONE).try_into().unwrap();
            let frac = (state.phase_q16 & (PHASE_ONE - 1)) as u32;
            let a = streaming_frame(state, input, src_index, prefix, channels, format, work);
            let b = if frac == 0 {
                a
            } else {
                streaming_frame(state, input, src_index + 1, prefix, channels, format, work)
            };
            write_s16(output, out_pos, lerp_s16(a.left, b.left, frac, work), work);
            write_s16(
                output,
                out_pos + 2,
                lerp_s16(a.right, b.right, frac, work),
                work,
            );
            out_pos += TARGET_FRAME_BYTES;
            advance_phase(state);
        }
        if state.phase_q16 > last_position {
            state.phase_q16 -= last_position;
            state.previous = read_frame(input, input_frames - 1, channels, format, work);
            state.previous_valid = true;
            state.chunk_done = true;
        }
        out_pos
    }
}
unsafe fn streaming_frame(
    state: &ResamplerState,
    input: &[u8],
    index: usize,
    prefix: usize,
    channels: u16,
    format: u16,
    work: *mut Work,
) -> StereoFrame {
    unsafe {
        if prefix != 0 && index == 0 {
            return state.previous;
        }
        read_frame(input, index - prefix, channels, format, work)
    }
}
fn advance_phase(state: &mut ResamplerState) {
    let numerator = state.rate as u64 * PHASE_ONE;
    state.phase_q16 += numerator / TARGET_RATE as u64;
    state.phase_remainder += (numerator % TARGET_RATE as u64) as u32;
    if state.phase_remainder >= TARGET_RATE {
        state.phase_q16 += 1;
        state.phase_remainder -= TARGET_RATE;
    }
}
pub(crate) unsafe fn finish_streaming_to_stereo_s16(
    state: &mut ResamplerState,
    output: &mut [u8],
    work: *mut Work,
) -> usize {
    unsafe {
        if !state.chunk_done || !state.previous_valid {
            return 0;
        }
        let mut out_pos = 0;
        while out_pos + TARGET_FRAME_BYTES <= output.len() && state.phase_q16 < PHASE_ONE {
            write_s16(output, out_pos, state.previous.left, work);
            write_s16(output, out_pos + 2, state.previous.right, work);
            out_pos += TARGET_FRAME_BYTES;
            advance_phase(state);
        }
        if state.phase_q16 >= PHASE_ONE {
            state.previous_valid = false;
            state.phase_q16 = 0;
            state.phase_remainder = 0;
        }
        out_pos
    }
}
// Preserve the original retained fractional tail calculation and overflow boundary.
#[allow(clippy::manual_div_ceil)]
pub fn pending_tail_bytes(state: &ResamplerState) -> usize {
    if !state.chunk_done || !state.previous_valid || state.rate == 0 || state.phase_q16 >= PHASE_ONE
    {
        return 0;
    }
    let remaining =
        (PHASE_ONE - state.phase_q16) * TARGET_RATE as u64 - state.phase_remainder as u64;
    let step = state.rate as u64 * PHASE_ONE;
    (((remaining + step - 1) / step) * TARGET_FRAME_BYTES as u64)
        .try_into()
        .unwrap()
}
unsafe fn read_frame(
    input: &[u8],
    frame_index: usize,
    channels: u16,
    format: u16,
    work: *mut Work,
) -> StereoFrame {
    unsafe {
        let Some(frame_bytes) = source_frame_bytes(channels, format) else {
            return StereoFrame::default();
        };
        let pos = frame_index * frame_bytes;
        if format == FORMAT_S16LE {
            let left = read_s16(input, pos, work);
            let right = if channels == 2 {
                read_s16(input, pos + 2, work)
            } else {
                left
            };
            return StereoFrame { left, right };
        }
        let left = u8_to_s16(input[pos]);
        let right = if channels == 2 {
            u8_to_s16(input[pos + 1])
        } else {
            left
        };
        StereoFrame { left, right }
    }
}
pub(crate) unsafe fn read_s16(input: &[u8], offset: usize, work: *mut Work) -> i16 {
    unsafe {
        if !work.is_null() {
            (*work).sample_reads += 1;
        }
        i16::from_le_bytes([input[offset], input[offset + 1]])
    }
}
pub(crate) unsafe fn write_s16(output: &mut [u8], offset: usize, value: i16, work: *mut Work) {
    unsafe {
        if !work.is_null() {
            (*work).sample_writes += 1;
        }
        let bytes = value.to_le_bytes();
        output[offset] = bytes[0];
        output[offset + 1] = bytes[1];
    }
}
fn u8_to_s16(value: u8) -> i16 {
    (value as i16 - 128) << 8
}
unsafe fn lerp_s16(a: i16, b: i16, frac: u32, work: *mut Work) -> i16 {
    unsafe {
        if !work.is_null() {
            (*work).interpolations += 1;
        }
        let av = a as i64;
        let bv = b as i64;
        clamp_i16(av + (bv - av) * frac as i64 / 65_536)
    }
}
fn clamp_i16(value: i64) -> i16 {
    if value > 32_767 {
        32_767
    } else if value < -32_768 {
        -32_768
    } else {
        value.try_into().unwrap()
    }
}
// Retained host test facades borrow this work record. The normal kernel has
// no global counter export; productive callers pass no diagnostic counter.
#[cfg(test)]
#[unsafe(no_mangle)]
pub static mut r4os_rust_audio_pcm_work: Work = Work {
    sample_reads: 0,
    sample_writes: 0,
    interpolations: 0,
    direct_bytes: 0,
};
#[cfg(test)]
#[repr(C)]
pub struct Span {
    pub pointer: *const u8,
    pub len: usize,
}
#[cfg(test)]
#[repr(C)]
pub struct Args {
    pub state: *mut ResamplerState,
    pub input: *const u8,
    pub input_len: usize,
    pub output: *mut u8,
    pub output_len: usize,
    pub rate: u32,
    pub channels: u16,
    pub format: u16,
    pub start: usize,
    pub work: *mut Work,
    pub span: *mut Span,
}
/// # Safety
/// The private facade supplies exact fixed records and live bounded spans.
#[cfg(test)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_audio_pcm(op: u32, args: *const Args) -> usize {
    unsafe {
        let a = &*args;
        let input = || {
            if a.input_len == 0 {
                &[][..]
            } else {
                slice::from_raw_parts(a.input, a.input_len)
            }
        };
        let output = || {
            if a.output_len == 0 {
                &mut [][..]
            } else {
                slice::from_raw_parts_mut(a.output, a.output_len)
            }
        };
        match op {
            0 => {
                (*a.state).reset();
                0
            }
            1 => {
                (*a.state).reset_if_format_changed(a.rate, a.channels, a.format);
                0
            }
            2 => {
                (*a.state).begin_chunk(a.rate, a.channels, a.format);
                0
            }
            3 => source_frame_bytes(a.channels, a.format).unwrap_or(0),
            4 => output_frame_count(a.input_len, a.rate, a.channels, a.format),
            5 => {
                if let Some(span) = take_direct_chunk(
                    &mut *a.state,
                    input(),
                    a.rate,
                    a.channels,
                    a.format,
                    a.output_len,
                    a.work,
                ) {
                    a.span.write(Span {
                        pointer: span.as_ptr(),
                        len: span.len(),
                    });
                    1
                } else {
                    0
                }
            }
            6 => convert_to_stereo_s16(
                input(),
                a.rate,
                a.channels,
                a.format,
                output(),
                a.start,
                a.work,
            ),
            7 => convert_streaming_to_stereo_s16(
                &mut *a.state,
                input(),
                a.rate,
                a.channels,
                a.format,
                output(),
                a.work,
            ),
            8 => finish_streaming_to_stereo_s16(&mut *a.state, output(), a.work),
            9 => pending_tail_bytes(&*a.state),
            10 => read_s16(input(), a.start, a.work) as u16 as usize,
            11 => {
                write_s16(output(), a.start, a.rate as u16 as i16, a.work);
                0
            }
            _ => 0,
        }
    }
}
const _: () = {
    assert!(size_of::<ResamplerState>() == 32);
    assert!(align_of::<ResamplerState>() == 8);
    assert!(core::mem::offset_of!(ResamplerState, previous) == 20);
};

#[cfg(test)]
const _: () = {
    assert!(size_of::<Args>() == 72);
};
