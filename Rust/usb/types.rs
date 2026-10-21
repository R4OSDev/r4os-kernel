//! Private USB records. External descriptor/handle layouts remain unchanged.
use crate::sched::sync_types::Text;
use core::ffi::c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Interface {
    pub active: bool,
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
    pub bulk_in_endpoint_max_burst: u8,
    pub bulk_out_endpoint_address: u8,
    pub bulk_out_endpoint_max_burst: u8,
    pub hid_descriptor_len: u8,
    pub hid_report_descriptor_len: u16,
}
impl Interface {
    pub const EMPTY: Self = Self {
        active: false,
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
        bulk_in_endpoint_max_burst: 0,
        bulk_out_endpoint_address: 0,
        bulk_out_endpoint_max_burst: 0,
        hid_descriptor_len: 0,
        hid_report_descriptor_len: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<Interface>() == 22);
    assert!(core::mem::align_of::<Interface>() == 2);
    assert!(core::mem::offset_of!(Interface, active) == 0);
    assert!(core::mem::offset_of!(Interface, number) == 1);
    assert!(core::mem::offset_of!(Interface, class_code) == 2);
    assert!(core::mem::offset_of!(Interface, subclass) == 3);
    assert!(core::mem::offset_of!(Interface, protocol) == 4);
    assert!(core::mem::offset_of!(Interface, endpoint_count) == 5);
    assert!(core::mem::offset_of!(Interface, first_endpoint_address) == 6);
    assert!(core::mem::offset_of!(Interface, first_endpoint_attributes) == 7);
    assert!(core::mem::offset_of!(Interface, first_endpoint_max_packet) == 8);
    assert!(core::mem::offset_of!(Interface, first_endpoint_interval) == 10);
    assert!(core::mem::offset_of!(Interface, interrupt_in_endpoint_address) == 11);
    assert!(core::mem::offset_of!(Interface, interrupt_in_endpoint_max_packet) == 12);
    assert!(core::mem::offset_of!(Interface, interrupt_in_endpoint_interval) == 14);
    assert!(core::mem::offset_of!(Interface, bulk_in_endpoint_address) == 15);
    assert!(core::mem::offset_of!(Interface, bulk_in_endpoint_max_burst) == 16);
    assert!(core::mem::offset_of!(Interface, bulk_out_endpoint_address) == 17);
    assert!(core::mem::offset_of!(Interface, bulk_out_endpoint_max_burst) == 18);
    assert!(core::mem::offset_of!(Interface, hid_descriptor_len) == 19);
    assert!(core::mem::offset_of!(Interface, hid_report_descriptor_len) == 20);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Device {
    pub controller_id: u32,
    pub generation: u64,
    pub active: bool,
    pub configured: bool,
    pub controller: Text,
    pub slot_id: u8,
    pub port: u8,
    pub speed: u8,
    pub usb_version_bcd: u16,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_version_bcd: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub config_value: u8,
    pub config_attributes: u8,
    pub config_max_power_ma: u16,
    pub interface_count: u8,
    pub interface_record_count: u8,
    pub interfaces: [Interface; 8],
    pub endpoint_count: u8,
    pub first_interface_number: u8,
    pub first_interface_class: u8,
    pub first_interface_subclass: u8,
    pub first_interface_protocol: u8,
    pub first_endpoint_address: u8,
    pub first_endpoint_attributes: u8,
    pub first_endpoint_max_packet: u16,
    pub first_endpoint_interval: u8,
    pub hid_descriptor_len: u8,
    pub hid_report_descriptor_len: u16,
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
}
impl Device {
    pub const EMPTY: Self = Self {
        controller_id: 0,
        generation: 0,
        active: false,
        configured: false,
        controller: Text::literal(b"unknown"),
        slot_id: 0,
        port: 0,
        speed: 0,
        usb_version_bcd: 0,
        vendor_id: 0,
        product_id: 0,
        device_version_bcd: 0,
        device_class: 0,
        device_subclass: 0,
        device_protocol: 0,
        config_value: 0,
        config_attributes: 0,
        config_max_power_ma: 0,
        interface_count: 0,
        interface_record_count: 0,
        interfaces: [Interface::EMPTY; 8],
        endpoint_count: 0,
        first_interface_number: 0,
        first_interface_class: 0,
        first_interface_subclass: 0,
        first_interface_protocol: 0,
        first_endpoint_address: 0,
        first_endpoint_attributes: 0,
        first_endpoint_max_packet: 0,
        first_endpoint_interval: 0,
        hid_descriptor_len: 0,
        hid_report_descriptor_len: 0,
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
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CoreSummary {
    pub count: u32,
    pub configured: u32,
    pub truncated: bool,
}
impl CoreSummary {
    pub const EMPTY: Self = Self {
        count: 0,
        configured: 0,
        truncated: false,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostStatus {
    pub state: u32,
    pub source: u32,
    pub ports: u32,
    pub devices: u32,
    pub transfers: u64,
    pub failures: u64,
    pub flags: u32,
    pub queue_depth: u16,
    pub reserved0: u16,
    pub max_transfer_bytes: u32,
    pub active_transfers: u32,
    pub completions: u64,
    pub interrupts: u64,
    pub polls: u64,
    pub cancellations: u64,
}
impl HostStatus {
    pub const EMPTY: Self = Self {
        state: 0,
        source: 0,
        ports: 0,
        devices: 0,
        transfers: 0,
        failures: 0,
        flags: 0,
        queue_depth: 0,
        reserved0: 0,
        max_transfer_bytes: 0,
        active_transfers: 0,
        completions: 0,
        interrupts: 0,
        polls: 0,
        cancellations: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<HostStatus>() == 80);
    assert!(core::mem::align_of::<HostStatus>() == 8);
    assert!(core::mem::offset_of!(HostStatus, state) == 0);
    assert!(core::mem::offset_of!(HostStatus, source) == 4);
    assert!(core::mem::offset_of!(HostStatus, ports) == 8);
    assert!(core::mem::offset_of!(HostStatus, devices) == 12);
    assert!(core::mem::offset_of!(HostStatus, transfers) == 16);
    assert!(core::mem::offset_of!(HostStatus, failures) == 24);
    assert!(core::mem::offset_of!(HostStatus, flags) == 32);
    assert!(core::mem::offset_of!(HostStatus, queue_depth) == 36);
    assert!(core::mem::offset_of!(HostStatus, reserved0) == 38);
    assert!(core::mem::offset_of!(HostStatus, max_transfer_bytes) == 40);
    assert!(core::mem::offset_of!(HostStatus, active_transfers) == 44);
    assert!(core::mem::offset_of!(HostStatus, completions) == 48);
    assert!(core::mem::offset_of!(HostStatus, interrupts) == 56);
    assert!(core::mem::offset_of!(HostStatus, polls) == 64);
    assert!(core::mem::offset_of!(HostStatus, cancellations) == 72);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostDevice {
    pub controller_id: u32,
    pub port: u8,
    pub slot_id: u8,
    pub speed: u8,
    pub config_value: u8,
    pub vendor_id: u16,
    pub product_id: u16,
}
impl HostDevice {
    pub const EMPTY: Self = Self {
        controller_id: 0,
        port: 0,
        slot_id: 0,
        speed: 0,
        config_value: 0,
        vendor_id: 0,
        product_id: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<HostDevice>() == 12);
    assert!(core::mem::align_of::<HostDevice>() == 4);
    assert!(core::mem::offset_of!(HostDevice, controller_id) == 0);
    assert!(core::mem::offset_of!(HostDevice, port) == 4);
    assert!(core::mem::offset_of!(HostDevice, slot_id) == 5);
    assert!(core::mem::offset_of!(HostDevice, speed) == 6);
    assert!(core::mem::offset_of!(HostDevice, config_value) == 7);
    assert!(core::mem::offset_of!(HostDevice, vendor_id) == 8);
    assert!(core::mem::offset_of!(HostDevice, product_id) == 10);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostEndpoint {
    pub device: HostDevice,
    pub kind: u32,
    pub address: u8,
    pub endpoint_id: u8,
    pub max_packet: u16,
    pub interval: u8,
    pub max_burst: u8,
}
impl HostEndpoint {
    pub const EMPTY: Self = Self {
        device: HostDevice::EMPTY,
        kind: 0,
        address: 0,
        endpoint_id: 0,
        max_packet: 0,
        interval: 0,
        max_burst: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<HostEndpoint>() == 24);
    assert!(core::mem::align_of::<HostEndpoint>() == 4);
    assert!(core::mem::offset_of!(HostEndpoint, device) == 0);
    assert!(core::mem::offset_of!(HostEndpoint, kind) == 12);
    assert!(core::mem::offset_of!(HostEndpoint, address) == 16);
    assert!(core::mem::offset_of!(HostEndpoint, endpoint_id) == 17);
    assert!(core::mem::offset_of!(HostEndpoint, max_packet) == 18);
    assert!(core::mem::offset_of!(HostEndpoint, interval) == 20);
    assert!(core::mem::offset_of!(HostEndpoint, max_burst) == 21);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ControlRequest {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index_value: u16,
    pub length: u16,
    pub direction: u32,
}
impl ControlRequest {
    pub const EMPTY: Self = Self {
        request_type: 0,
        request: 0,
        value: 0,
        index_value: 0,
        length: 0,
        direction: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<ControlRequest>() == 12);
    assert!(core::mem::align_of::<ControlRequest>() == 4);
    assert!(core::mem::offset_of!(ControlRequest, request_type) == 0);
    assert!(core::mem::offset_of!(ControlRequest, request) == 1);
    assert!(core::mem::offset_of!(ControlRequest, value) == 2);
    assert!(core::mem::offset_of!(ControlRequest, index_value) == 4);
    assert!(core::mem::offset_of!(ControlRequest, length) == 6);
    assert!(core::mem::offset_of!(ControlRequest, direction) == 8);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostDescriptor {
    pub version: u32,
    pub size: u32,
    pub flags: u32,
    pub source: u32,
    pub context: *mut c_void,
    pub port_scan: usize,
    pub address_device: usize,
    pub configure_device: usize,
    pub control_transfer: usize,
    pub bulk_transfer: usize,
    pub interrupt_transfer: usize,
    pub reset_port: usize,
    pub clear_halt: usize,
    pub reset_endpoint: usize,
    pub poll: usize,
    pub shutdown: usize,
    pub status: usize,
}
impl HostDescriptor {
    pub const EMPTY: Self = Self {
        version: 2,
        size: 120,
        flags: 0,
        source: 0,
        context: core::ptr::null_mut(),
        port_scan: 0,
        address_device: 0,
        configure_device: 0,
        control_transfer: 0,
        bulk_transfer: 0,
        interrupt_transfer: 0,
        reset_port: 0,
        clear_halt: 0,
        reset_endpoint: 0,
        poll: 0,
        shutdown: 0,
        status: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<HostDescriptor>() == 120);
    assert!(core::mem::align_of::<HostDescriptor>() == 8);
    assert!(core::mem::offset_of!(HostDescriptor, version) == 0);
    assert!(core::mem::offset_of!(HostDescriptor, size) == 4);
    assert!(core::mem::offset_of!(HostDescriptor, flags) == 8);
    assert!(core::mem::offset_of!(HostDescriptor, source) == 12);
    assert!(core::mem::offset_of!(HostDescriptor, context) == 16);
    assert!(core::mem::offset_of!(HostDescriptor, port_scan) == 24);
    assert!(core::mem::offset_of!(HostDescriptor, address_device) == 32);
    assert!(core::mem::offset_of!(HostDescriptor, configure_device) == 40);
    assert!(core::mem::offset_of!(HostDescriptor, control_transfer) == 48);
    assert!(core::mem::offset_of!(HostDescriptor, bulk_transfer) == 56);
    assert!(core::mem::offset_of!(HostDescriptor, interrupt_transfer) == 64);
    assert!(core::mem::offset_of!(HostDescriptor, reset_port) == 72);
    assert!(core::mem::offset_of!(HostDescriptor, clear_halt) == 80);
    assert!(core::mem::offset_of!(HostDescriptor, reset_endpoint) == 88);
    assert!(core::mem::offset_of!(HostDescriptor, poll) == 96);
    assert!(core::mem::offset_of!(HostDescriptor, shutdown) == 104);
    assert!(core::mem::offset_of!(HostDescriptor, status) == 112);
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Controller {
    pub id: u32,
    pub callbacks: u32,
    pub closing: bool,
    pub used: bool,
    pub name: [u8; 32],
    pub name_len: usize,
    pub source: u8,
    pub owner_id: u32,
    pub state: u8,
    pub descriptor: *const HostDescriptor,
    pub transfers: u64,
    pub failures: u64,
    pub polls: u64,
}
impl Controller {
    pub const EMPTY: Self = Self {
        id: 0,
        callbacks: 0,
        closing: false,
        used: false,
        name: [0; 32],
        name_len: 0,
        source: 0,
        owner_id: 0,
        state: 0,
        descriptor: core::ptr::null(),
        transfers: 0,
        failures: 0,
        polls: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CleanupResult {
    pub removed: u32,
    pub failed: bool,
}
impl CleanupResult {
    pub const EMPTY: Self = Self {
        removed: 0,
        failed: false,
    };
}
const _: () = {
    assert!(core::mem::size_of::<CleanupResult>() == 8);
    assert!(core::mem::align_of::<CleanupResult>() == 4);
    assert!(core::mem::offset_of!(CleanupResult, removed) == 0);
    assert!(core::mem::offset_of!(CleanupResult, failed) == 4);
};
