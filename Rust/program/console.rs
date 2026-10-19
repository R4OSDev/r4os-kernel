//! Console backing blocks, transcript references and output routing. Sealed
//! producer blocks preserve both visible text and retained completion output.
//! The original output mutex, counters and all per-reference acknowledgments
//! have one native owner; no payload borrow survives a yield or allocation.
use super::{payload, payload_types as t, process, registry, registry_types::*};
use crate::{
    contract,
    fs::vfs_types::Bytes,
    memory::heap,
    sched::{
        sync_locks,
        sync_types::{Mutex, Text},
    },
};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr, slice,
    sync::atomic::{AtomicBool, Ordering},
};
const CAPACITY: usize = 16384;
#[repr(C)]
pub struct State {
    pub write_calls: u64,
    pub source_bytes: u64,
    pub visible_append_bytes: u64,
    pub capture_append_bytes: u64,
    pub shared_bytes: u64,
    pub revision_batches: u64,
    pub desktop_signals: u64,
    pub compactions: u64,
    pub compaction_bytes: u64,
    pub segment_drops: u64,
    pub segment_drop_bytes: u64,
    pub lock: Mutex,
}
struct Storage(UnsafeCell<State>);
// SAFETY: the same output mutex and original caller admission guard access.
// Diagnostic reads retain the original cooperative snapshot semantics.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    write_calls: 0,
    source_bytes: 0,
    visible_append_bytes: 0,
    capture_append_bytes: 0,
    shared_bytes: 0,
    revision_batches: 0,
    desktop_signals: 0,
    compactions: 0,
    compaction_bytes: 0,
    segment_drops: 0,
    segment_drop_bytes: 0,
    lock: Mutex {
        rank: 100,
        name: Text::literal(b"console-output"),
        ..Mutex::EMPTY
    },
}));
fn state() -> *mut State {
    STATE.0.get()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_state() -> *mut State {
    state()
}

pub(super) struct OutputGuard;
impl OutputGuard {
    pub(super) fn acquire() -> Option<Self> {
        if unsafe {
            sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*state()).lock), 1, u64::MAX)
        } {
            Some(Self)
        } else {
            None
        }
    }
}
impl Drop for OutputGuard {
    fn drop(&mut self) {
        unsafe { sync_locks::r4os_rust_sync_mutex(ptr::addr_of_mut!((*state()).lock), 2, 0) };
    }
}
pub(super) struct Lease(pub(super) ProgramInstanceLease);
impl Lease {
    pub(super) unsafe fn id(id: u32) -> Option<Self> {
        let mut out = MaybeUninit::uninit();
        if unsafe { registry::r4os_rust_program_registry_pin_id(id, out.as_mut_ptr()) } {
            Some(Self(unsafe { out.assume_init() }))
        } else {
            None
        }
    }
    pub(super) unsafe fn handle(handle: contract::ProgramProcessHandle) -> Option<Self> {
        let mut out = MaybeUninit::uninit();
        if unsafe { registry::r4os_rust_program_registry_pin(handle, false, out.as_mut_ptr()) } {
            Some(Self(unsafe { out.assume_init() }))
        } else {
            None
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        unsafe { registry::unpin(&self.0) };
    }
}
unsafe fn bytes<'a>(value: Bytes) -> &'a [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value.data, value.len) }
    }
}
macro_rules! add {
    ($field:ident,$amount:expr) => {
        (*state()).$field = (*state()).$field.wrapping_add($amount);
    };
}
fn storage_stat(offset: usize) {
    payload::add(offset, 8, 1);
}
fn header_error() {
    storage_stat(mem::offset_of!(t::Stats, header_errors));
}
pub(super) unsafe fn output_valid(output: *const t::ConsoleOutput, owner: u32) -> bool {
    unsafe {
        if !payload::valid(
            ptr::addr_of!((*output).header),
            owner,
            5,
            mem::size_of::<t::ConsoleOutput>(),
        ) {
            return false;
        }
        if (*output).ref_count == 0
            || (*output).active_ref_count > (*output).ref_count
            || (*output).reserved.iter().any(|&b| b != 0)
        {
            header_error();
            return false;
        }
        true
    }
}
pub(super) unsafe fn allocate_output(owner: u32) -> *mut t::ConsoleOutput {
    unsafe {
        let output =
            payload::r4os_rust_program_payload_allocate(5, owner, 5).cast::<t::ConsoleOutput>();
        if output.is_null() {
            return output;
        }
        (*output).ref_count = 1;
        (*output).active_ref_count = 1;
        (*output).active_accounted =
            registry::r4os_rust_program_registry_predicate(4, owner, ptr::null());
        output
    }
}
unsafe fn retain(output: *mut t::ConsoleOutput, active: bool) -> bool {
    unsafe {
        if (*output).ref_count == u32::MAX || (active && (*output).active_ref_count == u32::MAX) {
            return false;
        }
        (*output).ref_count += 1;
        if active {
            if (*output).active_ref_count == 0 && !(*output).active_accounted {
                payload::note_active(mem::size_of::<t::ConsoleOutput>(), false);
                (*output).active_accounted = true;
            }
            (*output).active_ref_count += 1;
        }
        true
    }
}
pub(super) unsafe fn inactive(output: *mut t::ConsoleOutput) -> bool {
    unsafe {
        if (*output).active_ref_count == 0 {
            return false;
        }
        (*output).active_ref_count -= 1;
        if (*output).active_ref_count == 0 && (*output).active_accounted {
            payload::note_active(mem::size_of::<t::ConsoleOutput>(), true);
            (*output).active_accounted = false;
        }
        true
    }
}
unsafe fn free_output(output: *mut t::ConsoleOutput) -> bool {
    unsafe {
        let owner = (*output).header.owner_id;
        if (*output).ref_count != 0
            || (*output).active_ref_count != 0
            || (*output).active_accounted
            || !payload::valid(
                ptr::addr_of!((*output).header),
                owner,
                5,
                mem::size_of::<t::ConsoleOutput>(),
            )
        {
            payload::quarantine(5, mem::size_of::<t::ConsoleOutput>());
            return false;
        }
        if heap::r4os_rust_heap_free(output.cast(), mem::size_of::<t::ConsoleOutput>()) != 0 {
            storage_stat(mem::offset_of!(t::Stats, free_failures));
            payload::quarantine(5, mem::size_of::<t::ConsoleOutput>());
            return false;
        }
        storage_stat(mem::offset_of!(t::Stats, payload_releases));
        payload::note_release(5, mem::size_of::<t::ConsoleOutput>());
        true
    }
}
pub(super) unsafe fn release(output: *mut t::ConsoleOutput, active: bool) -> bool {
    unsafe {
        if (*output).ref_count == 0 || (active && !inactive(output)) {
            return false;
        }
        (*output).ref_count -= 1;
        if (*output).ref_count != 0 {
            return true;
        }
        if (*output).active_ref_count != 0 {
            header_error();
            return false;
        }
        free_output(output)
    }
}
const EMPTY_SEGMENT: t::ConsoleSegment = t::ConsoleSegment {
    payload: ptr::null_mut(),
    start_sequence: 0,
    length: 0,
    reserved: 0,
};
pub(super) unsafe fn release_segments(transcript: *mut t::ConsoleTranscript, active: bool) -> bool {
    unsafe {
        let mut released = true;
        let mut index = 0;
        while index < (*transcript).segment_count as usize {
            let slot = ptr::addr_of_mut!((*transcript).segments[index]);
            let output = (*slot).payload;
            index += 1;
            if output.is_null() {
                released = false;
                continue;
            }
            slot.write(EMPTY_SEGMENT);
            if !release(output, active) {
                released = false;
            }
        }
        (*transcript).segment_count = 0;
        (*transcript).output_len = 0;
        released
    }
}
pub(super) unsafe fn transcript_inactive(transcript: *mut t::ConsoleTranscript) -> bool {
    unsafe {
        let mut converted = true;
        let mut index = 0;
        while index < (*transcript).segment_count as usize {
            let output = (*transcript).segments[index].payload;
            index += 1;
            if output.is_null() {
                converted = false;
                continue;
            }
            if !inactive(output) {
                converted = false;
            }
        }
        converted
    }
}
pub(super) unsafe fn transcript_valid(transcript: *const t::ConsoleTranscript, owner: u32) -> bool {
    unsafe {
        if !payload::valid(
            ptr::addr_of!((*transcript).header),
            owner,
            11,
            mem::size_of::<t::ConsoleTranscript>(),
        ) || (*transcript).segment_count > 256
            || (*transcript).output_len > CAPACITY as u32
        {
            header_error();
            return false;
        }
        let mut total = 0u64;
        for index in 0..(*transcript).segment_count as usize {
            let segment = (*transcript).segments[index];
            let output = segment.payload;
            if output.is_null() {
                header_error();
                return false;
            }
            if segment.length == 0
                || segment.reserved != 0
                || !output_valid(output, (*output).header.owner_id)
            {
                header_error();
                return false;
            }
            let Some(end) = segment
                .start_sequence
                .checked_add(u64::from(segment.length))
            else {
                header_error();
                return false;
            };
            let retained = (*output)
                .next_sequence
                .saturating_sub((*output).next_sequence.min(CAPACITY as u64));
            if segment.start_sequence < retained || end > (*output).next_sequence {
                header_error();
                return false;
            }
            total += u64::from(segment.length);
        }
        if total != u64::from((*transcript).output_len) {
            header_error();
            return false;
        }
        true
    }
}
pub(super) unsafe fn transcript(instance: *const ProgramInstance) -> *mut t::ConsoleTranscript {
    unsafe { (*payload::console(instance)).transcript_payload }
}
unsafe fn read(
    transcript: *const t::ConsoleTranscript,
    offset: usize,
    out: *mut u8,
    len: usize,
) -> usize {
    unsafe {
        if offset >= (*transcript).output_len as usize || len == 0 {
            return 0;
        }
        let mut logical = offset;
        let mut written = 0;
        let mut index = 0;
        while index < (*transcript).segment_count as usize && written < len {
            let segment = (*transcript).segments[index];
            index += 1;
            let segment_len = segment.length as usize;
            if logical >= segment_len {
                logical -= segment_len;
                continue;
            }
            let output = segment.payload;
            if output.is_null() {
                break;
            }
            let mut sequence = segment.start_sequence + logical as u64;
            let mut remaining = (segment_len - logical).min(len - written);
            while remaining != 0 {
                let offset = (sequence % CAPACITY as u64) as usize;
                let count = remaining.min(CAPACITY - offset);
                ptr::copy_nonoverlapping(
                    ptr::addr_of!((*output).bytes).cast::<u8>().add(offset),
                    out.add(written),
                    count,
                );
                written += count;
                sequence += count as u64;
                remaining -= count;
            }
            logical = 0;
        }
        written
    }
}
unsafe fn remove_first(transcript: *mut t::ConsoleTranscript, active: bool) -> bool {
    unsafe {
        if (*transcript).segment_count == 0 {
            return false;
        }
        let output = (*transcript).segments[0].payload;
        if output.is_null() {
            return false;
        }
        let mut index = 1;
        while index < (*transcript).segment_count as usize {
            (*transcript).segments[index - 1] = (*transcript).segments[index];
            index += 1;
        }
        (*transcript).segment_count -= 1;
        (*transcript).segments[(*transcript).segment_count as usize] = EMPTY_SEGMENT;
        release(output, active)
    }
}
unsafe fn drop_prefix(instance: *mut ProgramInstance, count: usize) {
    unsafe {
        let transcript = transcript(instance);
        if transcript.is_null() {
            return;
        }
        let mut remaining = count.min((*transcript).output_len as usize);
        let dropped = remaining;
        while remaining != 0 && (*transcript).segment_count != 0 {
            let first = ptr::addr_of_mut!((*transcript).segments[0]);
            let length = (*first).length as usize;
            if remaining < length {
                (*first).start_sequence += remaining as u64;
                (*first).length -= u32::try_from(remaining).expect("Transcript prefix");
                (*transcript).output_len -= u32::try_from(remaining).expect("Transcript prefix");
                remaining = 0;
                break;
            }
            remaining -= length;
            (*transcript).output_len -= (*first).length;
            add!(segment_drops, 1);
            remove_first(transcript, true);
        }
        let actual = dropped - remaining;
        if actual != 0 {
            let console = payload::console(instance);
            (*console).state.output_dropped_bytes = (*console)
                .state
                .output_dropped_bytes
                .wrapping_add(u32::try_from(actual).expect("Transcript dropped bytes"));
            add!(segment_drop_bytes, actual as u64);
        }
    }
}
unsafe fn compact(instance: *mut ProgramInstance) -> bool {
    unsafe {
        let transcript = transcript(instance);
        if transcript.is_null() {
            return false;
        }
        if (*transcript).segment_count < 256 {
            return true;
        }
        let output = allocate_output((*instance).id);
        if output.is_null() {
            return false;
        }
        let length = (*transcript).output_len as usize;
        assert!(length <= CAPACITY);
        if read(
            transcript,
            0,
            ptr::addr_of_mut!((*output).bytes).cast(),
            length,
        ) != length
        {
            release(output, true);
            return false;
        }
        (*output).next_sequence = length as u64;
        if length != 0 && !retain(output, true) {
            release(output, true);
            return false;
        }
        if !release_segments(transcript, true) {
            if length != 0 {
                release(output, true);
            }
            release(output, true);
            return false;
        }
        if length != 0 {
            (*transcript).segments[0] = t::ConsoleSegment {
                payload: output,
                start_sequence: 0,
                length: u32::try_from(length).expect("Transcript compact length"),
                reserved: 0,
            };
            (*transcript).segment_count = 1;
            (*transcript).output_len = u32::try_from(length).expect("Transcript compact length");
        }
        release(output, true);
        add!(compactions, 1);
        add!(compaction_bytes, length as u64);
        true
    }
}
#[expect(
    clippy::while_immutable_condition,
    reason = "drop_prefix mutates this exact raw transcript through the retained instance"
)]
unsafe fn ensure_segment(instance: *mut ProgramInstance) -> bool {
    unsafe {
        let transcript = transcript(instance);
        if transcript.is_null() {
            return false;
        }
        if (*transcript).segment_count < 256 {
            return true;
        }
        if compact(instance) {
            return true;
        }
        while (*transcript).segment_count >= 256 && (*transcript).segment_count != 0 {
            drop_prefix(instance, (*transcript).segments[0].length as usize);
        }
        (*transcript).segment_count < 256
    }
}
unsafe fn appended(
    instance: *mut ProgramInstance,
    transcript: *mut t::ConsoleTranscript,
    capture: bool,
) {
    unsafe {
        if capture {
            add!(capture_append_bytes, 1);
        } else {
            add!(visible_append_bytes, 1);
        }
        if (*transcript).output_len > CAPACITY as u32 {
            drop_prefix(instance, (*transcript).output_len as usize - CAPACITY);
        }
        (*payload::console(instance)).state.output_len = (*transcript).output_len;
    }
}
unsafe fn append_reference(
    instance: *mut ProgramInstance,
    output: *mut t::ConsoleOutput,
    sequence: u64,
    capture: bool,
) -> bool {
    unsafe {
        let transcript = transcript(instance);
        if transcript.is_null() {
            return false;
        }
        if (*transcript).segment_count != 0 {
            let tail =
                ptr::addr_of_mut!((*transcript).segments[(*transcript).segment_count as usize - 1]);
            if (*tail).payload == output
                && (*tail).start_sequence + u64::from((*tail).length) == sequence
                && (*tail).length != u32::MAX
            {
                (*tail).length += 1;
                (*transcript).output_len += 1;
                appended(instance, transcript, capture);
                return true;
            }
        }
        if !ensure_segment(instance) || !retain(output, true) {
            return false;
        }
        (*transcript).segments[(*transcript).segment_count as usize] = t::ConsoleSegment {
            payload: output,
            start_sequence: sequence,
            length: 1,
            reserved: 0,
        };
        (*transcript).segment_count += 1;
        (*transcript).output_len += 1;
        appended(instance, transcript, capture);
        true
    }
}
unsafe fn ensure_writer(instance: *mut ProgramInstance) -> *mut t::ConsoleOutput {
    unsafe {
        let console = payload::console(instance);
        let writer = (*console).writer_payload;
        if !writer.is_null() {
            if (*writer).next_sequence < CAPACITY as u64 {
                return writer;
            }
            let replacement = allocate_output((*instance).id);
            if replacement.is_null() {
                return replacement;
            }
            (*console).writer_payload = replacement;
            release(writer, true);
            return replacement;
        }
        let writer = allocate_output((*instance).id);
        if writer.is_null() {
            return writer;
        }
        (*console).writer_payload = writer;
        writer
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SourceByte {
    pub payload: *mut t::ConsoleOutput,
    pub sequence: u64,
}
unsafe fn source_byte(instance: *mut ProgramInstance, ch: u8) -> Option<SourceByte> {
    unsafe {
        let writer = ensure_writer(instance);
        if writer.is_null() {
            return None;
        }
        let sequence = (*writer).next_sequence;
        (*writer).bytes[(sequence % CAPACITY as u64) as usize] = ch;
        (*writer).next_sequence += 1;
        add!(source_bytes, 1);
        Some(SourceByte {
            payload: writer,
            sequence,
        })
    }
}
unsafe fn read_tail(transcript: *const t::ConsoleTranscript, out: *mut u8, len: usize) -> usize {
    unsafe {
        let count = len.min((*transcript).output_len as usize);
        read(
            transcript,
            (*transcript).output_len as usize - count,
            out,
            count,
        )
    }
}
pub(super) unsafe fn raw_presenter(instance: *const ProgramInstance) -> bool {
    unsafe { (*instance).role == 1 && (*payload::console(instance)).host == 2 }
}
unsafe fn mirror(instance: *const ProgramInstance, ch: u8) {
    unsafe {
        if (*payload::console(instance)).host != 2 {
            return;
        }
        program_console_sink(0, u32::from(ch), 0);
        if raw_presenter(instance) {
            program_console_sink(1, u32::from(ch), 0);
        }
    }
}
unsafe fn stream_count(instance: *mut ProgramInstance, stream: u32) {
    unsafe {
        let console = payload::console(instance);
        match stream {
            0 => {}
            1 => (*console).state.stdout_bytes = (*console).state.stdout_bytes.wrapping_add(1),
            2 => (*console).state.stderr_bytes = (*console).state.stderr_bytes.wrapping_add(1),
            _ => unreachable!(),
        }
    }
}
pub(super) unsafe fn pending_locked(console: *mut t::Console) {
    unsafe {
        (*console).state.stdin_pending =
            u32::try_from(if (*console).input_head >= (*console).input_tail {
                (*console).input_head - (*console).input_tail
            } else {
                4096 - (*console).input_tail + (*console).input_head
            })
            .expect("Console pending bytes");
    }
}
unsafe fn pending(instance: *mut ProgramInstance) {
    unsafe {
        let console = payload::console(instance);
        let mutex = ptr::addr_of_mut!((*console).input_lock);
        if !sync_locks::r4os_rust_sync_mutex(mutex, 1, u64::MAX) {
            return;
        }
        pending_locked(console);
        sync_locks::r4os_rust_sync_mutex(mutex, 2, 0);
    }
}
unsafe fn printable(ch: u8) -> bool {
    ch >= 0x20 && ch != 0x7f
}
unsafe fn lines(transcript: *const t::ConsoleTranscript, cols: u32) -> u32 {
    unsafe {
        let cols = cols.max(1);
        let mut lines = 0;
        let mut line_len = 0;
        let mut offset = 0;
        let mut buffer = [0u8; 256];
        while offset < (*transcript).output_len as usize {
            let count = read(transcript, offset, buffer.as_mut_ptr(), buffer.len());
            if count == 0 {
                break;
            }
            for &ch in &buffer[..count] {
                if ch == b'\r' {
                    continue;
                }
                if ch == b'\n' || line_len >= cols {
                    lines += 1;
                    line_len = 0;
                    if ch == b'\n' {
                        continue;
                    }
                }
                if printable(ch) {
                    line_len += 1;
                }
            }
            offset += count;
        }
        if line_len > 0 || lines == 0 {
            lines += 1;
        }
        lines
    }
}
unsafe fn refresh(instance: *mut ProgramInstance) {
    unsafe {
        pending(instance);
        let console = payload::console(instance);
        (*console).state.output_capacity = CAPACITY as u32;
        let Some(_guard) = OutputGuard::acquire() else {
            return;
        };
        let transcript = transcript(instance);
        if transcript.is_null() {
            return;
        }
        (*console).state.output_len = (*transcript).output_len;
        (*console).state.scrollback_lines = lines(transcript, (*console).state.cols);
    }
}
unsafe fn row(instance: *mut ProgramInstance) {
    unsafe {
        let console = payload::console(instance);
        let max = i32::try_from((*console).state.rows.saturating_sub(1)).expect("Console rows");
        if (*console).state.cursor_y < max {
            (*console).state.cursor_y += 1;
        } else {
            (*console).state.cursor_y = max;
        }
    }
}
unsafe fn clamp_cursor(instance: *mut ProgramInstance) {
    unsafe {
        let console = payload::console(instance);
        let maxx = i32::try_from((*console).state.cols.saturating_sub(1)).expect("Console cols");
        let maxy = i32::try_from((*console).state.rows.saturating_sub(1)).expect("Console rows");
        (*console).state.cursor_x = (*console).state.cursor_x.max(0).min(maxx);
        (*console).state.cursor_y = (*console).state.cursor_y.max(0).min(maxy);
    }
}
unsafe fn cursor(instance: *mut ProgramInstance, ch: u8) {
    unsafe {
        let console = payload::console(instance);
        if ch == b'\r' {
            (*console).state.cursor_x = 0;
            return;
        }
        if ch == b'\n' {
            (*console).state.cursor_x = 0;
            row(instance);
            return;
        }
        if !printable(ch) {
            return;
        }
        (*console).state.cursor_x += 1;
        if (*console).state.cursor_x >= i32::try_from((*console).state.cols).expect("Console cols")
        {
            (*console).state.cursor_x = 0;
            row(instance);
        }
        clamp_cursor(instance);
    }
}
unsafe fn revision(instance: *mut ProgramInstance, present: bool, output: bool) {
    unsafe {
        let console = payload::console(instance);
        (*console).revision = (*console).revision.wrapping_add(1);
        if (*console).revision == 0 {
            (*console).revision = 1;
        }
        if output {
            add!(revision_batches, 1);
            if present {
                add!(desktop_signals, 1);
            }
        }
        if present {
            program_console_sink(4, 0, 0);
        }
    }
}
unsafe fn clear_state(instance: *mut ProgramInstance) {
    unsafe {
        let console = payload::console(instance);
        let transcript = transcript(instance);
        if transcript.is_null() {
            return;
        }
        release_segments(transcript, true);
        (*console).state.clear_count = (*console).state.clear_count.wrapping_add(1);
        (*console).state.output_len = 0;
        (*console).state.scrollback_lines = 1;
        (*console).state.output_dropped_bytes = 0;
        (*console).state.cursor_x = 0;
        (*console).state.cursor_y = 0;
        (*console).state.cursor_visible = 1;
        if raw_presenter(instance) {
            program_console_sink(2, 0, 0);
        }
    }
}
unsafe fn clear(instance: *mut ProgramInstance) {
    let Some(guard) = OutputGuard::acquire() else {
        return;
    };
    unsafe {
        clear_state(instance);
    }
    drop(guard);
    unsafe {
        revision(instance, true, true);
    }
}
unsafe fn backspace(instance: *mut ProgramInstance) {
    unsafe {
        let console = payload::console(instance);
        let transcript = transcript(instance);
        if transcript.is_null() {
            return;
        }
        if (*transcript).segment_count != 0 && (*transcript).output_len != 0 {
            let tail =
                ptr::addr_of_mut!((*transcript).segments[(*transcript).segment_count as usize - 1]);
            (*tail).length -= 1;
            (*transcript).output_len -= 1;
            if (*tail).length == 0 {
                let output = (*tail).payload;
                if output.is_null() {
                    return;
                }
                (*transcript).segment_count -= 1;
                (*transcript).segments[(*transcript).segment_count as usize] = EMPTY_SEGMENT;
                release(output, true);
            }
        }
        (*console).state.output_len = (*transcript).output_len;
        if (*console).state.cursor_x > 0 {
            (*console).state.cursor_x -= 1;
        } else if (*console).state.cursor_y > 0 {
            (*console).state.cursor_y -= 1;
            (*console).state.cursor_x =
                i32::try_from((*console).state.cols.saturating_sub(1)).expect("Console cols");
        }
    }
}
unsafe fn emit(
    source: *mut ProgramInstance,
    target: *mut ProgramInstance,
    stream: u32,
    ch: u8,
    inherited: bool,
    source_changed: *mut bool,
    target_changed: *mut bool,
) {
    unsafe {
        if ch == 0x0c || ch == 0x08 {
            if ch == 0x0c {
                clear_state(source);
            } else {
                backspace(source);
            }
            *source_changed = true;
            if !target.is_null() {
                if target != source {
                    if ch == 0x0c {
                        clear_state(target);
                    } else {
                        backspace(target);
                    }
                }
                *target_changed = true;
                mirror(target, ch);
            }
            return;
        }
        let Some(stored) = source_byte(source, ch) else {
            return;
        };
        let appended = append_reference(
            source,
            stored.payload,
            stored.sequence,
            inherited && (target.is_null() || target != source),
        );
        if appended {
            stream_count(source, stream);
            cursor(source, ch);
            *source_changed = true;
        }
        if !target.is_null() {
            if target == source {
                *target_changed = appended;
                if appended {
                    mirror(target, ch);
                }
            } else {
                let shown = append_reference(target, stored.payload, stored.sequence, false);
                if shown {
                    stream_count(target, stream);
                    cursor(target, ch);
                    *target_changed = true;
                    mirror(target, ch);
                }
                if appended && shown {
                    add!(shared_bytes, 1);
                }
            }
        }
    }
}
unsafe fn route(stream: u32, data: &[u8], normalize: bool) -> bool {
    unsafe {
        if super::threads::compute_worker() {
            return false;
        }
        if data.is_empty() {
            return true;
        }
        let source = super::context::current();
        if source.is_null() || (*source).app_class != 0 {
            return false;
        }
        let console = payload::console(source);
        let inherited = (*console).io_target_id != 0;
        let mut lease = None;
        let target = if inherited {
            lease = Lease::handle(contract::ProgramProcessHandle {
                instance_id: (*console).io_target_id,
                reserved: 0,
                generation: (*console).io_target_generation,
            });
            match lease.as_ref() {
                Some(pin) if (*pin.0.instance).app_class == 0 => pin.0.instance,
                _ => ptr::null_mut(),
            }
        } else {
            let target = super::console_input::current();
            if target.is_null() {
                return false;
            }
            target
        };
        let Some(_guard) = OutputGuard::acquire() else {
            return true;
        };
        add!(write_calls, 1);
        let mut source_changed = false;
        let mut target_changed = false;
        for &ch in data {
            if normalize && ch == b'\r' {
                continue;
            }
            if normalize && ch == b'\n' {
                emit(
                    source,
                    target,
                    stream,
                    b'\r',
                    inherited,
                    &mut source_changed,
                    &mut target_changed,
                );
                emit(
                    source,
                    target,
                    stream,
                    b'\n',
                    inherited,
                    &mut source_changed,
                    &mut target_changed,
                );
            } else {
                emit(
                    source,
                    target,
                    stream,
                    ch,
                    inherited,
                    &mut source_changed,
                    &mut target_changed,
                );
            }
        }
        if !target.is_null() {
            if target == source {
                if source_changed || target_changed {
                    revision(source, true, true);
                }
            } else {
                if source_changed {
                    revision(source, false, true);
                }
                if target_changed {
                    revision(target, true, true);
                }
            }
        } else if source_changed {
            revision(source, false, true);
        }
        // Keep the exact target pin until the output guard has been released.
        drop(_guard);
        drop(lease);
        true
    }
}
/// # Safety
/// Exact output record under its original reference/output-owner admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_output(
    op: u8,
    output: *mut t::ConsoleOutput,
    owner: u32,
    active: bool,
) -> bool {
    unsafe {
        match op {
            0 => output_valid(output, owner),
            1 => retain(output, active),
            2 => inactive(output),
            3 => free_output(output),
            4 => release(output, active),
            _ => unreachable!(),
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_allocate(owner: u32) -> *mut t::ConsoleOutput {
    unsafe { allocate_output(owner) }
}
/// # Safety
/// Exact transcript and referenced blocks under original output admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_transcript(
    op: u8,
    transcript: *mut t::ConsoleTranscript,
    owner: u32,
    active: bool,
) -> bool {
    unsafe {
        match op {
            0 => transcript_valid(transcript, owner),
            1 => release_segments(transcript, active),
            2 => transcript_inactive(transcript),
            3 => remove_first(transcript, active),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Retained instance with its original console payload present.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_get(
    instance: *const ProgramInstance,
) -> *mut t::ConsoleTranscript {
    unsafe { transcript(instance) }
}
/// # Safety
/// Valid transcript and disjoint writable output span under output admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_read(
    transcript: *const t::ConsoleTranscript,
    offset: usize,
    out: *mut u8,
    len: usize,
    tail: bool,
) -> usize {
    unsafe {
        if tail {
            read_tail(transcript, out, len)
        } else {
            read(transcript, offset, out, len)
        }
    }
}
/// # Safety
/// Caller retains the exact instance, output mutex and backing block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_append(
    instance: *mut ProgramInstance,
    output: *mut t::ConsoleOutput,
    sequence: u64,
    capture: bool,
) -> bool {
    unsafe { append_reference(instance, output, sequence, capture) }
}
/// # Safety
/// Retained instance and original output admission; exclusive result storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_source(
    instance: *mut ProgramInstance,
    ch: u8,
    out: *mut SourceByte,
) -> bool {
    unsafe {
        let Some(value) = source_byte(instance, ch) else {
            return false;
        };
        out.write(value);
        true
    }
}
/// # Safety
/// Retained instance and original output admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_writer(
    instance: *mut ProgramInstance,
) -> *mut t::ConsoleOutput {
    unsafe { ensure_writer(instance) }
}
/// # Safety
/// Exact retained instance and original caller admission for the selected work.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_instance(
    op: u8,
    instance: *mut ProgramInstance,
    value: u64,
) -> bool {
    unsafe {
        match op {
            0 => {
                drop_prefix(instance, usize::try_from(value).expect("Console prefix"));
                true
            }
            1 => compact(instance),
            2 => ensure_segment(instance),
            3 => {
                backspace(instance);
                true
            }
            4 => {
                clear(instance);
                true
            }
            5 => {
                clear_state(instance);
                true
            }
            6 => raw_presenter(instance),
            7 => {
                stream_count(instance, u32::try_from(value).expect("Console stream"));
                true
            }
            8 => {
                pending(instance);
                true
            }
            9 => {
                refresh(instance);
                true
            }
            10 => {
                cursor(instance, u8::try_from(value).expect("Console char"));
                true
            }
            11 => {
                row(instance);
                true
            }
            12 => {
                clamp_cursor(instance);
                true
            }
            13 => {
                revision(instance, true, false);
                true
            }
            14 => {
                revision(instance, value != 0, true);
                true
            }
            15 => {
                mirror(instance, u8::try_from(value).expect("Console char"));
                true
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Exact console payload under its existing input mutex.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_pending(console: *mut t::Console) {
    unsafe {
        pending_locked(console);
    }
}
/// # Safety
/// Exact transcript under the original output mutex.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_lines(
    transcript: *const t::ConsoleTranscript,
    cols: u32,
) -> u32 {
    unsafe { lines(transcript, cols) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_printable(ch: u8) -> bool {
    unsafe { printable(ch) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_stream(raw: u32) -> bool {
    raw <= 2
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_metric(value: u32, min: u32, max: u32) -> u32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}
/// # Safety
/// Readable data span with the original current-instance execution lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_route(
    stream: u32,
    data: Bytes,
    normalize: bool,
) -> bool {
    unsafe { route(stream, bytes(data), normalize) }
}
/// # Safety
/// Exact retained source/target and output admission; exclusive change flags.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_emit(
    source: *mut ProgramInstance,
    target: *mut ProgramInstance,
    stream: u32,
    ch: u8,
    inherited: bool,
    source_changed: *mut bool,
    target_changed: *mut bool,
) {
    unsafe {
        emit(
            source,
            target,
            stream,
            ch,
            inherited,
            source_changed,
            target_changed,
        );
    }
}
/// # Safety
/// Valid writable output span of capacity bytes. Lookup pins its exact instance.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_api_output(
    id: u32,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe {
        if capacity == 0 {
            return -2;
        }
        if let Some(lease) = Lease::id(id) {
            let instance = lease.0.instance;
            if (*instance).console_payload.is_null() {
                out.write(0);
                return -3;
            }
            let Some(_guard) = OutputGuard::acquire() else {
                out.write(0);
                return -1;
            };
            let transcript = transcript(instance);
            if transcript.is_null() {
                out.write(0);
                return -3;
            }
            let count = read_tail(transcript, out, (capacity - 1) as usize);
            out.add(count).write(0);
            return i32::try_from(count).expect("Console output count");
        }
        if !registry::lock() {
            return -1;
        }
        let completion = registry::completion_id(id);
        let result = if completion.is_null() {
            -1
        } else if (*completion).app_class != 0 {
            out.write(0);
            -3
        } else if (*completion).output_payload.is_null() {
            out.write(0);
            0
        } else {
            let length = (*completion).output_length as usize;
            let count = ((capacity - 1) as usize).min(length);
            let copied = read(
                (*completion).output_payload.cast(),
                length - count,
                out,
                count,
            );
            if copied != count {
                out.write(0);
                -1
            } else {
                out.add(count).write(0);
                i32::try_from(count).expect("Console completion count")
            }
        };
        registry::unlock();
        result
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_api_revision(id: u32) -> u32 {
    unsafe {
        if let Some(lease) = Lease::id(id) {
            let console = (*lease.0.instance).console_payload.cast::<t::Console>();
            return if console.is_null() {
                0
            } else {
                (*console).revision
            };
        }
        if !registry::lock() {
            return 0;
        }
        let node = registry::completion_id(id);
        let value = if node.is_null() || (*node).app_class != 0 {
            0
        } else {
            (*node).output_revision
        };
        registry::unlock();
        value
    }
}
/// # Safety
/// Exclusive canonical ConsoleState output; lookup retains the exact owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_api_state(
    id: u32,
    out: *mut contract::ConsoleState,
) -> i32 {
    unsafe {
        if let Some(lease) = Lease::id(id) {
            let instance = lease.0.instance;
            if (*instance).app_class != 0 {
                out.write(Default::default());
                return -2;
            }
            refresh(instance);
            out.write((*payload::console(instance)).state);
            return 0;
        }
        if !registry::lock() {
            out.write(Default::default());
            return -1;
        }
        let node = registry::completion_id(id);
        let result = if node.is_null() {
            out.write(Default::default());
            -1
        } else if (*node).app_class != 0 {
            out.write(Default::default());
            -2
        } else {
            out.write((*node).console_state);
            0
        };
        registry::unlock();
        result
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_console_api_metrics(id: u32, cols: u32, rows: u32) -> i32 {
    unsafe {
        process::r4os_rust_program_process_signal();
        let Some(lease) = Lease::id(id) else {
            return -1;
        };
        let instance = lease.0.instance;
        if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) {
            return -2;
        }
        if (*instance).app_class != 0 {
            return -3;
        }
        let cols = cols.clamp(1, 512);
        let rows = rows.clamp(1, 256);
        let console = payload::console(instance);
        if (*console).state.cols == cols && (*console).state.rows == rows {
            return 0;
        }
        (*console).state.cols = cols;
        (*console).state.rows = rows;
        clamp_cursor(instance);
        revision(instance, true, false);
        0
    }
}
/// # Safety
/// Existing current-console execution lifetime. Op3 requires exclusive state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_console_host(
    op: u8,
    a: u32,
    b: u32,
    out: *mut contract::ConsoleState,
) -> i32 {
    unsafe {
        if super::threads::compute_worker() {
            return 0;
        }
        let instance = super::console_input::current();
        match op {
            0 => i32::from(!instance.is_null()),
            1 => {
                if instance.is_null() {
                    return 0;
                }
                clear(instance);
                1
            }
            2 => {
                if instance.is_null() {
                    return 0;
                }
                let console = payload::console(instance);
                (*console).state.fg = a;
                (*console).state.bg = b;
                if raw_presenter(instance) {
                    program_console_sink(3, a, b);
                }
                revision(instance, true, false);
                1
            }
            3 => {
                if instance.is_null() {
                    return 0;
                }
                refresh(instance);
                out.write((*payload::console(instance)).state);
                1
            }
            4 => {
                if instance.is_null() {
                    0
                } else {
                    i32::try_from((*payload::console(instance)).host).expect("Console host")
                }
            }
            5 => {
                if instance.is_null()
                    || (*instance).role != 2
                    || (*payload::console(instance)).host != 2
                {
                    return 0;
                }
                AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).desktop_requested))
                    .store(true, Ordering::Release);
                revision(instance, true, false);
                1
            }
            _ => unreachable!(),
        }
    }
}
const _: () = {
    assert!(mem::offset_of!(State, write_calls) == 0);
    assert!(mem::offset_of!(State, source_bytes) == 8);
    assert!(mem::offset_of!(State, visible_append_bytes) == 16);
    assert!(mem::offset_of!(State, capture_append_bytes) == 24);
    assert!(mem::offset_of!(State, shared_bytes) == 32);
    assert!(mem::offset_of!(State, revision_batches) == 40);
    assert!(mem::offset_of!(State, desktop_signals) == 48);
    assert!(mem::offset_of!(State, compactions) == 56);
    assert!(mem::offset_of!(State, compaction_bytes) == 64);
    assert!(mem::offset_of!(State, segment_drops) == 72);
    assert!(mem::offset_of!(State, segment_drop_bytes) == 80);
    assert!(mem::offset_of!(State, lock) == 88);
    assert!(mem::size_of::<SourceByte>() == 16);
    assert!(mem::offset_of!(SourceByte, sequence) == 8);
};

pub(super) unsafe fn sink(op: u8, a: u32, b: u32) {
    unsafe {
        program_console_sink(op, a, b);
    }
}

unsafe fn program_console_sink(op: u8, a: u32, b: u32) {
    use crate::boot::log_route as log;
    match op {
        0 => log::serial_putc_raw(u8::try_from(a).unwrap()),
        1 => log::console_putc_raw(u8::try_from(a).unwrap()),
        2 => log::clear_console(),
        3 => {
            log::r4os_rust_log_control(6, a, b, 0, 0);
        }
        4 => crate::service::desktop_events::r4os_rust_desktop_signal(),
        5 => crate::boot::log::text(&[u8::try_from(a).unwrap()]),
        _ => panic!("Invalid console sink"),
    }
}
