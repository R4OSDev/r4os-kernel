//! Legacy GUI producer operations over the same native private frame storage.
//! Fallible preparation precedes command/resource publication; exact Alpha8
//! bytes and XRGB masking retain the frozen transport and legacy raster views.
use super::{
    alpha8, gui_api, gui_events::Producer, gui_flow as flow, gui_font as font,
    gui_storage as storage, payload, payload_types as t, process_data,
    registry_types::ProgramInstance,
};
use crate::contract as c;
use core::{
    mem, ptr, slice,
    sync::atomic::{AtomicU32, Ordering},
};
#[expect(
    clippy::too_many_arguments,
    reason = "Original text producer has independent layout/color/font/flags arguments"
)]
pub(super) unsafe fn text(
    instance: *mut ProgramInstance,
    x: i32,
    y: i32,
    input: *const u8,
    fg: u32,
    bg: u32,
    requested: u32,
    flags: u32,
) -> i32 {
    unsafe {
        if input.is_null() {
            return -1;
        }
        if !font::renderable(requested) {
            return -2;
        }
        let id = font::normalize(requested);
        let mut len = 0;
        while len < 1048576 && input.add(len).read() != 0 {
            len += 1;
        }
        if len == 1048576 {
            return -2;
        }
        if core::str::from_utf8(if len == 0 {
            &[]
        } else {
            slice::from_raw_parts(input, len)
        })
        .is_err()
        {
            return -2;
        }
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -2;
        }
        let frame = flow::ensure(instance);
        if frame.is_null() {
            return -4;
        }
        let nodes = u64::from(storage::needs_payload(frame)) + u64::from(len != 0);
        if !storage::can_link(frame, nodes) {
            (*frame).build_failed = true;
            return -4;
        }
        let Some(mut command) = storage::prepare_command(instance, frame) else {
            flow::failed(gui, frame);
            return -4;
        };
        let mut resource = None;
        if len != 0 {
            let Some(reservation) = storage::prepare_resource(instance, frame, len, 1) else {
                storage::cancel_command(instance, &mut command);
                flow::failed(gui, frame);
                payload::add(mem::offset_of!(t::Stats, transaction_rollbacks), 8, 1);
                return -4;
            };
            assert!((*reservation.payload).byte_count as usize == len);
            ptr::copy_nonoverlapping(
                input,
                reservation
                    .payload
                    .cast::<u8>()
                    .add(mem::size_of::<t::Resource>()),
                len,
            );
            resource = Some(reservation);
        }
        let metrics = font::measure(id, input, len + 1);
        let mut value = t::GuiCommand {
            kind: 3,
            x,
            y,
            fg,
            bg,
            font_id: id,
            flags,
            text_w: metrics.width,
            text_h: metrics.height,
            baseline: metrics.baseline,
            line_height: metrics.line_height,
            ..t::EMPTY_COMMAND
        };
        if let Some(resource) = resource.as_ref() {
            value.resource_kind = 1;
            value.payload_offset = (*resource.payload).logical_offset;
            value.payload_bytes = u64::from((*resource.payload).byte_count);
        }
        let result = storage::commit_command(&mut command, value);
        if let Some(resource) = resource.as_mut() {
            storage::commit_resource(resource);
        }
        if result < 0 {
            result
        } else {
            i32::try_from(len).expect("GUI text command length")
        }
    }
}
unsafe fn clear(rgb: u32) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return -1;
        };
        let instance = producer.instance;
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -2;
        }
        let building = (*gui).building_frame;
        if !building.is_null() && (*building).explicit_build && (*building).build_failed {
            return -4;
        }
        let explicit = if building.is_null() {
            false
        } else {
            (*building).explicit_build
        };
        let frame = flow::replace(instance, explicit);
        if frame.is_null() {
            return -4;
        }
        let Some(mut reservation) = storage::prepare_command(instance, frame) else {
            flow::failed(gui, frame);
            return -4;
        };
        storage::commit_command(
            &mut reservation,
            t::GuiCommand {
                kind: 1,
                rgb,
                ..t::EMPTY_COMMAND
            },
        )
    }
}
unsafe fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return -1;
        };
        gui_api::append_command(
            producer.instance,
            t::GuiCommand {
                kind: 2,
                x,
                y,
                w,
                h,
                rgb,
                ..t::EMPTY_COMMAND
            },
        )
    }
}
unsafe fn blit(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    scale: u32,
    pixels: *const u32,
    pixel_count: u32,
) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return -1;
        };
        let instance = producer.instance;
        if pixels.is_null() {
            return -1;
        }
        if w == 0 || h == 0 || w > 128 || h > 128 {
            return -2;
        }
        let total = u64::from(w) * u64::from(h);
        if total > 16384 {
            return -2;
        }
        if u64::from(pixel_count) < total {
            return -3;
        }
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -6;
        }
        let frame = flow::ensure(instance);
        if frame.is_null() {
            return -6;
        }
        let Some(mut render) = storage::prepare_blit(instance, frame, total as usize, 2) else {
            flow::failed(gui, frame);
            return -6;
        };
        let node = render.resource.payload;
        assert!((*node).byte_count as usize / 4 >= total as usize);
        let destination = node
            .cast::<u8>()
            .add(mem::size_of::<t::Resource>())
            .cast::<u32>();
        for index in 0..total as usize {
            destination
                .add(index)
                .write(pixels.add(index).read() & 0x00ff_ffff);
        }
        let scale = if scale == 0 { 1 } else { scale.min(16) };
        storage::commit_blit(
            &mut render,
            t::GuiCommand {
                kind: 4,
                x,
                y,
                w,
                h,
                flags: scale,
                resource_kind: 2,
                payload_offset: (*node).logical_offset,
                payload_bytes: u64::from((*node).byte_count),
                raster_word_offset: (*node).raster_word_offset,
                parameter0: u64::from(scale),
                ..t::EMPTY_COMMAND
            },
        )
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Frozen Alpha8 ABI has separate geometry stride tint and source span"
)]
unsafe fn alpha(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    stride: u32,
    rgb: u32,
    source: *const u8,
    source_len: u32,
) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return -1;
        };
        let instance = producer.instance;
        if source.is_null() {
            return -1;
        }
        let geometry = match alpha8::validate(
            w,
            h,
            stride,
            source_len as usize,
            c::gui_alpha8_max_width,
            c::gui_alpha8_max_height,
            c::gui_alpha8_max_pixels as usize,
        ) {
            Ok(geometry) => geometry,
            Err(alpha8::Error::Source) => return -3,
            Err(_) => return -2,
        };
        let gui = process_data::ensure_gui(instance);
        if gui.is_null() {
            return -6;
        }
        let frame = flow::ensure(instance);
        if frame.is_null() {
            return -6;
        }
        let Some(mut render) = storage::prepare_blit(instance, frame, geometry.packed_words, 3)
        else {
            flow::failed(gui, frame);
            return -6;
        };
        let node = render.resource.payload;
        let destination = node
            .cast::<u8>()
            .add(mem::size_of::<t::Resource>())
            .cast::<u32>();
        if alpha8::pack(
            destination,
            (*node).byte_count as usize / 4,
            source,
            source_len as usize,
            w,
            h,
            stride,
        )
        .is_err()
        {
            storage::cancel_blit(instance, &mut render);
            return -2;
        }
        storage::commit_blit(
            &mut render,
            t::GuiCommand {
                kind: 5,
                x,
                y,
                w,
                h,
                rgb: rgb & 0x00ff_ffff,
                flags: 1,
                resource_kind: 3,
                payload_offset: (*node).logical_offset,
                payload_bytes: geometry.pixel_count as u64,
                raster_word_offset: (*node).raster_word_offset,
                parameter0: 0,
                parameter1: 0,
                ..t::EMPTY_COMMAND
            },
        )
    }
}
/// # Safety
/// Original current GUI producer lifetime and gate admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_clear(rgb: u32) -> i32 {
    unsafe { clear(rgb) }
}
/// # Safety
/// Original current GUI producer lifetime; zero geometry keeps legacy behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_rect(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    rgb: u32,
) -> i32 {
    unsafe { rect(x, y, w, h, rgb) }
}
/// # Safety
/// Original readable pixel span and current exact GUI producer lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_blit(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    scale: u32,
    pixels: *const u32,
    count: u32,
) -> i32 {
    unsafe { blit(x, y, w, h, scale, pixels, count) }
}
/// # Safety
/// Original padded Alpha8 source span and current exact GUI producer lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_alpha(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    stride: u32,
    rgb: u32,
    source: *const u8,
    len: u32,
) -> i32 {
    unsafe { alpha(x, y, w, h, stride, rgb, source, len) }
}
/// # Safety
/// Readable original NUL-terminated text, retained current producer. Current-font
/// form obtains that font only after the original second lazy-GUI admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_draw_text(
    x: i32,
    y: i32,
    input: *const u8,
    fg: u32,
    bg: u32,
    requested: u32,
    flags: u32,
    current_font: bool,
) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return -1;
        };
        let instance = producer.instance;
        let id = if current_font {
            let gui = process_data::ensure_gui(instance);
            if gui.is_null() {
                return -2;
            }
            AtomicU32::from_ptr(ptr::addr_of_mut!((*gui).font_id)).load(Ordering::Acquire)
        } else {
            requested
        };
        text(
            instance,
            x,
            y,
            input,
            fg,
            bg,
            id,
            if current_font { 0 } else { flags },
        )
    }
}
/// # Safety
/// Exact admitted producer instance and original readable NUL-terminated text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_text_command(
    instance: *mut ProgramInstance,
    x: i32,
    y: i32,
    input: *const u8,
    fg: u32,
    bg: u32,
    requested: u32,
    flags: u32,
) -> i32 {
    unsafe { text(instance, x, y, input, fg, bg, requested, flags) }
}
