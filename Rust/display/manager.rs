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
// Preserve the original six-method framebuffer vtable seam.
#[allow(clippy::type_complexity)]
#[derive(Default)]
struct DeviceOps {
    fill: Option<fn(*mut Device, u32) -> bool>,
    rect: Option<fn(*mut Device, i32, i32, u32, u32, u32) -> bool>,
    present_packed32_rect: Option<unsafe fn(*mut Device, u64, u64, u64, u64, &[u8], u64) -> bool>,
    #[allow(dead_code)] // Retain the original vtable seam; common XRGB uses the regions path.
    present_xrgb32_rect: Option<unsafe fn(*mut Device, u64, u64, u64, u64, &[u8], u64) -> bool>,
    put_packed32: Option<fn(*mut Device, u64, u64, u32) -> bool>,
    put_xrgb32: Option<fn(*mut Device, u64, u64, u32) -> bool>,
}
static EMPTY_OPS: DeviceOps = DeviceOps {
    fill: None,
    rect: None,
    present_packed32_rect: None,
    present_xrgb32_rect: None,
    put_packed32: None,
    put_xrgb32: None,
};
static BOOTFB_OPS: DeviceOps = DeviceOps {
    fill: Some(bootfb_fill),
    rect: Some(bootfb_rect),
    present_packed32_rect: Some(bootfb_present_packed32_rect),
    present_xrgb32_rect: Some(bootfb_present_xrgb32_rect),
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
pub fn hold_boot(holder: Holder) -> Result<BootHoldResult, Error> {
    let _commit = OutputCommit::enter()?;
    let p = context();
    let boot = unsafe { (*p).saved_boot }.ok_or(Error::Unavailable)?;
    if unsafe { (*p).held_boot.is_some() || (*p).native_backend.is_some() } {
        return Err(Error::Busy);
    }
    if holder.expected_generation == 0
        || holder.expected_generation != unsafe { (*p).manager.value.generation }
    {
        return Err(Error::Stale);
    }
    let generation = unsafe { &mut (*p).manager }.begin(holder.owner, holder.adapter_id)?;
    if !firmware::r4os_rust_display_firmware_gate.try_revoke() {
        unsafe { &mut (*p).manager }
            .abort(holder.owner, generation, 4)
            .expect("exact failed hold admission");
        publish_stats();
        return Err(Error::Busy);
    }
    unsafe {
        (*p).held_boot = Some(HeldBoot {
            holder,
            generation,
            captured: false,
            effects: false,
            restored: false,
        });
        (*p).primary = ptr::null_mut();
    }
    publish_stats();
    if unsafe { holder.capture.call(holder.context, &boot) } {
        unsafe {
            (*p).held_boot.as_mut().expect("exact boot hold").captured = true;
        }
        return Ok(BootHoldResult {
            generation,
            captured: true,
            retained: true,
        });
    }
    let released = unsafe { holder.release.call(holder.context) };
    if released {
        release_boot_locked(holder.owner, generation, 4);
    }
    Ok(BootHoldResult {
        generation,
        captured: false,
        retained: !released,
    })
}
pub fn finish_boot(owner: usize, generation: u64, operation: u32) -> Result<bool, Error> {
    let _execution = execution()?;
    let p = context();
    if unsafe { (*p).native_backend.is_some() } {
        return Err(Error::Busy);
    }
    unsafe { &(*p).manager }.check_pending(owner, generation)?;
    let hold = unsafe { (*p).held_boot }.ok_or(Error::Stale)?;
    if operation > 2 {
        return Err(Error::Invalid);
    }
    if operation == 1 {
        if unsafe { (*p).system_transition } {
            return Err(Error::Busy);
        }
        if !hold.captured {
            return Err(Error::Invalid);
        }
        if hold.effects {
            return Err(Error::Busy);
        }
        unsafe {
            (*p).held_boot.as_mut().expect("exact boot hold").effects = true;
        }
        return Ok(false);
    }
    if hold.effects && operation != 2 {
        return Err(Error::Busy);
    }
    if hold.effects && !hold.restored {
        let boot = unsafe { (*p).saved_boot }.ok_or(Error::Unavailable)?;
        if !unsafe {
            hold.holder
                .restore
                .call(hold.holder.context, generation, &boot)
        } {
            return Err(Error::RestoreFailed);
        }
        unsafe {
            (*p).held_boot.as_mut().expect("exact boot hold").restored = true;
        }
    }
    if !unsafe { hold.holder.release.call(hold.holder.context) } {
        return Err(Error::RestoreFailed);
    }
    release_boot_locked(owner, generation, 0);
    Ok(true)
}
fn release_boot_locked(owner: usize, generation: u64, reason: u32) {
    let p = context();
    unsafe { &mut (*p).manager }
        .abort(owner, generation, reason)
        .expect("exact boot hold release");
    unsafe {
        (*p).held_boot = None;
    }
    if !unsafe { (*p).system_transition } {
        unsafe {
            (*p).primary = ptr::addr_of_mut!((*p).boot);
        }
        firmware::r4os_rust_display_firmware_gate.restore();
    }
    publish_stats()
}
pub fn prepare_native(backend: Backend) -> Result<u64, Error> {
    prepare_native_impl(backend, 0, 0)
}
pub fn prepare_held_native(backend: Backend, held_generation: u64) -> Result<u64, Error> {
    if held_generation == 0 {
        Err(Error::Stale)
    } else {
        prepare_native_impl(backend, held_generation, 0)
    }
}
pub fn prepare_recovered_native(
    backend: Backend,
    held_generation: u64,
    reset_generation: u64,
) -> Result<u64, Error> {
    if reset_generation == 0 {
        Err(Error::Stale)
    } else {
        prepare_native_impl(backend, held_generation, reset_generation)
    }
}
fn prepare_native_impl(
    backend: Backend,
    held_generation: u64,
    reset_generation: u64,
) -> Result<u64, Error> {
    let _commit = OutputCommit::enter()?;
    let p = context();
    let boot = unsafe { (*p).saved_boot }.ok_or(Error::Unavailable)?;
    let target = backend.target;
    let frame = if target.framebuffer.is_null() {
        return Err(Error::Invalid);
    } else {
        unsafe { target.framebuffer.read() }
    };
    if backend.begin_cpu.is_none() != backend.end_cpu.is_none() {
        return Err(Error::Invalid);
    }
    if target.kind != NATIVE
        || target.mapping.kind != 2
        || target.name.len == 0
        || target.name.len >= blit_backend::NAME_BYTES
        || target.mode.width != boot.mode.width
        || target.mode.height != boot.mode.height
        || target.mode.bpp != boot.mode.bpp
        || target.mode.red_mask_size != boot.mode.red_mask_size
        || target.mode.red_mask_shift != boot.mode.red_mask_shift
        || target.mode.green_mask_size != boot.mode.green_mask_size
        || target.mode.green_mask_shift != boot.mode.green_mask_shift
        || target.mode.blue_mask_size != boot.mode.blue_mask_size
        || target.mode.blue_mask_shift != boot.mode.blue_mask_shift
        || !fb::supports_rgb32(&frame)
        || frame.width == 0
        || frame.height == 0
        || frame.width != u64::from(target.mode.width)
        || frame.height != u64::from(target.mode.height)
        || frame.pitch != u64::from(target.mode.pitch)
        || target.mode.bpp != 32
        || frame.red_mask_size != target.mode.red_mask_size
        || frame.red_mask_shift != target.mode.red_mask_shift
        || frame.green_mask_size != target.mode.green_mask_size
        || frame.green_mask_shift != target.mode.green_mask_shift
        || frame.blue_mask_size != target.mode.blue_mask_size
        || frame.blue_mask_shift != target.mode.blue_mask_shift
        || frame.pitch & 3 != 0
        || frame.address.addr() & 3 != 0
        || target.mapping.virt_base != frame.address.addr() as u64
        || frame.height > u64::MAX / frame.pitch
        || target.mapping.byte_len < frame.pitch * frame.height
        || target.mapping.virt_base > u64::MAX - target.mapping.byte_len
        || target.flags & CPU_PRESENT == 0
    {
        return Err(Error::Invalid);
    }
    if unsafe { (*p).native_backend.is_some() } {
        return Err(Error::Busy);
    }
    if unsafe { (*p).held_boot.is_some() } && held_generation == 0 {
        return Err(Error::Busy);
    }
    if unsafe { (*p).held_boot.is_none() } && held_generation != 0 {
        return Err(Error::Stale);
    }
    if held_generation != 0 {
        let hold = unsafe { (*p).held_boot }.ok_or(Error::Stale)?;
        if hold.generation != held_generation
            || hold.holder.owner != backend.owner
            || hold.holder.adapter_id != backend.adapter_id
        {
            return Err(Error::Stale);
        }
        if !hold.captured || !hold.effects || hold.restored {
            return Err(Error::Invalid);
        }
    }
    let generation = if reset_generation != 0 {
        unsafe { &mut (*p).manager }.reprepare(
            backend.owner,
            reset_generation,
            backend.adapter_id,
        )?
    } else if held_generation != 0 {
        unsafe { &(*p).manager }.check_pending(backend.owner, held_generation)?;
        held_generation
    } else {
        unsafe { &mut (*p).manager }.begin(backend.owner, backend.adapter_id)?
    };
    unsafe {
        (*p).native_backend = Some(backend);
        (*p).retired_reset_generation = 0;
        (*p).native_hold_generation = held_generation;
        (*p).native_reset_generation = reset_generation;
        (*p).native_restore_confirmed = false;
        (*p).native = Device::target(Target {
            name: Text::literal(b"native"),
            ..target
        });
    }
    publish_stats();
    Ok(generation)
}
pub fn abort_native(owner: usize, generation: u64) -> Result<(), Error> {
    let _execution = execution()?;
    let p = context();
    if unsafe { (*p).held_boot.is_some() && (*p).native_hold_generation == 0 } {
        return Err(Error::Busy);
    }
    if unsafe { (*p).native_reset_generation } != 0 {
        unsafe { &mut (*p).manager }.abort(owner, generation, 4)?;
    } else if unsafe { (*p).native_hold_generation } != 0 {
        unsafe { &(*p).manager }.check_pending(owner, generation)?;
    } else {
        unsafe { &mut (*p).manager }.abort(owner, generation, 4)?;
    }
    unsafe {
        (*p).native_backend = None;
        (*p).native = Device::empty(false);
        (*p).native_hold_generation = 0;
        (*p).native_reset_generation = 0;
        (*p).native_restore_confirmed = false;
    }
    publish_stats();
    Ok(())
}
pub fn commit_native(owner: usize, generation: u64) -> Result<u32, Error> {
    let _commit = OutputCommit::enter()?;
    let p = context();
    if unsafe { (*p).held_boot.is_some() && (*p).native_hold_generation == 0 } {
        return Err(Error::Busy);
    }
    unsafe { &(*p).manager }.check_pending(owner, generation)?;
    let backend = unsafe { (*p).native_backend }.ok_or(Error::Unavailable)?;
    let boot = unsafe { (*p).saved_boot }.ok_or(Error::Unavailable)?;
    if unsafe { (*p).native_hold_generation } == 0
        && !firmware::r4os_rust_display_firmware_gate.try_revoke()
    {
        return Err(Error::Busy);
    }
    publish_stats();
    let result = unsafe { backend.commit.call(backend.context, generation, &boot) };
    match result {
        0 => {
            unsafe { &mut (*p).manager }
                .commit(owner, generation, true)
                .expect("exact native commit");
            unsafe {
                (*p).primary = ptr::addr_of_mut!((*p).native);
            }
        }
        1 => {
            if unsafe { (*p).native_reset_generation } != 0 {
                unsafe { &mut (*p).manager }
                    .abort(owner, generation, 5)
                    .expect("exact recovered rejection");
            } else if unsafe { (*p).native_hold_generation } == 0 {
                unsafe { &mut (*p).manager }
                    .abort(owner, generation, 5)
                    .expect("exact native rejection");
                firmware::r4os_rust_display_firmware_gate.restore();
            }
            unsafe {
                (*p).native_backend = None;
                (*p).native = Device::empty(false);
                (*p).native_hold_generation = 0;
                (*p).native_reset_generation = 0;
                (*p).native_restore_confirmed = false;
            }
        }
        2 => {
            unsafe { &mut (*p).manager }
                .fail_commit(owner, generation)
                .expect("exact failed native commit");
            unsafe {
                (*p).primary = ptr::null_mut();
            }
        }
        _ => panic!("display commit result enum"),
    }
    publish_stats();
    Ok(result)
}
pub fn restore_boot_backend(owner: usize, generation: u64) -> Result<(), Error> {
    let _execution = execution()?;
    restore_boot_locked(owner, generation)
}
fn mode_pending() -> bool {
    super::native_driver::mode_pending()
}
pub fn begin_device_reset(owner: usize, generation: u64, adapter: u32) -> Result<u64, Error> {
    let _execution = execution()?;
    let p = context();
    let current = unsafe { (*p).manager.value };
    let restored_console = current.state == backend::BOOTFB;
    if restored_console {
        if !unsafe { &(*p).manager }.restored_boot_owner(owner, generation, adapter) {
            return Err(Error::Stale);
        }
        if !unsafe { (*p).system_transition }
            || unsafe { (*p).held_boot.is_some() || (*p).native_backend.is_some() }
        {
            return Err(Error::Busy);
        }
    } else if adapter == 0
        || adapter
            != if current.state == backend::PREPARING {
                current.pending_adapter_id
            } else {
                current.adapter_id
            }
    {
        return Err(Error::Stale);
    }
    if current.state == backend::RECOVERING
        && unsafe { (*p).retired_reset_generation } != generation
    {
        return Err(Error::Busy);
    }
    let mut candidate = unsafe { (*p).manager };
    let next = if restored_console {
        candidate.begin_restored_boot_reset(owner, generation, adapter)?
    } else {
        candidate.begin_device_reset(owner, generation)?
    };
    if !firmware::r4os_rust_display_firmware_gate.is_revoked()
        && !firmware::r4os_rust_display_firmware_gate.try_revoke()
    {
        return Err(Error::Busy);
    }
    unsafe {
        (*p).manager = candidate;
        (*p).retired_reset_generation = 0;
        (*p).primary = ptr::null_mut();
    }
    publish_stats();
    Ok(next)
}
pub fn restored_boot_owner(owner: usize, generation: u64, adapter: u32) -> Result<bool, Error> {
    let _execution = execution()?;
    let p = context();
    Ok(unsafe {
        (*p).held_boot.is_none()
            && (*p).native_backend.is_none()
            && (*p).manager.restored_boot_owner(owner, generation, adapter)
    })
}
pub fn begin_reset_retirement(owner: usize, generation: u64, adapter: u32) -> Result<(), Error> {
    if !unsafe { ownership::enter(EXECUTION.0.get(), 0) } {
        return Err(Error::Busy);
    }
    let result = (|| {
        let p = context();
        unsafe { &(*p).manager }.check_active(owner, generation)?;
        if adapter == 0 || unsafe { (*p).manager.value.adapter_id } != adapter {
            return Err(Error::Stale);
        }
        if unsafe { (*p).manager.value.state } != backend::RECOVERING {
            return Err(Error::Busy);
        }
        Ok(())
    })();
    if result.is_err() {
        end_reset_retirement();
    }
    result
}
pub fn end_reset_retirement() {
    unsafe { ownership::leave(EXECUTION.0.get()) }
}
pub fn retire_device_reset(owner: usize, generation: u64) -> Result<(), Error> {
    let _execution = execution()?;
    let p = context();
    unsafe { &(*p).manager }.check_active(owner, generation)?;
    if unsafe { (*p).manager.value.state } != backend::RECOVERING {
        return Err(Error::Busy);
    }
    if mode_pending() {
        return Err(Error::Busy);
    }
    unsafe {
        (*p).primary = ptr::null_mut();
        (*p).native_backend = None;
        (*p).native = Device::empty(false);
        (*p).native_hold_generation = 0;
        (*p).native_reset_generation = 0;
        (*p).native_restore_confirmed = false;
        (*p).retired_reset_generation = generation;
    }
    publish_stats();
    Ok(())
}
pub fn release_terminal(owner: usize, generation: u64, adapter: u32) -> Result<(), Error> {
    let _execution = execution()?;
    let p = context();
    if !unsafe { (*p).system_transition } {
        return Err(Error::Busy);
    }
    unsafe { &(*p).manager }.check_active(owner, generation)?;
    if adapter == 0 || adapter != unsafe { (*p).manager.value.adapter_id } {
        return Err(Error::Stale);
    }
    if unsafe { (*p).retired_reset_generation } != generation
        || !unsafe { (*p).primary.is_null() }
        || unsafe { (*p).native_backend.is_some() }
        || !firmware::r4os_rust_display_firmware_gate.is_revoked()
    {
        return Err(Error::Busy);
    }
    let mut candidate = unsafe { (*p).manager };
    candidate.finish_terminal(owner, generation)?;
    if let Some(hold) = unsafe { (*p).held_boot } {
        if hold.holder.owner != owner || hold.holder.adapter_id != adapter {
            return Err(Error::Stale);
        }
        let release = hold.holder.release_adopted.unwrap_or(hold.holder.release);
        if !unsafe { release.call(hold.holder.context) } {
            return Err(Error::Busy);
        }
        unsafe {
            (*p).held_boot = None;
        }
    }
    unsafe {
        (*p).manager = candidate;
        (*p).retired_reset_generation = 0;
    }
    publish_stats();
    Ok(())
}
/// # Safety
/// Caller holds DisplayExecution through the exact physical mode transaction;
/// the retained frame stays resident through all following CPU use/retirement.
pub unsafe fn replace_native_frame_locked(
    owner: usize,
    generation: u64,
    frame: *mut Framebuffer,
    bytes: u64,
) -> Result<(), Error> {
    let p = context();
    let native = unsafe { (*p).native_backend }.ok_or(Error::Unavailable)?;
    unsafe { &(*p).manager }.check_active(owner, generation)?;
    let f = unsafe { frame.read() };
    if native.owner != owner
        || unsafe { (*p).system_transition }
        || !fb::is_native_xrgb32(&f)
        || f.width == 0
        || f.height == 0
        || f.width > 65536
        || f.height > 65536
        || f.pitch > u64::from(u32::MAX)
        || f.pitch < f.width * 4
        || f.pitch & 3 != 0
        || f.address.addr() & 3 != 0
        || bytes != f.pitch * f.height
        || f.address.addr() as u64 > u64::MAX - bytes
    {
        return Err(Error::Invalid);
    }
    unsafe { &mut (*p).manager }.mode_result(owner, generation, true)?;
    unsafe {
        (*p).native.value.mode.width = u32::try_from(f.width).expect("native frame width");
        (*p).native.value.mode.height = u32::try_from(f.height).expect("native frame height");
        (*p).native.value.mode.pitch = u32::try_from(f.pitch).expect("native frame pitch");
        (*p).native.value.mapping = Mapping {
            kind: 2,
            virt_base: f.address.addr() as u64,
            byte_len: bytes,
            ..Mapping::default()
        };
        (*p).native.frame = frame;
        (*p).native.value.flags &= !FIXED_MODE;
        let native = (*p)
            .native_backend
            .as_mut()
            .expect("exact native frame owner");
        native.target.mode = (*p).native.value.mode;
        native.target.mapping = (*p).native.value.mapping;
        native.target.framebuffer = frame;
        native.target.flags = (*p).native.value.flags;
        (*p).primary = ptr::addr_of_mut!((*p).native);
    }
    publish_stats();
    Ok(())
}
pub fn lose_native_mode_locked(owner: usize, generation: u64) -> Result<(), Error> {
    let p = context();
    unsafe { &mut (*p).manager }.mode_result(owner, generation, false)?;
    unsafe {
        (*p).primary = ptr::null_mut();
    }
    publish_stats();
    Ok(())
}
fn restore_boot_locked(owner: usize, generation: u64) -> Result<(), Error> {
    if mode_pending() {
        return Err(Error::Busy);
    }
    let p = context();
    let native = unsafe { (*p).native_backend };
    if native.is_none() && unsafe { (*p).held_boot.is_none() } {
        return Err(Error::Unavailable);
    }
    let boot = unsafe { (*p).saved_boot }.ok_or(Error::Unavailable)?;
    let recovery_generation = if unsafe { (*p).manager.value.state } == backend::RECOVERING {
        unsafe { &(*p).manager }.check_active(owner, generation)?;
        generation
    } else {
        unsafe { &mut (*p).manager }.begin_recovery(owner, generation)?
    };
    unsafe {
        (*p).primary = ptr::null_mut();
    }
    publish_stats();
    if !unsafe { (*p).native_restore_confirmed } {
        let restored = if let Some(native) = native {
            unsafe {
                native
                    .restore
                    .call(native.context, recovery_generation, &boot)
            }
        } else {
            let hold = unsafe { (*p).held_boot }.expect("held boot restore owner");
            if hold.holder.owner != owner || !hold.captured || !hold.effects {
                false
            } else {
                let restore = hold.holder.restore_adopted.unwrap_or(hold.holder.restore);
                unsafe { restore.call(hold.holder.context, recovery_generation, &boot) }
            }
        };
        if !restored {
            unsafe { &mut (*p).manager }
                .recovery_failed(owner, recovery_generation)
                .expect("exact failed restore");
            publish_stats();
            return Err(Error::RestoreFailed);
        }
    }
    unsafe {
        (*p).native_restore_confirmed = true;
    }
    if let Some(hold) = unsafe { (*p).held_boot } {
        let release = hold.holder.release_adopted.unwrap_or(hold.holder.release);
        if !unsafe { release.call(hold.holder.context) } {
            unsafe { &mut (*p).manager }
                .recovery_failed(owner, recovery_generation)
                .expect("exact failed restore release");
            publish_stats();
            return Err(Error::RestoreFailed);
        }
        unsafe {
            (*p).held_boot = None;
            (*p).native_hold_generation = 0;
        }
    }
    if let Err(error) = unsafe { &mut (*p).manager }.restore_boot(owner, recovery_generation) {
        unsafe { &mut (*p).manager }
            .recovery_failed(owner, recovery_generation)
            .expect("exact failed boot adoption");
        publish_stats();
        return Err(error);
    }
    if !unsafe { (*p).system_transition } {
        unsafe {
            (*p).primary = ptr::addr_of_mut!((*p).boot);
        }
        firmware::r4os_rust_display_firmware_gate.restore();
    }
    unsafe {
        (*p).native_backend = None;
        (*p).native = Device::empty(false);
        (*p).native_reset_generation = 0;
        (*p).native_restore_confirmed = false;
        (*p).retired_reset_generation = 0;
    }
    publish_stats();
    Ok(())
}
pub fn register_boot_backend(target: Target) {
    let Ok(_commit) = OutputCommit::enter() else {
        return;
    };
    let _state = ownership::enter_state();
    let p = context();
    if unsafe { (*p).manager.value.generation } != 0 {
        return;
    }
    unsafe {
        (*p).boot = Device::target(target);
        (*p).primary = ptr::addr_of_mut!((*p).boot);
        (*p).present_generation = 0;
        (*p).completed_fence = 0;
        (*p).manager.init_boot();
        if !target.framebuffer.is_null() {
            (*p).saved_boot = Some(BootSnapshot {
                mode: target.mode,
                mapping: target.mapping,
                framebuffer: target.framebuffer.read(),
            });
        }
        (*p).completed_backend = (*p).manager.value;
        (*p).completed_stats = capture_stats();
    }
    publish_geometry_locked(unsafe { (*p).completed_stats });
    unsafe {
        (*p).completed_boot_mode = target.mode;
        (*p).completed_boot_mapping = target.mapping;
        copy_name(&mut (*p).completed_output_name, target.name.bytes());
    }
}
struct Publish {
    boot_mapping: Option<Mapping>,
}
impl Drop for Publish {
    fn drop(&mut self) {
        publish_stats_with_mapping(self.boot_mapping)
    }
}
pub fn enable_framebuffer_write_combining() -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let mut publication = Publish { boot_mapping: None };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    if unsafe { (*device).value.kind } != BOOTFB {
        return false;
    }
    if cpu::r4os_rust_cpu_control(2, 0) == 0 {
        unsafe {
            (*device).value.mapping.cache_policy = 3;
        }
        return false;
    }
    let mapping = unsafe { (*device).value.mapping };
    if mapping.virt_base == 0 || mapping.byte_len == 0 {
        unsafe {
            (*device).value.mapping.cache_policy = 4;
        }
        return false;
    }
    if !paging::r4os_rust_paging_write_combining(mapping.virt_base, mapping.byte_len) {
        unsafe {
            (*device).value.mapping.cache_policy = 4;
        }
        return false;
    }
    unsafe {
        (*device).value.mapping.cache_policy = 2;
    }
    if unsafe { (*device).value.kind } == BOOTFB {
        publication.boot_mapping = Some(unsafe { (*device).value.mapping });
    }
    true
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
fn begin_cpu_write(device: *const Device) -> Option<CpuWrite> {
    if unsafe { (*device).value.kind } != NATIVE {
        return Some(CpuWrite {
            callback: None,
            context: 0,
            active: true,
            damage: None,
        });
    }
    let native = unsafe { (*context()).native_backend }?;
    if let Some(begin) = native.begin_cpu {
        if native.end_cpu.is_none() || !unsafe { begin.call(native.context) } {
            return None;
        }
        return Some(CpuWrite {
            callback: native.end_cpu,
            context: native.context,
            active: true,
            damage: None,
        });
    }
    if native.end_cpu.is_none() {
        Some(CpuWrite {
            callback: None,
            context: 0,
            active: true,
            damage: None,
        })
    } else {
        None
    }
}
pub fn fill(rgb: u32) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.fill }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let start = timer::tick_count();
    let ok = op(device, rgb);
    if ok {
        write.damage = Some(unsafe { (*device).value.last_present_rect });
    }
    if !write.finish(ok) {
        return false;
    }
    if ok {
        record_timing(device, start);
        publish_stats();
    }
    ok
}
pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.rect }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let start = timer::tick_count();
    let ok = op(device, x, y, w, h, rgb);
    if ok {
        write.damage = Some(unsafe { (*device).value.last_present_rect });
    }
    if !write.finish(ok) {
        return false;
    }
    if ok {
        record_timing(device, start);
        publish_stats();
    }
    ok
}
/// # Safety
/// Input retains at least a readable terminator or4096bytes on the exact
/// calling task. Display execution retains the frame through CPU completion.
pub unsafe fn text_z(
    font_id: Option<u32>,
    x: i32,
    y: i32,
    value: *const u8,
    fg: u32,
    bg: u32,
) -> bool {
    if value.is_null() {
        return false;
    }
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    if !fb::supports_rgb32(&frame) {
        return false;
    }
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let mut length = 0;
    while length < 4096 && unsafe { value.add(length).read() } != 0 {
        length += 1;
    }
    let catalog = font::owner::acquire();
    let state = unsafe { &*catalog.state() };
    let selected = state.normalize_id(font_id.unwrap_or_else(|| state.current_font_id()));
    let line_height = state.glyph_height_for_font(selected);
    let packed_fg = fb::pack_rgb(&frame, fg);
    let packed_bg = fb::pack_rgb(&frame, bg);
    let mut pen_x = i64::from(x);
    let mut pen_y = i64::from(y);
    let mut offset = 0;
    let mut pixels = 0;
    let mut bounds = Rect::default();
    let start = timer::tick_count();
    while offset < length {
        let scalar = unsafe { font::decode::decode_raw(value, length, offset) };
        offset += scalar.consumed;
        if scalar.codepoint == u32::from(b'\r') {
            continue;
        }
        if scalar.codepoint == u32::from(b'\n') {
            pen_x = i64::from(x);
            pen_y += i64::from(line_height);
            continue;
        }
        let glyph = state.glyph_bitmap_for_font(selected, scalar.codepoint);
        if let Some(clipped) = clip_signed_rect(
            pen_x,
            pen_y,
            glyph.width.max(glyph.advance),
            glyph.line_height,
            unsafe { (*device).value.mode },
        ) {
            bounds = if pixels == 0 {
                clipped
            } else {
                merge_rect(bounds, clipped)
            };
            pixels += u64::from(clipped.w) * u64::from(clipped.h);
            for row in 0..clipped.h {
                let gy = usize::try_from(i64::from(clipped.y) + i64::from(row) - pen_y)
                    .expect("clipped glyph row");
                for column in 0..clipped.w {
                    let gx = usize::try_from(i64::from(clipped.x) + i64::from(column) - pen_x)
                        .expect("clipped glyph column");
                    let ink = gy < glyph.height as usize
                        && gy < glyph.rows.len()
                        && gx < glyph.width as usize
                        && gx < 64
                        && glyph.rows[gy] & (1u64 << gx) != 0;
                    unsafe {
                        fb::put_packed(
                            &frame,
                            u64::from(clipped.x + column),
                            u64::from(clipped.y + row),
                            if ink { packed_fg } else { packed_bg },
                        );
                    }
                }
            }
        }
        pen_x += i64::from(glyph.advance);
    }
    if pixels == 0 {
        return false;
    }
    write.damage = Some(bounds);
    if !write.finish(true) {
        return false;
    }
    record_present_aggregate(device, REASON_RECT, bounds, pixels, false);
    record_timing(device, start);
    publish_stats();
    true
}
pub fn put_packed32(x: u64, y: u64, color32: u32) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.put_packed32 }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let ok = op(device, x, y, color32);
    if ok {
        write.damage = Some(Rect {
            x: u32::try_from(x).expect("pixel x"),
            y: u32::try_from(y).expect("pixel y"),
            w: 1,
            h: 1,
        });
    }
    write.finish(ok) && ok
}
pub fn put_xrgb32(x: u64, y: u64, rgb: u32) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.put_xrgb32 }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let ok = op(device, x, y, rgb);
    if ok {
        write.damage = Some(Rect {
            x: u32::try_from(x).expect("pixel x"),
            y: u32::try_from(y).expect("pixel y"),
            w: 1,
            h: 1,
        });
    }
    write.finish(ok) && ok
}
/// # Safety
/// Original readable source span retained through the exact CPU write lease.
pub unsafe fn present_packed32_rect(
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    src: &[u8],
    stride: u64,
) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return false;
    }
    let Some(op) = (unsafe { (*device).ops.present_packed32_rect }) else {
        return false;
    };
    let Some(mut write) = begin_cpu_write(device) else {
        return false;
    };
    let start = timer::tick_count();
    let ok = unsafe { op(device, x, y, w, h, src, stride) };
    if ok {
        write.damage = Some(unsafe { (*device).value.last_present_rect });
    }
    if !write.finish(ok) {
        return false;
    }
    if ok {
        record_timing(device, start);
        publish_stats();
    }
    ok
}
/// # Safety
/// Exact original readable four-byte pixels and source byte extent through
/// presentation; alignment/geometry retain the original wrapper validation.
pub unsafe fn present_xrgb32_rect(x: u64, y: u64, w: u64, h: u64, src: &[u8], stride: u64) -> bool {
    if x > u64::from(u32::MAX)
        || y > u64::from(u32::MAX)
        || w > u64::from(u32::MAX)
        || h > u64::from(u32::MAX)
        || stride > u64::from(u32::MAX)
        || src.len() & 3 != 0
        || src.as_ptr().addr() & 3 != 0
    {
        return false;
    }
    let region = blit_backend::Region {
        dst_x: x as u32,
        dst_y: y as u32,
        src_x: 0,
        src_y: 0,
        w: w as u32,
        h: h as u32,
    };
    unsafe {
        present_xrgb32_regions(
            src.as_ptr().cast(),
            u32::try_from(src.len() / 4).expect("present pixel count"),
            stride as u32,
            &[region],
            0,
            0,
            false,
        )
    }
    .success
}
/// # Safety
/// Source and each admitted pixel region remain readable, with the exact
/// retained output/CPU lease until this synchronous generation completes.
pub unsafe fn present_xrgb32_regions(
    source: *const u32,
    source_pixels: u32,
    source_stride: u32,
    regions: &[blit_backend::Region],
    source_generation: u64,
    input_tick: u64,
    input_tick_valid: bool,
) -> PresentOutcome {
    let mut outcome = PresentOutcome {
        source_generation,
        ..PresentOutcome::default()
    };
    let Ok(_execution) = execution() else {
        return outcome;
    };
    let device = unsafe { (*context()).primary };
    if device.is_null() {
        return outcome;
    }
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return outcome;
    }
    let frame = unsafe { frame.read() };
    if !fb::supports_rgb32(&frame)
        || source_pixels == 0
        || source_stride == 0
        || regions.is_empty()
        || regions.len() > 8
    {
        return outcome;
    }
    let mut pixels_total = 0u64;
    let mut bounds = Rect::default();
    for (index, &region) in regions.iter().enumerate() {
        if !valid_present_region(
            region,
            unsafe { (*device).value.mode },
            source_pixels,
            source_stride,
        ) {
            return outcome;
        }
        let pixels = u64::from(region.w) * u64::from(region.h);
        if pixels_total > u64::from(u32::MAX) - pixels {
            return outcome;
        }
        pixels_total += pixels;
        let rect = Rect {
            x: region.dst_x,
            y: region.dst_y,
            w: region.w,
            h: region.h,
        };
        bounds = if index == 0 {
            rect
        } else {
            merge_rect(bounds, rect)
        };
    }
    let Some(mut write) = begin_cpu_write(device) else {
        return outcome;
    };
    let start = timer::tick_count();
    let mut external = blit_backend::InvokeResult::default();
    if fb::is_native_xrgb32(&frame) && frame.pitch & 3 == 0 {
        let job = blit_backend::Job {
            version: 1,
            size: core::mem::size_of::<blit_backend::Job>() as u32,
            format: 1,
            flags: 0,
            target_address: frame.address.addr() as u64,
            target_width: u32::try_from(frame.width).expect("frame width"),
            target_height: u32::try_from(frame.height).expect("frame height"),
            target_pitch_pixels: u32::try_from(frame.pitch / 4).expect("frame pitch"),
            source_pixel_count: source_pixels,
            source_address: source.addr() as u64,
            source_stride_pixels: source_stride,
            region_count: u32::try_from(regions.len()).expect("region count"),
            regions_address: regions.as_ptr().addr() as u64,
        };
        external = unsafe { blit_backend::invoke(&job) };
    }
    if external.attempted && external.result == 0 {
        outcome.accelerated = true;
        outcome.backend_name = external.name;
    } else {
        let source_bytes =
            unsafe { slice::from_raw_parts(source.cast::<u8>(), source_pixels as usize * 4) };
        for &region in regions {
            if !unsafe { bootfb_copy_xrgb32_region(device, region, source_bytes, source_stride) } {
                return outcome;
            }
        }
        outcome.fallback = true;
        outcome.fallback_regions = regions.len() as u32;
        outcome.backend_error = if external.attempted {
            external.result
        } else {
            0
        };
        copy_name(
            &mut outcome.backend_name,
            if unsafe { (*device).value.kind } == BOOTFB {
                b"bootfb-cpu"
            } else {
                b"native-cpu"
            },
        );
    }
    write.damage = Some(bounds);
    if !write.finish(true) {
        return outcome;
    }
    let completed_tick = timer::tick_count();
    record_present_aggregate(device, REASON_XRGB32, bounds, pixels_total, false);
    record_timing_at(device, start, completed_tick);
    unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)) };
    let (generation, completed) = {
        let _state = ownership::enter_state();
        let p = context();
        unsafe {
            (*p).present_generation = (*p).present_generation.wrapping_add(1);
            if (*p).present_generation == 0 {
                (*p).present_generation = 1;
            }
            (*p).completed_fence = (*p).present_generation;
            (*p).completed_stats = capture_stats();
            ((*p).present_generation, (*p).completed_fence)
        }
    };
    outcome.success = true;
    outcome.present_generation = generation;
    outcome.fence = generation;
    outcome.completed_fence = completed;
    outcome.region_count = regions.len() as u32;
    outcome.pixel_count = u32::try_from(pixels_total).expect("present pixels");
    outcome.present_tick = completed_tick;
    outcome.elapsed_ticks = if input_tick_valid && completed_tick >= input_tick {
        completed_tick - input_tick
    } else {
        0
    };
    outcome
}
pub fn present_capabilities() -> PresentCapabilities {
    let current = stats();
    if !current.registered {
        return PresentCapabilities {
            flags: 0,
            formats: 0,
            max_regions: 0,
            backend_kind: 0,
            ..PresentCapabilities::default()
        };
    }
    let mut result = PresentCapabilities {
        flags: 1 | 2 | 4 | 32,
        backend_kind: if current.kind == BOOTFB { 1 } else { 3 },
        ..PresentCapabilities::default()
    };
    let cpu_name = if current.kind == BOOTFB {
        &b"bootfb-cpu"[..]
    } else {
        &b"native-cpu"[..]
    };
    copy_name(&mut result.backend_name, cpu_name);
    copy_name(&mut result.fallback_name, cpu_name);
    let external = blit_backend::snapshot();
    let mode = current.mode;
    let compatible = current.registered
        && mode.bpp == 32
        && mode.red_mask_size == 8
        && mode.red_mask_shift == 16
        && mode.green_mask_size == 8
        && mode.green_mask_shift == 8
        && mode.blue_mask_size == 8
        && mode.blue_mask_shift == 0
        && mode.pitch & 3 == 0;
    if external.active && compatible {
        result.flags |= 8 | 16;
        result.backend_kind = 2;
        result.backend_name = external.name;
        result.max_regions = external.max_regions.min(8);
    }
    result
}
pub fn highest_completed_fence() -> u64 {
    let _state = ownership::enter_state();
    unsafe { (*context()).completed_fence }
}
pub fn present_fence_completed(fence: u64) -> bool {
    fence != 0 && fence <= highest_completed_fence()
}
fn bootfb_fill(device: *mut Device, rgb: u32) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    unsafe { fb::fill(&frame, rgb) };
    record_present(
        device,
        REASON_FILL,
        0,
        0,
        u32::try_from(frame.width).expect("fill width"),
        u32::try_from(frame.height).expect("fill height"),
        false,
    );
    true
}
fn clip_signed_rect(x: i64, y: i64, w: u32, h: u32, mode: Mode) -> Option<Rect> {
    let left = x.max(0);
    let top = y.max(0);
    let right = i64::from(mode.width).min(x + i64::from(w));
    let bottom = i64::from(mode.height).min(y + i64::from(h));
    if left >= right || top >= bottom {
        None
    } else {
        Some(Rect {
            x: u32::try_from(left).expect("clip x"),
            y: u32::try_from(top).expect("clip y"),
            w: u32::try_from(right - left).expect("clip width"),
            h: u32::try_from(bottom - top).expect("clip height"),
        })
    }
}
fn bootfb_rect(device: *mut Device, x: i32, y: i32, w: u32, h: u32, rgb: u32) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let Some(rect) = clip_signed_rect(i64::from(x), i64::from(y), w, h, unsafe {
        (*device).value.mode
    }) else {
        return false;
    };
    unsafe {
        fb::rect(
            &frame.read(),
            u64::from(rect.x),
            u64::from(rect.y),
            u64::from(rect.w),
            u64::from(rect.h),
            rgb,
        );
    }
    record_present(device, REASON_RECT, rect.x, rect.y, rect.w, rect.h, false);
    true
}
unsafe fn bootfb_copy_packed32_rect(
    device: *mut Device,
    reason: u8,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    src: &[u8],
    stride: u64,
) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    if !fb::supports_rgb32(&frame) || w == 0 || h == 0 || x >= frame.width || y >= frame.height {
        return false;
    }
    let clipped_w = w.min(frame.width - x);
    let clipped_h = h.min(frame.height - y);
    let stride_bytes = stride * 4;
    let row_bytes = clipped_w * 4;
    if (src.len() as u64) < (clipped_h - 1) * stride_bytes + row_bytes {
        return false;
    }
    for row in 0..clipped_h {
        let offset = usize::try_from(row * stride_bytes).expect("packed row offset");
        let bytes = usize::try_from(row_bytes).expect("packed row bytes");
        let dst = unsafe {
            frame
                .address
                .add(((y + row) * frame.pitch + x * 4) as usize)
        };
        unsafe { copy_to_visible(dst, &src[offset..offset + bytes]) };
    }
    record_present(
        device,
        reason,
        u32::try_from(x).expect("packed x"),
        u32::try_from(y).expect("packed y"),
        u32::try_from(clipped_w).expect("packed width"),
        u32::try_from(clipped_h).expect("packed height"),
        false,
    );
    true
}
unsafe fn bootfb_copy_xrgb32_region(
    device: *mut Device,
    region: blit_backend::Region,
    src: &[u8],
    stride: u32,
) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    let stride_bytes = u64::from(stride) * 4;
    let row_bytes = u64::from(region.w) * 4;
    let first = (u64::from(region.src_y) * u64::from(stride) + u64::from(region.src_x)) * 4;
    let last_end = first + (u64::from(region.h) - 1) * stride_bytes + row_bytes;
    if last_end > src.len() as u64 {
        return false;
    }
    for y in 0..region.h {
        let offset = usize::try_from(first + u64::from(y) * stride_bytes).expect("XRGB row offset");
        if fb::is_native_xrgb32(&frame) {
            let dst = unsafe {
                frame.address.add(
                    ((u64::from(region.dst_y) + u64::from(y)) * frame.pitch
                        + u64::from(region.dst_x) * 4) as usize,
                )
            };
            unsafe { copy_to_visible(dst, &src[offset..offset + row_bytes as usize]) };
            continue;
        }
        for x in 0..region.w {
            let pixel_offset = offset + x as usize * 4;
            unsafe {
                fb::put_packed(
                    &frame,
                    u64::from(region.dst_x) + u64::from(x),
                    u64::from(region.dst_y) + u64::from(y),
                    fb::pack_rgb(&frame, read_xrgb32(src, pixel_offset)),
                );
            }
        }
    }
    true
}
fn valid_present_region(
    region: blit_backend::Region,
    mode: Mode,
    pixels: u32,
    stride: u32,
) -> bool {
    if region.w == 0
        || region.h == 0
        || region.src_x >= stride
        || region.w > stride - region.src_x
        || region.dst_x >= mode.width
        || region.dst_y >= mode.height
        || region.w > mode.width - region.dst_x
        || region.h > mode.height - region.dst_y
    {
        return false;
    }
    let last_row = u64::from(region.src_y) + u64::from(region.h) - 1;
    let last_end = last_row * u64::from(stride) + u64::from(region.src_x) + u64::from(region.w);
    last_end <= u64::from(pixels)
}
fn merge_rect(a: Rect, b: Rect) -> Rect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (u64::from(a.x) + u64::from(a.w)).max(u64::from(b.x) + u64::from(b.w));
    let bottom = (u64::from(a.y) + u64::from(a.h)).max(u64::from(b.y) + u64::from(b.h));
    Rect {
        x: left,
        y: top,
        w: u32::try_from(right - u64::from(left)).expect("merged rectangle width"),
        h: u32::try_from(bottom - u64::from(top)).expect("merged rectangle height"),
    }
}
fn copy_name(out: &mut [u8], name: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let count = name.len().min(out.len() - 1);
    if count != 0 {
        out[..count].copy_from_slice(&name[..count]);
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
unsafe fn copy_to_visible(dst: *mut u8, src: &[u8]) {
    let word_len = src.len() & !3;
    if dst.addr() & 3 == 0 && src.as_ptr().addr() & 3 == 0 {
        let dst_words = dst.cast::<u32>();
        let src_words = src.as_ptr().cast::<u32>();
        for index in 0..word_len / 4 {
            unsafe {
                dst_words
                    .add(index)
                    .write_volatile(src_words.add(index).read())
            };
        }
        for (index, &byte) in src.iter().enumerate().skip(word_len) {
            unsafe { dst.add(index).write_volatile(byte) };
        }
        return;
    }
    if dst.addr() & 3 == 0 && word_len == src.len() {
        let dst_words = dst.cast::<u32>();
        for index in 0..word_len / 4 {
            let offset = index * 4;
            let value = u32::from(src[offset])
                | (u32::from(src[offset + 1]) << 8)
                | (u32::from(src[offset + 2]) << 16)
                | (u32::from(src[offset + 3]) << 24);
            unsafe { dst_words.add(index).write_volatile(value) };
        }
        return;
    }
    for (index, &byte) in src.iter().enumerate() {
        unsafe { dst.add(index).write_volatile(byte) };
    }
}
fn read_xrgb32(src: &[u8], offset: usize) -> u32 {
    if offset + 3 >= src.len() {
        return 0;
    }
    let p = unsafe { src.as_ptr().add(offset) };
    if p.addr() & 3 == 0 {
        unsafe { p.cast::<u32>().read() }
    } else {
        u32::from(src[offset])
            | (u32::from(src[offset + 1]) << 8)
            | (u32::from(src[offset + 2]) << 16)
            | (u32::from(src[offset + 3]) << 24)
    }
}
unsafe extern "C" fn rejected_commit(_: usize, _: u64, _: *const BootSnapshot) -> u32 {
    1
}
unsafe extern "C" fn rejected_restore(_: usize, _: u64, _: *const BootSnapshot) -> bool {
    false
}
pub fn probe_rejected_takeover() -> bool {
    let Some(boot) = (unsafe { (*context()).saved_boot }) else {
        return false;
    };
    let before = backend_state();
    if before.state != backend::BOOTFB || before.policy != 0 {
        return false;
    }
    let owner = u32::MAX as usize;
    let mut frame = boot.framebuffer;
    let mapping = Mapping {
        kind: 2,
        ..boot.mapping
    };
    let candidate = Backend {
        owner,
        adapter_id: u32::MAX,
        target: Target {
            name: Text::literal(b"rejection-probe"),
            kind: NATIVE,
            flags: CPU_PRESENT,
            framebuffer: &mut frame,
            mode: boot.mode,
            mapping,
        },
        context: 0,
        commit: Commit(rejected_commit),
        restore: Restore(rejected_restore),
        begin_cpu: None,
        end_cpu: None,
    };
    let Ok(generation) = prepare_native(candidate) else {
        return false;
    };
    let result = match commit_native(owner, generation) {
        Ok(value) => value,
        Err(_) => {
            let _ = abort_native(owner, generation);
            return false;
        }
    };
    let after = backend_state();
    result == 1
        && after.state == backend::BOOTFB
        && after.reason == 5
        && after.generation == before.generation
        && !retains_driver_owner(owner)
        && !firmware::r4os_rust_display_firmware_gate.is_revoked()
}
pub fn operation_names(flags: u32) -> &'static [u8] {
    if flags & 128 != 0 {
        b"present/blit/fill/rect/xrgb32"
    } else if flags & 64 != 0 {
        b"present/blit/fill/rect/packed32"
    } else if flags & 32 != 0 {
        b"fill/rect"
    } else {
        b"none"
    }
}
pub fn present_reason_name(reason: u8) -> &'static [u8] {
    match reason {
        0 => b"none",
        1 => b"fill",
        2 => b"rect",
        3 => b"packed32-present",
        4 => b"xrgb32-present",
        _ => panic!("present reason enum"),
    }
}
pub fn mapping_kind_name(kind: u8) -> &'static [u8] {
    match kind {
        0 => b"none",
        1 => b"bootloader-framebuffer",
        2 => b"native-scanout",
        _ => panic!("mapping kind enum"),
    }
}
pub fn cache_policy_name(policy: u8) -> &'static [u8] {
    match policy {
        0 => b"unknown",
        1 => b"bootloader-default",
        2 => b"pat-write-combining",
        3 => b"write-combining-unsupported",
        4 => b"write-combining-failed",
        _ => panic!("cache policy enum"),
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

fn bootfb_put_packed32(device: *mut Device, x: u64, y: u64, color: u32) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    unsafe {
        fb::put_packed(&frame.read(), x, y, color);
    }
    true
}
fn bootfb_put_xrgb32(device: *mut Device, x: u64, y: u64, rgb: u32) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    unsafe {
        fb::put_packed(&frame, x, y, fb::pack_rgb(&frame, rgb));
    }
    true
}
unsafe fn bootfb_present_packed32_rect(
    device: *mut Device,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    src: &[u8],
    stride: u64,
) -> bool {
    unsafe { bootfb_copy_packed32_rect(device, REASON_PACKED32, x, y, w, h, src, stride) }
}
unsafe fn bootfb_present_xrgb32_rect(
    device: *mut Device,
    x: u64,
    y: u64,
    w: u64,
    h: u64,
    src: &[u8],
    stride: u64,
) -> bool {
    let frame = unsafe { (*device).frame };
    if frame.is_null() {
        return false;
    }
    let frame = unsafe { frame.read() };
    if !fb::supports_rgb32(&frame) || w == 0 || h == 0 || x >= frame.width || y >= frame.height {
        return false;
    }
    let clipped_w = w.min(frame.width - x);
    let clipped_h = h.min(frame.height - y);
    let stride_bytes = stride * 4;
    let row_bytes = clipped_w * 4;
    if (src.len() as u64) < (clipped_h - 1) * stride_bytes + row_bytes {
        return false;
    }
    if fb::is_native_xrgb32(&frame) {
        return unsafe {
            bootfb_copy_packed32_rect(
                device,
                REASON_XRGB32,
                x,
                y,
                clipped_w,
                clipped_h,
                src,
                stride,
            )
        };
    }
    for row in 0..clipped_h {
        let offset = usize::try_from(row * stride_bytes).expect("converted row offset");
        for column in 0..clipped_w {
            let pixel_offset =
                offset + usize::try_from(column * 4).expect("converted pixel offset");
            unsafe {
                fb::put_packed(
                    &frame,
                    x + column,
                    y + row,
                    fb::pack_rgb(&frame, read_xrgb32(src, pixel_offset)),
                );
            }
        }
    }
    record_present(
        device,
        REASON_XRGB32,
        u32::try_from(x).expect("converted x"),
        u32::try_from(y).expect("converted y"),
        u32::try_from(clipped_w).expect("converted width"),
        u32::try_from(clipped_h).expect("converted height"),
        true,
    );
    true
}
