//! Sole privileged display bridge. The R4D owns physical device commands;
//! common display execution and the reentrant bridge guard retain exact BO,
//! mode, reset and restored-console identities until their real receipts.
#![allow(clippy::too_many_arguments)]
use super::{
    backend_state as backend,
    core_types::{self as core_display, Backend, BootSnapshot, Rect, Target, Text},
    cursor_work,
    equal::same,
    error::Error,
    framebuffer::{self, Framebuffer},
    manager as display, mode_work, native_additional_mode,
    native_types::ModeBinding,
    output_runtime, outputs,
    ownership::{self, Resident},
    queue,
    queue_resources::{self as resource, DISPLAY_OWNER as OWNER},
    queue_state as model,
};
use crate::{
    contract as a,
    driver::{api_admission, irq_router},
    gfx::{
        buffer_api, buffers, layout,
        types::{Handle, Owner},
    },
    memory::paging,
    platform::{monotonic, pci_inventory as pci, timer},
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    fmt::{self, Write},
    mem::size_of,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
type Callback = unsafe extern "C" fn(u64, u64, *const a::GfxNativeBootInfo) -> i32;
// The old inactive bridge had an undefined framebuffer. Keep initialized raw
// storage in Rust; it is never used before a real saved boot frame is bound.
const EMPTY_FRAME: Framebuffer = Framebuffer {
    address: ptr::null_mut(),
    width: 0,
    height: 0,
    pitch: 0,
    bpp: 0,
    memory_model: 0,
    red_mask_size: 0,
    red_mask_shift: 0,
    green_mask_size: 0,
    green_mask_shift: 0,
    blue_mask_size: 0,
    blue_mask_shift: 0,
    unused: [0; 5],
    edid_size: 0,
    edid: ptr::null(),
};
#[derive(Clone, Copy)]
struct Bridge {
    driver_owner: Owner,
    registration: a::GfxNativeRegistration,
    reference: Handle,
    cpu_lease: Handle,
    cpu_refresh_required: bool,
    cpu_retry_reported: bool,
    timeline: u64,
    pending: Option<model::Fence>,
    image_pending: Option<model::Fence>,
    image_direct: bool,
    bytes: u64,
    frame: Framebuffer,
    output_format: u32,
    encoded_output: bool,
    held_generation: u64,
    generation: u64,
    ready: bool,
    cancelled: bool,
    hardware_restored: bool,
    pixels_restored: bool,
    queue_stopped: bool,
    modes_enabled: bool,
    driver_reference: Handle,
}
impl Bridge {
    const EMPTY: Self = Self {
        driver_owner: Owner {
            kind: 2,
            id: 0,
            generation: 0,
        },
        registration: a::GfxNativeRegistration {
            version: 1,
            size: 128,
            backend: a::GfxBackendBinding {
                version: 1,
                size: 32,
                adapter_id: 0,
                milestone: 0,
                device_generation: 0,
                reset_generation: 0,
            },
            output: a::GfxOutputId {
                adapter_id: 0,
                connector_id: 0,
                device_generation: 0,
                connection_generation: 0,
            },
            reference: a::GfxBufferHandle {
                id: 0,
                reserved0: 0,
                generation: 0,
            },
            name: [0; 24],
            context: 0,
            commit_callback: 0,
            restore_callback: 0,
        },
        reference: Handle::EMPTY,
        cpu_lease: Handle::EMPTY,
        cpu_refresh_required: false,
        cpu_retry_reported: false,
        timeline: 0,
        pending: None,
        image_pending: None,
        image_direct: false,
        bytes: 0,
        frame: EMPTY_FRAME,
        output_format: layout::XRGB8888,
        encoded_output: false,
        held_generation: 0,
        generation: 0,
        ready: false,
        cancelled: false,
        hardware_restored: false,
        pixels_restored: false,
        queue_stopped: false,
        modes_enabled: false,
        driver_reference: Handle::EMPTY,
    };
}
#[derive(Clone, Copy)]
struct Surface {
    reference: Handle,
    driver_reference: Handle,
    read_lease: Handle,
    bytes: u64,
    frame: Framebuffer,
    output_format: u32,
    encoded_output: bool,
}
#[derive(Clone, Copy)]
struct Replacement {
    old: Surface,
    new: Surface,
    using_new: bool,
}
#[derive(Clone, Copy)]
struct ResetRecord {
    driver: Owner,
    backend: a::GfxBackendBinding,
    original_generation: u64,
    generation: u64,
    retired: bool,
    terminal_released: bool,
    resumed_backend: Option<a::GfxBackendBinding>,
    reported_step: ResetStep,
    reported_result: i32,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ResetStep {
    None,
    Queue,
    Mode,
    Cursor,
    Output,
    Display,
    Bridge,
    Retired,
}
impl ResetStep {
    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Queue => "queue",
            Self::Mode => "mode",
            Self::Cursor => "cursor",
            Self::Output => "output",
            Self::Display => "display",
            Self::Bridge => "bridge",
            Self::Retired => "retired",
        }
    }
}
#[derive(Clone, Copy)]
struct ConsoleReceipt {
    driver: Owner,
    adapter: u32,
    reset_generation: u64,
    restored_generation: u64,
}
static BRIDGE: Resident<Bridge> = Resident(UnsafeCell::new(Bridge::EMPTY));
static REPLACEMENT: Resident<Option<Replacement>> = Resident(UnsafeCell::new(None));
static RESET: Resident<Option<ResetRecord>> = Resident(UnsafeCell::new(None));
static CONSOLE: Resident<Option<ConsoleReceipt>> = Resident(UnsafeCell::new(None));
static EXECUTION: Resident<crate::sched::sync_types::UnwindGuard> = Resident(UnsafeCell::new(
    ownership::execution(b"native-display-driver"),
));
static RETAINED: AtomicU32 = AtomicU32::new(0);
fn execution() -> Result<ownership::RecursiveExecution, Error> {
    unsafe { ownership::RecursiveExecution::acquire(EXECUTION.0.get()) }
}
struct BoLock;
impl BoLock {
    fn acquire() -> Self {
        buffers::lock();
        Self
    }
    fn prepared(references: usize, leases: usize) -> Result<Self, Error> {
        buffers::lock_prepared(buffers::Need {
            objects: 0,
            references,
            leases,
        })?;
        Ok(Self)
    }
}
impl Drop for BoLock {
    fn drop(&mut self) {
        buffers::unlock()
    }
}
struct DisplayExecution;
impl DisplayExecution {
    fn acquire() -> Result<Self, Error> {
        if display::begin_output_commit() {
            Ok(Self)
        } else {
            Err(Error::Busy)
        }
    }
}
impl Drop for DisplayExecution {
    fn drop(&mut self) {
        display::end_output_commit()
    }
}
pub fn binding(value: a::GfxBackendBinding) -> model::Binding {
    model::Binding {
        adapter: value.adapter_id,
        device_generation: value.device_generation,
        reset_generation: value.reset_generation,
    }
}
pub fn retained(id: u32) -> bool {
    id != 0 && RETAINED.load(Ordering::Acquire) == id
}
// Read only under common DisplayExecution; replacement changes also require it.
pub fn mode_pending() -> bool {
    unsafe {
        match *REPLACEMENT.0.get() {
            None => false,
            Some(change) => {
                !change.using_new
                    || !mode_work::presentation_ready(
                        (*BRIDGE.0.get()).driver_owner,
                        (*BRIDGE.0.get()).registration.backend,
                    )
            }
        }
    }
}
pub fn code(error: Error) -> i32 {
    match error {
        Error::Busy => a::gfx_output_error_busy,
        Error::Stale | Error::WrongOwner => a::gfx_output_error_stale,
        Error::Disabled | Error::Unsupported => a::gfx_output_error_unsupported,
        Error::Unavailable => a::gfx_output_error_unavailable,
        Error::OutOfMemory | Error::Capacity => a::gfx_output_error_capacity,
        _ => a::gfx_output_error_invalid,
    }
}
// Log transport remains with the existing kernel log sink. This thunk has no
// state and preserves raw serial plus explicit boot-ring publication exactly.

struct Message<const N: usize> {
    bytes: [u8; N],
    len: usize,
}
impl<const N: usize> Message<N> {
    fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
    fn emit(&self) {
        unsafe { native_display_log(self.bytes.as_ptr(), self.len) }
    }
    fn text(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).expect("ASCII diagnostic")
    }
}
impl<const N: usize> Write for Message<N> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if text.len() > N - self.len {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + text.len()].copy_from_slice(text.as_bytes());
        self.len += text.len();
        Ok(())
    }
}
fn state_name(value: u32) -> &'static str {
    match value {
        0 => "unavailable",
        1 => "bootfb",
        2 => "preparing",
        3 => "native",
        4 => "software_native",
        5 => "recovering",
        _ => unreachable!(),
    }
}
fn reason_name(value: u32) -> &'static str {
    match value {
        0 => "none",
        1 => "no_native_backend",
        2 => "policy_disabled",
        3 => "backend_rejected",
        4 => "prepare_failed",
        5 => "commit_failed",
        6 => "device_lost",
        7 => "restore_failed",
        _ => unreachable!(),
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Args {
    pub owner: Owner,
    pub backend: a::GfxBackendBinding,
    pub generation: u64,
    pub held: u64,
    pub reset: u64,
    pub value: u32,
    pub other: u32,
    pub input: *const c_void,
    pub second: *const c_void,
}
#[repr(C)]
pub struct Prepare {
    pub assignment: a::GfxScanoutState,
    pub mode: a::GfxOutputMode,
    pub has_format: bool,
    pub format: u32,
}
/// # Safety
/// Existing Kernel facades retain exact fixed records and pointer payloads.
/// Public R4D operations validate their original headers before full access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_native_driver(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { args.read() };
    let public = unsafe {
        match op {
            0 => Some(boot_info(out.cast())),
            5..=7 => Some(if op == 6 && args.held == 0 || op == 7 && args.reset == 0 {
                a::gfx_output_error_stale
            } else {
                prepare_request(
                    args.owner,
                    args.input.cast(),
                    args.held,
                    args.reset,
                    out.cast(),
                )
            }),
            8 => Some(transition(
                args.value,
                args.generation,
                args.other,
                out.cast(),
            )),
            21 => Some(device_reset(
                args.owner,
                args.input.cast(),
                args.generation,
                args.value,
                out.cast(),
            )),
            22 => Some(resume_headless(
                args.owner,
                args.input.cast(),
                args.generation,
                out.cast(),
            )),
            23 => Some(release_terminal(
                args.owner,
                args.input.cast(),
                args.generation,
                out.cast(),
            )),
            _ => None,
        }
    };
    if let Some(public) = public {
        return public;
    }
    let result = (|| -> Result<(), Error> {
        unsafe {
            match op {
                1 => out.cast::<bool>().write(retained(args.value)),
                2 => out.cast::<bool>().write(mode_pending()),
                3 => out.cast::<bool>().write(valid_adapter(args.value)),
                4 => out.cast::<a::GfxNativeBootInfo>().write(boot_description(
                    args.generation,
                    &args.input.cast::<BootSnapshot>().read(),
                )),
                9 => out
                    .cast::<a::GfxOutputTarget>()
                    .write(output_target(args.value, args.other)?),
                10 => {
                    let image = args.input.cast::<output_runtime::Image>().read();
                    let dependencies = if image.dependencies_len == 0 {
                        &[]
                    } else {
                        core::slice::from_raw_parts(image.dependencies, image.dependencies_len)
                    };
                    let status = submit_image(
                        args.owner,
                        args.generation,
                        &resource::Submission {
                            deadline_ns: image.deadline,
                            frame_key: image.frame_key,
                            dependencies,
                        },
                        &*image.request,
                    )?;
                    out.cast::<model::Status>().write(status);
                }
                11 => enable_modes(args.owner, args.backend)?,
                12 => out
                    .cast::<bool>()
                    .write(modes_enabled(args.value, args.backend)),
                13 => out.cast::<ModeBinding>().write(mode_binding(
                    args.input.cast::<a::GfxScanoutState>().read(),
                )?),
                14 => {
                    let input = &*args.input.cast::<Prepare>();
                    out.cast::<a::GfxBufferReference>()
                        .write(prepare_mode_encoding(
                            args.owner,
                            input.assignment,
                            input.mode,
                            if input.has_format {
                                Some(input.format)
                            } else {
                                None
                            },
                        )?);
                }
                15 => start_mode(args.value, args.input.cast::<ModeBinding>().read())?,
                16 => settle_mode(args.value, args.other)?,
                17 => settle_mode_for(
                    args.value,
                    args.other,
                    args.input.cast::<ModeBinding>().read(),
                )?,
                18 => abort_prepared_mode(args.input.cast::<ModeBinding>().read()),
                19 => retire_mode_after_reset(args.owner, args.backend)?,
                20 => out.cast::<cursor_work::Binding>().write(cursor_binding()?),
                _ => panic!("native display operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

// Private projections for the existing console-restoration oracle only. No
// fixture backup or state inspection API is compiled into the product image.

const _: () = {
    assert!(size_of::<Args>() == 104);
    assert!(core::mem::offset_of!(Args, backend) == 24);
    assert!(core::mem::offset_of!(Args, generation) == 56);
    assert!(core::mem::offset_of!(Args, held) == 64);
    assert!(core::mem::offset_of!(Args, reset) == 72);
    assert!(core::mem::offset_of!(Args, value) == 80);
    assert!(core::mem::offset_of!(Args, other) == 84);
    assert!(core::mem::offset_of!(Args, input) == 88);
    assert!(core::mem::offset_of!(Args, second) == 96);
    assert!(size_of::<cursor_work::Binding>() == 88);
};

unsafe fn native_display_log(bytes: *const u8, len: usize) {
    unsafe {
        crate::boot::log_route::r4os_rust_log_write(1, bytes, len);
        crate::boot::log::r4os_rust_bootlog_write(bytes, len)
    };
}

// Private operation modules share the sole state and original typed owners.
mod cpu_access;
mod modes;
mod preparation;
mod reset_retirement;
use cpu_access::{begin_cpu, discard, end_cpu, image_idle};
pub use cpu_access::{cursor_binding, output_target, submit_image};
pub use modes::{
    abort_prepared_mode, enable_modes, mode_binding, modes_enabled, prepare_mode_encoding,
    retire_mode_after_reset, settle_mode, settle_mode_for, start_mode,
};
use preparation::driver_id;
pub use preparation::{boot_info, prepare_request, transition, valid_adapter};
pub use reset_retirement::{boot_description, device_reset, release_terminal, resume_headless};
use reset_retirement::{require_quiesced_fence, restore_after_reset};
