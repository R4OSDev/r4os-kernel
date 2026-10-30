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
        let generation = g!(next_stream_generation);
        let Some(next_generation) = generation.checked_add(1) else {
            return a::service_api_result_full;
        };
        g!(next_stream_generation) = next_generation;
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
            progress: playback::Source::new(generation),
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
        let Some(frame_bytes) = pcm::source_frame_bytes((*p).channels, (*p).format) else {
            return INVALID;
        };
        let requested_bytes = byte_count as usize;
        let requested_frames = requested_bytes / frame_bytes;
        if requested_frames == 0 {
            return INVALID;
        }
        let accepted_ns = clocksource::now_nanoseconds().unwrap_or(0);
        if !(*p).progress.can_accept(accepted_ns) {
            schedule_progress(&MIX_DUE, 0);
            return BUSY;
        }
        let mut canonical_free = ((*p).ring.len - (*p).available) / pcm::TARGET_FRAME_BYTES;
        if (*p).progress.continuous {
            canonical_free = canonical_free.min(
                u64::from((*p).high_water_frames)
                    .saturating_sub((*p).progress.accepted.saturating_sub((*p).progress.retired))
                    as usize,
            );
        }
        let fitting_frames = canonical_free as u64 * (*p).rate as u64 / pcm::TARGET_RATE as u64;
        let accepted_frames = requested_frames.min(fitting_frames.try_into().unwrap());
        if accepted_frames == 0 {
            schedule_progress(&MIX_DUE, 0);
            return BUSY;
        }
        let accepted_bytes = accepted_frames * frame_bytes;
        let input = slice::from_raw_parts(data, accepted_bytes);
        let capacity = canonical_free * pcm::TARGET_FRAME_BYTES;
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
        if !output.is_empty()
            && !(*p)
                .progress
                .accept((output.len() / pcm::TARGET_FRAME_BYTES) as u64, accepted_ns)
        {
            return BUSY;
        }
        (*p).progress_eligible = true;
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
        g!(stream_available_high_water) =
            g!(stream_available_high_water).max((*p).available as u64);
        (*p).total_written = (*p).total_written.wrapping_add(accepted_bytes as u64);
        g!(total_stream_writes) = g!(total_stream_writes).wrapping_add(1);
        if accepted_bytes < requested_bytes {
            g!(stream_write_truncations) = g!(stream_write_truncations).wrapping_add(1);
        }
        // Acceptance never dispatches the backend on this RPC caller.
        schedule_progress(&MIX_DUE, 0);
        accepted_bytes.try_into().unwrap()
    }
}

pub(crate) unsafe fn close_stream(id: u32) -> i32 {
    unsafe { close_stream_for_owner(Owner::KERNEL, id) }
}

pub(crate) unsafe fn close_stream_for_owner(owner: Owner, id: u32) -> i32 {
    unsafe { close_stream_inner(owner, id, 0, ptr::null_mut()) }
}

pub(crate) unsafe fn close_stream_progress_for_owner(
    owner: Owner,
    id: u32,
    generation: u64,
    out: *mut a::AudioStreamProgress,
) -> i32 {
    if generation == 0 || out.is_null() {
        return INVALID;
    }
    unsafe {
        out.write(a::AudioStreamProgress::default());
        close_stream_inner(owner, id, generation, out)
    }
}

unsafe fn close_stream_inner(
    owner: Owner,
    id: u32,
    generation: u64,
    out: *mut a::AudioStreamProgress,
) -> i32 {
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
        if generation != 0 && generation != (*p).progress.generation {
            return a::service_api_result_not_found;
        }
        if (*p).transaction || (*p).read_reserved {
            return BUSY;
        }
        let token = task_context::enter();
        if !token.admitted() {
            return BUSY;
        }
        (*p).transaction = true;
        let _transaction = Transaction { stream: p, token };
        (*p).progress.expectation(false, 0);
        let finish = finish_stream_pcm_locked(p);
        if finish < 0 {
            return finish;
        }
        let pump = pump_draining_stream_locked(p);
        if pump < 0 {
            return pump;
        }
        let (stopped, fresh) = stop_closing_and_observe_locked(1, !out.is_null());
        if stopped < 0 {
            return stopped;
        }
        if !out.is_null() {
            // A shared output may still carry this closed source. Its last
            // known prefix is retained; no future callback can update a
            // freed/reused stream, so the remaining tail is unconfirmed.
            if (*p).progress.retired < (*p).progress.submitted {
                let through = (*p).progress.submitted;
                (*p).progress.retire(generation, through, false);
            }
            snapshot_stream_progress(p, out, fresh);
            (*out).flags |= a::audio_progress_closed;
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
        if owner_queued
            && active_output_present()
            && pump_goal_locked(PumpGoal::Streams(reserved)) < 0
        {
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
    rate <= 192000
        && stream_window_fits(rate, channels, format, RING_BYTES / pcm::TARGET_FRAME_BYTES)
}

pub(super) fn stream_window_fits(
    rate: u32,
    channels: u16,
    format: u16,
    canonical_frames: usize,
) -> bool {
    let Some(frame_bytes) = pcm::source_frame_bytes(channels, format) else {
        return false;
    };
    // Admission consumes complete source frames. Even an empty window must
    // fit one frame's canonical expansion for every retained resampler phase;
    // a smaller first output does not make later chunks progress-capable.
    let minimum = pcm::output_frame_count(frame_bytes, rate, channels, format);
    minimum != 0 && minimum <= canonical_frames
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
    unsafe { stop_closing_and_observe_locked(closing_count, false).0 }
}

unsafe fn stop_closing_and_observe_locked(closing_count: u32, observe: bool) -> (i32, bool) {
    unsafe {
        if open_stream_count() != closing_count || sid_producing() || midi_producing() {
            return (0, false);
        }
        unlock();
        let Some(_output) = OutputGuard::acquire(false) else {
            relock();
            return (BUSY, false);
        };
        relock();
        if open_stream_count() != closing_count || sid_producing() || midi_producing() {
            return (0, false);
        }
        unlock();
        let result = idle_active_pcm_owned();
        // Capture the final cursor under the same output permit as Drain.
        // Dropping and reacquiring it could turn an already successful Close
        // into BUSY, or let a new output displace the exact final epoch.
        let fresh = result == 0 && observe && refresh_playback_owned();
        relock();
        (result, fresh)
    }
}

pub(super) unsafe fn finish_stream_pcm_locked(p: *mut Stream) -> i32 {
    unsafe {
        while (*p).resampler.previous_valid {
            if !(*p).resampler.chunk_done {
                return BUSY;
            }
            let free_bytes = (*p).ring.len - (*p).available;
            let accepted_ns = clocksource::now_nanoseconds().unwrap_or(0);
            if !(*p).progress.can_accept(accepted_ns) {
                return BUSY;
            }
            let mut scratch = [0; MIX_QUANTUM_BYTES];
            let mut resampler = (*p).resampler;
            unlock();
            let produced = pcm::finish_streaming_to_stereo_s16(
                &mut resampler,
                &mut scratch[..free_bytes.min(MIX_QUANTUM_BYTES)],
                ptr::null_mut(),
            );
            relock();
            if produced != 0
                && !(*p)
                    .progress
                    .accept((produced / pcm::TARGET_FRAME_BYTES) as u64, accepted_ns)
            {
                return BUSY;
            }
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
            let pumped = pump_draining_stream_locked(p);
            if pumped < 0 {
                return pumped;
            }
        }
        0
    }
}

#[derive(Clone, Copy)]
enum PumpGoal {
    All,
    Streams([bool; MAX_STREAMS]),
}

unsafe fn goal_drained(goal: PumpGoal) -> bool {
    unsafe {
        match goal {
            PumpGoal::All => false,
            PumpGoal::Streams(mask) => !(0..MAX_STREAMS)
                .any(|i| mask[i] && (*stream(i)).available >= pcm::TARGET_FRAME_BYTES),
        }
    }
}

// Closing owns a pinned transaction on these exact stream slots. Other
// producers may continue; their remaining PCM cannot prolong this close.
unsafe fn pump_draining_stream_locked(p: *mut Stream) -> i32 {
    unsafe {
        let mut mask = [false; MAX_STREAMS];
        for (i, selected) in mask.iter_mut().enumerate() {
            *selected = ptr::eq(p, stream(i));
        }
        pump_goal_locked(PumpGoal::Streams(mask))
    }
}

unsafe fn pump_goal_locked(goal: PumpGoal) -> i32 {
    unsafe {
        unlock();
        let Some(output) = OutputGuard::acquire(false) else {
            relock();
            return BUSY;
        };
        let result = pump_output_owned(goal);
        drop(output);
        relock();
        result
    }
}

pub(super) unsafe fn pump_available_output_owned() -> i32 {
    unsafe { pump_output_owned(PumpGoal::All) }
}

unsafe fn pump_output_owned(goal: PumpGoal) -> i32 {
    unsafe {
        refresh_playback_owned();
        let mut flow_revision = match playback_progress::apply_flow_owned() {
            Ok(revision) => revision,
            Err(result) => return result,
        };
        relock();
        let _lock = StreamLock;
        let max_passes = MAX_STREAMS * (RING_BYTES / MIX_QUANTUM_BYTES + 2);
        for _ in 0..max_passes {
            if flow_revision != playback_progress::flow_revision() {
                // A promise may commit while the output owner is inside a
                // foreign callback. Do not reserve PCM accepted after that
                // promise until its exact newer flow has actually returned.
                // The StreamLock prevents a publication between this check
                // and reservation; an appended tail cannot enter this chunk.
                unlock();
                let applied = playback_progress::apply_flow_owned();
                relock();
                flow_revision = match applied {
                    Ok(revision) => revision,
                    Err(result) => return result,
                };
                continue;
            }
            if goal_drained(goal) {
                return 0;
            }
            let mut selected = [false; MAX_STREAMS];
            let mut ready_count = 0;
            let mut single_ready = 0;
            let mut chunk_bytes = MIX_QUANTUM_BYTES;
            for i in 0..MAX_STREAMS {
                let p = stream(i);
                if (*p).state != StreamState::Open {
                    continue;
                }
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
            if g!(playback_ledger).full() {
                schedule_progress(&BACKEND_DUE, 2);
                return BUSY;
            }
            let mut sources = [mixer::Source::EMPTY; MAX_STREAMS];
            let mut contribution = playback::Span::<MAX_STREAMS>::EMPTY;
            let mut source_count = 0;
            for (i, value) in selected.iter().enumerate() {
                if !*value {
                    continue;
                }
                let p = stream(i);
                (*p).read_reserved = true;
                contribution.sources[source_count] = playback::Contribution {
                    key: playback::SourceKey {
                        slot: i,
                        generation: (*p).progress.generation,
                    },
                    first: (*p).progress.submitted,
                };
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
            let (result, backend_generation, receipt) =
                write_mixed_tracked_owned(slice::from_raw_parts(scratch, chunk_bytes));
            relock();
            for (i, value) in selected.iter().enumerate() {
                if *value {
                    (*stream(i)).read_reserved = false;
                }
            }
            g!(backend_write_calls) = g!(backend_write_calls).wrapping_add(1);

            if result != 0 {
                if result != BUSY {
                    g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
                }
                return result;
            }
            g!(total_backend_ok) = g!(total_backend_ok).wrapping_add(1);
            contribution.count = source_count;
            contribution.frames = (chunk_bytes / pcm::TARGET_FRAME_BYTES) as u64;
            contribution.first = receipt.first_frame;
            contribution.epoch = playback::Epoch {
                backend: backend_generation,
                output: receipt.output_generation,
                reset: receipt.reset_generation,
            };
            for c in &contribution.sources[..source_count] {
                let p = stream(c.key.slot);
                assert_eq!((*p).progress.submit(contribution.frames), Some(c.first));
            }
            let valid_receipt = receipt.version == 1
                && receipt.size == size_of::<a::AudioBackendWriteReceipt>() as u32
                && receipt.flags & a::audio_playback_valid != 0
                && receipt.frames == contribution.frames
                && receipt.reserved == 0
                && receipt.flags == a::audio_playback_valid
                && backend_generation != 0
                && receipt.output_generation != 0
                && receipt.reset_generation != 0
                && receipt.first_frame.checked_add(receipt.frames).is_some();
            contribution.tracking_valid = valid_receipt;
            if !valid_receipt {
                contribution.first = 0;
                contribution.epoch = playback::Epoch::default();
            }
            // An unavailable receipt retains its FIFO position; retiring it
            // immediately would skip older still-valid mixed PCM.
            assert!(g!(playback_ledger).push(contribution));
            schedule_progress(&BACKEND_DUE, 2);
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
            }
        }
        if goal_drained(goal) { 0 } else { BUSY }
    }
}
