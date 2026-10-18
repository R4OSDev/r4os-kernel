//! Sole display manager: one presentation/transition execution owner, short
//! published metadata snapshots, exact physical recovery and retained images.
#![allow(clippy::too_many_arguments)]
use super::{
    backend_state as backend, blit_backend,
    core_types::*,
    error::Error,
    firmware,
    framebuffer::{self as fb, Framebuffer},
    ownership::{self, Resident},
    presentation_stats, surface_pipeline,
};
use crate::{
    contract as a, font,
    memory::paging,
    platform::{cpu, timer},
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr, slice};
pub const NONE: u8 = 0;
pub const BOOTFB: u8 = 1;
pub const NATIVE: u8 = 2;
pub(crate) const VISIBLE: u32 = 1;
pub(crate) const FIXED_MODE: u32 = 2;
pub(crate) const CPU_PRESENT: u32 = 4;
pub(crate) const XRGB32: u32 = 1 << 7;
const REASON_NONE: u8 = 0;
const REASON_FILL: u8 = 1;
const REASON_RECT: u8 = 2;
const REASON_PACKED32: u8 = 3;
const REASON_XRGB32: u8 = 4;
#[derive(Clone, Copy)]
struct Device {
    value: Stats,
    frame: *mut Framebuffer,
    ops: &'static DeviceOps,
}
// Native framebuffer operations; common XRGB presentation uses regions.
#[allow(clippy::type_complexity)]
#[derive(Default)]
struct DeviceOps {
    fill: Option<fn(*mut Device, u32) -> bool>,
    rect: Option<fn(*mut Device, i32, i32, u32, u32, u32) -> bool>,
    present_packed32_rect: Option<unsafe fn(*mut Device, u64, u64, u64, u64, &[u8], u64) -> bool>,
    put_packed32: Option<fn(*mut Device, u64, u64, u32) -> bool>,
    put_xrgb32: Option<fn(*mut Device, u64, u64, u32) -> bool>,
}
static EMPTY_OPS: DeviceOps = DeviceOps {
    fill: None,
    rect: None,
    present_packed32_rect: None,
    put_packed32: None,
    put_xrgb32: None,
};
static BOOTFB_OPS: DeviceOps = DeviceOps {
    fill: Some(bootfb_fill),
    rect: Some(bootfb_rect),
    present_packed32_rect: Some(bootfb_present_packed32_rect),
    put_packed32: Some(bootfb_put_packed32),
    put_xrgb32: Some(bootfb_put_xrgb32),
};
impl Device {
    const fn empty(builtin_ops: bool) -> Self {
        Self {
            value: Stats::EMPTY,
            frame: ptr::null_mut(),
            ops: if builtin_ops { &BOOTFB_OPS } else { &EMPTY_OPS },
        }
    }
    fn target(target: Target) -> Self {
        Self {
            value: Stats::target(target),
            frame: target.framebuffer,
            ops: &BOOTFB_OPS,
        }
    }
}
#[derive(Clone, Copy)]
struct HeldBoot {
    holder: Holder,
    generation: u64,
    captured: bool,
    effects: bool,
    restored: bool,
}
struct Context {
    boot: Device,
    native: Device,
    primary: *mut Device,
    present_generation: u64,
    completed_fence: u64,
    system_transition: bool,
    completed_stats: Stats,
    presentation: presentation_stats::Owner,
    manager: backend::Manager,
    completed_backend: backend::Snapshot,
    completed_boot_mode: Mode,
    completed_boot_mapping: Mapping,
    completed_firmware_writable: bool,
    completed_output_name: [u8; blit_backend::NAME_BYTES],
    native_backend: Option<Backend>,
    saved_boot: Option<BootSnapshot>,
    native_hold_generation: u64,
    native_reset_generation: u64,
    native_restore_confirmed: bool,
    retired_reset_generation: u64,
    held_boot: Option<HeldBoot>,
}
const EMPTY_BACKEND: backend::Snapshot = backend::Snapshot {
    state: 0,
    policy: 0,
    reason: 1,
    revision: 0,
    generation: 0,
    reset_generation: 0,
    owner: 0,
    adapter_id: 0,
    pending_owner: 0,
    pending_adapter_id: 0,
    pending_generation: 0,
};
static DATA: Resident<Context> = Resident(UnsafeCell::new(Context {
    boot: Device::empty(true),
    native: Device::empty(false),
    primary: ptr::null_mut(),
    present_generation: 0,
    completed_fence: 0,
    system_transition: false,
    completed_stats: Stats::EMPTY,
    presentation: presentation_stats::Owner {
        driver: 0,
        driver_generation: 0,
        display_generation: 0,
        backend: a::GfxBackendBinding {
            version: 1,
            size: 32,
            adapter_id: 0,
            milestone: 0,
            device_generation: 0,
            reset_generation: 0,
        },
        heads: [presentation_stats::Slot::EMPTY; presentation_stats::CAPACITY],
        infos: [presentation_stats::Slot::EMPTY; presentation_stats::CAPACITY],
        history: [presentation_stats::Slot::EMPTY; 32],
        history_cursor: 0,
    },
    manager: backend::Manager {
        value: EMPTY_BACKEND,
        serial: 0,
        recovering_preparation: false,
        has_restored_boot: false,
        restored_boot: backend::Restored {
            owner: 0,
            adapter: 0,
            generation: 0,
        },
    },
    completed_backend: EMPTY_BACKEND,
    completed_boot_mode: Mode::EMPTY,
    completed_boot_mapping: Mapping::EMPTY,
    completed_firmware_writable: true,
    completed_output_name: [0; blit_backend::NAME_BYTES],
    native_backend: None,
    saved_boot: None,
    native_hold_generation: 0,
    native_reset_generation: 0,
    native_restore_confirmed: false,
    retired_reset_generation: 0,
    held_boot: None,
}));
static EXECUTION: Resident<crate::sched::sync_types::UnwindGuard> =
    Resident(UnsafeCell::new(ownership::execution(b"display-present")));
// Exact execution owns device/transition fields; the short state owner covers
// published fields and presentation statistics. Resident defaults are complete
// from image load, as before. No full Context borrow crosses another owner.
fn context() -> *mut Context {
    DATA.0.get()
}
fn execution() -> Result<ownership::Execution, Error> {
    unsafe { ownership::Execution::acquire(EXECUTION.0.get()) }
}
struct OutputCommit;
impl OutputCommit {
    fn enter() -> Result<Self, Error> {
        if begin_output_commit() {
            Ok(Self)
        } else {
            Err(Error::Busy)
        }
    }
}
impl Drop for OutputCommit {
    fn drop(&mut self) {
        end_output_commit()
    }
}
pub fn begin_output_commit() -> bool {
    if !unsafe { ownership::enter(EXECUTION.0.get(), 0) } {
        return false;
    }
    if unsafe { (*context()).system_transition } {
        unsafe { ownership::leave(EXECUTION.0.get()) };
        return false;
    }
    true
}
pub fn end_output_commit() {
    unsafe { ownership::leave(EXECUTION.0.get()) }
}
pub fn begin_system_transition(timeout_ticks: u64) -> bool {
    if !unsafe { ownership::enter(EXECUTION.0.get(), timeout_ticks) } {
        return false;
    }
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            end_output_commit()
        }
    }
    let _leave = Leave;
    let p = context();
    if unsafe { (*p).system_transition } {
        return true;
    }
    if !firmware::r4os_rust_display_firmware_gate.is_revoked()
        && !firmware::r4os_rust_display_firmware_gate.try_revoke()
    {
        return false;
    }
    unsafe {
        (*p).system_transition = true;
        (*p).primary = ptr::null_mut();
    }
    publish_stats();
    true
}
pub fn system_transition_quiesced() -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let p = context();
    unsafe {
        (*p).system_transition
            && (*p).primary.is_null()
            && (*p).held_boot.is_none()
            && (*p).native_backend.is_none()
            && (*p).manager.value.owner == 0
            && (*p).manager.value.pending_owner == 0
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BackendView {
    pub state: backend::Snapshot,
    pub device: Stats,
    pub boot_mode: Mode,
    pub boot_mapping: Mapping,
    pub firmware_writable: bool,
    pub name: [u8; blit_backend::NAME_BYTES],
}
pub fn backend_view() -> BackendView {
    let _state = ownership::enter_state();
    let p = context();
    unsafe {
        BackendView {
            state: (*p).completed_backend,
            device: (*p).completed_stats,
            boot_mode: (*p).completed_boot_mode,
            boot_mapping: (*p).completed_boot_mapping,
            firmware_writable: (*p).completed_firmware_writable,
            name: (*p).completed_output_name,
        }
    }
}
pub fn backend_state() -> backend::Snapshot {
    let _state = ownership::enter_state();
    unsafe { (*context()).completed_backend }
}
pub fn boot_snapshot() -> Option<BootSnapshot> {
    let _state = ownership::enter_state();
    unsafe { (*context()).saved_boot }
}
pub fn retains_driver_owner(owner: usize) -> bool {
    let current = backend_state();
    owner != 0 && (current.owner == owner || current.pending_owner == owner)
}
pub fn set_backend_policy(policy: u32) -> bool {
    let Ok(_commit) = OutputCommit::enter() else {
        return false;
    };
    if unsafe { &mut (*context()).manager }
        .set_policy(policy)
        .is_err()
    {
        return false;
    }
    publish_stats();
    true
}
pub fn reject_native_backend() {
    let Ok(_execution) = execution() else {
        return;
    };
    unsafe { &mut (*context()).manager }.reject(3);
    publish_stats()
}

struct Publish {
    boot_mapping: Option<Mapping>,
}
impl Drop for Publish {
    fn drop(&mut self) {
        publish_stats_with_mapping(self.boot_mapping)
    }
}

pub fn stats() -> Stats {
    let _state = ownership::enter_state();
    unsafe { (*context()).completed_stats }
}
/// Copy the original completed stats and name before releasing publication
/// admission. No borrow into the native backend's label escapes this probe.
pub(crate) fn diagnostic_stats() -> (Stats, [u8; 32]) {
    let _state = ownership::enter_state();
    let mut value = unsafe { (*context()).completed_stats };
    let mut name = [0; 32];
    let len = value.name.len.min(name.len() - 1);
    if len != 0 {
        unsafe {
            name[..len].copy_from_slice(core::slice::from_raw_parts(value.name.pointer, len));
        }
    }
    value.name = Text::literal(b"");
    (value, name)
}

pub fn bind_presentation_stats(
    owner: usize,
    owner_generation: u64,
    binding: a::GfxBackendBinding,
    generation: u64,
) -> Result<(), Error> {
    let _state = ownership::enter_state();
    let p = context();
    let current = unsafe { (*p).completed_backend };
    if current.pending_owner != owner
        || current.pending_generation != generation
        || current.pending_adapter_id != binding.adapter_id
    {
        return Err(Error::Stale);
    }
    unsafe { &mut (*p).presentation }.bind(owner, owner_generation, binding, generation)
}
pub fn publish_presentation_stats(
    owner: usize,
    owner_generation: u64,
    value: a::DisplayPresentationStats,
) -> Result<(), Error> {
    let _state = ownership::enter_state();
    let p = context();
    unsafe { &mut (*p).presentation }.publish(owner, owner_generation, value, unsafe {
        (*p).completed_backend
    })
}
pub fn presentation_stats(head: u32) -> Result<a::DisplayPresentationStats, Error> {
    let _state = ownership::enter_state();
    let p = context();
    unsafe { &(*p).presentation }.read(head, unsafe { (*p).completed_backend })
}
pub fn publish_presentation_info(
    owner: usize,
    owner_generation: u64,
    value: a::DisplayPresentationInfo,
) -> Result<(), Error> {
    let _state = ownership::enter_state();
    let p = context();
    unsafe { &mut (*p).presentation }.publish_info(owner, owner_generation, value, unsafe {
        (*p).completed_backend
    })
}
pub fn presentation_info(head: u32) -> Result<a::DisplayPresentationInfo, Error> {
    let _state = ownership::enter_state();
    let p = context();
    let current = unsafe { (*p).completed_backend };
    if let Ok(value) = unsafe { &(*p).presentation }.read_info(head, current) {
        return Ok(value);
    }
    let value = unsafe { (*p).completed_stats };
    let mode = value.mode;
    if head != 0 || mode.width == 0 || mode.height == 0 {
        return Err(Error::Unsupported);
    }
    Ok(a::DisplayPresentationInfo {
        flags: if current.state == backend::RECOVERING {
            a::display_presentation_info_lost
        } else if !value.registered || value.flags & VISIBLE == 0 {
            a::display_presentation_info_occluded
        } else {
            a::display_presentation_info_active
        },
        display_generation: current.generation.max(1),
        sequence: 1,
        width: mode.width,
        height: mode.height,
        format: a::gfx_buffer_format_xrgb8888,
        policies: 7,
        buffer_count: 2,
        plane_count: 1,
        ..a::DisplayPresentationInfo::default()
    })
}
pub fn presentation_feedback(
    head: u32,
    source: a::GfxFence,
) -> Result<a::DisplayPresentationStats, Error> {
    let _state = ownership::enter_state();
    let p = context();
    unsafe { &(*p).presentation }.feedback(head, source, unsafe { (*p).completed_backend })
}
fn publish_stats() {
    publish_stats_with_mapping(None)
}
// Original receipt/validation branch scopes retained during the port.
#[allow(clippy::collapsible_if)]
fn publish_stats_with_mapping(boot_mapping: Option<Mapping>) {
    unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)) };
    let value = capture_stats();
    let p = context();
    {
        let _state = ownership::enter_state();
        // Publish the cache update under the same owner that boot_snapshot()
        // uses. It consumes the existing publication lock, without another
        // admission or any change to the hardware/cache operation itself.
        if let Some(mapping) = boot_mapping {
            if let Some(boot) = unsafe { &mut (*p).saved_boot } {
                boot.mapping = mapping;
            }
        }
        unsafe {
            (*p).completed_stats = value;
        }
        publish_geometry_locked(value);
        unsafe {
            (*p).completed_backend = (*p).manager.value;
        }
        if let Some(boot) = unsafe { (*p).saved_boot } {
            unsafe {
                (*p).completed_boot_mode = boot.mode;
                (*p).completed_boot_mapping = boot.mapping;
            }
        }
        unsafe {
            (*p).completed_firmware_writable =
                !firmware::r4os_rust_display_firmware_gate.is_revoked();
        }
        let output_name = if unsafe { (*p).manager.value.owner } != 0 {
            unsafe { (*p).native_backend }.map_or(Text::default(), |backend| backend.target.name)
        } else {
            value.name
        };
        unsafe { copy_name(&mut (*p).completed_output_name, output_name.bytes()) };
    }

    super::outputs::boot_active(value.registered && value.kind == BOOTFB);
}
fn publish_geometry_locked(value: Stats) {
    surface_pipeline::publish_target_locked(if value.registered {
        surface_pipeline::Target {
            width: u64::from(value.mode.width),
            height: u64::from(value.mode.height),
            pitch: u64::from(value.mode.pitch),
            bpp: value.mode.bpp,
        }
    } else {
        surface_pipeline::Target::default()
    });
}
fn capture_stats() -> Stats {
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        Stats::default()
    } else {
        Stats {
            registered: true,
            ..unsafe { (*device).value }
        }
    }
}
struct CpuWrite {
    callback: Option<EndCpu>,
    context: usize,
    active: bool,
    damage: Option<Rect>,
}
impl CpuWrite {
    fn finish(&mut self, changed: bool) -> bool {
        if !self.active {
            return true;
        }
        self.active = false;
        let Some(callback) = self.callback else {
            return true;
        };
        let result = unsafe {
            callback.call(
                self.context,
                changed,
                self.damage.as_ref().map_or(ptr::null(), ptr::from_ref),
            )
        };
        match result {
            0 => return true,
            1 => return false,
            2 => {}
            _ => panic!("display CPU result enum"),
        }
        let p = context();
        if let Some(native) = unsafe { (*p).native_backend } {
            let _ = restore_boot_locked(native.owner, unsafe { (*p).manager.value.generation });
        }
        false
    }
}
impl Drop for CpuWrite {
    fn drop(&mut self) {
        let _ = self.finish(false);
    }
}

fn record_present(
    device: *mut Device,
    reason: u8,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    converted: bool,
) {
    record_present_aggregate(
        device,
        reason,
        Rect { x, y, w, h },
        u64::from(w) * u64::from(h),
        converted,
    )
}
fn record_present_aggregate(
    device: *mut Device,
    reason: u8,
    bounds: Rect,
    pixels: u64,
    converted: bool,
) {
    let value = unsafe { &mut (*device).value };
    let bytes = pixels * 4;
    value.present_count += 1;
    value.present_pixels_total += pixels;
    value.present_bytes_total += bytes;
    value.last_present_pixels = pixels;
    value.last_present_bytes = bytes;
    value.last_present_rect = bounds;
    value.last_present_reason = reason;
    value.last_present_converted = converted;
    if converted {
        value.conversion_present_count += 1;
    }
    if bounds.x == 0
        && bounds.y == 0
        && bounds.w == value.mode.width
        && bounds.h == value.mode.height
        && pixels == u64::from(value.mode.width) * u64::from(value.mode.height)
    {
        value.full_present_count += 1;
    } else {
        value.partial_present_count += 1;
    }
    match reason {
        REASON_NONE => {}
        REASON_FILL => value.fill_present_count += 1,
        REASON_RECT => value.rect_present_count += 1,
        REASON_PACKED32 => value.packed32_present_count += 1,
        REASON_XRGB32 => value.xrgb32_present_count += 1,
        _ => panic!("present reason enum"),
    }
}
fn record_timing(device: *mut Device, start: u64) {
    record_timing_at(device, start, timer::tick_count())
}
// Preserve the exact original instrumentation arithmetic.
#[allow(clippy::implicit_saturating_sub)]
fn record_timing_at(device: *mut Device, start: u64, end: u64) {
    let value = unsafe { &mut (*device).value };
    let elapsed = if end >= start { end - start } else { 0 };
    value.present_total_ticks = value.present_total_ticks.wrapping_add(elapsed);
    value.present_last_ticks = elapsed;
    if elapsed > value.present_max_ticks {
        value.present_max_ticks = elapsed;
    }
    if elapsed > 1 {
        value.present_slow_count = value.present_slow_count.wrapping_add(1);
    }
}

#[repr(C)]
pub struct Args {
    pub owner: usize,
    pub generation: u64,
    pub other_generation: u64,
    pub adapter: u32,
    pub operation: u32,
    pub value: u64,
    pub x: i64,
    pub y: i64,
    pub w: u64,
    pub h: u64,
    pub color: u32,
    pub background: u32,
    pub count: u32,
    pub stride: u64,
    pub source_generation: u64,
    pub input_tick: u64,
    pub flag: bool,
    pub has_font: bool,
    pub input: *const core::ffi::c_void,
    pub frame: *mut Framebuffer,
    pub regions: *const blit_backend::Region,
    pub regions_len: usize,
}
#[repr(C)]
pub struct Optional<T> {
    pub has: bool,
    pub value: MaybeUninit<T>,
}
fn optional<T>(value: Option<T>) -> Optional<T> {
    Optional {
        has: value.is_some(),
        value: value.map_or(MaybeUninit::uninit(), MaybeUninit::new),
    }
}
/// # Safety
/// Exact initialized fixed C args and matching result. Frame/input spans and
/// callbacks keep their original task/execution lifetime. Locked operations
/// retain the caller's exact DisplayExecution until their completion returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_core(
    op: u32,
    args: *const Args,
    out: *mut core::ffi::c_void,
) -> i32 {
    let args = unsafe { &*args };
    macro_rules! store {
        ($ty:ty,$value:expr) => {{
            let value = $value;
            unsafe { out.cast::<$ty>().write(value) };
            Ok(())
        }};
    }
    let result = match op {
        0 => store!(bool, begin_output_commit()),
        1 => {
            end_output_commit();
            Ok(())
        }
        2 => store!(bool, begin_system_transition(args.value)),
        3 => store!(bool, system_transition_quiesced()),
        4 => store!(BackendView, backend_view()),
        6 => finish_boot(args.owner, args.generation, args.operation)
            .map(|v| unsafe { out.cast::<bool>().write(v) }),
        7 => store!(Optional<BootSnapshot>, optional(boot_snapshot())),
        8 => store!(backend::Snapshot, backend_state()),
        9 => store!(bool, retains_driver_owner(args.owner)),
        10 => store!(bool, set_backend_policy(args.operation)),
        11 => {
            reject_native_backend();
            Ok(())
        }
        15 => abort_native(args.owner, args.generation),
        16 => commit_native(args.owner, args.generation)
            .map(|v| unsafe { out.cast::<u32>().write(v) }),
        17 => restore_boot_backend(args.owner, args.generation),
        18 => begin_device_reset(args.owner, args.generation, args.adapter)
            .map(|v| unsafe { out.cast::<u64>().write(v) }),
        19 => restored_boot_owner(args.owner, args.generation, args.adapter)
            .map(|v| unsafe { out.cast::<bool>().write(v) }),
        20 => begin_reset_retirement(args.owner, args.generation, args.adapter),
        21 => {
            end_reset_retirement();
            Ok(())
        }
        22 => retire_device_reset(args.owner, args.generation),
        23 => release_terminal(args.owner, args.generation, args.adapter),
        24 => unsafe {
            replace_native_frame_locked(args.owner, args.generation, args.frame, args.value)
        },
        25 => lose_native_mode_locked(args.owner, args.generation),
        26 => {
            register_boot_backend(unsafe { args.input.cast::<Target>().read() });
            Ok(())
        }
        27 => store!(bool, enable_framebuffer_write_combining()),
        28 => store!(Stats, stats()),
        29 => bind_presentation_stats(
            args.owner,
            args.value,
            unsafe { args.input.cast::<a::GfxBackendBinding>().read() },
            args.generation,
        ),
        30 => publish_presentation_stats(args.owner, args.value, unsafe {
            args.input.cast::<a::DisplayPresentationStats>().read()
        }),
        31 => presentation_stats(args.operation)
            .map(|v| unsafe { out.cast::<a::DisplayPresentationStats>().write(v) }),
        32 => publish_presentation_info(args.owner, args.value, unsafe {
            args.input.cast::<a::DisplayPresentationInfo>().read()
        }),
        33 => presentation_info(args.operation)
            .map(|v| unsafe { out.cast::<a::DisplayPresentationInfo>().write(v) }),
        34 => presentation_feedback(args.operation, unsafe {
            args.input.cast::<a::GfxFence>().read()
        })
        .map(|v| unsafe { out.cast::<a::DisplayPresentationStats>().write(v) }),
        35 => store!(bool, fill(args.color)),
        36 => store!(
            bool,
            rect(
                i32::try_from(args.x).expect("rectangle x"),
                i32::try_from(args.y).expect("rectangle y"),
                u32::try_from(args.w).expect("rectangle width"),
                u32::try_from(args.h).expect("rectangle height"),
                args.color
            )
        ),
        37 => store!(bool, unsafe {
            text_z(
                if args.has_font {
                    Some(args.operation)
                } else {
                    None
                },
                i32::try_from(args.x).expect("text x"),
                i32::try_from(args.y).expect("text y"),
                args.input.cast(),
                args.color,
                args.background,
            )
        }),
        38 => store!(bool, put_packed32(args.w, args.h, args.color)),
        39 => store!(bool, put_xrgb32(args.w, args.h, args.color)),
        40 | 41 => {
            let source = if args.value == 0 {
                &[][..]
            } else {
                unsafe { slice::from_raw_parts(args.input.cast::<u8>(), args.value as usize) }
            };
            let value = if op == 40 {
                unsafe {
                    present_packed32_rect(
                        args.x as u64,
                        args.y as u64,
                        args.w,
                        args.h,
                        source,
                        args.stride,
                    )
                }
            } else {
                unsafe {
                    present_xrgb32_rect(
                        args.x as u64,
                        args.y as u64,
                        args.w,
                        args.h,
                        source,
                        args.stride,
                    )
                }
            };
            store!(bool, value)
        }
        42 => {
            let regions = if args.regions_len == 0 {
                &[][..]
            } else {
                unsafe { slice::from_raw_parts(args.regions, args.regions_len) }
            };
            store!(PresentOutcome, unsafe {
                present_xrgb32_regions(
                    args.input.cast(),
                    args.count,
                    u32::try_from(args.stride).expect("present source stride"),
                    regions,
                    args.source_generation,
                    args.input_tick,
                    args.flag,
                )
            })
        }
        43 => store!(PresentCapabilities, present_capabilities()),
        44 => store!(bool, present_fence_completed(args.value)),
        45 => store!(u64, highest_completed_fence()),
        46 => store!(bool, probe_rejected_takeover()),
        47..=50 => {
            let value = match op {
                47 => operation_names(args.operation),
                48 => present_reason_name(u8::try_from(args.operation).expect("present reason")),
                49 => mapping_kind_name(u8::try_from(args.operation).expect("mapping kind")),
                _ => cache_policy_name(u8::try_from(args.operation).expect("cache policy")),
            };
            store!(Text, Text::literal(value))
        }
        _ => panic!("display manager operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

// Private operation modules share the sole state and original typed owners.
mod boot_framebuffer;
mod diagnostics;
mod presentation;
mod transitions;
use boot_framebuffer::{
    bootfb_copy_xrgb32_region, bootfb_fill, bootfb_present_packed32_rect, bootfb_put_packed32,
    bootfb_put_xrgb32, bootfb_rect, clip_signed_rect, copy_name, merge_rect, valid_present_region,
};
pub use diagnostics::{
    cache_policy_name, mapping_kind_name, operation_names, present_reason_name,
    probe_rejected_takeover,
};
pub use presentation::{
    fill, highest_completed_fence, present_capabilities, present_fence_completed,
    present_packed32_rect, present_xrgb32_rect, present_xrgb32_regions, put_packed32, put_xrgb32,
    rect, text_z,
};
use transitions::restore_boot_locked;
pub use transitions::{
    abort_native, begin_device_reset, begin_reset_retirement, commit_native,
    enable_framebuffer_write_combining, end_reset_retirement, finish_boot, hold_boot,
    lose_native_mode_locked, prepare_held_native, prepare_native, prepare_recovered_native,
    register_boot_backend, release_terminal, replace_native_frame_locked, restore_boot_backend,
    restored_boot_owner, retire_device_reset,
};
