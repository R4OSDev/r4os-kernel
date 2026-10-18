//! Exact modes operations of the parent owner; no duplicated state.
use super::*;

pub fn enable_modes(identity: Owner, backend: a::GfxBackendBinding) -> Result<(), Error> {
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if !(*b).ready
            || (*b).driver_owner != identity
            || !same(&(*b).registration.backend, &backend)
            || (*b).cancelled
            || (*b).hardware_restored
            || display::backend_state().state != super::super::backend_state::SOFTWARE_NATIVE
        {
            return Err(Error::Stale);
        }
        queue::validate_output_binding(driver_id(identity), backend)?;
        (*b).modes_enabled = true;
        Ok(())
    }
}

pub fn modes_enabled(id: u32, backend: a::GfxBackendBinding) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    unsafe {
        let b = BRIDGE.0.get();
        (*b).ready
            && (*b).modes_enabled
            && (*b).driver_owner.id == u64::from(id)
            && same(&(*b).registration.backend, &backend)
            && !(*b).cancelled
            && !(*b).hardware_restored
    }
}

pub fn mode_binding(assignment: a::GfxScanoutState) -> Result<ModeBinding, Error> {
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if !(*b).ready
            || !(*b).modes_enabled
            || (*b).cancelled
            || (*b).hardware_restored
            || display::backend_state().state != backend::SOFTWARE_NATIVE
        {
            return Err(Error::Unsupported);
        }
        if assignment.output.adapter_id != (*b).registration.backend.adapter_id
            || assignment.output.device_generation != (*b).registration.backend.device_generation
        {
            return Err(Error::Stale);
        }
        queue::validate_output_binding(driver_id((*b).driver_owner), (*b).registration.backend)?;
        if assignment.output.connector_id != (*b).registration.output.connector_id {
            let additional = output_runtime::mode_binding(assignment)?;
            if additional.driver != (*b).driver_owner
                || !same(&additional.backend, &(*b).registration.backend)
            {
                return Err(Error::Stale);
            }
            return Ok(ModeBinding {
                driver: additional.driver,
                backend: additional.backend,
                generation: additional.target.display_generation,
                has_additional: true,
                additional,
            });
        }
        Ok(ModeBinding {
            driver: (*b).driver_owner,
            backend: (*b).registration.backend,
            generation: (*b).generation,
            ..Default::default()
        })
    }
}

pub fn prepare_mode_encoding(
    caller: Owner,
    assignment: a::GfxScanoutState,
    mode: a::GfxOutputMode,
    encoded_format: Option<u32>,
) -> Result<a::GfxBufferReference, Error> {
    if encoded_format
        .is_some_and(|format| !matches!(format, layout::XRGB8888 | layout::XRGB2101010))
    {
        return Err(Error::Unsupported);
    }
    let _execution = execution()?;
    let selected = mode_binding(assignment)?;
    if selected.has_additional {
        return native_additional_mode::prepare(caller, assignment, mode, selected.additional);
    }
    unsafe {
        let b = BRIDGE.0.get();
        if (*REPLACEMENT.0.get()).is_some()
            || (*b).cpu_lease.id != 0
            || (*b).pending.is_some()
            || !image_idle()
        {
            return Err(Error::Busy);
        }
        if assignment.source_x != 0
            || assignment.source_y != 0
            || assignment.destination_x != 0
            || assignment.destination_y != 0
            || assignment.source_width != mode.width
            || assignment.source_height != mode.height
            || assignment.destination_width != mode.width
            || assignment.destination_height != mode.height
            || assignment.rotation != 0
            || assignment.color != 0
            || assignment.bits_per_color != 8
        {
            return Err(Error::Unsupported);
        }
        let source = buffer_api::handle(assignment.buffer)?;
        let _bo = BoLock::prepared(2, 1)?;
        let d = buffers::read().describe(source, caller)?;
        let usage = 2 | 4 | 32;
        if d.format != layout::XRGB8888
            || d.location != 0
            || !d.binding.portable()
            || d.modifier != 0
            || d.plane_count != 1
            || d.planes[0].offset != 0
            || d.usage & usage != usage
            || d.width != mode.width
            || d.height != mode.height
            || d.planes[0].pitch > u64::from(u32::MAX)
            || d.bytes != d.planes[0].pitch * u64::from(d.height)
        {
            return Err(Error::Unsupported);
        }
        if buffers::read().buffer_for(source, caller)?
            == buffers::read().buffer_for((*b).reference, OWNER)?
        {
            return Err(Error::Invalid);
        }
        let reference = buffers::table().share(source, OWNER)?;
        let mut driver_reference = Handle::EMPTY;
        let mut read_lease = Handle::EMPTY;
        let prepared = (|| -> Result<a::GfxBufferReference, Error> {
            driver_reference = buffers::table().share(source, (*b).driver_owner)?;
            let mapped = buffers::map_locked(reference, OWNER, 1, 0, d.bytes)?;
            buffers::unmap_cpu_locked(mapped.lease, OWNER)?;
            read_lease = buffers::table()
                .use_resource(reference, OWNER, 2, 0, d.bytes)?
                .lease;
            let wire = buffer_api::reference_locked(driver_reference, (*b).driver_owner)?;
            let mut frame = (*b).frame;
            frame.address = mapped.backing.cpu_address as *mut u8;
            frame.width = u64::from(d.width);
            frame.height = u64::from(d.height);
            frame.pitch = d.planes[0].pitch;
            frame.edid = ptr::null();
            frame.edid_size = 0;
            *REPLACEMENT.0.get() = Some(Replacement {
                old: Surface {
                    reference: (*b).reference,
                    driver_reference: (*b).driver_reference,
                    read_lease: Handle::EMPTY,
                    bytes: (*b).bytes,
                    frame: (*b).frame,
                    output_format: (*b).output_format,
                    encoded_output: (*b).encoded_output,
                },
                new: Surface {
                    reference,
                    driver_reference,
                    read_lease,
                    bytes: d.bytes,
                    frame,
                    output_format: encoded_format.unwrap_or(layout::XRGB8888),
                    encoded_output: encoded_format.is_some(),
                },
                using_new: false,
            });
            Ok(wire)
        })();
        if prepared.is_err() {
            if read_lease.id != 0 {
                buffers::table()
                    .end_use(read_lease, OWNER, true)
                    .expect("new mode read lease")
            }
            if driver_reference.id != 0 {
                buffers::table()
                    .drop_reference(driver_reference, (*b).driver_owner)
                    .expect("new driver reference")
            }
            buffers::table()
                .drop_reference(reference, OWNER)
                .expect("new display reference");
        }
        prepared
    }
}

pub(super) unsafe fn change() -> Result<*mut Replacement, Error> {
    unsafe {
        (*REPLACEMENT.0.get())
            .as_mut()
            .map(ptr::from_mut)
            .ok_or(Error::Stale)
    }
}

pub fn start_mode(operation: u32, expected: ModeBinding) -> Result<(), Error> {
    let _execution = execution()?;
    if expected.has_additional {
        return native_additional_mode::start(operation, expected.additional);
    }
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).driver_owner != expected.driver
            || (*b).generation != expected.generation
            || !same(&(*b).registration.backend, &expected.backend)
            || (*b).cancelled
            || (*b).hardware_restored
        {
            return Err(Error::Stale);
        }
        let change = change()?;
        if (*b).cpu_lease.id != 0 || (*b).pending.is_some() || !image_idle() {
            return Err(Error::Busy);
        }
        queue::validate_output_binding(driver_id(expected.driver), expected.backend)?;
        let surface = if operation == a::gfx_mode_operation_apply {
            ptr::addr_of_mut!((*change).old)
        } else if operation == a::gfx_mode_operation_rollback {
            ptr::addr_of_mut!((*change).new)
        } else {
            return Ok(());
        };
        if (*surface).read_lease.id == 0 {
            let _bo = BoLock::prepared(0, 1)?;
            (*surface).read_lease = buffers::table()
                .use_resource((*surface).reference, OWNER, 2, 0, (*surface).bytes)?
                .lease;
        }
    }
    Ok(())
}

pub(super) unsafe fn release_read(surface: *mut Surface) -> Result<(), Error> {
    unsafe {
        if (*surface).read_lease.id == 0 {
            return Ok(());
        }
        let _bo = BoLock::acquire();
        buffers::table().end_use((*surface).read_lease, OWNER, true)?;
        (*surface).read_lease = Handle::EMPTY;
    }
    Ok(())
}

pub(super) unsafe fn release_surface(surface: *mut Surface) -> Result<(), Error> {
    unsafe {
        release_read(surface)?;
        if (*surface).driver_reference.id != 0 {
            buffers::drop_reference((*surface).driver_reference, (*BRIDGE.0.get()).driver_owner)?;
            (*surface).driver_reference = Handle::EMPTY;
        }
        if (*surface).reference.id != 0 {
            buffers::drop_reference((*surface).reference, OWNER)?;
            (*surface).reference = Handle::EMPTY
        }
    }
    Ok(())
}

pub(super) unsafe fn select_surface(surface: *const Surface) -> Result<(), Error> {
    unsafe {
        let b = BRIDGE.0.get();
        let before = (*b).frame;
        (*b).frame = (*surface).frame;
        if let Err(e) = display::replace_native_frame_locked(
            (*b).driver_owner.id as usize,
            (*b).generation,
            ptr::addr_of_mut!((*b).frame),
            (*surface).bytes,
        ) {
            (*b).frame = before;
            return Err(e);
        }
        (*b).reference = (*surface).reference;
        (*b).driver_reference = (*surface).driver_reference;
        (*b).bytes = (*surface).bytes;
        (*b).output_format = (*surface).output_format;
        (*b).encoded_output = (*surface).encoded_output;
        outputs::native_active(driver_id((*b).driver_owner), (*b).registration.output, true);
    }
    Ok(())
}

pub fn settle_mode(operation: u32, outcome: u32) -> Result<(), Error> {
    let _execution = execution()?;
    unsafe {
        let change = change()?;
        let b = BRIDGE.0.get();
        if outcome == a::gfx_output_outcome_lost {
            display::lose_native_mode_locked((*b).driver_owner.id as usize, (*b).generation)?;
            outputs::native_active(
                driver_id((*b).driver_owner),
                (*b).registration.output,
                false,
            );
            return Ok(());
        }
        if outcome == a::gfx_output_outcome_applied {
            if operation == a::gfx_mode_operation_apply {
                release_read(ptr::addr_of_mut!((*change).new))?;
                select_surface(ptr::addr_of!((*change).new))?;
                (*change).using_new = true;
                return Ok(());
            }
            if operation != a::gfx_mode_operation_confirm || !(*change).using_new {
                return Err(Error::Invalid);
            }
            select_surface(ptr::addr_of!((*change).new))?;
            release_surface(ptr::addr_of_mut!((*change).old))?;
        } else if outcome == a::gfx_output_outcome_old_preserved {
            release_read(ptr::addr_of_mut!((*change).old))?;
            select_surface(ptr::addr_of!((*change).old))?;
            release_surface(ptr::addr_of_mut!((*change).new))?;
        } else {
            return Err(Error::Invalid);
        }
        *REPLACEMENT.0.get() = None;
    }
    Ok(())
}

pub fn settle_mode_for(operation: u32, outcome: u32, expected: ModeBinding) -> Result<(), Error> {
    if expected.has_additional {
        native_additional_mode::settle(operation, outcome)
    } else {
        settle_mode(operation, outcome)
    }
}

pub fn abort_prepared_mode(expected: ModeBinding) {
    let _ = settle_mode_for(
        a::gfx_mode_operation_apply,
        a::gfx_output_outcome_old_preserved,
        expected,
    );
}

pub fn retire_mode_after_reset(
    identity: Owner,
    backend: a::GfxBackendBinding,
) -> Result<(), Error> {
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).driver_owner.id != 0 {
            if (*b).driver_owner != identity || !same(&(*b).registration.backend, &backend) {
                return Err(Error::Stale);
            }
            if (*b).cpu_lease.id != 0 {
                let _bo = BoLock::acquire();
                buffers::unmap_cpu_locked((*b).cpu_lease, OWNER)?;
                (*b).cpu_lease = Handle::EMPTY;
            }
            if let Some(fence) = (*b).pending {
                require_quiesced_fence(fence)?;
                if let Err(e) = queue::drop_reference(OWNER, fence)
                    && e != Error::Stale
                {
                    return Err(e);
                }
                (*b).pending = None;
            }
            if let Some(fence) = (*b).image_pending {
                require_quiesced_fence(fence)?;
                (*b).image_pending = None
            }
            (*b).modes_enabled = false;
            (*b).cancelled = true;
            if (*REPLACEMENT.0.get()).is_some() {
                let change = change()?;
                (*b).reference = Handle::EMPTY;
                (*b).driver_reference = Handle::EMPTY;
                release_surface(ptr::addr_of_mut!((*change).old))?;
                release_surface(ptr::addr_of_mut!((*change).new))?;
                *REPLACEMENT.0.get() = None;
            }
        }
    }
    native_additional_mode::retire_after_reset(identity, backend)
}
