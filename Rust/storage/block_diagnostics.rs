//! Original block labels, bounded watchdog formatting and stalled execution
//! diagnostics. Generic text/incident primitives remain temporary owner hooks.
use crate::{
    memory::owner::Guard,
    platform::timer,
    sched::{sync_locks, sync_types::Text},
    storage::{block_execution, block_io, block_registry},
};
use core::{ffi::c_void, mem::MaybeUninit};
unsafe extern "C" {
    fn r4os_zig_block_text(channel: u8, text: *const u8, len: usize);
    fn r4os_zig_block_decimal(channel: u8, value: u64);
    fn r4os_zig_block_line(channel: u8);
    fn r4os_zig_block_incident_begin() -> u64;
}
fn put(channel: u8, bytes: &[u8]) {
    unsafe {
        r4os_zig_block_text(channel, bytes.as_ptr(), bytes.len());
    }
}
fn raw(channel: u8, text: Text) {
    unsafe {
        r4os_zig_block_text(channel, text.pointer, text.len);
    }
}
fn number(channel: u8, value: u64) {
    unsafe {
        r4os_zig_block_decimal(channel, value);
    }
}
fn line(channel: u8) {
    unsafe {
        r4os_zig_block_line(channel);
    }
}
fn label(category: u8, value: u8) -> &'static [u8] {
    match category {
        0 => match value {
            0 => b"unknown",
            1 => b"ata",
            2 => b"ahci",
            3 => b"nvme",
            4 => b"usb",
            5 => b"ram",
            6 => b"virtio",
            _ => unreachable!(),
        },
        1 => match value {
            0 => b"registered",
            1 => b"active",
            2 => b"busy",
            3 => b"recovering",
            4 => b"failed",
            _ => unreachable!(),
        },
        2 => match value {
            0 => b"none",
            1 => b"busy",
            2 => b"invalid-request",
            3 => b"request-too-large",
            4 => b"out-of-range",
            5 => b"buffer-too-small",
            6 => b"no-writer",
            7 => b"timeout",
            8 => b"backend-read",
            9 => b"backend-write",
            10 => b"backend-flush",
            11 => b"cancelled",
            12 => b"reset",
            13 => b"shutdown",
            _ => unreachable!(),
        },
        3 => match value {
            0 => b"built-in",
            1 => b"preload",
            2 => b"disk",
            _ => unreachable!(),
        },
        4 => match value {
            0 => b"none",
            1 => b"read",
            2 => b"write",
            3 => b"flush",
            _ => unreachable!(),
        },
        5 => match value {
            0 => b"free",
            1 => b"queued",
            2 => b"active",
            3 => b"completed",
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}
/// # Safety
/// Caller supplies original bounded enum scalars and exclusive C Text output;
/// returned bytes are immutable kernel literals with static lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_label(category: u8, value: u8, out: *mut Text) {
    let text = label(category, value);
    unsafe {
        out.write(Text::literal(text));
    }
}
unsafe fn incident(token: *mut u64) {
    if unsafe { token.read() } == 0 {
        unsafe {
            token.write(r4os_zig_block_incident_begin());
        }
    }
}
/// # Safety
/// Exact admitted resident device; token belongs to the reporting caller and
/// retains its generation through every observation slice. No metadata guard
/// survives generic output or incident calls.
pub(crate) unsafe fn lock_report(device: *mut c_void, slices: u32, token: *mut u64) {
    unsafe {
        incident(token);
    }
    let view = unsafe { block_execution::view(device) };
    for channel in [1, 0] {
        put(channel, b"[BLOCK] queue-lock stall dev=");
        raw(channel, view.name);
        put(channel, b" slice=");
        number(channel, u64::from(slices));
        if channel == 1 {
            line(1);
        } else {
            put(0, b"\r\n");
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Snapshot {
    state: u8,
    kind: u8,
    lba: u64,
    sectors: u16,
    submit_tick: u64,
    start_tick: u64,
}
unsafe fn snapshot(view: block_execution::View, index: usize) -> Snapshot {
    assert!(index < 16);
    let p = unsafe { view.slots.add(index) };
    let _owner = Guard::acquire(1);
    unsafe {
        Snapshot {
            state: (*p).state,
            kind: (*p).kind,
            lba: (*p).lba,
            sectors: (*p).sectors,
            submit_tick: (*p).submit_tick,
            start_tick: (*p).start_tick,
        }
    }
}
/// # Safety
/// Exact device remains pinned by the waiting caller. This formatter tries
/// queue admission once and preserves the original marker before that try;
/// output/incident callbacks never retain mutable slot/pool references.
pub(crate) unsafe fn wait_report(device: *mut c_void, id: u64, slices: u32, token: *mut u64) {
    unsafe {
        incident(token);
    }
    let view = unsafe { block_execution::view(device) };
    put(1, b"[BLOCK] stall dev=");
    raw(1, view.name);
    put(1, b" slice=");
    number(1, u64::from(slices));
    line(1);
    put(0, b"[BLOCK] watchdog: stall dev=");
    raw(0, view.name);
    put(0, b" slice=");
    number(0, u64::from(slices));
    put(0, b"\n");
    let locked = unsafe { sync_locks::r4os_rust_sync_mutex(view.mutex, 0, 0) };
    let mut slot = None;
    if locked {
        let index =
            unsafe { crate::storage::block_requests::r4os_rust_block_select(view.slots, 2, id) };
        if index != usize::MAX {
            slot = Some(unsafe { snapshot(view, index) });
        }
    }
    let state = {
        let _owner = Guard::acquire(1);
        unsafe { view.state.state.read() }
    };
    if locked {
        unsafe {
            sync_locks::r4os_rust_sync_mutex(view.mutex, 2, 0);
        }
    }
    put(1, b"[BLOCK] request dev=");
    raw(1, view.name);
    put(1, b" state=");
    put(1, label(1, state));
    if !locked {
        put(1, b" queue_lock=busy");
    } else if let Some(slot) = slot {
        put(1, b" req=");
        put(1, label(4, slot.kind));
        put(1, b" slot=");
        put(1, label(5, slot.state));
        put(1, b" lba=");
        number(1, slot.lba);
        put(1, b" sectors=");
        number(1, u64::from(slot.sectors));
    } else {
        put(1, b" req=unknown");
    }
    line(1);
    put(0, b"[BLOCK] watchdog: request stalled dev=");
    raw(0, view.name);
    put(0, b" driver=");
    raw(0, view.driver);
    put(0, b" state=");
    put(0, label(1, state));
    put(0, b" slice=");
    number(0, u64::from(slices));
    if let Some(slot) = slot {
        put(0, b" req=");
        put(0, label(4, slot.kind));
        put(0, b" slot=");
        put(0, label(5, slot.state));
        put(0, b" lba=");
        number(0, slot.lba);
        put(0, b" sectors=");
        number(0, u64::from(slot.sectors));
        put(0, b" submit_tick=");
        number(0, slot.submit_tick);
        put(0, b" start_tick=");
        number(0, slot.start_tick);
        put(0, b" now=");
        number(0, timer::tick_count());
    } else {
        put(0, b" req=unknown");
    }
    put(0, b"\n");
}
fn stalled(snapshot: Snapshot, now: u64, threshold: u64) -> bool {
    snapshot.state == 2 && now >= snapshot.start_tick && now - snapshot.start_tick > threshold
}
/// # Safety
/// Read-only crash-class observation; metadata is copied under native storage
/// admission before the exact device can retire. No mutable pool loan escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_stalled(now: u64, threshold: u64) -> bool {
    let mut index = 0;
    while index < block_registry::r4os_rust_block_registry_query(1, 0) {
        let _owner = Guard::acquire(1);
        let mut slot = MaybeUninit::uninit();
        if unsafe {
            block_registry::r4os_rust_block_registry_slot(index, 0, 0, true, slot.as_mut_ptr())
        } {
            let slot = unsafe { slot.assume_init() };
            let view = unsafe { block_execution::view(slot.device) };
            for request in 0..16 {
                if stalled(unsafe { snapshot(view, request) }, now, threshold) {
                    return true;
                }
            }
        }
        index += 1;
    }
    false
}
/// # Safety
/// Explicit crash diagnostic output. Each selected device is pinned while its
/// borrowed immutable names are printed; all physical state is copied first.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_dump(now: u64, threshold: u64, diag: bool) {
    let channel = u8::from(diag);
    let mut index = 0;
    while index < block_registry::r4os_rust_block_registry_query(1, 0) {
        let mut pin = MaybeUninit::uninit();
        if unsafe { block_registry::r4os_rust_block_registry_pin(index, pin.as_mut_ptr()) } {
            let mut pin = unsafe { pin.assume_init() };
            let view = unsafe { block_execution::view(pin.device) };
            for i in 0..16 {
                let slot = unsafe { snapshot(view, i) };
                if !stalled(slot, now, threshold) {
                    continue;
                }
                put(
                    channel,
                    if diag {
                        b"[BLOCK] stalled exec dev="
                    } else {
                        b"[BLOCK] stalled execution dev="
                    },
                );
                raw(channel, view.name);
                put(channel, if diag { b" drv=" } else { b" driver=" });
                raw(channel, view.driver);
                put(channel, b" req=");
                put(channel, label(4, slot.kind));
                put(channel, b" lba=");
                number(channel, slot.lba);
                if !diag {
                    put(0, b" sectors=");
                    number(0, u64::from(slot.sectors));
                }
                put(channel, b" active_for=");
                number(channel, now - slot.start_tick);
                if diag {
                    line(1);
                } else {
                    put(0, b" ticks\r\n");
                }
            }
            unsafe {
                block_registry::r4os_rust_block_registry_unpin(&mut pin);
            }
        }
        index += 1;
    }
}
fn probe_failure(reason: &[u8]) -> bool {
    put(0, b"[NVMEIRQPROBE] result=FAILED reason=");
    put(0, reason);
    put(0, b"\r\n");
    false
}
#[repr(C, align(64))]
struct Sector([u8; 4096]);
/// # Safety
/// Existing test-profile-only read-only acceptance. Every candidate is pinned
/// while immutable geometry and stats are projected; direct sector memory is
/// resident on this exact task stack until physical completion is acknowledged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_nvme_probe() -> bool {
    for index in 0..16 {
        let mut pin = MaybeUninit::uninit();
        if !unsafe { block_registry::r4os_rust_block_registry_pin(index, pin.as_mut_ptr()) } {
            continue;
        }
        let mut pin = unsafe { pin.assume_init() };
        let view = unsafe { block_execution::view(pin.device) };
        if view.bus != 3 {
            unsafe {
                block_registry::r4os_rust_block_registry_unpin(&mut pin);
            }
            continue;
        }
        let accepted = unsafe { probe(index, pin.device, view) };
        unsafe {
            block_registry::r4os_rust_block_registry_unpin(&mut pin);
        }
        return accepted;
    }
    probe_failure(b"device-missing")
}
unsafe fn probe(index: usize, device: *mut c_void, view: block_execution::View) -> bool {
    let size = view.io.sector_size as usize;
    if size == 0 || size > 4096 {
        return probe_failure(b"sector-size");
    }
    let mut before = MaybeUninit::uninit();
    unsafe {
        block_execution::stats_snapshot(device, before.as_mut_ptr());
    }
    let before = unsafe { before.assume_init() };
    let mut sector = MaybeUninit::<Sector>::uninit();
    if !unsafe {
        block_io::r4os_rust_block_read(index, 0, 1, sector.as_mut_ptr().cast(), size, true)
    } {
        return probe_failure(b"read");
    }
    let mut after = MaybeUninit::uninit();
    unsafe {
        block_execution::stats_snapshot(device, after.as_mut_ptr());
    }
    let after = unsafe { after.assume_init() };
    let requests = after.worker_requests.wrapping_sub(before.worker_requests);
    let completions = after
        .worker_completions
        .wrapping_sub(before.worker_completions);
    let submissions = after
        .async_submissions
        .wrapping_sub(before.async_submissions);
    let asynchronous = after
        .async_completions
        .wrapping_sub(before.async_completions);
    if requests == 0
        || completions == 0
        || submissions == 0
        || asynchronous == 0
        || after.timeout_failures != before.timeout_failures
    {
        return probe_failure(b"runtime-counters");
    }
    put(0, b"[NVMEIRQPROBE] result=OK worker_requests=");
    number(0, requests);
    put(0, b" worker_completions=");
    number(0, completions);
    put(0, b" async_submissions=");
    number(0, submissions);
    put(0, b" async_completions=");
    number(0, asynchronous);
    put(0, b"\r\n");
    true
}
