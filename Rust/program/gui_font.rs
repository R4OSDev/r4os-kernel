//! GUI font selection and canonical metadata formatting. The existing font
//! catalog/measurement owner supplies bounded value snapshots; GUI ownership,
//! flags, defaults and fixed NUL-terminated output formatting remain native.
use super::{
    console::Lease, context, gui_events::Producer, gui_flow, payload_types as t, process,
    process_data,
};
use crate::{
    contract::{self as c, GuiFontInfo as Info},
    fs::vfs_types::Bytes,
};
use core::{
    mem::{self, MaybeUninit},
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
#[repr(C)]
pub struct Catalog {
    pub info: Info,
    pub reserved: u32,
    pub path_len: usize,
    pub family_len: usize,
    pub face_len: usize,
    pub style_len: usize,
    pub status_len: usize,
    pub renderable: bool,
    pub selected: bool,
    pub reserved2: [u8; 6],
}
#[repr(C)]
pub struct Measurement {
    pub width: u32,
    pub height: u32,
    pub baseline: i32,
    pub line_height: u32,
}
unsafe extern "C" {
    fn r4os_zig_program_font_control(op: u8, id: u32) -> u32;
    fn r4os_zig_program_font_catalog(id: u32, out: *mut Catalog) -> bool;
    fn r4os_zig_program_font_measure(
        id: u32,
        text: *const u8,
        max_bytes: usize,
        out: *mut Measurement,
    );
}
pub(super) fn renderable(id: u32) -> bool {
    unsafe { r4os_zig_program_font_control(0, id) != 0 }
}
pub(super) fn normalize(id: u32) -> u32 {
    unsafe { r4os_zig_program_font_control(1, id) }
}
pub(super) unsafe fn measure(id: u32, text: *const u8, max: usize) -> Measurement {
    unsafe {
        let mut out = MaybeUninit::uninit();
        r4os_zig_program_font_measure(id, text, max, out.as_mut_ptr());
        out.assume_init()
    }
}
unsafe fn copy<const N: usize>(out: *mut [u8; N], value: &[u8]) {
    unsafe {
        process_data::r4os_rust_program_data_copy_slice(
            out.cast(),
            N,
            Bytes {
                data: value.as_ptr(),
                len: value.len(),
            },
        );
    }
}
pub(super) unsafe fn fill(id: u32, selected: bool, out: *mut Info) -> i32 {
    unsafe {
        out.write(Info::default());
        if id == c::gui_font_builtin_id {
            out.write(Info {
                id: c::gui_font_builtin_id,
                kind: 0,
                flags: c::gui_font_flag_renderable
                    | c::gui_font_flag_builtin
                    | if selected || r4os_zig_program_font_control(2, 0) == c::gui_font_builtin_id {
                        c::gui_font_flag_selected
                    } else {
                        0
                    },
                weight: 400,
                style_flags: 0,
                charset_flags: 0,
                width: 8,
                height: 8,
                max_advance: 8,
                line_height: 8,
                baseline: 7,
                glyph_count: 95,
                strike_count: 1,
                ..Default::default()
            });
            copy(ptr::addr_of_mut!((*out).family), b"R4OS");
            copy(ptr::addr_of_mut!((*out).face), b"Builtin 8x8");
            copy(ptr::addr_of_mut!((*out).style), b"Regular");
            copy(ptr::addr_of_mut!((*out).status), b"builtin fallback");
            return 1;
        }
        let mut catalog = MaybeUninit::<Catalog>::uninit();
        if !r4os_zig_program_font_catalog(id, catalog.as_mut_ptr()) {
            return 0;
        }
        let catalog = catalog.assume_init();
        let info = catalog.info;
        out.write(Info {
            id,
            kind: info.kind,
            flags: if catalog.renderable {
                c::gui_font_flag_renderable
            } else {
                0
            } | if selected || catalog.selected {
                c::gui_font_flag_selected
            } else {
                0
            },
            weight: info.weight,
            style_flags: info.style_flags,
            charset_flags: info.charset_flags,
            width: info.width,
            height: info.height,
            max_advance: info.max_advance,
            line_height: info.line_height,
            baseline: info.baseline,
            glyph_count: info.glyph_count,
            strike_count: info.strike_count,
            ..Default::default()
        });
        copy(
            ptr::addr_of_mut!((*out).path),
            &info.path[..catalog.path_len],
        );
        copy(
            ptr::addr_of_mut!((*out).family),
            &info.family[..catalog.family_len],
        );
        copy(
            ptr::addr_of_mut!((*out).face),
            &info.face[..catalog.face_len],
        );
        copy(
            ptr::addr_of_mut!((*out).style),
            &info.style[..catalog.style_len],
        );
        copy(
            ptr::addr_of_mut!((*out).status),
            &info.status[..catalog.status_len],
        );
        1
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_gui_font_set(id: u32) -> i32 {
    unsafe {
        let Some(producer) = Producer::enter() else {
            return -1;
        };
        if !renderable(id) {
            return -2;
        }
        let gui = process_data::ensure_gui(producer.instance);
        if gui.is_null() {
            return -3;
        }
        AtomicU32::from_ptr(ptr::addr_of_mut!((*gui).font_id)).store(id, Ordering::Release);
        gui_flow::revision(producer.instance);
        0
    }
}
/// # Safety
/// Exclusive canonical font-info output. The exact instance pin ends before
/// catalog lookup, preserving the original value-only selection boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_font_get(id: u32, out: *mut Info) -> i32 {
    unsafe {
        if out.is_null() {
            return -1;
        }
        let selected = if id == 0 {
            let instance = context::current();
            if instance.is_null() {
                return -1;
            }
            let gui = (*instance).gui_payload.cast::<t::Gui>();
            if gui.is_null() {
                c::gui_font_builtin_id
            } else {
                AtomicU32::from_ptr(ptr::addr_of_mut!((*gui).font_id)).load(Ordering::Acquire)
            }
        } else {
            process::r4os_rust_program_process_signal();
            let Some(lease) = Lease::id(id) else {
                return -1;
            };
            let gui = (*lease.0.instance).gui_payload.cast::<t::Gui>();
            if gui.is_null() {
                c::gui_font_builtin_id
            } else {
                AtomicU32::from_ptr(ptr::addr_of_mut!((*gui).font_id)).load(Ordering::Acquire)
            }
        };
        fill(selected, true, out)
    }
}
/// # Safety
/// Exclusive complete canonical info output; catalog values are copied only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_gui_font_info(
    id: u32,
    selected: bool,
    out: *mut Info,
) -> i32 {
    unsafe { fill(id, selected, out) }
}
const _: () = {
    assert!(mem::size_of::<Info>() == 316);
    assert!(mem::size_of::<Catalog>() == 368);
    assert!(mem::align_of::<Catalog>() == 8);
    assert!(mem::offset_of!(Catalog, info) == 0);
    assert!(mem::offset_of!(Catalog, reserved) == 316);
    assert!(mem::offset_of!(Catalog, path_len) == 320);
    assert!(mem::offset_of!(Catalog, family_len) == 328);
    assert!(mem::offset_of!(Catalog, face_len) == 336);
    assert!(mem::offset_of!(Catalog, style_len) == 344);
    assert!(mem::offset_of!(Catalog, status_len) == 352);
    assert!(mem::offset_of!(Catalog, renderable) == 360);
    assert!(mem::offset_of!(Catalog, selected) == 361);
    assert!(mem::offset_of!(Catalog, reserved2) == 362);
    assert!(mem::size_of::<Measurement>() == 16);
    assert!(mem::align_of::<Measurement>() == 4);
    assert!(mem::offset_of!(Measurement, width) == 0);
    assert!(mem::offset_of!(Measurement, height) == 4);
    assert!(mem::offset_of!(Measurement, baseline) == 8);
    assert!(mem::offset_of!(Measurement, line_height) == 12);
};
