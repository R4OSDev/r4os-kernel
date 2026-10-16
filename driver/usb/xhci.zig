// Sole native xHCI owner; fixed compatibility records and calls only.
const usb_core = @import("core.zig");
const usb_host = @import("host_controller.zig");
const usb_wait = @import("usb_boot_wait.zig");
const pcie = @import("../../platform/pci_inventory.zig");
const diag_screen = @import("../../kernel/diag_screen.zig");
pub const Text = usb_core.Text;
const PciDevice = pcie.Device;
const Interface = usb_core.Interface;
const Advance = @import("xhci_ring_cycle.zig").Advance;
const IncidentToken = extern struct { generation: u64 = 0 };
pub const EndpointKind = enum(u8) { control, interrupt_in, bulk_in, bulk_out };
pub const InterruptPollResult = enum(u8) { report, no_report, failed };
pub const TopologyHook = *const fn () callconv(.c) void;
pub const Trb = extern struct {
    parameter: u64 = 0,
    status: u32 = 0,
    control: u32 = 0,
};
comptime {
    if (@sizeOf(Trb) != 16 or @alignOf(Trb) != 8) @compileError("xHCI Trb C layout");
    if (@offsetOf(Trb, "parameter") != 0) @compileError("xHCI Trb.parameter C layout");
    if (@offsetOf(Trb, "status") != 8) @compileError("xHCI Trb.status C layout");
    if (@offsetOf(Trb, "control") != 12) @compileError("xHCI Trb.control C layout");
}
pub const ErstEntry = extern struct {
    base: u64 = 0,
    size: u32 = 0,
    reserved: u32 = 0,
};
comptime {
    if (@sizeOf(ErstEntry) != 16 or @alignOf(ErstEntry) != 8) @compileError("xHCI ErstEntry C layout");
    if (@offsetOf(ErstEntry, "base") != 0) @compileError("xHCI ErstEntry.base C layout");
    if (@offsetOf(ErstEntry, "size") != 8) @compileError("xHCI ErstEntry.size C layout");
    if (@offsetOf(ErstEntry, "reserved") != 12) @compileError("xHCI ErstEntry.reserved C layout");
}
pub const PortStatus = extern struct {
    index: u8 = 0,
    portsc: u32 = 0,
    portpmsc: u32 = 0,
    portli: u32 = 0,
    porthlpmc: u32 = 0,
    change_bits: u32 = 0,
    link_state: u8 = 0,
    connected: bool = false,
    enabled: bool = false,
    powered: bool = false,
    debounce_ok: bool = false,
    reset_attempted: bool = false,
    reset_ok: bool = false,
    reset_reason: Text = Text.init("not-started"),
};
comptime {
    if (@sizeOf(PortStatus) != 48 or @alignOf(PortStatus) != 8) @compileError("xHCI PortStatus C layout");
    if (@offsetOf(PortStatus, "index") != 0) @compileError("xHCI PortStatus.index C layout");
    if (@offsetOf(PortStatus, "portsc") != 4) @compileError("xHCI PortStatus.portsc C layout");
    if (@offsetOf(PortStatus, "portpmsc") != 8) @compileError("xHCI PortStatus.portpmsc C layout");
    if (@offsetOf(PortStatus, "portli") != 12) @compileError("xHCI PortStatus.portli C layout");
    if (@offsetOf(PortStatus, "porthlpmc") != 16) @compileError("xHCI PortStatus.porthlpmc C layout");
    if (@offsetOf(PortStatus, "change_bits") != 20) @compileError("xHCI PortStatus.change_bits C layout");
    if (@offsetOf(PortStatus, "link_state") != 24) @compileError("xHCI PortStatus.link_state C layout");
    if (@offsetOf(PortStatus, "connected") != 25) @compileError("xHCI PortStatus.connected C layout");
    if (@offsetOf(PortStatus, "enabled") != 26) @compileError("xHCI PortStatus.enabled C layout");
    if (@offsetOf(PortStatus, "powered") != 27) @compileError("xHCI PortStatus.powered C layout");
    if (@offsetOf(PortStatus, "debounce_ok") != 28) @compileError("xHCI PortStatus.debounce_ok C layout");
    if (@offsetOf(PortStatus, "reset_attempted") != 29) @compileError("xHCI PortStatus.reset_attempted C layout");
    if (@offsetOf(PortStatus, "reset_ok") != 30) @compileError("xHCI PortStatus.reset_ok C layout");
    if (@offsetOf(PortStatus, "reset_reason") != 32) @compileError("xHCI PortStatus.reset_reason C layout");
}
pub const ProbeRecord = extern struct {
    active: bool = false,
    port: u8 = 0,
    slot: u8 = 0,
    speed: u8 = 0,
    pls: u8 = 0,
    stage: Text = Text.init("none"),
    command_cc: u8 = 0,
    transfer_cc: u8 = 0,
    class_code: u8 = 0,
    subclass: u8 = 0,
    protocol: u8 = 0,
    vid: u16 = 0,
    pid: u16 = 0,
};
comptime {
    if (@sizeOf(ProbeRecord) != 40 or @alignOf(ProbeRecord) != 8) @compileError("xHCI ProbeRecord C layout");
    if (@offsetOf(ProbeRecord, "active") != 0) @compileError("xHCI ProbeRecord.active C layout");
    if (@offsetOf(ProbeRecord, "port") != 1) @compileError("xHCI ProbeRecord.port C layout");
    if (@offsetOf(ProbeRecord, "slot") != 2) @compileError("xHCI ProbeRecord.slot C layout");
    if (@offsetOf(ProbeRecord, "speed") != 3) @compileError("xHCI ProbeRecord.speed C layout");
    if (@offsetOf(ProbeRecord, "pls") != 4) @compileError("xHCI ProbeRecord.pls C layout");
    if (@offsetOf(ProbeRecord, "stage") != 8) @compileError("xHCI ProbeRecord.stage C layout");
    if (@offsetOf(ProbeRecord, "command_cc") != 24) @compileError("xHCI ProbeRecord.command_cc C layout");
    if (@offsetOf(ProbeRecord, "transfer_cc") != 25) @compileError("xHCI ProbeRecord.transfer_cc C layout");
    if (@offsetOf(ProbeRecord, "class_code") != 26) @compileError("xHCI ProbeRecord.class_code C layout");
    if (@offsetOf(ProbeRecord, "subclass") != 27) @compileError("xHCI ProbeRecord.subclass C layout");
    if (@offsetOf(ProbeRecord, "protocol") != 28) @compileError("xHCI ProbeRecord.protocol C layout");
    if (@offsetOf(ProbeRecord, "vid") != 30) @compileError("xHCI ProbeRecord.vid C layout");
    if (@offsetOf(ProbeRecord, "pid") != 32) @compileError("xHCI ProbeRecord.pid C layout");
}
pub const DescriptorInterface = extern struct {
    valid: bool = false,
    number: u8 = 0,
    class_code: u8 = 0,
    subclass: u8 = 0,
    protocol: u8 = 0,
    endpoint_count: u8 = 0,
    first_endpoint_address: u8 = 0,
    first_endpoint_attributes: u8 = 0,
    first_endpoint_max_packet: u16 = 0,
    first_endpoint_interval: u8 = 0,
    interrupt_in_endpoint_address: u8 = 0,
    interrupt_in_endpoint_max_packet: u16 = 0,
    interrupt_in_endpoint_interval: u8 = 0,
    bulk_in_endpoint_address: u8 = 0,
    bulk_in_endpoint_max_packet: u16 = 0,
    bulk_in_endpoint_max_burst: u8 = 0,
    bulk_out_endpoint_address: u8 = 0,
    bulk_out_endpoint_max_packet: u16 = 0,
    bulk_out_endpoint_max_burst: u8 = 0,
    last_endpoint_address: u8 = 0,
    hid_descriptor_len: u8 = 0,
    hid_report_descriptor_len: u16 = 0,
};
comptime {
    if (@sizeOf(DescriptorInterface) != 28 or @alignOf(DescriptorInterface) != 2) @compileError("xHCI DescriptorInterface C layout");
    if (@offsetOf(DescriptorInterface, "valid") != 0) @compileError("xHCI DescriptorInterface.valid C layout");
    if (@offsetOf(DescriptorInterface, "number") != 1) @compileError("xHCI DescriptorInterface.number C layout");
    if (@offsetOf(DescriptorInterface, "class_code") != 2) @compileError("xHCI DescriptorInterface.class_code C layout");
    if (@offsetOf(DescriptorInterface, "subclass") != 3) @compileError("xHCI DescriptorInterface.subclass C layout");
    if (@offsetOf(DescriptorInterface, "protocol") != 4) @compileError("xHCI DescriptorInterface.protocol C layout");
    if (@offsetOf(DescriptorInterface, "endpoint_count") != 5) @compileError("xHCI DescriptorInterface.endpoint_count C layout");
    if (@offsetOf(DescriptorInterface, "first_endpoint_address") != 6) @compileError("xHCI DescriptorInterface.first_endpoint_address C layout");
    if (@offsetOf(DescriptorInterface, "first_endpoint_attributes") != 7) @compileError("xHCI DescriptorInterface.first_endpoint_attributes C layout");
    if (@offsetOf(DescriptorInterface, "first_endpoint_max_packet") != 8) @compileError("xHCI DescriptorInterface.first_endpoint_max_packet C layout");
    if (@offsetOf(DescriptorInterface, "first_endpoint_interval") != 10) @compileError("xHCI DescriptorInterface.first_endpoint_interval C layout");
    if (@offsetOf(DescriptorInterface, "interrupt_in_endpoint_address") != 11) @compileError("xHCI DescriptorInterface.interrupt_in_endpoint_address C layout");
    if (@offsetOf(DescriptorInterface, "interrupt_in_endpoint_max_packet") != 12) @compileError("xHCI DescriptorInterface.interrupt_in_endpoint_max_packet C layout");
    if (@offsetOf(DescriptorInterface, "interrupt_in_endpoint_interval") != 14) @compileError("xHCI DescriptorInterface.interrupt_in_endpoint_interval C layout");
    if (@offsetOf(DescriptorInterface, "bulk_in_endpoint_address") != 15) @compileError("xHCI DescriptorInterface.bulk_in_endpoint_address C layout");
    if (@offsetOf(DescriptorInterface, "bulk_in_endpoint_max_packet") != 16) @compileError("xHCI DescriptorInterface.bulk_in_endpoint_max_packet C layout");
    if (@offsetOf(DescriptorInterface, "bulk_in_endpoint_max_burst") != 18) @compileError("xHCI DescriptorInterface.bulk_in_endpoint_max_burst C layout");
    if (@offsetOf(DescriptorInterface, "bulk_out_endpoint_address") != 19) @compileError("xHCI DescriptorInterface.bulk_out_endpoint_address C layout");
    if (@offsetOf(DescriptorInterface, "bulk_out_endpoint_max_packet") != 20) @compileError("xHCI DescriptorInterface.bulk_out_endpoint_max_packet C layout");
    if (@offsetOf(DescriptorInterface, "bulk_out_endpoint_max_burst") != 22) @compileError("xHCI DescriptorInterface.bulk_out_endpoint_max_burst C layout");
    if (@offsetOf(DescriptorInterface, "last_endpoint_address") != 23) @compileError("xHCI DescriptorInterface.last_endpoint_address C layout");
    if (@offsetOf(DescriptorInterface, "hid_descriptor_len") != 24) @compileError("xHCI DescriptorInterface.hid_descriptor_len C layout");
    if (@offsetOf(DescriptorInterface, "hid_report_descriptor_len") != 26) @compileError("xHCI DescriptorInterface.hid_report_descriptor_len C layout");
}
pub const ControlRequest = extern struct {
    request_type: u8 = 0,
    request: u8 = 0,
    value: u16 = 0,
    index_value: u16 = 0,
    length: u16 = 0,
    data_phys: u64 = 0,
    direction: u8 = 0,
};
comptime {
    if (@sizeOf(ControlRequest) != 24 or @alignOf(ControlRequest) != 8) @compileError("xHCI ControlRequest C layout");
    if (@offsetOf(ControlRequest, "request_type") != 0) @compileError("xHCI ControlRequest.request_type C layout");
    if (@offsetOf(ControlRequest, "request") != 1) @compileError("xHCI ControlRequest.request C layout");
    if (@offsetOf(ControlRequest, "value") != 2) @compileError("xHCI ControlRequest.value C layout");
    if (@offsetOf(ControlRequest, "index_value") != 4) @compileError("xHCI ControlRequest.index_value C layout");
    if (@offsetOf(ControlRequest, "length") != 6) @compileError("xHCI ControlRequest.length C layout");
    if (@offsetOf(ControlRequest, "data_phys") != 8) @compileError("xHCI ControlRequest.data_phys C layout");
    if (@offsetOf(ControlRequest, "direction") != 16) @compileError("xHCI ControlRequest.direction C layout");
}
pub const DeviceRuntime = extern struct {
    active: bool = false,
    generation: u64 = 0,
    transaction_active: bool = false,
    transaction_task_id: u32 = 0,
    transaction_task_generation: u64 = 0,
    detach_pending: bool = false,
    slot_id: u8 = 0,
    port: u8 = 0,
    speed: u8 = 0,
    config_value: u8 = 0,
    set_configuration_attempted: bool = false,
    set_configuration_ok: bool = false,
    vendor_id: u16 = 0,
    product_id: u16 = 0,
    device_context_phys: u64 = 0,
    input_context_phys: u64 = 0,
    ep0_ring_phys: u64 = 0,
    interrupt_ring_phys: u64 = 0,
    interrupt_buffer_phys: u64 = 0,
    bulk_in_ring_phys: u64 = 0,
    bulk_out_ring_phys: u64 = 0,
    bulk_buffer_phys: u64 = 0,
    bulk_buffer_frames: u16 = 0,
    descriptor_phys: u64 = 0,
    device_virt: u64 = 0,
    input_virt: u64 = 0,
    ep0_ring_virt: u64 = 0,
    interrupt_ring_virt: u64 = 0,
    interrupt_buffer_virt: u64 = 0,
    bulk_in_ring_virt: u64 = 0,
    bulk_out_ring_virt: u64 = 0,
    bulk_buffer_virt: u64 = 0,
    descriptor_virt: u64 = 0,
    ep0_enqueue: u16 = 0,
    ep0_cycle: u8 = 1,
    control_endpoint_faulted: bool = false,
    interrupt_enqueue: u16 = 0,
    interrupt_cycle: u8 = 1,
    interrupt_endpoint_id: u8 = 0,
    interrupt_endpoint_address: u8 = 0,
    interrupt_endpoint_max_packet: u16 = 0,
    interrupt_endpoint_interval_raw: u8 = 0,
    interrupt_endpoint_interval_context: u8 = 0,
    interrupt_endpoint_configured: bool = false,
    interrupt_endpoint_faulted: bool = false,
    interrupt_pending: bool = false,
    interrupt_pending_trb_phys: u64 = 0,
    interrupt_transfer_handle: u32 = 0,
    interrupt_pending_streak: u64 = 0,
    last_interrupt_ep_state: u8 = 0,
    last_interrupt_request_len: u32 = 0,
    last_interrupt_residue: u32 = 0,
    last_interrupt_actual_len: u32 = 0,
    last_interrupt_completion_code: u8 = 0,
    bulk_endpoints_configured: bool = false,
    bulk_endpoints_faulted: bool = false,
    bulk_in_endpoint_id: u8 = 0,
    bulk_in_endpoint_address: u8 = 0,
    bulk_in_endpoint_max_packet: u16 = 0,
    bulk_in_endpoint_max_burst: u8 = 0,
    bulk_out_endpoint_id: u8 = 0,
    bulk_out_endpoint_address: u8 = 0,
    bulk_out_endpoint_max_packet: u16 = 0,
    bulk_out_endpoint_max_burst: u8 = 0,
    bulk_in_enqueue: u16 = 0,
    bulk_in_cycle: u8 = 1,
    bulk_in_link_update_pending: bool = false,
    bulk_out_enqueue: u16 = 0,
    bulk_out_cycle: u8 = 1,
    bulk_out_link_update_pending: bool = false,
    last_control_request_type: u8 = 0,
    last_control_request: u8 = 0,
    last_control_value: u16 = 0,
    last_control_index: u16 = 0,
    last_control_length: u16 = 0,
    last_control_direction: Text = Text.init("none"),
    last_control_completion_code: u8 = 0,
    last_control_residue: u32 = 0,
    last_control_ok: bool = false,
    last_bulk_direction: Text = Text.init("none"),
    last_bulk_result: Text = Text.init("none"),
    last_bulk_request_len: u32 = 0,
    last_bulk_residue: u32 = 0,
    last_bulk_actual_len: u32 = 0,
    last_bulk_completion_code: u8 = 0,
    last_wait_cancelled: bool = false,
    sync_transfer_incident: IncidentToken = .{},
};
comptime {
    if (@sizeOf(DeviceRuntime) != 384 or @alignOf(DeviceRuntime) != 8) @compileError("xHCI DeviceRuntime C layout");
    if (@offsetOf(DeviceRuntime, "active") != 0) @compileError("xHCI DeviceRuntime.active C layout");
    if (@offsetOf(DeviceRuntime, "generation") != 8) @compileError("xHCI DeviceRuntime.generation C layout");
    if (@offsetOf(DeviceRuntime, "transaction_active") != 16) @compileError("xHCI DeviceRuntime.transaction_active C layout");
    if (@offsetOf(DeviceRuntime, "transaction_task_id") != 20) @compileError("xHCI DeviceRuntime.transaction_task_id C layout");
    if (@offsetOf(DeviceRuntime, "transaction_task_generation") != 24) @compileError("xHCI DeviceRuntime.transaction_task_generation C layout");
    if (@offsetOf(DeviceRuntime, "detach_pending") != 32) @compileError("xHCI DeviceRuntime.detach_pending C layout");
    if (@offsetOf(DeviceRuntime, "slot_id") != 33) @compileError("xHCI DeviceRuntime.slot_id C layout");
    if (@offsetOf(DeviceRuntime, "port") != 34) @compileError("xHCI DeviceRuntime.port C layout");
    if (@offsetOf(DeviceRuntime, "speed") != 35) @compileError("xHCI DeviceRuntime.speed C layout");
    if (@offsetOf(DeviceRuntime, "config_value") != 36) @compileError("xHCI DeviceRuntime.config_value C layout");
    if (@offsetOf(DeviceRuntime, "set_configuration_attempted") != 37) @compileError("xHCI DeviceRuntime.set_configuration_attempted C layout");
    if (@offsetOf(DeviceRuntime, "set_configuration_ok") != 38) @compileError("xHCI DeviceRuntime.set_configuration_ok C layout");
    if (@offsetOf(DeviceRuntime, "vendor_id") != 40) @compileError("xHCI DeviceRuntime.vendor_id C layout");
    if (@offsetOf(DeviceRuntime, "product_id") != 42) @compileError("xHCI DeviceRuntime.product_id C layout");
    if (@offsetOf(DeviceRuntime, "device_context_phys") != 48) @compileError("xHCI DeviceRuntime.device_context_phys C layout");
    if (@offsetOf(DeviceRuntime, "input_context_phys") != 56) @compileError("xHCI DeviceRuntime.input_context_phys C layout");
    if (@offsetOf(DeviceRuntime, "ep0_ring_phys") != 64) @compileError("xHCI DeviceRuntime.ep0_ring_phys C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_ring_phys") != 72) @compileError("xHCI DeviceRuntime.interrupt_ring_phys C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_buffer_phys") != 80) @compileError("xHCI DeviceRuntime.interrupt_buffer_phys C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_ring_phys") != 88) @compileError("xHCI DeviceRuntime.bulk_in_ring_phys C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_ring_phys") != 96) @compileError("xHCI DeviceRuntime.bulk_out_ring_phys C layout");
    if (@offsetOf(DeviceRuntime, "bulk_buffer_phys") != 104) @compileError("xHCI DeviceRuntime.bulk_buffer_phys C layout");
    if (@offsetOf(DeviceRuntime, "bulk_buffer_frames") != 112) @compileError("xHCI DeviceRuntime.bulk_buffer_frames C layout");
    if (@offsetOf(DeviceRuntime, "descriptor_phys") != 120) @compileError("xHCI DeviceRuntime.descriptor_phys C layout");
    if (@offsetOf(DeviceRuntime, "device_virt") != 128) @compileError("xHCI DeviceRuntime.device_virt C layout");
    if (@offsetOf(DeviceRuntime, "input_virt") != 136) @compileError("xHCI DeviceRuntime.input_virt C layout");
    if (@offsetOf(DeviceRuntime, "ep0_ring_virt") != 144) @compileError("xHCI DeviceRuntime.ep0_ring_virt C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_ring_virt") != 152) @compileError("xHCI DeviceRuntime.interrupt_ring_virt C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_buffer_virt") != 160) @compileError("xHCI DeviceRuntime.interrupt_buffer_virt C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_ring_virt") != 168) @compileError("xHCI DeviceRuntime.bulk_in_ring_virt C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_ring_virt") != 176) @compileError("xHCI DeviceRuntime.bulk_out_ring_virt C layout");
    if (@offsetOf(DeviceRuntime, "bulk_buffer_virt") != 184) @compileError("xHCI DeviceRuntime.bulk_buffer_virt C layout");
    if (@offsetOf(DeviceRuntime, "descriptor_virt") != 192) @compileError("xHCI DeviceRuntime.descriptor_virt C layout");
    if (@offsetOf(DeviceRuntime, "ep0_enqueue") != 200) @compileError("xHCI DeviceRuntime.ep0_enqueue C layout");
    if (@offsetOf(DeviceRuntime, "ep0_cycle") != 202) @compileError("xHCI DeviceRuntime.ep0_cycle C layout");
    if (@offsetOf(DeviceRuntime, "control_endpoint_faulted") != 203) @compileError("xHCI DeviceRuntime.control_endpoint_faulted C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_enqueue") != 204) @compileError("xHCI DeviceRuntime.interrupt_enqueue C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_cycle") != 206) @compileError("xHCI DeviceRuntime.interrupt_cycle C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_endpoint_id") != 207) @compileError("xHCI DeviceRuntime.interrupt_endpoint_id C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_endpoint_address") != 208) @compileError("xHCI DeviceRuntime.interrupt_endpoint_address C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_endpoint_max_packet") != 210) @compileError("xHCI DeviceRuntime.interrupt_endpoint_max_packet C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_endpoint_interval_raw") != 212) @compileError("xHCI DeviceRuntime.interrupt_endpoint_interval_raw C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_endpoint_interval_context") != 213) @compileError("xHCI DeviceRuntime.interrupt_endpoint_interval_context C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_endpoint_configured") != 214) @compileError("xHCI DeviceRuntime.interrupt_endpoint_configured C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_endpoint_faulted") != 215) @compileError("xHCI DeviceRuntime.interrupt_endpoint_faulted C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_pending") != 216) @compileError("xHCI DeviceRuntime.interrupt_pending C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_pending_trb_phys") != 224) @compileError("xHCI DeviceRuntime.interrupt_pending_trb_phys C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_transfer_handle") != 232) @compileError("xHCI DeviceRuntime.interrupt_transfer_handle C layout");
    if (@offsetOf(DeviceRuntime, "interrupt_pending_streak") != 240) @compileError("xHCI DeviceRuntime.interrupt_pending_streak C layout");
    if (@offsetOf(DeviceRuntime, "last_interrupt_ep_state") != 248) @compileError("xHCI DeviceRuntime.last_interrupt_ep_state C layout");
    if (@offsetOf(DeviceRuntime, "last_interrupt_request_len") != 252) @compileError("xHCI DeviceRuntime.last_interrupt_request_len C layout");
    if (@offsetOf(DeviceRuntime, "last_interrupt_residue") != 256) @compileError("xHCI DeviceRuntime.last_interrupt_residue C layout");
    if (@offsetOf(DeviceRuntime, "last_interrupt_actual_len") != 260) @compileError("xHCI DeviceRuntime.last_interrupt_actual_len C layout");
    if (@offsetOf(DeviceRuntime, "last_interrupt_completion_code") != 264) @compileError("xHCI DeviceRuntime.last_interrupt_completion_code C layout");
    if (@offsetOf(DeviceRuntime, "bulk_endpoints_configured") != 265) @compileError("xHCI DeviceRuntime.bulk_endpoints_configured C layout");
    if (@offsetOf(DeviceRuntime, "bulk_endpoints_faulted") != 266) @compileError("xHCI DeviceRuntime.bulk_endpoints_faulted C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_endpoint_id") != 267) @compileError("xHCI DeviceRuntime.bulk_in_endpoint_id C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_endpoint_address") != 268) @compileError("xHCI DeviceRuntime.bulk_in_endpoint_address C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_endpoint_max_packet") != 270) @compileError("xHCI DeviceRuntime.bulk_in_endpoint_max_packet C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_endpoint_max_burst") != 272) @compileError("xHCI DeviceRuntime.bulk_in_endpoint_max_burst C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_endpoint_id") != 273) @compileError("xHCI DeviceRuntime.bulk_out_endpoint_id C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_endpoint_address") != 274) @compileError("xHCI DeviceRuntime.bulk_out_endpoint_address C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_endpoint_max_packet") != 276) @compileError("xHCI DeviceRuntime.bulk_out_endpoint_max_packet C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_endpoint_max_burst") != 278) @compileError("xHCI DeviceRuntime.bulk_out_endpoint_max_burst C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_enqueue") != 280) @compileError("xHCI DeviceRuntime.bulk_in_enqueue C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_cycle") != 282) @compileError("xHCI DeviceRuntime.bulk_in_cycle C layout");
    if (@offsetOf(DeviceRuntime, "bulk_in_link_update_pending") != 283) @compileError("xHCI DeviceRuntime.bulk_in_link_update_pending C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_enqueue") != 284) @compileError("xHCI DeviceRuntime.bulk_out_enqueue C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_cycle") != 286) @compileError("xHCI DeviceRuntime.bulk_out_cycle C layout");
    if (@offsetOf(DeviceRuntime, "bulk_out_link_update_pending") != 287) @compileError("xHCI DeviceRuntime.bulk_out_link_update_pending C layout");
    if (@offsetOf(DeviceRuntime, "last_control_request_type") != 288) @compileError("xHCI DeviceRuntime.last_control_request_type C layout");
    if (@offsetOf(DeviceRuntime, "last_control_request") != 289) @compileError("xHCI DeviceRuntime.last_control_request C layout");
    if (@offsetOf(DeviceRuntime, "last_control_value") != 290) @compileError("xHCI DeviceRuntime.last_control_value C layout");
    if (@offsetOf(DeviceRuntime, "last_control_index") != 292) @compileError("xHCI DeviceRuntime.last_control_index C layout");
    if (@offsetOf(DeviceRuntime, "last_control_length") != 294) @compileError("xHCI DeviceRuntime.last_control_length C layout");
    if (@offsetOf(DeviceRuntime, "last_control_direction") != 296) @compileError("xHCI DeviceRuntime.last_control_direction C layout");
    if (@offsetOf(DeviceRuntime, "last_control_completion_code") != 312) @compileError("xHCI DeviceRuntime.last_control_completion_code C layout");
    if (@offsetOf(DeviceRuntime, "last_control_residue") != 316) @compileError("xHCI DeviceRuntime.last_control_residue C layout");
    if (@offsetOf(DeviceRuntime, "last_control_ok") != 320) @compileError("xHCI DeviceRuntime.last_control_ok C layout");
    if (@offsetOf(DeviceRuntime, "last_bulk_direction") != 328) @compileError("xHCI DeviceRuntime.last_bulk_direction C layout");
    if (@offsetOf(DeviceRuntime, "last_bulk_result") != 344) @compileError("xHCI DeviceRuntime.last_bulk_result C layout");
    if (@offsetOf(DeviceRuntime, "last_bulk_request_len") != 360) @compileError("xHCI DeviceRuntime.last_bulk_request_len C layout");
    if (@offsetOf(DeviceRuntime, "last_bulk_residue") != 364) @compileError("xHCI DeviceRuntime.last_bulk_residue C layout");
    if (@offsetOf(DeviceRuntime, "last_bulk_actual_len") != 368) @compileError("xHCI DeviceRuntime.last_bulk_actual_len C layout");
    if (@offsetOf(DeviceRuntime, "last_bulk_completion_code") != 372) @compileError("xHCI DeviceRuntime.last_bulk_completion_code C layout");
    if (@offsetOf(DeviceRuntime, "last_wait_cancelled") != 373) @compileError("xHCI DeviceRuntime.last_wait_cancelled C layout");
    if (@offsetOf(DeviceRuntime, "sync_transfer_incident") != 376) @compileError("xHCI DeviceRuntime.sync_transfer_incident C layout");
}
pub const DeviceHandle = extern struct {
    generation: u64 = 0,
    controller: Text = Text.init("xhci"),
    port: u8 = 0,
    slot_id: u8 = 0,
    speed: u8 = 0,
    config_value: u8 = 0,
    vendor_id: u16 = 0,
    product_id: u16 = 0,
};
comptime {
    if (@sizeOf(DeviceHandle) != 32 or @alignOf(DeviceHandle) != 8) @compileError("xHCI DeviceHandle C layout");
    if (@offsetOf(DeviceHandle, "generation") != 0) @compileError("xHCI DeviceHandle.generation C layout");
    if (@offsetOf(DeviceHandle, "controller") != 8) @compileError("xHCI DeviceHandle.controller C layout");
    if (@offsetOf(DeviceHandle, "port") != 24) @compileError("xHCI DeviceHandle.port C layout");
    if (@offsetOf(DeviceHandle, "slot_id") != 25) @compileError("xHCI DeviceHandle.slot_id C layout");
    if (@offsetOf(DeviceHandle, "speed") != 26) @compileError("xHCI DeviceHandle.speed C layout");
    if (@offsetOf(DeviceHandle, "config_value") != 27) @compileError("xHCI DeviceHandle.config_value C layout");
    if (@offsetOf(DeviceHandle, "vendor_id") != 28) @compileError("xHCI DeviceHandle.vendor_id C layout");
    if (@offsetOf(DeviceHandle, "product_id") != 30) @compileError("xHCI DeviceHandle.product_id C layout");
}
pub const EndpointHandle = extern struct {
    device: DeviceHandle = .{},
    kind: EndpointKind = .control,
    address: u8 = 0,
    endpoint_id: u8 = 0,
    max_packet: u16 = 0,
    max_burst: u8 = 0,
    interval: u8 = 0,
};
comptime {
    if (@sizeOf(EndpointHandle) != 40 or @alignOf(EndpointHandle) != 8) @compileError("xHCI EndpointHandle C layout");
    if (@offsetOf(EndpointHandle, "device") != 0) @compileError("xHCI EndpointHandle.device C layout");
    if (@offsetOf(EndpointHandle, "kind") != 32) @compileError("xHCI EndpointHandle.kind C layout");
    if (@offsetOf(EndpointHandle, "address") != 33) @compileError("xHCI EndpointHandle.address C layout");
    if (@offsetOf(EndpointHandle, "endpoint_id") != 34) @compileError("xHCI EndpointHandle.endpoint_id C layout");
    if (@offsetOf(EndpointHandle, "max_packet") != 36) @compileError("xHCI EndpointHandle.max_packet C layout");
    if (@offsetOf(EndpointHandle, "max_burst") != 38) @compileError("xHCI EndpointHandle.max_burst C layout");
    if (@offsetOf(EndpointHandle, "interval") != 39) @compileError("xHCI EndpointHandle.interval C layout");
}
pub const DeviceTransaction = extern struct {
    runtime_index: usize = 0,
    generation: u64 = 0,
};
comptime {
    if (@sizeOf(DeviceTransaction) != 16 or @alignOf(DeviceTransaction) != 8) @compileError("xHCI DeviceTransaction C layout");
    if (@offsetOf(DeviceTransaction, "runtime_index") != 0) @compileError("xHCI DeviceTransaction.runtime_index C layout");
    if (@offsetOf(DeviceTransaction, "generation") != 8) @compileError("xHCI DeviceTransaction.generation C layout");
}
pub const Status = extern struct {
    probed: bool = false,
    present: bool = false,
    mapped: bool = false,
    device: PciDevice = .{},
    command_before: u16 = 0,
    command_after: u16 = 0,
    bar0_raw: u32 = 0,
    bar1_raw: u32 = 0,
    bar_is_io: bool = false,
    bar_is_64: bool = false,
    mmio_phys: u64 = 0,
    mmio_virt: u64 = 0,
    caplength: u8 = 0,
    hciversion: u16 = 0,
    hcsparams1: u32 = 0,
    hcsparams2: u32 = 0,
    hcsparams3: u32 = 0,
    hccparams1: u32 = 0,
    hccparams2: u32 = 0,
    dboff: u32 = 0,
    rtsoff: u32 = 0,
    op_virt: u64 = 0,
    runtime_virt: u64 = 0,
    max_slots: u8 = 0,
    max_interrupters: u16 = 0,
    max_ports: u8 = 0,
    usbcmd: u32 = 0,
    usbsts: u32 = 0,
    pagesize: u32 = 0,
    dnctrl: u32 = 0,
    config: u32 = 0,
    mfindex: u32 = 0,
    iman0: u32 = 0,
    imod0: u32 = 0,
    erstsz0: u32 = 0,
    erstba0: u64 = 0,
    erdp0: u64 = 0,
    crcr: u64 = 0,
    dcbaap: u64 = 0,
    port_count_seen: u8 = 0,
    first_ports: [16]PortStatus = .{PortStatus{}} ** 16,
    legacy_cap_offset: u32 = 0,
    legacy_usblegsup_before: u32 = 0,
    legacy_usblegsup_after: u32 = 0,
    bios_handoff: Text = Text.init("not-started"),
    controller_halted_before: bool = false,
    controller_stopped: bool = false,
    controller_reset_ok: bool = false,
    controller_running: bool = false,
    dma_ready: bool = false,
    context_size: u8 = 32,
    dcbaa_phys: u64 = 0,
    command_ring_phys: u64 = 0,
    event_ring_phys: u64 = 0,
    erst_phys: u64 = 0,
    scratchpad_array_phys: u64 = 0,
    scratchpad_count: u8 = 0,
    max_slots_enabled: u8 = 0,
    command_enqueue: u16 = 0,
    command_cycle: u8 = 1,
    event_dequeue: u16 = 0,
    event_cycle: u8 = 1,
    event_raw_cycle: u8 = 0,
    event_raw_type: u8 = 0,
    event_raw_code: u8 = 0,
    event_erdp_commits: u64 = 0,
    event_max_batch: u16 = 0,
    command_ring_wraps: u64 = 0,
    event_ring_wraps: u64 = 0,
    ep0_ring_wraps: u64 = 0,
    interrupt_ring_wraps: u64 = 0,
    bulk_in_ring_wraps: u64 = 0,
    bulk_out_ring_wraps: u64 = 0,
    commands: u64 = 0,
    events: u64 = 0,
    event_tick_deadline_waits: u64 = 0,
    event_hpet_deadline_waits: u64 = 0,
    event_tsc_deadline_waits: u64 = 0,
    event_guard_waits: u64 = 0,
    event_tick_timeouts: u64 = 0,
    event_hpet_timeouts: u64 = 0,
    event_guard_timeouts: u64 = 0,
    event_tsc_timeouts: u64 = 0,
    event_recovery_budget_timeouts: u64 = 0,
    event_cpu_guard_timeouts: u64 = 0,
    host_controller_id: u32 = 0,
    owner_id: u32 = 0,
    host_source: u32 = 0,
    irq_line: u8 = 0xFF,
    irq_pin: u8 = 0,
    irq_registered: bool = false,
    irq_mode: Text = Text.init("poll"),
    irq_count: u64 = 0,
    irq_handled: u64 = 0,
    irq_wakeups: u64 = 0,
    poll_fallbacks: u64 = 0,
    wait_calls: u64 = 0,
    wait_successes: u64 = 0,
    wait_timeouts: u64 = 0,
    wait_early_calls: u64 = 0,
    wait_runtime_calls: u64 = 0,
    wait_requested_ms: u64 = 0,
    wait_elapsed_ns: u64 = 0,
    wait_max_elapsed_ns: u64 = 0,
    wait_iterations: u64 = 0,
    wait_blocked_ticks: u64 = 0,
    last_wait_reason: Text = Text.init("none"),
    last_wait_phase: u8 = 0,
    last_wait_requested_ms: u32 = 0,
    last_wait_elapsed_ns: u64 = 0,
    last_wait_iterations: u64 = 0,
    last_wait_blocked_ticks: u64 = 0,
    last_wait_retry: u8 = 0,
    last_wait_success: bool = false,
    stale_events: u64 = 0,
    ring_full: u64 = 0,
    deferred_events_pending: usize = 0,
    deferred_events_queued: u64 = 0,
    deferred_events_delivered: u64 = 0,
    deferred_events_overflows: u64 = 0,
    deferred_events_purged: u64 = 0,
    deferred_events_high_water: usize = 0,
    retained_slots: u8 = 0,
    runtime_switches: u64 = 0,
    port_change_clears: u64 = 0,
    port_change_events: u64 = 0,
    port_change_queued: u64 = 0,
    port_change_coalesced: u64 = 0,
    port_change_invalid: u64 = 0,
    port_change_taken: u64 = 0,
    port_change_retries: u64 = 0,
    port_change_pending_mask: u16 = 0,
    port_change_pending: u8 = 0,
    port_change_high_water: u8 = 0,
    port_service_calls: u64 = 0,
    port_service_failures: u64 = 0,
    port_catalog_changes: u64 = 0,
    port_task_started: bool = false,
    port_task_id: u32 = 0,
    port_task_iterations: u64 = 0,
    port_debounce_failures: u64 = 0,
    port_power_requests: u64 = 0,
    port_u0_waits: u64 = 0,
    port_warm_resets: u64 = 0,
    port_disconnects: u64 = 0,
    reclaimed_slots: u64 = 0,
    disable_slot_commands: u64 = 0,
    disable_slot_failures: u64 = 0,
    enable_slot_attempted: bool = false,
    enable_slot_ok: bool = false,
    address_device_attempted: bool = false,
    address_device_ok: bool = false,
    get_descriptor_attempted: bool = false,
    get_descriptor_ok: bool = false,
    get_config_attempted: bool = false,
    get_config_ok: bool = false,
    get_strings_attempted: bool = false,
    get_strings_ok: bool = false,
    last_command_type: u8 = 0,
    last_completion_code: u8 = 0,
    last_transfer_completion_code: u8 = 0,
    last_event_type: u8 = 0,
    last_event_code: u8 = 0,
    last_event_slot: u8 = 0,
    last_event_endpoint: u8 = 0,
    last_event_parameter: u64 = 0,
    last_event_length: u32 = 0,
    last_stale_event_type: u8 = 0,
    last_stale_event_code: u8 = 0,
    last_stale_event_slot: u8 = 0,
    last_stale_event_endpoint: u8 = 0,
    last_slot_id: u8 = 0,
    addressed_slot_id: u8 = 0,
    addressed_port: u8 = 0,
    addressed_speed: u8 = 0,
    device_context_phys: u64 = 0,
    input_context_phys: u64 = 0,
    ep0_ring_phys: u64 = 0,
    interrupt_ring_phys: u64 = 0,
    interrupt_buffer_phys: u64 = 0,
    bulk_in_ring_phys: u64 = 0,
    bulk_out_ring_phys: u64 = 0,
    bulk_buffer_phys: u64 = 0,
    bulk_buffer_frames: u16 = 0,
    descriptor_phys: u64 = 0,
    ep0_enqueue: u16 = 0,
    ep0_cycle: u8 = 1,
    control_endpoint_faulted: bool = false,
    interrupt_enqueue: u16 = 0,
    interrupt_cycle: u8 = 1,
    interrupt_link_cycle: u8 = 0,
    interrupt_endpoint_id: u8 = 0,
    interrupt_endpoint_address: u8 = 0,
    interrupt_endpoint_max_packet: u16 = 0,
    interrupt_endpoint_interval_raw: u8 = 0,
    interrupt_endpoint_interval_context: u8 = 0,
    interrupt_endpoint_configured: bool = false,
    interrupt_endpoint_faulted: bool = false,
    interrupt_pending: bool = false,
    interrupt_pending_trb_phys: u64 = 0,
    interrupt_transfer_handle: u32 = 0,
    interrupt_pending_index: u16 = 0,
    interrupt_hw_dequeue: u64 = 0,
    bulk_endpoints_configured: bool = false,
    bulk_endpoints_faulted: bool = false,
    bulk_in_endpoint_id: u8 = 0,
    bulk_out_endpoint_id: u8 = 0,
    bulk_in_enqueue: u16 = 0,
    bulk_in_cycle: u8 = 1,
    bulk_in_link_update_pending: bool = false,
    bulk_out_enqueue: u16 = 0,
    bulk_out_cycle: u8 = 1,
    bulk_out_link_update_pending: bool = false,
    set_configuration_attempted: bool = false,
    set_configuration_ok: bool = false,
    hid_set_protocol_attempted: bool = false,
    hid_set_protocol_ok: bool = false,
    hid_set_idle_attempted: bool = false,
    hid_set_idle_ok: bool = false,
    interrupt_polls: u64 = 0,
    interrupt_reports: u64 = 0,
    interrupt_no_report: u64 = 0,
    interrupt_recoveries: u64 = 0,
    interrupt_pending_timeouts: u64 = 0,
    interrupt_halted_checks: u64 = 0,
    interrupt_pending_streak: u64 = 0,
    last_interrupt_ep_state: u8 = 0,
    last_interrupt_request_len: u32 = 0,
    last_interrupt_residue: u32 = 0,
    last_interrupt_actual_len: u32 = 0,
    last_interrupt_completion_code: u8 = 0,
    control_transfers: u64 = 0,
    control_failures: u64 = 0,
    control_timeouts: u64 = 0,
    control_stalls: u64 = 0,
    control_short_packets: u64 = 0,
    last_control_request_type: u8 = 0,
    last_control_request: u8 = 0,
    last_control_value: u16 = 0,
    last_control_index: u16 = 0,
    last_control_length: u16 = 0,
    last_control_direction: Text = Text.init("none"),
    last_control_completion_code: u8 = 0,
    last_control_residue: u32 = 0,
    last_control_ok: bool = false,
    transfer_events: u64 = 0,
    bulk_transfers: u64 = 0,
    bulk_failures: u64 = 0,
    transfer_objects_active: u32 = 0,
    transfer_objects_high_water: u32 = 0,
    transfer_objects_submitted: u64 = 0,
    transfer_objects_completed: u64 = 0,
    transfer_objects_timed_out: u64 = 0,
    transfer_objects_cancelled: u64 = 0,
    transfer_objects_failed: u64 = 0,
    last_bulk_direction: Text = Text.init("none"),
    last_bulk_result: Text = Text.init("none"),
    last_bulk_request_len: u32 = 0,
    last_bulk_residue: u32 = 0,
    last_bulk_actual_len: u32 = 0,
    last_bulk_completion_code: u8 = 0,
    ports_probed: u8 = 0,
    non_hid_devices_skipped: u8 = 0,
    probe_record_count: u8 = 0,
    probe_records: [16]ProbeRecord = .{ProbeRecord{}} ** 16,
    selected_mass_storage: bool = false,
    selected_hid_input: bool = false,
    descriptor_len: u8 = 0,
    descriptor_type: u8 = 0,
    usb_version_bcd: u16 = 0,
    device_class: u8 = 0,
    device_subclass: u8 = 0,
    device_protocol: u8 = 0,
    device_max_packet0: u8 = 0,
    device_vendor_id: u16 = 0,
    device_product_id: u16 = 0,
    device_version_bcd: u16 = 0,
    manufacturer_index: u8 = 0,
    product_index: u8 = 0,
    serial_index: u8 = 0,
    config_total_length: u16 = 0,
    config_value: u8 = 0,
    config_attributes: u8 = 0,
    config_max_power_ma: u16 = 0,
    interface_count: u8 = 0,
    interface_record_count: u8 = 0,
    interface_records: [8]Interface = .{Interface{}} ** 8,
    endpoint_count: u8 = 0,
    descriptor_records: u8 = 0,
    descriptor_unknown: u8 = 0,
    descriptor_malformed: u8 = 0,
    hid_descriptor_count: u8 = 0,
    ss_endpoint_companion_count: u8 = 0,
    selected_interface_reason: Text = Text.init("none"),
    first_interface_number: u8 = 0,
    first_interface_class: u8 = 0,
    first_interface_subclass: u8 = 0,
    first_interface_protocol: u8 = 0,
    first_endpoint_address: u8 = 0,
    first_endpoint_attributes: u8 = 0,
    first_endpoint_max_packet: u16 = 0,
    first_endpoint_interval: u8 = 0,
    first_hid_descriptor_len: u8 = 0,
    first_hid_report_descriptor_len: u16 = 0,
    bulk_in_endpoint_address: u8 = 0,
    bulk_in_endpoint_max_packet: u16 = 0,
    bulk_in_endpoint_max_burst: u8 = 0,
    bulk_out_endpoint_address: u8 = 0,
    bulk_out_endpoint_max_packet: u16 = 0,
    bulk_out_endpoint_max_burst: u8 = 0,
    string_language_id: u16 = 0,
    manufacturer_len: u8 = 0,
    product_len: u8 = 0,
    serial_len: u8 = 0,
    manufacturer_string: [32]u8 = .{0} ** 32,
    product_string: [32]u8 = .{0} ** 32,
    serial_string: [32]u8 = .{0} ** 32,
    connected_ports: u8 = 0,
    enabled_ports: u8 = 0,
    reset_ports: u8 = 0,
    failures: u64 = 0,
    timeouts: u64 = 0,
    reason: Text = Text.init("not initialized"),
    last_wait_cancelled: bool = false,
};
comptime {
    if (@sizeOf(Status) != 3176 or @alignOf(Status) != 8) @compileError("xHCI Status C layout");
    if (@offsetOf(Status, "probed") != 0) @compileError("xHCI Status.probed C layout");
    if (@offsetOf(Status, "present") != 1) @compileError("xHCI Status.present C layout");
    if (@offsetOf(Status, "mapped") != 2) @compileError("xHCI Status.mapped C layout");
    if (@offsetOf(Status, "device") != 4) @compileError("xHCI Status.device C layout");
    if (@offsetOf(Status, "command_before") != 16) @compileError("xHCI Status.command_before C layout");
    if (@offsetOf(Status, "command_after") != 18) @compileError("xHCI Status.command_after C layout");
    if (@offsetOf(Status, "bar0_raw") != 20) @compileError("xHCI Status.bar0_raw C layout");
    if (@offsetOf(Status, "bar1_raw") != 24) @compileError("xHCI Status.bar1_raw C layout");
    if (@offsetOf(Status, "bar_is_io") != 28) @compileError("xHCI Status.bar_is_io C layout");
    if (@offsetOf(Status, "bar_is_64") != 29) @compileError("xHCI Status.bar_is_64 C layout");
    if (@offsetOf(Status, "mmio_phys") != 32) @compileError("xHCI Status.mmio_phys C layout");
    if (@offsetOf(Status, "mmio_virt") != 40) @compileError("xHCI Status.mmio_virt C layout");
    if (@offsetOf(Status, "caplength") != 48) @compileError("xHCI Status.caplength C layout");
    if (@offsetOf(Status, "hciversion") != 50) @compileError("xHCI Status.hciversion C layout");
    if (@offsetOf(Status, "hcsparams1") != 52) @compileError("xHCI Status.hcsparams1 C layout");
    if (@offsetOf(Status, "hcsparams2") != 56) @compileError("xHCI Status.hcsparams2 C layout");
    if (@offsetOf(Status, "hcsparams3") != 60) @compileError("xHCI Status.hcsparams3 C layout");
    if (@offsetOf(Status, "hccparams1") != 64) @compileError("xHCI Status.hccparams1 C layout");
    if (@offsetOf(Status, "hccparams2") != 68) @compileError("xHCI Status.hccparams2 C layout");
    if (@offsetOf(Status, "dboff") != 72) @compileError("xHCI Status.dboff C layout");
    if (@offsetOf(Status, "rtsoff") != 76) @compileError("xHCI Status.rtsoff C layout");
    if (@offsetOf(Status, "op_virt") != 80) @compileError("xHCI Status.op_virt C layout");
    if (@offsetOf(Status, "runtime_virt") != 88) @compileError("xHCI Status.runtime_virt C layout");
    if (@offsetOf(Status, "max_slots") != 96) @compileError("xHCI Status.max_slots C layout");
    if (@offsetOf(Status, "max_interrupters") != 98) @compileError("xHCI Status.max_interrupters C layout");
    if (@offsetOf(Status, "max_ports") != 100) @compileError("xHCI Status.max_ports C layout");
    if (@offsetOf(Status, "usbcmd") != 104) @compileError("xHCI Status.usbcmd C layout");
    if (@offsetOf(Status, "usbsts") != 108) @compileError("xHCI Status.usbsts C layout");
    if (@offsetOf(Status, "pagesize") != 112) @compileError("xHCI Status.pagesize C layout");
    if (@offsetOf(Status, "dnctrl") != 116) @compileError("xHCI Status.dnctrl C layout");
    if (@offsetOf(Status, "config") != 120) @compileError("xHCI Status.config C layout");
    if (@offsetOf(Status, "mfindex") != 124) @compileError("xHCI Status.mfindex C layout");
    if (@offsetOf(Status, "iman0") != 128) @compileError("xHCI Status.iman0 C layout");
    if (@offsetOf(Status, "imod0") != 132) @compileError("xHCI Status.imod0 C layout");
    if (@offsetOf(Status, "erstsz0") != 136) @compileError("xHCI Status.erstsz0 C layout");
    if (@offsetOf(Status, "erstba0") != 144) @compileError("xHCI Status.erstba0 C layout");
    if (@offsetOf(Status, "erdp0") != 152) @compileError("xHCI Status.erdp0 C layout");
    if (@offsetOf(Status, "crcr") != 160) @compileError("xHCI Status.crcr C layout");
    if (@offsetOf(Status, "dcbaap") != 168) @compileError("xHCI Status.dcbaap C layout");
    if (@offsetOf(Status, "port_count_seen") != 176) @compileError("xHCI Status.port_count_seen C layout");
    if (@offsetOf(Status, "first_ports") != 184) @compileError("xHCI Status.first_ports C layout");
    if (@offsetOf(Status, "legacy_cap_offset") != 952) @compileError("xHCI Status.legacy_cap_offset C layout");
    if (@offsetOf(Status, "legacy_usblegsup_before") != 956) @compileError("xHCI Status.legacy_usblegsup_before C layout");
    if (@offsetOf(Status, "legacy_usblegsup_after") != 960) @compileError("xHCI Status.legacy_usblegsup_after C layout");
    if (@offsetOf(Status, "bios_handoff") != 968) @compileError("xHCI Status.bios_handoff C layout");
    if (@offsetOf(Status, "controller_halted_before") != 984) @compileError("xHCI Status.controller_halted_before C layout");
    if (@offsetOf(Status, "controller_stopped") != 985) @compileError("xHCI Status.controller_stopped C layout");
    if (@offsetOf(Status, "controller_reset_ok") != 986) @compileError("xHCI Status.controller_reset_ok C layout");
    if (@offsetOf(Status, "controller_running") != 987) @compileError("xHCI Status.controller_running C layout");
    if (@offsetOf(Status, "dma_ready") != 988) @compileError("xHCI Status.dma_ready C layout");
    if (@offsetOf(Status, "context_size") != 989) @compileError("xHCI Status.context_size C layout");
    if (@offsetOf(Status, "dcbaa_phys") != 992) @compileError("xHCI Status.dcbaa_phys C layout");
    if (@offsetOf(Status, "command_ring_phys") != 1000) @compileError("xHCI Status.command_ring_phys C layout");
    if (@offsetOf(Status, "event_ring_phys") != 1008) @compileError("xHCI Status.event_ring_phys C layout");
    if (@offsetOf(Status, "erst_phys") != 1016) @compileError("xHCI Status.erst_phys C layout");
    if (@offsetOf(Status, "scratchpad_array_phys") != 1024) @compileError("xHCI Status.scratchpad_array_phys C layout");
    if (@offsetOf(Status, "scratchpad_count") != 1032) @compileError("xHCI Status.scratchpad_count C layout");
    if (@offsetOf(Status, "max_slots_enabled") != 1033) @compileError("xHCI Status.max_slots_enabled C layout");
    if (@offsetOf(Status, "command_enqueue") != 1034) @compileError("xHCI Status.command_enqueue C layout");
    if (@offsetOf(Status, "command_cycle") != 1036) @compileError("xHCI Status.command_cycle C layout");
    if (@offsetOf(Status, "event_dequeue") != 1038) @compileError("xHCI Status.event_dequeue C layout");
    if (@offsetOf(Status, "event_cycle") != 1040) @compileError("xHCI Status.event_cycle C layout");
    if (@offsetOf(Status, "event_raw_cycle") != 1041) @compileError("xHCI Status.event_raw_cycle C layout");
    if (@offsetOf(Status, "event_raw_type") != 1042) @compileError("xHCI Status.event_raw_type C layout");
    if (@offsetOf(Status, "event_raw_code") != 1043) @compileError("xHCI Status.event_raw_code C layout");
    if (@offsetOf(Status, "event_erdp_commits") != 1048) @compileError("xHCI Status.event_erdp_commits C layout");
    if (@offsetOf(Status, "event_max_batch") != 1056) @compileError("xHCI Status.event_max_batch C layout");
    if (@offsetOf(Status, "command_ring_wraps") != 1064) @compileError("xHCI Status.command_ring_wraps C layout");
    if (@offsetOf(Status, "event_ring_wraps") != 1072) @compileError("xHCI Status.event_ring_wraps C layout");
    if (@offsetOf(Status, "ep0_ring_wraps") != 1080) @compileError("xHCI Status.ep0_ring_wraps C layout");
    if (@offsetOf(Status, "interrupt_ring_wraps") != 1088) @compileError("xHCI Status.interrupt_ring_wraps C layout");
    if (@offsetOf(Status, "bulk_in_ring_wraps") != 1096) @compileError("xHCI Status.bulk_in_ring_wraps C layout");
    if (@offsetOf(Status, "bulk_out_ring_wraps") != 1104) @compileError("xHCI Status.bulk_out_ring_wraps C layout");
    if (@offsetOf(Status, "commands") != 1112) @compileError("xHCI Status.commands C layout");
    if (@offsetOf(Status, "events") != 1120) @compileError("xHCI Status.events C layout");
    if (@offsetOf(Status, "event_tick_deadline_waits") != 1128) @compileError("xHCI Status.event_tick_deadline_waits C layout");
    if (@offsetOf(Status, "event_hpet_deadline_waits") != 1136) @compileError("xHCI Status.event_hpet_deadline_waits C layout");
    if (@offsetOf(Status, "event_tsc_deadline_waits") != 1144) @compileError("xHCI Status.event_tsc_deadline_waits C layout");
    if (@offsetOf(Status, "event_guard_waits") != 1152) @compileError("xHCI Status.event_guard_waits C layout");
    if (@offsetOf(Status, "event_tick_timeouts") != 1160) @compileError("xHCI Status.event_tick_timeouts C layout");
    if (@offsetOf(Status, "event_hpet_timeouts") != 1168) @compileError("xHCI Status.event_hpet_timeouts C layout");
    if (@offsetOf(Status, "event_guard_timeouts") != 1176) @compileError("xHCI Status.event_guard_timeouts C layout");
    if (@offsetOf(Status, "event_tsc_timeouts") != 1184) @compileError("xHCI Status.event_tsc_timeouts C layout");
    if (@offsetOf(Status, "event_recovery_budget_timeouts") != 1192) @compileError("xHCI Status.event_recovery_budget_timeouts C layout");
    if (@offsetOf(Status, "event_cpu_guard_timeouts") != 1200) @compileError("xHCI Status.event_cpu_guard_timeouts C layout");
    if (@offsetOf(Status, "host_controller_id") != 1208) @compileError("xHCI Status.host_controller_id C layout");
    if (@offsetOf(Status, "owner_id") != 1212) @compileError("xHCI Status.owner_id C layout");
    if (@offsetOf(Status, "host_source") != 1216) @compileError("xHCI Status.host_source C layout");
    if (@offsetOf(Status, "irq_line") != 1220) @compileError("xHCI Status.irq_line C layout");
    if (@offsetOf(Status, "irq_pin") != 1221) @compileError("xHCI Status.irq_pin C layout");
    if (@offsetOf(Status, "irq_registered") != 1222) @compileError("xHCI Status.irq_registered C layout");
    if (@offsetOf(Status, "irq_mode") != 1224) @compileError("xHCI Status.irq_mode C layout");
    if (@offsetOf(Status, "irq_count") != 1240) @compileError("xHCI Status.irq_count C layout");
    if (@offsetOf(Status, "irq_handled") != 1248) @compileError("xHCI Status.irq_handled C layout");
    if (@offsetOf(Status, "irq_wakeups") != 1256) @compileError("xHCI Status.irq_wakeups C layout");
    if (@offsetOf(Status, "poll_fallbacks") != 1264) @compileError("xHCI Status.poll_fallbacks C layout");
    if (@offsetOf(Status, "wait_calls") != 1272) @compileError("xHCI Status.wait_calls C layout");
    if (@offsetOf(Status, "wait_successes") != 1280) @compileError("xHCI Status.wait_successes C layout");
    if (@offsetOf(Status, "wait_timeouts") != 1288) @compileError("xHCI Status.wait_timeouts C layout");
    if (@offsetOf(Status, "wait_early_calls") != 1296) @compileError("xHCI Status.wait_early_calls C layout");
    if (@offsetOf(Status, "wait_runtime_calls") != 1304) @compileError("xHCI Status.wait_runtime_calls C layout");
    if (@offsetOf(Status, "wait_requested_ms") != 1312) @compileError("xHCI Status.wait_requested_ms C layout");
    if (@offsetOf(Status, "wait_elapsed_ns") != 1320) @compileError("xHCI Status.wait_elapsed_ns C layout");
    if (@offsetOf(Status, "wait_max_elapsed_ns") != 1328) @compileError("xHCI Status.wait_max_elapsed_ns C layout");
    if (@offsetOf(Status, "wait_iterations") != 1336) @compileError("xHCI Status.wait_iterations C layout");
    if (@offsetOf(Status, "wait_blocked_ticks") != 1344) @compileError("xHCI Status.wait_blocked_ticks C layout");
    if (@offsetOf(Status, "last_wait_reason") != 1352) @compileError("xHCI Status.last_wait_reason C layout");
    if (@offsetOf(Status, "last_wait_phase") != 1368) @compileError("xHCI Status.last_wait_phase C layout");
    if (@offsetOf(Status, "last_wait_requested_ms") != 1372) @compileError("xHCI Status.last_wait_requested_ms C layout");
    if (@offsetOf(Status, "last_wait_elapsed_ns") != 1376) @compileError("xHCI Status.last_wait_elapsed_ns C layout");
    if (@offsetOf(Status, "last_wait_iterations") != 1384) @compileError("xHCI Status.last_wait_iterations C layout");
    if (@offsetOf(Status, "last_wait_blocked_ticks") != 1392) @compileError("xHCI Status.last_wait_blocked_ticks C layout");
    if (@offsetOf(Status, "last_wait_retry") != 1400) @compileError("xHCI Status.last_wait_retry C layout");
    if (@offsetOf(Status, "last_wait_success") != 1401) @compileError("xHCI Status.last_wait_success C layout");
    if (@offsetOf(Status, "stale_events") != 1408) @compileError("xHCI Status.stale_events C layout");
    if (@offsetOf(Status, "ring_full") != 1416) @compileError("xHCI Status.ring_full C layout");
    if (@offsetOf(Status, "deferred_events_pending") != 1424) @compileError("xHCI Status.deferred_events_pending C layout");
    if (@offsetOf(Status, "deferred_events_queued") != 1432) @compileError("xHCI Status.deferred_events_queued C layout");
    if (@offsetOf(Status, "deferred_events_delivered") != 1440) @compileError("xHCI Status.deferred_events_delivered C layout");
    if (@offsetOf(Status, "deferred_events_overflows") != 1448) @compileError("xHCI Status.deferred_events_overflows C layout");
    if (@offsetOf(Status, "deferred_events_purged") != 1456) @compileError("xHCI Status.deferred_events_purged C layout");
    if (@offsetOf(Status, "deferred_events_high_water") != 1464) @compileError("xHCI Status.deferred_events_high_water C layout");
    if (@offsetOf(Status, "retained_slots") != 1472) @compileError("xHCI Status.retained_slots C layout");
    if (@offsetOf(Status, "runtime_switches") != 1480) @compileError("xHCI Status.runtime_switches C layout");
    if (@offsetOf(Status, "port_change_clears") != 1488) @compileError("xHCI Status.port_change_clears C layout");
    if (@offsetOf(Status, "port_change_events") != 1496) @compileError("xHCI Status.port_change_events C layout");
    if (@offsetOf(Status, "port_change_queued") != 1504) @compileError("xHCI Status.port_change_queued C layout");
    if (@offsetOf(Status, "port_change_coalesced") != 1512) @compileError("xHCI Status.port_change_coalesced C layout");
    if (@offsetOf(Status, "port_change_invalid") != 1520) @compileError("xHCI Status.port_change_invalid C layout");
    if (@offsetOf(Status, "port_change_taken") != 1528) @compileError("xHCI Status.port_change_taken C layout");
    if (@offsetOf(Status, "port_change_retries") != 1536) @compileError("xHCI Status.port_change_retries C layout");
    if (@offsetOf(Status, "port_change_pending_mask") != 1544) @compileError("xHCI Status.port_change_pending_mask C layout");
    if (@offsetOf(Status, "port_change_pending") != 1546) @compileError("xHCI Status.port_change_pending C layout");
    if (@offsetOf(Status, "port_change_high_water") != 1547) @compileError("xHCI Status.port_change_high_water C layout");
    if (@offsetOf(Status, "port_service_calls") != 1552) @compileError("xHCI Status.port_service_calls C layout");
    if (@offsetOf(Status, "port_service_failures") != 1560) @compileError("xHCI Status.port_service_failures C layout");
    if (@offsetOf(Status, "port_catalog_changes") != 1568) @compileError("xHCI Status.port_catalog_changes C layout");
    if (@offsetOf(Status, "port_task_started") != 1576) @compileError("xHCI Status.port_task_started C layout");
    if (@offsetOf(Status, "port_task_id") != 1580) @compileError("xHCI Status.port_task_id C layout");
    if (@offsetOf(Status, "port_task_iterations") != 1584) @compileError("xHCI Status.port_task_iterations C layout");
    if (@offsetOf(Status, "port_debounce_failures") != 1592) @compileError("xHCI Status.port_debounce_failures C layout");
    if (@offsetOf(Status, "port_power_requests") != 1600) @compileError("xHCI Status.port_power_requests C layout");
    if (@offsetOf(Status, "port_u0_waits") != 1608) @compileError("xHCI Status.port_u0_waits C layout");
    if (@offsetOf(Status, "port_warm_resets") != 1616) @compileError("xHCI Status.port_warm_resets C layout");
    if (@offsetOf(Status, "port_disconnects") != 1624) @compileError("xHCI Status.port_disconnects C layout");
    if (@offsetOf(Status, "reclaimed_slots") != 1632) @compileError("xHCI Status.reclaimed_slots C layout");
    if (@offsetOf(Status, "disable_slot_commands") != 1640) @compileError("xHCI Status.disable_slot_commands C layout");
    if (@offsetOf(Status, "disable_slot_failures") != 1648) @compileError("xHCI Status.disable_slot_failures C layout");
    if (@offsetOf(Status, "enable_slot_attempted") != 1656) @compileError("xHCI Status.enable_slot_attempted C layout");
    if (@offsetOf(Status, "enable_slot_ok") != 1657) @compileError("xHCI Status.enable_slot_ok C layout");
    if (@offsetOf(Status, "address_device_attempted") != 1658) @compileError("xHCI Status.address_device_attempted C layout");
    if (@offsetOf(Status, "address_device_ok") != 1659) @compileError("xHCI Status.address_device_ok C layout");
    if (@offsetOf(Status, "get_descriptor_attempted") != 1660) @compileError("xHCI Status.get_descriptor_attempted C layout");
    if (@offsetOf(Status, "get_descriptor_ok") != 1661) @compileError("xHCI Status.get_descriptor_ok C layout");
    if (@offsetOf(Status, "get_config_attempted") != 1662) @compileError("xHCI Status.get_config_attempted C layout");
    if (@offsetOf(Status, "get_config_ok") != 1663) @compileError("xHCI Status.get_config_ok C layout");
    if (@offsetOf(Status, "get_strings_attempted") != 1664) @compileError("xHCI Status.get_strings_attempted C layout");
    if (@offsetOf(Status, "get_strings_ok") != 1665) @compileError("xHCI Status.get_strings_ok C layout");
    if (@offsetOf(Status, "last_command_type") != 1666) @compileError("xHCI Status.last_command_type C layout");
    if (@offsetOf(Status, "last_completion_code") != 1667) @compileError("xHCI Status.last_completion_code C layout");
    if (@offsetOf(Status, "last_transfer_completion_code") != 1668) @compileError("xHCI Status.last_transfer_completion_code C layout");
    if (@offsetOf(Status, "last_event_type") != 1669) @compileError("xHCI Status.last_event_type C layout");
    if (@offsetOf(Status, "last_event_code") != 1670) @compileError("xHCI Status.last_event_code C layout");
    if (@offsetOf(Status, "last_event_slot") != 1671) @compileError("xHCI Status.last_event_slot C layout");
    if (@offsetOf(Status, "last_event_endpoint") != 1672) @compileError("xHCI Status.last_event_endpoint C layout");
    if (@offsetOf(Status, "last_event_parameter") != 1680) @compileError("xHCI Status.last_event_parameter C layout");
    if (@offsetOf(Status, "last_event_length") != 1688) @compileError("xHCI Status.last_event_length C layout");
    if (@offsetOf(Status, "last_stale_event_type") != 1692) @compileError("xHCI Status.last_stale_event_type C layout");
    if (@offsetOf(Status, "last_stale_event_code") != 1693) @compileError("xHCI Status.last_stale_event_code C layout");
    if (@offsetOf(Status, "last_stale_event_slot") != 1694) @compileError("xHCI Status.last_stale_event_slot C layout");
    if (@offsetOf(Status, "last_stale_event_endpoint") != 1695) @compileError("xHCI Status.last_stale_event_endpoint C layout");
    if (@offsetOf(Status, "last_slot_id") != 1696) @compileError("xHCI Status.last_slot_id C layout");
    if (@offsetOf(Status, "addressed_slot_id") != 1697) @compileError("xHCI Status.addressed_slot_id C layout");
    if (@offsetOf(Status, "addressed_port") != 1698) @compileError("xHCI Status.addressed_port C layout");
    if (@offsetOf(Status, "addressed_speed") != 1699) @compileError("xHCI Status.addressed_speed C layout");
    if (@offsetOf(Status, "device_context_phys") != 1704) @compileError("xHCI Status.device_context_phys C layout");
    if (@offsetOf(Status, "input_context_phys") != 1712) @compileError("xHCI Status.input_context_phys C layout");
    if (@offsetOf(Status, "ep0_ring_phys") != 1720) @compileError("xHCI Status.ep0_ring_phys C layout");
    if (@offsetOf(Status, "interrupt_ring_phys") != 1728) @compileError("xHCI Status.interrupt_ring_phys C layout");
    if (@offsetOf(Status, "interrupt_buffer_phys") != 1736) @compileError("xHCI Status.interrupt_buffer_phys C layout");
    if (@offsetOf(Status, "bulk_in_ring_phys") != 1744) @compileError("xHCI Status.bulk_in_ring_phys C layout");
    if (@offsetOf(Status, "bulk_out_ring_phys") != 1752) @compileError("xHCI Status.bulk_out_ring_phys C layout");
    if (@offsetOf(Status, "bulk_buffer_phys") != 1760) @compileError("xHCI Status.bulk_buffer_phys C layout");
    if (@offsetOf(Status, "bulk_buffer_frames") != 1768) @compileError("xHCI Status.bulk_buffer_frames C layout");
    if (@offsetOf(Status, "descriptor_phys") != 1776) @compileError("xHCI Status.descriptor_phys C layout");
    if (@offsetOf(Status, "ep0_enqueue") != 1784) @compileError("xHCI Status.ep0_enqueue C layout");
    if (@offsetOf(Status, "ep0_cycle") != 1786) @compileError("xHCI Status.ep0_cycle C layout");
    if (@offsetOf(Status, "control_endpoint_faulted") != 1787) @compileError("xHCI Status.control_endpoint_faulted C layout");
    if (@offsetOf(Status, "interrupt_enqueue") != 1788) @compileError("xHCI Status.interrupt_enqueue C layout");
    if (@offsetOf(Status, "interrupt_cycle") != 1790) @compileError("xHCI Status.interrupt_cycle C layout");
    if (@offsetOf(Status, "interrupt_link_cycle") != 1791) @compileError("xHCI Status.interrupt_link_cycle C layout");
    if (@offsetOf(Status, "interrupt_endpoint_id") != 1792) @compileError("xHCI Status.interrupt_endpoint_id C layout");
    if (@offsetOf(Status, "interrupt_endpoint_address") != 1793) @compileError("xHCI Status.interrupt_endpoint_address C layout");
    if (@offsetOf(Status, "interrupt_endpoint_max_packet") != 1794) @compileError("xHCI Status.interrupt_endpoint_max_packet C layout");
    if (@offsetOf(Status, "interrupt_endpoint_interval_raw") != 1796) @compileError("xHCI Status.interrupt_endpoint_interval_raw C layout");
    if (@offsetOf(Status, "interrupt_endpoint_interval_context") != 1797) @compileError("xHCI Status.interrupt_endpoint_interval_context C layout");
    if (@offsetOf(Status, "interrupt_endpoint_configured") != 1798) @compileError("xHCI Status.interrupt_endpoint_configured C layout");
    if (@offsetOf(Status, "interrupt_endpoint_faulted") != 1799) @compileError("xHCI Status.interrupt_endpoint_faulted C layout");
    if (@offsetOf(Status, "interrupt_pending") != 1800) @compileError("xHCI Status.interrupt_pending C layout");
    if (@offsetOf(Status, "interrupt_pending_trb_phys") != 1808) @compileError("xHCI Status.interrupt_pending_trb_phys C layout");
    if (@offsetOf(Status, "interrupt_transfer_handle") != 1816) @compileError("xHCI Status.interrupt_transfer_handle C layout");
    if (@offsetOf(Status, "interrupt_pending_index") != 1820) @compileError("xHCI Status.interrupt_pending_index C layout");
    if (@offsetOf(Status, "interrupt_hw_dequeue") != 1824) @compileError("xHCI Status.interrupt_hw_dequeue C layout");
    if (@offsetOf(Status, "bulk_endpoints_configured") != 1832) @compileError("xHCI Status.bulk_endpoints_configured C layout");
    if (@offsetOf(Status, "bulk_endpoints_faulted") != 1833) @compileError("xHCI Status.bulk_endpoints_faulted C layout");
    if (@offsetOf(Status, "bulk_in_endpoint_id") != 1834) @compileError("xHCI Status.bulk_in_endpoint_id C layout");
    if (@offsetOf(Status, "bulk_out_endpoint_id") != 1835) @compileError("xHCI Status.bulk_out_endpoint_id C layout");
    if (@offsetOf(Status, "bulk_in_enqueue") != 1836) @compileError("xHCI Status.bulk_in_enqueue C layout");
    if (@offsetOf(Status, "bulk_in_cycle") != 1838) @compileError("xHCI Status.bulk_in_cycle C layout");
    if (@offsetOf(Status, "bulk_in_link_update_pending") != 1839) @compileError("xHCI Status.bulk_in_link_update_pending C layout");
    if (@offsetOf(Status, "bulk_out_enqueue") != 1840) @compileError("xHCI Status.bulk_out_enqueue C layout");
    if (@offsetOf(Status, "bulk_out_cycle") != 1842) @compileError("xHCI Status.bulk_out_cycle C layout");
    if (@offsetOf(Status, "bulk_out_link_update_pending") != 1843) @compileError("xHCI Status.bulk_out_link_update_pending C layout");
    if (@offsetOf(Status, "set_configuration_attempted") != 1844) @compileError("xHCI Status.set_configuration_attempted C layout");
    if (@offsetOf(Status, "set_configuration_ok") != 1845) @compileError("xHCI Status.set_configuration_ok C layout");
    if (@offsetOf(Status, "hid_set_protocol_attempted") != 1846) @compileError("xHCI Status.hid_set_protocol_attempted C layout");
    if (@offsetOf(Status, "hid_set_protocol_ok") != 1847) @compileError("xHCI Status.hid_set_protocol_ok C layout");
    if (@offsetOf(Status, "hid_set_idle_attempted") != 1848) @compileError("xHCI Status.hid_set_idle_attempted C layout");
    if (@offsetOf(Status, "hid_set_idle_ok") != 1849) @compileError("xHCI Status.hid_set_idle_ok C layout");
    if (@offsetOf(Status, "interrupt_polls") != 1856) @compileError("xHCI Status.interrupt_polls C layout");
    if (@offsetOf(Status, "interrupt_reports") != 1864) @compileError("xHCI Status.interrupt_reports C layout");
    if (@offsetOf(Status, "interrupt_no_report") != 1872) @compileError("xHCI Status.interrupt_no_report C layout");
    if (@offsetOf(Status, "interrupt_recoveries") != 1880) @compileError("xHCI Status.interrupt_recoveries C layout");
    if (@offsetOf(Status, "interrupt_pending_timeouts") != 1888) @compileError("xHCI Status.interrupt_pending_timeouts C layout");
    if (@offsetOf(Status, "interrupt_halted_checks") != 1896) @compileError("xHCI Status.interrupt_halted_checks C layout");
    if (@offsetOf(Status, "interrupt_pending_streak") != 1904) @compileError("xHCI Status.interrupt_pending_streak C layout");
    if (@offsetOf(Status, "last_interrupt_ep_state") != 1912) @compileError("xHCI Status.last_interrupt_ep_state C layout");
    if (@offsetOf(Status, "last_interrupt_request_len") != 1916) @compileError("xHCI Status.last_interrupt_request_len C layout");
    if (@offsetOf(Status, "last_interrupt_residue") != 1920) @compileError("xHCI Status.last_interrupt_residue C layout");
    if (@offsetOf(Status, "last_interrupt_actual_len") != 1924) @compileError("xHCI Status.last_interrupt_actual_len C layout");
    if (@offsetOf(Status, "last_interrupt_completion_code") != 1928) @compileError("xHCI Status.last_interrupt_completion_code C layout");
    if (@offsetOf(Status, "control_transfers") != 1936) @compileError("xHCI Status.control_transfers C layout");
    if (@offsetOf(Status, "control_failures") != 1944) @compileError("xHCI Status.control_failures C layout");
    if (@offsetOf(Status, "control_timeouts") != 1952) @compileError("xHCI Status.control_timeouts C layout");
    if (@offsetOf(Status, "control_stalls") != 1960) @compileError("xHCI Status.control_stalls C layout");
    if (@offsetOf(Status, "control_short_packets") != 1968) @compileError("xHCI Status.control_short_packets C layout");
    if (@offsetOf(Status, "last_control_request_type") != 1976) @compileError("xHCI Status.last_control_request_type C layout");
    if (@offsetOf(Status, "last_control_request") != 1977) @compileError("xHCI Status.last_control_request C layout");
    if (@offsetOf(Status, "last_control_value") != 1978) @compileError("xHCI Status.last_control_value C layout");
    if (@offsetOf(Status, "last_control_index") != 1980) @compileError("xHCI Status.last_control_index C layout");
    if (@offsetOf(Status, "last_control_length") != 1982) @compileError("xHCI Status.last_control_length C layout");
    if (@offsetOf(Status, "last_control_direction") != 1984) @compileError("xHCI Status.last_control_direction C layout");
    if (@offsetOf(Status, "last_control_completion_code") != 2000) @compileError("xHCI Status.last_control_completion_code C layout");
    if (@offsetOf(Status, "last_control_residue") != 2004) @compileError("xHCI Status.last_control_residue C layout");
    if (@offsetOf(Status, "last_control_ok") != 2008) @compileError("xHCI Status.last_control_ok C layout");
    if (@offsetOf(Status, "transfer_events") != 2016) @compileError("xHCI Status.transfer_events C layout");
    if (@offsetOf(Status, "bulk_transfers") != 2024) @compileError("xHCI Status.bulk_transfers C layout");
    if (@offsetOf(Status, "bulk_failures") != 2032) @compileError("xHCI Status.bulk_failures C layout");
    if (@offsetOf(Status, "transfer_objects_active") != 2040) @compileError("xHCI Status.transfer_objects_active C layout");
    if (@offsetOf(Status, "transfer_objects_high_water") != 2044) @compileError("xHCI Status.transfer_objects_high_water C layout");
    if (@offsetOf(Status, "transfer_objects_submitted") != 2048) @compileError("xHCI Status.transfer_objects_submitted C layout");
    if (@offsetOf(Status, "transfer_objects_completed") != 2056) @compileError("xHCI Status.transfer_objects_completed C layout");
    if (@offsetOf(Status, "transfer_objects_timed_out") != 2064) @compileError("xHCI Status.transfer_objects_timed_out C layout");
    if (@offsetOf(Status, "transfer_objects_cancelled") != 2072) @compileError("xHCI Status.transfer_objects_cancelled C layout");
    if (@offsetOf(Status, "transfer_objects_failed") != 2080) @compileError("xHCI Status.transfer_objects_failed C layout");
    if (@offsetOf(Status, "last_bulk_direction") != 2088) @compileError("xHCI Status.last_bulk_direction C layout");
    if (@offsetOf(Status, "last_bulk_result") != 2104) @compileError("xHCI Status.last_bulk_result C layout");
    if (@offsetOf(Status, "last_bulk_request_len") != 2120) @compileError("xHCI Status.last_bulk_request_len C layout");
    if (@offsetOf(Status, "last_bulk_residue") != 2124) @compileError("xHCI Status.last_bulk_residue C layout");
    if (@offsetOf(Status, "last_bulk_actual_len") != 2128) @compileError("xHCI Status.last_bulk_actual_len C layout");
    if (@offsetOf(Status, "last_bulk_completion_code") != 2132) @compileError("xHCI Status.last_bulk_completion_code C layout");
    if (@offsetOf(Status, "ports_probed") != 2133) @compileError("xHCI Status.ports_probed C layout");
    if (@offsetOf(Status, "non_hid_devices_skipped") != 2134) @compileError("xHCI Status.non_hid_devices_skipped C layout");
    if (@offsetOf(Status, "probe_record_count") != 2135) @compileError("xHCI Status.probe_record_count C layout");
    if (@offsetOf(Status, "probe_records") != 2136) @compileError("xHCI Status.probe_records C layout");
    if (@offsetOf(Status, "selected_mass_storage") != 2776) @compileError("xHCI Status.selected_mass_storage C layout");
    if (@offsetOf(Status, "selected_hid_input") != 2777) @compileError("xHCI Status.selected_hid_input C layout");
    if (@offsetOf(Status, "descriptor_len") != 2778) @compileError("xHCI Status.descriptor_len C layout");
    if (@offsetOf(Status, "descriptor_type") != 2779) @compileError("xHCI Status.descriptor_type C layout");
    if (@offsetOf(Status, "usb_version_bcd") != 2780) @compileError("xHCI Status.usb_version_bcd C layout");
    if (@offsetOf(Status, "device_class") != 2782) @compileError("xHCI Status.device_class C layout");
    if (@offsetOf(Status, "device_subclass") != 2783) @compileError("xHCI Status.device_subclass C layout");
    if (@offsetOf(Status, "device_protocol") != 2784) @compileError("xHCI Status.device_protocol C layout");
    if (@offsetOf(Status, "device_max_packet0") != 2785) @compileError("xHCI Status.device_max_packet0 C layout");
    if (@offsetOf(Status, "device_vendor_id") != 2786) @compileError("xHCI Status.device_vendor_id C layout");
    if (@offsetOf(Status, "device_product_id") != 2788) @compileError("xHCI Status.device_product_id C layout");
    if (@offsetOf(Status, "device_version_bcd") != 2790) @compileError("xHCI Status.device_version_bcd C layout");
    if (@offsetOf(Status, "manufacturer_index") != 2792) @compileError("xHCI Status.manufacturer_index C layout");
    if (@offsetOf(Status, "product_index") != 2793) @compileError("xHCI Status.product_index C layout");
    if (@offsetOf(Status, "serial_index") != 2794) @compileError("xHCI Status.serial_index C layout");
    if (@offsetOf(Status, "config_total_length") != 2796) @compileError("xHCI Status.config_total_length C layout");
    if (@offsetOf(Status, "config_value") != 2798) @compileError("xHCI Status.config_value C layout");
    if (@offsetOf(Status, "config_attributes") != 2799) @compileError("xHCI Status.config_attributes C layout");
    if (@offsetOf(Status, "config_max_power_ma") != 2800) @compileError("xHCI Status.config_max_power_ma C layout");
    if (@offsetOf(Status, "interface_count") != 2802) @compileError("xHCI Status.interface_count C layout");
    if (@offsetOf(Status, "interface_record_count") != 2803) @compileError("xHCI Status.interface_record_count C layout");
    if (@offsetOf(Status, "interface_records") != 2804) @compileError("xHCI Status.interface_records C layout");
    if (@offsetOf(Status, "endpoint_count") != 2980) @compileError("xHCI Status.endpoint_count C layout");
    if (@offsetOf(Status, "descriptor_records") != 2981) @compileError("xHCI Status.descriptor_records C layout");
    if (@offsetOf(Status, "descriptor_unknown") != 2982) @compileError("xHCI Status.descriptor_unknown C layout");
    if (@offsetOf(Status, "descriptor_malformed") != 2983) @compileError("xHCI Status.descriptor_malformed C layout");
    if (@offsetOf(Status, "hid_descriptor_count") != 2984) @compileError("xHCI Status.hid_descriptor_count C layout");
    if (@offsetOf(Status, "ss_endpoint_companion_count") != 2985) @compileError("xHCI Status.ss_endpoint_companion_count C layout");
    if (@offsetOf(Status, "selected_interface_reason") != 2992) @compileError("xHCI Status.selected_interface_reason C layout");
    if (@offsetOf(Status, "first_interface_number") != 3008) @compileError("xHCI Status.first_interface_number C layout");
    if (@offsetOf(Status, "first_interface_class") != 3009) @compileError("xHCI Status.first_interface_class C layout");
    if (@offsetOf(Status, "first_interface_subclass") != 3010) @compileError("xHCI Status.first_interface_subclass C layout");
    if (@offsetOf(Status, "first_interface_protocol") != 3011) @compileError("xHCI Status.first_interface_protocol C layout");
    if (@offsetOf(Status, "first_endpoint_address") != 3012) @compileError("xHCI Status.first_endpoint_address C layout");
    if (@offsetOf(Status, "first_endpoint_attributes") != 3013) @compileError("xHCI Status.first_endpoint_attributes C layout");
    if (@offsetOf(Status, "first_endpoint_max_packet") != 3014) @compileError("xHCI Status.first_endpoint_max_packet C layout");
    if (@offsetOf(Status, "first_endpoint_interval") != 3016) @compileError("xHCI Status.first_endpoint_interval C layout");
    if (@offsetOf(Status, "first_hid_descriptor_len") != 3017) @compileError("xHCI Status.first_hid_descriptor_len C layout");
    if (@offsetOf(Status, "first_hid_report_descriptor_len") != 3018) @compileError("xHCI Status.first_hid_report_descriptor_len C layout");
    if (@offsetOf(Status, "bulk_in_endpoint_address") != 3020) @compileError("xHCI Status.bulk_in_endpoint_address C layout");
    if (@offsetOf(Status, "bulk_in_endpoint_max_packet") != 3022) @compileError("xHCI Status.bulk_in_endpoint_max_packet C layout");
    if (@offsetOf(Status, "bulk_in_endpoint_max_burst") != 3024) @compileError("xHCI Status.bulk_in_endpoint_max_burst C layout");
    if (@offsetOf(Status, "bulk_out_endpoint_address") != 3025) @compileError("xHCI Status.bulk_out_endpoint_address C layout");
    if (@offsetOf(Status, "bulk_out_endpoint_max_packet") != 3026) @compileError("xHCI Status.bulk_out_endpoint_max_packet C layout");
    if (@offsetOf(Status, "bulk_out_endpoint_max_burst") != 3028) @compileError("xHCI Status.bulk_out_endpoint_max_burst C layout");
    if (@offsetOf(Status, "string_language_id") != 3030) @compileError("xHCI Status.string_language_id C layout");
    if (@offsetOf(Status, "manufacturer_len") != 3032) @compileError("xHCI Status.manufacturer_len C layout");
    if (@offsetOf(Status, "product_len") != 3033) @compileError("xHCI Status.product_len C layout");
    if (@offsetOf(Status, "serial_len") != 3034) @compileError("xHCI Status.serial_len C layout");
    if (@offsetOf(Status, "manufacturer_string") != 3035) @compileError("xHCI Status.manufacturer_string C layout");
    if (@offsetOf(Status, "product_string") != 3067) @compileError("xHCI Status.product_string C layout");
    if (@offsetOf(Status, "serial_string") != 3099) @compileError("xHCI Status.serial_string C layout");
    if (@offsetOf(Status, "connected_ports") != 3131) @compileError("xHCI Status.connected_ports C layout");
    if (@offsetOf(Status, "enabled_ports") != 3132) @compileError("xHCI Status.enabled_ports C layout");
    if (@offsetOf(Status, "reset_ports") != 3133) @compileError("xHCI Status.reset_ports C layout");
    if (@offsetOf(Status, "failures") != 3136) @compileError("xHCI Status.failures C layout");
    if (@offsetOf(Status, "timeouts") != 3144) @compileError("xHCI Status.timeouts C layout");
    if (@offsetOf(Status, "reason") != 3152) @compileError("xHCI Status.reason C layout");
    if (@offsetOf(Status, "last_wait_cancelled") != 3168) @compileError("xHCI Status.last_wait_cancelled C layout");
}
pub const BulkSubmission = extern struct {
    trb_phys: [32]u64 = .{0} ** 32,
    trb_count: u8 = 0,
};
comptime {
    if (@sizeOf(BulkSubmission) != 264 or @alignOf(BulkSubmission) != 8) @compileError("xHCI BulkSubmission C layout");
    if (@offsetOf(BulkSubmission, "trb_phys") != 0) @compileError("xHCI BulkSubmission.trb_phys C layout");
    if (@offsetOf(BulkSubmission, "trb_count") != 256) @compileError("xHCI BulkSubmission.trb_count C layout");
}
pub const BulkTrbSubmission = extern struct {
    trb_phys: u64 = 0,
};
comptime {
    if (@sizeOf(BulkTrbSubmission) != 8 or @alignOf(BulkTrbSubmission) != 8) @compileError("xHCI BulkTrbSubmission C layout");
    if (@offsetOf(BulkTrbSubmission, "trb_phys") != 0) @compileError("xHCI BulkTrbSubmission.trb_phys C layout");
}
pub const InterruptSubmission = extern struct {
    trb_phys: u64 = 0,
    enqueue: u16 = 0,
    producer_cycle: u8 = 0,
    advance: Advance = .{ .next_enqueue = 0, .next_producer_cycle = 0, .link_cycle = 0, .wrapped = false },
};
comptime {
    if (@sizeOf(InterruptSubmission) != 24 or @alignOf(InterruptSubmission) != 8) @compileError("xHCI InterruptSubmission C layout");
    if (@offsetOf(InterruptSubmission, "trb_phys") != 0) @compileError("xHCI InterruptSubmission.trb_phys C layout");
    if (@offsetOf(InterruptSubmission, "enqueue") != 8) @compileError("xHCI InterruptSubmission.enqueue C layout");
    if (@offsetOf(InterruptSubmission, "producer_cycle") != 10) @compileError("xHCI InterruptSubmission.producer_cycle C layout");
    if (@offsetOf(InterruptSubmission, "advance") != 12) @compileError("xHCI InterruptSubmission.advance C layout");
}
extern fn r4os_rust_usb_xhci_acquire() bool;
pub fn acquireControllerOwnership() bool {
    return r4os_rust_usb_xhci_acquire();
}
extern fn r4os_rust_usb_xhci_release() void;
pub fn releaseControllerOwnership() void {
    r4os_rust_usb_xhci_release();
}
extern fn r4os_rust_usb_xhci_begin_transaction(*DeviceHandle, *DeviceTransaction) bool;
pub fn beginDeviceTransaction(h: *DeviceHandle) ?DeviceTransaction {
    var out: DeviceTransaction = undefined;
    return if (r4os_rust_usb_xhci_begin_transaction(h, &out)) out else null;
}
extern fn r4os_rust_usb_xhci_end_transaction(*const DeviceTransaction) void;
pub fn endDeviceTransaction(t: DeviceTransaction) void {
    r4os_rust_usb_xhci_end_transaction(&t);
}
extern fn r4os_rust_usb_xhci_status(*Status) void;
pub fn status() Status {
    var out: Status = undefined;
    r4os_rust_usb_xhci_status(&out);
    return out;
}
extern fn r4os_rust_usb_xhci_status_for_device(*const DeviceHandle, *Status) bool;
pub fn statusForDevice(h: DeviceHandle) ?Status {
    var out: Status = undefined;
    return if (r4os_rust_usb_xhci_status_for_device(&h, &out)) out else null;
}
extern fn r4os_rust_usb_xhci_take_device_incident(*const DeviceHandle) u64;
pub fn takeSyncTransferIncidentForDevice(h: DeviceHandle) diag_screen.IncidentToken {
    return .{ .generation = r4os_rust_usb_xhci_take_device_incident(&h) };
}
extern fn r4os_rust_usb_xhci_set_topology_hook(?TopologyHook) void;
pub fn setTopologyHook(h: ?TopologyHook) void {
    r4os_rust_usb_xhci_set_topology_hook(h);
}
extern fn r4os_rust_usb_xhci_service_port_changes() bool;
pub fn servicePortChanges() bool {
    return r4os_rust_usb_xhci_service_port_changes();
}
extern fn r4os_rust_usb_xhci_probe() bool;
pub fn probe() bool {
    return r4os_rust_usb_xhci_probe();
}
extern fn r4os_rust_usb_xhci_activate(u32, u32) i32;
pub fn activate(owner_id: u32, source: u32) i32 {
    return r4os_rust_usb_xhci_activate(owner_id, source);
}
extern fn r4os_rust_usb_xhci_start_port_task() bool;
pub fn startPortTask() bool {
    return r4os_rust_usb_xhci_start_port_task();
}
extern fn r4os_rust_usb_xhci_usb_host_device_handle(*const DeviceHandle, *usb_host.DeviceHandle) void;
pub fn usbHostDeviceHandle(h: DeviceHandle) usb_host.DeviceHandle {
    var out: usb_host.DeviceHandle = undefined;
    r4os_rust_usb_xhci_usb_host_device_handle(&h, &out);
    return out;
}
extern fn r4os_rust_usb_xhci_usb_host_endpoint_handle(*const EndpointHandle, *usb_host.EndpointHandle) void;
pub fn usbHostEndpointHandle(h: EndpointHandle) usb_host.EndpointHandle {
    var out: usb_host.EndpointHandle = undefined;
    r4os_rust_usb_xhci_usb_host_endpoint_handle(&h, &out);
    return out;
}
extern fn r4os_rust_usb_xhci_device_handle_from_core(*const usb_core.Device, *DeviceHandle) void;
pub fn deviceHandleFromCore(h: *const usb_core.Device) DeviceHandle {
    var out: DeviceHandle = undefined;
    r4os_rust_usb_xhci_device_handle_from_core(h, &out);
    return out;
}
extern fn r4os_rust_usb_xhci_interrupt_in_handle_from_core(*const usb_core.Device, *EndpointHandle) void;
pub fn interruptInHandleFromCore(h: *const usb_core.Device) EndpointHandle {
    var out: EndpointHandle = undefined;
    r4os_rust_usb_xhci_interrupt_in_handle_from_core(h, &out);
    return out;
}
extern fn r4os_rust_usb_xhci_bulk_in_handle_from_core(*const usb_core.Device, *EndpointHandle) void;
pub fn bulkInHandleFromCore(h: *const usb_core.Device) EndpointHandle {
    var out: EndpointHandle = undefined;
    r4os_rust_usb_xhci_bulk_in_handle_from_core(h, &out);
    return out;
}
extern fn r4os_rust_usb_xhci_bulk_out_handle_from_core(*const usb_core.Device, *EndpointHandle) void;
pub fn bulkOutHandleFromCore(h: *const usb_core.Device) EndpointHandle {
    var out: EndpointHandle = undefined;
    r4os_rust_usb_xhci_bulk_out_handle_from_core(h, &out);
    return out;
}
extern fn r4os_rust_usb_xhci_select_device_handle(*DeviceHandle) bool;
pub fn selectDeviceHandle(h: *DeviceHandle) bool {
    return r4os_rust_usb_xhci_select_device_handle(h);
}
extern fn r4os_rust_usb_xhci_device_handle_current(*const DeviceHandle) bool;
pub fn deviceHandleCurrent(h: DeviceHandle) bool {
    return r4os_rust_usb_xhci_device_handle_current(&h);
}
extern fn r4os_rust_usb_xhci_configure_interrupt_in_endpoint_handle(*EndpointHandle) bool;
pub fn configureInterruptInEndpointHandle(h: *EndpointHandle) bool {
    return r4os_rust_usb_xhci_configure_interrupt_in_endpoint_handle(h);
}
extern fn r4os_rust_usb_xhci_select_device_by_port(u8) bool;
pub fn selectDeviceByPort(port: u8) bool {
    return r4os_rust_usb_xhci_select_device_by_port(port);
}
extern fn r4os_rust_usb_xhci_configure_first_interrupt_in_endpoint(u8, u16, u8) bool;
pub fn configureFirstInterruptInEndpoint(address: u8, packet: u16, interval: u8) bool {
    return r4os_rust_usb_xhci_configure_first_interrupt_in_endpoint(address, packet, interval);
}
extern fn r4os_rust_usb_xhci_configure_first_bulk_endpoints(u8, u16, u8, u8, u16, u8) bool;
pub fn configureFirstBulkEndpoints(input_address: u8, input_packet: u16, input_burst: u8, output_address: u8, output_packet: u16, output_burst: u8) bool {
    return r4os_rust_usb_xhci_configure_first_bulk_endpoints(input_address, input_packet, input_burst, output_address, output_packet, output_burst);
}
extern fn r4os_rust_usb_xhci_configure_bulk_endpoint_handles(*EndpointHandle, *EndpointHandle) bool;
pub fn configureBulkEndpointHandles(input: *EndpointHandle, output: *EndpointHandle) bool {
    return r4os_rust_usb_xhci_configure_bulk_endpoint_handles(input, output);
}
extern fn r4os_rust_usb_xhci_set_first_configuration() bool;
pub fn setFirstConfiguration() bool {
    return r4os_rust_usb_xhci_set_first_configuration();
}
extern fn r4os_rust_usb_xhci_set_first_configuration_for_hid() bool;
pub fn setFirstConfigurationForHid() bool {
    return r4os_rust_usb_xhci_set_first_configuration_for_hid();
}
extern fn r4os_rust_usb_xhci_set_configuration_for_handle(*DeviceHandle) bool;
pub fn setConfigurationForHandle(h: *DeviceHandle) bool {
    return r4os_rust_usb_xhci_set_configuration_for_handle(h);
}
extern fn r4os_rust_usb_xhci_set_first_hid_boot_protocol(u8) bool;
pub fn setFirstHidBootProtocol(interface: u8) bool {
    return r4os_rust_usb_xhci_set_first_hid_boot_protocol(interface);
}
extern fn r4os_rust_usb_xhci_set_hid_boot_protocol_for_handle(*DeviceHandle, u8) bool;
pub fn setHidBootProtocolForHandle(h: *DeviceHandle, interface: u8) bool {
    return r4os_rust_usb_xhci_set_hid_boot_protocol_for_handle(h, interface);
}
extern fn r4os_rust_usb_xhci_set_first_hid_idle(u8) bool;
pub fn setFirstHidIdle(interface: u8) bool {
    return r4os_rust_usb_xhci_set_first_hid_idle(interface);
}
extern fn r4os_rust_usb_xhci_set_hid_idle_for_handle(*DeviceHandle, u8) bool;
pub fn setHidIdleForHandle(h: *DeviceHandle, interface: u8) bool {
    return r4os_rust_usb_xhci_set_hid_idle_for_handle(h, interface);
}
extern fn r4os_rust_usb_xhci_reset_mass_storage_interface_for_handle(*DeviceHandle, u8) bool;
pub fn resetMassStorageInterfaceForHandle(h: *DeviceHandle, interface: u8) bool {
    return r4os_rust_usb_xhci_reset_mass_storage_interface_for_handle(h, interface);
}
extern fn r4os_rust_usb_xhci_reset_mass_storage_interface_for_handle_within(*DeviceHandle, u8, *const usb_wait.Deadline) bool;
pub fn resetMassStorageInterfaceForHandleWithin(h: *DeviceHandle, interface: u8, budget: *const usb_wait.Deadline) bool {
    return r4os_rust_usb_xhci_reset_mass_storage_interface_for_handle_within(h, interface, budget);
}
extern fn r4os_rust_usb_xhci_reset_first_mass_storage_interface(u8) bool;
pub fn resetFirstMassStorageInterface(interface: u8) bool {
    return r4os_rust_usb_xhci_reset_first_mass_storage_interface(interface);
}
extern fn r4os_rust_usb_xhci_clear_endpoint_halt_for_handle(*DeviceHandle, u8) bool;
pub fn clearEndpointHaltForHandle(h: *DeviceHandle, address: u8) bool {
    return r4os_rust_usb_xhci_clear_endpoint_halt_for_handle(h, address);
}
extern fn r4os_rust_usb_xhci_clear_endpoint_halt_for_handle_within(*DeviceHandle, u8, *const usb_wait.Deadline) bool;
pub fn clearEndpointHaltForHandleWithin(h: *DeviceHandle, address: u8, budget: *const usb_wait.Deadline) bool {
    return r4os_rust_usb_xhci_clear_endpoint_halt_for_handle_within(h, address, budget);
}
extern fn r4os_rust_usb_xhci_clear_first_endpoint_halt(u8) bool;
pub fn clearFirstEndpointHalt(address: u8) bool {
    return r4os_rust_usb_xhci_clear_first_endpoint_halt(address);
}
extern fn r4os_rust_usb_xhci_reset_endpoint_state_for_handle(*DeviceHandle, u8) bool;
pub fn resetEndpointStateForHandle(h: *DeviceHandle, address: u8) bool {
    return r4os_rust_usb_xhci_reset_endpoint_state_for_handle(h, address);
}
extern fn r4os_rust_usb_xhci_reset_endpoint_state_for_handle_within(*DeviceHandle, u8, *const usb_wait.Deadline) bool;
pub fn resetEndpointStateForHandleWithin(h: *DeviceHandle, address: u8, budget: *const usb_wait.Deadline) bool {
    return r4os_rust_usb_xhci_reset_endpoint_state_for_handle_within(h, address, budget);
}
extern fn r4os_rust_usb_xhci_reset_first_endpoint_state(u8) bool;
pub fn resetFirstEndpointState(address: u8) bool {
    return r4os_rust_usb_xhci_reset_first_endpoint_state(address);
}
extern fn r4os_rust_usb_xhci_reconfigure_mass_storage_bulk_endpoints_for_handle_within(*DeviceHandle, u8, u8, *const usb_wait.Deadline) bool;
pub fn reconfigureMassStorageBulkEndpointsForHandleWithin(h: *DeviceHandle, input_address: u8, output_address: u8, budget: *const usb_wait.Deadline) bool {
    return r4os_rust_usb_xhci_reconfigure_mass_storage_bulk_endpoints_for_handle_within(h, input_address, output_address, budget);
}
extern fn r4os_rust_usb_xhci_get_hid_report_descriptor_for_handle(*DeviceHandle, u8, u16, [*]u8, usize) usize;
pub fn getHidReportDescriptorForHandle(h: *DeviceHandle, interface: u8, expected: u16, out: []u8) ?usize {
    const n = r4os_rust_usb_xhci_get_hid_report_descriptor_for_handle(h, interface, expected, out.ptr, out.len);
    return if (n == ~@as(usize, 0)) null else n;
}
extern fn r4os_rust_usb_xhci_last_interrupt_actual_length() usize;
pub fn lastInterruptActualLength() usize {
    return r4os_rust_usb_xhci_last_interrupt_actual_length();
}
extern fn r4os_rust_usb_xhci_last_interrupt_requested_length() usize;
pub fn lastInterruptRequestedLength() usize {
    return r4os_rust_usb_xhci_last_interrupt_requested_length();
}
extern fn r4os_rust_usb_xhci_last_interrupt_residue() u32;
pub fn lastInterruptResidue() u32 {
    return r4os_rust_usb_xhci_last_interrupt_residue();
}
extern fn r4os_rust_usb_xhci_take_last_sync_transfer_incident_token() u64;
pub fn takeLastSyncTransferIncidentToken() diag_screen.IncidentToken {
    return .{ .generation = r4os_rust_usb_xhci_take_last_sync_transfer_incident_token() };
}
extern fn r4os_rust_usb_xhci_poll_first_interrupt_in_report_status([*]u8, usize) u8;
pub fn pollFirstInterruptInReportStatus(out: []u8) InterruptPollResult {
    return @enumFromInt(r4os_rust_usb_xhci_poll_first_interrupt_in_report_status(out.ptr, out.len));
}
pub fn pollFirstInterruptInReport(out: []u8) bool {
    return pollFirstInterruptInReportStatus(out) == .report;
}
extern fn r4os_rust_usb_xhci_poll_interrupt_in_report_status(*EndpointHandle, [*]u8, usize) u8;
pub fn pollInterruptInReportStatus(h: *EndpointHandle, out: []u8) InterruptPollResult {
    return @enumFromInt(r4os_rust_usb_xhci_poll_interrupt_in_report_status(h, out.ptr, out.len));
}
pub fn pollInterruptInReport(h: *EndpointHandle, out: []u8) bool {
    return pollInterruptInReportStatus(h, out) == .report;
}
extern fn r4os_rust_usb_xhci_bulk_out([*]const u8, usize) bool;
pub fn bulkOut(bytes: []const u8) bool {
    return r4os_rust_usb_xhci_bulk_out(bytes.ptr, bytes.len);
}
extern fn r4os_rust_usb_xhci_bulk_out_for_handle(*EndpointHandle, [*]const u8, usize) bool;
pub fn bulkOutForHandle(h: *EndpointHandle, bytes: []const u8) bool {
    return r4os_rust_usb_xhci_bulk_out_for_handle(h, bytes.ptr, bytes.len);
}
extern fn r4os_rust_usb_xhci_bulk_in([*]u8, usize) bool;
pub fn bulkIn(bytes: []u8) bool {
    return r4os_rust_usb_xhci_bulk_in(bytes.ptr, bytes.len);
}
extern fn r4os_rust_usb_xhci_bulk_in_for_handle(*EndpointHandle, [*]u8, usize) bool;
pub fn bulkInForHandle(h: *EndpointHandle, bytes: []u8) bool {
    return r4os_rust_usb_xhci_bulk_in_for_handle(h, bytes.ptr, bytes.len);
}

export fn r4os_zig_usb_xhci_incident_begin() u64 {
    return diag_screen.beginResolvableIncident().generation;
}
export fn r4os_zig_usb_xhci_incident_resolve(generation: u64) bool {
    return diag_screen.resolveIncident(.{ .generation = generation });
}
export fn r4os_zig_usb_xhci_diag_write(p: [*]const u8, n: usize) void {
    diag_screen.write(p[0..n]);
}
export fn r4os_zig_usb_xhci_diag_dec(v: u64) void {
    diag_screen.writeDec(v);
}
export fn r4os_zig_usb_xhci_diag_hex(v: u64) void {
    diag_screen.writeHex(v);
}
export fn r4os_zig_usb_xhci_diag_end() void {
    diag_screen.endLine();
}
export fn r4os_zig_usb_xhci_diag_line(p: [*]const u8, n: usize) void {
    diag_screen.line(p[0..n]);
}
