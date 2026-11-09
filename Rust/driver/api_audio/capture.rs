//! Optional input extension of the original R4D audio record and permit.
//! No capture queue, media policy or DMA allocation lives in this bridge.
use super::*;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);
static LIVE: AtomicUsize = AtomicUsize::new(0);
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Lease {
    pub used: bool,
    pub id: a::AudioCaptureHandle,
    pub format: a::AudioCaptureFormat,
    pub frames: u32,
}
impl Lease {
    pub const EMPTY: Self = Self {
        used: false,
        id: a::AudioCaptureHandle {
            service: a::ProgramProcessHandle {
                instance_id: 0,
                reserved: 0,
                generation: 0,
            },
            source: a::AudioCaptureSourceId {
                registration: 0,
                source: 0,
                generation: 0,
            },
            stream: 0,
            generation: 0,
        },
        format: a::AudioCaptureFormat {
            rate_hz: 0,
            channels: 0,
            sample_format: 1,
            frame_bytes: 0,
        },
        frames: 0,
    };
}
fn same(x: &a::AudioCaptureHandle, y: &a::AudioCaptureHandle) -> bool {
    x.service.instance_id == y.service.instance_id
        && x.service.generation == y.service.generation
        && x.source.registration == y.source.registration
        && x.source.source == y.source.source
        && x.source.generation == y.source.generation
        && x.stream == y.stream
        && x.generation == y.generation
}
fn absent(input: &a::AudioCaptureRequest, result: i32, stopped: bool) -> a::AudioCaptureReply {
    a::AudioCaptureReply {
        result,
        handle: input.handle,
        flags: if stopped {
            a::audio_capture_reply_hardware_stopped
        } else {
            0
        },
        state: if stopped {
            a::audio_capture_state_closed
        } else {
            a::audio_capture_state_source_gone
        },
        ..Default::default()
    }
}
fn valid_format(f: a::AudioCaptureFormat) -> bool {
    f.rate_hz > 0
        && f.rate_hz <= 192000
        && (1..=8).contains(&f.channels)
        && f.sample_format == 1
        && f.frame_bytes == u32::from(f.channels) * 2
}
fn valid_source(s: &a::AudioCaptureSourceInfo) -> bool {
    let kind = s.flags & (a::audio_capture_source_adc | a::audio_capture_source_controlled);
    let end = s.name.iter().position(|&b| b == 0).unwrap_or(s.name.len());
    s.version == 1
        && s.size == size_of::<a::AudioCaptureSourceInfo>() as u32
        && s.id.generation > 0
        && (kind == a::audio_capture_source_adc || kind == a::audio_capture_source_controlled)
        && s.flags & !7 == 0
        && s.formats == 1
        && s.min_rate > 0
        && s.min_rate <= s.max_rate
        && s.max_rate <= 192000
        && s.min_channels > 0
        && s.min_channels <= s.max_channels
        && s.max_channels <= 8
        && s.max_queue_frames > 0
        && core::str::from_utf8(&s.name[..end]).is_ok()
        && ((s.flags & a::audio_capture_source_hardware_timestamp != 0
            && s.clock_id > 0
            && s.clock_generation > 0
            && s.clock_frequency > 0)
            || (s.flags & a::audio_capture_source_hardware_timestamp == 0
                && s.clock_id == 0
                && s.clock_generation == 0
                && s.clock_frequency == 0))
}
fn span(address: usize, bytes: usize) -> Option<(usize, usize)> {
    Some((address, address.checked_add(bytes)?))
}
fn separate(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 == a.1 || b.0 == b.1 || a.0 >= b.1 || b.0 >= a.1
}
/// Caller owns mapped disjoint buffers through the actual synchronous callback.
unsafe fn invoke(
    record: Record,
    input: &a::AudioCaptureRequest,
    bytes: *mut u8,
    capacity: u32,
    terminal: bool,
) -> Result<a::AudioCaptureReply, i32> {
    let token = if terminal {
        None
    } else {
        Some(Callback::enter(record.owner).ok_or(a::service_api_result_busy)?)
    };
    let descriptor = unsafe { ptr::read(record.input) };
    let callback = descriptor.io.ok_or(a::audio_capture_error_unsupported)?;
    let mut reply = a::AudioCaptureReply::default();
    let rc = unsafe { callback(descriptor.context, input, &mut reply, bytes, capacity) };
    drop(token);
    if rc != 0 {
        return Err(if rc < 0 {
            rc
        } else {
            a::audio_capture_error_provider
        });
    }
    if reply.version != 1
        || reply.size != size_of::<a::AudioCaptureReply>() as u32
        || reply.reserved != 0
        || reply.result > 0
        || reply.flags & !31 != 0
        || reply.state > a::audio_capture_state_source_gone
    {
        return Err(a::audio_capture_error_provider);
    }
    if input.op != a::audio_capture_op_catalog && !same(&input.handle, &reply.handle) {
        return Err(a::audio_capture_error_provider);
    }
    if reply.bytes > capacity
        || reply.bytes > a::audio_capture_max_read_bytes as u32
        || reply.frames > input.frames && input.op == a::audio_capture_op_read
    {
        return Err(a::audio_capture_error_provider);
    }
    if reply.result < 0 && (reply.bytes != 0 || reply.frames != 0) {
        return Err(a::audio_capture_error_provider);
    }
    if (reply.frames > 0 && reply.bytes == 0)
        || (reply.flags & a::audio_capture_reply_hardware_stopped != 0
            && matches!(
                reply.state,
                a::audio_capture_state_open | a::audio_capture_state_stopping
            ))
        || (input.op == a::audio_capture_op_read
            && reply.result == 0
            && reply.bytes == 0
            && (reply.state != a::audio_capture_state_stopped
                || reply.flags & a::audio_capture_reply_hardware_stopped == 0))
    {
        return Err(a::audio_capture_error_provider);
    }
    if reply.bytes > 0
        && (!valid_format(reply.format)
            || reply.bytes
                != reply
                    .frames
                    .checked_mul(reply.format.frame_bytes)
                    .ok_or(a::audio_capture_error_provider)?
            || input.op != a::audio_capture_op_read)
    {
        return Err(a::audio_capture_error_provider);
    }
    if reply.flags & a::audio_capture_reply_hardware_timestamp == 0 && reply.first_timestamp != 0 {
        return Err(a::audio_capture_error_provider);
    }
    if reply.flags & a::audio_capture_reply_source != 0 && !valid_source(&reply.source) {
        return Err(a::audio_capture_error_provider);
    }
    if reply.flags & a::audio_capture_reply_hardware_timestamp != 0
        && (reply.flags & a::audio_capture_reply_source == 0
            || reply.source.flags & a::audio_capture_source_hardware_timestamp == 0)
    {
        return Err(a::audio_capture_error_provider);
    }
    Ok(reply)
}
/// New immutable optional descriptor attaches only to its original exact R4D
/// audio registration. All descriptor/code/context bytes remain retained until
/// explicit unregister or terminal cleanup really acknowledges quiescence.
pub(crate) unsafe extern "C" fn register_capture2(
    name: *const u8,
    input: *const a::AudioCaptureBackend2,
) -> i32 {
    let Some(owner) = admission::backend_registration_owner() else {
        return -1;
    };
    if name.is_null() || input.is_null() || !(input as usize).is_multiple_of(8) {
        return -1;
    }
    let Some(_guard) = (unsafe { audio::OutputGuard::acquire(false) }) else {
        return a::service_api_result_busy;
    };
    let value = unsafe { ptr::read(input) };
    if value.magic != a::audio_capture_backend_magic
        || value.version != 1
        || value.size != size_of::<a::AudioCaptureBackend2>() as u32
        || value.flags != 0
        || value.io.is_none()
        || value.shutdown.is_none()
    {
        return -1;
    }
    let text = unsafe { text(name) };
    let Some(index) = (0..8).find(|&i| {
        let r = unsafe { (*STATE.0.get())[i] };
        r.used && r.owner == owner && text == &r.name[..r.name_len]
    }) else {
        return -1;
    };
    if !unsafe { (*STATE.0.get())[index].input }.is_null() {
        return a::service_api_result_busy;
    }
    let Ok(epoch) =
        NEXT_EPOCH.try_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
    else {
        return a::audio_capture_error_exhausted;
    };
    unsafe {
        let p = ptr::addr_of_mut!((*STATE.0.get())[index]);
        (*p).input = input;
        (*p).input_epoch = epoch;
        (*p).input_closing = false;
        (*p).capture = [Lease::EMPTY; 4];
    }
    0
}
/// Poll only. The original output permit owns descriptor/registry lifetime;
/// short runtime/stream/IRQ locks are never held across the foreign callback.
pub(crate) unsafe fn capture_io(
    input: *const a::AudioCaptureRequest,
    out: *mut a::AudioCaptureReply,
    bytes: *mut u8,
    capacity: u32,
) -> i32 {
    if input.is_null()
        || out.is_null()
        || bytes.is_null()
        || !(input as usize).is_multiple_of(8)
        || !(out as usize).is_multiple_of(8)
        || capacity > a::audio_capture_max_read_bytes as u32
    {
        return a::audio_capture_error_invalid;
    }
    let Some(rspan) = span(input as usize, size_of::<a::AudioCaptureRequest>()) else {
        return a::audio_capture_error_invalid;
    };
    let Some(ospan) = span(out as usize, size_of::<a::AudioCaptureReply>()) else {
        return a::audio_capture_error_invalid;
    };
    let Some(bspan) = span(bytes as usize, capacity as usize) else {
        return a::audio_capture_error_invalid;
    };
    if !separate(rspan, ospan) || !separate(rspan, bspan) || !separate(ospan, bspan) {
        return a::audio_capture_error_invalid;
    }
    let request = unsafe { ptr::read(input) };
    if request.version != 1
        || request.size != size_of::<a::AudioCaptureRequest>() as u32
        || request.flags != 0
        || request.buffer_bytes
            != if request.op == a::audio_capture_op_read {
                capacity
            } else {
                0
            }
        || request.op > 5
    {
        return a::audio_capture_error_invalid;
    }
    let Some(_guard) = (unsafe { audio::OutputGuard::acquire(false) }) else {
        return a::service_api_result_busy;
    };
    let reply = if request.op == a::audio_capture_op_catalog {
        if request.handle.source.registration != 0 || request.handle.source.generation != 0 {
            return a::audio_capture_error_invalid;
        }
        let mut ordinal = 0u64;
        let wanted = request.handle.source.source;
        let mut found = None;
        let mut any = false;
        'outer: for index in 0..8 {
            let r = unsafe { (*STATE.0.get())[index] };
            if !r.used || r.input.is_null() || r.input_closing {
                continue;
            }
            for local in 0..a::audio_capture_max_sources_per_backend {
                let mut query = request;
                query.handle.source.source = local;
                let mut response = match unsafe { invoke(r, &query, bytes, capacity, false) } {
                    Ok(v) => v,
                    Err(e) => return e,
                };
                if response.result < 0 {
                    if response.result == a::audio_capture_error_unsupported {
                        break;
                    } else {
                        return response.result;
                    }
                }
                if response.flags & a::audio_capture_reply_source == 0 {
                    break;
                }
                any = true;
                response.source.id.registration = r.input_epoch;
                response.handle.source = response.source.id;
                if ordinal == wanted {
                    found = Some(response);
                    break 'outer;
                }
                ordinal += 1;
            }
        }
        found.unwrap_or_else(|| {
            absent(
                &request,
                if any {
                    0
                } else {
                    a::audio_capture_error_unsupported
                },
                true,
            )
        })
    } else {
        if request.handle.source.registration == 0
            || request.handle.source.generation == 0
            || request.handle.stream == 0
            || request.handle.generation == 0
            || request.handle.service.instance_id == 0
            || request.handle.service.generation == 0
            || request.handle.service.reserved != 0
        {
            return a::audio_capture_error_invalid;
        }
        let Some(index) = (0..8).find(|&i| {
            let r = unsafe { (*STATE.0.get())[i] };
            r.used && !r.input.is_null() && r.input_epoch == request.handle.source.registration
        }) else {
            unsafe { out.write(absent(&request, a::audio_capture_error_source_gone, true)) };
            return 0;
        };
        let r = unsafe { (*STATE.0.get())[index] };
        if r.input_closing && request.op <= a::audio_capture_op_read {
            return a::audio_capture_error_busy;
        }
        let existing = r
            .capture
            .iter()
            .position(|p| p.used && same(&p.id, &request.handle));
        let lease = if request.op == a::audio_capture_op_open {
            if !valid_format(request.format)
                || request.queue_frames == 0
                || u64::from(request.queue_frames) * u64::from(request.format.frame_bytes)
                    > a::audio_capture_max_queue_bytes
            {
                return a::audio_capture_error_format;
            }
            if let Some(slot) = existing {
                let old = r.capture[slot];
                if old.format.rate_hz != request.format.rate_hz
                    || old.format.channels != request.format.channels
                    || old.format.sample_format != request.format.sample_format
                    || old.frames != request.queue_frames
                {
                    return a::audio_capture_error_stale;
                }
                slot
            } else {
                let Some(slot) = r.capture.iter().position(|p| !p.used) else {
                    return a::audio_capture_error_capacity;
                };
                unsafe {
                    (*STATE.0.get())[index].capture[slot] = Lease {
                        used: true,
                        id: request.handle,
                        format: request.format,
                        frames: request.queue_frames,
                    }
                };
                LIVE.fetch_add(1, Ordering::Release);
                slot
            }
        } else {
            match existing {
                Some(slot) => slot,
                None => {
                    unsafe {
                        out.write(absent(
                            &request,
                            if request.op == a::audio_capture_op_close {
                                0
                            } else {
                                a::audio_capture_error_stale
                            },
                            true,
                        ))
                    };
                    return 0;
                }
            }
        };
        let result = match unsafe { invoke(r, &request, bytes, capacity, false) } {
            Ok(v) => v,
            Err(e) => return e,
        };
        if result.flags & a::audio_capture_reply_hardware_stopped != 0
            && (request.op == a::audio_capture_op_close && result.result == 0
                || request.op == a::audio_capture_op_open && result.result < 0)
        {
            unsafe { (*STATE.0.get())[index].capture[lease] = Lease::EMPTY };
            LIVE.fetch_sub(1, Ordering::AcqRel);
        }
        result
    };
    unsafe { out.write(reply) };
    0
}
/// Called inside the existing exact driver lifecycle admission, before record
/// removal. Failure retains descriptor, leases, module and all driver DMA/pins.
pub(super) unsafe fn retire(index: usize) -> i32 {
    if unsafe { (*STATE.0.get())[index].input }.is_null() {
        return 0;
    }
    let Some(_guard) = (unsafe { audio::OutputGuard::acquire(false) }) else {
        return a::service_api_result_busy;
    };
    let record = unsafe { (*STATE.0.get())[index] };
    unsafe { (*STATE.0.get())[index].input_closing = true };
    let d = unsafe { ptr::read(record.input) };
    let Some(stop) = d.shutdown else {
        return a::service_api_result_busy;
    };
    if unsafe { stop(d.context) } != 0 {
        return a::service_api_result_busy;
    }
    let count = record.capture.iter().filter(|p| p.used).count();
    unsafe {
        let p = ptr::addr_of_mut!((*STATE.0.get())[index]);
        (*p).input = ptr::null();
        (*p).input_epoch = 0;
        (*p).capture = [Lease::EMPTY; 4];
    }
    if count > 0 {
        LIVE.fetch_sub(count, Ordering::AcqRel);
    }
    0
}
pub(super) unsafe fn closing(index: usize, value: bool) -> bool {
    if unsafe { (*STATE.0.get())[index].input }.is_null() {
        return true;
    }
    let Some(_guard) = (unsafe { audio::OutputGuard::acquire(false) }) else {
        return false;
    };
    unsafe { (*STATE.0.get())[index].input_closing = value };
    true
}
/// Original program reaper stage9 retries without releasing its exact process
/// reservation until every input lease obtains the real physical Close ACK.
pub(crate) unsafe fn close_for_owner(owner: crate::audio::mixer::Owner) -> bool {
    if LIVE.load(Ordering::Acquire) == 0 {
        return true;
    }
    let Some(_guard) = (unsafe { audio::OutputGuard::acquire(false) }) else {
        return false;
    };
    let mut scratch = [0u8; 1];
    for index in 0..8 {
        let record = unsafe { (*STATE.0.get())[index] };
        if !record.used || record.input.is_null() {
            continue;
        }
        for (slot, lease) in record.capture.iter().enumerate() {
            if !lease.used
                || lease.id.service.instance_id != owner.instance_id
                || lease.id.service.generation != owner.generation
            {
                continue;
            }
            let mut request = a::AudioCaptureRequest {
                op: a::audio_capture_op_stop,
                handle: lease.id,
                ..Default::default()
            };
            let stopped = unsafe { invoke(record, &request, scratch.as_mut_ptr(), 0, false) };
            if !matches!(stopped,Ok(v)if v.result==0&&v.flags&a::audio_capture_reply_hardware_stopped!=0)
            {
                return false;
            }
            request.op = a::audio_capture_op_close;
            let closed = unsafe { invoke(record, &request, scratch.as_mut_ptr(), 0, false) };
            if !matches!(closed,Ok(v)if v.result==0&&v.flags&a::audio_capture_reply_hardware_stopped!=0)
            {
                return false;
            }
            unsafe { (*STATE.0.get())[index].capture[slot] = Lease::EMPTY };
            LIVE.fetch_sub(1, Ordering::AcqRel);
        }
    }
    true
}
