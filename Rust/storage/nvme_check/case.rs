//! Existing resident one-sector read, scoped to an exact device and request.
use super::*;
use crate::storage::{block_device as device, block_io as io};
struct Binding;
impl Drop for Binding {
    fn drop(&mut self) {
        unsafe {
            if (*session()).pin_active {
                block_registry::r4os_rust_block_registry_unpin((*session()).pin.as_mut_ptr());
                (*session()).pin_active = false;
            }
            (*session()).buffer_active = false;
        }
    }
}
fn status(index: usize) -> Option<c::StorageBackendStatus> {
    let mut out = c::StorageBackendStatus::default();
    unsafe { crate::driver::api_storage::r4os_rust_driver_storage_status(index, &mut out) }
        .then_some(out)
}
fn unavailable(reason: u32) {
    unsafe {
        (*session()).report.status = c::kernel_nvme_check_status_unavailable;
        (*session()).report.unavailable_reason = reason;
    }
}
pub(super) fn run() {
    let s = session();
    unsafe {
        (*s).report.fail_stage = c::kernel_nvme_check_stage_binding;
    }
    let mut found = None;
    for index in 0..crate::storage::access_state::MAX_DEVICES {
        let mut pin = MaybeUninit::uninit();
        if !unsafe { block_registry::r4os_rust_block_registry_pin(index, pin.as_mut_ptr()) } {
            continue;
        }
        let mut pin = unsafe { pin.assume_init() };
        let view = unsafe { crate::storage::block_execution::view(pin.device) };
        if view.bus != 3 {
            unsafe {
                block_registry::r4os_rust_block_registry_unpin(&mut pin);
            }
            continue;
        }
        unsafe {
            (*s).pin.write(pin);
            (*s).pin_active = true;
        }
        found = Some((index, view));
        break;
    }
    let Some((index, view)) = found else {
        unavailable(c::kernel_nvme_check_unavailable_device);
        return;
    };
    let _binding = Binding;
    let report = unsafe { &mut (*s).report };
    report.device_slot = index as u32;
    report.device_generation = view.io.reference.generation;
    report.sector_size = view.io.sector_size;
    let Some(before_status) = status(index) else {
        unavailable(c::kernel_nvme_check_unavailable_backend);
        return;
    };
    report.driver_state_before = before_status.state;
    if !view.io.has_submit {
        unavailable(c::kernel_nvme_check_unavailable_backend);
        return;
    }
    if before_status.state & c::storage_backend_status_observation_supported == 0 {
        unavailable(c::kernel_nvme_check_unavailable_observation);
        return;
    }
    let size = view.io.sector_size as usize;
    if size == 0 || size > 4096 {
        unavailable(c::kernel_nvme_check_unavailable_geometry);
        return;
    }
    if CANCEL.load(Ordering::Acquire) {
        report.status = c::kernel_nvme_check_status_cancelled;
        return;
    }
    if let Some((_, _, before)) = device::probe_facts(view.io.reference) {
        report.worker_requests_before = before.worker_requests;
        report.worker_completions_before = before.worker_completions;
        report.async_submissions_before = before.async_submissions;
        report.async_completions_before = before.async_completions;
    }
    report.fail_stage = c::kernel_nvme_check_stage_read;
    unsafe {
        (*s).buffer_active = true;
        buffer().write_bytes(0xa5, size);
    }
    let read = unsafe {
        io::read_observed(
            view.io.reference,
            0,
            buffer(),
            size,
            ptr::addr_of!((*s).subscription),
            ptr::addr_of_mut!((*s).receipt),
        )
    };
    let receipt = unsafe { (*s).receipt };
    report.request_id = receipt.request_id;
    report.backend_handle = receipt.backend_handle;
    report.native_receipt_flags = receipt.flags;
    if let Some((_, _, after)) = device::probe_facts(view.io.reference) {
        report.worker_requests_after = after.worker_requests;
        report.worker_completions_after = after.worker_completions;
        report.async_submissions_after = after.async_submissions;
        report.async_completions_after = after.async_completions;
    }
    if let Some(after) = status(index) {
        report.driver_state_after = after.state;
    }
    if read {
        report.flags |= c::kernel_nvme_check_flag_work_complete;
        let mut checksum = 0xcbf29ce484222325u64;
        for i in 0..size {
            checksum = (checksum ^ u64::from(unsafe { buffer().add(i).read_volatile() }))
                .wrapping_mul(0x100000001b3);
        }
        report.data_checksum = checksum;
    }
    let (count, observed) = observer::snapshot();
    report.callback_count = count;
    if let Some(observed) = observed {
        report.observation_flags = observed.flags;
        report.observed_handle = observed.handle;
        report.controller_index = observed.controller_index;
        report.controller_generation = observed.controller_generation;
        report.namespace_generation = observed.namespace_generation;
        report.namespace_id = observed.namespace_id;
        report.namespace_slot = observed.namespace_slot;
        report.queue_id = observed.queue_id;
        report.command_id = observed.command_id;
        report.hardware_status = observed.hardware_status;
        report.driver_result = observed.result;
        report.completed_bytes = observed.bytes;
        report.irq_mode = observed.irq_mode;
        report.irq_generation = observed.irq_generation;
        report.work_sequence = observed.work_sequence;
        report.command_sequence = observed.command_sequence;
        report.start_tick = observed.start_tick;
        report.finish_tick = observed.finish_tick;
    }
    report.status = c::kernel_nvme_check_status_failed;
    if !read {
        return;
    }
    report.fail_stage = c::kernel_nvme_check_stage_observation;
    if count != 1
        || observed.is_none()
        || report.request_id == 0
        || receipt.backend_handle == 0
        || report.observed_handle != receipt.backend_handle
        || receipt.bytes != size as u64
        || receipt.flags != 15
        || report.driver_result != 0
        || report.completed_bytes != size as u32
        || report.hardware_status != 0
        || report.controller_generation == 0
        || report.namespace_generation == 0
        || report.namespace_id == 0
        || report.queue_id == 0
        || report.command_id > u16::MAX as u32
        || report.finish_tick < report.start_tick
    {
        return;
    }
    report.fail_stage = c::kernel_nvme_check_stage_irq;
    if report.irq_mode == c::storage_observation_irq_none {
        unavailable(c::kernel_nvme_check_unavailable_irq);
        return;
    }
    if !matches!(
        report.irq_mode,
        c::storage_observation_irq_msi | c::storage_observation_irq_msix
    ) || report.observation_flags != 15
        || report.irq_generation == 0
        || report.work_sequence == 0
    {
        return;
    }
    report.fail_stage = 0;
    report.status = if CANCEL.load(Ordering::Acquire) {
        c::kernel_nvme_check_status_cancelled
    } else {
        c::kernel_nvme_check_status_passed
    };
}
