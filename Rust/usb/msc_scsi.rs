//! Original SCSI request construction/response policy; parsing remains in the
//! existing USBSCSI.R4P. Raw medium storage stays pinned by its BOT owner.
use super::{
    msc::medium,
    msc_bot, msc_retry,
    msc_types::{Direction, ScsiCommand, Status},
    timing,
    wait_types::Deadline,
};
use crate::{
    contract as c, fs::vfs_types::Bytes, loader::r4p, sched::sync_types::Text,
    storage::block_execution,
};
use core::{
    mem, ptr,
    sync::atomic::{AtomicI32, AtomicU64, Ordering},
};
pub(super) static BUILDS: AtomicU64 = AtomicU64::new(0);
pub(super) static PARSES: AtomicU64 = AtomicU64::new(0);
pub(super) static DISPATCH_FAILURES: AtomicU64 = AtomicU64::new(0);
pub(super) static LAST_RESULT: AtomicI32 = AtomicI32::new(0);
fn role() -> Bytes {
    Bytes {
        data: b"usb.scsi_block".as_ptr(),
        len: 14,
    }
}
fn active() -> bool {
    unsafe { r4p::r4os_rust_r4p_has_active(role()) }
}
fn dispatch(opcode: u32, op: &mut c::UsbScsiBlockOp) -> bool {
    let input = c::ProtocolBuffer {
        data: (op as *mut c::UsbScsiBlockOp).cast(),
        len: mem::size_of::<c::UsbScsiBlockOp>() as u32,
        capacity: mem::size_of::<c::UsbScsiBlockOp>() as u32,
        flags: 0,
        reserved: 0,
    };
    let mut out = c::ProtocolBuffer::default();
    let result = unsafe { r4p::r4os_rust_r4p_dispatch(role(), opcode, &input, &mut out) };
    LAST_RESULT.store(result, Ordering::Relaxed);
    if result != c::usb_scsi_result_ok {
        DISPATCH_FAILURES.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    true
}
unsafe fn build(current: *mut Status, opcode: u32, lba: u64, sectors: u16) -> Option<ScsiCommand> {
    if !active() {
        return None;
    }
    let mut op = c::UsbScsiBlockOp {
        lba: lba as u32,
        lba64: lba,
        sectors,
        block_count: u32::from(sectors),
        logical_block_size: unsafe {
            if (*current).sector_size == 0 {
                512
            } else {
                (*current).sector_size
            }
        },
        ..c::UsbScsiBlockOp::default()
    };
    if !dispatch(opcode, &mut op)
        || op.result != c::usb_scsi_result_ok
        || op.cdb_len == 0
        || op.cdb_len > 16
    {
        return None;
    }
    let mut cmd = ScsiCommand {
        cdb: [0; 16],
        len: op.cdb_len as usize,
        direction: match op.direction {
            c::usb_scsi_dir_in => Direction::In,
            c::usb_scsi_dir_out => Direction::Out,
            _ => Direction::None,
        },
        transfer_len: op.transfer_len,
    };
    cmd.cdb[..cmd.len].copy_from_slice(&op.cdb[..cmd.len]);
    BUILDS.fetch_add(1, Ordering::Relaxed);
    Some(cmd)
}
unsafe fn data(current: *mut Status) -> *mut u8 {
    unsafe { ptr::addr_of_mut!((*medium(current)).data_buffer).cast() }
}
unsafe fn command_scratch(current: *mut Status, cmd: ScsiCommand) -> bool {
    assert!(
        cmd.transfer_len as usize <= 4096,
        "SCSI command scratch length"
    );
    unsafe {
        msc_bot::command(
            current,
            &cmd.cdb[..cmd.len],
            cmd.direction,
            cmd.transfer_len,
            data(current),
            cmd.transfer_len as usize,
        )
    }
}
pub(super) unsafe fn inquiry(current: *mut Status) -> bool {
    unsafe {
        let Some(cmd) = build(current, c::usb_scsi_op_build_inquiry, 0, 0) else {
            (*current).reason = Text::literal(b"SCSI INQUIRY command build failed");
            return msc_bot::fail_command(current);
        };
        if !command_scratch(current, cmd) {
            return false;
        }
        let actual = (*current).last_data_actual_len as usize;
        if actual < 5 {
            msc_bot::set_data_length_reason(
                current,
                b"inquiry-short",
                (*current).last_data_actual_len,
            );
            return msc_bot::fail_command(current);
        }
        let declared =
            (cmd.transfer_len as usize).min(usize::from(data(current).add(4).read()) + 5);
        if actual < declared {
            msc_bot::set_data_length_reason(
                current,
                b"inquiry-truncated",
                (*current).last_data_actual_len,
            );
            return msc_bot::fail_command(current);
        }
        (*current).model_len = 0;
        if actual >= 32 && declared >= 32 {
            for i in 8..32 {
                let byte = data(current).add(i).read();
                if !(32..=126).contains(&byte) {
                    (*current).model_len = 0;
                    break;
                }
                ptr::addr_of_mut!((*current).model)
                    .cast::<u8>()
                    .add((*current).model_len)
                    .write(byte);
                (*current).model_len += 1;
            }
            while (*current).model_len != 0 && (*current).model[(*current).model_len - 1] == b' ' {
                (*current).model_len -= 1;
            }
        }
        true
    }
}
pub(super) unsafe fn test_unit_ready(current: *mut Status) -> bool {
    unsafe {
        let Some(cmd) = build(current, c::usb_scsi_op_build_test_unit_ready, 0, 0) else {
            return msc_bot::fail_command(current);
        };
        command_scratch(current, cmd)
    }
}
pub(super) unsafe fn invalidate_sense(current: *mut Status) {
    unsafe {
        (*current).sense_valid = false;
        (*current).sense_for_opcode = 0;
        (*current).sense_key = 0;
        (*current).sense_asc = 0;
        (*current).sense_ascq = 0;
    }
}
pub(super) unsafe fn request_sense(current: *mut Status) -> bool {
    unsafe {
        let failed = (*current).last_opcode;
        invalidate_sense(current);
        let Some(cmd) = build(current, c::usb_scsi_op_build_request_sense, 0, 0) else {
            return msc_bot::fail_command(current);
        };
        (*current).sense_requests += 1;
        if !command_scratch(current, cmd) {
            (*current).last_opcode = failed;
            return false;
        }
        (*current).last_opcode = failed;
        let actual = (*current).last_data_actual_len as usize;
        if !parse_sense(current, data(current), actual, failed) {
            msc_bot::set_data_length_reason(
                current,
                b"sense-short-or-invalid",
                (*current).last_data_actual_len,
            );
            return msc_bot::fail_command(current);
        }
        true
    }
}
unsafe fn parse_input(
    bytes: *const u8,
    len: usize,
    opcode: u32,
    failed: u8,
) -> Option<c::UsbScsiBlockOp> {
    if !active() || len > c::usb_scsi_max_data {
        return None;
    }
    let mut op = c::UsbScsiBlockOp {
        allocation_len: len as u32,
        failed_opcode: failed,
        ..c::UsbScsiBlockOp::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(bytes, op.data.as_mut_ptr(), len);
    }
    if !dispatch(opcode, &mut op) || op.result != c::usb_scsi_result_ok {
        return None;
    }
    Some(op)
}
unsafe fn parse_sense(current: *mut Status, bytes: *const u8, len: usize, failed: u8) -> bool {
    unsafe {
        let Some(op) = parse_input(bytes, len, c::usb_scsi_op_parse_sense, failed) else {
            return false;
        };
        (*current).sense_valid = true;
        (*current).sense_for_opcode = failed;
        (*current).sense_key = op.sense_key;
        (*current).sense_asc = op.sense_asc;
        (*current).sense_ascq = op.sense_ascq;
        if (*current).block_registered
            && (matches!(op.sense_asc, 0x28 | 0x3a) || (op.sense_asc == 0x2a && op.sense_ascq == 9))
        {
            (*current).media_changed = true;
        }
        match (*current).sense_key {
            2 => (*current).sense_not_ready += 1,
            5 => (*current).sense_illegal_request += 1,
            6 => (*current).sense_unit_attention += 1,
            7 => (*current).sense_write_protect += 1,
            _ => {}
        }
        if (*current).block_index != usize::MAX {
            block_execution::r4os_rust_block_device_operation(
                (*current).block_index,
                4,
                (*current).sense_for_opcode,
                (*current).sense_key,
                (*current).sense_asc,
                (*current).sense_ascq,
            );
        }
        PARSES.fetch_add(1, Ordering::Relaxed);
        true
    }
}
unsafe fn read_capacity_kind(current: *mut Status, extended: bool) -> bool {
    unsafe {
        let build_op = if extended {
            c::usb_scsi_op_build_read_capacity16
        } else {
            c::usb_scsi_op_build_read_capacity10
        };
        let Some(cmd) = build(current, build_op, 0, 0) else {
            return msc_bot::fail_command(current);
        };
        if !command_scratch(current, cmd) {
            return false;
        }
        let parse_op = if extended {
            c::usb_scsi_op_parse_capacity16
        } else {
            c::usb_scsi_op_parse_capacity10
        };
        let Some(op) = parse_input(
            data(current),
            (*current).last_data_actual_len as usize,
            parse_op,
            0,
        ) else {
            msc_bot::set_data_length_reason(
                current,
                if extended {
                    b"capacity16-short-or-invalid"
                } else {
                    b"capacity-short-or-invalid"
                },
                (*current).last_data_actual_len,
            );
            return msc_bot::fail_command(current);
        };
        (*current).sector_count = op.sector_count;
        (*current).sector_size = op.sector_size;
        (*current).capacity_format = op.capacity_format;
        PARSES.fetch_add(1, Ordering::Relaxed);
        if extended {
            (*current).capacity16_used = true;
        }
        true
    }
}
pub(super) unsafe fn read_capacity(current: *mut Status) -> bool {
    unsafe {
        read_capacity_kind(current, false)
            && ((*current).sector_count != 0 || read_capacity_kind(current, true))
    }
}
pub(super) unsafe fn mode_sense6(current: *mut Status) -> bool {
    unsafe {
        let Some(cmd) = build(current, c::usb_scsi_op_build_mode_sense6, 0, 0) else {
            return msc_bot::fail_command(current);
        };
        if !command_scratch(current, cmd) {
            return false;
        }
        let Some(op) = parse_input(
            data(current),
            (*current).last_data_actual_len as usize,
            c::usb_scsi_op_parse_mode_sense6,
            0,
        ) else {
            msc_bot::set_data_length_reason(
                current,
                b"mode-sense-short-or-invalid",
                (*current).last_data_actual_len,
            );
            return msc_bot::fail_command(current);
        };
        (*current).write_protected_known = op.write_protected_known != 0;
        (*current).write_protected = op.write_protected != 0;
        PARSES.fetch_add(1, Ordering::Relaxed);
        true
    }
}
pub(super) fn uses_read_write16(lba: u64, sectors: u16) -> bool {
    lba > u64::from(u32::MAX)
        || (sectors != 0 && u64::from(sectors - 1) > u64::from(u32::MAX) - lba)
}
pub(super) unsafe fn read(
    current: *mut Status,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
) -> bool {
    unsafe {
        let extended = uses_read_write16(lba, sectors);
        let opcode = if extended {
            c::usb_scsi_op_build_read16
        } else {
            c::usb_scsi_op_build_read10
        };
        let Some(cmd) = build(current, opcode, lba, sectors) else {
            return msc_bot::fail_command(current);
        };
        if extended {
            (*current).read16_commands = (*current).read16_commands.wrapping_add(1);
        } else {
            (*current).read10_commands = (*current).read10_commands.wrapping_add(1);
        }
        msc_bot::command(
            current,
            &cmd.cdb[..cmd.len],
            cmd.direction,
            len as u32,
            out,
            len,
        )
    }
}
pub(super) unsafe fn write(
    current: *mut Status,
    lba: u64,
    sectors: u16,
    bytes: *const u8,
    len: usize,
) -> bool {
    unsafe {
        let extended = uses_read_write16(lba, sectors);
        let opcode = if extended {
            c::usb_scsi_op_build_write16
        } else {
            c::usb_scsi_op_build_write10
        };
        let Some(cmd) = build(current, opcode, lba, sectors) else {
            return msc_bot::fail_command(current);
        };
        if extended {
            (*current).write16_commands = (*current).write16_commands.wrapping_add(1);
        } else {
            (*current).write10_commands = (*current).write10_commands.wrapping_add(1);
        }
        msc_bot::command(
            current,
            &cmd.cdb[..cmd.len],
            cmd.direction,
            len as u32,
            bytes.cast_mut(),
            len,
        )
    }
}
pub(super) unsafe fn synchronize_cache10(current: *mut Status) -> bool {
    unsafe {
        let Some(cmd) = build(current, c::usb_scsi_op_build_sync_cache10, 0, 0) else {
            return msc_bot::fail_command(current);
        };
        command_scratch(current, cmd)
    }
}
unsafe fn should_retry_failure(current: *mut Status, retries: u8) -> bool {
    unsafe {
        if (*current).last_failure_transport {
            return msc_retry::retry_scsi(
                true,
                (*current).last_recovery_ok,
                false,
                0,
                0,
                0,
                retries,
            );
        }
        request_sense(current);
        let retry = msc_retry::retry_scsi(
            (*current).last_failure_transport,
            (*current).last_recovery_ok,
            (*current).sense_valid,
            (*current).sense_key,
            (*current).sense_asc,
            (*current).sense_ascq,
            retries,
        );
        if (*current).sense_valid {
            if retry {
                (*current).transient_sense_retries += 1;
            } else {
                (*current).permanent_sense_stops += 1;
            }
        } else if retry {
            (*current).unknown_sense_retries += 1;
        }
        retry
    }
}
pub(super) unsafe fn retry_delay(
    current: *mut Status,
    reason: &'static [u8],
    opcode: u8,
    retry: u8,
) {
    unsafe {
        (*current).retry_delay_calls += 1;
        (*current).last_retry_opcode = opcode;
        (*current).last_retry_number = retry;
    }
    super::wait::milliseconds(timing::SCSI_RETRY_DELAY_MS, Text::literal(reason), retry);
}
#[derive(Clone, Copy)]
pub(super) enum RetryKind {
    Inquiry,
    Ready,
    Capacity,
}
pub(super) unsafe fn with_retry(current: *mut Status, kind: RetryKind) -> bool {
    let (duration, attempts, reason, opcode) = match kind {
        RetryKind::Inquiry => (
            timing::SCSI_INQUIRY_BUDGET_MS,
            timing::SCSI_INQUIRY_ATTEMPTS,
            b"usbmsc-inquiry-retry".as_slice(),
            0x12,
        ),
        RetryKind::Ready => (
            timing::SCSI_READY_BUDGET_MS,
            timing::SCSI_READY_ATTEMPTS,
            b"usbmsc-ready-retry".as_slice(),
            0,
        ),
        RetryKind::Capacity => (
            timing::SCSI_CAPACITY_BUDGET_MS,
            timing::SCSI_CAPACITY_ATTEMPTS,
            b"usbmsc-capacity-retry".as_slice(),
            0x25,
        ),
    };
    let budget = Deadline::begin(duration);
    let success = unsafe {
        let mut result = false;
        for attempt in 0..attempts {
            if match kind {
                RetryKind::Inquiry => inquiry(current),
                RetryKind::Ready => test_unit_ready(current),
                RetryKind::Capacity => read_capacity(current),
            } {
                result = true;
                break;
            }
            if attempt + 1 >= attempts {
                break;
            }
            if budget.expired_any() {
                (*current).retry_budget_timeouts += 1;
                break;
            }
            if !should_retry_failure(current, attempt) {
                break;
            }
            match kind {
                RetryKind::Inquiry => (*current).inquiry_retries += 1,
                RetryKind::Ready => (*current).test_unit_ready_retries += 1,
                RetryKind::Capacity => (*current).read_capacity_retries += 1,
            }
            retry_delay(current, reason, opcode, attempt + 1);
        }
        result
    };
    unsafe {
        match kind {
            RetryKind::Inquiry => (*current).inquiry_wait_elapsed_ns = budget.elapsed_nanoseconds(),
            RetryKind::Ready => (*current).ready_wait_elapsed_ns = budget.elapsed_nanoseconds(),
            RetryKind::Capacity => {
                (*current).capacity_wait_elapsed_ns = budget.elapsed_nanoseconds()
            }
        }
    }
    budget.finish();
    success
}
