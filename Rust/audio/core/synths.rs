//! Exact synths operations of the parent owner; no duplicated state.
use super::*;

fn sid_resumed() {
    SID_PHASE.store(SID_RUNNING, Ordering::Release);
}

pub(crate) unsafe fn configure_sid_model(model: &[u8]) {
    unsafe {
        g!(sid_model_option) = Span {
            pointer: model.as_ptr(),
            len: model.len(),
        };
        apply_configured_sid_model();
    }
}

pub(crate) unsafe fn apply_configured_sid_model() -> i32 {
    unsafe {
        let Some(_execution) = sessions::Execution::acquire(SynthSource::Sid) else {
            return BUSY;
        };
        if let Some(p) = sid_engine() {
            let config = (*p).config;
            if let Some(set_model) = config.sid_model {
                g!(sid_last_result) =
                    sessions::callback(p, || set_model(config.context, sid_model_id()));
                return g!(sid_last_result);
            }
        }
        NO_ENDPOINT
    }
}

pub(crate) unsafe fn sid_model_name_z() -> *const u8 {
    if unsafe { name_eq(g!(sid_model_option).bytes(), b"6581") } {
        c"6581".as_ptr().cast()
    } else {
        c"8580".as_ptr().cast()
    }
}

pub(crate) unsafe fn sid_acquire() -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -1 };
        let c = (*p).config;
        let Some(acquire) = c.sid_acquire else {
            return -3;
        };
        if g!(sid_acquired) {
            return -2;
        }
        let result = sessions::callback(p, || acquire(c.context));
        g!(sid_last_result) = result;
        if result > 0 {
            g!(sid_acquired) = true;
            g!(sid_pcm_pending_len) = 0;
            sid_resumed();
        }
        result
    }
}

pub(crate) unsafe fn sid_write_register(handle: u32, register: u8, value: u8) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(write) = c.sid_write else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        if register >= 25 {
            return -2;
        }
        g!(sid_last_register) = register;
        g!(sid_last_value) = value;
        g!(sid_write_count) += 1;
        g!(sid_last_result) = sessions::callback(p, || {
            write(c.context, handle, register as u32, value as u32)
        });
        if g!(sid_last_result) == 0 {
            sid_resumed();
        }
        g!(sid_last_result)
    }
}

pub(crate) unsafe fn sid_release(handle: u32) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(release) = c.sid_release else {
            return -3;
        };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        let stopped = sid_stop(handle);
        if stopped != 0 {
            return stopped;
        }
        g!(sid_last_result) = sessions::callback(p, || release(c.context, handle));
        if g!(sid_last_result) == 0 {
            g!(sid_acquired) = false;
            g!(sid_pcm_pending_len) = 0;
            sid_resumed();
        }
        g!(sid_last_result)
    }
}

pub(crate) unsafe fn sid_load_data(
    handle: u32,
    load_addr: u16,
    data: *const u8,
    byte_count: u32,
) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(load) = c.sid_load else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        if byte_count == 0 || byte_count > 65_536 {
            return -2;
        }
        g!(sid_last_result) = sessions::callback(p, || {
            load(c.context, handle, load_addr as u32, data, byte_count)
        });
        if g!(sid_last_result) == 0 {
            sid_resumed();
            byte_count.try_into().unwrap()
        } else {
            g!(sid_last_result)
        }
    }
}

pub(crate) unsafe fn sid_init(handle: u32, init_addr: u16, song: u16) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(init) = c.sid_init else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        g!(sid_last_result) =
            sessions::callback(p, || init(c.context, handle, init_addr as u32, song as u32));
        if g!(sid_last_result) == 0 {
            sid_resumed();
        }
        g!(sid_last_result)
    }
}

pub(crate) unsafe fn sid_play_frame(handle: u32, play_addr: u16, frame_hz: u16) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(play) = c.sid_play else { return -3 };
        let Some(render) = c.sid_pcm else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        if g!(sid_pcm_pending_len) != 0 {
            return flush_pending_sid_pcm();
        }
        let Some(callback) = sessions::Callback::enter(p) else {
            return BUSY;
        };
        g!(sid_last_result) = play(c.context, handle, play_addr as u32, frame_hz as u32);
        if g!(sid_last_result) != 0 {
            return g!(sid_last_result);
        }
        sid_resumed();
        let rendered = render(
            c.context,
            handle,
            ptr::addr_of_mut!(g!(sid_pcm_pending)).cast(),
            SID_RENDER_BYTES as u32,
        );
        g!(sid_last_result) = rendered;
        if rendered <= 0
            || rendered > SID_RENDER_BYTES as i32
            || !(rendered as u32).is_multiple_of(pcm::TARGET_FRAME_BYTES as u32)
        {
            g!(sid_pcm_fail) = g!(sid_pcm_fail).wrapping_add(1);
            g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
            return if rendered < 0 { rendered } else { INVALID };
        }
        g!(sid_pcm_pending_len) = rendered as u32;
        g!(sid_pcm_writes) = g!(sid_pcm_writes).wrapping_add(1);
        (*p).renders = (*p).renders.wrapping_add(1);
        drop(callback);
        flush_pending_sid_pcm()
    }
}

pub(crate) unsafe fn sid_stop(handle: u32) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(stop) = c.stop else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        if SID_PHASE.load(Ordering::Acquire) == SID_STOPPED {
            g!(sid_last_result) = 0;
            return 0;
        }
        if SID_PHASE.load(Ordering::Acquire) == SID_RUNNING {
            let revision = OUTPUT_REVISION.load(Ordering::Acquire);
            g!(sid_last_result) = sessions::callback(p, || stop(c.context));
            if g!(sid_last_result) != 0 {
                // The private source has not acknowledged Stop. Its pending
                // rendered PCM and the shared output still belong to it.
                return g!(sid_last_result);
            }
            (*p).stops = (*p).stops.wrapping_add(1);
            g!(sid_pcm_pending_len) = 0;
            g!(sid_stop_target) = None;
            g!(sid_stop_revision) = revision;
            SID_PHASE.store(SID_QUIET, Ordering::Release);
        }
        g!(sid_last_result) = finish_synth_output(SynthSource::Sid);
        if g!(sid_last_result) == 0 {
            SID_PHASE.store(SID_STOPPED, Ordering::Release);
        }
        g!(sid_last_result)
    }
}

pub(super) unsafe fn flush_pending_sid_pcm() -> i32 {
    unsafe {
        let len = g!(sid_pcm_pending_len) as usize;
        if len == 0 {
            return 0;
        }
        let data = slice::from_raw_parts(ptr::addr_of!(g!(sid_pcm_pending)).cast(), len);
        let result = write_active_pcm(data, DEFAULT_RATE, DEFAULT_CHANNELS, FORMAT_S16LE)
            .unwrap_or(NO_ENDPOINT);
        g!(sid_last_result) = result;
        if result == 0 {
            g!(sid_pcm_pending_len) = 0;
            g!(sid_pcm_ok) = g!(sid_pcm_ok).wrapping_add(1);
            g!(total_backend_ok) = g!(total_backend_ok).wrapping_add(1);
        } else if result != BUSY {
            g!(sid_pcm_fail) = g!(sid_pcm_fail).wrapping_add(1);
            g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
        }
        result
    }
}

pub(crate) unsafe fn midi_open_synth(backend_z: *const u8) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(requested) = copy_z(backend_z, &mut buf) else {
            return -1;
        };
        if g!(midi_acquired) {
            return -3;
        }
        let selected = if requested.is_empty() {
            let Some(slot) = find_renderable_synth() else {
                return -4;
            };
            slot
        } else {
            let Some(slot) = find_synth(requested) else {
                return -2;
            };
            if !synth_can_render_midi(slot) {
                return -4;
            }
            slot
        };
        (*synth(selected)).pcm_pending_len = 0;
        g!(active_midi_synth_slot) = Some(selected);
        g!(midi_acquired) = true;
        1
    }
}

pub(crate) unsafe fn midi_send(handle: u32, channel: u8, status: u8, data1: u8, data2: u8) -> i32 {
    unsafe {
        if handle != 1 || !g!(midi_acquired) {
            return -1;
        }
        if channel > 15 {
            return -2;
        }
        if status == 0 {
            let encoded = data1 as u32 | ((data2 as u32) << 8);
            let requested = if encoded == 0 {
                SYNTH_RENDER_MAX_FRAMES
            } else {
                encoded
            };
            if requested > SYNTH_RENDER_MAX_FRAMES {
                return INVALID;
            }
            return render_active_midi(requested);
        }
        let Some(event) = classify_midi_event(channel, status, data1, data2) else {
            return -3;
        };
        g!(midi_last_channel) = channel;
        g!(midi_last_status) = event.normalized_status;
        g!(midi_last_data1) = data1;
        g!(midi_last_data2) = data2;
        g!(midi_last_event) = event.event;
        g!(midi_send_count) += 1;
        send_active_midi(channel, event.normalized_status, data1, data2)
    }
}

pub(crate) unsafe fn midi_close(handle: u32) -> i32 {
    unsafe {
        if handle != 1 || !g!(midi_acquired) {
            return -1;
        }
        if MIDI_PHASE.load(Ordering::Acquire) == SID_RUNNING {
            let revision = OUTPUT_REVISION.load(Ordering::Acquire);
            let stopped = stop_active_midi();
            if stopped != 0 {
                return stopped;
            }
            if let Some(slot) = g!(active_midi_synth_slot) {
                (*synth(slot)).pcm_pending_len = 0;
            }
            g!(midi_stop_target) = None;
            g!(midi_stop_revision) = revision;
            MIDI_PHASE.store(SID_QUIET, Ordering::Release);
        }
        let ended = finish_synth_output(SynthSource::Midi);
        if ended != 0 {
            return ended;
        }
        g!(active_midi_synth_slot) = None;
        g!(midi_acquired) = false;
        0
    }
}

pub(crate) unsafe fn opl3_reset() -> i32 {
    unsafe {
        let mut op = a::AudioOpl3Op::default();
        if has_protocol(b"audio.opl3") {
            dispatch_opl3(a::audio_opl3_op_reset, &mut op);
        }
        let Some(slot) = find_synth(b"OPL3") else {
            return -2;
        };
        let c = (*synth(slot)).config;
        let Some(reset) = c.opl3_reset else { return -3 };
        sessions::callback(synth(slot), || reset(c.context))
    }
}

pub(crate) unsafe fn opl3_write_register(bank: u8, reg: u8, value: u8) -> i32 {
    unsafe {
        if classify_opl3_register(bank, reg, value).is_none() {
            return -1;
        }
        let Some(slot) = find_synth(b"OPL3") else {
            return -2;
        };
        let c = (*synth(slot)).config;
        let Some(write) = c.opl3_write else { return -3 };
        sessions::callback(synth(slot), || write(c.context, bank, reg, value))
    }
}

pub(crate) unsafe fn opl3_render_block() -> i32 {
    unsafe {
        let Some(slot) = find_synth(b"OPL3") else {
            return -2;
        };
        render_synth_pcm(slot, SYNTH_RENDER_MAX_FRAMES)
    }
}

pub(crate) unsafe fn opl3_stop() -> i32 {
    unsafe {
        let Some(slot) = find_synth(b"OPL3") else {
            return -2;
        };
        let p = synth(slot);
        let c = (*p).config;
        if let Some(stop) = c.stop {
            let result = sessions::callback(p, || stop(c.context));
            if result >= 0 {
                (*p).stops += 1;
            }
            return result;
        }
        0
    }
}

pub(super) unsafe fn classify_midi_event(
    channel: u8,
    status: u8,
    data1: u8,
    data2: u8,
) -> Option<a::AudioMidiOp> {
    unsafe {
        if has_protocol(b"audio.midi") {
            let mut op = a::AudioMidiOp {
                channel,
                status,
                data1,
                data2,
                ..Default::default()
            };
            if dispatch_midi(a::audio_midi_op_classify_event, &mut op)
                && op.result == a::audio_midi_result_ok
            {
                g!(midi_r4p_classify) = g!(midi_r4p_classify).wrapping_add(1);
                g!(midi_last_event) = op.event;
                return Some(op);
            }
        }
        g!(midi_missing_required) = g!(midi_missing_required).wrapping_add(1);
        g!(midi_last_result) = -5;
        g!(midi_last_event) = a::audio_midi_event_ignore;
        None
    }
}

pub(super) unsafe fn dispatch_protocol<T>(name: &[u8], opcode: u32, op: *mut T) -> i32 {
    unsafe {
        let input = a::ProtocolBuffer {
            data: op.cast(),
            len: size_of::<T>() as u32,
            capacity: size_of::<T>() as u32,
            flags: 0,
            reserved: 0,
        };
        let mut output = a::ProtocolBuffer::default();
        r4p::r4os_rust_r4p_dispatch(role(name), opcode, &input, &mut output)
    }
}

pub(super) unsafe fn dispatch_midi(opcode: u32, op: *mut a::AudioMidiOp) -> bool {
    unsafe {
        let result = dispatch_protocol(b"audio.midi", opcode, op);
        g!(midi_last_result) = result;
        if result != a::audio_midi_result_ok {
            g!(midi_dispatch_failures) = g!(midi_dispatch_failures).wrapping_add(1);
            return false;
        }
        true
    }
}

pub(super) unsafe fn dispatch_opl3(opcode: u32, op: *mut a::AudioOpl3Op) -> bool {
    unsafe {
        let result = dispatch_protocol(b"audio.opl3", opcode, op);
        g!(opl3_last_result) = result;
        if result != a::audio_opl3_result_ok {
            g!(opl3_dispatch_failures) = g!(opl3_dispatch_failures).wrapping_add(1);
            return false;
        }
        true
    }
}

pub(super) unsafe fn classify_opl3_register(
    bank: u8,
    reg: u8,
    value: u8,
) -> Option<a::AudioOpl3Op> {
    unsafe {
        if has_protocol(b"audio.opl3") {
            let mut op = a::AudioOpl3Op {
                bank,
                register: reg,
                value,
                ..Default::default()
            };
            if dispatch_opl3(a::audio_opl3_op_write_register, &mut op)
                && op.result == a::audio_opl3_result_ok
            {
                g!(opl3_r4p_register) = g!(opl3_r4p_register).wrapping_add(1);
                g!(opl3_last_write_kind) = op.write_kind;
                return Some(op);
            }
        }
        g!(opl3_missing_required) = g!(opl3_missing_required).wrapping_add(1);
        g!(opl3_last_result) = -5;
        g!(opl3_last_write_kind) = a::audio_opl3_write_other;
        None
    }
}

pub(super) unsafe fn render_active_midi(requested: u32) -> i32 {
    unsafe {
        let Some(slot) = g!(active_midi_synth_slot) else {
            return NO_ENDPOINT;
        };
        g!(midi_render_ticks) = g!(midi_render_ticks).wrapping_add(1);
        render_synth_pcm(slot, requested)
    }
}

pub(super) unsafe fn send_active_midi(channel: u8, status: u8, data1: u8, data2: u8) -> i32 {
    unsafe {
        let Some(slot) = g!(active_midi_synth_slot) else {
            return NO_ENDPOINT;
        };
        let p = synth(slot);
        let c = (*p).config;
        if let Some(send) = c.send {
            let result = sessions::callback(p, || send(c.context, channel, status, data1, data2));
            if result >= 0 {
                (*p).sends = (*p).sends.wrapping_add(1);
                MIDI_PHASE.store(SID_RUNNING, Ordering::Release);
            }
            return result;
        }

        NO_ENDPOINT
    }
}

pub(super) unsafe fn stop_active_midi() -> i32 {
    unsafe {
        let Some(slot) = g!(active_midi_synth_slot) else {
            return 0;
        };
        let p = synth(slot);
        let c = (*p).config;
        if let Some(stop) = c.stop {
            let result = sessions::callback(p, || stop(c.context));
            if result >= 0 {
                (*p).stops = (*p).stops.wrapping_add(1);
            }
            return result;
        }

        0
    }
}

pub(super) unsafe fn render_synth_pcm(slot: usize, requested: u32) -> i32 {
    unsafe {
        if requested == 0 || requested > SYNTH_RENDER_MAX_FRAMES {
            return INVALID;
        }
        let p = synth(slot);
        if (*p).pcm_pending_len != 0 {
            return flush_pending_synth_pcm(p);
        }
        let c = (*p).config;
        let Some(render) = c.render_pcm else {
            return NO_ENDPOINT;
        };
        let capacity = requested * pcm::TARGET_FRAME_BYTES as u32;
        let rendered = sessions::callback(p, || {
            render(
                c.context,
                ptr::addr_of_mut!((*p).pcm_pending).cast(),
                capacity,
                DEFAULT_RATE,
                DEFAULT_CHANNELS,
                FORMAT_S16LE,
            )
        });
        if rendered < 0 {
            return rendered;
        }
        if rendered == 0 {
            return 0;
        }
        if rendered != capacity as i32 {
            return INVALID;
        }
        (*p).pcm_pending_len = rendered as u32;
        (*p).renders = (*p).renders.wrapping_add(1);
        if g!(midi_acquired) && g!(active_midi_synth_slot) == Some(slot) {
            MIDI_PHASE.store(SID_RUNNING, Ordering::Release);
        }
        flush_pending_synth_pcm(p)
    }
}

pub(super) unsafe fn flush_pending_synth_pcm(p: *mut SynthEngine) -> i32 {
    unsafe {
        let len = (*p).pcm_pending_len as usize;
        if len == 0 {
            return 0;
        }
        let data = slice::from_raw_parts(ptr::addr_of!((*p).pcm_pending).cast(), len);
        let result = write_active_pcm(data, DEFAULT_RATE, DEFAULT_CHANNELS, FORMAT_S16LE)
            .unwrap_or(NO_ENDPOINT);
        if result == 0 {
            (*p).pcm_pending_len = 0;
            g!(total_backend_ok) = g!(total_backend_ok).wrapping_add(1);
        } else if result != BUSY {
            g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
        }
        result
    }
}

pub(crate) unsafe fn register_external_synth_engine_z(
    name: *const u8,
    config: SynthRegistration,
    driver_owner: u32,
) -> i32 {
    unsafe {
        if driver_owner == 0 {
            return INVALID;
        }
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        register_synth_internal(name, config, driver_owner)
    }
}

pub(super) unsafe fn register_synth_internal(
    name: &[u8],
    config: SynthRegistration,
    driver_owner: u32,
) -> i32 {
    unsafe {
        let Some(_sid) = sessions::Execution::acquire(SynthSource::Sid) else {
            return BUSY;
        };
        let Some(_midi) = sessions::Execution::acquire(SynthSource::Midi) else {
            return BUSY;
        };
        let Some(slot) = find_synth(name).or_else(|| free_synth_slot()) else {
            boot(b"[AUDIO][WARN] synth engine registry full\r\n");
            return -2;
        };
        let p = synth(slot);
        if (*p).closing || sessions::retained(slot) {
            return BUSY;
        }
        let generation = g!(next_synth_generation);
        let Some(next) = generation.checked_add(1) else {
            return -2;
        };
        g!(next_synth_generation) = next;
        let sends = (*p).sends;
        let renders = (*p).renders;
        let stops = (*p).stops;
        p.write(SynthEngine::EMPTY);
        (*p).registered = true;
        (*p).driver_owner = driver_owner;
        (*p).generation = generation;
        (*p).config = config;
        (*p).sends = sends;
        (*p).renders = renders;
        (*p).stops = stops;
        (*p).name_len = if name.len() < MAX_NAME {
            name.len()
        } else {
            MAX_NAME - 1
        };
        let len = (*p).name_len;
        if len > 0 {
            ptr::copy_nonoverlapping(name.as_ptr(), ptr::addr_of_mut!((*p).name).cast(), len);
        }
        if name_eq(name, b"SID") {
            // Replacing even the same registration creates a new source;
            // an earlier provider's private Stop receipt cannot apply.
            sid_resumed();
        }
        boot(b"[AUDIO] synth engine registered ");
        boot(slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), len));
        boot(b" external");
        boot(b" [OK]\r\n");
        SYNTH_REGISTERED.fetch_or(1 << slot, Ordering::Release);
        0
    }
}

pub(super) unsafe fn find_synth(name: &[u8]) -> Option<usize> {
    unsafe {
        for i in 0..MAX_SYNTH_ENGINES {
            let p = synth(i);
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

pub(super) unsafe fn sid_engine() -> Option<*mut SynthEngine> {
    unsafe {
        find_synth(b"SID")
            .filter(|&i| !(*synth(i)).closing)
            .map(|i| synth(i))
    }
}

pub(super) unsafe fn sid_model_id() -> u32 {
    unsafe {
        if name_eq(g!(sid_model_option).bytes(), b"6581") {
            a::audio_sid_model_6581 as u32
        } else {
            a::audio_sid_model_8580 as u32
        }
    }
}

// Keep the original OPL3 then MIDI then registry fallback selection order.
#[allow(clippy::collapsible_if)]
pub(super) unsafe fn find_renderable_synth() -> Option<usize> {
    unsafe {
        if let Some(slot) = find_synth(b"OPL3") {
            if synth_can_render_midi(slot) {
                return Some(slot);
            }
        }
        if let Some(slot) = find_synth(b"MIDI") {
            if synth_can_render_midi(slot) {
                return Some(slot);
            }
        }
        for i in 0..MAX_SYNTH_ENGINES {
            if (*synth(i)).registered && synth_can_render_midi(i) {
                return Some(i);
            }
        }
        None
    }
}

pub(super) unsafe fn synth_can_render_midi(slot: usize) -> bool {
    unsafe {
        let c = (*synth(slot)).config;
        !(*synth(slot)).closing && c.flags & 1 != 0 && (c.send.is_some()) && c.render_pcm.is_some()
    }
}

pub(super) unsafe fn free_synth_slot() -> Option<usize> {
    unsafe { (0..MAX_SYNTH_ENGINES).find(|&i| !(*synth(i)).registered) }
}
