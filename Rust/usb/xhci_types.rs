//! Native xHCI state and private projections; external HostDescriptor stays
//! in types.rs. No second controller/device/endpoint state is introduced.
use super::{ring_cycle::Advance, types::Interface};
use crate::platform::pci_scan::Device as PciDevice;
use crate::sched::sync_types::Text;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IncidentToken {
    pub generation: u64,
}
impl IncidentToken {
    pub const EMPTY: Self = Self { generation: 0 };
    pub fn valid(self) -> bool {
        self.generation != 0
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Trb {
    pub parameter: u64,
    pub status: u32,
    pub control: u32,
}
impl Trb {
    pub const EMPTY: Self = Self {
        parameter: 0,
        status: 0,
        control: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<Trb>() == 16);
    assert!(core::mem::align_of::<Trb>() == 8);
    assert!(core::mem::offset_of!(Trb, parameter) == 0);
    assert!(core::mem::offset_of!(Trb, status) == 8);
    assert!(core::mem::offset_of!(Trb, control) == 12);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ErstEntry {
    pub base: u64,
    pub size: u32,
    pub reserved: u32,
}
impl ErstEntry {
    pub const EMPTY: Self = Self {
        base: 0,
        size: 0,
        reserved: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<ErstEntry>() == 16);
    assert!(core::mem::align_of::<ErstEntry>() == 8);
    assert!(core::mem::offset_of!(ErstEntry, base) == 0);
    assert!(core::mem::offset_of!(ErstEntry, size) == 8);
    assert!(core::mem::offset_of!(ErstEntry, reserved) == 12);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PortStatus {
    pub index: u8,
    pub portsc: u32,
    pub portpmsc: u32,
    pub portli: u32,
    pub porthlpmc: u32,
    pub change_bits: u32,
    pub link_state: u8,
    pub connected: bool,
    pub enabled: bool,
    pub powered: bool,
    pub debounce_ok: bool,
    pub reset_attempted: bool,
    pub reset_ok: bool,
    pub reset_reason: Text,
}
impl PortStatus {
    pub const EMPTY: Self = Self {
        index: 0,
        portsc: 0,
        portpmsc: 0,
        portli: 0,
        porthlpmc: 0,
        change_bits: 0,
        link_state: 0,
        connected: false,
        enabled: false,
        powered: false,
        debounce_ok: false,
        reset_attempted: false,
        reset_ok: false,
        reset_reason: Text::literal(b"not-started"),
    };
}
const _: () = {
    assert!(core::mem::size_of::<PortStatus>() == 48);
    assert!(core::mem::align_of::<PortStatus>() == 8);
    assert!(core::mem::offset_of!(PortStatus, index) == 0);
    assert!(core::mem::offset_of!(PortStatus, portsc) == 4);
    assert!(core::mem::offset_of!(PortStatus, portpmsc) == 8);
    assert!(core::mem::offset_of!(PortStatus, portli) == 12);
    assert!(core::mem::offset_of!(PortStatus, porthlpmc) == 16);
    assert!(core::mem::offset_of!(PortStatus, change_bits) == 20);
    assert!(core::mem::offset_of!(PortStatus, link_state) == 24);
    assert!(core::mem::offset_of!(PortStatus, connected) == 25);
    assert!(core::mem::offset_of!(PortStatus, enabled) == 26);
    assert!(core::mem::offset_of!(PortStatus, powered) == 27);
    assert!(core::mem::offset_of!(PortStatus, debounce_ok) == 28);
    assert!(core::mem::offset_of!(PortStatus, reset_attempted) == 29);
    assert!(core::mem::offset_of!(PortStatus, reset_ok) == 30);
    assert!(core::mem::offset_of!(PortStatus, reset_reason) == 32);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProbeRecord {
    pub active: bool,
    pub port: u8,
    pub slot: u8,
    pub speed: u8,
    pub pls: u8,
    pub stage: Text,
    pub command_cc: u8,
    pub transfer_cc: u8,
    pub class_code: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub vid: u16,
    pub pid: u16,
}
impl ProbeRecord {
    pub const EMPTY: Self = Self {
        active: false,
        port: 0,
        slot: 0,
        speed: 0,
        pls: 0,
        stage: Text::literal(b"none"),
        command_cc: 0,
        transfer_cc: 0,
        class_code: 0,
        subclass: 0,
        protocol: 0,
        vid: 0,
        pid: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<ProbeRecord>() == 40);
    assert!(core::mem::align_of::<ProbeRecord>() == 8);
    assert!(core::mem::offset_of!(ProbeRecord, active) == 0);
    assert!(core::mem::offset_of!(ProbeRecord, port) == 1);
    assert!(core::mem::offset_of!(ProbeRecord, slot) == 2);
    assert!(core::mem::offset_of!(ProbeRecord, speed) == 3);
    assert!(core::mem::offset_of!(ProbeRecord, pls) == 4);
    assert!(core::mem::offset_of!(ProbeRecord, stage) == 8);
    assert!(core::mem::offset_of!(ProbeRecord, command_cc) == 24);
    assert!(core::mem::offset_of!(ProbeRecord, transfer_cc) == 25);
    assert!(core::mem::offset_of!(ProbeRecord, class_code) == 26);
    assert!(core::mem::offset_of!(ProbeRecord, subclass) == 27);
    assert!(core::mem::offset_of!(ProbeRecord, protocol) == 28);
    assert!(core::mem::offset_of!(ProbeRecord, vid) == 30);
    assert!(core::mem::offset_of!(ProbeRecord, pid) == 32);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DescriptorInterface {
    pub valid: bool,
    pub number: u8,
    pub class_code: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub endpoint_count: u8,
    pub first_endpoint_address: u8,
    pub first_endpoint_attributes: u8,
    pub first_endpoint_max_packet: u16,
    pub first_endpoint_interval: u8,
    pub interrupt_in_endpoint_address: u8,
    pub interrupt_in_endpoint_max_packet: u16,
    pub interrupt_in_endpoint_interval: u8,
    pub bulk_in_endpoint_address: u8,
    pub bulk_in_endpoint_max_packet: u16,
    pub bulk_in_endpoint_max_burst: u8,
    pub bulk_out_endpoint_address: u8,
    pub bulk_out_endpoint_max_packet: u16,
    pub bulk_out_endpoint_max_burst: u8,
    pub last_endpoint_address: u8,
    pub hid_descriptor_len: u8,
    pub hid_report_descriptor_len: u16,
}
impl DescriptorInterface {
    pub const EMPTY: Self = Self {
        valid: false,
        number: 0,
        class_code: 0,
        subclass: 0,
        protocol: 0,
        endpoint_count: 0,
        first_endpoint_address: 0,
        first_endpoint_attributes: 0,
        first_endpoint_max_packet: 0,
        first_endpoint_interval: 0,
        interrupt_in_endpoint_address: 0,
        interrupt_in_endpoint_max_packet: 0,
        interrupt_in_endpoint_interval: 0,
        bulk_in_endpoint_address: 0,
        bulk_in_endpoint_max_packet: 0,
        bulk_in_endpoint_max_burst: 0,
        bulk_out_endpoint_address: 0,
        bulk_out_endpoint_max_packet: 0,
        bulk_out_endpoint_max_burst: 0,
        last_endpoint_address: 0,
        hid_descriptor_len: 0,
        hid_report_descriptor_len: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<DescriptorInterface>() == 28);
    assert!(core::mem::align_of::<DescriptorInterface>() == 2);
    assert!(core::mem::offset_of!(DescriptorInterface, valid) == 0);
    assert!(core::mem::offset_of!(DescriptorInterface, number) == 1);
    assert!(core::mem::offset_of!(DescriptorInterface, class_code) == 2);
    assert!(core::mem::offset_of!(DescriptorInterface, subclass) == 3);
    assert!(core::mem::offset_of!(DescriptorInterface, protocol) == 4);
    assert!(core::mem::offset_of!(DescriptorInterface, endpoint_count) == 5);
    assert!(core::mem::offset_of!(DescriptorInterface, first_endpoint_address) == 6);
    assert!(core::mem::offset_of!(DescriptorInterface, first_endpoint_attributes) == 7);
    assert!(core::mem::offset_of!(DescriptorInterface, first_endpoint_max_packet) == 8);
    assert!(core::mem::offset_of!(DescriptorInterface, first_endpoint_interval) == 10);
    assert!(core::mem::offset_of!(DescriptorInterface, interrupt_in_endpoint_address) == 11);
    assert!(core::mem::offset_of!(DescriptorInterface, interrupt_in_endpoint_max_packet) == 12);
    assert!(core::mem::offset_of!(DescriptorInterface, interrupt_in_endpoint_interval) == 14);
    assert!(core::mem::offset_of!(DescriptorInterface, bulk_in_endpoint_address) == 15);
    assert!(core::mem::offset_of!(DescriptorInterface, bulk_in_endpoint_max_packet) == 16);
    assert!(core::mem::offset_of!(DescriptorInterface, bulk_in_endpoint_max_burst) == 18);
    assert!(core::mem::offset_of!(DescriptorInterface, bulk_out_endpoint_address) == 19);
    assert!(core::mem::offset_of!(DescriptorInterface, bulk_out_endpoint_max_packet) == 20);
    assert!(core::mem::offset_of!(DescriptorInterface, bulk_out_endpoint_max_burst) == 22);
    assert!(core::mem::offset_of!(DescriptorInterface, last_endpoint_address) == 23);
    assert!(core::mem::offset_of!(DescriptorInterface, hid_descriptor_len) == 24);
    assert!(core::mem::offset_of!(DescriptorInterface, hid_report_descriptor_len) == 26);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ControlRequest {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index_value: u16,
    pub length: u16,
    pub data_phys: u64,
    pub direction: u8,
}
impl ControlRequest {
    pub const EMPTY: Self = Self {
        request_type: 0,
        request: 0,
        value: 0,
        index_value: 0,
        length: 0,
        data_phys: 0,
        direction: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<ControlRequest>() == 24);
    assert!(core::mem::align_of::<ControlRequest>() == 8);
    assert!(core::mem::offset_of!(ControlRequest, request_type) == 0);
    assert!(core::mem::offset_of!(ControlRequest, request) == 1);
    assert!(core::mem::offset_of!(ControlRequest, value) == 2);
    assert!(core::mem::offset_of!(ControlRequest, index_value) == 4);
    assert!(core::mem::offset_of!(ControlRequest, length) == 6);
    assert!(core::mem::offset_of!(ControlRequest, data_phys) == 8);
    assert!(core::mem::offset_of!(ControlRequest, direction) == 16);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceRuntime {
    pub active: bool,
    pub generation: u64,
    pub transaction_active: bool,
    pub transaction_task_id: u32,
    pub transaction_task_generation: u64,
    pub detach_pending: bool,
    pub slot_id: u8,
    pub port: u8,
    pub speed: u8,
    pub config_value: u8,
    pub set_configuration_attempted: bool,
    pub set_configuration_ok: bool,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_context_phys: u64,
    pub input_context_phys: u64,
    pub ep0_ring_phys: u64,
    pub interrupt_ring_phys: u64,
    pub interrupt_buffer_phys: u64,
    pub bulk_in_ring_phys: u64,
    pub bulk_out_ring_phys: u64,
    pub bulk_buffer_phys: u64,
    pub bulk_buffer_frames: u16,
    pub descriptor_phys: u64,
    pub device_virt: u64,
    pub input_virt: u64,
    pub ep0_ring_virt: u64,
    pub interrupt_ring_virt: u64,
    pub interrupt_buffer_virt: u64,
    pub bulk_in_ring_virt: u64,
    pub bulk_out_ring_virt: u64,
    pub bulk_buffer_virt: u64,
    pub descriptor_virt: u64,
    pub ep0_enqueue: u16,
    pub ep0_cycle: u8,
    pub control_endpoint_faulted: bool,
    pub interrupt_enqueue: u16,
    pub interrupt_cycle: u8,
    pub interrupt_endpoint_id: u8,
    pub interrupt_endpoint_address: u8,
    pub interrupt_endpoint_max_packet: u16,
    pub interrupt_endpoint_interval_raw: u8,
    pub interrupt_endpoint_interval_context: u8,
    pub interrupt_endpoint_configured: bool,
    pub interrupt_endpoint_faulted: bool,
    pub interrupt_pending: bool,
    pub interrupt_pending_trb_phys: u64,
    pub interrupt_transfer_handle: u32,
    pub interrupt_pending_streak: u64,
    pub last_interrupt_ep_state: u8,
    pub last_interrupt_request_len: u32,
    pub last_interrupt_residue: u32,
    pub last_interrupt_actual_len: u32,
    pub last_interrupt_completion_code: u8,
    pub bulk_endpoints_configured: bool,
    pub bulk_endpoints_faulted: bool,
    pub bulk_in_endpoint_id: u8,
    pub bulk_in_endpoint_address: u8,
    pub bulk_in_endpoint_max_packet: u16,
    pub bulk_in_endpoint_max_burst: u8,
    pub bulk_out_endpoint_id: u8,
    pub bulk_out_endpoint_address: u8,
    pub bulk_out_endpoint_max_packet: u16,
    pub bulk_out_endpoint_max_burst: u8,
    pub bulk_in_enqueue: u16,
    pub bulk_in_cycle: u8,
    pub bulk_in_link_update_pending: bool,
    pub bulk_out_enqueue: u16,
    pub bulk_out_cycle: u8,
    pub bulk_out_link_update_pending: bool,
    pub last_control_request_type: u8,
    pub last_control_request: u8,
    pub last_control_value: u16,
    pub last_control_index: u16,
    pub last_control_length: u16,
    pub last_control_direction: Text,
    pub last_control_completion_code: u8,
    pub last_control_residue: u32,
    pub last_control_ok: bool,
    pub last_bulk_direction: Text,
    pub last_bulk_result: Text,
    pub last_bulk_request_len: u32,
    pub last_bulk_residue: u32,
    pub last_bulk_actual_len: u32,
    pub last_bulk_completion_code: u8,
    pub last_wait_cancelled: bool,
    pub sync_transfer_incident: IncidentToken,
}
impl DeviceRuntime {
    pub const EMPTY: Self = Self {
        active: false,
        generation: 0,
        transaction_active: false,
        transaction_task_id: 0,
        transaction_task_generation: 0,
        detach_pending: false,
        slot_id: 0,
        port: 0,
        speed: 0,
        config_value: 0,
        set_configuration_attempted: false,
        set_configuration_ok: false,
        vendor_id: 0,
        product_id: 0,
        device_context_phys: 0,
        input_context_phys: 0,
        ep0_ring_phys: 0,
        interrupt_ring_phys: 0,
        interrupt_buffer_phys: 0,
        bulk_in_ring_phys: 0,
        bulk_out_ring_phys: 0,
        bulk_buffer_phys: 0,
        bulk_buffer_frames: 0,
        descriptor_phys: 0,
        device_virt: 0,
        input_virt: 0,
        ep0_ring_virt: 0,
        interrupt_ring_virt: 0,
        interrupt_buffer_virt: 0,
        bulk_in_ring_virt: 0,
        bulk_out_ring_virt: 0,
        bulk_buffer_virt: 0,
        descriptor_virt: 0,
        ep0_enqueue: 0,
        ep0_cycle: 1,
        control_endpoint_faulted: false,
        interrupt_enqueue: 0,
        interrupt_cycle: 1,
        interrupt_endpoint_id: 0,
        interrupt_endpoint_address: 0,
        interrupt_endpoint_max_packet: 0,
        interrupt_endpoint_interval_raw: 0,
        interrupt_endpoint_interval_context: 0,
        interrupt_endpoint_configured: false,
        interrupt_endpoint_faulted: false,
        interrupt_pending: false,
        interrupt_pending_trb_phys: 0,
        interrupt_transfer_handle: 0,
        interrupt_pending_streak: 0,
        last_interrupt_ep_state: 0,
        last_interrupt_request_len: 0,
        last_interrupt_residue: 0,
        last_interrupt_actual_len: 0,
        last_interrupt_completion_code: 0,
        bulk_endpoints_configured: false,
        bulk_endpoints_faulted: false,
        bulk_in_endpoint_id: 0,
        bulk_in_endpoint_address: 0,
        bulk_in_endpoint_max_packet: 0,
        bulk_in_endpoint_max_burst: 0,
        bulk_out_endpoint_id: 0,
        bulk_out_endpoint_address: 0,
        bulk_out_endpoint_max_packet: 0,
        bulk_out_endpoint_max_burst: 0,
        bulk_in_enqueue: 0,
        bulk_in_cycle: 1,
        bulk_in_link_update_pending: false,
        bulk_out_enqueue: 0,
        bulk_out_cycle: 1,
        bulk_out_link_update_pending: false,
        last_control_request_type: 0,
        last_control_request: 0,
        last_control_value: 0,
        last_control_index: 0,
        last_control_length: 0,
        last_control_direction: Text::literal(b"none"),
        last_control_completion_code: 0,
        last_control_residue: 0,
        last_control_ok: false,
        last_bulk_direction: Text::literal(b"none"),
        last_bulk_result: Text::literal(b"none"),
        last_bulk_request_len: 0,
        last_bulk_residue: 0,
        last_bulk_actual_len: 0,
        last_bulk_completion_code: 0,
        last_wait_cancelled: false,
        sync_transfer_incident: IncidentToken::EMPTY,
    };
}
const _: () = {
    assert!(core::mem::size_of::<DeviceRuntime>() == 384);
    assert!(core::mem::align_of::<DeviceRuntime>() == 8);
    assert!(core::mem::offset_of!(DeviceRuntime, active) == 0);
    assert!(core::mem::offset_of!(DeviceRuntime, generation) == 8);
    assert!(core::mem::offset_of!(DeviceRuntime, transaction_active) == 16);
    assert!(core::mem::offset_of!(DeviceRuntime, transaction_task_id) == 20);
    assert!(core::mem::offset_of!(DeviceRuntime, transaction_task_generation) == 24);
    assert!(core::mem::offset_of!(DeviceRuntime, detach_pending) == 32);
    assert!(core::mem::offset_of!(DeviceRuntime, slot_id) == 33);
    assert!(core::mem::offset_of!(DeviceRuntime, port) == 34);
    assert!(core::mem::offset_of!(DeviceRuntime, speed) == 35);
    assert!(core::mem::offset_of!(DeviceRuntime, config_value) == 36);
    assert!(core::mem::offset_of!(DeviceRuntime, set_configuration_attempted) == 37);
    assert!(core::mem::offset_of!(DeviceRuntime, set_configuration_ok) == 38);
    assert!(core::mem::offset_of!(DeviceRuntime, vendor_id) == 40);
    assert!(core::mem::offset_of!(DeviceRuntime, product_id) == 42);
    assert!(core::mem::offset_of!(DeviceRuntime, device_context_phys) == 48);
    assert!(core::mem::offset_of!(DeviceRuntime, input_context_phys) == 56);
    assert!(core::mem::offset_of!(DeviceRuntime, ep0_ring_phys) == 64);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_ring_phys) == 72);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_buffer_phys) == 80);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_ring_phys) == 88);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_ring_phys) == 96);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_buffer_phys) == 104);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_buffer_frames) == 112);
    assert!(core::mem::offset_of!(DeviceRuntime, descriptor_phys) == 120);
    assert!(core::mem::offset_of!(DeviceRuntime, device_virt) == 128);
    assert!(core::mem::offset_of!(DeviceRuntime, input_virt) == 136);
    assert!(core::mem::offset_of!(DeviceRuntime, ep0_ring_virt) == 144);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_ring_virt) == 152);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_buffer_virt) == 160);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_ring_virt) == 168);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_ring_virt) == 176);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_buffer_virt) == 184);
    assert!(core::mem::offset_of!(DeviceRuntime, descriptor_virt) == 192);
    assert!(core::mem::offset_of!(DeviceRuntime, ep0_enqueue) == 200);
    assert!(core::mem::offset_of!(DeviceRuntime, ep0_cycle) == 202);
    assert!(core::mem::offset_of!(DeviceRuntime, control_endpoint_faulted) == 203);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_enqueue) == 204);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_cycle) == 206);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_endpoint_id) == 207);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_endpoint_address) == 208);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_endpoint_max_packet) == 210);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_endpoint_interval_raw) == 212);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_endpoint_interval_context) == 213);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_endpoint_configured) == 214);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_endpoint_faulted) == 215);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_pending) == 216);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_pending_trb_phys) == 224);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_transfer_handle) == 232);
    assert!(core::mem::offset_of!(DeviceRuntime, interrupt_pending_streak) == 240);
    assert!(core::mem::offset_of!(DeviceRuntime, last_interrupt_ep_state) == 248);
    assert!(core::mem::offset_of!(DeviceRuntime, last_interrupt_request_len) == 252);
    assert!(core::mem::offset_of!(DeviceRuntime, last_interrupt_residue) == 256);
    assert!(core::mem::offset_of!(DeviceRuntime, last_interrupt_actual_len) == 260);
    assert!(core::mem::offset_of!(DeviceRuntime, last_interrupt_completion_code) == 264);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_endpoints_configured) == 265);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_endpoints_faulted) == 266);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_endpoint_id) == 267);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_endpoint_address) == 268);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_endpoint_max_packet) == 270);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_endpoint_max_burst) == 272);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_endpoint_id) == 273);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_endpoint_address) == 274);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_endpoint_max_packet) == 276);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_endpoint_max_burst) == 278);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_enqueue) == 280);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_cycle) == 282);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_in_link_update_pending) == 283);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_enqueue) == 284);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_cycle) == 286);
    assert!(core::mem::offset_of!(DeviceRuntime, bulk_out_link_update_pending) == 287);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_request_type) == 288);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_request) == 289);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_value) == 290);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_index) == 292);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_length) == 294);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_direction) == 296);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_completion_code) == 312);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_residue) == 316);
    assert!(core::mem::offset_of!(DeviceRuntime, last_control_ok) == 320);
    assert!(core::mem::offset_of!(DeviceRuntime, last_bulk_direction) == 328);
    assert!(core::mem::offset_of!(DeviceRuntime, last_bulk_result) == 344);
    assert!(core::mem::offset_of!(DeviceRuntime, last_bulk_request_len) == 360);
    assert!(core::mem::offset_of!(DeviceRuntime, last_bulk_residue) == 364);
    assert!(core::mem::offset_of!(DeviceRuntime, last_bulk_actual_len) == 368);
    assert!(core::mem::offset_of!(DeviceRuntime, last_bulk_completion_code) == 372);
    assert!(core::mem::offset_of!(DeviceRuntime, last_wait_cancelled) == 373);
    assert!(core::mem::offset_of!(DeviceRuntime, sync_transfer_incident) == 376);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceHandle {
    pub generation: u64,
    pub controller: Text,
    pub port: u8,
    pub slot_id: u8,
    pub speed: u8,
    pub config_value: u8,
    pub vendor_id: u16,
    pub product_id: u16,
}
impl DeviceHandle {
    pub const EMPTY: Self = Self {
        generation: 0,
        controller: Text::literal(b"xhci"),
        port: 0,
        slot_id: 0,
        speed: 0,
        config_value: 0,
        vendor_id: 0,
        product_id: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<DeviceHandle>() == 32);
    assert!(core::mem::align_of::<DeviceHandle>() == 8);
    assert!(core::mem::offset_of!(DeviceHandle, generation) == 0);
    assert!(core::mem::offset_of!(DeviceHandle, controller) == 8);
    assert!(core::mem::offset_of!(DeviceHandle, port) == 24);
    assert!(core::mem::offset_of!(DeviceHandle, slot_id) == 25);
    assert!(core::mem::offset_of!(DeviceHandle, speed) == 26);
    assert!(core::mem::offset_of!(DeviceHandle, config_value) == 27);
    assert!(core::mem::offset_of!(DeviceHandle, vendor_id) == 28);
    assert!(core::mem::offset_of!(DeviceHandle, product_id) == 30);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EndpointHandle {
    pub device: DeviceHandle,
    pub kind: u8,
    pub address: u8,
    pub endpoint_id: u8,
    pub max_packet: u16,
    pub max_burst: u8,
    pub interval: u8,
}
impl EndpointHandle {
    pub const EMPTY: Self = Self {
        device: DeviceHandle::EMPTY,
        kind: 0,
        address: 0,
        endpoint_id: 0,
        max_packet: 0,
        max_burst: 0,
        interval: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<EndpointHandle>() == 40);
    assert!(core::mem::align_of::<EndpointHandle>() == 8);
    assert!(core::mem::offset_of!(EndpointHandle, device) == 0);
    assert!(core::mem::offset_of!(EndpointHandle, kind) == 32);
    assert!(core::mem::offset_of!(EndpointHandle, address) == 33);
    assert!(core::mem::offset_of!(EndpointHandle, endpoint_id) == 34);
    assert!(core::mem::offset_of!(EndpointHandle, max_packet) == 36);
    assert!(core::mem::offset_of!(EndpointHandle, max_burst) == 38);
    assert!(core::mem::offset_of!(EndpointHandle, interval) == 39);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceTransaction {
    pub runtime_index: usize,
    pub generation: u64,
}
impl DeviceTransaction {
    pub const EMPTY: Self = Self {
        runtime_index: 0,
        generation: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<DeviceTransaction>() == 16);
    assert!(core::mem::align_of::<DeviceTransaction>() == 8);
    assert!(core::mem::offset_of!(DeviceTransaction, runtime_index) == 0);
    assert!(core::mem::offset_of!(DeviceTransaction, generation) == 8);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub probed: bool,
    pub present: bool,
    pub mapped: bool,
    pub device: PciDevice,
    pub command_before: u16,
    pub command_after: u16,
    pub bar0_raw: u32,
    pub bar1_raw: u32,
    pub bar_is_io: bool,
    pub bar_is_64: bool,
    pub mmio_phys: u64,
    pub mmio_virt: u64,
    pub caplength: u8,
    pub hciversion: u16,
    pub hcsparams1: u32,
    pub hcsparams2: u32,
    pub hcsparams3: u32,
    pub hccparams1: u32,
    pub hccparams2: u32,
    pub dboff: u32,
    pub rtsoff: u32,
    pub op_virt: u64,
    pub runtime_virt: u64,
    pub max_slots: u8,
    pub max_interrupters: u16,
    pub max_ports: u8,
    pub usbcmd: u32,
    pub usbsts: u32,
    pub pagesize: u32,
    pub dnctrl: u32,
    pub config: u32,
    pub mfindex: u32,
    pub iman0: u32,
    pub imod0: u32,
    pub erstsz0: u32,
    pub erstba0: u64,
    pub erdp0: u64,
    pub crcr: u64,
    pub dcbaap: u64,
    pub port_count_seen: u8,
    pub first_ports: [PortStatus; 16],
    pub legacy_cap_offset: u32,
    pub legacy_usblegsup_before: u32,
    pub legacy_usblegsup_after: u32,
    pub bios_handoff: Text,
    pub controller_halted_before: bool,
    pub controller_stopped: bool,
    pub controller_reset_ok: bool,
    pub controller_running: bool,
    pub dma_ready: bool,
    pub context_size: u8,
    pub dcbaa_phys: u64,
    pub command_ring_phys: u64,
    pub event_ring_phys: u64,
    pub erst_phys: u64,
    pub scratchpad_array_phys: u64,
    pub scratchpad_count: u8,
    pub max_slots_enabled: u8,
    pub command_enqueue: u16,
    pub command_cycle: u8,
    pub event_dequeue: u16,
    pub event_cycle: u8,
    pub event_raw_cycle: u8,
    pub event_raw_type: u8,
    pub event_raw_code: u8,
    pub event_erdp_commits: u64,
    pub event_max_batch: u16,
    pub command_ring_wraps: u64,
    pub event_ring_wraps: u64,
    pub ep0_ring_wraps: u64,
    pub interrupt_ring_wraps: u64,
    pub bulk_in_ring_wraps: u64,
    pub bulk_out_ring_wraps: u64,
    pub commands: u64,
    pub events: u64,
    pub event_tick_deadline_waits: u64,
    pub event_hpet_deadline_waits: u64,
    pub event_tsc_deadline_waits: u64,
    pub event_guard_waits: u64,
    pub event_tick_timeouts: u64,
    pub event_hpet_timeouts: u64,
    pub event_guard_timeouts: u64,
    pub event_tsc_timeouts: u64,
    pub event_recovery_budget_timeouts: u64,
    pub event_cpu_guard_timeouts: u64,
    pub host_controller_id: u32,
    pub owner_id: u32,
    pub host_source: u32,
    pub irq_line: u8,
    pub irq_pin: u8,
    pub irq_registered: bool,
    pub irq_mode: Text,
    pub irq_count: u64,
    pub irq_handled: u64,
    pub irq_wakeups: u64,
    pub poll_fallbacks: u64,
    pub wait_calls: u64,
    pub wait_successes: u64,
    pub wait_timeouts: u64,
    pub wait_early_calls: u64,
    pub wait_runtime_calls: u64,
    pub wait_requested_ms: u64,
    pub wait_elapsed_ns: u64,
    pub wait_max_elapsed_ns: u64,
    pub wait_iterations: u64,
    pub wait_blocked_ticks: u64,
    pub last_wait_reason: Text,
    pub last_wait_phase: u8,
    pub last_wait_requested_ms: u32,
    pub last_wait_elapsed_ns: u64,
    pub last_wait_iterations: u64,
    pub last_wait_blocked_ticks: u64,
    pub last_wait_retry: u8,
    pub last_wait_success: bool,
    pub stale_events: u64,
    pub ring_full: u64,
    pub deferred_events_pending: usize,
    pub deferred_events_queued: u64,
    pub deferred_events_delivered: u64,
    pub deferred_events_overflows: u64,
    pub deferred_events_purged: u64,
    pub deferred_events_high_water: usize,
    pub retained_slots: u8,
    pub runtime_switches: u64,
    pub port_change_clears: u64,
    pub port_change_events: u64,
    pub port_change_queued: u64,
    pub port_change_coalesced: u64,
    pub port_change_invalid: u64,
    pub port_change_taken: u64,
    pub port_change_retries: u64,
    pub port_change_pending_mask: u16,
    pub port_change_pending: u8,
    pub port_change_high_water: u8,
    pub port_service_calls: u64,
    pub port_service_failures: u64,
    pub port_catalog_changes: u64,
    pub port_task_started: bool,
    pub port_task_id: u32,
    pub port_task_iterations: u64,
    pub port_debounce_failures: u64,
    pub port_power_requests: u64,
    pub port_u0_waits: u64,
    pub port_warm_resets: u64,
    pub port_disconnects: u64,
    pub reclaimed_slots: u64,
    pub disable_slot_commands: u64,
    pub disable_slot_failures: u64,
    pub enable_slot_attempted: bool,
    pub enable_slot_ok: bool,
    pub address_device_attempted: bool,
    pub address_device_ok: bool,
    pub get_descriptor_attempted: bool,
    pub get_descriptor_ok: bool,
    pub get_config_attempted: bool,
    pub get_config_ok: bool,
    pub get_strings_attempted: bool,
    pub get_strings_ok: bool,
    pub last_command_type: u8,
    pub last_completion_code: u8,
    pub last_transfer_completion_code: u8,
    pub last_event_type: u8,
    pub last_event_code: u8,
    pub last_event_slot: u8,
    pub last_event_endpoint: u8,
    pub last_event_parameter: u64,
    pub last_event_length: u32,
    pub last_stale_event_type: u8,
    pub last_stale_event_code: u8,
    pub last_stale_event_slot: u8,
    pub last_stale_event_endpoint: u8,
    pub last_slot_id: u8,
    pub addressed_slot_id: u8,
    pub addressed_port: u8,
    pub addressed_speed: u8,
    pub device_context_phys: u64,
    pub input_context_phys: u64,
    pub ep0_ring_phys: u64,
    pub interrupt_ring_phys: u64,
    pub interrupt_buffer_phys: u64,
    pub bulk_in_ring_phys: u64,
    pub bulk_out_ring_phys: u64,
    pub bulk_buffer_phys: u64,
    pub bulk_buffer_frames: u16,
    pub descriptor_phys: u64,
    pub ep0_enqueue: u16,
    pub ep0_cycle: u8,
    pub control_endpoint_faulted: bool,
    pub interrupt_enqueue: u16,
    pub interrupt_cycle: u8,
    pub interrupt_link_cycle: u8,
    pub interrupt_endpoint_id: u8,
    pub interrupt_endpoint_address: u8,
    pub interrupt_endpoint_max_packet: u16,
    pub interrupt_endpoint_interval_raw: u8,
    pub interrupt_endpoint_interval_context: u8,
    pub interrupt_endpoint_configured: bool,
    pub interrupt_endpoint_faulted: bool,
    pub interrupt_pending: bool,
    pub interrupt_pending_trb_phys: u64,
    pub interrupt_transfer_handle: u32,
    pub interrupt_pending_index: u16,
    pub interrupt_hw_dequeue: u64,
    pub bulk_endpoints_configured: bool,
    pub bulk_endpoints_faulted: bool,
    pub bulk_in_endpoint_id: u8,
    pub bulk_out_endpoint_id: u8,
    pub bulk_in_enqueue: u16,
    pub bulk_in_cycle: u8,
    pub bulk_in_link_update_pending: bool,
    pub bulk_out_enqueue: u16,
    pub bulk_out_cycle: u8,
    pub bulk_out_link_update_pending: bool,
    pub set_configuration_attempted: bool,
    pub set_configuration_ok: bool,
    pub hid_set_protocol_attempted: bool,
    pub hid_set_protocol_ok: bool,
    pub hid_set_idle_attempted: bool,
    pub hid_set_idle_ok: bool,
    pub interrupt_polls: u64,
    pub interrupt_reports: u64,
    pub interrupt_no_report: u64,
    pub interrupt_recoveries: u64,
    pub interrupt_pending_timeouts: u64,
    pub interrupt_halted_checks: u64,
    pub interrupt_pending_streak: u64,
    pub last_interrupt_ep_state: u8,
    pub last_interrupt_request_len: u32,
    pub last_interrupt_residue: u32,
    pub last_interrupt_actual_len: u32,
    pub last_interrupt_completion_code: u8,
    pub control_transfers: u64,
    pub control_failures: u64,
    pub control_timeouts: u64,
    pub control_stalls: u64,
    pub control_short_packets: u64,
    pub last_control_request_type: u8,
    pub last_control_request: u8,
    pub last_control_value: u16,
    pub last_control_index: u16,
    pub last_control_length: u16,
    pub last_control_direction: Text,
    pub last_control_completion_code: u8,
    pub last_control_residue: u32,
    pub last_control_ok: bool,
    pub transfer_events: u64,
    pub bulk_transfers: u64,
    pub bulk_failures: u64,
    pub transfer_objects_active: u32,
    pub transfer_objects_high_water: u32,
    pub transfer_objects_submitted: u64,
    pub transfer_objects_completed: u64,
    pub transfer_objects_timed_out: u64,
    pub transfer_objects_cancelled: u64,
    pub transfer_objects_failed: u64,
    pub last_bulk_direction: Text,
    pub last_bulk_result: Text,
    pub last_bulk_request_len: u32,
    pub last_bulk_residue: u32,
    pub last_bulk_actual_len: u32,
    pub last_bulk_completion_code: u8,
    pub ports_probed: u8,
    pub non_hid_devices_skipped: u8,
    pub probe_record_count: u8,
    pub probe_records: [ProbeRecord; 16],
    pub selected_mass_storage: bool,
    pub selected_hid_input: bool,
    pub descriptor_len: u8,
    pub descriptor_type: u8,
    pub usb_version_bcd: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub device_max_packet0: u8,
    pub device_vendor_id: u16,
    pub device_product_id: u16,
    pub device_version_bcd: u16,
    pub manufacturer_index: u8,
    pub product_index: u8,
    pub serial_index: u8,
    pub config_total_length: u16,
    pub config_value: u8,
    pub config_attributes: u8,
    pub config_max_power_ma: u16,
    pub interface_count: u8,
    pub interface_record_count: u8,
    pub interface_records: [Interface; 8],
    pub endpoint_count: u8,
    pub descriptor_records: u8,
    pub descriptor_unknown: u8,
    pub descriptor_malformed: u8,
    pub hid_descriptor_count: u8,
    pub ss_endpoint_companion_count: u8,
    pub selected_interface_reason: Text,
    pub first_interface_number: u8,
    pub first_interface_class: u8,
    pub first_interface_subclass: u8,
    pub first_interface_protocol: u8,
    pub first_endpoint_address: u8,
    pub first_endpoint_attributes: u8,
    pub first_endpoint_max_packet: u16,
    pub first_endpoint_interval: u8,
    pub first_hid_descriptor_len: u8,
    pub first_hid_report_descriptor_len: u16,
    pub bulk_in_endpoint_address: u8,
    pub bulk_in_endpoint_max_packet: u16,
    pub bulk_in_endpoint_max_burst: u8,
    pub bulk_out_endpoint_address: u8,
    pub bulk_out_endpoint_max_packet: u16,
    pub bulk_out_endpoint_max_burst: u8,
    pub string_language_id: u16,
    pub manufacturer_len: u8,
    pub product_len: u8,
    pub serial_len: u8,
    pub manufacturer_string: [u8; 32],
    pub product_string: [u8; 32],
    pub serial_string: [u8; 32],
    pub connected_ports: u8,
    pub enabled_ports: u8,
    pub reset_ports: u8,
    pub failures: u64,
    pub timeouts: u64,
    pub reason: Text,
    pub last_wait_cancelled: bool,
}
impl Status {
    pub const EMPTY: Self = Self {
        probed: false,
        present: false,
        mapped: false,
        device: PciDevice::EMPTY,
        command_before: 0,
        command_after: 0,
        bar0_raw: 0,
        bar1_raw: 0,
        bar_is_io: false,
        bar_is_64: false,
        mmio_phys: 0,
        mmio_virt: 0,
        caplength: 0,
        hciversion: 0,
        hcsparams1: 0,
        hcsparams2: 0,
        hcsparams3: 0,
        hccparams1: 0,
        hccparams2: 0,
        dboff: 0,
        rtsoff: 0,
        op_virt: 0,
        runtime_virt: 0,
        max_slots: 0,
        max_interrupters: 0,
        max_ports: 0,
        usbcmd: 0,
        usbsts: 0,
        pagesize: 0,
        dnctrl: 0,
        config: 0,
        mfindex: 0,
        iman0: 0,
        imod0: 0,
        erstsz0: 0,
        erstba0: 0,
        erdp0: 0,
        crcr: 0,
        dcbaap: 0,
        port_count_seen: 0,
        first_ports: [PortStatus::EMPTY; 16],
        legacy_cap_offset: 0,
        legacy_usblegsup_before: 0,
        legacy_usblegsup_after: 0,
        bios_handoff: Text::literal(b"not-started"),
        controller_halted_before: false,
        controller_stopped: false,
        controller_reset_ok: false,
        controller_running: false,
        dma_ready: false,
        context_size: 32,
        dcbaa_phys: 0,
        command_ring_phys: 0,
        event_ring_phys: 0,
        erst_phys: 0,
        scratchpad_array_phys: 0,
        scratchpad_count: 0,
        max_slots_enabled: 0,
        command_enqueue: 0,
        command_cycle: 1,
        event_dequeue: 0,
        event_cycle: 1,
        event_raw_cycle: 0,
        event_raw_type: 0,
        event_raw_code: 0,
        event_erdp_commits: 0,
        event_max_batch: 0,
        command_ring_wraps: 0,
        event_ring_wraps: 0,
        ep0_ring_wraps: 0,
        interrupt_ring_wraps: 0,
        bulk_in_ring_wraps: 0,
        bulk_out_ring_wraps: 0,
        commands: 0,
        events: 0,
        event_tick_deadline_waits: 0,
        event_hpet_deadline_waits: 0,
        event_tsc_deadline_waits: 0,
        event_guard_waits: 0,
        event_tick_timeouts: 0,
        event_hpet_timeouts: 0,
        event_guard_timeouts: 0,
        event_tsc_timeouts: 0,
        event_recovery_budget_timeouts: 0,
        event_cpu_guard_timeouts: 0,
        host_controller_id: 0,
        owner_id: 0,
        host_source: 0,
        irq_line: 0xFF,
        irq_pin: 0,
        irq_registered: false,
        irq_mode: Text::literal(b"poll"),
        irq_count: 0,
        irq_handled: 0,
        irq_wakeups: 0,
        poll_fallbacks: 0,
        wait_calls: 0,
        wait_successes: 0,
        wait_timeouts: 0,
        wait_early_calls: 0,
        wait_runtime_calls: 0,
        wait_requested_ms: 0,
        wait_elapsed_ns: 0,
        wait_max_elapsed_ns: 0,
        wait_iterations: 0,
        wait_blocked_ticks: 0,
        last_wait_reason: Text::literal(b"none"),
        last_wait_phase: 0,
        last_wait_requested_ms: 0,
        last_wait_elapsed_ns: 0,
        last_wait_iterations: 0,
        last_wait_blocked_ticks: 0,
        last_wait_retry: 0,
        last_wait_success: false,
        stale_events: 0,
        ring_full: 0,
        deferred_events_pending: 0,
        deferred_events_queued: 0,
        deferred_events_delivered: 0,
        deferred_events_overflows: 0,
        deferred_events_purged: 0,
        deferred_events_high_water: 0,
        retained_slots: 0,
        runtime_switches: 0,
        port_change_clears: 0,
        port_change_events: 0,
        port_change_queued: 0,
        port_change_coalesced: 0,
        port_change_invalid: 0,
        port_change_taken: 0,
        port_change_retries: 0,
        port_change_pending_mask: 0,
        port_change_pending: 0,
        port_change_high_water: 0,
        port_service_calls: 0,
        port_service_failures: 0,
        port_catalog_changes: 0,
        port_task_started: false,
        port_task_id: 0,
        port_task_iterations: 0,
        port_debounce_failures: 0,
        port_power_requests: 0,
        port_u0_waits: 0,
        port_warm_resets: 0,
        port_disconnects: 0,
        reclaimed_slots: 0,
        disable_slot_commands: 0,
        disable_slot_failures: 0,
        enable_slot_attempted: false,
        enable_slot_ok: false,
        address_device_attempted: false,
        address_device_ok: false,
        get_descriptor_attempted: false,
        get_descriptor_ok: false,
        get_config_attempted: false,
        get_config_ok: false,
        get_strings_attempted: false,
        get_strings_ok: false,
        last_command_type: 0,
        last_completion_code: 0,
        last_transfer_completion_code: 0,
        last_event_type: 0,
        last_event_code: 0,
        last_event_slot: 0,
        last_event_endpoint: 0,
        last_event_parameter: 0,
        last_event_length: 0,
        last_stale_event_type: 0,
        last_stale_event_code: 0,
        last_stale_event_slot: 0,
        last_stale_event_endpoint: 0,
        last_slot_id: 0,
        addressed_slot_id: 0,
        addressed_port: 0,
        addressed_speed: 0,
        device_context_phys: 0,
        input_context_phys: 0,
        ep0_ring_phys: 0,
        interrupt_ring_phys: 0,
        interrupt_buffer_phys: 0,
        bulk_in_ring_phys: 0,
        bulk_out_ring_phys: 0,
        bulk_buffer_phys: 0,
        bulk_buffer_frames: 0,
        descriptor_phys: 0,
        ep0_enqueue: 0,
        ep0_cycle: 1,
        control_endpoint_faulted: false,
        interrupt_enqueue: 0,
        interrupt_cycle: 1,
        interrupt_link_cycle: 0,
        interrupt_endpoint_id: 0,
        interrupt_endpoint_address: 0,
        interrupt_endpoint_max_packet: 0,
        interrupt_endpoint_interval_raw: 0,
        interrupt_endpoint_interval_context: 0,
        interrupt_endpoint_configured: false,
        interrupt_endpoint_faulted: false,
        interrupt_pending: false,
        interrupt_pending_trb_phys: 0,
        interrupt_transfer_handle: 0,
        interrupt_pending_index: 0,
        interrupt_hw_dequeue: 0,
        bulk_endpoints_configured: false,
        bulk_endpoints_faulted: false,
        bulk_in_endpoint_id: 0,
        bulk_out_endpoint_id: 0,
        bulk_in_enqueue: 0,
        bulk_in_cycle: 1,
        bulk_in_link_update_pending: false,
        bulk_out_enqueue: 0,
        bulk_out_cycle: 1,
        bulk_out_link_update_pending: false,
        set_configuration_attempted: false,
        set_configuration_ok: false,
        hid_set_protocol_attempted: false,
        hid_set_protocol_ok: false,
        hid_set_idle_attempted: false,
        hid_set_idle_ok: false,
        interrupt_polls: 0,
        interrupt_reports: 0,
        interrupt_no_report: 0,
        interrupt_recoveries: 0,
        interrupt_pending_timeouts: 0,
        interrupt_halted_checks: 0,
        interrupt_pending_streak: 0,
        last_interrupt_ep_state: 0,
        last_interrupt_request_len: 0,
        last_interrupt_residue: 0,
        last_interrupt_actual_len: 0,
        last_interrupt_completion_code: 0,
        control_transfers: 0,
        control_failures: 0,
        control_timeouts: 0,
        control_stalls: 0,
        control_short_packets: 0,
        last_control_request_type: 0,
        last_control_request: 0,
        last_control_value: 0,
        last_control_index: 0,
        last_control_length: 0,
        last_control_direction: Text::literal(b"none"),
        last_control_completion_code: 0,
        last_control_residue: 0,
        last_control_ok: false,
        transfer_events: 0,
        bulk_transfers: 0,
        bulk_failures: 0,
        transfer_objects_active: 0,
        transfer_objects_high_water: 0,
        transfer_objects_submitted: 0,
        transfer_objects_completed: 0,
        transfer_objects_timed_out: 0,
        transfer_objects_cancelled: 0,
        transfer_objects_failed: 0,
        last_bulk_direction: Text::literal(b"none"),
        last_bulk_result: Text::literal(b"none"),
        last_bulk_request_len: 0,
        last_bulk_residue: 0,
        last_bulk_actual_len: 0,
        last_bulk_completion_code: 0,
        ports_probed: 0,
        non_hid_devices_skipped: 0,
        probe_record_count: 0,
        probe_records: [ProbeRecord::EMPTY; 16],
        selected_mass_storage: false,
        selected_hid_input: false,
        descriptor_len: 0,
        descriptor_type: 0,
        usb_version_bcd: 0,
        device_class: 0,
        device_subclass: 0,
        device_protocol: 0,
        device_max_packet0: 0,
        device_vendor_id: 0,
        device_product_id: 0,
        device_version_bcd: 0,
        manufacturer_index: 0,
        product_index: 0,
        serial_index: 0,
        config_total_length: 0,
        config_value: 0,
        config_attributes: 0,
        config_max_power_ma: 0,
        interface_count: 0,
        interface_record_count: 0,
        interface_records: [Interface::EMPTY; 8],
        endpoint_count: 0,
        descriptor_records: 0,
        descriptor_unknown: 0,
        descriptor_malformed: 0,
        hid_descriptor_count: 0,
        ss_endpoint_companion_count: 0,
        selected_interface_reason: Text::literal(b"none"),
        first_interface_number: 0,
        first_interface_class: 0,
        first_interface_subclass: 0,
        first_interface_protocol: 0,
        first_endpoint_address: 0,
        first_endpoint_attributes: 0,
        first_endpoint_max_packet: 0,
        first_endpoint_interval: 0,
        first_hid_descriptor_len: 0,
        first_hid_report_descriptor_len: 0,
        bulk_in_endpoint_address: 0,
        bulk_in_endpoint_max_packet: 0,
        bulk_in_endpoint_max_burst: 0,
        bulk_out_endpoint_address: 0,
        bulk_out_endpoint_max_packet: 0,
        bulk_out_endpoint_max_burst: 0,
        string_language_id: 0,
        manufacturer_len: 0,
        product_len: 0,
        serial_len: 0,
        manufacturer_string: [0; 32],
        product_string: [0; 32],
        serial_string: [0; 32],
        connected_ports: 0,
        enabled_ports: 0,
        reset_ports: 0,
        failures: 0,
        timeouts: 0,
        reason: Text::literal(b"not initialized"),
        last_wait_cancelled: false,
    };
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 3176);
    assert!(core::mem::align_of::<Status>() == 8);
    assert!(core::mem::offset_of!(Status, probed) == 0);
    assert!(core::mem::offset_of!(Status, present) == 1);
    assert!(core::mem::offset_of!(Status, mapped) == 2);
    assert!(core::mem::offset_of!(Status, device) == 4);
    assert!(core::mem::offset_of!(Status, command_before) == 16);
    assert!(core::mem::offset_of!(Status, command_after) == 18);
    assert!(core::mem::offset_of!(Status, bar0_raw) == 20);
    assert!(core::mem::offset_of!(Status, bar1_raw) == 24);
    assert!(core::mem::offset_of!(Status, bar_is_io) == 28);
    assert!(core::mem::offset_of!(Status, bar_is_64) == 29);
    assert!(core::mem::offset_of!(Status, mmio_phys) == 32);
    assert!(core::mem::offset_of!(Status, mmio_virt) == 40);
    assert!(core::mem::offset_of!(Status, caplength) == 48);
    assert!(core::mem::offset_of!(Status, hciversion) == 50);
    assert!(core::mem::offset_of!(Status, hcsparams1) == 52);
    assert!(core::mem::offset_of!(Status, hcsparams2) == 56);
    assert!(core::mem::offset_of!(Status, hcsparams3) == 60);
    assert!(core::mem::offset_of!(Status, hccparams1) == 64);
    assert!(core::mem::offset_of!(Status, hccparams2) == 68);
    assert!(core::mem::offset_of!(Status, dboff) == 72);
    assert!(core::mem::offset_of!(Status, rtsoff) == 76);
    assert!(core::mem::offset_of!(Status, op_virt) == 80);
    assert!(core::mem::offset_of!(Status, runtime_virt) == 88);
    assert!(core::mem::offset_of!(Status, max_slots) == 96);
    assert!(core::mem::offset_of!(Status, max_interrupters) == 98);
    assert!(core::mem::offset_of!(Status, max_ports) == 100);
    assert!(core::mem::offset_of!(Status, usbcmd) == 104);
    assert!(core::mem::offset_of!(Status, usbsts) == 108);
    assert!(core::mem::offset_of!(Status, pagesize) == 112);
    assert!(core::mem::offset_of!(Status, dnctrl) == 116);
    assert!(core::mem::offset_of!(Status, config) == 120);
    assert!(core::mem::offset_of!(Status, mfindex) == 124);
    assert!(core::mem::offset_of!(Status, iman0) == 128);
    assert!(core::mem::offset_of!(Status, imod0) == 132);
    assert!(core::mem::offset_of!(Status, erstsz0) == 136);
    assert!(core::mem::offset_of!(Status, erstba0) == 144);
    assert!(core::mem::offset_of!(Status, erdp0) == 152);
    assert!(core::mem::offset_of!(Status, crcr) == 160);
    assert!(core::mem::offset_of!(Status, dcbaap) == 168);
    assert!(core::mem::offset_of!(Status, port_count_seen) == 176);
    assert!(core::mem::offset_of!(Status, first_ports) == 184);
    assert!(core::mem::offset_of!(Status, legacy_cap_offset) == 952);
    assert!(core::mem::offset_of!(Status, legacy_usblegsup_before) == 956);
    assert!(core::mem::offset_of!(Status, legacy_usblegsup_after) == 960);
    assert!(core::mem::offset_of!(Status, bios_handoff) == 968);
    assert!(core::mem::offset_of!(Status, controller_halted_before) == 984);
    assert!(core::mem::offset_of!(Status, controller_stopped) == 985);
    assert!(core::mem::offset_of!(Status, controller_reset_ok) == 986);
    assert!(core::mem::offset_of!(Status, controller_running) == 987);
    assert!(core::mem::offset_of!(Status, dma_ready) == 988);
    assert!(core::mem::offset_of!(Status, context_size) == 989);
    assert!(core::mem::offset_of!(Status, dcbaa_phys) == 992);
    assert!(core::mem::offset_of!(Status, command_ring_phys) == 1000);
    assert!(core::mem::offset_of!(Status, event_ring_phys) == 1008);
    assert!(core::mem::offset_of!(Status, erst_phys) == 1016);
    assert!(core::mem::offset_of!(Status, scratchpad_array_phys) == 1024);
    assert!(core::mem::offset_of!(Status, scratchpad_count) == 1032);
    assert!(core::mem::offset_of!(Status, max_slots_enabled) == 1033);
    assert!(core::mem::offset_of!(Status, command_enqueue) == 1034);
    assert!(core::mem::offset_of!(Status, command_cycle) == 1036);
    assert!(core::mem::offset_of!(Status, event_dequeue) == 1038);
    assert!(core::mem::offset_of!(Status, event_cycle) == 1040);
    assert!(core::mem::offset_of!(Status, event_raw_cycle) == 1041);
    assert!(core::mem::offset_of!(Status, event_raw_type) == 1042);
    assert!(core::mem::offset_of!(Status, event_raw_code) == 1043);
    assert!(core::mem::offset_of!(Status, event_erdp_commits) == 1048);
    assert!(core::mem::offset_of!(Status, event_max_batch) == 1056);
    assert!(core::mem::offset_of!(Status, command_ring_wraps) == 1064);
    assert!(core::mem::offset_of!(Status, event_ring_wraps) == 1072);
    assert!(core::mem::offset_of!(Status, ep0_ring_wraps) == 1080);
    assert!(core::mem::offset_of!(Status, interrupt_ring_wraps) == 1088);
    assert!(core::mem::offset_of!(Status, bulk_in_ring_wraps) == 1096);
    assert!(core::mem::offset_of!(Status, bulk_out_ring_wraps) == 1104);
    assert!(core::mem::offset_of!(Status, commands) == 1112);
    assert!(core::mem::offset_of!(Status, events) == 1120);
    assert!(core::mem::offset_of!(Status, event_tick_deadline_waits) == 1128);
    assert!(core::mem::offset_of!(Status, event_hpet_deadline_waits) == 1136);
    assert!(core::mem::offset_of!(Status, event_tsc_deadline_waits) == 1144);
    assert!(core::mem::offset_of!(Status, event_guard_waits) == 1152);
    assert!(core::mem::offset_of!(Status, event_tick_timeouts) == 1160);
    assert!(core::mem::offset_of!(Status, event_hpet_timeouts) == 1168);
    assert!(core::mem::offset_of!(Status, event_guard_timeouts) == 1176);
    assert!(core::mem::offset_of!(Status, event_tsc_timeouts) == 1184);
    assert!(core::mem::offset_of!(Status, event_recovery_budget_timeouts) == 1192);
    assert!(core::mem::offset_of!(Status, event_cpu_guard_timeouts) == 1200);
    assert!(core::mem::offset_of!(Status, host_controller_id) == 1208);
    assert!(core::mem::offset_of!(Status, owner_id) == 1212);
    assert!(core::mem::offset_of!(Status, host_source) == 1216);
    assert!(core::mem::offset_of!(Status, irq_line) == 1220);
    assert!(core::mem::offset_of!(Status, irq_pin) == 1221);
    assert!(core::mem::offset_of!(Status, irq_registered) == 1222);
    assert!(core::mem::offset_of!(Status, irq_mode) == 1224);
    assert!(core::mem::offset_of!(Status, irq_count) == 1240);
    assert!(core::mem::offset_of!(Status, irq_handled) == 1248);
    assert!(core::mem::offset_of!(Status, irq_wakeups) == 1256);
    assert!(core::mem::offset_of!(Status, poll_fallbacks) == 1264);
    assert!(core::mem::offset_of!(Status, wait_calls) == 1272);
    assert!(core::mem::offset_of!(Status, wait_successes) == 1280);
    assert!(core::mem::offset_of!(Status, wait_timeouts) == 1288);
    assert!(core::mem::offset_of!(Status, wait_early_calls) == 1296);
    assert!(core::mem::offset_of!(Status, wait_runtime_calls) == 1304);
    assert!(core::mem::offset_of!(Status, wait_requested_ms) == 1312);
    assert!(core::mem::offset_of!(Status, wait_elapsed_ns) == 1320);
    assert!(core::mem::offset_of!(Status, wait_max_elapsed_ns) == 1328);
    assert!(core::mem::offset_of!(Status, wait_iterations) == 1336);
    assert!(core::mem::offset_of!(Status, wait_blocked_ticks) == 1344);
    assert!(core::mem::offset_of!(Status, last_wait_reason) == 1352);
    assert!(core::mem::offset_of!(Status, last_wait_phase) == 1368);
    assert!(core::mem::offset_of!(Status, last_wait_requested_ms) == 1372);
    assert!(core::mem::offset_of!(Status, last_wait_elapsed_ns) == 1376);
    assert!(core::mem::offset_of!(Status, last_wait_iterations) == 1384);
    assert!(core::mem::offset_of!(Status, last_wait_blocked_ticks) == 1392);
    assert!(core::mem::offset_of!(Status, last_wait_retry) == 1400);
    assert!(core::mem::offset_of!(Status, last_wait_success) == 1401);
    assert!(core::mem::offset_of!(Status, stale_events) == 1408);
    assert!(core::mem::offset_of!(Status, ring_full) == 1416);
    assert!(core::mem::offset_of!(Status, deferred_events_pending) == 1424);
    assert!(core::mem::offset_of!(Status, deferred_events_queued) == 1432);
    assert!(core::mem::offset_of!(Status, deferred_events_delivered) == 1440);
    assert!(core::mem::offset_of!(Status, deferred_events_overflows) == 1448);
    assert!(core::mem::offset_of!(Status, deferred_events_purged) == 1456);
    assert!(core::mem::offset_of!(Status, deferred_events_high_water) == 1464);
    assert!(core::mem::offset_of!(Status, retained_slots) == 1472);
    assert!(core::mem::offset_of!(Status, runtime_switches) == 1480);
    assert!(core::mem::offset_of!(Status, port_change_clears) == 1488);
    assert!(core::mem::offset_of!(Status, port_change_events) == 1496);
    assert!(core::mem::offset_of!(Status, port_change_queued) == 1504);
    assert!(core::mem::offset_of!(Status, port_change_coalesced) == 1512);
    assert!(core::mem::offset_of!(Status, port_change_invalid) == 1520);
    assert!(core::mem::offset_of!(Status, port_change_taken) == 1528);
    assert!(core::mem::offset_of!(Status, port_change_retries) == 1536);
    assert!(core::mem::offset_of!(Status, port_change_pending_mask) == 1544);
    assert!(core::mem::offset_of!(Status, port_change_pending) == 1546);
    assert!(core::mem::offset_of!(Status, port_change_high_water) == 1547);
    assert!(core::mem::offset_of!(Status, port_service_calls) == 1552);
    assert!(core::mem::offset_of!(Status, port_service_failures) == 1560);
    assert!(core::mem::offset_of!(Status, port_catalog_changes) == 1568);
    assert!(core::mem::offset_of!(Status, port_task_started) == 1576);
    assert!(core::mem::offset_of!(Status, port_task_id) == 1580);
    assert!(core::mem::offset_of!(Status, port_task_iterations) == 1584);
    assert!(core::mem::offset_of!(Status, port_debounce_failures) == 1592);
    assert!(core::mem::offset_of!(Status, port_power_requests) == 1600);
    assert!(core::mem::offset_of!(Status, port_u0_waits) == 1608);
    assert!(core::mem::offset_of!(Status, port_warm_resets) == 1616);
    assert!(core::mem::offset_of!(Status, port_disconnects) == 1624);
    assert!(core::mem::offset_of!(Status, reclaimed_slots) == 1632);
    assert!(core::mem::offset_of!(Status, disable_slot_commands) == 1640);
    assert!(core::mem::offset_of!(Status, disable_slot_failures) == 1648);
    assert!(core::mem::offset_of!(Status, enable_slot_attempted) == 1656);
    assert!(core::mem::offset_of!(Status, enable_slot_ok) == 1657);
    assert!(core::mem::offset_of!(Status, address_device_attempted) == 1658);
    assert!(core::mem::offset_of!(Status, address_device_ok) == 1659);
    assert!(core::mem::offset_of!(Status, get_descriptor_attempted) == 1660);
    assert!(core::mem::offset_of!(Status, get_descriptor_ok) == 1661);
    assert!(core::mem::offset_of!(Status, get_config_attempted) == 1662);
    assert!(core::mem::offset_of!(Status, get_config_ok) == 1663);
    assert!(core::mem::offset_of!(Status, get_strings_attempted) == 1664);
    assert!(core::mem::offset_of!(Status, get_strings_ok) == 1665);
    assert!(core::mem::offset_of!(Status, last_command_type) == 1666);
    assert!(core::mem::offset_of!(Status, last_completion_code) == 1667);
    assert!(core::mem::offset_of!(Status, last_transfer_completion_code) == 1668);
    assert!(core::mem::offset_of!(Status, last_event_type) == 1669);
    assert!(core::mem::offset_of!(Status, last_event_code) == 1670);
    assert!(core::mem::offset_of!(Status, last_event_slot) == 1671);
    assert!(core::mem::offset_of!(Status, last_event_endpoint) == 1672);
    assert!(core::mem::offset_of!(Status, last_event_parameter) == 1680);
    assert!(core::mem::offset_of!(Status, last_event_length) == 1688);
    assert!(core::mem::offset_of!(Status, last_stale_event_type) == 1692);
    assert!(core::mem::offset_of!(Status, last_stale_event_code) == 1693);
    assert!(core::mem::offset_of!(Status, last_stale_event_slot) == 1694);
    assert!(core::mem::offset_of!(Status, last_stale_event_endpoint) == 1695);
    assert!(core::mem::offset_of!(Status, last_slot_id) == 1696);
    assert!(core::mem::offset_of!(Status, addressed_slot_id) == 1697);
    assert!(core::mem::offset_of!(Status, addressed_port) == 1698);
    assert!(core::mem::offset_of!(Status, addressed_speed) == 1699);
    assert!(core::mem::offset_of!(Status, device_context_phys) == 1704);
    assert!(core::mem::offset_of!(Status, input_context_phys) == 1712);
    assert!(core::mem::offset_of!(Status, ep0_ring_phys) == 1720);
    assert!(core::mem::offset_of!(Status, interrupt_ring_phys) == 1728);
    assert!(core::mem::offset_of!(Status, interrupt_buffer_phys) == 1736);
    assert!(core::mem::offset_of!(Status, bulk_in_ring_phys) == 1744);
    assert!(core::mem::offset_of!(Status, bulk_out_ring_phys) == 1752);
    assert!(core::mem::offset_of!(Status, bulk_buffer_phys) == 1760);
    assert!(core::mem::offset_of!(Status, bulk_buffer_frames) == 1768);
    assert!(core::mem::offset_of!(Status, descriptor_phys) == 1776);
    assert!(core::mem::offset_of!(Status, ep0_enqueue) == 1784);
    assert!(core::mem::offset_of!(Status, ep0_cycle) == 1786);
    assert!(core::mem::offset_of!(Status, control_endpoint_faulted) == 1787);
    assert!(core::mem::offset_of!(Status, interrupt_enqueue) == 1788);
    assert!(core::mem::offset_of!(Status, interrupt_cycle) == 1790);
    assert!(core::mem::offset_of!(Status, interrupt_link_cycle) == 1791);
    assert!(core::mem::offset_of!(Status, interrupt_endpoint_id) == 1792);
    assert!(core::mem::offset_of!(Status, interrupt_endpoint_address) == 1793);
    assert!(core::mem::offset_of!(Status, interrupt_endpoint_max_packet) == 1794);
    assert!(core::mem::offset_of!(Status, interrupt_endpoint_interval_raw) == 1796);
    assert!(core::mem::offset_of!(Status, interrupt_endpoint_interval_context) == 1797);
    assert!(core::mem::offset_of!(Status, interrupt_endpoint_configured) == 1798);
    assert!(core::mem::offset_of!(Status, interrupt_endpoint_faulted) == 1799);
    assert!(core::mem::offset_of!(Status, interrupt_pending) == 1800);
    assert!(core::mem::offset_of!(Status, interrupt_pending_trb_phys) == 1808);
    assert!(core::mem::offset_of!(Status, interrupt_transfer_handle) == 1816);
    assert!(core::mem::offset_of!(Status, interrupt_pending_index) == 1820);
    assert!(core::mem::offset_of!(Status, interrupt_hw_dequeue) == 1824);
    assert!(core::mem::offset_of!(Status, bulk_endpoints_configured) == 1832);
    assert!(core::mem::offset_of!(Status, bulk_endpoints_faulted) == 1833);
    assert!(core::mem::offset_of!(Status, bulk_in_endpoint_id) == 1834);
    assert!(core::mem::offset_of!(Status, bulk_out_endpoint_id) == 1835);
    assert!(core::mem::offset_of!(Status, bulk_in_enqueue) == 1836);
    assert!(core::mem::offset_of!(Status, bulk_in_cycle) == 1838);
    assert!(core::mem::offset_of!(Status, bulk_in_link_update_pending) == 1839);
    assert!(core::mem::offset_of!(Status, bulk_out_enqueue) == 1840);
    assert!(core::mem::offset_of!(Status, bulk_out_cycle) == 1842);
    assert!(core::mem::offset_of!(Status, bulk_out_link_update_pending) == 1843);
    assert!(core::mem::offset_of!(Status, set_configuration_attempted) == 1844);
    assert!(core::mem::offset_of!(Status, set_configuration_ok) == 1845);
    assert!(core::mem::offset_of!(Status, hid_set_protocol_attempted) == 1846);
    assert!(core::mem::offset_of!(Status, hid_set_protocol_ok) == 1847);
    assert!(core::mem::offset_of!(Status, hid_set_idle_attempted) == 1848);
    assert!(core::mem::offset_of!(Status, hid_set_idle_ok) == 1849);
    assert!(core::mem::offset_of!(Status, interrupt_polls) == 1856);
    assert!(core::mem::offset_of!(Status, interrupt_reports) == 1864);
    assert!(core::mem::offset_of!(Status, interrupt_no_report) == 1872);
    assert!(core::mem::offset_of!(Status, interrupt_recoveries) == 1880);
    assert!(core::mem::offset_of!(Status, interrupt_pending_timeouts) == 1888);
    assert!(core::mem::offset_of!(Status, interrupt_halted_checks) == 1896);
    assert!(core::mem::offset_of!(Status, interrupt_pending_streak) == 1904);
    assert!(core::mem::offset_of!(Status, last_interrupt_ep_state) == 1912);
    assert!(core::mem::offset_of!(Status, last_interrupt_request_len) == 1916);
    assert!(core::mem::offset_of!(Status, last_interrupt_residue) == 1920);
    assert!(core::mem::offset_of!(Status, last_interrupt_actual_len) == 1924);
    assert!(core::mem::offset_of!(Status, last_interrupt_completion_code) == 1928);
    assert!(core::mem::offset_of!(Status, control_transfers) == 1936);
    assert!(core::mem::offset_of!(Status, control_failures) == 1944);
    assert!(core::mem::offset_of!(Status, control_timeouts) == 1952);
    assert!(core::mem::offset_of!(Status, control_stalls) == 1960);
    assert!(core::mem::offset_of!(Status, control_short_packets) == 1968);
    assert!(core::mem::offset_of!(Status, last_control_request_type) == 1976);
    assert!(core::mem::offset_of!(Status, last_control_request) == 1977);
    assert!(core::mem::offset_of!(Status, last_control_value) == 1978);
    assert!(core::mem::offset_of!(Status, last_control_index) == 1980);
    assert!(core::mem::offset_of!(Status, last_control_length) == 1982);
    assert!(core::mem::offset_of!(Status, last_control_direction) == 1984);
    assert!(core::mem::offset_of!(Status, last_control_completion_code) == 2000);
    assert!(core::mem::offset_of!(Status, last_control_residue) == 2004);
    assert!(core::mem::offset_of!(Status, last_control_ok) == 2008);
    assert!(core::mem::offset_of!(Status, transfer_events) == 2016);
    assert!(core::mem::offset_of!(Status, bulk_transfers) == 2024);
    assert!(core::mem::offset_of!(Status, bulk_failures) == 2032);
    assert!(core::mem::offset_of!(Status, transfer_objects_active) == 2040);
    assert!(core::mem::offset_of!(Status, transfer_objects_high_water) == 2044);
    assert!(core::mem::offset_of!(Status, transfer_objects_submitted) == 2048);
    assert!(core::mem::offset_of!(Status, transfer_objects_completed) == 2056);
    assert!(core::mem::offset_of!(Status, transfer_objects_timed_out) == 2064);
    assert!(core::mem::offset_of!(Status, transfer_objects_cancelled) == 2072);
    assert!(core::mem::offset_of!(Status, transfer_objects_failed) == 2080);
    assert!(core::mem::offset_of!(Status, last_bulk_direction) == 2088);
    assert!(core::mem::offset_of!(Status, last_bulk_result) == 2104);
    assert!(core::mem::offset_of!(Status, last_bulk_request_len) == 2120);
    assert!(core::mem::offset_of!(Status, last_bulk_residue) == 2124);
    assert!(core::mem::offset_of!(Status, last_bulk_actual_len) == 2128);
    assert!(core::mem::offset_of!(Status, last_bulk_completion_code) == 2132);
    assert!(core::mem::offset_of!(Status, ports_probed) == 2133);
    assert!(core::mem::offset_of!(Status, non_hid_devices_skipped) == 2134);
    assert!(core::mem::offset_of!(Status, probe_record_count) == 2135);
    assert!(core::mem::offset_of!(Status, probe_records) == 2136);
    assert!(core::mem::offset_of!(Status, selected_mass_storage) == 2776);
    assert!(core::mem::offset_of!(Status, selected_hid_input) == 2777);
    assert!(core::mem::offset_of!(Status, descriptor_len) == 2778);
    assert!(core::mem::offset_of!(Status, descriptor_type) == 2779);
    assert!(core::mem::offset_of!(Status, usb_version_bcd) == 2780);
    assert!(core::mem::offset_of!(Status, device_class) == 2782);
    assert!(core::mem::offset_of!(Status, device_subclass) == 2783);
    assert!(core::mem::offset_of!(Status, device_protocol) == 2784);
    assert!(core::mem::offset_of!(Status, device_max_packet0) == 2785);
    assert!(core::mem::offset_of!(Status, device_vendor_id) == 2786);
    assert!(core::mem::offset_of!(Status, device_product_id) == 2788);
    assert!(core::mem::offset_of!(Status, device_version_bcd) == 2790);
    assert!(core::mem::offset_of!(Status, manufacturer_index) == 2792);
    assert!(core::mem::offset_of!(Status, product_index) == 2793);
    assert!(core::mem::offset_of!(Status, serial_index) == 2794);
    assert!(core::mem::offset_of!(Status, config_total_length) == 2796);
    assert!(core::mem::offset_of!(Status, config_value) == 2798);
    assert!(core::mem::offset_of!(Status, config_attributes) == 2799);
    assert!(core::mem::offset_of!(Status, config_max_power_ma) == 2800);
    assert!(core::mem::offset_of!(Status, interface_count) == 2802);
    assert!(core::mem::offset_of!(Status, interface_record_count) == 2803);
    assert!(core::mem::offset_of!(Status, interface_records) == 2804);
    assert!(core::mem::offset_of!(Status, endpoint_count) == 2980);
    assert!(core::mem::offset_of!(Status, descriptor_records) == 2981);
    assert!(core::mem::offset_of!(Status, descriptor_unknown) == 2982);
    assert!(core::mem::offset_of!(Status, descriptor_malformed) == 2983);
    assert!(core::mem::offset_of!(Status, hid_descriptor_count) == 2984);
    assert!(core::mem::offset_of!(Status, ss_endpoint_companion_count) == 2985);
    assert!(core::mem::offset_of!(Status, selected_interface_reason) == 2992);
    assert!(core::mem::offset_of!(Status, first_interface_number) == 3008);
    assert!(core::mem::offset_of!(Status, first_interface_class) == 3009);
    assert!(core::mem::offset_of!(Status, first_interface_subclass) == 3010);
    assert!(core::mem::offset_of!(Status, first_interface_protocol) == 3011);
    assert!(core::mem::offset_of!(Status, first_endpoint_address) == 3012);
    assert!(core::mem::offset_of!(Status, first_endpoint_attributes) == 3013);
    assert!(core::mem::offset_of!(Status, first_endpoint_max_packet) == 3014);
    assert!(core::mem::offset_of!(Status, first_endpoint_interval) == 3016);
    assert!(core::mem::offset_of!(Status, first_hid_descriptor_len) == 3017);
    assert!(core::mem::offset_of!(Status, first_hid_report_descriptor_len) == 3018);
    assert!(core::mem::offset_of!(Status, bulk_in_endpoint_address) == 3020);
    assert!(core::mem::offset_of!(Status, bulk_in_endpoint_max_packet) == 3022);
    assert!(core::mem::offset_of!(Status, bulk_in_endpoint_max_burst) == 3024);
    assert!(core::mem::offset_of!(Status, bulk_out_endpoint_address) == 3025);
    assert!(core::mem::offset_of!(Status, bulk_out_endpoint_max_packet) == 3026);
    assert!(core::mem::offset_of!(Status, bulk_out_endpoint_max_burst) == 3028);
    assert!(core::mem::offset_of!(Status, string_language_id) == 3030);
    assert!(core::mem::offset_of!(Status, manufacturer_len) == 3032);
    assert!(core::mem::offset_of!(Status, product_len) == 3033);
    assert!(core::mem::offset_of!(Status, serial_len) == 3034);
    assert!(core::mem::offset_of!(Status, manufacturer_string) == 3035);
    assert!(core::mem::offset_of!(Status, product_string) == 3067);
    assert!(core::mem::offset_of!(Status, serial_string) == 3099);
    assert!(core::mem::offset_of!(Status, connected_ports) == 3131);
    assert!(core::mem::offset_of!(Status, enabled_ports) == 3132);
    assert!(core::mem::offset_of!(Status, reset_ports) == 3133);
    assert!(core::mem::offset_of!(Status, failures) == 3136);
    assert!(core::mem::offset_of!(Status, timeouts) == 3144);
    assert!(core::mem::offset_of!(Status, reason) == 3152);
    assert!(core::mem::offset_of!(Status, last_wait_cancelled) == 3168);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BulkSubmission {
    pub trb_phys: [u64; 32],
    pub trb_count: u8,
}
impl BulkSubmission {
    pub const EMPTY: Self = Self {
        trb_phys: [0; 32],
        trb_count: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<BulkSubmission>() == 264);
    assert!(core::mem::align_of::<BulkSubmission>() == 8);
    assert!(core::mem::offset_of!(BulkSubmission, trb_phys) == 0);
    assert!(core::mem::offset_of!(BulkSubmission, trb_count) == 256);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BulkTrbSubmission {
    pub trb_phys: u64,
}
impl BulkTrbSubmission {
    pub const EMPTY: Self = Self { trb_phys: 0 };
}
const _: () = {
    assert!(core::mem::size_of::<BulkTrbSubmission>() == 8);
    assert!(core::mem::align_of::<BulkTrbSubmission>() == 8);
    assert!(core::mem::offset_of!(BulkTrbSubmission, trb_phys) == 0);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptSubmission {
    pub trb_phys: u64,
    pub enqueue: u16,
    pub producer_cycle: u8,
    pub advance: Advance,
}
impl InterruptSubmission {
    pub const EMPTY: Self = Self {
        trb_phys: 0,
        enqueue: 0,
        producer_cycle: 0,
        advance: Advance {
            next_enqueue: 0,
            next_producer_cycle: 0,
            link_cycle: 0,
            wrapped: false,
        },
    };
}
const _: () = {
    assert!(core::mem::size_of::<InterruptSubmission>() == 24);
    assert!(core::mem::align_of::<InterruptSubmission>() == 8);
    assert!(core::mem::offset_of!(InterruptSubmission, trb_phys) == 0);
    assert!(core::mem::offset_of!(InterruptSubmission, enqueue) == 8);
    assert!(core::mem::offset_of!(InterruptSubmission, producer_cycle) == 10);
    assert!(core::mem::offset_of!(InterruptSubmission, advance) == 12);
};
