//! Exact cpu_access operations of the parent owner; no duplicated state.
use super::*;

pub(super) unsafe extern "C" fn begin_cpu(_: usize) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).encoded_output
            || (*b).cpu_lease.id != 0
            || (*b).reference.id == 0
            || (*b).pending.is_some()
            || !image_idle()
        {
            return false;
        }
        let Ok(_bo) = BoLock::prepared(0, 1) else {
            return false;
        };
        let Ok(use_) = buffers::map_locked((*b).reference, OWNER, 1, 0, (*b).bytes) else {
            return false;
        };
        if use_.backing.cpu_address != (*b).frame.address.addr() as u64 {
            let _ = buffers::unmap_cpu_locked(use_.lease, OWNER);
            return false;
        }
        (*b).cpu_lease = use_.lease;
        true
    }
}

pub(super) fn image_idle() -> bool {
    unsafe {
        let b = BRIDGE.0.get();
        let Some(fence) = (*b).image_pending else {
            return true;
        };
        let status = match queue::query(fence) {
            Ok(s) => s,
            Err(e) => {
                if e != Error::Stale {
                    return false;
                }
                (*b).image_pending = None;
                return true;
            }
        };
        if status.phase != model::TERMINAL
            || (!(*b).image_direct && (status.device_active || status.resources_held))
        {
            return false;
        }
        (*b).image_pending = None;
        true
    }
}

pub fn output_target(adapter: u32, head: u32) -> Result<a::GfxOutputTarget, Error> {
    if let Some(target) = output_runtime::target_at(adapter, head)? {
        return Ok(target);
    }
    let _display = DisplayExecution::acquire()?;
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        let state = display::backend_state();
        let target = if adapter == 0 {
            outputs::active_target(
                0,
                a::GfxOutputId {
                    connector_id: 1,
                    device_generation: 1,
                    ..Default::default()
                },
                state.generation,
            )?
        } else {
            if !(*b).ready
                || (*b).cancelled
                || (*b).hardware_restored
                || state.state != backend::SOFTWARE_NATIVE
            {
                return Err(Error::Unsupported);
            }
            queue::validate_output_binding(
                driver_id((*b).driver_owner),
                (*b).registration.backend,
            )?;
            outputs::active_target(
                driver_id((*b).driver_owner),
                (*b).registration.output,
                (*b).generation,
            )?
        };
        if target.adapter_id != adapter || target.head_id != head {
            return Err(Error::Unsupported);
        }
        Ok(target)
    }
}

/// # Safety
/// Caller keeps all request pointer payloads resident through exact admission.
pub unsafe fn submit_image(
    caller: Owner,
    timeline: u64,
    submission: &resource::Submission<'_>,
    request: &resource::Request,
) -> Result<model::Status, Error> {
    if output_runtime::contains(request.display_target) {
        return unsafe {
            output_runtime::submit_image(
                caller,
                timeline,
                &output_runtime::Image {
                    request,
                    deadline: submission.deadline_ns,
                    frame_key: submission.frame_key,
                    dependencies: submission.dependencies.as_ptr(),
                    dependencies_len: submission.dependencies.len(),
                },
            )
        };
    }
    let _display = DisplayExecution::acquire()?;
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if !(*b).ready
            || (*b).cancelled
            || (*b).hardware_restored
            || display::backend_state().state != backend::SOFTWARE_NATIVE
        {
            return Err(Error::Unsupported);
        }
        if mode_pending() || (*b).cpu_lease.id != 0 || (*b).pending.is_some() || !image_idle() {
            return Err(Error::Busy);
        }
        if outputs::native_paused(driver_id((*b).driver_owner), (*b).registration.output) {
            return Err(Error::Unavailable);
        }
        if request.display_target.connector_id != 0 {
            let current = outputs::active_target(
                driver_id((*b).driver_owner),
                (*b).registration.output,
                (*b).generation,
            )?;
            if !super::super::output_target::same(&current, &request.display_target) {
                return Err(Error::Stale);
            }
        }
        queue::validate_output_binding(driver_id((*b).driver_owner), (*b).registration.backend)?;
        let descriptor = {
            let _bo = BoLock::acquire();
            buffers::read().describe(request.source, caller)?
        };
        if !matches!(
            request.operation,
            resource::PRESENT | resource::DIRECT_PRESENT
        ) || u64::from(descriptor.width) != (*b).frame.width
            || u64::from(descriptor.height) != (*b).frame.height
            || descriptor.format != (*b).output_format
            || !matches!(descriptor.location, 0 | 1)
            || descriptor.plane_count != 1
            || descriptor.planes[0].offset != 0
        {
            return Err(Error::Unsupported);
        }
        let accepted = queue::submit_display_image(
            caller,
            timeline,
            submission,
            request,
            binding((*b).registration.backend),
        )?;
        (*b).image_pending = Some(accepted.fence);
        (*b).image_direct = request.operation == resource::DIRECT_PRESENT;
        Ok(accepted)
    }
}

pub(super) fn cpu_failure(stage: &str, reason: &str) -> u32 {
    let mut message = Message::<320>::new();
    if unsafe { super::super::ownership::raw_guard(EXECUTION.0.get(), 0, 0) } {
        unsafe {
            let b = BRIDGE.0.get();
            writeln!(&mut message,"[GFX-CPU] stage={} reason={} owner={} generation={} timeline={} lease={} pending={} image-pending={}",
                stage,reason,(*b).driver_owner.id,(*b).generation,(*b).timeline,(*b).cpu_lease.id,
                u8::from((*b).pending.is_some()),u8::from((*b).image_pending.is_some())).expect("original CPU log bound");
        }
    } else {
        writeln!(
            &mut message,
            "[GFX-CPU] stage={} reason={} bridge-snapshot=not-owned",
            stage, reason
        )
        .expect("original CPU log bound");
    }
    message.emit();
    2
}

pub(super) fn cpu_error(stage: &str, error: Error) -> u32 {
    let mut reason = Message::<32>::new();
    write!(&mut reason, "{error:?}").expect("error name bound");
    cpu_failure(stage, reason.text())
}

pub(super) unsafe extern "C" fn end_cpu(_: usize, changed: bool, damage: *const Rect) -> u32 {
    let Ok(_execution) = execution() else {
        return cpu_failure("finish-entry", "Busy");
    };
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).cpu_lease.id == 0 {
            return cpu_failure("finish-lease", "Missing");
        }
        {
            let bo = BoLock::acquire();
            if let Err(e) = buffers::unmap_cpu_locked((*b).cpu_lease, OWNER) {
                drop(bo);
                return cpu_error("finish-unmap", e.into());
            }
            (*b).cpu_lease = Handle::EMPTY;
        }
        if !changed {
            return 0;
        }
        let full = Rect {
            w: u32::try_from((*b).frame.width).expect("frame width"),
            h: u32::try_from((*b).frame.height).expect("frame height"),
            ..Default::default()
        };
        let requested = if damage.is_null() {
            full
        } else {
            damage.read()
        };
        if requested.w == 0
            || requested.h == 0
            || u64::from(requested.x) >= (*b).frame.width
            || u64::from(requested.y) >= (*b).frame.height
            || u64::from(requested.w) > (*b).frame.width - u64::from(requested.x)
            || u64::from(requested.h) > (*b).frame.height - u64::from(requested.y)
        {
            return cpu_failure("finish-damage", "Invalid");
        }
        let rect = if (*b).cpu_refresh_required {
            full
        } else {
            requested
        };
        (*b).cpu_refresh_required = true;
        if outputs::native_paused(driver_id((*b).driver_owner), (*b).registration.output) {
            return 0;
        }
        let offset = u64::from(rect.y) * (*b).frame.pitch + u64::from(rect.x) * 4;
        let span = u64::from(rect.h - 1) * (*b).frame.pitch + u64::from(rect.w) * 4;
        let Some(now) = monotonic::now_nanoseconds() else {
            return cpu_failure("finish-clock", "Unavailable");
        };
        let accepted = match queue::submit(
            OWNER,
            (*b).timeline,
            &resource::Submission {
                deadline_ns: now.saturating_add(3_000_000_000),
                frame_key: 0,
                dependencies: &[],
            },
            &resource::Request {
                operation: resource::UPLOAD,
                source: (*b).reference,
                source_offset: offset,
                bytes: span,
                ..Default::default()
            },
        ) {
            Ok(value) => value,
            Err(error) => {
                if matches!(error, Error::Busy | Error::Capacity) {
                    if !(*b).cpu_retry_reported {
                        (*b).cpu_retry_reported = true;
                        cpu_error("retry-submit", error);
                    }
                    return 1;
                }
                return cpu_error("finish-submit", error);
            }
        };
        (*b).pending = Some(accepted.fence);
        let complete =
            match queue::wait(accepted.fence, u64::from(timer::frequency().max(1)) * 4, 1) {
                Ok(status) => status,
                Err(e) => return cpu_error("finish-wait", e),
            };
        if let Err(e) = queue::drop_reference(OWNER, accepted.fence) {
            return cpu_error("finish-drop", e);
        }
        (*b).pending = None;
        let succeeded = complete.result == model::COMPLETE
            && !complete.device_active
            && !complete.resources_held;
        if succeeded {
            (*b).cpu_refresh_required = false;
            (*b).cpu_retry_reported = false
        }
        if succeeded
            || (complete.result == model::CANCELLED
                && !complete.device_active
                && !complete.resources_held)
        {
            return 0;
        }
        let mut reason = Message::<192>::new();
        let result = match complete.result {
            0 => "pending",
            1 => "complete",
            2 => "cancelled",
            3 => "dropped",
            4 => "device_lost",
            5 => "timeout",
            6 => "failed",
            7 => "dependency_failed",
            _ => unreachable!(),
        };
        write!(
            &mut reason,
            "result={} device-active={} resources-held={} fence={}:{}",
            result,
            u8::from(complete.device_active),
            u8::from(complete.resources_held),
            accepted.fence.timeline,
            accepted.fence.point
        )
        .expect("original result log bound");
        cpu_failure("finish-result", reason.text())
    }
}

pub(super) fn discard() -> bool {
    unsafe {
        let b = BRIDGE.0.get();
        if (*REPLACEMENT.0.get()).is_some() || !image_idle() {
            return false;
        }
        if (*b).cpu_lease.id != 0 {
            let _bo = BoLock::acquire();
            if buffers::unmap_cpu_locked((*b).cpu_lease, OWNER).is_err() {
                return false;
            }
            (*b).cpu_lease = Handle::EMPTY;
        }
        if let Some(fence) = (*b).pending {
            if queue::drop_reference(OWNER, fence).is_err() {
                return false;
            }
            (*b).pending = None;
        }
        if (*b).timeline != 0 {
            queue::unbind_display_queue(
                driver_id((*b).driver_owner),
                binding((*b).registration.backend),
                (*b).timeline,
            );
            if let Err(e) = queue::close(OWNER, (*b).timeline)
                && e != Error::Stale
            {
                return false;
            }
            (*b).timeline = 0;
        }
        if (*b).reference.id != 0 {
            if buffers::drop_reference((*b).reference, OWNER).is_err() {
                return false;
            }
            (*b).reference = Handle::EMPTY;
        }
        if (*b).driver_reference.id != 0 {
            if buffers::drop_reference((*b).driver_reference, (*b).driver_owner).is_err() {
                return false;
            }
            (*b).driver_reference = Handle::EMPTY;
        }
        b.write(Bridge::EMPTY);
        RETAINED.store(0, Ordering::Release);
        true
    }
}

pub fn cursor_binding() -> Result<cursor_work::Binding, Error> {
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if !(*b).ready
            || (*b).cancelled
            || (*b).hardware_restored
            || display::backend_state().state != backend::SOFTWARE_NATIVE
        {
            return Err(Error::Unsupported);
        }
        if mode_pending() || (*b).cpu_lease.id != 0 || (*b).pending.is_some() || !image_idle() {
            return Err(Error::Busy);
        }
        queue::validate_output_binding(driver_id((*b).driver_owner), (*b).registration.backend)?;
        Ok(cursor_work::Binding {
            driver: (*b).driver_owner,
            backend: (*b).registration.backend,
            generation: (*b).generation,
            head: outputs::cursor_head(driver_id((*b).driver_owner), (*b).registration.output)?,
            timeline: 0,
            point: 0,
        })
    }
}
