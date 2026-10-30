//! Exclusive synth sessions and provider lifetime; PCM keeps its own owners.
use super::*;
use crate::driver::api_admission;
use core::mem::MaybeUninit;

#[derive(Clone, Copy)]
pub(super) struct Session {
    owner: Owner,
    slot: usize,
    generation: u64,
    active: bool,
}
impl Session {
    pub(super) const EMPTY: Self = Self {
        owner: Owner::KERNEL,
        slot: 0,
        generation: 0,
        active: false,
    };
    unsafe fn matches(self, owner: Owner) -> bool {
        unsafe {
            self.active
                && owner.valid()
                && mixer::same_owner(self.owner, owner)
                && (*synth(self.slot)).registered
                && !(*synth(self.slot)).closing
                && (*synth(self.slot)).generation == self.generation
        }
    }
}

const fn execution(name: &'static [u8]) -> UnwindGuard {
    UnwindGuard {
        name: Text::literal(name),
        ..UnwindGuard::EMPTY
    }
}
static SID_EXECUTION: Resident<UnwindGuard> = Resident(UnsafeCell::new(execution(b"audio-sid")));
static MIDI_EXECUTION: Resident<UnwindGuard> =
    Resident(UnsafeCell::new(execution(b"audio-midi-opl3")));

pub(super) struct Execution(*mut UnwindGuard);
impl Execution {
    pub(super) unsafe fn acquire(source: SynthSource) -> Option<Self> {
        let p = match source {
            SynthSource::Sid => SID_EXECUTION.0.get(),
            SynthSource::Midi => MIDI_EXECUTION.0.get(),
        };
        unsafe {
            if !sync_locks::r4os_rust_sync_guard(p, 2, 0) {
                return None;
            }
            if (*p).depth != 1 {
                sync_locks::r4os_rust_sync_guard(p, 3, 0);
                return None;
            }
        }
        Some(Self(p))
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        assert!(unsafe { sync_locks::r4os_rust_sync_guard(self.0, 3, 0) });
    }
}

// This token binds DriverApi context only. The execution/closing barriers
// above retain the provider. Never keep it across a different PCM provider.
pub(super) struct Callback(Option<api_admission::CallbackToken>);
impl Callback {
    pub(super) unsafe fn enter(p: *mut SynthEngine) -> Option<Self> {
        if unsafe { !(*p).registered || (*p).closing } {
            return None;
        }
        let owner = unsafe { (*p).driver_owner };
        if owner == 0 {
            return Some(Self(None));
        }
        let mut token = MaybeUninit::uninit();
        if unsafe { api_admission::r4os_rust_driver_api_storage_enter(owner, token.as_mut_ptr()) } {
            Some(Self(Some(unsafe { token.assume_init() })))
        } else {
            None
        }
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        if let Some(token) = &self.0 {
            unsafe { api_admission::r4os_rust_driver_api_storage_leave(token) }
        }
    }
}
pub(super) unsafe fn callback(p: *mut SynthEngine, f: impl FnOnce() -> i32) -> i32 {
    let Some(_context) = (unsafe { Callback::enter(p) }) else {
        return BUSY;
    };
    f()
}

pub(super) fn sid_producing() -> bool {
    SID_ACTIVE.load(Ordering::Acquire) != 0 && SID_PHASE.load(Ordering::Acquire) == SID_RUNNING
}
pub(super) fn midi_producing() -> bool {
    MIDI_ACTIVE.load(Ordering::Acquire) != 0 && MIDI_PHASE.load(Ordering::Acquire) == SID_RUNNING
}

pub(crate) unsafe fn sid_acquire_for_owner(owner: Owner) -> i32 {
    unsafe {
        if !owner.valid() {
            return INVALID;
        }
        let Some(_execution) = Execution::acquire(SynthSource::Sid) else {
            return BUSY;
        };
        let result = synths::sid_acquire();
        if result > 0 {
            let slot = synths::find_synth(b"SID").expect("acquired SID registration");
            g!(sid_session) = Session {
                owner,
                slot,
                generation: (*synth(slot)).generation,
                active: true,
            };
            SID_ACTIVE.store(1, Ordering::Release);
        }
        result
    }
}

macro_rules! sid_call {
    ($plain:ident, $owned:ident $(, $arg:ident: $ty:ty)*) => {
        pub(crate) unsafe fn $owned(owner: Owner, $($arg: $ty),*) -> i32 {
            unsafe {
                let Some(_execution) = Execution::acquire(SynthSource::Sid) else { return BUSY };
                if !g!(sid_session).matches(owner) { return INVALID; }
                synths::$plain($($arg),*)
            }
        }
    };
}
sid_call!(sid_write_register, sid_write_register_for_owner, handle: u32, register: u8, value: u8);
sid_call!(sid_load_data, sid_load_data_for_owner, handle: u32, load_addr: u16, data: *const u8, byte_count: u32);
sid_call!(sid_init, sid_init_for_owner, handle: u32, init_addr: u16, song: u16);
sid_call!(sid_play_frame, sid_play_frame_for_owner, handle: u32, play_addr: u16, frame_hz: u16);
sid_call!(sid_stop, sid_stop_for_owner, handle: u32);

pub(crate) unsafe fn sid_release_for_owner(owner: Owner, handle: u32) -> i32 {
    unsafe {
        let Some(_execution) = Execution::acquire(SynthSource::Sid) else {
            return BUSY;
        };
        if !g!(sid_session).matches(owner) {
            return INVALID;
        }
        release_sid(handle)
    }
}
unsafe fn release_sid(handle: u32) -> i32 {
    unsafe {
        let result = synths::sid_release(handle);
        if result == 0 {
            g!(sid_session) = Session::EMPTY;
            SID_ACTIVE.store(0, Ordering::Release);
        }
        result
    }
}

pub(crate) unsafe fn midi_open_synth_for_owner(owner: Owner, name: *const u8) -> i32 {
    unsafe {
        if !owner.valid() {
            return INVALID;
        }
        let Some(_execution) = Execution::acquire(SynthSource::Midi) else {
            return BUSY;
        };
        let result = synths::midi_open_synth(name);
        if result > 0 {
            let slot = g!(active_midi_synth_slot).expect("acquired MIDI registration");
            g!(midi_session) = Session {
                owner,
                slot,
                generation: (*synth(slot)).generation,
                active: true,
            };
            MIDI_PHASE.store(SID_RUNNING, Ordering::Release);
            MIDI_ACTIVE.store(1, Ordering::Release);
        }
        result
    }
}
pub(crate) unsafe fn midi_send_for_owner(
    owner: Owner,
    handle: u32,
    channel: u8,
    status: u8,
    data1: u8,
    data2: u8,
) -> i32 {
    unsafe {
        let Some(_execution) = Execution::acquire(SynthSource::Midi) else {
            return BUSY;
        };
        if !g!(midi_session).matches(owner) {
            return INVALID;
        }
        synths::midi_send(handle, channel, status, data1, data2)
    }
}
pub(crate) unsafe fn midi_close_for_owner(owner: Owner, handle: u32) -> i32 {
    unsafe {
        let Some(_execution) = Execution::acquire(SynthSource::Midi) else {
            return BUSY;
        };
        if !g!(midi_session).matches(owner) {
            return INVALID;
        }
        release_midi(handle)
    }
}
unsafe fn release_midi(handle: u32) -> i32 {
    unsafe {
        let result = synths::midi_close(handle);
        if result == 0 {
            g!(midi_session) = Session::EMPTY;
            MIDI_ACTIVE.store(0, Ordering::Release);
        }
        result
    }
}

macro_rules! global_opl3 {
    ($name:ident $(, $arg:ident: $ty:ty)*) => {
        pub(crate) unsafe fn $name($($arg: $ty),*) -> i32 {
            unsafe {
                let Some(_execution) = Execution::acquire(SynthSource::Midi) else { return BUSY };
                if let Some(slot) = synths::find_synth(b"OPL3") {
                    if (*synth(slot)).closing { return BUSY; }
                }
                synths::$name($($arg),*)
            }
        }
    };
}
global_opl3!(opl3_reset);
global_opl3!(opl3_stop);
global_opl3!(opl3_write_register, bank: u8, register: u8, value: u8);
global_opl3!(opl3_render_block);

pub(crate) unsafe fn close_synths_for_owner(owner: Owner) -> bool {
    unsafe {
        if !owner.valid() || mixer::same_owner(owner, Owner::KERNEL) {
            return false;
        }
        {
            let Some(_execution) = Execution::acquire(SynthSource::Sid) else {
                return false;
            };
            let session = g!(sid_session);
            if session.active
                && mixer::same_owner(session.owner, owner)
                && (!session.matches(owner) || release_sid(1) != 0)
            {
                return false;
            }
        }
        {
            let Some(_execution) = Execution::acquire(SynthSource::Midi) else {
                return false;
            };
            let session = g!(midi_session);
            if session.active
                && mixer::same_owner(session.owner, owner)
                && (!session.matches(owner) || release_midi(1) != 0)
            {
                return false;
            }
        }
        true
    }
}

pub(super) unsafe fn retained(slot: usize) -> bool {
    unsafe {
        [g!(sid_session), g!(midi_session)]
            .iter()
            .any(|s| s.active && s.slot == slot)
    }
}
pub(crate) unsafe fn set_synth_driver_closing(owner: u32, closing: bool) -> bool {
    unsafe {
        let Some(_sid) = Execution::acquire(SynthSource::Sid) else {
            return false;
        };
        let Some(_midi) = Execution::acquire(SynthSource::Midi) else {
            return false;
        };
        if closing
            && (0..MAX_SYNTH_ENGINES)
                .any(|i| (*synth(i)).registered && (*synth(i)).driver_owner == owner && retained(i))
        {
            return false;
        }
        for i in 0..MAX_SYNTH_ENGINES {
            let p = synth(i);
            if (*p).registered && (*p).driver_owner == owner {
                (*p).closing = closing;
            }
        }
        true
    }
}
pub(crate) unsafe fn unregister_synth_driver(owner: u32) -> bool {
    unsafe {
        let Some(_sid) = Execution::acquire(SynthSource::Sid) else {
            return false;
        };
        let Some(_midi) = Execution::acquire(SynthSource::Midi) else {
            return false;
        };
        if (0..MAX_SYNTH_ENGINES)
            .any(|i| (*synth(i)).registered && (*synth(i)).driver_owner == owner && retained(i))
        {
            return false;
        }
        for i in 0..MAX_SYNTH_ENGINES {
            let p = synth(i);
            if (*p).registered && (*p).driver_owner == owner {
                p.write(SynthEngine::EMPTY);
                SYNTH_REGISTERED.fetch_and(!(1 << i), Ordering::Release);
            }
        }
        true
    }
}
