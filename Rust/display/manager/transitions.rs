//! Exact transitions operations of the parent owner; no duplicated state.
use super::*;

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

pub(super) fn release_boot_locked(owner: usize, generation: u64, reason: u32) {
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

pub(super) fn prepare_native_impl(
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

pub(super) fn mode_pending() -> bool {
    super::super::native_driver::mode_pending()
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

pub(super) fn restore_boot_locked(owner: usize, generation: u64) -> Result<(), Error> {
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
