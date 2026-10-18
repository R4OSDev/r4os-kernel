//! Exact streams operations of the parent owner; no duplicated state.
use super::*;

pub(crate) unsafe fn open_stream(rate: u32, channels: u16, format: u16) -> i32 {
    unsafe { open_stream_for_owner(Owner::KERNEL, rate, channels, format) }
}

pub(crate) unsafe fn open_stream_for_owner(
    owner: Owner,
    rate: u32,
    channels: u16,
    format: u16,
) -> i32 {
    unsafe {
        if !owner.valid() || !valid_stream_format(rate, channels, format) {
            return INVALID;
        }
        let Some(_output) = OutputGuard::acquire(true) else {
            return BUSY;
        };
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        if !active_output_present() {
            return NO_ENDPOINT;
        }
        let Some(slot) = free_stream_slot() else {
            return a::service_api_result_full;
        };
        let p = stream(slot);
        let ring = if (*p).ring.len == RING_BYTES {
            (*p).ring
        } else {
            let pointer = heap::r4os_rust_heap_alloc(RING_BYTES, 16);
            if pointer.is_null() {
                return -3;
            }
            Ring {
                pointer,
                len: RING_BYTES,
            }
        };
        let id = g!(next_stream_id);
        g!(next_stream_id) = id.wrapping_add(1);
        if g!(next_stream_id) == 0 {
            g!(next_stream_id) = 1;
        }
        p.write(Stream {
            state: StreamState::Open,
            id,
            owner,
            rate,
            channels,
            format,
            ring,
            ..Stream::EMPTY
        });
        boot(b"[AUDIO] stream open id=");
        boot_dec(id as u64);
        boot(b"\r\n");
        id.try_into().unwrap()
    }
}

pub(crate) unsafe fn write_stream(id: u32, data: *const u8, byte_count: u32) -> i32 {
    unsafe { write_stream_for_owner(Owner::KERNEL, id, data, byte_count) }
}

pub(crate) unsafe fn write_stream_for_owner(
    owner: Owner,
    id: u32,
    data: *const u8,
    byte_count: u32,
) -> i32 {
    unsafe {
        let write_start = timer::tick_count();
        if !owner.valid() {
            return INVALID;
        }
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        let Some(p) = stream_by_owner(owner, id) else {
            return a::service_api_result_not_found;
        };
        if byte_count == 0 {
            return 0;
        }
        if (*p).transaction {
            return BUSY;
        }
        if !active_output_present() {
            return NO_ENDPOINT;
        }
        let token = task_context::enter();
        if !token.admitted() {
            return BUSY;
        }
        (*p).transaction = true;
        let _transaction = Transaction { stream: p, token };
        let _timing = WriteTiming(write_start);
        let Some(frame_bytes) = pcm::source_frame_bytes((*p).channels, (*p).format) else {
            return INVALID;
        };
        let requested_bytes = byte_count as usize;
        let requested_frames = requested_bytes / frame_bytes;
        if requested_frames == 0 {
            return INVALID;
        }
        let mut fitting_frames =
            (((*p).ring.len - (*p).available) / pcm::TARGET_FRAME_BYTES) as u64 * (*p).rate as u64
                / pcm::TARGET_RATE as u64;
        if fitting_frames == 0 {
            let result = pump_available_locked(true);
            if result < 0 {
                return result;
            }
            fitting_frames = (((*p).ring.len - (*p).available) / pcm::TARGET_FRAME_BYTES) as u64
                * (*p).rate as u64
                / pcm::TARGET_RATE as u64;
        }
        let accepted_frames = requested_frames.min(fitting_frames.try_into().unwrap());
        if accepted_frames == 0 {
            return BUSY;
        }
        let accepted_bytes = accepted_frames * frame_bytes;
        let input = slice::from_raw_parts(data, accepted_bytes);
        let capacity = (*p).ring.len - (*p).available;
        let mut resampler = (*p).resampler;
        let rate = (*p).rate;
        let channels = (*p).channels;
        let format = (*p).format;
        unlock();
        let mut converted = core::mem::MaybeUninit::<[u8; RING_BYTES]>::uninit();
        resampler.begin_chunk(rate, channels, format);
        // Initialize the bounded Rust stack span only for conversion. Native direct spans retain
        // caller input; neither path holds a Rust view into resident stream metadata.
        let output = if let Some(direct) = pcm::take_direct_chunk(
            &mut resampler,
            input,
            rate,
            channels,
            format,
            capacity,
            ptr::null_mut(),
        ) {
            direct
        } else {
            let scratch = &mut converted.write([0u8; RING_BYTES])[..capacity];
            let produced = pcm::convert_streaming_to_stereo_s16(
                &mut resampler,
                input,
                rate,
                channels,
                format,
                scratch,
                ptr::null_mut(),
            );
            &scratch[..produced]
        };
        relock();
        if !resampler.chunk_done {
            return BUSY;
        }
        let was_empty = (*p).available == 0;
        assert_eq!(
            mixer::ring_write(
                (*p).ring.bytes_mut(),
                &mut (*p).write_pos,
                &mut (*p).available,
                output
            ),
            output.len()
        );
        (*p).resampler = resampler;
        if was_empty {
            (*p).deferred_once = false;
        }
        g!(stream_available_high_water) =
            g!(stream_available_high_water).max((*p).available as u64);
        (*p).total_written = (*p).total_written.wrapping_add(accepted_bytes as u64);
        g!(total_stream_writes) = g!(total_stream_writes).wrapping_add(1);
        if accepted_bytes < requested_bytes {
            g!(stream_write_truncations) = g!(stream_write_truncations).wrapping_add(1);
        }
        if pump_available_locked(false) == BUSY {
            schedule_progress(&MIX_DUE, 2);
        }
        accepted_bytes.try_into().unwrap()
    }
}

pub(crate) unsafe fn close_stream(id: u32) -> i32 {
    unsafe { close_stream_for_owner(Owner::KERNEL, id) }
}

pub(crate) unsafe fn close_stream_for_owner(owner: Owner, id: u32) -> i32 {
    unsafe {
        if !owner.valid() {
            return INVALID;
        }
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        let Some(p) = stream_by_owner(owner, id) else {
            return a::service_api_result_not_found;
        };
        if (*p).transaction || (*p).read_reserved {
            return BUSY;
        }
        let token = task_context::enter();
        if !token.admitted() {
            return BUSY;
        }
        (*p).transaction = true;
        let _transaction = Transaction { stream: p, token };
        let finish = finish_stream_pcm_locked(p);
        if finish < 0 {
            return finish;
        }
        let pump = pump_available_locked(true);
        if pump < 0 {
            return pump;
        }
        let stopped = stop_closing_streams_locked(1);
        if stopped < 0 {
            return stopped;
        }
        if !release_stream(p) {
            return BUSY;
        }
        boot(b"[AUDIO] stream close id=");
        boot_dec(id as u64);
        boot(b"\r\n");
        0
    }
}

pub(crate) unsafe fn close_streams_for_owner(owner: Owner) -> bool {
    unsafe {
        if !owner.valid() || (owner.instance_id == 0 && owner.generation == 0) {
            return false;
        }
        let Some(_lock) = StreamLock::acquire() else {
            return false;
        };
        let mut reserved = [false; MAX_STREAMS];
        for (i, reserve) in reserved.iter_mut().enumerate() {
            let p = stream(i);
            if (*p).state != StreamState::Open || !mixer::same_owner((*p).owner, owner) {
                continue;
            }
            if (*p).transaction || (*p).read_reserved {
                return false;
            }
            *reserve = true;
        }
        let token = task_context::enter();
        if !token.admitted() {
            return false;
        }
        for (i, value) in reserved.iter().enumerate() {
            if *value {
                (*stream(i)).transaction = true;
            }
        }
        let _transactions = Transactions { reserved, token };
        let mut matching = 0;
        let mut owner_queued = false;
        for (i, value) in reserved.iter().enumerate() {
            if !*value {
                continue;
            }
            let p = stream(i);
            if finish_stream_pcm_locked(p) < 0 {
                return false;
            }
            matching += 1;
            owner_queued |= (*p).available != 0;
        }
        if matching == 0 {
            return true;
        }
        if owner_queued && active_output_present() && pump_available_locked(true) < 0 {
            return false;
        }
        if stop_closing_streams_locked(matching) < 0 {
            return false;
        }
        for (i, value) in reserved.iter().enumerate() {
            let p = stream(i);
            if *value && (*p).read_reserved {
                return false;
            }
            if !*value && (*p).state == StreamState::Open && mixer::same_owner((*p).owner, owner) {
                return false;
            }
        }
        let mut released = true;
        for (i, value) in reserved.iter().enumerate() {
            if !*value {
                continue;
            }
            let p = stream(i);
            g!(stream_dropped_bytes) = g!(stream_dropped_bytes).wrapping_add((*p).available as u64);
            (*p).available = 0;
            if !release_stream(p) {
                released = false;
            }
        }
        released
    }
}

pub(crate) unsafe fn set_volume(id: u32, volume: u32) -> i32 {
    unsafe { set_volume_for_owner(Owner::KERNEL, id, volume) }
}

pub(crate) unsafe fn set_volume_for_owner(owner: Owner, id: u32, volume: u32) -> i32 {
    unsafe {
        if !owner.valid() {
            return INVALID;
        }
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        let Some(p) = stream_by_owner(owner, id) else {
            return a::service_api_result_not_found;
        };
        (*p).volume = volume;
        0
    }
}

pub(super) fn valid_stream_format(rate: u32, channels: u16, format: u16) -> bool {
    rate != 0
        && rate <= 192000
        && channels != 0
        && channels <= 2
        && (format == FORMAT_S16LE || format == FORMAT_U8)
}

pub(super) unsafe fn free_stream_slot() -> Option<usize> {
    unsafe {
        (0..MAX_STREAMS).find(|&i| {
            let state = (*stream(i)).state;
            state == StreamState::Empty || state == StreamState::Closed
        })
    }
}

pub(super) unsafe fn stream_by_owner(owner: Owner, id: u32) -> Option<*mut Stream> {
    unsafe {
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            if (*p).state == StreamState::Open
                && (*p).id == id
                && mixer::same_owner((*p).owner, owner)
            {
                return Some(p);
            }
        }
        None
    }
}

pub(super) unsafe fn open_stream_count() -> u32 {
    unsafe {
        (0..MAX_STREAMS)
            .filter(|&i| (*stream(i)).state == StreamState::Open)
            .count() as u32
    }
}

pub(super) unsafe fn release_stream(p: *mut Stream) -> bool {
    unsafe {
        if (*p).read_reserved {
            return false;
        }
        let id = (*p).id;
        let ring = (*p).ring;
        p.write(Stream {
            state: StreamState::Closed,
            id,
            ring,
            ..Stream::EMPTY
        });
        true
    }
}

pub(super) unsafe fn stop_closing_streams_locked(closing_count: u32) -> i32 {
    unsafe {
        if open_stream_count() != closing_count || g!(sid_acquired) || g!(midi_acquired) {
            return 0;
        }
        unlock();
        let Some(_output) = OutputGuard::acquire(false) else {
            relock();
            return BUSY;
        };
        relock();
        if open_stream_count() != closing_count || g!(sid_acquired) || g!(midi_acquired) {
            return 0;
        }
        unlock();
        let result = idle_active_pcm_owned();
        relock();
        result
    }
}

pub(super) unsafe fn finish_stream_pcm_locked(p: *mut Stream) -> i32 {
    unsafe {
        while (*p).resampler.previous_valid {
            if !(*p).resampler.chunk_done {
                return BUSY;
            }
            let free_bytes = (*p).ring.len - (*p).available;
            let mut scratch = [0; MIX_QUANTUM_BYTES];
            let mut resampler = (*p).resampler;
            unlock();
            let produced = pcm::finish_streaming_to_stereo_s16(
                &mut resampler,
                &mut scratch[..free_bytes.min(MIX_QUANTUM_BYTES)],
                ptr::null_mut(),
            );
            relock();
            (*p).resampler = resampler;
            if mixer::ring_write(
                (*p).ring.bytes_mut(),
                &mut (*p).write_pos,
                &mut (*p).available,
                &scratch[..produced],
            ) != produced
            {
                return BUSY;
            }
            if !(*p).resampler.previous_valid {
                return 0;
            }
            let pumped = pump_available_locked(true);
            if pumped < 0 {
                return pumped;
            }
        }
        0
    }
}

pub(super) unsafe fn pump_available_locked(force: bool) -> i32 {
    unsafe {
        unlock();
        let Some(output) = OutputGuard::acquire(false) else {
            relock();
            return BUSY;
        };
        let result = pump_available_output_owned(force);
        drop(output);
        relock();
        result
    }
}

pub(super) unsafe fn pump_available_output_owned(force: bool) -> i32 {
    unsafe {
        relock();
        let _lock = StreamLock;
        let max_passes = MAX_STREAMS * (RING_BYTES / MIX_QUANTUM_BYTES + 2);
        for _ in 0..max_passes {
            let mut selected = [false; MAX_STREAMS];
            let mut open_count = 0;
            let mut ready_count = 0;
            let mut single_ready = 0;
            let mut chunk_bytes = MIX_QUANTUM_BYTES;
            for i in 0..MAX_STREAMS {
                let p = stream(i);
                if (*p).state != StreamState::Open {
                    continue;
                }
                open_count += 1;
                if (*p).available < pcm::TARGET_FRAME_BYTES {
                    continue;
                }
                ready_count += 1;
                single_ready = i;
            }
            if ready_count == 0 {
                return 0;
            }
            if ready_count == 1 {
                let p = stream(single_ready);
                if open_count > 1 && !force && !(*p).deferred_once {
                    (*p).deferred_once = true;
                    schedule_progress(&MIX_DUE, 2);
                    return 0;
                }
                selected[single_ready] = true;
                chunk_bytes = chunk_bytes.min((*p).available);
            } else {
                for (i, value) in selected.iter_mut().enumerate() {
                    let p = stream(i);
                    if (*p).state != StreamState::Open || (*p).available < pcm::TARGET_FRAME_BYTES {
                        continue;
                    }
                    *value = true;
                    chunk_bytes = chunk_bytes.min((*p).available);
                }
            }
            chunk_bytes -= chunk_bytes % pcm::TARGET_FRAME_BYTES;
            if chunk_bytes == 0 {
                return 0;
            }
            let mut sources = [mixer::Source::EMPTY; MAX_STREAMS];
            let mut source_count = 0;
            for (i, value) in selected.iter().enumerate() {
                if !*value {
                    continue;
                }
                let p = stream(i);
                (*p).read_reserved = true;
                sources[source_count] = mixer::Source {
                    ring: (*p).ring.pointer,
                    len: (*p).ring.len,
                    read_pos: (*p).read_pos,
                    volume: (*p).volume,
                };
                source_count += 1;
            }
            unlock();
            let scratch = ptr::addr_of_mut!(g!(mix_scratch)).cast::<u8>();
            mixer::mix(
                &sources[..source_count],
                slice::from_raw_parts_mut(scratch, chunk_bytes),
                ptr::null_mut(),
            );
            let start = timer::tick_count();
            let result = write_active_pcm_owned(
                slice::from_raw_parts(scratch, chunk_bytes),
                pcm::TARGET_RATE,
                pcm::TARGET_CHANNELS,
                pcm::FORMAT_S16LE,
            )
            .unwrap_or(NO_ENDPOINT);
            relock();
            for (i, value) in selected.iter().enumerate() {
                if *value {
                    (*stream(i)).read_reserved = false;
                }
            }
            g!(backend_write_calls) = g!(backend_write_calls).wrapping_add(1);
            record_tick_stat(
                ptr::addr_of_mut!(g!(backend_write_total_ticks)),
                ptr::addr_of_mut!(g!(backend_write_max_ticks)),
                ptr::addr_of_mut!(g!(backend_write_last_ticks)),
                start,
            );
            if result != 0 {
                if result != BUSY {
                    g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
                }
                return result;
            }
            g!(total_backend_ok) = g!(total_backend_ok).wrapping_add(1);
            for (i, value) in selected.iter().enumerate() {
                if !*value {
                    continue;
                }
                let p = stream(i);
                mixer::ring_consume(
                    (*p).ring.len,
                    &mut (*p).read_pos,
                    &mut (*p).available,
                    chunk_bytes,
                );
                (*p).deferred_once = false;
            }
        }
        BUSY
    }
}

pub(super) unsafe fn stop_pcm_if_no_streams() {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            return;
        };
        relock();
        let empty = open_stream_count() == 0;
        unlock();
        if empty {
            idle_active_pcm_owned();
        }
    }
}
