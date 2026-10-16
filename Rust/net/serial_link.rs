//! Sole SerialLink runtime. UART/protocol calls and sleeps retain no state
//! borrow or network token; short metadata updates preserve the original FIFO.
use super::{
    r4p_contract as wire,
    serial_runtime::{Message, MessageQueue, TxDeadline},
    timing,
};
use crate::{
    boot::serial, contract, fs::vfs_types::Bytes, loader::r4p, memory::owner::Guard,
    sched::scheduler,
};
use core::{cell::UnsafeCell, slice};
const COM2: u16 = 0x2f8;
pub const VERSION: u8 = 1;
pub const MAX_PAYLOAD: usize = 256;
pub const HEADER_SIZE: usize = 10;
pub const MAX_FRAME: usize = HEADER_SIZE + MAX_PAYLOAD;
pub const TYPE_DIAG: u8 = 1;
pub const TYPE_MESSAGE: u8 = 2;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub present: bool,
    pub initialized: bool,
    pub port_base: u16,
    pub loopback_tests: u64,
    pub host_tests: u64,
    pub message_tx: u64,
    pub message_rx: u64,
    pub tx_skipped: u64,
    pub tx_frames: u64,
    pub tx_bytes: u64,
    pub rx_frames: u64,
    pub rx_bytes: u64,
    pub polls: u64,
    pub bad_magic: u64,
    pub bad_version: u64,
    pub bad_length: u64,
    pub checksum_errors: u64,
    pub overflows: u64,
    pub timeouts: u64,
    pub r4p_build: u64,
    pub r4p_parse: u64,
    pub r4p_self: u64,
    pub r4p_dispatch_failures: u64,
    pub last_type: u8,
    pub last_payload_len: u16,
    pub last_message_len: u16,
    pub last_payload: [u8; MAX_PAYLOAD],
    pub last_message: [u8; MAX_PAYLOAD],
    pub last_error: [u8; 32],
}
impl Snapshot {
    pub const EMPTY: Self = Self {
        present: false,
        initialized: false,
        port_base: COM2,
        loopback_tests: 0,
        host_tests: 0,
        message_tx: 0,
        message_rx: 0,
        tx_skipped: 0,
        tx_frames: 0,
        tx_bytes: 0,
        rx_frames: 0,
        rx_bytes: 0,
        polls: 0,
        bad_magic: 0,
        bad_version: 0,
        bad_length: 0,
        checksum_errors: 0,
        overflows: 0,
        timeouts: 0,
        r4p_build: 0,
        r4p_parse: 0,
        r4p_self: 0,
        r4p_dispatch_failures: 0,
        last_type: 0,
        last_payload_len: 0,
        last_message_len: 0,
        last_payload: [0; MAX_PAYLOAD],
        last_message: [0; MAX_PAYLOAD],
        last_error: [0; 32],
    };
}
struct State {
    snapshot: Snapshot,
    queue: MessageQueue,
}
struct Storage(UnsafeCell<State>);
// SAFETY: all metadata and queue access holds the common network owner.
// No reference to this storage spans a module callback, UART wait or yield.
unsafe impl Sync for Storage {}
const INITIAL: Snapshot = Snapshot {
    last_error: [
        110, 111, 110, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0,
    ],
    ..Snapshot::EMPTY
};
static STATE: Storage = Storage(UnsafeCell::new(State {
    snapshot: INITIAL,
    queue: MessageQueue::EMPTY,
}));
fn state() -> *mut State {
    STATE.0.get()
}

fn set_error(snapshot: &mut Snapshot, reason: &[u8]) {
    snapshot.last_error.fill(0);
    let len = reason.len().min(snapshot.last_error.len() - 1);
    snapshot.last_error[..len].copy_from_slice(&reason[..len]);
}
fn fail(reason: &[u8]) -> bool {
    let _guard = Guard::acquire(2);
    unsafe { set_error(&mut (*state()).snapshot, reason) };
    false
}
fn role() -> Bytes {
    Bytes {
        data: b"net.serial_link".as_ptr(),
        len: 15,
    }
}
fn active() -> bool {
    unsafe { r4p::r4os_rust_r4p_has_active(role()) }
}
pub(super) fn init() {
    let present = serial::r4os_rust_serial_present(COM2);
    {
        let _guard = Guard::acquire(2);
        unsafe {
            state().write(State {
                snapshot: Snapshot {
                    present,
                    initialized: present,
                    ..Snapshot::EMPTY
                },
                queue: MessageQueue::EMPTY,
            });
            set_error(
                &mut (*state()).snapshot,
                if present { b"none" } else { b"com2-missing" },
            );
        }
    }
    if present {
        serial::r4os_rust_serial_init(COM2);
    }
}
pub(super) fn snapshot() -> Snapshot {
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &(*state()).snapshot;
        let mut out = *s;
        out.last_payload.fill(0);
        out.last_message.fill(0);
        let payload = usize::from(s.last_payload_len);
        let message = usize::from(s.last_message_len);
        out.last_payload[..payload].copy_from_slice(&s.last_payload[..payload]);
        out.last_message[..message].copy_from_slice(&s.last_message[..message]);
        out
    }
}
pub(super) fn take_message() -> Option<Message> {
    let _guard = Guard::acquire(2);
    unsafe { (*state()).queue.pop() }
}
pub(super) fn poll() {
    {
        let _guard = Guard::acquire(2);
        unsafe {
            let s = &mut (*state()).snapshot;
            if !s.present || !s.initialized {
                return;
            }
            s.polls += 1;
        }
    }
    for _ in 0..256 {
        let byte = serial::r4os_rust_serial_getc(COM2);
        if byte < 0 {
            return;
        }
        parse_byte(byte as u8);
    }
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &mut (*state()).snapshot;
        s.timeouts += 1;
        set_error(s, b"poll-budget");
    }
}
pub(super) fn send_frame(frame_type: u8, bytes: &[u8]) -> bool {
    let (present, initialized) = {
        let _guard = Guard::acquire(2);
        unsafe { ((*state()).snapshot.present, (*state()).snapshot.initialized) }
    };
    if !present {
        return fail(b"com2-missing");
    }
    if !initialized {
        return fail(b"not-initialized");
    }
    let mut frame = [0u8; MAX_FRAME];
    let Some(len) = build_frame_path(frame_type, bytes, &mut frame) else {
        let _guard = Guard::acquire(2);
        unsafe {
            let s = &mut (*state()).snapshot;
            s.bad_length += 1;
            set_error(s, b"tx-length");
        }
        return false;
    };
    let written = write_frame(&frame[..len]);
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &mut (*state()).snapshot;
        if written != len {
            s.tx_skipped = s.tx_skipped.wrapping_add((len - written) as u64);
            s.timeouts = s.timeouts.wrapping_add(1);
            set_error(s, b"tx-timeout");
            return false;
        }
        s.tx_frames = s.tx_frames.wrapping_add(1);
        set_error(s, b"none");
        true
    }
}
pub(super) fn send_message(bytes: &[u8]) -> bool {
    if !send_frame(TYPE_MESSAGE, bytes) {
        return false;
    }
    let _guard = Guard::acquire(2);
    unsafe {
        (*state()).snapshot.message_tx += 1;
    }
    true
}
fn write_frame(frame: &[u8]) -> usize {
    let deadline = TxDeadline {
        start_tick: timing::now_ticks(),
        timeout_ticks: timing::ms_to_ticks(250).max(1),
    };
    let mut written = 0;
    while written < frame.len() {
        let progress = unsafe {
            serial::r4os_rust_serial_available(
                COM2,
                frame[written..].as_ptr(),
                frame.len() - written,
            )
        };
        if progress != 0 {
            written += progress;
            continue;
        }
        if deadline.expired(timing::now_ticks()) {
            break;
        }
        unsafe { scheduler::sleep(1, b"serial-link-tx-wait".as_ptr(), 19) };
    }
    let _guard = Guard::acquire(2);
    unsafe {
        (*state()).snapshot.tx_bytes = (*state()).snapshot.tx_bytes.wrapping_add(written as u64);
    }
    written
}
fn parse_byte(byte: u8) {
    if !active() {
        let _guard = Guard::acquire(2);
        unsafe {
            let s = &mut (*state()).snapshot;
            s.r4p_dispatch_failures += 1;
            set_error(s, b"r4p-required");
        }
        return;
    }
    if !parse_byte_r4p(byte) {
        fail(b"r4p-dispatch");
    }
}
fn new_operation() -> Option<wire::R4slOp> {
    if active() {
        Some(wire::R4slOp::default())
    } else {
        None
    }
}
fn dispatch(opcode: u32, op: &mut wire::R4slOp) -> bool {
    let mut buffer = contract::ProtocolBuffer {
        data: core::ptr::from_mut(op).cast(),
        len: core::mem::size_of::<wire::R4slOp>() as u32,
        capacity: core::mem::size_of::<wire::R4slOp>() as u32,
        flags: 0,
        reserved: 0,
    };
    let result =
        unsafe { r4p::r4os_rust_r4p_dispatch(role(), opcode, &raw const buffer, &raw mut buffer) };
    if result == -5 || (result == -4 && op.result == 0) {
        let _guard = Guard::acquire(2);
        unsafe {
            (*state()).snapshot.r4p_dispatch_failures += 1;
        }
        return false;
    }
    true
}
fn build_frame_path(frame_type: u8, bytes: &[u8], out: &mut [u8]) -> Option<usize> {
    let Some(mut op) = new_operation() else {
        let _guard = Guard::acquire(2);
        unsafe {
            let s = &mut (*state()).snapshot;
            s.r4p_dispatch_failures += 1;
            set_error(s, b"r4p-required");
        }
        return None;
    };
    if bytes.len() > op.payload.len() {
        let _guard = Guard::acquire(2);
        unsafe {
            let s = &mut (*state()).snapshot;
            s.bad_length += 1;
            set_error(s, b"payload-size");
        }
        return None;
    }
    op.frame_type = frame_type;
    op.payload_len = bytes.len() as u32;
    op.payload[..bytes.len()].copy_from_slice(bytes);
    if !dispatch(wire::R4SL_OP_BUILD_FRAME, &mut op)
        || op.result != wire::R4SL_RESULT_OK
        || op.frame_len as usize > out.len()
    {
        let _guard = Guard::acquire(2);
        unsafe {
            let s = &mut (*state()).snapshot;
            s.r4p_dispatch_failures += 1;
            set_error(s, b"r4p-build");
        }
        return None;
    }
    let len = op.frame_len as usize;
    out[..len].copy_from_slice(&op.frame[..len]);
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &mut (*state()).snapshot;
        s.r4p_build += 1;
        set_error(s, b"none");
    }
    Some(len)
}
fn parse_byte_r4p(byte: u8) -> bool {
    let Some(mut op) = new_operation() else {
        return false;
    };
    op.input[0] = byte;
    op.input_len = 1;
    if !dispatch(wire::R4SL_OP_PARSE_BYTES, &mut op) {
        return false;
    }
    match op.result {
        wire::R4SL_RESULT_NEED_MORE => true,
        wire::R4SL_RESULT_OK => {
            if op.completed != 0 {
                apply_frame(&op);
                let _guard = Guard::acquire(2);
                unsafe {
                    (*state()).snapshot.r4p_parse += 1;
                }
            }
            true
        }
        other => {
            let _guard = Guard::acquire(2);
            unsafe {
                let s = &mut (*state()).snapshot;
                match other {
                    wire::R4SL_RESULT_BAD_MAGIC => {
                        s.bad_magic += 1;
                        set_error(s, b"bad-magic");
                    }
                    wire::R4SL_RESULT_BAD_VERSION => {
                        s.bad_version += 1;
                        set_error(s, b"bad-version");
                    }
                    wire::R4SL_RESULT_BAD_LENGTH => {
                        s.bad_length += 1;
                        set_error(s, b"bad-length");
                    }
                    wire::R4SL_RESULT_CHECKSUM => {
                        s.checksum_errors += 1;
                        set_error(s, b"checksum");
                    }
                    wire::R4SL_RESULT_OVERFLOW => {
                        s.overflows += 1;
                        set_error(s, b"overflow");
                    }
                    _ => {
                        s.r4p_dispatch_failures += 1;
                        return false;
                    }
                }
            }
            true
        }
    }
}
fn apply_frame(op: &wire::R4slOp) {
    let len = op.payload_len as usize;
    let _guard = Guard::acquire(2);
    unsafe {
        let s = state();
        let value = &mut (*s).snapshot;
        value.rx_frames += 1;
        value.rx_bytes += len as u64;
        value.last_type = op.frame_type;
        value.last_payload_len = u16::try_from(len).expect("R4SL payload");
        value.last_payload[..len].copy_from_slice(&op.payload[..len]);
        if op.frame_type == TYPE_MESSAGE {
            value.message_rx = value.message_rx.wrapping_add(1);
            value.last_message_len = len as u16;
            value.last_message[..len].copy_from_slice(&op.payload[..len]);
            if !(*s).queue.push(&op.payload[..len]) {
                value.overflows = value.overflows.wrapping_add(1);
                set_error(value, b"message-queue-full");
                return;
            }
        }
        set_error(value, b"none");
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_link_init() {
    init();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_serial_link_poll() {
    poll();
}
/// # Safety
/// Readable bounded payload; normal original caller/module admission applies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_serial_link_send_frame(
    kind: u8,
    data: *const u8,
    len: usize,
) -> bool {
    send_frame(
        kind,
        if len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(data, len) }
        },
    )
}
/// # Safety
/// Readable bounded payload; normal original caller/module admission applies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_serial_link_send_message(data: *const u8, len: usize) -> bool {
    send_message(if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, len) }
    })
}
/// # Safety
/// Exact exclusive paired snapshot output; no resident pointer escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_serial_link_snapshot(out: *mut Snapshot) {
    unsafe { out.write(snapshot()) };
}
/// # Safety
/// Exact exclusive paired message output, distinct from resident queue storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_serial_link_message(out: *mut Message) -> bool {
    unsafe {
        if let Some(value) = take_message() {
            out.write(value);
            true
        } else {
            out.write(Message::EMPTY);
            false
        }
    }
}

const _: () = {
    assert!(core::mem::size_of::<Snapshot>() == 720);
    assert!(core::mem::align_of::<Snapshot>() == 8);
    assert!(core::mem::offset_of!(Snapshot, present) == 0);
    assert!(core::mem::offset_of!(Snapshot, initialized) == 1);
    assert!(core::mem::offset_of!(Snapshot, port_base) == 2);
    assert!(core::mem::offset_of!(Snapshot, loopback_tests) == 8);
    assert!(core::mem::offset_of!(Snapshot, host_tests) == 16);
    assert!(core::mem::offset_of!(Snapshot, message_tx) == 24);
    assert!(core::mem::offset_of!(Snapshot, message_rx) == 32);
    assert!(core::mem::offset_of!(Snapshot, tx_skipped) == 40);
    assert!(core::mem::offset_of!(Snapshot, tx_frames) == 48);
    assert!(core::mem::offset_of!(Snapshot, tx_bytes) == 56);
    assert!(core::mem::offset_of!(Snapshot, rx_frames) == 64);
    assert!(core::mem::offset_of!(Snapshot, rx_bytes) == 72);
    assert!(core::mem::offset_of!(Snapshot, polls) == 80);
    assert!(core::mem::offset_of!(Snapshot, bad_magic) == 88);
    assert!(core::mem::offset_of!(Snapshot, bad_version) == 96);
    assert!(core::mem::offset_of!(Snapshot, bad_length) == 104);
    assert!(core::mem::offset_of!(Snapshot, checksum_errors) == 112);
    assert!(core::mem::offset_of!(Snapshot, overflows) == 120);
    assert!(core::mem::offset_of!(Snapshot, timeouts) == 128);
    assert!(core::mem::offset_of!(Snapshot, r4p_build) == 136);
    assert!(core::mem::offset_of!(Snapshot, r4p_parse) == 144);
    assert!(core::mem::offset_of!(Snapshot, r4p_self) == 152);
    assert!(core::mem::offset_of!(Snapshot, r4p_dispatch_failures) == 160);
    assert!(core::mem::offset_of!(Snapshot, last_type) == 168);
    assert!(core::mem::offset_of!(Snapshot, last_payload_len) == 170);
    assert!(core::mem::offset_of!(Snapshot, last_message_len) == 172);
    assert!(core::mem::offset_of!(Snapshot, last_payload) == 174);
    assert!(core::mem::offset_of!(Snapshot, last_message) == 430);
    assert!(core::mem::offset_of!(Snapshot, last_error) == 686);
};
