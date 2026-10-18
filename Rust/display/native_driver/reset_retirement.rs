//! Exact reset_retirement operations of the parent owner; no duplicated state.
use super::*;

pub(super) unsafe fn reset_result(reset: *mut ResetRecord, step: ResetStep, result: i32) -> i32 {
    unsafe {
        if (*reset).reported_step != step || (*reset).reported_result != result {
            (*reset).reported_step = step;
            (*reset).reported_result = result;
            let mut message = Message::<192>::new();
            writeln!(
                &mut message,
                "[GFX-RESET] owner={} generation={} retire={} result={}",
                (*reset).driver.id,
                (*reset).generation,
                step.name(),
                result
            )
            .expect("original reset bound");
            message.emit();
            let current = display::backend_state();
            let b = BRIDGE.0.get();
            let mut detail = Message::<320>::new();
            writeln!(&mut detail,"[GFX-RESET] display={}/{} owner={} generation={} adapter={} bridge-generation={} timeline={} reference={}/{} cpu-lease={} pending={}/{} replacement={}",
                state_name(current.state),reason_name(current.reason),current.owner,current.generation,current.adapter_id,
                (*b).generation,(*b).timeline,(*b).reference.id,(*b).driver_reference.id,(*b).cpu_lease.id,
                u8::from((*b).pending.is_some()),u8::from((*b).image_pending.is_some()),u8::from((*REPLACEMENT.0.get()).is_some()))
                .expect("original reset detail bound");
            detail.emit();
        }
    }
    result
}

pub(super) fn restore_after_reset(id: u32, generation: u64) -> Result<(), Error> {
    unsafe {
        if (*RESET.0.get()).is_some_and(|r| !r.retired) {
            return Err(Error::Busy);
        }
        display::restore_boot_backend(id as usize, generation)?;
        *CONSOLE.0.get() = None;
        if let Some(reset) = *RESET.0.get() {
            let restored = display::backend_state();
            if reset.driver.id == u64::from(id)
                && reset.retired
                && !reset.terminal_released
                && reset.resumed_backend.is_none()
                && restored.state == backend::BOOTFB
                && restored.owner == 0
                && restored.generation > reset.generation
                && display::restored_boot_owner(
                    id as usize,
                    restored.generation,
                    reset.backend.adapter_id,
                )
                .unwrap_or(false)
            {
                *CONSOLE.0.get() = Some(ConsoleReceipt {
                    driver: reset.driver,
                    adapter: reset.backend.adapter_id,
                    reset_generation: reset.generation,
                    restored_generation: restored.generation,
                });
            }
        }
        *RESET.0.get() = None;
    }
    Ok(())
}

pub(super) fn owns_restored_console(identity: Owner, generation: u64, adapter: u32) -> bool {
    unsafe {
        (*CONSOLE.0.get()).is_some_and(|receipt| {
            receipt.driver == identity
                && receipt.adapter == adapter
                && receipt.restored_generation == generation
                && generation > receipt.reset_generation
        })
    }
}

pub fn boot_description(generation: u64, saved: &BootSnapshot) -> a::GfxNativeBootInfo {
    let state = display::backend_state();
    let mut physical = 0;
    if !unsafe {
        paging::r4os_rust_paging_contiguous(
            saved.mapping.virt_base,
            saved.mapping.byte_len,
            &mut physical,
        )
    } {
        physical = 0
    }
    a::GfxNativeBootInfo {
        generation,
        physical_address: physical,
        byte_length: saved.mapping.byte_len,
        width: saved.mode.width,
        height: saved.mode.height,
        pitch: saved.mode.pitch,
        format: if framebuffer::is_native_xrgb32(&saved.framebuffer) {
            layout::XRGB8888
        } else {
            0
        },
        policy: state.policy,
        state: state.state,
        ..Default::default()
    }
}

pub(super) fn require_quiesced_fence(fence: model::Fence) -> Result<(), Error> {
    let status = match queue::query(fence) {
        Ok(s) => s,
        Err(Error::Stale) => return Ok(()),
        Err(e) => return Err(e),
    };
    if status.phase != model::TERMINAL || status.device_active || status.resources_held {
        Err(Error::Busy)
    } else {
        Ok(())
    }
}

pub(super) fn reset_pointer() -> Result<*mut ResetRecord, Error> {
    unsafe {
        (*RESET.0.get())
            .as_mut()
            .map(ptr::from_mut)
            .ok_or(Error::Stale)
    }
}

/// # Safety
/// R4D keeps both full fixed records live through the admitted reset operation.
// Original receipt/validation branch scopes retained during the port.
#[allow(clippy::collapsible_if)]
pub unsafe fn device_reset(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    generation: u64,
    quiesced: u32,
    out: *mut a::GfxNativeState,
) -> i32 {
    if input.is_null()
        || irq_router::in_dispatch()
        || !unsafe { buffer_api::valid_output(out) }
        || quiesced > 1
        || unsafe {
            (*input).version != 1
                || ((*input).size as usize) < size_of::<a::GfxBackendBinding>()
                || generation == 0
                || !identity.valid()
                || identity.kind != 2
                || !valid_adapter((*input).adapter_id)
                || (*input).milestone != a::gfx_queue_milestone_device_execution
                || ((*input).device_generation == 0) != ((*input).reset_generation == 0)
        }
    {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    let result = (|| -> Result<a::GfxNativeState, i32> {
        unsafe {
            let input = input.read();
            let b = BRIDGE.0.get();
            let mut rebuilt_headless = false;
            let restored_console = quiesced == 0
                && input.device_generation == 0
                && input.reset_generation == 0
                && display::restored_boot_owner(identity.id as usize, generation, input.adapter_id)
                    .map_err(code)?;
            if restored_console
                && (!owns_restored_console(identity, generation, input.adapter_id)
                    || (*b).driver_owner.id != 0)
            {
                return Err(a::gfx_output_error_stale);
            }
            if let Some(reset) = *RESET.0.get()
                && let Some(resumed) = reset.resumed_backend
            {
                if quiesced != 0
                    || generation != reset.generation
                    || reset.driver != identity
                    || !same(&resumed, &input)
                    || reset.terminal_released
                    || !reset.retired
                {
                    return Err(a::gfx_output_error_stale);
                }
                rebuilt_headless = true;
            }
            if (*RESET.0.get()).is_none() || rebuilt_headless || restored_console {
                if quiesced != 0 {
                    return Err(a::gfx_output_error_stale);
                }
                let current = display::backend_state();
                if restored_console {
                    // Common begin_reset authenticates the exact restored epoch.
                } else if current.state == backend::PREPARING {
                    if current.pending_owner != identity.id as usize
                        || current.pending_generation != generation
                        || current.pending_adapter_id != input.adapter_id
                    {
                        return Err(a::gfx_output_error_stale);
                    }
                } else if current.owner != identity.id as usize
                    || current.generation != generation
                    || current.adapter_id != input.adapter_id
                {
                    return Err(a::gfx_output_error_stale);
                }
                if (*b).driver_owner.id != 0
                    && ((*b).driver_owner != identity
                        || !same(&(*b).registration.backend, &input)
                        || ((*b).generation != generation
                            && !(current.state == backend::UNAVAILABLE
                                && current.reason == 7
                                && (*b).generation < generation)))
                {
                    return Err(a::gfx_output_error_stale);
                }
                mode_work::stopped_driver(driver_id(identity));
                cursor_work::stopped_driver(driver_id(identity));
                output_runtime::stopped_driver(driver_id(identity));
                let next =
                    display::begin_device_reset(identity.id as usize, generation, input.adapter_id)
                        .map_err(code)?;
                (*b).cancelled = true;
                *RESET.0.get() = Some(ResetRecord {
                    driver: identity,
                    backend: input,
                    original_generation: generation,
                    generation: next,
                    retired: false,
                    terminal_released: false,
                    resumed_backend: None,
                    reported_step: ResetStep::None,
                    reported_result: 0,
                });
                *CONSOLE.0.get() = None;
            }
            let reset = reset_pointer().map_err(code)?;
            if (*reset).terminal_released
                || (*reset).driver != identity
                || !same(&(*reset).backend, &input)
                || generation
                    != if quiesced == 0 {
                        (*reset).original_generation
                    } else {
                        (*reset).generation
                    }
            {
                return Err(a::gfx_output_error_stale);
            }
            if quiesced == 1 && !(*reset).retired {
                // Keep the original public error mapping and changed-checkpoint
                // logging. A completed prefix stays published across Busy retries.
                let retired = (|| -> Result<(), (ResetStep, Error)> {
                    if input.device_generation != 0 {
                        if let Err(e) =
                            queue::unregister_native(driver_id(identity), binding(input), true)
                            && e != Error::Stale
                        {
                            return Err((ResetStep::Queue, e));
                        }
                    }
                    mode_work::retire_after_reset(identity, input, (*reset).generation)
                        .map_err(|e| (ResetStep::Mode, e))?;
                    cursor_work::retire_after_reset(identity, input)
                        .map_err(|e| (ResetStep::Cursor, e))?;
                    output_runtime::retire_after_reset(identity, input)
                        .map_err(|e| (ResetStep::Output, e))?;
                    display::retire_device_reset(identity.id as usize, (*reset).generation)
                        .map_err(|e| (ResetStep::Display, e))?;
                    if !discard() {
                        return Err((ResetStep::Bridge, Error::Busy));
                    }
                    Ok(())
                })();
                if let Err((step, error)) = retired {
                    // Cursor has its own wider public capacity/stale projection.
                    let public = if step == ResetStep::Cursor {
                        cursor_work::code(error)
                    } else {
                        code(error)
                    };
                    return Err(reset_result(reset, step, public));
                }
                (*reset).retired = true;
                reset_result(reset, ResetStep::Retired, a::gfx_output_ok);
            }
            Ok(a::GfxNativeState {
                generation: (*reset).generation,
                state: display::backend_state().state,
                outcome: a::gfx_output_outcome_lost,
                retained: 1,
                ..Default::default()
            })
        }
    })();
    match result {
        Ok(value) => {
            unsafe { out.write(value) };
            a::gfx_output_ok
        }
        Err(public) => public,
    }
}

/// # Safety
/// Exact initialized backend/output remain resident for this R4D operation.
pub unsafe fn resume_headless(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    generation: u64,
    out: *mut a::GfxNativeState,
) -> i32 {
    if !unsafe { valid_transition_records(identity, input, generation, out) } {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    let result = (|| -> Result<a::GfxNativeState, Error> {
        unsafe {
            let input = input.read();
            let reset = reset_pointer()?;
            if (*reset).driver != identity
                || (*reset).generation != generation
                || (*reset).terminal_released
                || input.adapter_id != (*reset).backend.adapter_id
                || input.device_generation <= (*reset).backend.device_generation
            {
                return Err(Error::Stale);
            }
            if !(*reset).retired {
                return Err(Error::Busy);
            }
            let current = display::backend_state();
            if current.state != backend::RECOVERING
                || current.owner != identity.id as usize
                || current.generation != generation
                || current.adapter_id != input.adapter_id
                || (*BRIDGE.0.get()).driver_owner.id != 0
            {
                return Err(Error::Stale);
            }
            if let Some(resumed) = (*reset).resumed_backend {
                if !same(&resumed, &input) {
                    return Err(Error::Stale);
                }
            } else {
                queue::validate_output_binding(driver_id(identity), input)?;
                if input.milestone != a::gfx_queue_milestone_device_execution {
                    return Err(Error::Stale);
                }
                (*reset).resumed_backend = Some(input);
            }
            Ok(a::GfxNativeState {
                generation,
                state: a::display_state_recovering,
                outcome: a::gfx_output_outcome_lost,
                retained: 1,
                ..Default::default()
            })
        }
    })();
    match result {
        Ok(value) => {
            unsafe { out.write(value) };
            a::gfx_output_ok
        }
        Err(e) => code(e),
    }
}

pub(super) unsafe fn valid_transition_records(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    generation: u64,
    out: *const a::GfxNativeState,
) -> bool {
    !input.is_null()
        && input
            .addr()
            .is_multiple_of(core::mem::align_of::<a::GfxBackendBinding>())
        && !irq_router::in_dispatch()
        && unsafe {
            buffer_api::valid_output(out)
                && (*input).version == 1
                && (*input).size as usize >= size_of::<a::GfxBackendBinding>()
        }
        && generation != 0
        && identity.valid()
        && identity.kind == 2
}

/// # Safety
/// R4D retains exact backend and initialized state output through release.
pub unsafe fn release_terminal(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    generation: u64,
    out: *mut a::GfxNativeState,
) -> i32 {
    if !unsafe { valid_transition_records(identity, input, generation, out) } {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    let result = (|| -> Result<a::GfxNativeState, Error> {
        unsafe {
            let reset = reset_pointer()?;
            if (*reset).driver != identity
                || !same(&(*reset).backend, &input.read())
                || generation != (*reset).generation
            {
                return Err(Error::Stale);
            }
            if !(*reset).retired || (*reset).resumed_backend.is_some() {
                return Err(Error::Busy);
            }
            if !(*reset).terminal_released {
                display::release_terminal(identity.id as usize, generation, (*input).adapter_id)?;
                (*reset).terminal_released = true
            }
            Ok(a::GfxNativeState {
                generation,
                state: display::backend_state().state,
                outcome: a::gfx_output_outcome_lost,
                retained: 0,
                ..Default::default()
            })
        }
    })();
    match result {
        Ok(value) => {
            unsafe { out.write(value) };
            a::gfx_output_ok
        }
        Err(e) => code(e),
    }
}
