//! Exact outputs operations of the parent owner; no duplicated state.
use super::streams::open_stream_count;
use super::*;

pub(crate) unsafe fn register_mixer_backend_z(name: *const u8, pointer: *const c_void) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        store_mixer(name, pointer);
        boot(b"[AUDIO] mixer backend ");
        boot(name);
        boot(b" [OK]\r\n");
        0
    }
}

pub(crate) unsafe fn register_external_audio_backend_z(
    name: *const u8,
    config: BackendRegistration,
) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        if register_audio_backend_internal(name, config) {
            0
        } else {
            -2
        }
    }
}

pub(crate) unsafe fn register_external_audio_outputs(
    name: &[u8],
    outputs: a::AudioOutputExtension,
    idle: Option<StopPcmCtxFn>,
) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let p = backend(slot);
        if (*p).closing {
            return false;
        }
        (*p).outputs = Some(outputs);
        (*p).idle = idle;
        g!(initial_output_pending) = true;
        true
    }
}

pub(super) unsafe fn output_record(slot: usize, index: u32, out: *mut a::AudioOutputInfo) -> i32 {
    unsafe {
        let p = backend(slot);
        out.write(a::AudioOutputInfo::default());
        let c = (*p).config;
        if !(*p).registered || (*p).closing || (c.write.is_none()) {
            return 0;
        }
        if let Some(outputs) = (*p).outputs {
            let result = (outputs.query)(c.context as u64, index, out);
            if result != 1 {
                return result;
            }
            if (*out).version != 1
                || (*out).size != size_of::<a::AudioOutputInfo>() as u32
                || !(*out).id.contains(&0)
                || !(*out).name.contains(&0)
            {
                return -1;
            }
            (*out).flags &= !a::audio_output_flag_active;
            if g!(active_audio_slot) == Some(slot)
                && (outputs.active)(c.context as u64) == i32::try_from(index).unwrap()
            {
                (*out).flags |= a::audio_output_flag_active;
            }
            return 1;
        }
        if index != 0 {
            return 0;
        }
        let len = (*p).name_len;
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*p).name).cast::<u8>(),
            ptr::addr_of_mut!((*out).id).cast(),
            len,
        );
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*p).name).cast::<u8>(),
            ptr::addr_of_mut!((*out).name).cast(),
            len,
        );
        (*out).availability = a::audio_output_available;
        if let Some(status) = c.status {
            let mut value = BackendStatus::default();
            if status(c.context, &mut value) < 0 || value.active == 0 {
                (*out).availability = a::audio_output_unavailable;
            }
        }
        if g!(active_audio_slot) == Some(slot) {
            (*out).flags = a::audio_output_flag_active;
        }
        (*out).preferred_rate = DEFAULT_RATE;
        (*out).channels = DEFAULT_CHANNELS;
        (*out).format = FORMAT_S16LE;
        1
    }
}

pub(crate) unsafe fn register_external_audio_playback(
    name: &[u8],
    tracked: TrackedWriteCtxFn,
    playback: PlaybackCtxFn,
    flow: FlowCtxFn,
) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let p = backend(slot);
        if (*p).closing {
            return false;
        }
        (*p).tracked_write = Some(tracked);
        (*p).playback = Some(playback);
        (*p).flow = Some(flow);
        true
    }
}

pub(crate) unsafe fn audio_output_info(index: u32, out: *mut a::AudioOutputInfo) -> i32 {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return BUSY;
        };
        let mut ordinal = 0;
        for slot in 0..MAX_AUDIO_BACKENDS {
            for local in 0..32 {
                let result = output_record(slot, local, out);
                if result < 0 {
                    return result;
                }
                if result == 0 {
                    break;
                }
                if ordinal == index {
                    return 1;
                }
                ordinal += 1;
            }
        }
        out.write(a::AudioOutputInfo::default());
        0
    }
}

pub(super) unsafe fn select_output(slot: usize, local: u32) -> i32 {
    unsafe {
        let p = backend(slot);
        let changed = g!(active_audio_slot) != Some(slot)
            || (*p).outputs.is_some_and(|outputs| {
                (outputs.active)((*p).config.context as u64) != local as i32
            });
        // Selection can also finish the preceding backend. Reserve both
        // identities before any callback, rather than ever reuse a receipt.
        if changed
            && OUTPUT_REVISION
                .load(Ordering::Relaxed)
                .checked_add(2)
                .is_none()
        {
            return INVALID;
        }
        if let Some(outputs) = (*p).outputs {
            let result = (outputs.select)((*p).config.context as u64, local);
            if result < 0 {
                return result;
            }
        }
        if g!(active_audio_slot) != Some(slot) {
            let stopped = stop_active_pcm_owned();
            if stopped < 0 {
                return stopped;
            }
            set_active_backend(slot);
        }
        if changed {
            OUTPUT_REVISION.fetch_add(1, Ordering::Release);
        }
        0
    }
}

pub(crate) unsafe fn audio_select_output(id: &[u8]) -> i32 {
    unsafe {
        if id.is_empty() || id.len() >= 64 {
            return INVALID;
        }
        let Some(_output) = OutputGuard::acquire(true) else {
            return BUSY;
        };
        let mut record = a::AudioOutputInfo::default();
        for slot in 0..MAX_AUDIO_BACKENDS {
            for local in 0..32 {
                let result = output_record(slot, local, &mut record);
                if result < 0 {
                    return result;
                }
                if result == 0 {
                    break;
                }
                let Some(len) = record.id.iter().position(|&b| b == 0) else {
                    return -1;
                };
                if id != &record.id[..len] {
                    continue;
                }
                if record.availability != a::audio_output_available {
                    return NO_ENDPOINT;
                }
                let selected = select_output(slot, local);
                if selected == 0 {
                    g!(initial_output_pending) = false;
                }
                return selected;
            }
        }
        NO_ENDPOINT
    }
}

// Keep the original complete-snapshot selection and success publication scope.
#[allow(clippy::collapsible_if)]
pub(super) unsafe fn resolve_initial_output() {
    unsafe {
        if !g!(initial_output_pending) {
            return;
        }
        let mut best_slot = None;
        let mut best_local = 0;
        let mut best_score = 0;
        let mut record = a::AudioOutputInfo::default();
        for slot in 0..MAX_AUDIO_BACKENDS {
            for local in 0..32 {
                let result = output_record(slot, local, &mut record);
                if result < 0 {
                    return;
                }
                if result == 0 {
                    break;
                }
                if record.availability != a::audio_output_available {
                    continue;
                }
                let p = backend(slot);
                let score = if record.kind == a::audio_output_kind_hdmi {
                    100
                } else if let Some(outputs) = (*p).outputs {
                    if (outputs.active)((*p).config.context as u64) == local as i32 {
                        20
                    } else {
                        1
                    }
                } else {
                    1
                };
                if score <= best_score {
                    continue;
                }
                best_score = score;
                best_slot = Some(slot);
                best_local = local;
            }
        }
        if let Some(slot) = best_slot {
            if select_output(slot, best_local) == 0 {
                g!(initial_output_pending) = false;
            }
        }
    }
}

pub(crate) unsafe fn unregister_audio_backend_by_name(name: &[u8]) -> i32 {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return BUSY;
        };
        let Some(slot) = find_backend(name) else {
            return -1;
        };
        let was_active = g!(active_audio_slot) == Some(slot);
        if was_active {
            let result = stop_active_pcm_owned();
            if result < 0 {
                return result;
            }
        }
        backend(slot).write(AudioBackend::EMPTY);
        if was_active {
            g!(active_audio_slot) = None;
            OUTPUT_PRESENT.store(0, Ordering::Release);
            activate_first_backend();
        }
        boot(b"[AUDIO] audio backend unregistered ");
        boot(name);
        boot(b" [OK]\r\n");
        0
    }
}

pub(crate) unsafe fn unregister_audio_backend_z(name: *const u8) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        unregister_audio_backend_by_name(name)
    }
}

pub(crate) unsafe fn set_audio_backend_closing(name: &[u8], closing: bool) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let p = backend(slot);
        (*p).closing = closing;
        if g!(active_audio_slot) == Some(slot) {
            let c = (*p).config;
            OUTPUT_PRESENT.store(u8::from(!closing && (c.write.is_some())), Ordering::Release);
        }
        true
    }
}

pub(crate) unsafe fn audio_backend_registered(name: &[u8]) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        find_backend(name).is_some()
    }
}

pub(crate) unsafe fn audio_backend_family_status(family: &[u8]) -> BackendFamilyStatus {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return BackendFamilyStatus::default();
        };
        let mut result = BackendFamilyStatus::default();
        for slot in 0..MAX_AUDIO_BACKENDS {
            let p = backend(slot);
            if !(*p).registered {
                continue;
            }
            let name = slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), (*p).name_len);
            if name.len() < family.len() || !name[..family.len()].eq_ignore_ascii_case(family) {
                continue;
            }
            if name.len() != family.len() && name[family.len()] != b'-' {
                continue;
            }
            result.registered = true;
            if (*p).closing {
                continue;
            }
            result.active |= g!(active_audio_slot) == Some(slot) && (*p).active;
            let c = (*p).config;
            result.has_output |= c.write.is_some();
        }
        result
    }
}

pub(crate) unsafe fn audio_backend_active(name: &[u8]) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let p = backend(slot);
        g!(active_audio_slot) == Some(slot) && (*p).active && !(*p).closing
    }
}

pub(crate) unsafe fn audio_backend_has_output(name: &[u8]) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let c = (*backend(slot)).config;
        c.write.is_some()
    }
}

pub(super) unsafe fn register_mixer_backend(name: &[u8], pointer: *const c_void) {
    unsafe {
        store_mixer(name, pointer);
        boot(b"[AUDIO] mixer backend ");
        boot(name);
        boot(b" [OK]\r\n");
    }
}

pub(super) unsafe fn store_mixer(name: &[u8], pointer: *const c_void) {
    unsafe {
        let p = ptr::addr_of_mut!(g!(mixer_backend));
        p.write(NamedBackend {
            active: true,
            pointer,
            ..NamedBackend::EMPTY
        });
        (*p).name_len = if name.len() < MAX_NAME {
            name.len()
        } else {
            MAX_NAME - 1
        };
        if (*p).name_len > 0 {
            ptr::copy_nonoverlapping(
                name.as_ptr(),
                ptr::addr_of_mut!((*p).name).cast(),
                (*p).name_len,
            );
        }
    }
}

pub(super) unsafe fn register_audio_backend_internal(
    name: &[u8],
    config: BackendRegistration,
) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name).or_else(|| free_backend_slot()) else {
            boot(b"[AUDIO][WARN] audio backend registry full\r\n");
            return false;
        };
        let was_active = g!(active_audio_slot) == Some(slot);
        let p = backend(slot);
        if (*p).closing {
            return false;
        }
        let revision = OUTPUT_REVISION.load(Ordering::Relaxed);
        if was_active && revision.checked_add(1).is_none() {
            return false;
        }
        let generation = g!(next_backend_generation);
        let Some(next) = generation.checked_add(1) else {
            return false;
        };
        g!(next_backend_generation) = next;
        p.write(AudioBackend::EMPTY);
        (*p).generation = generation;
        (*p).registered = true;
        (*p).config = config;
        (*p).name_len = if name.len() < MAX_NAME {
            name.len()
        } else {
            MAX_NAME - 1
        };
        let len = (*p).name_len;
        if len > 0 {
            ptr::copy_nonoverlapping(name.as_ptr(), ptr::addr_of_mut!((*p).name).cast(), len);
        }
        boot(b"[AUDIO] audio backend registered ");
        boot(slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), len));
        boot(b" r4d");
        boot(b" [OK]\r\n");
        if was_active || (g!(active_audio_slot).is_none() && (config.write.is_some())) {
            set_active_backend(slot);
        }
        if was_active {
            OUTPUT_REVISION.store(revision + 1, Ordering::Release);
        }
        true
    }
}

pub(super) unsafe fn set_active_backend(slot: usize) {
    unsafe {
        for i in 0..MAX_AUDIO_BACKENDS {
            (*backend(i)).active = false;
        }
        let p = backend(slot);
        (*p).active = true;
        g!(active_audio_slot) = Some(slot);
        let c = (*p).config;
        OUTPUT_PRESENT.store(
            u8::from(!(*p).closing && (c.write.is_some())),
            Ordering::Release,
        );
        boot(b"[AUDIO] active audio backend ");
        boot(slice::from_raw_parts(
            ptr::addr_of!((*p).name).cast(),
            (*p).name_len,
        ));
        boot(b" [OK]\r\n");
    }
}

pub(super) unsafe fn write_active_pcm(
    data: &[u8],
    rate: u32,
    channels: u16,
    format: u16,
) -> Option<i32> {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            return Some(BUSY);
        };
        write_active_pcm_owned(data, rate, channels, format)
    }
}

// Keep the original legacy/contextual/no-endpoint callback branches explicit.
#[allow(clippy::question_mark)]
pub(super) unsafe fn write_active_pcm_owned(
    data: &[u8],
    rate: u32,
    channels: u16,
    format: u16,
) -> Option<i32> {
    unsafe {
        resolve_initial_output();
        let slot = g!(active_audio_slot)?;
        let p = backend(slot);
        if (*p).closing {
            return Some(BUSY);
        }
        let c = (*p).config;
        if c.limits_present && !c.limits.accepts(rate, channels, format) {
            return Some(INVALID);
        }
        let result = if let Some(write) = c.write {
            write(
                c.context,
                data.as_ptr(),
                data.len().try_into().unwrap(),
                rate,
                channels,
                format,
            )
        } else {
            return None;
        };
        if result == 0 {
            schedule_progress(&BACKEND_DUE, 10);
        }
        Some(result)
    }
}

pub(super) unsafe fn finish_synth_output(source: SynthSource) -> i32 {
    unsafe {
        // Avoid touching hardware for other real producers. Check again
        // after output admission because a PCM Open may win that lease.
        {
            let Some(_streams) = StreamLock::acquire() else {
                return BUSY;
            };
            if open_stream_count() != 0
                || match source {
                    SynthSource::Sid => midi_producing(),
                    SynthSource::Midi => sid_producing(),
                }
            {
                return 0;
            }
        }
        let Some(_output) = OutputGuard::acquire(false) else {
            return BUSY;
        };
        {
            let Some(_streams) = StreamLock::acquire() else {
                return BUSY;
            };
            if open_stream_count() != 0
                || match source {
                    SynthSource::Sid => midi_producing(),
                    SynthSource::Midi => sid_producing(),
                }
            {
                return 0;
            }
        }
        let revision = match source {
            SynthSource::Sid => g!(sid_stop_revision),
            SynthSource::Midi => g!(midi_stop_revision),
        };
        if OUTPUT_REVISION.load(Ordering::Relaxed) != revision {
            return 0;
        }
        let Some(slot) = g!(active_audio_slot) else {
            return 0;
        };
        let p = backend(slot);
        let c = (*p).config;
        let target = SidOutputTarget {
            backend: (*p).generation,
            revision: OUTPUT_REVISION.load(Ordering::Relaxed),
            local: if let Some(outputs) = (*p).outputs {
                let active = (outputs.active)(c.context as u64);
                if active < 0 {
                    return active;
                }
                active
            } else {
                0
            },
        };
        let previous = match source {
            SynthSource::Sid => g!(sid_stop_target),
            SynthSource::Midi => g!(midi_stop_target),
        };
        if let Some(previous) = previous {
            if previous != target {
                // Selection/unregister or another completed output end has
                // already retired this exact target. Never idle its successor.
                return 0;
            }
        } else {
            match source {
                SynthSource::Sid => g!(sid_stop_target) = Some(target),
                SynthSource::Midi => g!(midi_stop_target) = Some(target),
            }
        }
        idle_active_pcm_owned()
    }
}

pub(super) unsafe fn idle_active_pcm_owned() -> i32 {
    unsafe {
        let Some(slot) = g!(active_audio_slot) else {
            return 0;
        };
        let p = backend(slot);
        let c = (*p).config;

        let Some(next) = OUTPUT_REVISION.load(Ordering::Relaxed).checked_add(1) else {
            return INVALID;
        };
        let result =
            backend_contract::finish_pcm(EndMode::Idle, (*p).closing, c.context, c.stop, (*p).idle);
        if result == 0 {
            OUTPUT_REVISION.store(next, Ordering::Release);
        }
        result
    }
}

pub(super) unsafe fn stop_active_pcm_owned() -> i32 {
    unsafe {
        let Some(slot) = g!(active_audio_slot) else {
            return 0;
        };
        let p = backend(slot);
        let c = (*p).config;

        let Some(next) = OUTPUT_REVISION.load(Ordering::Relaxed).checked_add(1) else {
            return INVALID;
        };
        let result =
            backend_contract::finish_pcm(EndMode::Stop, (*p).closing, c.context, c.stop, (*p).idle);
        if result == 0 {
            OUTPUT_REVISION.store(next, Ordering::Release);
        }
        result
    }
}

pub(super) unsafe fn activate_first_backend() {
    unsafe {
        for i in 0..MAX_AUDIO_BACKENDS {
            let p = backend(i);
            let c = (*p).config;
            if !(*p).registered || (*p).closing || (c.write.is_none()) {
                continue;
            }
            set_active_backend(i);
            return;
        }
        boot(b"[AUDIO][WARN] no active audio backend\r\n");
    }
}

pub(super) unsafe fn find_backend(name: &[u8]) -> Option<usize> {
    unsafe {
        for i in 0..MAX_AUDIO_BACKENDS {
            let p = backend(i);
            if (*p).registered
                && name_eq(
                    slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), (*p).name_len),
                    name,
                )
            {
                return Some(i);
            }
        }
        None
    }
}

pub(super) unsafe fn free_backend_slot() -> Option<usize> {
    unsafe { (0..MAX_AUDIO_BACKENDS).find(|&i| !(*backend(i)).registered) }
}

pub(super) fn active_output_present() -> bool {
    OUTPUT_PRESENT.load(Ordering::Acquire) != 0
}
