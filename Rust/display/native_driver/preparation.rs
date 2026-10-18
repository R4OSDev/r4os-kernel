//! Exact preparation operations of the parent owner; no duplicated state.
use super::*;

/// # Safety
/// R4D retains the initialized output header and writable admitted prefix.
pub unsafe fn boot_info(out: *mut a::GfxNativeBootInfo) -> i32 {
    if irq_router::in_dispatch() || !unsafe { buffer_api::valid_output(out) } {
        return a::gfx_output_error_invalid;
    }
    let Some(saved) = display::boot_snapshot() else {
        return a::gfx_output_error_unavailable;
    };
    unsafe {
        out.write(boot_description(
            display::backend_state().generation,
            &saved,
        ))
    };
    a::gfx_output_ok
}

pub(super) fn state_result(id: u32, outcome: u32) -> a::GfxNativeState {
    let state = display::backend_state();
    let generation = unsafe {
        if retained(id) && !(*BRIDGE.0.get()).ready {
            (*BRIDGE.0.get()).generation
        } else if state.pending_owner == id as usize {
            state.pending_generation
        } else {
            state.generation
        }
    };
    a::GfxNativeState {
        generation,
        state: state.state,
        outcome,
        retained: u32::from(display::retains_driver_owner(id as usize) || retained(id)),
        ..Default::default()
    }
}

pub fn valid_adapter(adapter: u32) -> bool {
    for index in 0..pci::count() {
        let Some(item) = pci::device_at(index) else {
            return false;
        };
        if item.class_code == 3
            && adapter
                == (0x0100_0000
                    | (u32::from(item.bus) << 8)
                    | (u32::from(item.device) << 3)
                    | u32::from(item.function))
        {
            return true;
        }
    }
    false
}

/// # Safety
/// Exact admitted R4D keeps request/output resident through preparation.
pub unsafe fn prepare_request(
    identity: Owner,
    input: *const a::GfxNativeRegistration,
    held_generation: u64,
    reset_generation: u64,
    out: *mut a::GfxNativeState,
) -> i32 {
    if input.is_null()
        || irq_router::in_dispatch()
        || unsafe {
            (*input).version != 1
                || ((*input).size as usize) < size_of::<a::GfxNativeRegistration>()
                || !buffer_api::valid_output(out)
        }
    {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    unsafe {
        if reset_generation != 0 {
            let Some(reset) = *RESET.0.get() else {
                return a::gfx_output_error_stale;
            };
            if reset.driver != identity
                || reset.generation != reset_generation
                || (*input).backend.adapter_id != reset.backend.adapter_id
                || (*input).backend.device_generation <= reset.backend.device_generation
            {
                return a::gfx_output_error_stale;
            }
            if !reset.retired {
                return a::gfx_output_error_busy;
            }
            if reset.terminal_released
                || reset
                    .resumed_backend
                    .is_some_and(|resumed| !same(&resumed, &(*input).backend))
            {
                return a::gfx_output_error_stale;
            }
        } else if (*RESET.0.get()).is_some() {
            return a::gfx_output_error_busy;
        }
        let result = match prepare_impl(identity, input.read(), held_generation, reset_generation) {
            Ok(result) => result,
            Err(error) => {
                if (*BRIDGE.0.get()).driver_owner == identity && !(*BRIDGE.0.get()).ready {
                    out.write(state_result(
                        driver_id(identity),
                        a::gfx_output_outcome_lost,
                    ));
                }
                return code(error);
            }
        };
        if reset_generation != 0 {
            *RESET.0.get() = None
        }
        out.write(result);
    }
    a::gfx_output_ok
}

pub(super) fn driver_id(identity: Owner) -> u32 {
    u32::try_from(identity.id).expect("original R4D id range")
}

pub(super) fn prepare_impl(
    identity: Owner,
    request: a::GfxNativeRegistration,
    held_generation: u64,
    reset_generation: u64,
) -> Result<a::GfxNativeState, Error> {
    unsafe {
        let b = BRIDGE.0.get();
        if identity.kind != 2 || !identity.valid() || (*b).driver_owner.id != 0 {
            return Err(Error::Busy);
        }
        if request.version != 1
            || (request.size as usize) < size_of::<a::GfxNativeRegistration>()
            || request.reference.reserved0 != 0
            || request.commit_callback < 0xffff800000000000
            || request.restore_callback < 0xffff800000000000
            || !valid_adapter(request.backend.adapter_id)
            || request.backend.milestone != 1
        {
            return Err(Error::Invalid);
        }
        let end = request
            .name
            .iter()
            .position(|&c| c == 0)
            .ok_or(Error::Invalid)?;
        if end == 0 || request.name[end..].iter().any(|&c| c != 0) {
            return Err(Error::Invalid);
        }
        let saved = display::boot_snapshot().ok_or(Error::Unavailable)?;
        if !framebuffer::is_native_xrgb32(&saved.framebuffer) {
            return Err(Error::Unsupported);
        }
        outputs::validate_native(
            driver_id(identity),
            request.backend,
            request.output,
            saved.mode.width,
            saved.mode.height,
            held_generation != 0,
        )?;
        let caller_reference = buffer_api::handle(request.reference)?;
        let (reference, address, descriptor) = {
            let _bo = BoLock::prepared(1, 1)?;
            let d = buffers::read().describe(caller_reference, identity)?;
            if d.format != layout::XRGB8888
                || d.location != 0
                || !d.binding.portable()
                || d.modifier != 0
                || d.plane_count != 1
                || d.planes[0].offset != 0
                || d.width != saved.mode.width
                || d.height != saved.mode.height
                || d.planes[0].pitch > u64::from(u32::MAX)
                || d.planes[0].pitch != u64::from(d.width) * 4
                || d.bytes != d.planes[0].pitch * u64::from(d.height)
                || d.usage & 6 != 6
            {
                return Err(Error::Unsupported);
            }
            let reference = buffers::table().share(caller_reference, OWNER)?;
            let mapped = match buffers::map_locked(reference, OWNER, 1, 0, d.bytes) {
                Ok(mapped) => mapped,
                Err(error) => {
                    buffers::table()
                        .drop_reference(reference, OWNER)
                        .expect("exact new reference");
                    return Err(error.into());
                }
            };
            if let Err(error) = buffers::unmap_cpu_locked(mapped.lease, OWNER) {
                buffers::table()
                    .drop_reference(reference, OWNER)
                    .expect("exact new reference");
                return Err(error.into());
            }
            (reference, mapped.backing.cpu_address, d)
        };
        b.write(Bridge {
            driver_owner: identity,
            registration: request,
            reference,
            bytes: descriptor.bytes,
            frame: saved.framebuffer,
            held_generation,
            ..Bridge::EMPTY
        });
        RETAINED.store(driver_id(identity), Ordering::Release);
        let prepared = (|| -> Result<a::GfxNativeState, Error> {
            (*b).frame.address = address as *mut u8;
            (*b).frame.pitch = descriptor.planes[0].pitch;
            (*b).frame.edid = ptr::null();
            (*b).frame.edid_size = 0;
            let mut mode = saved.mode;
            mode.pitch = u32::try_from((*b).frame.pitch).expect("validated pitch");
            let candidate = Backend {
                owner: identity.id as usize,
                adapter_id: request.backend.adapter_id,
                target: Target {
                    name: Text {
                        pointer: ptr::addr_of!((*b).registration.name).cast(),
                        len: end,
                    },
                    kind: display::NATIVE,
                    flags: 255,
                    mode,
                    mapping: core_display::Mapping {
                        kind: 2,
                        cache_policy: 0,
                        virt_base: address,
                        byte_len: (*b).bytes,
                        volatile_cpu_writes: false,
                    },
                    framebuffer: ptr::addr_of_mut!((*b).frame),
                },
                context: b.addr(),
                commit: core_display::Commit(commit),
                restore: core_display::Restore(restore),
                begin_cpu: Some(core_display::BeginCpu(begin_cpu)),
                end_cpu: Some(core_display::EndCpu(end_cpu)),
            };
            (*b).generation = if reset_generation != 0 {
                display::prepare_recovered_native(candidate, held_generation, reset_generation)?
            } else if held_generation != 0 {
                display::prepare_held_native(candidate, held_generation)?
            } else {
                display::prepare_native(candidate)?
            };
            (*b).timeline = queue::open(
                OWNER,
                model::Config {
                    binding: binding(request.backend),
                    milestone: 1,
                    capacity: 1,
                    ..Default::default()
                },
            )?;
            queue::bind_display_queue(
                driver_id(identity),
                binding(request.backend),
                (*b).timeline,
            )?;
            display::bind_presentation_stats(
                identity.id as usize,
                identity.generation,
                request.backend,
                (*b).generation,
            )?;
            (*b).ready = true;
            Ok(state_result(
                driver_id(identity),
                a::gfx_output_outcome_validated,
            ))
        })();
        if prepared.is_err() {
            cancel_preparation()
        }
        prepared
    }
}

pub(super) fn cancel_preparation() {
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).generation != 0
            && display::abort_native((*b).driver_owner.id as usize, (*b).generation).is_err()
        {
            return;
        }
        (*b).cancelled = true;
        let _ = discard();
    }
}

/// # Safety
/// Output follows the initialized native-state header contract.
pub unsafe fn transition(
    id: u32,
    generation: u64,
    operation: u32,
    out: *mut a::GfxNativeState,
) -> i32 {
    if id == 0
        || irq_router::in_dispatch()
        || !unsafe { buffer_api::valid_output(out) }
        || operation > 2
    {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    unsafe {
        let b = BRIDGE.0.get();
        if (*REPLACEMENT.0.get()).is_some() {
            return a::gfx_output_error_busy;
        }
        if (*b).driver_owner.id != u64::from(id)
            && !(operation == 2
                && (*b).driver_owner.id == 0
                && display::retains_driver_owner(id as usize))
        {
            return a::gfx_output_error_stale;
        }
        let result = (|| -> Result<u32, Error> {
            Ok(match operation {
                0 => {
                    if !(*b).ready || (*b).cancelled {
                        return Err(Error::Busy);
                    }
                    let result = display::commit_native(id as usize, generation)?;
                    if result == 1 {
                        (*b).cancelled = true;
                        (*b).ready = false;
                        if !discard() {
                            a::gfx_output_outcome_lost
                        } else {
                            a::gfx_output_outcome_old_preserved
                        }
                    } else if result == 0 {
                        a::gfx_output_outcome_applied
                    } else {
                        a::gfx_output_outcome_lost
                    }
                }
                1 => {
                    if generation != (*b).generation {
                        return Err(Error::Stale);
                    }
                    if !(*b).cancelled {
                        display::abort_native(id as usize, generation)?
                    }
                    (*b).cancelled = true;
                    (*b).ready = false;
                    if discard() {
                        a::gfx_output_outcome_old_preserved
                    } else {
                        a::gfx_output_outcome_lost
                    }
                }
                2 => match restore_after_reset(id, generation) {
                    Ok(()) => a::gfx_output_outcome_applied,
                    Err(Error::RestoreFailed) => a::gfx_output_outcome_lost,
                    Err(e) => return Err(e),
                },
                _ => unreachable!(),
            })
        })();
        match result {
            Ok(outcome) => out.write(state_result(id, outcome)),
            Err(error) => return code(error),
        }
    }
    a::gfx_output_ok
}

pub(super) unsafe extern "C" fn commit(
    _: usize,
    generation: u64,
    saved: *const BootSnapshot,
) -> u32 {
    let Ok(_execution) = execution() else {
        return 1;
    };
    unsafe {
        let saved = saved.read();
        let b = BRIDGE.0.get();
        if outputs::validate_native(
            driver_id((*b).driver_owner),
            (*b).registration.backend,
            (*b).registration.output,
            saved.mode.width,
            saved.mode.height,
            (*b).held_generation != 0,
        )
        .is_err()
            || !begin_cpu(0)
        {
            return 1;
        }
        let copied = if (*b).held_generation != 0 {
            copy_boot_snapshot(
                (*b).driver_owner,
                (*b).held_generation,
                &saved,
                ptr::addr_of!((*b).frame),
                false,
            )
        } else {
            for y in 0..u64::from(saved.mode.height) {
                for x in 0..u64::from(saved.mode.width) {
                    let source = saved
                        .framebuffer
                        .address
                        .add((y * saved.framebuffer.pitch + x * 4) as usize)
                        .cast::<u32>();
                    let target = (*b)
                        .frame
                        .address
                        .add((y * (*b).frame.pitch + x * 4) as usize)
                        .cast::<u32>();
                    target.write(source.read_volatile());
                }
            }
            true
        };
        let released = end_cpu(0, false, ptr::null());
        if !copied || released != 0 {
            return 1;
        }
        if !api_admission::r4os_rust_driver_api_enter(
            driver_id((*b).driver_owner),
            u64::from(timer::frequency().max(1)),
        ) {
            return 1;
        }
        let callback: Callback = core::mem::transmute((*b).registration.commit_callback as usize);
        let boot = boot_description(generation, &saved);
        let result = callback((*b).registration.context, generation, &boot);
        if result == 1 {
            outputs::native_active(driver_id((*b).driver_owner), (*b).registration.output, true)
        }
        let _ = api_admission::r4os_rust_driver_api_leave();
        match result {
            1 => 0,
            2 => 1,
            _ => 2,
        }
    }
}

pub(super) unsafe fn copy_boot_snapshot(
    identity: Owner,
    generation: u64,
    saved: *const BootSnapshot,
    target: *const Framebuffer,
    padding: bool,
) -> bool {
    let identity = super::super::boot_driver::Owner {
        kind: identity.kind,
        id: identity.id,
        generation: identity.generation,
    };
    unsafe {
        super::super::boot_driver::r4os_rust_boot_driver_copy(
            &identity,
            generation,
            saved.cast(),
            target,
            padding,
        )
    }
}

pub(super) unsafe extern "C" fn restore(
    _: usize,
    generation: u64,
    saved: *const BootSnapshot,
) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    unsafe {
        let b = BRIDGE.0.get();
        if (*REPLACEMENT.0.get()).is_some() {
            return false;
        }
        if !(*b).hardware_restored {
            if !api_admission::r4os_rust_driver_api_enter(
                driver_id((*b).driver_owner),
                u64::from(timer::frequency().max(1)),
            ) {
                return false;
            }
            let callback: Callback =
                core::mem::transmute((*b).registration.restore_callback as usize);
            let boot = boot_description(generation, &saved.read());
            let restored = callback((*b).registration.context, generation, &boot) == 1;
            let _ = api_admission::r4os_rust_driver_api_leave();
            if !restored {
                return false;
            }
            (*b).hardware_restored = true;
        }
        if (*b).held_generation != 0 && !(*b).pixels_restored {
            if !copy_boot_snapshot(
                (*b).driver_owner,
                (*b).held_generation,
                saved,
                ptr::addr_of!((*saved).framebuffer),
                true,
            ) {
                return false;
            }
            (*b).pixels_restored = true;
        }
        if !(*b).queue_stopped {
            outputs::native_active(
                driver_id((*b).driver_owner),
                (*b).registration.output,
                false,
            );
            if let Err(e) = queue::unregister_native(
                driver_id((*b).driver_owner),
                binding((*b).registration.backend),
                true,
            ) && !matches!(e, Error::Busy | Error::Stale)
            {
                return false;
            }
            (*b).queue_stopped = true;
        }
        discard()
    }
}
