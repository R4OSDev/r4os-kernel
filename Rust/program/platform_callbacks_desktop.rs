//! Direct desktop platform callbacks; original typed owners and call boundaries.
use crate::contract as c;

pub(super) unsafe extern "C" fn desk_read_key() -> u8 {
    { crate::program::program_io::r4os_rust_program_io_key(true) }
        .try_into()
        .expect("checked callback result")
}
pub(super) unsafe extern "C" fn desk_read_key_codepoint() -> u32 {
    crate::program::program_io::r4os_rust_program_io_key(false)
}
pub(super) unsafe extern "C" fn desk_mouse_state(out: *mut c::Mouse) {
    let _ = unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            0,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    };
}
pub(super) unsafe extern "C" fn desk_mouse_motion(out: *mut c::MouseMotion) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            1,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_mouse_show() {
    let _ = unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            2,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}
pub(super) unsafe extern "C" fn desk_mouse_hide() {
    let _ = unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            3,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}
pub(super) unsafe extern "C" fn desk_keyboard_layout_current(
    out: *mut c::KeyboardLayoutInfo,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            5,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_keyboard_layout_at(
    index: u32,
    out: *mut c::KeyboardLayoutInfo,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            6,
            &crate::program::r4desk::Args {
                value: index,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_keyboard_layout_set(layout_name: *const u8) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            7,
            &crate::program::r4desk::Args {
                input: layout_name.cast(),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_program_set_window(id: u32, window_id: i32) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_window(
            id,
            core::ptr::null(),
            window_id,
            false,
        )
    }
}
pub(super) unsafe extern "C" fn desk_program_window_id() -> i32 {
    crate::program::program_api::r4os_rust_program_api_action(0)
}
pub(super) unsafe extern "C" fn desk_gui_set_text(text: *const u8) -> i32 {
    unsafe { crate::program::gui_events::r4os_rust_program_gui_simple(0, text.cast(), 0, 0) }
}
pub(super) unsafe extern "C" fn desk_gui_text(id: u32, out: *mut u8, capacity: u32) -> i32 {
    unsafe {
        crate::program::gui_events::r4os_rust_program_gui_string(id, out.cast(), capacity, false)
    }
}
pub(super) unsafe extern "C" fn desk_gui_set_title(title: *const u8) -> i32 {
    unsafe { crate::program::gui_events::r4os_rust_program_gui_simple(1, title.cast(), 0, 0) }
}
pub(super) unsafe extern "C" fn desk_gui_title(id: u32, out: *mut u8, capacity: u32) -> i32 {
    unsafe {
        crate::program::gui_events::r4os_rust_program_gui_string(id, out.cast(), capacity, true)
    }
}
pub(super) unsafe extern "C" fn desk_gui_set_min_size(w: i32, h: i32) -> i32 {
    unsafe { crate::program::gui_events::r4os_rust_program_gui_simple(2, core::ptr::null(), w, h) }
}
pub(super) unsafe extern "C" fn desk_console_push_key(id: u32, key: u8) -> i32 {
    unsafe {
        crate::program::console_input::r4os_rust_program_console_input_api_push(
            id,
            [key].as_ptr(),
            1,
            true,
        )
    }
}
pub(super) unsafe extern "C" fn desk_console_write(
    stream_raw: u32,
    data: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        crate::program::program_io::r4os_rust_program_io_write(stream_raw, data.cast(), len, true)
    }
}
pub(super) unsafe extern "C" fn desk_clipboard_write(data: *const u8, len: u32) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            8,
            &crate::program::r4desk::Args {
                input: data.cast(),
                value: len,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_clipboard_read(out: *mut u8, capacity: u32) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            9,
            &crate::program::r4desk::Args {
                value: capacity,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_clipboard_revision() -> u32 {
    unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            10,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}
pub(super) unsafe extern "C" fn desk_clipboard_info(out: *mut c::ClipboardInfo) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            11,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_clipboard_clear() -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            12,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_remote_frame_info(out: *mut c::RemoteFrameInfo) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            13,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_remote_frame_read(
    offset_pixels: u32,
    out: *mut u32,
    pixel_count: u32,
    out_info: *mut c::RemoteFrameInfo,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            14,
            &crate::program::r4desk::Args {
                a: u64::from(offset_pixels),
                value: pixel_count,
                second: out_info.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_remote_frame_wait(
    last_revision: u32,
    timeout_ticks: u64,
    out: *mut c::RemoteFrameInfo,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            15,
            &crate::program::r4desk::Args {
                value: last_revision,
                a: timeout_ticks,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_remote_input_push(event: *const c::RemoteInputEvent) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            26,
            &crate::program::r4desk::Args {
                input: event.cast(),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_remote_input_poll(out: *mut c::RemoteInputEvent) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            27,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_remote_input_status(out: *mut c::RemoteInputStatus) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            28,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_desktop_activity_wait(
    last_seq: u64,
    timeout_ticks: u64,
    out_seq: *mut u64,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            16,
            &crate::program::r4desk::Args {
                a: last_seq,
                b: timeout_ticks,
                ..Default::default()
            },
            out_seq.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_gui_activity_wait(
    last: u64,
    timeout: u64,
    subscriptions: u32,
    out: *mut u64,
) -> i32 {
    unsafe { crate::program::gui_activity::wait(last, timeout, subscriptions, out) }
}
pub(super) unsafe extern "C" fn desk_gui_activity_notify(categories: u32) -> i32 {
    crate::program::gui_activity::notify(categories)
}
pub(super) unsafe extern "C" fn desk_remote_frame_map(out: *mut c::RemoteFrameMapInfo) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            29,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_program_current_console_host() -> u32 {
    { crate::program::program_api::r4os_rust_program_api_action(1) }
        .try_into()
        .expect("checked callback result")
}
pub(super) unsafe extern "C" fn desk_program_request_desktop() -> i32 {
    crate::program::program_api::r4os_rust_program_api_action(2)
}
pub(super) unsafe extern "C" fn desk_program_spawn_with_console_host(
    path_ptr: *const u8,
    args_ptr: *const u8,
    policy_raw: u32,
    host_raw: u32,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            1,
            path_ptr.cast(),
            args_ptr.cast(),
            policy_raw,
            host_raw,
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn desk_program_spawn_with_console_host_handle(
    path_ptr: *const u8,
    args_ptr: *const u8,
    policy_raw: u32,
    host_raw: u32,
    out: *mut c::ProgramProcessHandle,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_launch(
            2,
            path_ptr.cast(),
            args_ptr.cast(),
            policy_raw,
            host_raw,
            out.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn desk_program_set_window_handle(
    handle_ptr: *const c::ProgramProcessHandle,
    window_id: i32,
) -> i32 {
    unsafe {
        crate::program::program_api::r4os_rust_program_api_window(
            0,
            handle_ptr.cast(),
            window_id,
            true,
        )
    }
}
pub(super) unsafe extern "C" fn desk_console_push_input(
    id: u32,
    data: *const u8,
    length: u32,
) -> i32 {
    unsafe {
        crate::program::console_input::r4os_rust_program_console_input_api_push(
            id,
            data.cast(),
            length,
            false,
        )
    }
}
pub(super) unsafe extern "C" fn desk_remote_frame_consumers() -> u32 {
    unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            23,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}
pub(super) unsafe extern "C" fn desk_physical_key_poll(out: *mut c::PhysicalKeyEvent) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            4,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_remote_frame_capture_stats(
    out: *mut c::RemoteFrameCaptureStats,
) -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            32,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn desk_desktop_activity_notify() -> i32 {
    (unsafe {
        crate::program::r4desk::r4os_rust_r4desk(
            17,
            &crate::program::r4desk::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
