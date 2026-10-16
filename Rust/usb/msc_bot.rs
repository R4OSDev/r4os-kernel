//! Original CBW/data/CSW lifetime, residue checks and terminal BOT recovery.
//! The caller retains its exact medium transaction through every parked wait.
use super::{
    host_controller, msc_scsi,
    msc_types::{Direction, Status},
    timing,
    wait_types::Deadline,
    xhci,
    xhci_api::{self, Guard},
    xhci_diag as diag,
    xhci_types::{EndpointHandle, IncidentToken},
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
pub(super) static CSWS: AtomicU64 = AtomicU64::new(0);
pub(super) static DISPATCH_FAILURES: AtomicU64 = AtomicU64::new(0);
pub(super) static LAST_RESULT: AtomicI32 = AtomicI32::new(0);
fn role() -> Bytes {
    Bytes {
        data: b"usb.msc_bot".as_ptr(),
        len: 11,
    }
}
fn active() -> bool {
    unsafe { r4p::r4os_rust_r4p_has_active(role()) }
}
fn dispatch(opcode: u32, op: &mut c::UsbMscBotOp) -> bool {
    let input = c::ProtocolBuffer {
        data: (op as *mut c::UsbMscBotOp).cast(),
        len: mem::size_of::<c::UsbMscBotOp>() as u32,
        capacity: mem::size_of::<c::UsbMscBotOp>() as u32,
        flags: 0,
        reserved: 0,
    };
    let mut out = c::ProtocolBuffer::default();
    let result = unsafe { r4p::r4os_rust_r4p_dispatch(role(), opcode, &input, &mut out) };
    LAST_RESULT.store(result, Ordering::Relaxed);
    if result != c::usb_msc_bot_result_ok && result != c::usb_msc_bot_result_command_failed {
        DISPATCH_FAILURES.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    true
}
fn build_cbw(cdb: &[u8], direction: Direction, length: u32, tag: u32, out: &mut [u8; 31]) -> bool {
    if !active() || cdb.len() > c::usb_msc_bot_max_cdb {
        return false;
    }
    let mut op = c::UsbMscBotOp {
        tag,
        transfer_len: length,
        direction: match direction {
            Direction::None => c::usb_msc_bot_dir_none,
            Direction::In => c::usb_msc_bot_dir_in,
            Direction::Out => c::usb_msc_bot_dir_out,
        },
        cdb_len: cdb.len() as u8,
        ..c::UsbMscBotOp::default()
    };
    op.cdb[..cdb.len()].copy_from_slice(cdb);
    if !dispatch(c::usb_msc_bot_op_build_cbw, &mut op) || op.result != c::usb_msc_bot_result_ok {
        return false;
    }
    out.copy_from_slice(&op.cbw[..31]);
    BUILDS.fetch_add(1, Ordering::Relaxed);
    true
}
unsafe fn parse_csw(current: *mut Status, csw: &[u8; 13], tag: u32, length: u32) -> i32 {
    if !active() {
        return c::usb_msc_bot_result_bad_csw;
    }
    let mut op = c::UsbMscBotOp {
        tag,
        transfer_len: length,
        ..c::UsbMscBotOp::default()
    };
    op.csw[..13].copy_from_slice(csw);
    if !dispatch(c::usb_msc_bot_op_parse_csw, &mut op) {
        return c::usb_msc_bot_result_bad_csw;
    }
    unsafe {
        (*current).last_csw_tag = op.csw_tag;
        (*current).last_csw_residue = op.residue;
        (*current).last_csw_status = op.status;
    }
    CSWS.fetch_add(1, Ordering::Relaxed);
    op.result
}
unsafe fn host_bulk(
    endpoint: *const EndpointHandle,
    bytes: *mut u8,
    len: usize,
    direction: u32,
) -> Option<u32> {
    let mut host = super::types::HostEndpoint::EMPTY;
    unsafe {
        xhci_api::r4os_rust_usb_xhci_usb_host_endpoint_handle(endpoint, &mut host);
    }
    let result = unsafe { host_controller::r4os_rust_usb_host_bulk(&host, bytes, len, direction) };
    if result < 0 {
        None
    } else {
        Some(result as u32)
    }
}
struct IncidentBoundary(*mut Status);
impl Drop for IncidentBoundary {
    fn drop(&mut self) {
        unsafe {
            resolve_transport_incident(self.0);
        }
    }
}
pub(super) unsafe fn command(
    current: *mut Status,
    cdb: &[u8],
    direction: Direction,
    transfer_len: u32,
    buffer: *mut u8,
    buffer_len: usize,
) -> bool {
    let _incident = IncidentBoundary(current);
    unsafe {
        (*current).last_failure_transport = false;
        (*current).last_recovery_ok = false;
        if (*current).transport_faulted || (*current).media_changed {
            (*current).last_failure_transport = true;
            (*current).reason = Text::literal(b"USBMSC transport recovery incomplete");
            return false;
        }
        if cdb.is_empty() || cdb.len() > 16 {
            (*current).reason = Text::literal(b"SCSI invalid CDB length");
            return fail_command(current);
        }
        if transfer_len as usize > buffer_len {
            (*current).reason = Text::literal(b"SCSI transfer buffer too small");
            return fail_command(current);
        }
        if cdb[0] != 3 {
            msc_scsi::invalidate_sense(current);
        }
        let mut cbw = [0; 31];
        (*current).tags = (*current).tags.wrapping_add(1);
        if (*current).tags == 0 {
            (*current).tags = 1;
        }
        let tag = (*current).tags;
        (*current).last_opcode = cdb[0];
        (*current).last_transfer_len = transfer_len;
        (*current).last_data_actual_len = 0;
        (*current).last_csw_tag = 0;
        (*current).last_csw_residue = 0;
        (*current).last_csw_status = 255;
        if !build_cbw(cdb, direction, transfer_len, tag, &mut cbw) {
            (*current).reason = Text::literal(b"SCSI BOT CBW build failed");
            return fail_command(current);
        }
        if host_bulk(
            ptr::addr_of!((*current).bulk_out_handle),
            cbw.as_mut_ptr(),
            31,
            2,
        )
        .is_none()
        {
            return fail_transport_at(current, b"cbw-out");
        }
        let mut actual = 0;
        if transfer_len != 0 {
            let (endpoint, dir, stage) = match direction {
                Direction::In => (
                    ptr::addr_of!((*current).bulk_in_handle),
                    1,
                    b"data-in".as_slice(),
                ),
                Direction::Out => (
                    ptr::addr_of!((*current).bulk_out_handle),
                    2,
                    b"data-out".as_slice(),
                ),
                Direction::None => {
                    (*current).reason = Text::literal(b"SCSI data direction missing");
                    return fail_command(current);
                }
            };
            let Some(n) = host_bulk(endpoint, buffer, transfer_len as usize, dir) else {
                return fail_transport_at(current, stage);
            };
            actual = n;
            (*current).last_data_actual_len = n;
        }
        let mut csw = [0; 13];
        let Some(csw_actual) = host_bulk(
            ptr::addr_of!((*current).bulk_in_handle),
            csw.as_mut_ptr(),
            13,
            1,
        ) else {
            return fail_transport_at(current, b"csw-in");
        };
        if csw_actual != 13 {
            return fail_transport_at(current, b"csw-short");
        }
        let result = parse_csw(current, &csw, tag, transfer_len);
        (*current).commands += 1;
        if direction == Direction::In
            && transfer_len != 0
            && matches!(
                result,
                c::usb_msc_bot_result_ok | c::usb_msc_bot_result_command_failed
            )
            && (actual > transfer_len || transfer_len - actual != (*current).last_csw_residue)
        {
            (*current).csw_residue_errors += 1;
            set_data_length_reason(current, b"data-residue-mismatch", actual);
            return fail_transport(current);
        }
        if result == c::usb_msc_bot_result_ok
            && matches!((*current).last_opcode, 0x28 | 0x2a | 0x88 | 0x8a)
            && (actual != transfer_len || (*current).last_csw_residue != 0)
        {
            (*current).short_data_errors += 1;
            set_data_length_reason(current, b"short-fixed-data", actual);
            return fail_transport(current);
        }
        match result {
            c::usb_msc_bot_result_ok => {
                resolve_transport_incident(current);
                true
            }
            c::usb_msc_bot_result_command_failed => {
                resolve_transport_incident(current);
                set_protocol_reason(current, b"command-failed");
                fail_command(current)
            }
            c::usb_msc_bot_result_bad_csw => {
                (*current).invalid_csw += 1;
                set_protocol_reason(current, b"bad-csw");
                fail_transport(current)
            }
            c::usb_msc_bot_result_tag_mismatch => {
                (*current).csw_tag_mismatches += 1;
                set_protocol_reason(current, b"tag-mismatch");
                fail_transport(current)
            }
            c::usb_msc_bot_result_residue => {
                (*current).csw_residue_errors += 1;
                set_protocol_reason(current, b"invalid-residue");
                fail_transport(current)
            }
            c::usb_msc_bot_result_phase_error => {
                (*current).csw_phase_errors += 1;
                set_protocol_reason(current, b"phase-error");
                fail_transport(current)
            }
            _ => {
                (*current).invalid_csw += 1;
                set_protocol_reason(current, b"unsupported-csw");
                fail_transport(current)
            }
        }
    }
}
pub(super) unsafe fn fail_command(current: *mut Status) -> bool {
    unsafe {
        (*current).last_failure_transport = false;
        (*current).last_recovery_ok = false;
        (*current).failures += 1;
        (*current).command_failures += 1;
    }
    false
}
unsafe fn fail_transport_at(current: *mut Status, stage: &[u8]) -> bool {
    unsafe {
        set_transport_reason(current, stage);
        fail_transport(current)
    }
}
unsafe fn fail_transport(current: *mut Status) -> bool {
    unsafe {
        (*current).last_failure_transport = true;
        (*current).transport_faulted = true;
        (*current).failures += 1;
        (*current).transport_failures += 1;
        let token = IncidentToken {
            generation: xhci_api::r4os_rust_usb_xhci_take_device_incident(ptr::addr_of!(
                (*current).device
            )),
        };
        if token.valid() {
            if (*current).transport_incident.valid()
                && (*current).transport_incident.generation != token.generation
            {
                diag::resolve((*current).transport_incident);
            }
            (*current).transport_incident = token;
        }
        if !(*current).transport_incident.valid() {
            (*current).transport_incident = diag::begin();
        }
        diag::write(b"[USBMSC] transport root op=");
        diag::write(opcode_name((*current).last_opcode));
        diag::write(b" reason=");
        diag::write(text((*current).reason));
        diag::write(b" csw=");
        diag::dec((*current).last_csw_status);
        diag::write(b" residue=");
        diag::dec((*current).last_csw_residue);
        diag::write(b" bytes=");
        diag::dec((*current).last_data_actual_len);
        diag::write(b"/");
        diag::dec((*current).last_transfer_len);
        diag::end();
        (*current).last_recovery_ok = recover_transport(current);
        diag::write(b"[USBMSC] transport recovery=");
        diag::write(if (*current).last_recovery_ok {
            b"ok"
        } else {
            b"failed"
        });
        diag::end();
        false
    }
}
unsafe fn resolve_transport_incident(current: *mut Status) {
    unsafe {
        diag::resolve((*current).transport_incident);
        (*current).transport_incident = IncidentToken::EMPTY;
    }
}
pub(super) unsafe fn text<'a>(value: Text) -> &'a [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(value.pointer, value.len) }
    }
}
fn opcode_name(opcode: u8) -> &'static [u8] {
    match opcode {
        0 => b"TUR",
        3 => b"REQUEST-SENSE",
        0x12 => b"INQUIRY",
        0x1a => b"MODE-SENSE",
        0x25 => b"READ-CAPACITY",
        0x28 => b"READ10",
        0x2a => b"WRITE10",
        0x35 => b"SYNC-CACHE",
        0x88 => b"READ16",
        0x8a => b"WRITE16",
        0x9e => b"READ-CAPACITY16",
        _ => b"OP",
    }
}
unsafe fn append_reason(current: *mut Status, cursor: &mut usize, bytes: &[u8]) {
    unsafe {
        let available = 128 - (*cursor).min(128);
        let count = available.min(bytes.len());
        if count != 0 {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                ptr::addr_of_mut!((*current).reason_storage)
                    .cast::<u8>()
                    .add(*cursor),
                count,
            );
        }
        *cursor += count;
    }
}
unsafe fn append_number(current: *mut Status, cursor: &mut usize, mut value: u32) {
    let mut digits = [0; 10];
    let mut count = 0;
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    while count != 0 {
        count -= 1;
        unsafe {
            append_reason(current, cursor, &digits[count..count + 1]);
        }
    }
}
unsafe fn publish_reason(current: *mut Status, cursor: usize) {
    unsafe {
        (*current).reason = Text {
            pointer: ptr::addr_of!((*current).reason_storage).cast(),
            len: cursor.min(128),
        };
    }
}
unsafe fn begin_reason(current: *mut Status) -> usize {
    unsafe {
        ptr::write_bytes(
            ptr::addr_of_mut!((*current).reason_storage).cast::<u8>(),
            0,
            128,
        );
        let mut cursor = 0;
        append_reason(current, &mut cursor, b"SCSI ");
        append_reason(current, &mut cursor, opcode_name((*current).last_opcode));
        append_reason(current, &mut cursor, b" ");
        cursor
    }
}
unsafe fn set_protocol_reason(current: *mut Status, detail: &[u8]) {
    unsafe {
        let mut cursor = begin_reason(current);
        append_reason(current, &mut cursor, detail);
        publish_reason(current, cursor);
    }
}
unsafe fn set_transport_reason(current: *mut Status, stage: &[u8]) {
    unsafe {
        let mut xs = mem::MaybeUninit::uninit();
        if !xhci_api::r4os_rust_usb_xhci_status_for_device(
            ptr::addr_of!((*current).device),
            xs.as_mut_ptr(),
        ) {
            return;
        }
        let xs = xs.assume_init();
        let mut cursor = begin_reason(current);
        append_reason(current, &mut cursor, stage);
        append_reason(current, &mut cursor, b" xHCI=");
        append_reason(current, &mut cursor, text(xs.last_bulk_result));
        append_reason(current, &mut cursor, b" cc=");
        append_number(
            current,
            &mut cursor,
            u32::from(xs.last_bulk_completion_code),
        );
        append_reason(current, &mut cursor, b" bytes=");
        append_number(current, &mut cursor, xs.last_bulk_actual_len);
        append_reason(current, &mut cursor, b"/");
        append_number(current, &mut cursor, xs.last_bulk_request_len);
        publish_reason(current, cursor);
    }
}
pub(super) unsafe fn set_data_length_reason(current: *mut Status, detail: &[u8], actual: u32) {
    unsafe {
        let mut cursor = begin_reason(current);
        append_reason(current, &mut cursor, detail);
        append_reason(current, &mut cursor, b" bytes=");
        append_number(current, &mut cursor, actual);
        append_reason(current, &mut cursor, b"/");
        append_number(current, &mut cursor, (*current).last_transfer_len);
        append_reason(current, &mut cursor, b" residue=");
        append_number(current, &mut cursor, (*current).last_csw_residue);
        publish_reason(current, cursor);
    }
}
unsafe fn recover_transport(current: *mut Status) -> bool {
    let Some(_g) = Guard::enter() else {
        return false;
    };
    unsafe {
        (*current).recoveries += 1;
        (*current).transport_faulted = true;
        if (*current).block_index != usize::MAX {
            block_execution::r4os_rust_block_device_operation(
                (*current).block_index,
                1,
                0,
                0,
                0,
                0,
            );
        }
        let budget = Deadline::begin(timing::MSC_RECOVERY_BUDGET_MS);
        let ok = (|| {
            let handle = ptr::addr_of_mut!((*current).device);
            if (*current).bulk_in != 0
                && !xhci::reset_endpoint_state_for_handle(handle, (*current).bulk_in, Some(&budget))
            {
                return false;
            }
            if (*current).bulk_out != 0
                && !xhci::reset_endpoint_state_for_handle(
                    handle,
                    (*current).bulk_out,
                    Some(&budget),
                )
            {
                return false;
            }
            if !xhci::reset_mass_storage_interface_for_handle(
                handle,
                (*current).interface_number,
                Some(&budget),
            ) {
                return false;
            }
            (*current).resets += 1;
            if (*current).bulk_in != 0 {
                if !xhci::clear_endpoint_halt_for_handle(handle, (*current).bulk_in, Some(&budget))
                {
                    return false;
                }
                (*current).clear_halts += 1;
            }
            if (*current).bulk_out != 0 {
                if !xhci::clear_endpoint_halt_for_handle(handle, (*current).bulk_out, Some(&budget))
                {
                    return false;
                }
                (*current).clear_halts += 1;
            }
            if (*current).bulk_in == 0
                || (*current).bulk_out == 0
                || !xhci::reconfigure_mass_storage_bulk_endpoints_for_handle_within(
                    handle,
                    (*current).bulk_in,
                    (*current).bulk_out,
                    &budget,
                )
            {
                return false;
            }
            (*current).transport_faulted = false;
            true
        })();
        budget.finish();
        if !ok {
            (*current).recovery_failures += 1;
        }
        if (*current).block_index != usize::MAX {
            block_execution::r4os_rust_block_device_operation(
                (*current).block_index,
                if ok { 2 } else { 3 },
                0,
                0,
                0,
                0,
            );
        }
        ok
    }
}
