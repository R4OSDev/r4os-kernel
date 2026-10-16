//! Private native BOT medium status and command records.
use super::xhci_types::{DeviceHandle, EndpointHandle, IncidentToken};
use crate::sched::sync_types::Text;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub initialized: bool,
    pub present: bool,
    pub bound: bool,
    pub block_registered: bool,
    pub block_index: usize,
    pub port: u8,
    pub device: DeviceHandle,
    pub bulk_in_handle: EndpointHandle,
    pub bulk_out_handle: EndpointHandle,
    pub interface_number: u8,
    pub bulk_in: u8,
    pub bulk_out: u8,
    pub bulk_in_max_packet: u16,
    pub bulk_out_max_packet: u16,
    pub sector_count: u64,
    pub sector_size: u32,
    pub max_sectors_per_request: u16,
    pub capacity_format: u8,
    pub capacity16_used: bool,
    pub read10_commands: u64,
    pub read16_commands: u64,
    pub write10_commands: u64,
    pub write16_commands: u64,
    pub transport: Text,
    pub lun: u8,
    pub max_lun: u8,
    pub uas_supported: bool,
    pub inquiry_ok: bool,
    pub model: [u8; 32],
    pub model_len: usize,
    pub inquiry_retries: u64,
    pub inquiry_wait_elapsed_ns: u64,
    pub test_unit_ready_ok: bool,
    pub test_unit_ready_retries: u64,
    pub ready_wait_elapsed_ns: u64,
    pub read_capacity_ok: bool,
    pub read_capacity_retries: u64,
    pub capacity_wait_elapsed_ns: u64,
    pub retry_delay_calls: u64,
    pub retry_budget_timeouts: u64,
    pub transient_sense_retries: u64,
    pub unknown_sense_retries: u64,
    pub permanent_sense_stops: u64,
    pub last_retry_opcode: u8,
    pub last_retry_number: u8,
    pub mode_sense_ok: bool,
    pub write_protected_known: bool,
    pub write_protected: bool,
    pub tags: u32,
    pub commands: u64,
    pub failures: u64,
    pub reads: u64,
    pub read_failures: u64,
    pub read_transport_retries: u64,
    pub read_transport_retry_successes: u64,
    pub writes: u64,
    pub write_failures: u64,
    pub write_transport_retries: u64,
    pub write_transport_retry_successes: u64,
    pub flushes: u64,
    pub flush_failures: u64,
    pub flush_transport_retries: u64,
    pub flush_transport_retry_successes: u64,
    pub flush_unsupported: u64,
    pub recoveries: u64,
    pub recovery_failures: u64,
    pub resets: u64,
    pub clear_halts: u64,
    pub sense_requests: u64,
    pub sense_valid: bool,
    pub sense_for_opcode: u8,
    pub sense_key: u8,
    pub sense_asc: u8,
    pub sense_ascq: u8,
    pub sense_not_ready: u64,
    pub sense_unit_attention: u64,
    pub sense_illegal_request: u64,
    pub sense_write_protect: u64,
    pub command_failures: u64,
    pub transport_failures: u64,
    pub transport_faulted: bool,
    pub media_changed: bool,
    pub last_failure_transport: bool,
    pub last_recovery_ok: bool,
    pub invalid_csw: u64,
    pub csw_tag_mismatches: u64,
    pub csw_phase_errors: u64,
    pub csw_residue_errors: u64,
    pub short_data_errors: u64,
    pub last_opcode: u8,
    pub last_transfer_len: u32,
    pub last_data_actual_len: u32,
    pub last_csw_tag: u32,
    pub last_csw_residue: u32,
    pub last_csw_status: u8,
    pub bot_source: Text,
    pub bot_r4p_build: u64,
    pub bot_r4p_csw: u64,
    pub bot_dispatch_failures: u64,
    pub bot_last_result: i32,
    pub scsi_source: Text,
    pub scsi_r4p_build: u64,
    pub scsi_r4p_parse: u64,
    pub scsi_dispatch_failures: u64,
    pub scsi_last_result: i32,
    pub protocol_required_missing: u64,
    pub reason: Text,
    pub reason_storage: [u8; 128],
    pub transport_incident: IncidentToken,
}
impl Status {
    pub const EMPTY: Self = Self {
        initialized: false,
        present: false,
        bound: false,
        block_registered: false,
        block_index: usize::MAX,
        port: 0,
        device: DeviceHandle::EMPTY,
        bulk_in_handle: EndpointHandle::EMPTY,
        bulk_out_handle: EndpointHandle::EMPTY,
        interface_number: 0,
        bulk_in: 0,
        bulk_out: 0,
        bulk_in_max_packet: 0,
        bulk_out_max_packet: 0,
        sector_count: 0,
        sector_size: 0,
        max_sectors_per_request: 0,
        capacity_format: 0,
        capacity16_used: false,
        read10_commands: 0,
        read16_commands: 0,
        write10_commands: 0,
        write16_commands: 0,
        transport: Text::literal(b"BOT"),
        lun: 0,
        max_lun: 0,
        uas_supported: false,
        inquiry_ok: false,
        model: [0; 32],
        model_len: 0,
        inquiry_retries: 0,
        inquiry_wait_elapsed_ns: 0,
        test_unit_ready_ok: false,
        test_unit_ready_retries: 0,
        ready_wait_elapsed_ns: 0,
        read_capacity_ok: false,
        read_capacity_retries: 0,
        capacity_wait_elapsed_ns: 0,
        retry_delay_calls: 0,
        retry_budget_timeouts: 0,
        transient_sense_retries: 0,
        unknown_sense_retries: 0,
        permanent_sense_stops: 0,
        last_retry_opcode: 0,
        last_retry_number: 0,
        mode_sense_ok: false,
        write_protected_known: false,
        write_protected: false,
        tags: 0,
        commands: 0,
        failures: 0,
        reads: 0,
        read_failures: 0,
        read_transport_retries: 0,
        read_transport_retry_successes: 0,
        writes: 0,
        write_failures: 0,
        write_transport_retries: 0,
        write_transport_retry_successes: 0,
        flushes: 0,
        flush_failures: 0,
        flush_transport_retries: 0,
        flush_transport_retry_successes: 0,
        flush_unsupported: 0,
        recoveries: 0,
        recovery_failures: 0,
        resets: 0,
        clear_halts: 0,
        sense_requests: 0,
        sense_valid: false,
        sense_for_opcode: 0,
        sense_key: 0,
        sense_asc: 0,
        sense_ascq: 0,
        sense_not_ready: 0,
        sense_unit_attention: 0,
        sense_illegal_request: 0,
        sense_write_protect: 0,
        command_failures: 0,
        transport_failures: 0,
        transport_faulted: false,
        media_changed: false,
        last_failure_transport: false,
        last_recovery_ok: false,
        invalid_csw: 0,
        csw_tag_mismatches: 0,
        csw_phase_errors: 0,
        csw_residue_errors: 0,
        short_data_errors: 0,
        last_opcode: 0,
        last_transfer_len: 0,
        last_data_actual_len: 0,
        last_csw_tag: 0,
        last_csw_residue: 0,
        last_csw_status: 0xFF,
        bot_source: Text::literal(b"none"),
        bot_r4p_build: 0,
        bot_r4p_csw: 0,
        bot_dispatch_failures: 0,
        bot_last_result: 0,
        scsi_source: Text::literal(b"none"),
        scsi_r4p_build: 0,
        scsi_r4p_parse: 0,
        scsi_dispatch_failures: 0,
        scsi_last_result: 0,
        protocol_required_missing: 0,
        reason: Text::literal(b"not initialized"),
        reason_storage: [0; 128],
        transport_incident: IncidentToken::EMPTY,
    };
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 920);
    assert!(core::mem::align_of::<Status>() == 8);
    assert!(core::mem::offset_of!(Status, initialized) == 0);
    assert!(core::mem::offset_of!(Status, present) == 1);
    assert!(core::mem::offset_of!(Status, bound) == 2);
    assert!(core::mem::offset_of!(Status, block_registered) == 3);
    assert!(core::mem::offset_of!(Status, block_index) == 8);
    assert!(core::mem::offset_of!(Status, port) == 16);
    assert!(core::mem::offset_of!(Status, device) == 24);
    assert!(core::mem::offset_of!(Status, bulk_in_handle) == 56);
    assert!(core::mem::offset_of!(Status, bulk_out_handle) == 96);
    assert!(core::mem::offset_of!(Status, interface_number) == 136);
    assert!(core::mem::offset_of!(Status, bulk_in) == 137);
    assert!(core::mem::offset_of!(Status, bulk_out) == 138);
    assert!(core::mem::offset_of!(Status, bulk_in_max_packet) == 140);
    assert!(core::mem::offset_of!(Status, bulk_out_max_packet) == 142);
    assert!(core::mem::offset_of!(Status, sector_count) == 144);
    assert!(core::mem::offset_of!(Status, sector_size) == 152);
    assert!(core::mem::offset_of!(Status, max_sectors_per_request) == 156);
    assert!(core::mem::offset_of!(Status, capacity_format) == 158);
    assert!(core::mem::offset_of!(Status, capacity16_used) == 159);
    assert!(core::mem::offset_of!(Status, read10_commands) == 160);
    assert!(core::mem::offset_of!(Status, read16_commands) == 168);
    assert!(core::mem::offset_of!(Status, write10_commands) == 176);
    assert!(core::mem::offset_of!(Status, write16_commands) == 184);
    assert!(core::mem::offset_of!(Status, transport) == 192);
    assert!(core::mem::offset_of!(Status, lun) == 208);
    assert!(core::mem::offset_of!(Status, max_lun) == 209);
    assert!(core::mem::offset_of!(Status, uas_supported) == 210);
    assert!(core::mem::offset_of!(Status, inquiry_ok) == 211);
    assert!(core::mem::offset_of!(Status, model) == 212);
    assert!(core::mem::offset_of!(Status, model_len) == 248);
    assert!(core::mem::offset_of!(Status, inquiry_retries) == 256);
    assert!(core::mem::offset_of!(Status, inquiry_wait_elapsed_ns) == 264);
    assert!(core::mem::offset_of!(Status, test_unit_ready_ok) == 272);
    assert!(core::mem::offset_of!(Status, test_unit_ready_retries) == 280);
    assert!(core::mem::offset_of!(Status, ready_wait_elapsed_ns) == 288);
    assert!(core::mem::offset_of!(Status, read_capacity_ok) == 296);
    assert!(core::mem::offset_of!(Status, read_capacity_retries) == 304);
    assert!(core::mem::offset_of!(Status, capacity_wait_elapsed_ns) == 312);
    assert!(core::mem::offset_of!(Status, retry_delay_calls) == 320);
    assert!(core::mem::offset_of!(Status, retry_budget_timeouts) == 328);
    assert!(core::mem::offset_of!(Status, transient_sense_retries) == 336);
    assert!(core::mem::offset_of!(Status, unknown_sense_retries) == 344);
    assert!(core::mem::offset_of!(Status, permanent_sense_stops) == 352);
    assert!(core::mem::offset_of!(Status, last_retry_opcode) == 360);
    assert!(core::mem::offset_of!(Status, last_retry_number) == 361);
    assert!(core::mem::offset_of!(Status, mode_sense_ok) == 362);
    assert!(core::mem::offset_of!(Status, write_protected_known) == 363);
    assert!(core::mem::offset_of!(Status, write_protected) == 364);
    assert!(core::mem::offset_of!(Status, tags) == 368);
    assert!(core::mem::offset_of!(Status, commands) == 376);
    assert!(core::mem::offset_of!(Status, failures) == 384);
    assert!(core::mem::offset_of!(Status, reads) == 392);
    assert!(core::mem::offset_of!(Status, read_failures) == 400);
    assert!(core::mem::offset_of!(Status, read_transport_retries) == 408);
    assert!(core::mem::offset_of!(Status, read_transport_retry_successes) == 416);
    assert!(core::mem::offset_of!(Status, writes) == 424);
    assert!(core::mem::offset_of!(Status, write_failures) == 432);
    assert!(core::mem::offset_of!(Status, write_transport_retries) == 440);
    assert!(core::mem::offset_of!(Status, write_transport_retry_successes) == 448);
    assert!(core::mem::offset_of!(Status, flushes) == 456);
    assert!(core::mem::offset_of!(Status, flush_failures) == 464);
    assert!(core::mem::offset_of!(Status, flush_transport_retries) == 472);
    assert!(core::mem::offset_of!(Status, flush_transport_retry_successes) == 480);
    assert!(core::mem::offset_of!(Status, flush_unsupported) == 488);
    assert!(core::mem::offset_of!(Status, recoveries) == 496);
    assert!(core::mem::offset_of!(Status, recovery_failures) == 504);
    assert!(core::mem::offset_of!(Status, resets) == 512);
    assert!(core::mem::offset_of!(Status, clear_halts) == 520);
    assert!(core::mem::offset_of!(Status, sense_requests) == 528);
    assert!(core::mem::offset_of!(Status, sense_valid) == 536);
    assert!(core::mem::offset_of!(Status, sense_for_opcode) == 537);
    assert!(core::mem::offset_of!(Status, sense_key) == 538);
    assert!(core::mem::offset_of!(Status, sense_asc) == 539);
    assert!(core::mem::offset_of!(Status, sense_ascq) == 540);
    assert!(core::mem::offset_of!(Status, sense_not_ready) == 544);
    assert!(core::mem::offset_of!(Status, sense_unit_attention) == 552);
    assert!(core::mem::offset_of!(Status, sense_illegal_request) == 560);
    assert!(core::mem::offset_of!(Status, sense_write_protect) == 568);
    assert!(core::mem::offset_of!(Status, command_failures) == 576);
    assert!(core::mem::offset_of!(Status, transport_failures) == 584);
    assert!(core::mem::offset_of!(Status, transport_faulted) == 592);
    assert!(core::mem::offset_of!(Status, media_changed) == 593);
    assert!(core::mem::offset_of!(Status, last_failure_transport) == 594);
    assert!(core::mem::offset_of!(Status, last_recovery_ok) == 595);
    assert!(core::mem::offset_of!(Status, invalid_csw) == 600);
    assert!(core::mem::offset_of!(Status, csw_tag_mismatches) == 608);
    assert!(core::mem::offset_of!(Status, csw_phase_errors) == 616);
    assert!(core::mem::offset_of!(Status, csw_residue_errors) == 624);
    assert!(core::mem::offset_of!(Status, short_data_errors) == 632);
    assert!(core::mem::offset_of!(Status, last_opcode) == 640);
    assert!(core::mem::offset_of!(Status, last_transfer_len) == 644);
    assert!(core::mem::offset_of!(Status, last_data_actual_len) == 648);
    assert!(core::mem::offset_of!(Status, last_csw_tag) == 652);
    assert!(core::mem::offset_of!(Status, last_csw_residue) == 656);
    assert!(core::mem::offset_of!(Status, last_csw_status) == 660);
    assert!(core::mem::offset_of!(Status, bot_source) == 664);
    assert!(core::mem::offset_of!(Status, bot_r4p_build) == 680);
    assert!(core::mem::offset_of!(Status, bot_r4p_csw) == 688);
    assert!(core::mem::offset_of!(Status, bot_dispatch_failures) == 696);
    assert!(core::mem::offset_of!(Status, bot_last_result) == 704);
    assert!(core::mem::offset_of!(Status, scsi_source) == 712);
    assert!(core::mem::offset_of!(Status, scsi_r4p_build) == 728);
    assert!(core::mem::offset_of!(Status, scsi_r4p_parse) == 736);
    assert!(core::mem::offset_of!(Status, scsi_dispatch_failures) == 744);
    assert!(core::mem::offset_of!(Status, scsi_last_result) == 752);
    assert!(core::mem::offset_of!(Status, protocol_required_missing) == 760);
    assert!(core::mem::offset_of!(Status, reason) == 768);
    assert!(core::mem::offset_of!(Status, reason_storage) == 784);
    assert!(core::mem::offset_of!(Status, transport_incident) == 912);
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    None,
    In,
    Out,
}
#[derive(Clone, Copy)]
pub(super) struct ScsiCommand {
    pub cdb: [u8; 16],
    pub len: usize,
    pub direction: Direction,
    pub transfer_len: u32,
}
