//! Direct drawing platform callbacks; original typed owners and call boundaries.
use crate::contract as c;

pub(super) unsafe extern "C" fn draw_screen_width() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            3,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}
pub(super) unsafe extern "C" fn draw_screen_height() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            4,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}
pub(super) unsafe extern "C" fn draw_clear(rgb: u32) {
    let _ = unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            5,
            &crate::program::r4draw::Args {
                color: rgb,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}
pub(super) unsafe extern "C" fn draw_rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) {
    let _ = unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            6,
            &crate::program::r4draw::Args {
                x,
                y,
                w,
                h,
                color: rgb,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}
pub(super) unsafe extern "C" fn draw_text(x: i32, y: i32, value: *const u8, fg: u32, bg: u32) {
    let _ = unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            7,
            &crate::program::r4draw::Args {
                x,
                y,
                input: value.cast(),
                color: fg,
                bg,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}
pub(super) unsafe extern "C" fn draw_display_revision() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            8,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}
pub(super) unsafe extern "C" fn draw_display_begin_frame() -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            9,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_begin_frame_rect(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            10,
            &crate::program::r4draw::Args {
                x,
                y,
                w,
                h,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_present() -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            11,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_blit_xrgb32(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    pixels: *const u32,
    pixel_count: u32,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            12,
            &crate::program::r4draw::Args {
                x,
                y,
                w,
                h,
                input: pixels.cast(),
                b: u64::from(pixel_count),
                a: u64::from(w),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_gui_draw_text(
    x: i32,
    y: i32,
    text: *const u8,
    fg: u32,
    bg: u32,
) -> i32 {
    unsafe {
        crate::program::gui_draw::r4os_rust_program_gui_draw_text(
            x,
            y,
            text.cast(),
            fg,
            bg,
            0,
            0,
            true,
        )
    }
}
pub(super) unsafe extern "C" fn draw_gui_draw_text_ex(
    x: i32,
    y: i32,
    text: *const u8,
    fg: u32,
    bg: u32,
    font_id: u32,
    flags: u32,
) -> i32 {
    unsafe {
        crate::program::gui_draw::r4os_rust_program_gui_draw_text(
            x,
            y,
            text.cast(),
            fg,
            bg,
            font_id,
            flags,
            false,
        )
    }
}
pub(super) unsafe extern "C" fn draw_gui_present() -> i32 {
    unsafe { crate::program::gui_events::r4os_rust_program_gui_simple(3, core::ptr::null(), 0, 0) }
}
pub(super) unsafe extern "C" fn draw_font_count() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            22,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}
pub(super) unsafe extern "C" fn draw_font_info(font_id: u32, out: *mut c::GuiFontInfo) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            23,
            &crate::program::r4draw::Args {
                w: font_id,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_font_measure(
    font_id: u32,
    value: *const u8,
    out: *mut c::GuiTextMetrics,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            24,
            &crate::program::r4draw::Args {
                w: font_id,
                input: value.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_text_font(
    font_id: u32,
    x: i32,
    y: i32,
    value: *const u8,
    fg: u32,
    bg: u32,
) {
    let _ = unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            29,
            &crate::program::r4draw::Args {
                w: font_id,
                x,
                y,
                input: value.cast(),
                color: fg,
                bg,
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    };
}
pub(super) unsafe extern "C" fn draw_font_reload() -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            27,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_font_glyph_row(font_id: u32, codepoint: u32, row: u32) -> u64 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            25,
            &crate::program::r4draw::Args {
                w: font_id,
                h: codepoint,
                a: u64::from(row),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gui_frame_begin() -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(0, core::ptr::null(), 0)
    }
}
pub(super) unsafe extern "C" fn draw_gui_frame_commit() -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(3, core::ptr::null(), 0)
    }
}
pub(super) unsafe extern "C" fn draw_gui_frame_cancel() -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(4, core::ptr::null(), 0)
    }
}
pub(super) unsafe extern "C" fn draw_gui_frame_info(
    handle_ptr: *const c::ProgramProcessHandle,
    caller_out: *mut c::GuiFrameInfo,
) -> i32 {
    unsafe {
        crate::program::gui_view::r4os_rust_program_gui_view_api(
            0,
            handle_ptr.cast(),
            0,
            caller_out.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_display_blit_xrgb32_stride(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    pixels: *const u32,
    pixel_count: u32,
    source_stride_pixels: u32,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            12,
            &crate::program::r4draw::Args {
                x,
                y,
                w,
                h,
                input: pixels.cast(),
                b: u64::from(pixel_count),
                a: u64::from(source_stride_pixels),
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_present_regions(
    request: *const c::DisplayPresentRequest,
    pixels: *const u32,
    pixel_count: u32,
    regions: *const c::DisplayDamageRect,
    region_count: u32,
    out: *mut c::DisplayPresentResult,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            13,
            &crate::program::r4draw::Args {
                input: request.cast(),
                second: pixels.cast(),
                third: regions.cast(),
                b: u64::from(pixel_count),
                a: u64::from(region_count),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_present_capabilities(
    out: *mut c::DisplayPresentCapabilities,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            14,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_present_completion(
    fence: u64,
    out: *mut c::DisplayPresentCompletion,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            15,
            &crate::program::r4draw::Args {
                a: fence,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_presentation_stats(
    head_id: u32,
    out: *mut c::DisplayPresentationStats,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            19,
            &crate::program::r4draw::Args {
                w: head_id,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_presentation_info(
    head_id: u32,
    out: *mut c::DisplayPresentationInfo,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            20,
            &crate::program::r4draw::Args {
                w: head_id,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_presentation_feedback(
    head_id: u32,
    source: *const c::GfxFence,
    out: *mut c::DisplayPresentationStats,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            21,
            &crate::program::r4draw::Args {
                w: head_id,
                input: source.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_cursor_info(output: *mut c::DisplayCursorInfo) -> i32 {
    unsafe {
        crate::display::cursor_work::r4os_rust_display_cursor_work(
            1,
            core::ptr::null(),
            core::ptr::null(),
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_display_cursor_status(
    output: *mut c::DisplayCursorStatus,
) -> i32 {
    unsafe {
        crate::display::cursor_work::r4os_rust_display_cursor_work(
            2,
            core::ptr::null(),
            core::ptr::null(),
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gui_frame_begin_damage(
    regions_ptr: *const c::DisplayDamageRect,
    region_count: u32,
) -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(
            1,
            regions_ptr.cast(),
            region_count,
        )
    }
}
pub(super) unsafe extern "C" fn draw_gui_frame_generation_info(
    handle_ptr: *const c::ProgramProcessHandle,
    generation: u64,
    caller_out: *mut c::GuiFrameGenerationInfo,
) -> i32 {
    unsafe {
        crate::program::gui_view::r4os_rust_program_gui_view_api(
            2,
            handle_ptr.cast(),
            generation,
            caller_out.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_font_glyph_bitmap(
    font_id: u32,
    codepoint: u32,
    out: *mut c::GuiGlyphBitmap,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            26,
            &crate::program::r4draw::Args {
                w: font_id,
                h: codepoint,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_font_revision() -> u32 {
    unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            28,
            &crate::program::r4draw::Args {
                ..Default::default()
            },
            core::ptr::null_mut(),
        )
    }
    .try_into()
    .expect("checked callback result")
}
pub(super) unsafe extern "C" fn draw_gui_frame_begin_replace(
    regions_ptr: *const c::DisplayDamageRect,
    region_count: u32,
) -> i32 {
    unsafe {
        crate::program::gui_api::r4os_rust_program_gui_api_transition(
            2,
            regions_ptr.cast(),
            region_count,
        )
    }
}
pub(super) unsafe extern "C" fn draw_gui_frame_stream_info(
    handle_ptr: *const c::ProgramProcessHandle,
    caller_out: *mut c::GuiFrameStreamInfo,
) -> i32 {
    unsafe {
        crate::program::gui_view::r4os_rust_program_gui_view_api(
            1,
            handle_ptr.cast(),
            0,
            caller_out.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_buffer_stats(output: *mut c::GfxBufferStats) -> i32 {
    unsafe {
        crate::gfx::buffer_api::r4os_rust_gfx_buffer_api(
            7,
            &crate::gfx::buffer_owner::Args {
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_memory_budget(
    input: *const c::GfxDeviceBudgetRequest,
    output: *mut c::GfxDeviceBudgetState,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::budget_api::r4os_rust_gfx_budget_api(&args, false, output.cast()) }
}
pub(super) unsafe extern "C" fn draw_gfx_telemetry(
    input: *const c::GfxTelemetryRequest,
    output: *mut c::GfxTelemetryState,
) -> i32 {
    let args = crate::gfx::buffer_owner::Args {
        input: input.cast(),
        ..Default::default()
    };
    unsafe { crate::gfx::telemetry_api::r4os_rust_gfx_telemetry_api(0, &args, output.cast()) }
}
pub(super) unsafe extern "C" fn draw_display_output_target(
    adapter_id: u32,
    head_id: u32,
    out: *mut c::GfxOutputTarget,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            16,
            &crate::program::r4draw::Args {
                w: adapter_id,
                h: head_id,
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_output_presentation_info(
    input: *const c::GfxOutputTarget,
    out: *mut c::DisplayPresentationInfo,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            17,
            &crate::program::r4draw::Args {
                input: input.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_display_output_presentation_feedback(
    input: *const c::GfxOutputTarget,
    source: *const c::GfxFence,
    out: *mut c::DisplayPresentationStats,
) -> i32 {
    (unsafe {
        crate::program::r4draw::r4os_rust_r4draw(
            18,
            &crate::program::r4draw::Args {
                input: input.cast(),
                second: source.cast(),
                ..Default::default()
            },
            out.cast(),
        )
    } as u32) as i32
}
pub(super) unsafe extern "C" fn draw_gfx_fence_query(
    input: *const c::GfxFence,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            7,
            &crate::program::gfx_queue_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_fence_wait(
    input: *const c::GfxFence,
    timeout_ticks: u64,
    wait_for: u32,
    output: *mut c::GfxFenceStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            8,
            &crate::program::gfx_queue_api::Args {
                input: input.cast(),
                value: timeout_ticks,
                which: wait_for,
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_queue_backend(
    index: u32,
    output: *mut c::GfxBackendBinding,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            0,
            &crate::program::gfx_queue_api::Args {
                which: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_queue_backend_info(
    index: u32,
    output: *mut c::GfxBackendInfo,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            1,
            &crate::program::gfx_queue_api::Args {
                which: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_queue_backend_properties(
    input: *const c::GfxBackendBinding,
    output: *mut c::GfxBackendProperties,
) -> i32 {
    unsafe {
        crate::program::gfx_queue_api::r4os_rust_gfx_queue_api(
            2,
            &crate::program::gfx_queue_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_output_revision(
    output: *mut c::GfxDisplayRevision,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            0,
            &crate::program::gfx_output_api::Args {
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_output_info(
    index: u32,
    output: *mut c::GfxOutputInfo,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            1,
            &crate::program::gfx_output_api::Args {
                value: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_output_mode(
    input: *const c::GfxOutputId,
    index: u32,
    output: *mut c::GfxOutputMode,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            2,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                value: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_output_edid(
    input: *const c::GfxOutputId,
    index: u32,
    output: *mut c::GfxEdidBlock,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            3,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                value: index,
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_output_color(
    input: *const c::GfxOutputId,
    output: *mut c::GfxOutputColorState,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            4,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_output_refresh(
    input: *const c::GfxOutputTarget,
    output: *mut c::GfxOutputRefresh,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            5,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_output_power(
    input: *const c::GfxOutputId,
    output: *mut c::GfxOutputPower,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            7,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_output_brightness(
    input: *const c::GfxOutputId,
    output: *mut c::GfxOutputBrightness,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            15,
            &crate::program::gfx_output_api::Args {
                input: input.cast(),
                ..Default::default()
            },
            output.cast(),
        )
    }
}
pub(super) unsafe extern "C" fn draw_gfx_atomic_status(
    ticket: u64,
    output: *mut c::GfxModeStatus,
) -> i32 {
    unsafe {
        crate::program::gfx_output_api::r4os_rust_gfx_output_api(
            13,
            &crate::program::gfx_output_api::Args {
                ticket,
                ..Default::default()
            },
            output.cast(),
        )
    }
}
