//! Original remaining R4D entrypoint admission and backend adapters.
use crate::{
    boot::log,
    contract as a,
    driver::{
        api_admission as admission, api_msi as msi, api_validation as validation, irq_router, work,
    },
};
use core::{ffi::c_void, ptr};
unsafe extern "C" {
    fn r4os_zig_driver_log(level: u8, owner: u32, text: *const u8);
    fn r4os_zig_driver_usb(
        op: u8,
        name: *const u8,
        len: usize,
        owner: u32,
        source: u32,
        descriptor: *const a::UsbHostController,
    ) -> isize;
    fn r4os_zig_driver_blit(
        register: bool,
        name: *const u8,
        len: usize,
        owner: u32,
        descriptor: *const c_void,
    ) -> i32;
}
fn puts(s: &[u8]) {
    unsafe { log::r4os_rust_bootlog_write(s.as_ptr(), s.len()) }
}
unsafe fn put_z(text: *const u8) {
    let mut i = 0;
    while unsafe { *text.add(i) } != 0 && i < 512 {
        unsafe { log::r4os_rust_bootlog_write(text.add(i), 1) };
        i += 1;
    }
}
unsafe fn text<'a>(name: *const u8) -> &'a [u8] {
    let mut n = 0;
    while n < 512 && unsafe { *name.add(n) } != 0 {
        n += 1;
    }
    unsafe { core::slice::from_raw_parts(name, n) }
}
/// # Safety
/// The resident readable zero-terminated text retains its original bounded
/// logging lifetime and genuine calling driver/thread identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_log(level: u8, text: *const u8) {
    unsafe {
        r4os_zig_driver_log(
            level,
            admission::r4os_rust_driver_api_runtime_owner(true),
            text,
        )
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_irq_legacy(release: bool) -> i32 {
    puts(if release {
        b"[R4D][WARN] release_irq not implemented yet\r\n"
    } else {
        b"[R4D][WARN] request_irq not implemented yet\r\n"
    });
    -1
}
/// # Safety
/// The exact executable IRQ handler remains resident through unregister; MSI-X
/// failure rolls back only this handler and retains its hardware ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_irq_register(
    irq: u8,
    handler: irq_router::Handler,
    context: usize,
    flags: u32,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    let result =
        unsafe { irq_router::r4os_rust_irq_router_register(irq, handler, context, flags, owner) };
    if result != 0 {
        return result;
    }
    if flags & 4 != 0 && !msi::r4os_rust_driver_msi_unmask(owner, irq) {
        unsafe { irq_router::r4os_rust_irq_router_unregister(irq, handler, context) };
        return -5;
    }
    0
}
/// # Safety
/// Resident request and output have the exact declared driver Work C layouts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_api_work_request(
    request: *const work::Request,
    out: *mut u32,
) -> i32 {
    unsafe {
        work::r4os_rust_driver_work_request(
            admission::r4os_rust_driver_api_runtime_owner(false),
            request,
            out,
        )
    }
}
/// # Safety
/// Resident name and descriptor keep their original prefix and USB core
/// lifetime through actual registration/unregister acknowledgement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_usb_register(
    name: *const u8,
    d: *const a::UsbHostController,
) -> i32 {
    if !unsafe { validation::r4os_rust_driver_usb_valid(d) } {
        return -1;
    }
    let slice = unsafe { text(name) };
    if unsafe { r4os_zig_driver_usb(0, slice.as_ptr(), slice.len(), 0, 0, ptr::null()) } != 0 {
        puts(b"[R4D][ERROR] usb host duplicate name ");
        unsafe { put_z(name) };
        puts(b"\r\n");
        return -2;
    }
    let index = unsafe {
        r4os_zig_driver_usb(
            1,
            slice.as_ptr(),
            slice.len(),
            admission::r4os_rust_driver_api_owner_scalar(0),
            0,
            d,
        )
    };
    if index < 0 {
        -3
    } else {
        i32::try_from(index).expect("Original bounded USB host index")
    }
}
/// # Safety
/// The zero-terminated name remains readable through its original USB callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_usb_unregister(name: *const u8) -> i32 {
    let slice = unsafe { text(name) };
    if unsafe {
        r4os_zig_driver_usb(
            2,
            slice.as_ptr(),
            slice.len(),
            admission::r4os_rust_driver_api_runtime_owner(false),
            0,
            ptr::null(),
        )
    } != 0
    {
        0
    } else {
        -1
    }
}
/// # Safety
/// The zero-terminated name is readable under its original activation admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_usb_activate(name: *const u8, source: u32) -> i32 {
    for (i, expected) in b"XHCI".iter().enumerate() {
        if !unsafe { *name.add(i) }.eq_ignore_ascii_case(expected) {
            return -1;
        }
    }
    if unsafe { *name.add(4) } != 0 {
        return -1;
    }
    if source != a::usb_host_source_preload && source != a::usb_host_source_disk {
        return -2;
    }
    (unsafe {
        r4os_zig_driver_usb(
            3,
            ptr::null(),
            0,
            admission::r4os_rust_driver_api_runtime_owner(false),
            source,
            ptr::null(),
        )
    }) as i32
}
/// # Safety
/// The resident name/descriptor retain the admitted blit backend lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_blit(
    register: bool,
    name: *const u8,
    d: *const c_void,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    if register && (owner == 0 || d.is_null()) {
        return -1;
    }
    let slice = unsafe { text(name) };
    let result = unsafe { r4os_zig_driver_blit(register, slice.as_ptr(), slice.len(), owner, d) };
    if register && result == 0 {
        puts(b"[R4D] register display blit backend ");
        unsafe { put_z(name) };
        puts(b"\r\n");
    }
    result
}
/// # Safety
/// Resident synth descriptor keeps its declared prefix under the original
/// synchronous audio registration and actual backend lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_synth(name: *const u8, d: *const a::SynthEngine) -> i32 {
    if !unsafe { validation::r4os_rust_driver_synth_valid(d) } {
        return -1;
    }
    unsafe {
        crate::audio::core::register_external_synth_engine_z(
            name,
            crate::audio::types::SynthRegistration {
                flags: ptr::addr_of!((*d).flags).read(),
                context: ptr::addr_of!((*d).context).read(),
                send: ptr::addr_of!((*d).midi_send).read(),
                render: ptr::addr_of!((*d).render).read(),
                stop: ptr::addr_of!((*d).stop).read(),
                status: ptr::addr_of!((*d).status).read(),
                opl3_reset: ptr::addr_of!((*d).opl3_reset).read(),
                opl3_write: ptr::addr_of!((*d).opl3_write_register).read(),
                sid_acquire: ptr::addr_of!((*d).sid_acquire).read(),
                sid_release: ptr::addr_of!((*d).sid_release).read(),
                sid_model: ptr::addr_of!((*d).sid_set_model).read(),
                sid_write: ptr::addr_of!((*d).sid_write_register).read(),
                sid_load: ptr::addr_of!((*d).sid_load_data).read(),
                sid_init: ptr::addr_of!((*d).sid_init).read(),
                sid_play: ptr::addr_of!((*d).sid_play_frame).read(),
                sid_pcm: ptr::addr_of!((*d).sid_render_pcm).read(),
                render_pcm: validation::r4os_rust_driver_synth_pcm(d),
                external: true,
                ..Default::default()
            },
        )
    }
}
/// # Safety
/// The readable name preserves the original informational input registration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_input(name: *const u8) -> i32 {
    puts(b"[R4D] register input backend ");
    unsafe { put_z(name) };
    puts(b"\r\n");
    0
}
