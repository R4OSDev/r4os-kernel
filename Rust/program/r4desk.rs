//! R4DESK copied input/clipboard API. Capture storage lives at r4desk_frame;
//! input queue publication precedes the existing desktop activity wake.
use super::{r4desk_frame as frame, r4draw, remote_frame_state::Owner};
use crate::{
    contract as a,
    display::ownership::{self, Resident},
    fs::vfs_types::Bytes,
    input::{key_layout, keyboard, mouse, physical_key},
    platform::timer,
    sched::task_context::{self, Unwind},
    service::{desktop_events, ffi as services},
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{MaybeUninit, size_of},
    ptr, slice,
};
const CLIPBOARD_BYTES: usize = 4096;
const SERVICE_MAX_PAYLOAD: usize = 4096;
const SERVICE_TIMEOUT: u64 = 1200 * crate::arch::pit::DEFAULT_HZ as u64 / 1000;
const SERVICE_MAGIC: u32 = 0x43565352;
const SERVICE_SMALL: i32 = -6;
const SERVICE_TIMEOUT_ERROR: i32 = -8;
static CLIPBOARD: Resident<[u8; CLIPBOARD_BYTES]> = Resident(UnsafeCell::new([0; CLIPBOARD_BYTES]));
static CLIPBOARD_LEN: Resident<usize> = Resident(UnsafeCell::new(0));
static CLIPBOARD_REV: Resident<u32> = Resident(UnsafeCell::new(0));
const INPUT_CAPACITY: usize = 64;
struct Input {
    items: [a::RemoteInputEvent; INPUT_CAPACITY],
    head: usize,
    tail: usize,
    pending: usize,
    pushed: u32,
    polled: u32,
    dropped: u32,
    sequence: u32,
    last: a::RemoteInputEvent,
}
static INPUT: Resident<MaybeUninit<Input>> = Resident(UnsafeCell::new(MaybeUninit::uninit()));
static INPUT_READY: Resident<bool> = Resident(UnsafeCell::new(false));
fn input_state<R>(f: impl FnOnce(*mut Input) -> R) -> R {
    let _state = ownership::enter_state();
    unsafe {
        let p = INPUT.0.get().cast::<Input>();
        if !*INPUT_READY.0.get() {
            for i in 0..INPUT_CAPACITY {
                ptr::addr_of_mut!((*p).items)
                    .cast::<a::RemoteInputEvent>()
                    .add(i)
                    .write(a::RemoteInputEvent::default())
            }
            ptr::addr_of_mut!((*p).head).write(0);
            ptr::addr_of_mut!((*p).tail).write(0);
            ptr::addr_of_mut!((*p).pending).write(0);
            ptr::addr_of_mut!((*p).pushed).write(0);
            ptr::addr_of_mut!((*p).polled).write(0);
            ptr::addr_of_mut!((*p).dropped).write(0);
            ptr::addr_of_mut!((*p).sequence).write(0);
            ptr::addr_of_mut!((*p).last).write(a::RemoteInputEvent::default());
            *INPUT_READY.0.get() = true;
        }
        f(p)
    }
}
fn copy_fixed(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let count = value.len().min(out.len() - 1);
    if count > 0 {
        out[..count].copy_from_slice(&value[..count])
    }
}
/// # Safety
/// Output is the same complete non-null public mouse record as the original ABI.
pub unsafe fn mouse_state(out: *mut a::Mouse) {
    let s = mouse::snapshot_for_api();
    unsafe {
        out.write(a::Mouse {
            x: s.x,
            y: s.y,
            dx: s.dx,
            dy: s.dy,
            wheel: s.wheel,
            buttons: s.buttons,
            present: u8::from(s.present),
            reserved: 0,
            packets: s.packets,
        })
    }
}
/// # Safety
/// Complete initialized motion record remains writable through both snapshots.
pub unsafe fn mouse_motion(out: *mut a::MouseMotion) -> i32 {
    if out.is_null()
        || unsafe { (*out).version != 1 || ((*out).size as usize) < size_of::<a::MouseMotion>() }
    {
        return -1;
    }
    let mut value = a::MouseMotion {
        version: 1,
        size: size_of::<a::MouseMotion>() as u32,
        mouse: a::Mouse {
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
            wheel: 0,
            buttons: 0,
            present: 0,
            reserved: 0,
            packets: 0,
        },
        motion_x: 0,
        motion_y: 0,
    };
    unsafe { mouse_state(&mut value.mouse) };
    let totals = mouse::motion_totals();
    value.motion_x = totals as u32;
    value.motion_y = (totals >> 32) as u32;
    unsafe { out.write(value) };
    0
}
pub fn mouse_show() {
    r4draw::mark_display_used();
    mouse::r4os_rust_input_mouse_enable_cursor()
}
pub fn mouse_hide() {
    mouse::r4os_rust_input_mouse_disable_cursor()
}
/// # Safety
/// Caller retains a complete writable physical-event record for this poll.
pub unsafe fn physical_key_poll(out: *mut a::PhysicalKeyEvent) -> i32 {
    if out.is_null() {
        return a::physical_key_poll_error_invalid;
    }
    let mut event = MaybeUninit::<physical_key::Event>::uninit();
    if !unsafe { keyboard::r4os_rust_input_keyboard_physical(event.as_mut_ptr()) } {
        unsafe { out.write(a::PhysicalKeyEvent::default()) };
        return a::physical_key_poll_empty;
    }
    let event = unsafe { event.assume_init() };
    unsafe {
        out.write(a::PhysicalKeyEvent {
            magic: a::physical_key_magic,
            version: a::physical_key_version,
            size: size_of::<a::PhysicalKeyEvent>() as u16,
            kind: match event.kind {
                physical_key::Kind::Down => a::physical_key_kind_down,
                physical_key::Kind::Up => a::physical_key_kind_up,
                physical_key::Kind::Reset => a::physical_key_kind_reset,
            },
            key: u32::from(event.usage),
            modifiers: event.modifiers,
            flags: event.flags,
            sequence: event.sequence,
            tick: event.tick,
        })
    };
    a::physical_key_poll_ready
}
unsafe fn fill_layout(out: *mut a::KeyboardLayoutInfo, index: usize) -> bool {
    let count = key_layout::r4os_rust_input_layout_count();
    if index >= count {
        return false;
    }
    let definition = key_layout::definition(index as u32);
    let mut info = a::KeyboardLayoutInfo {
        index: index as u32,
        count: count as u32,
        ..Default::default()
    };
    unsafe {
        copy_fixed(
            &mut info.name,
            slice::from_raw_parts(definition.name.pointer, definition.name.len),
        );
        copy_fixed(
            &mut info.display,
            slice::from_raw_parts(definition.display.pointer, definition.display.len),
        );
        out.write(info)
    }
    true
}
/// # Safety
/// Output retains a complete writable keyboard-layout record.
pub unsafe fn layout_current(out: *mut a::KeyboardLayoutInfo) -> i32 {
    if unsafe { fill_layout(out, keyboard::r4os_rust_input_keyboard_layout_id() as usize) } {
        1
    } else {
        -1
    }
}
/// # Safety
/// A present catalogue item writes one complete output record.
pub unsafe fn layout_at(index: u32, out: *mut a::KeyboardLayoutInfo) -> i32 {
    i32::from(unsafe { fill_layout(out, index as usize) })
}
/// # Safety
/// Original NUL-terminated name retains up to the same32-byte bound.
pub unsafe fn layout_set(name: *const u8) -> i32 {
    let mut bytes = [0u8; 32];
    let mut len = 0;
    while len < bytes.len() && unsafe { name.add(len).read() } != 0 {
        bytes[len] = unsafe { name.add(len).read() };
        len += 1
    }
    if len == bytes.len() {
        return -1;
    }
    if unsafe { keyboard::r4os_rust_input_keyboard_layout_name(bytes.as_ptr(), len, false) } {
        0
    } else {
        -1
    }
}
fn bump_clipboard() {
    unsafe {
        let p = CLIPBOARD_REV.0.get();
        *p = (*p).wrapping_add(1);
        if *p == 0 {
            *p = 1
        }
    }
}
fn clear_clipboard() {
    unsafe {
        ptr::write_bytes(CLIPBOARD.0.get().cast::<u8>(), 0, CLIPBOARD_BYTES);
        *CLIPBOARD_LEN.0.get() = 0
    }
}
struct ServiceHandle(u32);
impl Drop for ServiceHandle {
    fn drop(&mut self) {
        let _ = unsafe { services::r4os_rust_services_endpoint_control(0, self.0, 0, 0) };
    }
}
struct ServiceRequest {
    handle: u32,
    id: u32,
    active: bool,
    unwind: Unwind,
}
impl Drop for ServiceRequest {
    fn drop(&mut self) {
        if self.active {
            let _ = unsafe {
                services::r4os_rust_services_endpoint_control(5, self.handle, self.id, 0)
            };
        }
        let _ = unsafe { task_context::leave(self.unwind) };
    }
}
fn bytes(value: &[u8]) -> Bytes {
    Bytes {
        data: value.as_ptr(),
        len: value.len(),
    }
}
fn service_call(
    op: u16,
    request: &[u8],
    response: &mut [u8],
    header: &mut a::ServiceMessageHeader,
) -> Option<i32> {
    if request.len() > SERVICE_MAX_PAYLOAD || response.len() > SERVICE_MAX_PAYLOAD {
        return None;
    }
    let mut info = a::ServiceInfo::default();
    let open = unsafe {
        services::r4os_rust_services_info(3, 0, bytes(b"CLIPSVC"), ptr::null(), &mut info, 0)
    };
    if open != 0 || info.handle == 0 {
        return None;
    }
    let _handle = ServiceHandle(info.handle);
    *header = a::ServiceMessageHeader {
        magic: 0,
        version: 0,
        ..Default::default()
    };
    let start = timer::tick_count();
    let deadline = timer::r4os_rust_timer_control(6, start, SERVICE_TIMEOUT);
    let mut unwind = Unwind::EMPTY;
    let raw = unsafe {
        services::r4os_rust_services_request(
            info.handle,
            0,
            op,
            bytes(request),
            SERVICE_TIMEOUT,
            true,
            &mut unwind,
        )
    };
    if raw <= 0 {
        return None;
    }
    let id = raw as u32;
    let mut request = ServiceRequest {
        handle: info.handle,
        id,
        active: true,
        unwind,
    };
    loop {
        let result = unsafe {
            services::r4os_rust_services_recv(
                info.handle,
                id,
                false,
                header,
                response.as_mut_ptr(),
                response.len(),
            )
        };
        if result < 0 {
            return Some(result);
        }
        if result > 0 || header.magic == SERVICE_MAGIC {
            request.active = false;
            return Some(result);
        }
        let now = timer::tick_count();
        if now >= deadline {
            break;
        }
        let wait = unsafe {
            services::r4os_rust_services_endpoint_control(4, info.handle, id, deadline - now)
        };
        if wait == SERVICE_TIMEOUT_ERROR {
            break;
        }
        if wait < 0 {
            // Preserve the original explicit cancel and still-active unwind
            // cleanup; this path intentionally keeps its existing retry.
            let _ = unsafe { services::r4os_rust_services_endpoint_control(5, info.handle, id, 0) };
            return Some(wait);
        }
    }
    let _ = unsafe { services::r4os_rust_services_endpoint_control(5, info.handle, id, 0) };
    request.active = false;
    Some(SERVICE_TIMEOUT_ERROR)
}
fn service_info(out: &mut a::ClipboardInfo) -> bool {
    let mut header = a::ServiceMessageHeader::default();
    let mut response = [0u8; 16];
    let Some(got) = service_call(
        a::clipboard_service_op_info,
        &[],
        &mut response,
        &mut header,
    ) else {
        return false;
    };
    if got != 16 || header.status != 0 {
        return false;
    }
    *out = a::ClipboardInfo {
        capacity: u32::from_le_bytes(response[0..4].try_into().expect("four bytes")),
        length: u32::from_le_bytes(response[4..8].try_into().expect("four bytes")),
        revision: u32::from_le_bytes(response[8..12].try_into().expect("four bytes")),
        flags: u32::from_le_bytes(response[12..16].try_into().expect("four bytes")),
    };
    out.capacity == a::clipboard_max_text_bytes && out.length <= a::clipboard_max_text_bytes
}
fn service_write(data: &[u8]) -> Option<i32> {
    let mut header = a::ServiceMessageHeader::default();
    let mut response = [0u8; 1];
    let got = service_call(
        a::clipboard_service_op_write,
        data,
        &mut response,
        &mut header,
    )?;
    if got < 0 { None } else { Some(header.status) }
}
unsafe fn service_read(out: *mut u8, capacity: usize) -> Option<i32> {
    let mut info = a::ClipboardInfo::default();
    if !service_info(&mut info) {
        return None;
    }
    let text_len = info.length as usize;
    if text_len == 0 {
        if capacity > 0 {
            unsafe { out.write(0) }
        }
        return Some(0);
    }
    if capacity <= text_len {
        if capacity > 0 {
            unsafe { out.write(0) }
        }
        return Some(a::clipboard_error_buffer_too_small);
    }
    let mut header = a::ServiceMessageHeader::default();
    let response = unsafe { slice::from_raw_parts_mut(out, capacity.min(SERVICE_MAX_PAYLOAD)) };
    let got = service_call(a::clipboard_service_op_read, &[], response, &mut header)?;
    if got == SERVICE_SMALL {
        return Some(a::clipboard_error_buffer_too_small);
    }
    if got < 0 {
        return None;
    }
    if header.status < 0 {
        return Some(header.status);
    }
    let count = got as usize;
    if count >= capacity {
        if capacity > 0 {
            unsafe { out.write(0) }
        }
        return Some(a::clipboard_error_buffer_too_small);
    }
    unsafe { out.add(count).write(0) }
    Some(count as i32)
}
fn service_clear() -> Option<i32> {
    let mut header = a::ServiceMessageHeader::default();
    let mut response = [0u8; 1];
    let got = service_call(
        a::clipboard_service_op_clear,
        &[],
        &mut response,
        &mut header,
    )?;
    if got < 0 { None } else { Some(header.status) }
}
/// # Safety
/// Caller retains len readable bytes throughout the original IPC/local write.
pub unsafe fn clipboard_write(data: *const u8, len: u32) -> i32 {
    if data.is_null() {
        return a::clipboard_error_invalid;
    }
    let count = len as usize;
    if len > a::clipboard_max_text_bytes {
        return a::clipboard_error_too_large;
    }
    for i in 0..count {
        if unsafe { data.add(i).read() } == 0 {
            return a::clipboard_error_invalid;
        }
    }
    let text = unsafe { slice::from_raw_parts(data, count) };
    if let Some(result) = service_write(text) {
        return result;
    }
    unsafe {
        let dest = CLIPBOARD.0.get().cast::<u8>();
        for (i, &byte) in text.iter().enumerate() {
            dest.add(i).write(byte)
        }
        dest.add(count).write(0);
        if count + 1 < CLIPBOARD_BYTES {
            ptr::write_bytes(dest.add(count + 1), 0, CLIPBOARD_BYTES - count - 1)
        }
        *CLIPBOARD_LEN.0.get() = count;
    }
    bump_clipboard();
    count as i32
}
/// # Safety
/// Caller retains capacity writable bytes throughout the original IPC/read.
pub unsafe fn clipboard_read(out: *mut u8, capacity: u32) -> i32 {
    if out.is_null() {
        return a::clipboard_error_invalid;
    }
    let capacity = capacity as usize;
    if let Some(result) = unsafe { service_read(out, capacity) } {
        return result;
    }
    unsafe {
        let len = *CLIPBOARD_LEN.0.get();
        if len == 0 {
            if capacity > 0 {
                out.write(0)
            }
            return 0;
        }
        if capacity <= len {
            if capacity > 0 {
                out.write(0)
            }
            return a::clipboard_error_buffer_too_small;
        }
        let count = len.min(capacity - 1);
        for i in 0..count {
            out.add(i)
                .write(CLIPBOARD.0.get().cast::<u8>().add(i).read())
        }
        out.add(count).write(0);
        count as i32
    }
}
pub fn clipboard_revision() -> u32 {
    let mut info = a::ClipboardInfo::default();
    if service_info(&mut info) {
        info.revision
    } else {
        unsafe { *CLIPBOARD_REV.0.get() }
    }
}
/// # Safety
/// Output is a complete writable public clipboard info record.
pub unsafe fn clipboard_info(out: *mut a::ClipboardInfo) -> i32 {
    if out.is_null() {
        return a::clipboard_error_invalid;
    }
    if service_info(unsafe { &mut *out }) {
        return 0;
    }
    unsafe {
        out.write(a::ClipboardInfo {
            capacity: a::clipboard_max_text_bytes,
            length: *CLIPBOARD_LEN.0.get() as u32,
            revision: *CLIPBOARD_REV.0.get(),
            flags: a::clipboard_flag_text
                | if *CLIPBOARD_LEN.0.get() > 0 {
                    a::clipboard_flag_has_text
                } else {
                    0
                },
        })
    }
    0
}
pub fn clipboard_clear() -> i32 {
    if let Some(result) = service_clear() {
        return result;
    }
    clear_clipboard();
    bump_clipboard();
    0
}
/// # Safety
/// Complete readable event remains resident while its metadata is admitted.
pub unsafe fn remote_input_push(event: *const a::RemoteInputEvent) -> i32 {
    if event.is_null() {
        return a::remote_input_error_invalid;
    }
    let event = unsafe { event.read() };
    if event.magic != a::remote_input_magic
        || event.version != a::remote_input_version
        || !matches!(
            event.kind,
            a::remote_input_kind_key_down
                | a::remote_input_kind_key_up
                | a::remote_input_kind_mouse_move
                | a::remote_input_kind_mouse_buttons
                | a::remote_input_kind_mouse_wheel
        )
    {
        return a::remote_input_error_invalid;
    }
    let result = input_state(|p| unsafe {
        if (*p).pending >= INPUT_CAPACITY {
            (*p).dropped = (*p).dropped.wrapping_add(1);
            return a::remote_input_error_full;
        }
        let mut stored = event;
        (*p).sequence = (*p).sequence.wrapping_add(1);
        if (*p).sequence == 0 {
            (*p).sequence = 1
        }
        stored.sequence = (*p).sequence;
        (*p).items[(*p).head] = stored;
        (*p).head = ((*p).head + 1) % INPUT_CAPACITY;
        (*p).pending += 1;
        (*p).pushed = (*p).pushed.wrapping_add(1);
        (*p).last = stored;
        1
    });
    if result == 1 {
        desktop_events::r4os_rust_desktop_signal()
    }
    result
}
/// # Safety
/// Output is one complete writable event through its metadata poll.
pub unsafe fn remote_input_poll(out: *mut a::RemoteInputEvent) -> i32 {
    if out.is_null() {
        return a::remote_input_error_invalid;
    }
    input_state(|p| unsafe {
        if (*p).pending == 0 {
            out.write(a::RemoteInputEvent::default());
            return 0;
        }
        out.write((*p).items[(*p).tail]);
        (*p).tail = ((*p).tail + 1) % INPUT_CAPACITY;
        (*p).pending -= 1;
        (*p).polled = (*p).polled.wrapping_add(1);
        1
    })
}
/// # Safety
/// Output is a complete writable public status record through metadata read.
pub unsafe fn remote_input_status(out: *mut a::RemoteInputStatus) -> i32 {
    if out.is_null() {
        return a::remote_input_error_invalid;
    }
    input_state(|p| unsafe {
        let last = (*p).last;
        out.write(a::RemoteInputStatus {
            capacity: INPUT_CAPACITY as u32,
            pending: (*p).pending as u32,
            dropped: (*p).dropped,
            pushed: (*p).pushed,
            polled: (*p).polled,
            last_sequence: last.sequence,
            last_kind: last.kind,
            last_buttons: last.buttons,
            last_key: last.key,
            last_x: last.x,
            last_y: last.y,
            last_wheel: last.wheel,
            ..Default::default()
        });
        0
    })
}
#[repr(C)]
#[derive(Default)]
pub struct Args {
    pub owner: Owner,
    pub a: u64,
    pub b: u64,
    pub value: u32,
    pub other: u32,
    pub input: *const c_void,
    pub second: *const c_void,
    pub third: *const c_void,
}
/// # Safety
/// Existing domain/Kernel facades retain exact admitted public records and
/// owner identities; each pointer range follows its original slot contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4desk(op: u32, args: *const Args, out: *mut c_void) -> u64 {
    let args = unsafe { &*args };
    let signed = |value: i32| value as i64 as u64;
    unsafe {
        match op {
            0 => {
                mouse_state(out.cast());
                0
            }
            1 => signed(mouse_motion(out.cast())),
            2 => {
                mouse_show();
                0
            }
            3 => {
                mouse_hide();
                0
            }
            4 => signed(physical_key_poll(out.cast())),
            5 => signed(layout_current(out.cast())),
            6 => signed(layout_at(args.value, out.cast())),
            7 => signed(layout_set(args.input.cast())),
            8 => signed(clipboard_write(args.input.cast(), args.value)),
            9 => signed(clipboard_read(out.cast(), args.value)),
            10 => u64::from(clipboard_revision()),
            11 => signed(clipboard_info(out.cast())),
            12 => signed(clipboard_clear()),
            13 => signed(frame::info(out.cast())),
            14 => signed(frame::read(
                args.a as u32,
                out.cast(),
                args.value,
                args.second.cast_mut().cast(),
            )),
            15 => signed(frame::wait(args.value, args.a, out.cast())),
            16 => signed(if out.is_null() {
                a::remote_input_error_invalid
            } else {
                desktop_events::r4os_rust_desktop_wait(args.a, args.b, out.cast())
            }),
            17 => {
                desktop_events::r4os_rust_desktop_signal();
                0
            }
            18 => signed(frame::acquire(args.owner)),
            19 => signed(frame::release(args.owner)),
            20 => u64::from(frame::publisher(args.owner)),
            21 => signed(frame::source_reset(args.owner)),
            22 => {
                frame::stopped(args.owner);
                0
            }
            23 => u64::from(frame::consumers()),
            24 => signed(frame::publish(
                args.input.cast(),
                args.second.cast(),
                args.value,
            )),
            25 => signed(frame::publish_regions(
                args.input.cast(),
                args.second.cast(),
                args.value,
                args.third.cast(),
                args.other,
            )),
            26 => signed(remote_input_push(args.input.cast())),
            27 => signed(remote_input_poll(out.cast())),
            28 => signed(remote_input_status(out.cast())),
            29 => signed(frame::map(out.cast())),
            30 => signed(frame::snapshot_acquire(
                args.owner,
                args.value,
                out.cast(),
                args.second.cast_mut().cast(),
            )),
            31 => signed(frame::snapshot_release(args.owner, args.input.cast())),
            32 => signed(frame::capture_stats(out.cast())),
            _ => panic!("R4DESK slot"),
        }
    }
}
const _: () = {
    assert!(size_of::<Args>() == 64);
    assert!(core::mem::offset_of!(Args, input) == 40);
};
