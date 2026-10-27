//! Cursor attribution under the existing output permit and stream mutex.
//! No task, codec access, unbounded history or caller pointer is retained here.
use super::streams::stream_by_owner;
use super::*;

// Promises are stream metadata, independent of a foreign backend callback.
// Publication is under StreamLock. The output owner snapshots the revision
// with desired_flow and rechecks it under that same lock before reserving PCM.
static FLOW_REVISION: AtomicU64 = AtomicU64::new(0);

pub(super) fn flow_revision() -> u64 {
    FLOW_REVISION.load(Ordering::Acquire)
}

fn now_ns() -> u64 {
    clocksource::now_nanoseconds().unwrap_or(0)
}

unsafe fn retire_source(key: playback::SourceKey, through: u64, stage: playback::Stage) {
    unsafe {
        if key.slot >= MAX_STREAMS {
            return;
        }
        let p = stream(key.slot);
        if (*p).state != StreamState::Open {
            return;
        }
        if (*p).progress.apply(key, through, stage) && stage != playback::Stage::Dma {
            (*p).progress_eligible = stage == playback::Stage::Consumed;
        }
    }
}

/// Output permit held, stream mutex not held. Driver callbacks retain their
/// original admission; only the scalar result is applied after reacquiring.
pub(super) unsafe fn refresh_playback_owned() -> bool {
    unsafe {
        let mut value = a::AudioBackendPlayback::default();
        let mut generation = 0;
        let result = if let Some(slot) = g!(active_audio_slot) {
            let p = backend(slot);
            generation = (*p).generation;
            if !(*p).registered || (*p).closing {
                NO_ENDPOINT
            } else if let Some(callback) = (*p).playback {
                callback((*p).config.context, &mut value)
            } else {
                NO_ENDPOINT
            }
        } else {
            NO_ENDPOINT
        };
        let Some(_lock) = StreamLock::acquire() else {
            schedule_progress(&BACKEND_DUE, 2);
            return false;
        };
        if result == BUSY {
            schedule_progress(&BACKEND_DUE, 2);
            return false;
        }
        let header_valid = result == 0
            && value.version == 1
            && value.size == size_of::<a::AudioBackendPlayback>() as u32
            && value.reserved == 0
            && value.reserved1 == 0
            && value.rate == pcm::TARGET_RATE
            && value.channels == pcm::TARGET_CHANNELS
            && value.format == pcm::FORMAT_S16LE;
        let valid = header_valid
            && value.flags & a::audio_playback_valid != 0
            && value.output_generation != 0
            && value.reset_generation != 0
            && value.rate == pcm::TARGET_RATE
            && value.channels == pcm::TARGET_CHANNELS
            && value.format == pcm::FORMAT_S16LE
            && value.consumed_frames <= value.dma_frames
            && value.dma_frames <= value.accepted_frames;
        let current = playback::Position {
            epoch: playback::Epoch {
                backend: generation,
                output: if valid { value.output_generation } else { 0 },
                reset: if valid { value.reset_generation } else { 0 },
            },
            consumed: if valid { value.consumed_frames } else { 0 },
            dma: if valid { value.dma_frames } else { 0 },
        };
        let previous = (header_valid
            && value.flags & a::audio_playback_previous_valid != 0
            && value.previous_output_generation != 0
            && value.previous_reset_generation != 0
            && value.previous_consumed_frames <= value.previous_dma_frames
            && value.previous_dma_frames <= value.previous_accepted_frames)
            .then_some(playback::Position {
                epoch: playback::Epoch {
                    backend: generation,
                    output: value.previous_output_generation,
                    reset: value.previous_reset_generation,
                },
                consumed: value.previous_consumed_frames,
                dma: value.previous_dma_frames,
            });
        let ledger = ptr::addr_of_mut!(g!(playback_ledger));
        (*ledger).observe(current, previous, |key, through, known| {
            retire_source(key, through, known)
        });
        g!(playback_position) = current;
        g!(playback_valid) = valid;
        g!(playback_observed_ns) = now_ns();
        let now = g!(playback_observed_ns);
        let mut next_due = None::<u64>;
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            if (*p).state != StreamState::Open {
                continue;
            }
            if !valid {
                (*p).progress_eligible = false;
            }
            if valid && now != 0 && (*p).progress_eligible {
                (*p).progress.check_source(now);
                if let Some(due) = (*p).progress.pending_deadline() {
                    next_due = Some(next_due.map_or(due, |old| old.min(due)));
                }
            }
        }
        if !(*ledger).empty() {
            schedule_progress(&BACKEND_DUE, 2);
        } else if let Some(due) = next_due {
            schedule_progress(
                &BACKEND_DUE,
                due.saturating_sub(now).div_ceil(1_000_000).max(1),
            );
        }
        valid
    }
}

unsafe fn desired_flow() -> a::AudioBackendFlow {
    unsafe {
        let mut value = a::AudioBackendFlow::default();
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            if (*p).state != StreamState::Open || !(*p).progress.continuous {
                continue;
            }
            if value.flags == 0 || (*p).high_water_frames < value.high_water_frames {
                value.low_water_frames = (*p).low_water_frames;
                value.high_water_frames = (*p).high_water_frames;
            }
            value.source_due_ns = if value.source_due_ns == 0 {
                (*p).progress.expected_due_ns
            } else {
                value.source_due_ns.min((*p).progress.expected_due_ns)
            };
            value.flags = a::audio_flow_continuous;
        }
        value
    }
}

pub(super) unsafe fn apply_flow_owned() -> Result<u64, i32> {
    unsafe {
        let Some(slot) = g!(active_audio_slot) else {
            return Err(NO_ENDPOINT);
        };
        let p = backend(slot);
        let Some(callback) = (*p).flow else {
            return Ok(flow_revision());
        };
        let (value, revision) = {
            let Some(_lock) = StreamLock::acquire() else {
                return Err(BUSY);
            };
            (desired_flow(), flow_revision())
        };
        let result = callback((*p).config.context, &value);
        if result < 0 {
            Err(result)
        } else {
            Ok(revision)
        }
    }
}

pub(crate) unsafe fn set_stream_expectation_for_owner(
    owner: Owner,
    id: u32,
    input: *const a::AudioStreamExpectation,
) -> i32 {
    unsafe {
        if !owner.valid() || input.is_null() {
            return INVALID;
        }
        let value = *input;
        if value.version != 1
            || value.size != size_of::<a::AudioStreamExpectation>() as u32
            || value.reserved != 0
            || value.flags & !a::audio_flow_continuous != 0
            || value.stream_generation == 0
            || value.flags != 0
                && (value.source_due_ns == 0
                    || value.low_water_frames == 0
                    || value.low_water_frames >= value.high_water_frames
                    || value.high_water_frames > (RING_BYTES / pcm::TARGET_FRAME_BYTES) as u32)
        {
            return INVALID;
        }
        {
            let Some(_lock) = StreamLock::acquire() else {
                return BUSY;
            };
            let Some(p) = stream_by_owner(owner, id) else {
                return a::service_api_result_not_found;
            };
            if (*p).progress.generation != value.stream_generation {
                return a::service_api_result_not_found;
            }
            if (*p).transaction {
                return BUSY;
            }
            (*p).low_water_frames = value.low_water_frames;
            (*p).high_water_frames = value.high_water_frames;
            (*p).progress
                .expectation(value.flags != 0, value.source_due_ns);
            (*p).progress_eligible = true;
            FLOW_REVISION.fetch_add(1, Ordering::Release);
        }
        // The owner has committed the logical promise. The existing audio
        // worker applies device flow before its next PCM write; a busy
        // device must not make this accepted control look uncommitted.
        schedule_progress(&MIX_DUE, 0);
        schedule_progress(&BACKEND_DUE, 0);
        0
    }
}

pub(super) unsafe fn snapshot_stream_progress(
    p: *mut Stream,
    out: *mut a::AudioStreamProgress,
    fresh: bool,
) {
    unsafe {
        let s = &(*p).progress;
        let now = now_ns();
        let epoch = g!(playback_position).epoch;
        out.write(a::AudioStreamProgress {
            flags: a::audio_progress_source_valid
                | if g!(playback_valid) {
                    a::audio_progress_device_valid
                } else {
                    0
                }
                | if now != 0 {
                    a::audio_progress_age_valid
                } else {
                    0
                }
                | if fresh { a::audio_progress_fresh } else { 0 }
                | if s.continuous {
                    a::audio_progress_continuous
                } else {
                    a::audio_progress_ended
                }
                | if s.unconfirmed != 0 {
                    a::audio_progress_unconfirmed
                } else {
                    0
                },
            stream_id: (*p).id,
            stream_generation: s.generation,
            backend_generation: epoch.backend,
            output_generation: epoch.output,
            reset_generation: epoch.reset,
            accepted_source_bytes: (*p).total_written,
            accepted_frames: s.accepted,
            backend_frames: s.submitted,
            submitted_frames: s.dma,
            consumed_frames: s.consumed,
            unconfirmed_frames: s.unconfirmed,
            queued_frames: s.accepted.saturating_sub(s.retired),
            queue_age_ns: s.queue_age_ns(now),
            observed_ns: g!(playback_observed_ns),
            source_due_ns: s.expected_due_ns,
            source_starvations: s.starvations,
            ..a::AudioStreamProgress::default()
        });
    }
}

pub(crate) unsafe fn stream_progress_for_owner(
    owner: Owner,
    id: u32,
    generation: u64,
    out: *mut a::AudioStreamProgress,
) -> i32 {
    unsafe {
        if !owner.valid() || out.is_null() {
            return INVALID;
        }
        out.write(a::AudioStreamProgress::default());
        let Some(_output) = OutputGuard::acquire(false) else {
            return BUSY;
        };
        let fresh = refresh_playback_owned();
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        let Some(p) = stream_by_owner(owner, id) else {
            return a::service_api_result_not_found;
        };
        if generation != 0 && generation != (*p).progress.generation {
            return a::service_api_result_not_found;
        }
        snapshot_stream_progress(p, out, fresh);
        0
    }
}

/// Caller holds output permit, drops stream mutex across the actual driver
/// write. Its receipt binds acceptance and epoch in that same driver lock.
pub(super) unsafe fn write_mixed_tracked_owned(
    data: &[u8],
) -> (i32, u64, a::AudioBackendWriteReceipt) {
    unsafe {
        let mut receipt = a::AudioBackendWriteReceipt::default();
        let Some(slot) = g!(active_audio_slot) else {
            return (NO_ENDPOINT, 0, receipt);
        };
        let p = backend(slot);
        let generation = (*p).generation;
        if (*p).closing {
            return (NO_ENDPOINT, generation, receipt);
        }
        let result = if let Some(callback) = (*p).tracked_write {
            callback(
                (*p).config.context,
                data.as_ptr(),
                data.len() as u32,
                pcm::TARGET_RATE,
                pcm::TARGET_CHANNELS,
                pcm::FORMAT_S16LE,
                &mut receipt,
            )
        } else {
            write_active_pcm_owned(
                data,
                pcm::TARGET_RATE,
                pcm::TARGET_CHANNELS,
                pcm::FORMAT_S16LE,
            )
            .unwrap_or(NO_ENDPOINT)
        };
        (result, generation, receipt)
    }
}
