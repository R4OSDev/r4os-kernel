const bootlog = @import("../../kernel/bootlog.zig");
const diag_screen = @import("../../kernel/diag_screen.zig");
const k = @import("../../kernel/log.zig");
const paging = @import("../../memory/paging.zig");
const phys = @import("../../memory/phys.zig");
const pcie = @import("../../platform/pcie.zig");
const sync = @import("../../sched/sync.zig");
const timer = @import("../../kernel/timer.zig");
const usb_core = @import("core.zig");
const usb_host = @import("host_controller.zig");
const event_router = @import("xhci_event_router.zig");
const ring_cycle = @import("xhci_ring_cycle.zig");
const bulk_completion = @import("xhci_bulk_completion.zig");
const endpoint_context = @import("xhci_endpoint_context.zig");
const endpoint_recovery = @import("xhci_endpoint_recovery.zig");
const usb_timing = @import("usb_boot_timing.zig");
const usb_wait = @import("usb_boot_wait.zig");

const MAP_BYTES: u64 = 0x10000;
const MAX_PORTS: usize = 16;
const MAX_USB_DEVICES: usize = usb_core.MAX_USB_DEVICES;
const MAX_USB_CONFIGS: usize = 1;
const MAX_USB_INTERFACES: usize = 8;
const MAX_USB_ENDPOINTS: usize = 16;
const MAX_STRING_CHARS: usize = 32;
const MAX_SCRATCHPADS: usize = 32;
// Last-resort finite bound if both the IRQ timer and TSC are broken. Normal
// timeouts are owned by usb_wait.Deadline and never by this CPU-speed loop.
const COMMAND_WAIT_GUARD: u32 = 4_000_000_000;
const CONTROLLER_OWNERSHIP_TIMEOUT_TICKS: u64 = 15 * @as(u64, timer.DEFAULT_HZ);
const TimeoutClock = enum { ticks, hpet, tsc, tsc_fallback, recovery_budget, cpu_guard };

const COMMAND_TRB_COUNT: usize = 256;
const EVENT_TRB_COUNT: usize = 256;

// `current`, the active runtime's mapped rings, the shared event ring and the
// command owner are one controller transaction domain. USBMSC runs in the
// block worker while HID runs in a high-priority poll task, so timer
// preemption must not let either task switch the active runtime halfway
// through the other's CBW/data/CSW or endpoint-recovery sequence.
//
// UnwindGuard is recursive (the protocol layers call nested xHCI helpers),
// generation-safe, usable during single-threaded boot and prevents hard kill
// until the exact outer owner has executed its defer.
var controller_ownership = sync.UnwindGuard.init("xhci-controller");

pub fn acquireControllerOwnership() bool {
    if (controller_ownership.enter(CONTROLLER_OWNERSHIP_TIMEOUT_TICKS)) return true;
    const incident_token = diag_screen.beginResolvableIncident();
    diag_screen.line("[XHCI] controller ownership timeout");
    _ = diag_screen.resolveIncident(incident_token);
    k.puts("[XHCI] controller ownership timeout\r\n");
    return false;
}

pub fn releaseControllerOwnership() void {
    if (!controller_ownership.leave()) {
        const incident_token = diag_screen.beginResolvableIncident();
        diag_screen.line("[XHCI] controller ownership release mismatch");
        _ = diag_screen.resolveIncident(incident_token);
        k.puts("[XHCI] controller ownership release mismatch\r\n");
    }
}

const PCI_COMMAND_MEMORY: u16 = 1 << 1;
const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;

const CAP_CAPLENGTH: u64 = 0x00;
const CAP_HCSPARAMS1: u64 = 0x04;
const CAP_HCSPARAMS2: u64 = 0x08;
const CAP_HCSPARAMS3: u64 = 0x0C;
const CAP_HCCPARAMS1: u64 = 0x10;
const CAP_DBOFF: u64 = 0x14;
const CAP_RTSOFF: u64 = 0x18;
const CAP_HCCPARAMS2: u64 = 0x1C;

const OP_USBCMD: u64 = 0x00;
const OP_USBSTS: u64 = 0x04;
const OP_PAGESIZE: u64 = 0x08;
const OP_DNCTRL: u64 = 0x14;
const OP_CRCR: u64 = 0x18;
const OP_DCBAAP: u64 = 0x30;
const OP_CONFIG: u64 = 0x38;
const OP_PORT_BASE: u64 = 0x400;
const OP_PORT_STRIDE: u64 = 0x10;

const RT_MFINDEX: u64 = 0x00;
const RT_IR0_IMAN: u64 = 0x20;
const RT_IR0_IMOD: u64 = 0x24;
const RT_IR0_ERSTSZ: u64 = 0x28;
const RT_IR0_ERSTBA: u64 = 0x30;
const RT_IR0_ERDP: u64 = 0x38;

const XCAP_ID_LEGACY: u8 = 1;
const USBLEGSUP_BIOS_OWNED: u32 = 1 << 16;
const USBLEGSUP_OS_OWNED: u32 = 1 << 24;

const USBCMD_RUN: u32 = 1 << 0;
const USBCMD_HCRST: u32 = 1 << 1;
const USBSTS_HCH: u32 = 1 << 0;
const USBSTS_EINT: u32 = 1 << 3;
const USBSTS_PCD: u32 = 1 << 4;
const USBSTS_CNR: u32 = 1 << 11;
const USBSTS_HCE: u32 = 1 << 12;

const TRB_CYCLE: u32 = 1 << 0;
const TRB_TYPE_SHIFT: u5 = 10;
const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_ENABLE_SLOT: u32 = 9;
const TRB_TYPE_DISABLE_SLOT: u32 = 10;
const TRB_TYPE_ADDRESS_DEVICE: u32 = 11;
const TRB_TYPE_CONFIGURE_ENDPOINT: u32 = 12;
const TRB_TYPE_RESET_ENDPOINT: u32 = 14;
const TRB_TYPE_STOP_ENDPOINT: u32 = 15;
const TRB_TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
const TRB_TYPE_NORMAL: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TYPE_COMMAND_COMPLETION_EVENT: u32 = 33;
const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
const TRB_IOC: u32 = 1 << 5;
const TRB_IDT: u32 = 1 << 6;
const SETUP_TRT_IN_DATA: u32 = 3 << 16;
const DATA_STAGE_DIR_IN: u32 = 1 << 16;
const STATUS_STAGE_DIR_IN: u32 = 1 << 16;
const SETUP_TRT_OUT_DATA: u32 = 2 << 16;

const COMPLETION_SUCCESS: u8 = 1;
const COMPLETION_STALL_ERROR: u8 = 6;
const COMPLETION_SHORT_PACKET: u8 = 13;
const COMPLETION_EVENT_RING_FULL_ERROR: u8 = 21;
const ENDPOINT_RECOVERY_COMMAND_ATTEMPTS: u8 = 6;

const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PED: u32 = 1 << 1;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_PLS_MASK: u32 = 0x0F << 5;
const PORTSC_PLS_U0: u32 = 0;
const PORTSC_PP: u32 = 1 << 9;
const PORTSC_PIC_MASK: u32 = 0x3 << 14;
const PORTSC_WPR: u32 = 1 << 31;
const PORTSC_CSC: u32 = 1 << 17;
const PORTSC_CHANGE_MASK: u32 = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 21) | (1 << 22) | (1 << 23);
const PORTSC_WRITE_PRESERVE_MASK: u32 = PORTSC_PP | PORTSC_PIC_MASK;

const IMAN_IP: u32 = 1 << 0;
const IMAN_IE: u32 = 1 << 1;
const ERDP_EHB: u64 = 1 << 3;

const CRCR_RCS: u64 = 1 << 0;

const COMMAND_RING_BYTES: usize = @intCast(phys.FRAME_SIZE);
const EVENT_RING_BYTES: usize = @intCast(phys.FRAME_SIZE);
const ERST_BYTES: usize = @intCast(phys.FRAME_SIZE);
const DCBAA_BYTES: usize = @intCast(phys.FRAME_SIZE);
const DEVICE_CONTEXT_BYTES: usize = @intCast(phys.FRAME_SIZE);
const INPUT_CONTEXT_BYTES: usize = @intCast(phys.FRAME_SIZE);
const EP0_RING_BYTES: usize = @intCast(phys.FRAME_SIZE);
const INTERRUPT_RING_BYTES: usize = @intCast(phys.FRAME_SIZE);
const INTERRUPT_BUFFER_BYTES: usize = @intCast(phys.FRAME_SIZE);
const DESCRIPTOR_BYTES: usize = @intCast(phys.FRAME_SIZE);
const TRANSFER_TRB_COUNT: usize = 256;
const DEVICE_DESCRIPTOR_LEN: u16 = 18;
const CONFIG_DESCRIPTOR_HEADER_LEN: u16 = 9;
const USB_REQ_CLEAR_FEATURE: u8 = 0x01;
const USB_REQ_GET_DESCRIPTOR: u8 = 0x06;
const USB_REQ_SET_CONFIGURATION: u8 = 0x09;
const USB_FEATURE_ENDPOINT_HALT: u16 = 0x0000;
const HID_REQ_SET_IDLE: u8 = 0x0A;
const HID_REQ_SET_PROTOCOL: u8 = 0x0B;
const MSC_REQ_BULK_ONLY_RESET: u8 = 0xFF;
const USB_DESC_DEVICE: u8 = 0x01;
const USB_DESC_CONFIGURATION: u8 = 0x02;
const USB_DESC_STRING: u8 = 0x03;
const USB_DESC_INTERFACE: u8 = 0x04;
const USB_DESC_ENDPOINT: u8 = 0x05;
const USB_DESC_REPORT: u8 = 0x22;
const USB_DESC_HID: u8 = 0x21;
const USB_DESC_SS_ENDPOINT_COMPANION: u8 = 0x30;

const Trb = extern struct {
    parameter: u64,
    status: u32,
    control: u32,
};

const ErstEntry = extern struct {
    base: u64,
    size: u32,
    reserved: u32,
};

pub const PortStatus = struct {
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
    reset_reason: []const u8 = "not-started",
};

const PortPreparePhase = enum {
    startup,
    probe,
};

const ProbeRecord = struct {
    active: bool = false,
    port: u8 = 0,
    slot: u8 = 0,
    speed: u8 = 0,
    pls: u8 = 0,
    stage: []const u8 = "none",
    command_cc: u8 = 0,
    transfer_cc: u8 = 0,
    class_code: u8 = 0,
    subclass: u8 = 0,
    protocol: u8 = 0,
    vid: u16 = 0,
    pid: u16 = 0,
};

const DescriptorInterface = struct {
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

const ControlDirection = enum {
    none,
    in,
    out,
};

pub const InterruptPollResult = enum {
    report,
    no_report,
    failed,
};

const ControlRequest = struct {
    request_type: u8 = 0,
    request: u8 = 0,
    value: u16 = 0,
    index_value: u16 = 0,
    length: u16 = 0,
    data_phys: u64 = 0,
    direction: ControlDirection = .none,
};

const DeviceRuntime = struct {
    active: bool = false,
    slot_id: u8 = 0,
    port: u8 = 0,
    speed: u8 = 0,
    config_value: u8 = 0,
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
};

pub const EndpointKind = enum {
    control,
    interrupt_in,
    bulk_in,
    bulk_out,
};

pub const DeviceHandle = struct {
    controller: []const u8 = "xhci",
    port: u8 = 0,
    slot_id: u8 = 0,
    speed: u8 = 0,
    config_value: u8 = 0,
    vendor_id: u16 = 0,
    product_id: u16 = 0,
};

pub const EndpointHandle = struct {
    device: DeviceHandle = .{},
    kind: EndpointKind = .control,
    address: u8 = 0,
    endpoint_id: u8 = 0,
    max_packet: u16 = 0,
    max_burst: u8 = 0,
    interval: u8 = 0,
};

pub const Status = struct {
    probed: bool = false,
    present: bool = false,
    mapped: bool = false,
    device: pcie.Device = .{},
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
    first_ports: [MAX_PORTS]PortStatus = .{PortStatus{}} ** MAX_PORTS,
    legacy_cap_offset: u32 = 0,
    legacy_usblegsup_before: u32 = 0,
    legacy_usblegsup_after: u32 = 0,
    bios_handoff: []const u8 = "not-started",
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
    last_wait_reason: []const u8 = "none",
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
    // 0.56.16: Endpoint-Halt-Recovery (Port der Backup-XHCI.R4D-Phase-4).
    // recoveries: RESET_ENDPOINT+SET_TR_DEQUEUE nach Fehler-Completion oder
    // Halted-Befund; pending_timeouts: Anteil davon aus dem proaktiven
    // Pending-Check; halted_checks: EP-State-Lesungen aus dem Output-
    // Device-Context. Im fehlerfreien QEMU-Lauf bleiben recoveries und
    // pending_timeouts 0 (idle Pending ist bei HID normal, SET_IDLE=0).
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
    last_control_direction: []const u8 = "none",
    last_control_completion_code: u8 = 0,
    last_control_residue: u32 = 0,
    last_control_ok: bool = false,
    transfer_events: u64 = 0,
    bulk_transfers: u64 = 0,
    bulk_failures: u64 = 0,
    last_bulk_direction: []const u8 = "none",
    last_bulk_result: []const u8 = "none",
    last_bulk_request_len: u32 = 0,
    last_bulk_residue: u32 = 0,
    last_bulk_actual_len: u32 = 0,
    last_bulk_completion_code: u8 = 0,
    ports_probed: u8 = 0,
    non_hid_devices_skipped: u8 = 0,
    probe_record_count: u8 = 0,
    probe_records: [MAX_PORTS]ProbeRecord = .{ProbeRecord{}} ** MAX_PORTS,
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
    interface_records: [MAX_USB_INTERFACES]usb_core.Interface = .{usb_core.Interface{}} ** MAX_USB_INTERFACES,
    endpoint_count: u8 = 0,
    descriptor_records: u8 = 0,
    descriptor_unknown: u8 = 0,
    descriptor_malformed: u8 = 0,
    hid_descriptor_count: u8 = 0,
    ss_endpoint_companion_count: u8 = 0,
    selected_interface_reason: []const u8 = "none",
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
    manufacturer_string: [MAX_STRING_CHARS]u8 = .{0} ** MAX_STRING_CHARS,
    product_string: [MAX_STRING_CHARS]u8 = .{0} ** MAX_STRING_CHARS,
    serial_string: [MAX_STRING_CHARS]u8 = .{0} ** MAX_STRING_CHARS,
    connected_ports: u8 = 0,
    enabled_ports: u8 = 0,
    reset_ports: u8 = 0,
    failures: u64 = 0,
    timeouts: u64 = 0,
    reason: []const u8 = "not initialized",
};

var current: Status = .{};
var command_ring_virt: u64 = 0;
var event_ring_virt: u64 = 0;
var erst_virt: u64 = 0;
var dcbaa_virt: u64 = 0;
var scratchpad_array_virt: u64 = 0;
var scratchpad_frames: [MAX_SCRATCHPADS]u64 = .{0} ** MAX_SCRATCHPADS;
var first_device_virt: u64 = 0;
var first_input_virt: u64 = 0;
var first_ep0_ring_virt: u64 = 0;
var first_interrupt_ring_virt: u64 = 0;
var first_interrupt_buffer_virt: u64 = 0;
var first_bulk_in_ring_virt: u64 = 0;
var first_bulk_out_ring_virt: u64 = 0;
var first_bulk_buffer_virt: u64 = 0;
var first_descriptor_virt: u64 = 0;
var runtimes: [MAX_USB_DEVICES]DeviceRuntime = .{DeviceRuntime{}} ** MAX_USB_DEVICES;
var active_runtime_index: ?usize = null;
var deferred_events = event_router.Mailbox.init();
var active_sync_transfer: ?event_router.Match = null;
var last_timed_out_sync_transfer: ?event_router.Match = null;
var last_sync_transfer_incident: diag_screen.IncidentToken = .{};
var active_command: ?event_router.Match = null;

pub fn probe() bool {
    closeLastSyncTransferIncident();
    current = .{ .probed = true };
    usb_wait.resetMetrics();
    command_ring_virt = 0;
    event_ring_virt = 0;
    erst_virt = 0;
    dcbaa_virt = 0;
    scratchpad_array_virt = 0;
    scratchpad_frames = .{0} ** MAX_SCRATCHPADS;
    first_device_virt = 0;
    first_input_virt = 0;
    first_ep0_ring_virt = 0;
    first_interrupt_ring_virt = 0;
    first_interrupt_buffer_virt = 0;
    first_bulk_in_ring_virt = 0;
    first_bulk_out_ring_virt = 0;
    first_bulk_buffer_virt = 0;
    first_descriptor_virt = 0;
    runtimes = .{DeviceRuntime{}} ** MAX_USB_DEVICES;
    active_runtime_index = null;
    deferred_events.reset();
    active_sync_transfer = null;
    last_timed_out_sync_transfer = null;
    last_sync_transfer_incident = .{};
    active_command = null;
    usb_core.reset();
    _ = usb_host.registerBuiltIn("XHCI");
    const ps = pcie.status();
    if (ps.xhci_count == 0) {
        current.reason = "no xHCI controller found";
        current.bios_handoff = "not-needed";
        bootlog.puts("[XHCI] not found\r\n");
        return false;
    }

    current.present = true;
    current.device = ps.first_xhci;
    current.command_before = pcie.readCommand(current.device);
    current.bar0_raw = pcie.readBar(current.device, 0);
    current.bar1_raw = pcie.readBar(current.device, 1);
    current.bar_is_io = (current.bar0_raw & 0x1) != 0;
    current.bar_is_64 = ((current.bar0_raw >> 1) & 0x3) == 0x2;
    if (current.bar_is_io) {
        current.failures += 1;
        current.reason = "BAR0 is I/O space, expected MMIO";
        bootlog.puts("[XHCI][WARN] BAR0 is I/O space\r\n");
        return false;
    }

    current.mmio_phys = pcie.readBar64(current.device, 0) & 0xFFFF_FFFF_FFFF_FFF0;
    if (current.mmio_phys == 0) {
        current.failures += 1;
        current.reason = "BAR0 MMIO base is zero";
        bootlog.puts("[XHCI][WARN] BAR0 MMIO base is zero\r\n");
        return false;
    }
    if (!mapMmio(current.mmio_phys, MAP_BYTES)) {
        current.failures += 1;
        current.reason = "failed to map xHCI MMIO";
        bootlog.puts("[XHCI][WARN] MMIO map failed\r\n");
        return false;
    }

    current.mapped = true;
    current.mmio_virt = phys.physToVirt(current.mmio_phys);
    enablePciMemoryAndBusMaster();
    readCapabilities();
    handleBiosHandoff();
    readOperational();
    readRuntime();
    readPorts();
    _ = initController();
    readOperational();
    readRuntime();
    readPorts();
    current.reason = if (current.get_config_ok)
        "xHCI controller running; first USB configuration parsed"
    else if (current.get_descriptor_ok)
        "xHCI controller running; first USB device descriptor read"
    else if (current.address_device_ok)
        "xHCI controller running; first USB device addressed"
    else if (current.controller_running)
        "xHCI controller running; USB enumeration core active"
    else if (current.dma_ready)
        "xHCI DMA prepared; controller did not reach running state"
    else
        "diagnostic MMIO mapped; xHCI init incomplete";
    logSummary();
    return true;
}

pub fn status() Status {
    var result = current;
    const waits = usb_wait.metrics();
    result.wait_calls = waits.calls;
    result.wait_successes = waits.successes;
    result.wait_timeouts = waits.timeouts;
    result.wait_early_calls = waits.early_calls;
    result.wait_runtime_calls = waits.runtime_calls;
    result.wait_requested_ms = waits.requested_ms;
    result.wait_elapsed_ns = waits.elapsed_ns;
    result.wait_max_elapsed_ns = waits.max_elapsed_ns;
    result.wait_iterations = waits.iterations;
    result.wait_blocked_ticks = waits.blocked_ticks;
    result.last_wait_reason = waits.last_reason;
    result.last_wait_phase = @intFromEnum(waits.last_phase);
    result.last_wait_requested_ms = waits.last_requested_ms;
    result.last_wait_elapsed_ns = waits.last_elapsed_ns;
    result.last_wait_iterations = waits.last_iterations;
    result.last_wait_blocked_ticks = waits.last_blocked_ticks;
    result.last_wait_retry = waits.last_retry;
    result.last_wait_success = waits.last_success;
    if (first_interrupt_ring_virt != 0) {
        const ring: [*]const Trb = @ptrFromInt(first_interrupt_ring_virt);
        result.interrupt_link_cycle = @intFromBool((ring[TRANSFER_TRB_COUNT - 1].control & TRB_CYCLE) != 0);
    }
    if (result.interrupt_pending and
        result.interrupt_pending_trb_phys >= result.interrupt_ring_phys)
    {
        const offset = result.interrupt_pending_trb_phys - result.interrupt_ring_phys;
        if ((offset % @sizeOf(Trb)) == 0) {
            const index = offset / @sizeOf(Trb);
            if (index < TRANSFER_TRB_COUNT - 1) result.interrupt_pending_index = @intCast(index);
        }
    }
    if (event_ring_virt != 0 and result.event_dequeue < EVENT_TRB_COUNT) {
        const event_addr = event_ring_virt + @as(u64, result.event_dequeue) * @sizeOf(Trb);
        const control = volatileRead32(event_addr + 12);
        result.event_raw_cycle = @intFromBool((control & TRB_CYCLE) != 0);
        result.event_raw_type = @truncate((control >> TRB_TYPE_SHIFT) & 0x3F);
        result.event_raw_code = @truncate(volatileRead32(event_addr + 8) >> 24);
    }
    result.interrupt_hw_dequeue = readInterruptHardwareDequeue();
    if (result.mapped and result.mmio_virt != 0) {
        result.usbsts = readOp32(OP_USBSTS);
        result.iman0 = readRt32(RT_IR0_IMAN);
        result.erdp0 = readRt64(RT_IR0_ERDP);
    }
    const deferred = deferred_events.snapshot();
    result.deferred_events_pending = deferred.pending;
    result.deferred_events_queued = deferred.queued;
    result.deferred_events_delivered = deferred.delivered;
    result.deferred_events_overflows = deferred.overflows;
    result.deferred_events_purged = deferred.purged;
    result.deferred_events_high_water = deferred.high_water;
    return result;
}

pub fn deviceHandleFromCore(dev: *const usb_core.Device) DeviceHandle {
    return .{
        .controller = dev.controller,
        .port = dev.port,
        .slot_id = dev.slot_id,
        .speed = dev.speed,
        .config_value = dev.config_value,
        .vendor_id = dev.vendor_id,
        .product_id = dev.product_id,
    };
}

pub fn interruptInHandleFromCore(dev: *const usb_core.Device) EndpointHandle {
    return .{
        .device = deviceHandleFromCore(dev),
        .kind = .interrupt_in,
        .address = dev.first_endpoint_address,
        .endpoint_id = endpointDci(dev.first_endpoint_address),
        .max_packet = dev.first_endpoint_max_packet,
        .interval = dev.first_endpoint_interval,
    };
}

pub fn bulkInHandleFromCore(dev: *const usb_core.Device) EndpointHandle {
    return .{
        .device = deviceHandleFromCore(dev),
        .kind = .bulk_in,
        .address = dev.bulk_in_endpoint_address,
        .endpoint_id = endpointDci(dev.bulk_in_endpoint_address),
        .max_packet = dev.bulk_in_endpoint_max_packet,
        .max_burst = dev.bulk_in_endpoint_max_burst,
    };
}

pub fn bulkOutHandleFromCore(dev: *const usb_core.Device) EndpointHandle {
    return .{
        .device = deviceHandleFromCore(dev),
        .kind = .bulk_out,
        .address = dev.bulk_out_endpoint_address,
        .endpoint_id = endpointDci(dev.bulk_out_endpoint_address),
        .max_packet = dev.bulk_out_endpoint_max_packet,
        .max_burst = dev.bulk_out_endpoint_max_burst,
    };
}

pub fn selectDeviceHandle(handle: *DeviceHandle) bool {
    if (handle.port == 0) return false;
    refreshPortsAndReclaim();
    if (!portIsConnected(handle.port)) return false;
    if (handle.slot_id != 0) {
        if (!activateRuntimeByIdentity(handle.slot_id, handle.port)) return false;
        if (current.addressed_slot_id != handle.slot_id or current.addressed_port != handle.port) return false;
        refreshHandleFromCurrent(handle);
        return true;
    }
    if (!selectDeviceByPort(handle.port)) return false;
    refreshHandleFromCurrent(handle);
    return true;
}

fn refreshHandleFromCurrent(handle: *DeviceHandle) void {
    handle.controller = "xhci";
    handle.slot_id = current.addressed_slot_id;
    handle.port = current.addressed_port;
    handle.speed = current.addressed_speed;
    handle.config_value = current.config_value;
    handle.vendor_id = current.device_vendor_id;
    handle.product_id = current.device_product_id;
}

fn sameDeviceTarget(a: DeviceHandle, b: DeviceHandle) bool {
    if (a.port == 0 or b.port == 0 or a.slot_id == 0 or b.slot_id == 0) return false;
    return a.port == b.port and a.slot_id == b.slot_id;
}

fn enablePciMemoryAndBusMaster() void {
    const wanted = current.command_before | PCI_COMMAND_MEMORY | PCI_COMMAND_BUS_MASTER;
    pcie.writeCommand(current.device, wanted);
    current.command_after = pcie.readCommand(current.device);
}

fn readCapabilities() void {
    current.caplength = read8(CAP_CAPLENGTH);
    current.hciversion = read16(CAP_CAPLENGTH + 2);
    current.hcsparams1 = read32(CAP_HCSPARAMS1);
    current.hcsparams2 = read32(CAP_HCSPARAMS2);
    current.hcsparams3 = read32(CAP_HCSPARAMS3);
    current.hccparams1 = read32(CAP_HCCPARAMS1);
    current.dboff = read32(CAP_DBOFF);
    current.rtsoff = read32(CAP_RTSOFF);
    current.hccparams2 = read32(CAP_HCCPARAMS2);
    current.max_slots = @truncate(current.hcsparams1 & 0xFF);
    current.max_interrupters = @truncate((current.hcsparams1 >> 8) & 0x7FF);
    current.max_ports = @truncate((current.hcsparams1 >> 24) & 0xFF);
    current.op_virt = current.mmio_virt + current.caplength;
    current.runtime_virt = current.mmio_virt + (current.rtsoff & 0xFFFF_FFE0);
}

fn handleBiosHandoff() void {
    const xecp_dwords = (current.hccparams1 >> 16) & 0xFFFF;
    if (xecp_dwords == 0) {
        current.bios_handoff = "no-extended-caps";
        return;
    }
    var offset: u32 = xecp_dwords * 4;
    var guard: u32 = 0;
    while (offset >= 0x20 and offset + 4 <= MAP_BYTES and guard < 32) : (guard += 1) {
        const cap = read32(offset);
        const cap_id: u8 = @truncate(cap & 0xFF);
        const next: u8 = @truncate((cap >> 8) & 0xFF);
        if (cap_id == XCAP_ID_LEGACY) {
            current.legacy_cap_offset = offset;
            current.legacy_usblegsup_before = read32(offset);
            if ((current.legacy_usblegsup_before & USBLEGSUP_BIOS_OWNED) == 0) {
                current.legacy_usblegsup_after = current.legacy_usblegsup_before;
                current.bios_handoff = "not-needed";
                return;
            }
            write32(offset, current.legacy_usblegsup_before | USBLEGSUP_OS_OWNED);
            var wait = usb_wait.Wait.begin(
                usb_timing.XHCI_BIOS_HANDOFF_TIMEOUT_MS,
                "xhci-bios-handoff",
                0,
            );
            while ((read32(offset) & USBLEGSUP_BIOS_OWNED) != 0 and !wait.expired()) wait.idle();
            current.legacy_usblegsup_after = read32(offset);
            const released = (current.legacy_usblegsup_after & USBLEGSUP_BIOS_OWNED) == 0;
            _ = wait.finish(released);
            if (released) {
                current.bios_handoff = "ok";
            } else {
                current.timeouts += 1;
                current.bios_handoff = "timeout";
            }
            return;
        }
        if (next == 0) break;
        offset += @as(u32, next) * 4;
    }
    current.bios_handoff = "no-legacy-cap";
}

fn initController() bool {
    current.controller_halted_before = (readOp32(OP_USBSTS) & USBSTS_HCH) != 0;
    if (!stopController()) return false;
    if (!resetController()) return false;
    readCapabilities();
    if (!allocControllerMemory()) return false;
    setupRingsAndContexts();
    if (!runController()) return false;
    readPorts();
    resetConnectedPorts();
    _ = enumerateFirstConnectedDevice();
    return true;
}

fn stopController() bool {
    var cmd = readOp32(OP_USBCMD);
    cmd &= ~USBCMD_RUN;
    writeOp32(OP_USBCMD, cmd);
    if (!waitOpSet(
        OP_USBSTS,
        USBSTS_HCH,
        usb_timing.XHCI_CONTROLLER_HALT_TIMEOUT_MS,
        "xhci-controller-halt",
    )) {
        current.timeouts += 1;
        current.reason = "timeout waiting for xHCI halt";
        return false;
    }
    current.controller_stopped = true;
    return true;
}

fn resetController() bool {
    writeOp32(OP_USBCMD, readOp32(OP_USBCMD) | USBCMD_HCRST);
    if (!waitOpClear(
        OP_USBCMD,
        USBCMD_HCRST,
        usb_timing.XHCI_CONTROLLER_RESET_TIMEOUT_MS,
        "xhci-controller-reset",
    )) {
        current.timeouts += 1;
        current.reason = "timeout waiting for xHCI reset bit clear";
        return false;
    }
    if (!waitOpClear(
        OP_USBSTS,
        USBSTS_CNR,
        usb_timing.XHCI_CONTROLLER_READY_TIMEOUT_MS,
        "xhci-controller-ready",
    )) {
        current.timeouts += 1;
        current.reason = "timeout waiting for controller not ready";
        return false;
    }
    writeOp32(OP_USBSTS, USBSTS_EINT | USBSTS_PCD | USBSTS_HCE);
    current.controller_reset_ok = true;
    return true;
}

fn allocControllerMemory() bool {
    current.dcbaa_phys = allocFrameZero() orelse return failAlloc("dcbaa");
    current.command_ring_phys = allocFrameZero() orelse return failAlloc("command ring");
    current.event_ring_phys = allocFrameZero() orelse return failAlloc("event ring");
    current.erst_phys = allocFrameZero() orelse return failAlloc("erst");
    dcbaa_virt = phys.physToVirt(current.dcbaa_phys);
    command_ring_virt = phys.physToVirt(current.command_ring_phys);
    event_ring_virt = phys.physToVirt(current.event_ring_phys);
    erst_virt = phys.physToVirt(current.erst_phys);
    current.context_size = if ((current.hccparams1 & (1 << 2)) != 0) 64 else 32;
    if (!allocScratchpads()) return false;
    current.dma_ready = true;
    return true;
}

fn failAlloc(name: []const u8) bool {
    _ = name;
    current.failures += 1;
    current.reason = "failed to allocate xHCI DMA frame";
    return false;
}

fn allocScratchpads() bool {
    const count = scratchpadCount();
    current.scratchpad_count = count;
    if (count == 0) return true;
    if (count > MAX_SCRATCHPADS) {
        current.failures += 1;
        current.reason = "too many xHCI scratchpad buffers";
        return false;
    }
    current.scratchpad_array_phys = allocFrameZero() orelse return failAlloc("scratchpad array");
    scratchpad_array_virt = phys.physToVirt(current.scratchpad_array_phys);
    const entries: [*]u64 = @ptrFromInt(scratchpad_array_virt);
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const frame = allocFrameZero() orelse return failAlloc("scratchpad");
        scratchpad_frames[i] = frame;
        entries[i] = frame;
    }
    const dcbaa: [*]u64 = @ptrFromInt(dcbaa_virt);
    dcbaa[0] = current.scratchpad_array_phys;
    return true;
}

fn setupRingsAndContexts() void {
    initCommandRing();
    initEventRing();
    // Alle Ring-/ERST-Schreibvorgaenge muessen sichtbar sein, bevor ERSTBA
    // die interne Event-Ring-State-Machine des xHC startet.
    dmaFence();
    current.max_slots_enabled = current.max_slots;
    writeOp64(OP_DCBAAP, current.dcbaa_phys);
    writeOp64(OP_CRCR, current.command_ring_phys | CRCR_RCS);
    writeOp32(OP_CONFIG, current.max_slots_enabled);
    writeRt32(RT_IR0_IMOD, 0);
    writeRt32(RT_IR0_ERSTSZ, 1);
    writeRt64(RT_IR0_ERDP, current.event_ring_phys);
    // xHCI 4.9.4: ERSTBA zuletzt; dieser Write setzt die Event-Ring-
    // State-Machine in den Startzustand. R4OS pollt, daher bleibt IE aus.
    writeRt64(RT_IR0_ERSTBA, current.erst_phys);
    writeRt32(RT_IR0_IMAN, IMAN_IP);
    _ = readRt32(RT_IR0_IMAN);
    current.crcr = readOp64(OP_CRCR);
    current.dcbaap = readOp64(OP_DCBAAP);
    current.config = readOp32(OP_CONFIG);
    current.erstsz0 = readRt32(RT_IR0_ERSTSZ);
    current.erstba0 = readRt64(RT_IR0_ERSTBA);
    current.erdp0 = readRt64(RT_IR0_ERDP);
    current.iman0 = readRt32(RT_IR0_IMAN);
    current.imod0 = readRt32(RT_IR0_IMOD);
}

fn initCommandRing() void {
    const ring: [*]Trb = @ptrFromInt(command_ring_virt);
    var i: usize = 0;
    while (i < COMMAND_TRB_COUNT) : (i += 1) ring[i] = .{ .parameter = 0, .status = 0, .control = 0 };
    ring[COMMAND_TRB_COUNT - 1] = .{
        .parameter = current.command_ring_phys,
        .status = 0,
        .control = trbType(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE,
    };
    current.command_enqueue = 0;
    current.command_cycle = 1;
    current.command_ring_wraps = 0;
}

fn initEventRing() void {
    deferred_events.reset();
    active_sync_transfer = null;
    last_timed_out_sync_transfer = null;
    active_command = null;
    const ring: [*]Trb = @ptrFromInt(event_ring_virt);
    var i: usize = 0;
    while (i < EVENT_TRB_COUNT) : (i += 1) ring[i] = .{ .parameter = 0, .status = 0, .control = 0 };
    const erst: [*]ErstEntry = @ptrFromInt(erst_virt);
    erst[0] = .{ .base = current.event_ring_phys, .size = EVENT_TRB_COUNT, .reserved = 0 };
    current.event_dequeue = 0;
    current.event_cycle = 1;
    current.event_ring_wraps = 0;
    current.event_erdp_commits = 0;
    current.event_max_batch = 0;
}

fn runController() bool {
    writeOp32(OP_USBSTS, USBSTS_EINT | USBSTS_PCD | USBSTS_HCE);
    writeOp32(OP_USBCMD, readOp32(OP_USBCMD) | USBCMD_RUN);
    if (!waitOpClear(
        OP_USBSTS,
        USBSTS_HCH,
        usb_timing.XHCI_CONTROLLER_RUN_TIMEOUT_MS,
        "xhci-controller-run",
    )) {
        current.timeouts += 1;
        current.reason = "timeout waiting for xHCI run";
        return false;
    }
    current.controller_running = true;
    return true;
}

fn resetConnectedPorts() void {
    current.connected_ports = 0;
    current.enabled_ports = 0;
    current.reset_ports = 0;
    var i: usize = 0;
    while (i < @as(usize, current.port_count_seen)) : (i += 1) {
        var p = current.first_ports[i];
        _ = preparePortState(i, &p, .startup);
        if (p.connected) current.connected_ports += 1;
        if (p.enabled) current.enabled_ports += 1;
        current.first_ports[i] = p;
    }
}

fn refreshPortStatus(p: *PortStatus, index_value: usize) void {
    const base = OP_PORT_BASE + (@as(u64, index_value) * OP_PORT_STRIDE);
    p.portsc = readOp32(base);
    p.portpmsc = readOp32(base + 0x04);
    p.portli = readOp32(base + 0x08);
    p.porthlpmc = readOp32(base + 0x0C);
    p.change_bits = p.portsc & PORTSC_CHANGE_MASK;
    p.link_state = @truncate(portLinkState(p.portsc));
    p.connected = (p.portsc & PORTSC_CCS) != 0;
    p.enabled = (p.portsc & PORTSC_PED) != 0;
    p.powered = (p.portsc & PORTSC_PP) != 0;
}

fn preparePortState(index: usize, p: *PortStatus, phase: PortPreparePhase) bool {
    refreshPortStatus(p, index);
    const was_debounced = p.debounce_ok;
    const initial_changes = clearPortChanges(index);
    if (initial_changes != 0) {
        p.change_bits |= initial_changes;
        refreshPortStatus(p, index);
    }
    if (!p.connected) {
        p.debounce_ok = false;
        p.reset_reason = "no-device";
        return false;
    }
    p.debounce_ok = was_debounced and (initial_changes & PORTSC_CSC) == 0;
    if (!p.debounce_ok and !debounceConnectedPort(index, p)) {
        p.reset_reason = phaseReason(phase, "debounce-timeout", "probe-debounce-timeout");
        current.port_debounce_failures += 1;
        return false;
    }
    if (!ensurePortPower(index, p)) {
        p.reset_reason = phaseReason(phase, "power-timeout", "probe-power-timeout");
        return false;
    }
    // HCRST deliberately does not reset downstream ports. In particular a
    // SuperSpeed port may survive the firmware handoff as Enabled/U0 while
    // its device still owns the firmware-assigned USB address. Every startup
    // enumeration therefore performs one real port reset even in U0.
    const force_enumeration_reset = phase == .startup and !p.reset_attempted;
    if (!p.enabled or force_enumeration_reset) {
        p.reset_attempted = true;
        p.reset_ok = resetPort(@intCast(index));
        p.reset_reason = phaseReason(phase, if (p.reset_ok) "reset-enabled" else "reset-timeout", if (p.reset_ok) "probe-reset-enabled" else "probe-reset-timeout");
        if (p.reset_ok) current.reset_ports += 1;
        refreshPortStatus(p, index);
        if (p.reset_ok and p.connected and p.enabled) {
            _ = usb_wait.millisecondsWithReason(
                usb_timing.PORT_RESET_RECOVERY_MS,
                "xhci-port-reset-recovery",
                0,
            );
            refreshPortStatus(p, index);
        }
    }
    if (p.enabled and portLinkState(p.portsc) != PORTSC_PLS_U0) {
        if (waitPortU0(@intCast(index), p)) {
            p.reset_reason = phaseReason(phase, "waited-u0", "probe-waited-u0");
        } else {
            p.reset_attempted = true;
            p.reset_ok = warmResetPort(@intCast(index));
            current.port_warm_resets += 1;
            p.reset_reason = phaseReason(phase, if (p.reset_ok) "warm-reset-u0" else "warm-reset-timeout", if (p.reset_ok) "probe-warm-reset-u0" else "probe-warm-reset-timeout");
            if (p.reset_ok) current.reset_ports += 1;
            refreshPortStatus(p, index);
            if (p.connected and p.enabled and portLinkState(p.portsc) != PORTSC_PLS_U0 and waitPortU0(@intCast(index), p)) {
                p.reset_reason = phaseReason(phase, "warm-reset-waited-u0", "probe-warm-waited-u0");
            }
        }
    } else if (p.connected and p.enabled and p.reset_reason.len == 0) {
        p.reset_reason = phaseReason(phase, "already-enabled", "probe-already-enabled");
    } else if (p.connected and p.enabled and !p.reset_attempted) {
        p.reset_reason = phaseReason(phase, "already-enabled", "probe-already-enabled");
    }
    _ = clearPortChanges(index);
    refreshPortStatus(p, index);
    return p.connected and p.enabled and portLinkState(p.portsc) == PORTSC_PLS_U0;
}

fn phaseReason(phase: PortPreparePhase, startup_reason: []const u8, probe_reason: []const u8) []const u8 {
    return switch (phase) {
        .startup => startup_reason,
        .probe => probe_reason,
    };
}

fn clearPortChanges(index: usize) u32 {
    const base = OP_PORT_BASE + (@as(u64, index) * OP_PORT_STRIDE);
    const portsc = readOp32(base);
    const changes = portsc & PORTSC_CHANGE_MASK;
    if (changes != 0) {
        writeOp32(base, (portsc & PORTSC_WRITE_PRESERVE_MASK) | changes);
        current.port_change_clears += 1;
    }
    return changes;
}

fn debounceConnectedPort(index: usize, p: *PortStatus) bool {
    const base = OP_PORT_BASE + (@as(u64, index) * OP_PORT_STRIDE);
    const observed_mask = PORTSC_CCS | PORTSC_PED | PORTSC_PLS_MASK | PORTSC_PR | PORTSC_WPR;
    var last = readOp32(base);
    var wait = usb_wait.Wait.begin(
        usb_timing.XHCI_PORT_DEBOUNCE_TIMEOUT_MS,
        "xhci-port-debounce",
        0,
    );
    var stable = usb_wait.Deadline.begin(usb_timing.XHCI_PORT_DEBOUNCE_STABLE_MS);
    while (!wait.expired()) {
        const now = readOp32(base);
        if ((now & PORTSC_CCS) == 0) {
            stable.finish();
            _ = wait.finish(false);
            refreshPortStatus(p, index);
            return false;
        }
        if ((now & observed_mask) != (last & observed_mask)) {
            stable.finish();
            stable = usb_wait.Deadline.begin(usb_timing.XHCI_PORT_DEBOUNCE_STABLE_MS);
            last = now;
        } else if (stable.expiredAny()) {
            stable.finish();
            _ = wait.finish(true);
            refreshPortStatus(p, index);
            p.debounce_ok = true;
            return true;
        }
        wait.idle();
    }
    stable.finish();
    _ = wait.finish(false);
    refreshPortStatus(p, index);
    p.debounce_ok = false;
    return false;
}

fn ensurePortPower(index: usize, p: *PortStatus) bool {
    if (p.powered) return true;
    const base = OP_PORT_BASE + (@as(u64, index) * OP_PORT_STRIDE);
    current.port_power_requests += 1;
    const before = readOp32(base);
    writeOp32(base, (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_PP | (before & PORTSC_CHANGE_MASK));
    var wait = usb_wait.Wait.begin(
        usb_timing.XHCI_PORT_POWER_TIMEOUT_MS,
        "xhci-port-power",
        0,
    );
    while (!wait.expired()) {
        refreshPortStatus(p, index);
        if (!p.connected) {
            _ = wait.finish(false);
            return false;
        }
        if (p.powered) {
            _ = wait.finish(true);
            return true;
        }
        wait.idle();
    }
    _ = wait.finish(false);
    current.timeouts += 1;
    return false;
}

fn resetPort(index_value: u8) bool {
    const base = OP_PORT_BASE + (@as(u64, index_value) * OP_PORT_STRIDE);
    const before = readOp32(base);
    _ = clearPortChanges(@intCast(index_value));
    const value = (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_PR;
    writeOp32(base, value);
    var wait = usb_wait.Wait.begin(
        usb_timing.XHCI_PORT_RESET_TIMEOUT_MS,
        "xhci-port-reset",
        0,
    );
    while (!wait.expired()) {
        const portsc = readOp32(base);
        if ((portsc & PORTSC_PR) == 0 and ((portsc & PORTSC_PED) != 0 or (portsc & PORTSC_CCS) == 0)) {
            _ = clearPortChanges(@intCast(index_value));
            _ = wait.finish(true);
            return true;
        }
        wait.idle();
    }
    _ = wait.finish(false);
    current.timeouts += 1;
    return false;
}

fn warmResetPort(index_value: u8) bool {
    const base = OP_PORT_BASE + (@as(u64, index_value) * OP_PORT_STRIDE);
    const before = readOp32(base);
    _ = clearPortChanges(@intCast(index_value));
    writeOp32(base, (before & PORTSC_WRITE_PRESERVE_MASK) | PORTSC_WPR);
    var wait = usb_wait.Wait.begin(
        usb_timing.XHCI_PORT_WARM_RESET_TIMEOUT_MS,
        "xhci-port-warm-reset",
        0,
    );
    while (!wait.expired()) {
        const portsc = readOp32(base);
        if ((portsc & PORTSC_WPR) == 0) {
            _ = clearPortChanges(@intCast(index_value));
            const ready = (portsc & PORTSC_CCS) == 0 or
                ((portsc & PORTSC_PED) != 0 and portLinkState(portsc) == PORTSC_PLS_U0);
            _ = wait.finish(ready);
            return ready;
        }
        wait.idle();
    }
    _ = wait.finish(false);
    current.timeouts += 1;
    return false;
}

fn waitPortU0(index_value: u8, p: *PortStatus) bool {
    const index_usize: usize = @intCast(index_value);
    current.port_u0_waits += 1;
    var wait = usb_wait.Wait.begin(
        usb_timing.XHCI_PORT_U0_TIMEOUT_MS,
        "xhci-port-u0",
        0,
    );
    while (!wait.expired()) {
        refreshPortStatus(p, index_usize);
        if ((p.portsc & PORTSC_CCS) == 0) {
            _ = wait.finish(false);
            return false;
        }
        if ((p.portsc & PORTSC_PED) != 0 and portLinkState(p.portsc) == PORTSC_PLS_U0) {
            _ = wait.finish(true);
            return true;
        }
        wait.idle();
    }
    _ = wait.finish(false);
    return false;
}

fn enableSlotSmoke() bool {
    current.enable_slot_attempted = true;
    const completion = submitCommand(0, 0, trbType(TRB_TYPE_ENABLE_SLOT)) orelse return false;
    current.enable_slot_ok = completion.code == COMPLETION_SUCCESS;
    current.last_completion_code = completion.code;
    current.last_slot_id = completion.slot_id;
    if (!current.enable_slot_ok) current.failures += 1;
    return current.enable_slot_ok;
}

fn enumerateFirstConnectedDevice() bool {
    refreshPortsAndReclaim();
    var mass_storage_port: ?usize = null;
    var hid_port: ?usize = null;
    var fallback_port: ?usize = null;
    var found_configured_device = false;
    var i: usize = 0;
    while (i < @as(usize, current.port_count_seen)) : (i += 1) {
        if (!preparePortForProbe(i)) continue;
        if (!probePortDevice(i)) {
            disableCurrentSlotForScan();
            continue;
        }
        if (current.get_config_ok) {
            found_configured_device = true;
            publishCurrentCoreDevice();
            if (isMassStorageBulk()) {
                if (mass_storage_port == null) mass_storage_port = i;
            } else if (isBootHidInput()) {
                if (hid_port == null) hid_port = i;
            } else {
                if (fallback_port == null) fallback_port = i;
                current.non_hid_devices_skipped += 1;
            }
            persistActiveRuntime();
            continue;
        }
        disableCurrentSlotForScan();
    }

    if (mass_storage_port) |port_index| {
        return selectPortDevice(port_index, .mass_storage);
    }
    if (hid_port) |port_index| {
        return selectPortDevice(port_index, .hid_input);
    }
    if (fallback_port) |port_index| {
        return selectPortDevice(port_index, .generic);
    }
    return found_configured_device;
}

pub fn selectDeviceByPort(port: u8) bool {
    if (port == 0) return false;
    if (activateRuntimeByPort(port)) return true;
    if (current.addressed_slot_id != 0 and current.addressed_port == port and current.get_config_ok) return true;
    var i: usize = 0;
    while (i < @as(usize, current.port_count_seen)) : (i += 1) {
        if (current.first_ports[i].index != port) continue;
        disableCurrentSlotForScan();
        return selectPortDevice(i, .generic);
    }
    return false;
}

const SelectedDeviceKind = enum {
    mass_storage,
    hid_input,
    generic,
};

fn selectPortDevice(port_index: usize, kind: SelectedDeviceKind) bool {
    if (activateRuntimeByPort(current.first_ports[port_index].index)) {
        switch (kind) {
            .mass_storage => current.selected_mass_storage = true,
            .hid_input => current.selected_hid_input = true,
            .generic => {},
        }
        return true;
    }
    if (!probePortDevice(port_index)) {
        disableCurrentSlotForScan();
        return false;
    }
    if (!current.get_config_ok) {
        disableCurrentSlotForScan();
        return false;
    }
    switch (kind) {
        .mass_storage => current.selected_mass_storage = true,
        .hid_input => current.selected_hid_input = true,
        .generic => {},
    }
    publishCurrentCoreDevice();
    return true;
}

fn firstEnabledPort() ?usize {
    var i: usize = 0;
    while (i < @as(usize, current.port_count_seen)) : (i += 1) {
        if (portEnabled(i)) return i;
    }
    return null;
}

fn portEnabled(index: usize) bool {
    if (index >= @as(usize, current.port_count_seen)) return false;
    const p = current.first_ports[index];
    return (p.portsc & PORTSC_CCS) != 0 and (p.portsc & PORTSC_PED) != 0 and portLinkState(p.portsc) == PORTSC_PLS_U0;
}

fn preparePortForProbe(index: usize) bool {
    if (index >= @as(usize, current.port_count_seen)) return false;
    var p = current.first_ports[index];
    const ready = preparePortState(index, &p, .probe);
    current.first_ports[index] = p;
    return ready;
}

fn portLinkState(portsc: u32) u32 {
    return (portsc & PORTSC_PLS_MASK) >> 5;
}

fn portLinkStateName(pls: u8) []const u8 {
    return switch (pls) {
        0 => "U0",
        1 => "U1",
        2 => "U2",
        3 => "U3",
        4 => "Disabled",
        5 => "RxDetect",
        6 => "Inactive",
        7 => "Polling",
        8 => "Recovery",
        9 => "HotReset",
        10 => "Compliance",
        11 => "Test",
        15 => "Resume",
        else => "Reserved",
    };
}

fn probePortDevice(port_index: usize) bool {
    persistActiveRuntime();
    active_runtime_index = null;
    resetFirstEnumerationFields();
    current.ports_probed += 1;
    const record_index = beginProbeRecord(port_index);
    if (!enableSlotSmoke()) {
        updateProbeRecord(record_index, "enable-slot-failed");
        return false;
    }
    current.addressed_slot_id = current.last_slot_id;
    current.addressed_port = current.first_ports[port_index].index;
    current.addressed_speed = @truncate((current.first_ports[port_index].portsc >> 10) & 0x0F);
    updateProbeRecord(record_index, "slot-enabled");
    if (!allocFirstDeviceRuntime()) {
        updateProbeRecord(record_index, "alloc-failed");
        return false;
    }
    setupFirstInputContext(port_index);
    if (!addressFirstDevice()) {
        updateProbeRecord(record_index, "address-failed");
        return false;
    }
    updateProbeRecord(record_index, "addressed");
    if (!getFirstDeviceDescriptor()) {
        updateProbeRecord(record_index, "device-desc-failed");
        return false;
    }
    updateProbeRecord(record_index, "device-desc");
    if (!getFirstConfigDescriptor()) {
        updateProbeRecord(record_index, "config-desc-failed");
        return false;
    }
    _ = getFirstStringDescriptors();
    updateProbeRecord(record_index, "configured");
    return current.get_config_ok;
}

fn beginProbeRecord(port_index: usize) usize {
    const idx = @as(usize, current.probe_record_count);
    if (idx >= MAX_PORTS) return MAX_PORTS - 1;
    const p = current.first_ports[port_index];
    current.probe_records[idx] = .{
        .active = true,
        .port = p.index,
        .speed = @truncate((p.portsc >> 10) & 0x0F),
        .pls = @truncate(portLinkState(p.portsc)),
        .stage = "start",
    };
    current.probe_record_count += 1;
    return idx;
}

fn updateProbeRecord(index_value: usize, stage: []const u8) void {
    if (index_value >= MAX_PORTS) return;
    var r = &current.probe_records[index_value];
    if (!r.active) return;
    r.stage = stage;
    r.slot = current.addressed_slot_id;
    r.command_cc = current.last_completion_code;
    r.transfer_cc = current.last_transfer_completion_code;
    r.class_code = current.first_interface_class;
    r.subclass = current.first_interface_subclass;
    r.protocol = current.first_interface_protocol;
    r.vid = current.device_vendor_id;
    r.pid = current.device_product_id;
}

fn resetFirstEnumerationFields() void {
    current.address_device_attempted = false;
    current.address_device_ok = false;
    current.get_descriptor_attempted = false;
    current.get_descriptor_ok = false;
    current.get_config_attempted = false;
    current.get_config_ok = false;
    current.get_strings_attempted = false;
    current.get_strings_ok = false;
    current.set_configuration_attempted = false;
    current.set_configuration_ok = false;
    current.hid_set_protocol_attempted = false;
    current.hid_set_protocol_ok = false;
    current.hid_set_idle_attempted = false;
    current.hid_set_idle_ok = false;
    current.interrupt_endpoint_configured = false;
    current.interrupt_endpoint_faulted = false;
    current.interrupt_pending = false;
    current.interrupt_pending_trb_phys = 0;
    current.interrupt_pending_streak = 0;
    current.last_interrupt_ep_state = 0;
    current.interrupt_endpoint_id = 0;
    current.interrupt_endpoint_address = 0;
    current.interrupt_endpoint_max_packet = 0;
    current.interrupt_endpoint_interval_raw = 0;
    current.interrupt_endpoint_interval_context = 0;
    current.interrupt_enqueue = 0;
    current.interrupt_cycle = 1;
    current.last_interrupt_request_len = 0;
    current.last_interrupt_residue = 0;
    current.last_interrupt_actual_len = 0;
    current.last_interrupt_completion_code = 0;
    current.bulk_endpoints_configured = false;
    current.bulk_endpoints_faulted = false;
    current.bulk_in_endpoint_id = 0;
    current.bulk_out_endpoint_id = 0;
    current.bulk_in_enqueue = 0;
    current.bulk_in_cycle = 1;
    current.bulk_in_link_update_pending = false;
    current.bulk_out_enqueue = 0;
    current.bulk_out_cycle = 1;
    current.bulk_out_link_update_pending = false;
    current.selected_mass_storage = false;
    current.selected_hid_input = false;
    current.last_control_request_type = 0;
    current.last_control_request = 0;
    current.last_control_value = 0;
    current.last_control_index = 0;
    current.last_control_length = 0;
    current.last_control_direction = "none";
    current.last_control_completion_code = 0;
    current.last_control_residue = 0;
    current.last_control_ok = false;
    current.control_endpoint_faulted = false;
    current.descriptor_len = 0;
    current.descriptor_type = 0;
    current.usb_version_bcd = 0;
    current.device_class = 0;
    current.device_subclass = 0;
    current.device_protocol = 0;
    current.device_max_packet0 = 0;
    current.device_vendor_id = 0;
    current.device_product_id = 0;
    current.device_version_bcd = 0;
    current.manufacturer_index = 0;
    current.product_index = 0;
    current.serial_index = 0;
    current.config_total_length = 0;
    current.config_value = 0;
    current.config_attributes = 0;
    current.config_max_power_ma = 0;
    current.interface_count = 0;
    current.interface_record_count = 0;
    current.interface_records = .{usb_core.Interface{}} ** MAX_USB_INTERFACES;
    current.endpoint_count = 0;
    current.descriptor_records = 0;
    current.descriptor_unknown = 0;
    current.descriptor_malformed = 0;
    current.hid_descriptor_count = 0;
    current.ss_endpoint_companion_count = 0;
    current.selected_interface_reason = "none";
    current.first_interface_number = 0;
    current.first_interface_class = 0;
    current.first_interface_subclass = 0;
    current.first_interface_protocol = 0;
    current.first_endpoint_address = 0;
    current.first_endpoint_attributes = 0;
    current.first_endpoint_max_packet = 0;
    current.first_endpoint_interval = 0;
    current.first_hid_descriptor_len = 0;
    current.first_hid_report_descriptor_len = 0;
    current.bulk_in_endpoint_address = 0;
    current.bulk_in_endpoint_max_packet = 0;
    current.bulk_in_endpoint_max_burst = 0;
    current.bulk_out_endpoint_address = 0;
    current.bulk_out_endpoint_max_packet = 0;
    current.bulk_out_endpoint_max_burst = 0;
    current.string_language_id = 0;
    current.manufacturer_len = 0;
    current.product_len = 0;
    current.serial_len = 0;
    @memset(current.manufacturer_string[0..], 0);
    @memset(current.product_string[0..], 0);
    @memset(current.serial_string[0..], 0);
}

fn isBootHidInput() bool {
    return current.first_interface_class == 0x03 and
        current.first_interface_subclass == 0x01 and
        (current.first_interface_protocol == 0x01 or current.first_interface_protocol == 0x02) and
        (current.first_endpoint_address & 0x80) != 0 and
        (current.first_endpoint_attributes & 0x03) == 0x03;
}

fn isMassStorageBulk() bool {
    return current.first_interface_class == 0x08 and
        current.first_interface_subclass == 0x06 and
        current.first_interface_protocol == 0x50 and
        current.bulk_in_endpoint_address != 0 and
        current.bulk_out_endpoint_address != 0;
}

fn disableCurrentSlotForScan() void {
    if (current.addressed_slot_id == 0) return;
    const slot = current.addressed_slot_id;
    const port = current.addressed_port;
    _ = disableSlot(slot);
    if (port != 0) _ = usb_core.removeByPort("xhci", port);
    releaseRuntimeBySlot(slot);
    current.addressed_slot_id = 0;
    current.addressed_port = 0;
    current.control_endpoint_faulted = false;
    current.interrupt_endpoint_configured = false;
    current.interrupt_endpoint_faulted = false;
    current.interrupt_pending = false;
    current.interrupt_pending_trb_phys = 0;
    current.interrupt_pending_streak = 0;
    current.interrupt_endpoint_id = 0;
    current.interrupt_endpoint_address = 0;
    current.interrupt_endpoint_max_packet = 0;
    current.interrupt_endpoint_interval_raw = 0;
    current.interrupt_endpoint_interval_context = 0;
    current.bulk_endpoints_configured = false;
    current.bulk_endpoints_faulted = false;
}

fn disableSlot(slot: u8) bool {
    if (slot == 0) return false;
    current.disable_slot_commands += 1;
    const control = trbType(TRB_TYPE_DISABLE_SLOT) | (@as(u32, slot) << 24);
    const completion = submitCommand(0, 0, control);
    const ok = completion != null and completion.?.code == COMPLETION_SUCCESS;
    if (!ok) {
        current.disable_slot_failures += 1;
    }
    if (dcbaa_virt != 0) {
        const dcbaa: [*]u64 = @ptrFromInt(dcbaa_virt);
        dcbaa[slot] = 0;
    }
    return ok;
}

fn allocFirstDeviceRuntime() bool {
    if (current.addressed_slot_id == 0) return false;
    const runtime_index = runtimeIndexForSlot(current.addressed_slot_id) orelse allocateRuntime(current.addressed_slot_id, current.addressed_port, current.addressed_speed) orelse return false;
    if (!activateRuntime(runtime_index)) return false;
    if (first_device_virt == 0 or first_input_virt == 0 or first_ep0_ring_virt == 0 or first_descriptor_virt == 0) return false;
    const device: [*]u8 = @ptrFromInt(first_device_virt);
    const input: [*]u8 = @ptrFromInt(first_input_virt);
    const descriptor: [*]u8 = @ptrFromInt(first_descriptor_virt);
    const interrupt_buffer: [*]u8 = @ptrFromInt(first_interrupt_buffer_virt);
    const bulk_buffer: [*]u8 = @ptrFromInt(first_bulk_buffer_virt);
    @memset(device[0..DEVICE_CONTEXT_BYTES], 0);
    @memset(input[0..INPUT_CONTEXT_BYTES], 0);
    @memset(descriptor[0..DESCRIPTOR_BYTES], 0);
    @memset(interrupt_buffer[0..INTERRUPT_BUFFER_BYTES], 0);
    @memset(bulk_buffer[0..INTERRUPT_BUFFER_BYTES], 0);
    const dcbaa: [*]u64 = @ptrFromInt(dcbaa_virt);
    dcbaa[current.addressed_slot_id] = current.device_context_phys;
    initEp0Ring();
    initInterruptRing();
    initBulkRings();
    return true;
}

fn allocateRuntime(slot_id: u8, port: u8, speed: u8) ?usize {
    _ = deferred_events.purgeSlot(slot_id);
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (runtimes[i].active) continue;
        var rt = &runtimes[i];
        rt.* = .{
            .active = true,
            .slot_id = slot_id,
            .port = port,
            .speed = speed,
            .device_context_phys = allocFrameZero() orelse return failAllocRuntime(i, "device context"),
            .input_context_phys = allocFrameZero() orelse return failAllocRuntime(i, "input context"),
            .ep0_ring_phys = allocFrameZero() orelse return failAllocRuntime(i, "ep0 ring"),
            .interrupt_ring_phys = allocFrameZero() orelse return failAllocRuntime(i, "interrupt ring"),
            .interrupt_buffer_phys = allocFrameZero() orelse return failAllocRuntime(i, "interrupt buffer"),
            .bulk_in_ring_phys = allocFrameZero() orelse return failAllocRuntime(i, "bulk in ring"),
            .bulk_out_ring_phys = allocFrameZero() orelse return failAllocRuntime(i, "bulk out ring"),
            .bulk_buffer_phys = allocFrameZero() orelse return failAllocRuntime(i, "bulk buffer"),
            .descriptor_phys = allocFrameZero() orelse return failAllocRuntime(i, "descriptor"),
        };
        rt.device_virt = phys.physToVirt(rt.device_context_phys);
        rt.input_virt = phys.physToVirt(rt.input_context_phys);
        rt.ep0_ring_virt = phys.physToVirt(rt.ep0_ring_phys);
        rt.interrupt_ring_virt = phys.physToVirt(rt.interrupt_ring_phys);
        rt.interrupt_buffer_virt = phys.physToVirt(rt.interrupt_buffer_phys);
        rt.bulk_in_ring_virt = phys.physToVirt(rt.bulk_in_ring_phys);
        rt.bulk_out_ring_virt = phys.physToVirt(rt.bulk_out_ring_phys);
        rt.bulk_buffer_virt = phys.physToVirt(rt.bulk_buffer_phys);
        rt.descriptor_virt = phys.physToVirt(rt.descriptor_phys);
        current.retained_slots += 1;
        return i;
    }
    current.failures += 1;
    current.reason = "xHCI runtime table full";
    return null;
}

fn failAllocRuntime(index_value: usize, what: []const u8) ?usize {
    runtimes[index_value] = .{};
    _ = failAlloc(what);
    return null;
}

fn activateRuntimeByPort(port: u8) bool {
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (runtimes[i].active and runtimes[i].port == port) return activateRuntime(i);
    }
    return false;
}

fn activateRuntimeBySlot(slot_id: u8) bool {
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (runtimes[i].active and runtimes[i].slot_id == slot_id) return activateRuntime(i);
    }
    return false;
}

fn activateRuntimeByIdentity(slot_id: u8, port: u8) bool {
    const index_value = runtimeIndexForIdentity(slot_id, port) orelse return false;
    return activateRuntime(index_value);
}

fn runtimeIndexForSlot(slot_id: u8) ?usize {
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (runtimes[i].active and runtimes[i].slot_id == slot_id) return i;
    }
    return null;
}

fn runtimeIndexForIdentity(slot_id: u8, port: u8) ?usize {
    if (slot_id == 0 or port == 0) return null;
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (runtimes[i].active and
            runtimes[i].slot_id == slot_id and
            runtimes[i].port == port)
            return i;
    }
    return null;
}

fn activateRuntime(index_value: usize) bool {
    if (index_value >= runtimes.len or !runtimes[index_value].active) return false;
    if (active_runtime_index) |active| {
        if (active == index_value) {
            persistActiveRuntime();
            return true;
        }
        persistActiveRuntime();
    }
    active_runtime_index = index_value;
    current.runtime_switches += 1;
    loadRuntime(index_value);
    return true;
}

fn persistActiveRuntime() void {
    const idx = active_runtime_index orelse return;
    if (idx >= runtimes.len or !runtimes[idx].active) return;
    var rt = &runtimes[idx];
    rt.slot_id = current.addressed_slot_id;
    rt.port = current.addressed_port;
    rt.speed = current.addressed_speed;
    rt.config_value = current.config_value;
    rt.vendor_id = current.device_vendor_id;
    rt.product_id = current.device_product_id;
    rt.device_context_phys = current.device_context_phys;
    rt.input_context_phys = current.input_context_phys;
    rt.ep0_ring_phys = current.ep0_ring_phys;
    rt.interrupt_ring_phys = current.interrupt_ring_phys;
    rt.interrupt_buffer_phys = current.interrupt_buffer_phys;
    rt.bulk_in_ring_phys = current.bulk_in_ring_phys;
    rt.bulk_out_ring_phys = current.bulk_out_ring_phys;
    rt.bulk_buffer_phys = current.bulk_buffer_phys;
    rt.descriptor_phys = current.descriptor_phys;
    rt.device_virt = first_device_virt;
    rt.input_virt = first_input_virt;
    rt.ep0_ring_virt = first_ep0_ring_virt;
    rt.interrupt_ring_virt = first_interrupt_ring_virt;
    rt.interrupt_buffer_virt = first_interrupt_buffer_virt;
    rt.bulk_in_ring_virt = first_bulk_in_ring_virt;
    rt.bulk_out_ring_virt = first_bulk_out_ring_virt;
    rt.bulk_buffer_virt = first_bulk_buffer_virt;
    rt.descriptor_virt = first_descriptor_virt;
    rt.ep0_enqueue = current.ep0_enqueue;
    rt.ep0_cycle = current.ep0_cycle;
    rt.control_endpoint_faulted = current.control_endpoint_faulted;
    rt.interrupt_enqueue = current.interrupt_enqueue;
    rt.interrupt_cycle = current.interrupt_cycle;
    rt.interrupt_endpoint_id = current.interrupt_endpoint_id;
    rt.interrupt_endpoint_address = current.interrupt_endpoint_address;
    rt.interrupt_endpoint_max_packet = current.interrupt_endpoint_max_packet;
    rt.interrupt_endpoint_interval_raw = current.interrupt_endpoint_interval_raw;
    rt.interrupt_endpoint_interval_context = current.interrupt_endpoint_interval_context;
    rt.interrupt_endpoint_configured = current.interrupt_endpoint_configured;
    rt.interrupt_endpoint_faulted = current.interrupt_endpoint_faulted;
    rt.interrupt_pending = current.interrupt_pending;
    rt.interrupt_pending_trb_phys = current.interrupt_pending_trb_phys;
    rt.interrupt_pending_streak = current.interrupt_pending_streak;
    rt.last_interrupt_ep_state = current.last_interrupt_ep_state;
    rt.last_interrupt_request_len = current.last_interrupt_request_len;
    rt.last_interrupt_residue = current.last_interrupt_residue;
    rt.last_interrupt_actual_len = current.last_interrupt_actual_len;
    rt.last_interrupt_completion_code = current.last_interrupt_completion_code;
    rt.bulk_endpoints_configured = current.bulk_endpoints_configured;
    rt.bulk_endpoints_faulted = current.bulk_endpoints_faulted;
    rt.bulk_in_endpoint_id = current.bulk_in_endpoint_id;
    rt.bulk_in_endpoint_address = current.bulk_in_endpoint_address;
    rt.bulk_in_endpoint_max_packet = current.bulk_in_endpoint_max_packet;
    rt.bulk_in_endpoint_max_burst = current.bulk_in_endpoint_max_burst;
    rt.bulk_out_endpoint_id = current.bulk_out_endpoint_id;
    rt.bulk_out_endpoint_address = current.bulk_out_endpoint_address;
    rt.bulk_out_endpoint_max_packet = current.bulk_out_endpoint_max_packet;
    rt.bulk_out_endpoint_max_burst = current.bulk_out_endpoint_max_burst;
    rt.bulk_in_enqueue = current.bulk_in_enqueue;
    rt.bulk_in_cycle = current.bulk_in_cycle;
    rt.bulk_in_link_update_pending = current.bulk_in_link_update_pending;
    rt.bulk_out_enqueue = current.bulk_out_enqueue;
    rt.bulk_out_cycle = current.bulk_out_cycle;
    rt.bulk_out_link_update_pending = current.bulk_out_link_update_pending;
}

fn loadRuntime(index_value: usize) void {
    const rt = runtimes[index_value];
    current.addressed_slot_id = rt.slot_id;
    current.addressed_port = rt.port;
    current.addressed_speed = rt.speed;
    current.config_value = rt.config_value;
    current.device_vendor_id = rt.vendor_id;
    current.device_product_id = rt.product_id;
    current.get_config_ok = rt.config_value != 0;
    current.device_context_phys = rt.device_context_phys;
    current.input_context_phys = rt.input_context_phys;
    current.ep0_ring_phys = rt.ep0_ring_phys;
    current.interrupt_ring_phys = rt.interrupt_ring_phys;
    current.interrupt_buffer_phys = rt.interrupt_buffer_phys;
    current.bulk_in_ring_phys = rt.bulk_in_ring_phys;
    current.bulk_out_ring_phys = rt.bulk_out_ring_phys;
    current.bulk_buffer_phys = rt.bulk_buffer_phys;
    current.descriptor_phys = rt.descriptor_phys;
    first_device_virt = rt.device_virt;
    first_input_virt = rt.input_virt;
    first_ep0_ring_virt = rt.ep0_ring_virt;
    first_interrupt_ring_virt = rt.interrupt_ring_virt;
    first_interrupt_buffer_virt = rt.interrupt_buffer_virt;
    first_bulk_in_ring_virt = rt.bulk_in_ring_virt;
    first_bulk_out_ring_virt = rt.bulk_out_ring_virt;
    first_bulk_buffer_virt = rt.bulk_buffer_virt;
    first_descriptor_virt = rt.descriptor_virt;
    current.ep0_enqueue = rt.ep0_enqueue;
    current.ep0_cycle = rt.ep0_cycle;
    current.control_endpoint_faulted = rt.control_endpoint_faulted;
    current.interrupt_enqueue = rt.interrupt_enqueue;
    current.interrupt_cycle = rt.interrupt_cycle;
    current.interrupt_endpoint_id = rt.interrupt_endpoint_id;
    current.interrupt_endpoint_address = rt.interrupt_endpoint_address;
    current.interrupt_endpoint_max_packet = rt.interrupt_endpoint_max_packet;
    current.interrupt_endpoint_interval_raw = rt.interrupt_endpoint_interval_raw;
    current.interrupt_endpoint_interval_context = rt.interrupt_endpoint_interval_context;
    current.interrupt_endpoint_configured = rt.interrupt_endpoint_configured;
    current.interrupt_endpoint_faulted = rt.interrupt_endpoint_faulted;
    current.interrupt_pending = rt.interrupt_pending;
    current.interrupt_pending_trb_phys = rt.interrupt_pending_trb_phys;
    current.interrupt_pending_streak = rt.interrupt_pending_streak;
    current.last_interrupt_ep_state = rt.last_interrupt_ep_state;
    current.last_interrupt_request_len = rt.last_interrupt_request_len;
    current.last_interrupt_residue = rt.last_interrupt_residue;
    current.last_interrupt_actual_len = rt.last_interrupt_actual_len;
    current.last_interrupt_completion_code = rt.last_interrupt_completion_code;
    current.bulk_endpoints_configured = rt.bulk_endpoints_configured;
    current.bulk_endpoints_faulted = rt.bulk_endpoints_faulted;
    current.bulk_in_endpoint_id = rt.bulk_in_endpoint_id;
    current.bulk_in_endpoint_address = rt.bulk_in_endpoint_address;
    current.bulk_in_endpoint_max_packet = rt.bulk_in_endpoint_max_packet;
    current.bulk_in_endpoint_max_burst = rt.bulk_in_endpoint_max_burst;
    current.bulk_out_endpoint_id = rt.bulk_out_endpoint_id;
    current.bulk_out_endpoint_address = rt.bulk_out_endpoint_address;
    current.bulk_out_endpoint_max_packet = rt.bulk_out_endpoint_max_packet;
    current.bulk_out_endpoint_max_burst = rt.bulk_out_endpoint_max_burst;
    current.bulk_in_enqueue = rt.bulk_in_enqueue;
    current.bulk_in_cycle = rt.bulk_in_cycle;
    current.bulk_in_link_update_pending = rt.bulk_in_link_update_pending;
    current.bulk_out_enqueue = rt.bulk_out_enqueue;
    current.bulk_out_cycle = rt.bulk_out_cycle;
    current.bulk_out_link_update_pending = rt.bulk_out_link_update_pending;
}

fn releaseRuntimeBySlot(slot_id: u8) void {
    _ = deferred_events.purgeSlot(slot_id);
    if (active_sync_transfer) |owner| {
        switch (owner) {
            .transfer => |transfer| if (transfer.slot_id == slot_id) {
                active_sync_transfer = null;
            },
            .command => {},
        }
    }
    if (last_timed_out_sync_transfer) |owner| {
        switch (owner) {
            .transfer => |transfer| if (transfer.slot_id == slot_id) {
                last_timed_out_sync_transfer = null;
                closeLastSyncTransferIncident();
            },
            .command => {},
        }
    }
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (!runtimes[i].active or runtimes[i].slot_id != slot_id) continue;
        runtimes[i].active = false;
        if (current.retained_slots > 0) current.retained_slots -= 1;
        if (active_runtime_index != null and active_runtime_index.? == i) active_runtime_index = null;
        return;
    }
}

fn reclaimDisconnectedRuntimes() void {
    persistActiveRuntime();
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (!runtimes[i].active) continue;
        const slot = runtimes[i].slot_id;
        const port = runtimes[i].port;
        if (slot == 0 or port == 0 or portIsConnected(port)) continue;
        current.port_disconnects += 1;
        _ = disableSlot(slot);
        _ = usb_core.removeByPort("xhci", port);
        releaseRuntimeBySlot(slot);
        current.reclaimed_slots += 1;
        if (current.addressed_slot_id == slot) {
            current.addressed_slot_id = 0;
            current.addressed_port = 0;
            current.control_endpoint_faulted = false;
            current.interrupt_endpoint_configured = false;
            current.interrupt_endpoint_faulted = false;
            current.interrupt_pending = false;
            current.interrupt_pending_trb_phys = 0;
            current.interrupt_pending_streak = 0;
            current.interrupt_endpoint_id = 0;
            current.interrupt_endpoint_address = 0;
            current.interrupt_endpoint_max_packet = 0;
            current.interrupt_endpoint_interval_raw = 0;
            current.interrupt_endpoint_interval_context = 0;
            current.bulk_endpoints_configured = false;
            current.bulk_endpoints_faulted = false;
        }
    }
}

fn refreshPortsAndReclaim() void {
    if (!current.present or current.mmio_virt == 0 or !current.controller_running) return;
    readOperational();
    readRuntime();
    readPorts();
    reclaimDisconnectedRuntimes();
}

fn portIsConnected(port: u8) bool {
    var i: usize = 0;
    while (i < @as(usize, current.port_count_seen)) : (i += 1) {
        const p = current.first_ports[i];
        if (p.index == port) return p.connected;
    }
    return false;
}

fn initEp0Ring() void {
    const ring: [*]Trb = @ptrFromInt(first_ep0_ring_virt);
    var i: usize = 0;
    while (i < TRANSFER_TRB_COUNT) : (i += 1) ring[i] = .{ .parameter = 0, .status = 0, .control = 0 };
    ring[TRANSFER_TRB_COUNT - 1] = .{
        .parameter = current.ep0_ring_phys,
        .status = 0,
        .control = trbType(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE,
    };
    current.ep0_enqueue = 0;
    current.ep0_cycle = 1;
    current.control_endpoint_faulted = false;
}

fn initInterruptRing() void {
    if (currentInterruptOwnerMatch()) |owner| _ = deferred_events.purge(owner);
    const ring: [*]Trb = @ptrFromInt(first_interrupt_ring_virt);
    var i: usize = 0;
    while (i < TRANSFER_TRB_COUNT) : (i += 1) ring[i] = .{ .parameter = 0, .status = 0, .control = 0 };
    ring[TRANSFER_TRB_COUNT - 1] = .{
        .parameter = current.interrupt_ring_phys,
        .status = 0,
        .control = trbType(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE,
    };
    current.interrupt_enqueue = 0;
    current.interrupt_cycle = 1;
    current.interrupt_pending = false;
    current.interrupt_pending_trb_phys = 0;
    current.interrupt_pending_streak = 0;
}

fn initBulkRings() void {
    initRing(first_bulk_in_ring_virt, current.bulk_in_ring_phys);
    initRing(first_bulk_out_ring_virt, current.bulk_out_ring_phys);
    current.bulk_in_enqueue = 0;
    current.bulk_in_cycle = 1;
    current.bulk_out_enqueue = 0;
    current.bulk_out_cycle = 1;
}

fn initRing(ring_virt: u64, ring_phys: u64) void {
    if (ring_virt == 0 or ring_phys == 0) return;
    const ring: [*]Trb = @ptrFromInt(ring_virt);
    var i: usize = 0;
    while (i < TRANSFER_TRB_COUNT) : (i += 1) ring[i] = .{ .parameter = 0, .status = 0, .control = 0 };
    ring[TRANSFER_TRB_COUNT - 1] = .{
        .parameter = ring_phys,
        .status = 0,
        .control = trbType(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE | TRB_CYCLE,
    };
}

fn setupFirstInputContext(port_index: usize) void {
    const input: [*]u32 = @ptrFromInt(first_input_virt);
    const ctx_dwords = @as(usize, current.context_size) / 4;
    const input_control = input;
    input_control[0] = 0;
    input_control[1] = 0x3;

    const slot = input + ctx_dwords;
    slot[0] = (@as(u32, current.addressed_speed) << 20) | (1 << 27);
    slot[1] = @as(u32, current.first_ports[port_index].index) << 16;
    slot[2] = 0;
    slot[3] = 0;

    const ep0 = input + (ctx_dwords * 2);
    ep0[0] = 0;
    ep0[1] = (3 << 1) | (4 << 3) | (@as(u32, maxPacketForSpeed(current.addressed_speed)) << 16);
    ep0[2] = @truncate(current.ep0_ring_phys | 1);
    ep0[3] = @truncate((current.ep0_ring_phys | 1) >> 32);
    ep0[4] = 8;
}

fn addressFirstDevice() bool {
    current.address_device_attempted = true;
    const control = trbType(TRB_TYPE_ADDRESS_DEVICE) | (@as(u32, current.addressed_slot_id) << 24);
    const completion = submitCommand(current.input_context_phys, 0, control) orelse return false;
    current.last_completion_code = completion.code;
    current.last_slot_id = completion.slot_id;
    current.address_device_ok = completion.code == COMPLETION_SUCCESS;
    if (!current.address_device_ok) current.failures += 1;
    if (current.address_device_ok) {
        _ = usb_wait.millisecondsWithReason(
            usb_timing.SET_ADDRESS_SETTLE_MS,
            "xhci-address-settle",
            0,
        );
    }
    return current.address_device_ok;
}

fn getFirstDeviceDescriptor() bool {
    current.get_descriptor_attempted = true;
    const desc: [*]u8 = @ptrFromInt(first_descriptor_virt);
    @memset(desc[0..DESCRIPTOR_BYTES], 0);
    if (!submitControlIn(USB_REQ_GET_DESCRIPTOR, @as(u16, USB_DESC_DEVICE) << 8, 0, DEVICE_DESCRIPTOR_LEN, current.descriptor_phys)) return false;
    parseDeviceDescriptor();
    current.get_descriptor_ok = current.descriptor_len == DEVICE_DESCRIPTOR_LEN and current.descriptor_type == USB_DESC_DEVICE;
    if (!current.get_descriptor_ok) current.failures += 1;
    return current.get_descriptor_ok;
}

fn getFirstConfigDescriptor() bool {
    current.get_config_attempted = true;
    const desc: [*]u8 = @ptrFromInt(first_descriptor_virt);
    @memset(desc[0..DESCRIPTOR_BYTES], 0);
    if (!submitControlIn(USB_REQ_GET_DESCRIPTOR, @as(u16, USB_DESC_CONFIGURATION) << 8, 0, CONFIG_DESCRIPTOR_HEADER_LEN, current.descriptor_phys)) return false;
    const total = readLe16(desc, 2);
    current.config_total_length = total;
    if (desc[0] < CONFIG_DESCRIPTOR_HEADER_LEN or desc[1] != USB_DESC_CONFIGURATION or total < CONFIG_DESCRIPTOR_HEADER_LEN or total > DESCRIPTOR_BYTES) {
        current.failures += 1;
        return false;
    }
    @memset(desc[0..DESCRIPTOR_BYTES], 0);
    if (!submitControlIn(USB_REQ_GET_DESCRIPTOR, @as(u16, USB_DESC_CONFIGURATION) << 8, 0, total, current.descriptor_phys)) return false;
    parseConfigDescriptor(total);
    current.get_config_ok = current.config_total_length == total and current.interface_count > 0;
    if (!current.get_config_ok) current.failures += 1;
    return current.get_config_ok;
}

fn getFirstStringDescriptors() bool {
    current.get_strings_attempted = true;
    current.manufacturer_len = 0;
    current.product_len = 0;
    current.serial_len = 0;
    @memset(current.manufacturer_string[0..], 0);
    @memset(current.product_string[0..], 0);
    @memset(current.serial_string[0..], 0);

    if (current.manufacturer_index == 0 and current.product_index == 0 and current.serial_index == 0) {
        current.get_strings_ok = true;
        return true;
    }

    current.string_language_id = readFirstStringLanguage() orelse 0x0409;
    var ok = true;
    if (current.manufacturer_index != 0) {
        ok = readAsciiStringDescriptor(current.manufacturer_index, current.string_language_id, &current.manufacturer_string, &current.manufacturer_len) and ok;
    }
    if (current.product_index != 0) {
        ok = readAsciiStringDescriptor(current.product_index, current.string_language_id, &current.product_string, &current.product_len) and ok;
    }
    if (current.serial_index != 0) {
        ok = readAsciiStringDescriptor(current.serial_index, current.string_language_id, &current.serial_string, &current.serial_len) and ok;
    }
    current.get_strings_ok = ok;
    if (!ok) current.failures += 1;
    return ok;
}

fn readFirstStringLanguage() ?u16 {
    const desc: [*]u8 = @ptrFromInt(first_descriptor_virt);
    @memset(desc[0..DESCRIPTOR_BYTES], 0);
    if (!submitControlIn(USB_REQ_GET_DESCRIPTOR, @as(u16, USB_DESC_STRING) << 8, 0, 4, current.descriptor_phys)) return null;
    if (desc[0] < 4 or desc[1] != USB_DESC_STRING) return null;
    return readLe16(desc, 2);
}

fn readAsciiStringDescriptor(index_value: u8, lang: u16, out: *[MAX_STRING_CHARS]u8, out_len: *u8) bool {
    @memset(out.*[0..], 0);
    out_len.* = 0;
    const desc: [*]u8 = @ptrFromInt(first_descriptor_virt);
    @memset(desc[0..DESCRIPTOR_BYTES], 0);
    if (!submitControlIn(USB_REQ_GET_DESCRIPTOR, (@as(u16, USB_DESC_STRING) << 8) | index_value, lang, 255, current.descriptor_phys)) return false;
    const length: usize = @min(@as(usize, @intCast(desc[0])), 255);
    if (length < 2 or desc[1] != USB_DESC_STRING) return false;

    var offset: usize = 2;
    var n: usize = 0;
    while (offset + 1 < length and n < MAX_STRING_CHARS) : ({
        offset += 2;
        n += 1;
    }) {
        const codepoint = readLe16(desc, offset);
        out.*[n] = if (codepoint >= 0x20 and codepoint <= 0x7E) @intCast(codepoint) else '?';
    }
    out_len.* = @intCast(n);
    return true;
}

fn publishCurrentCoreDevice() void {
    _ = usb_core.publishOrReplaceByPort(.{
        .configured = current.get_config_ok,
        .controller = "xhci",
        .slot_id = current.addressed_slot_id,
        .port = current.addressed_port,
        .speed = current.addressed_speed,
        .usb_version_bcd = current.usb_version_bcd,
        .vendor_id = current.device_vendor_id,
        .product_id = current.device_product_id,
        .device_version_bcd = current.device_version_bcd,
        .device_class = current.device_class,
        .device_subclass = current.device_subclass,
        .device_protocol = current.device_protocol,
        .config_value = current.config_value,
        .config_attributes = current.config_attributes,
        .config_max_power_ma = current.config_max_power_ma,
        .interface_count = current.interface_count,
        .interface_record_count = current.interface_record_count,
        .interfaces = current.interface_records,
        .endpoint_count = current.endpoint_count,
        .first_interface_number = current.first_interface_number,
        .first_interface_class = current.first_interface_class,
        .first_interface_subclass = current.first_interface_subclass,
        .first_interface_protocol = current.first_interface_protocol,
        .first_endpoint_address = current.first_endpoint_address,
        .first_endpoint_attributes = current.first_endpoint_attributes,
        .first_endpoint_max_packet = current.first_endpoint_max_packet,
        .first_endpoint_interval = current.first_endpoint_interval,
        .bulk_in_endpoint_address = current.bulk_in_endpoint_address,
        .bulk_in_endpoint_max_packet = current.bulk_in_endpoint_max_packet,
        .bulk_in_endpoint_max_burst = current.bulk_in_endpoint_max_burst,
        .bulk_out_endpoint_address = current.bulk_out_endpoint_address,
        .bulk_out_endpoint_max_packet = current.bulk_out_endpoint_max_packet,
        .bulk_out_endpoint_max_burst = current.bulk_out_endpoint_max_burst,
        .hid_descriptor_len = current.first_hid_descriptor_len,
        .hid_report_descriptor_len = current.first_hid_report_descriptor_len,
        .string_language_id = current.string_language_id,
        .manufacturer_len = current.manufacturer_len,
        .product_len = current.product_len,
        .serial_len = current.serial_len,
        .manufacturer_string = current.manufacturer_string,
        .product_string = current.product_string,
        .serial_string = current.serial_string,
    });
}

fn clampU8(value: u8, min_value: u8, max_value: u8) u8 {
    if (value < min_value) return min_value;
    if (value > max_value) return max_value;
    return value;
}

fn floorLog2U16(input: u16) u8 {
    var value = input;
    var result: u8 = 0;
    while (value > 1) : (value >>= 1) {
        result += 1;
    }
    return result;
}

fn encodeInterruptInterval(speed: u8, descriptor_interval: u8) u8 {
    const raw: u8 = if (descriptor_interval == 0) 1 else descriptor_interval;
    return switch (speed) {
        // xHCI table 6-12: LS/FS interrupt bInterval is linear milliseconds.
        // The Endpoint Context stores 125us * 2^Interval, rounded down.
        1, 2 => clampU8(floorLog2U16(@as(u16, raw) * 8), 3, 10),
        // HS/SS interrupt bInterval is already an exponent in 125us units.
        else => clampU8(raw - 1, 0, 15),
    };
}

pub fn configureFirstInterruptInEndpoint(endpoint_address: u8, max_packet: u16, interval: u8) bool {
    if (!current.get_config_ok or current.addressed_slot_id == 0 or first_input_virt == 0 or first_interrupt_ring_virt == 0) return false;
    if (current.interrupt_endpoint_faulted) return false;
    if ((endpoint_address & 0x80) == 0) return false;
    const endpoint_number: u8 = endpoint_address & 0x0F;
    if (endpoint_number == 0) return false;
    const dci: u8 = endpoint_number * 2 + 1;
    if (dci >= 32) return false;
    if (currentInterruptOwnerMatch()) |owner| _ = deferred_events.purge(owner);
    current.interrupt_pending = false;
    current.interrupt_pending_trb_phys = 0;
    current.interrupt_pending_streak = 0;

    const input: [*]u32 = @ptrFromInt(first_input_virt);
    @memset(input[0 .. INPUT_CONTEXT_BYTES / 4], 0);
    const ctx_dwords = @as(usize, current.context_size) / 4;
    input[1] = (@as(u32, 1) << @as(u5, @intCast(dci))) | 0x1;

    const slot = input + ctx_dwords;
    slot[0] = (@as(u32, current.addressed_speed) << 20) | (@as(u32, dci) << 27);
    slot[1] = @as(u32, current.addressed_port) << 16;

    const ep = endpointContext(input, ctx_dwords, dci);
    const ep_type: u32 = 7; // Interrupt IN
    const mps: u16 = if (max_packet == 0) 8 else max_packet;
    const context_interval = encodeInterruptInterval(current.addressed_speed, interval);
    current.interrupt_endpoint_id = dci;
    current.interrupt_endpoint_address = endpoint_address;
    current.interrupt_endpoint_max_packet = mps;
    current.interrupt_endpoint_interval_raw = interval;
    current.interrupt_endpoint_interval_context = context_interval;
    ep[0] = @as(u32, context_interval) << 16;
    ep[1] = (3 << 1) | (ep_type << 3) | (@as(u32, mps) << 16);
    const interrupt_dequeue = current.interrupt_ring_phys +
        @as(u64, current.interrupt_enqueue) * @sizeOf(Trb) |
        @as(u64, if (current.interrupt_cycle != 0) 1 else 0);
    ep[2] = @truncate(interrupt_dequeue);
    ep[3] = @truncate(interrupt_dequeue >> 32);
    ep[4] = @as(u32, mps) | (@as(u32, mps) << 16);

    const control = trbType(TRB_TYPE_CONFIGURE_ENDPOINT) | (@as(u32, current.addressed_slot_id) << 24);
    const completion = submitCommand(current.input_context_phys, 0, control) orelse {
        // The command may have reached hardware even if its completion was
        // lost.  Never issue another Add-Configure against unknown contexts.
        current.interrupt_endpoint_faulted = true;
        return false;
    };
    current.last_completion_code = completion.code;
    current.last_slot_id = completion.slot_id;
    current.interrupt_endpoint_configured = completion.code == COMPLETION_SUCCESS;
    if (current.interrupt_endpoint_configured) {
        current.interrupt_endpoint_faulted = false;
        return true;
    }
    current.interrupt_endpoint_faulted = true;
    current.failures += 1;
    return false;
}

pub fn configureInterruptInEndpointHandle(handle: *EndpointHandle) bool {
    if (handle.kind != .interrupt_in) return false;
    if (!selectDeviceHandle(&handle.device)) return false;
    if (!configureFirstInterruptInEndpoint(handle.address, handle.max_packet, handle.interval)) return false;
    refreshHandleFromCurrent(&handle.device);
    handle.endpoint_id = current.interrupt_endpoint_id;
    return true;
}

pub fn configureFirstBulkEndpoints(
    in_address: u8,
    in_max_packet: u16,
    in_max_burst: u8,
    out_address: u8,
    out_max_packet: u16,
    out_max_burst: u8,
) bool {
    if (!current.get_config_ok or current.addressed_slot_id == 0 or first_input_virt == 0) return false;
    if (current.bulk_endpoints_faulted) return false;
    if (first_bulk_in_ring_virt == 0 or first_bulk_out_ring_virt == 0) return false;
    if ((in_address & 0x80) == 0 or (out_address & 0x80) != 0) return false;
    const in_dci = endpointDci(in_address);
    const out_dci = endpointDci(out_address);
    if (in_dci == 0 or out_dci == 0 or in_dci >= 32 or out_dci >= 32) return false;
    const max_dci = if (in_dci > out_dci) in_dci else out_dci;

    const input: [*]u32 = @ptrFromInt(first_input_virt);
    @memset(input[0 .. INPUT_CONTEXT_BYTES / 4], 0);
    const ctx_dwords = @as(usize, current.context_size) / 4;
    input[1] = (@as(u32, 1) << @as(u5, @intCast(in_dci))) |
        (@as(u32, 1) << @as(u5, @intCast(out_dci))) | 0x1;

    const slot = input + ctx_dwords;
    slot[0] = (@as(u32, current.addressed_speed) << 20) | (@as(u32, max_dci) << 27);
    slot[1] = @as(u32, current.addressed_port) << 16;

    const in_dequeue = current.bulk_in_ring_phys +
        @as(u64, current.bulk_in_enqueue) * @sizeOf(Trb) |
        @as(u64, if (current.bulk_in_cycle != 0) 1 else 0);
    const out_dequeue = current.bulk_out_ring_phys +
        @as(u64, current.bulk_out_enqueue) * @sizeOf(Trb) |
        @as(u64, if (current.bulk_out_cycle != 0) 1 else 0);
    const in_fields = setupBulkEpContext(input, ctx_dwords, in_dci, true, in_dequeue, in_max_packet, in_max_burst);
    const out_fields = setupBulkEpContext(input, ctx_dwords, out_dci, false, out_dequeue, out_max_packet, out_max_burst);

    const control = trbType(TRB_TYPE_CONFIGURE_ENDPOINT) | (@as(u32, current.addressed_slot_id) << 24);
    const completion = submitCommand(current.input_context_phys, 0, control) orelse {
        // Lost Configure completion leaves Add-Context ownership unknown.
        // A blind repeat is not an idempotent operation.
        current.bulk_endpoints_faulted = true;
        return false;
    };
    current.last_completion_code = completion.code;
    current.last_slot_id = completion.slot_id;
    current.bulk_endpoints_configured = completion.code == COMPLETION_SUCCESS;
    if (current.bulk_endpoints_configured) {
        current.bulk_endpoints_faulted = false;
        current.bulk_in_endpoint_id = in_dci;
        current.bulk_out_endpoint_id = out_dci;
        current.bulk_in_endpoint_address = in_address;
        current.bulk_in_endpoint_max_packet = in_fields.max_packet;
        current.bulk_in_endpoint_max_burst = in_fields.max_burst;
        current.bulk_out_endpoint_address = out_address;
        current.bulk_out_endpoint_max_packet = out_fields.max_packet;
        current.bulk_out_endpoint_max_burst = out_fields.max_burst;
        return true;
    }
    current.bulk_endpoints_faulted = true;
    current.failures += 1;
    return false;
}

pub fn configureBulkEndpointHandles(in_handle: *EndpointHandle, out_handle: *EndpointHandle) bool {
    if (in_handle.kind != .bulk_in or out_handle.kind != .bulk_out) return false;
    if (!sameDeviceTarget(in_handle.device, out_handle.device)) return false;
    if (!selectDeviceHandle(&in_handle.device)) return false;
    out_handle.device = in_handle.device;
    if (!configureFirstBulkEndpoints(
        in_handle.address,
        in_handle.max_packet,
        in_handle.max_burst,
        out_handle.address,
        out_handle.max_packet,
        out_handle.max_burst,
    )) return false;
    refreshHandleFromCurrent(&in_handle.device);
    out_handle.device = in_handle.device;
    in_handle.endpoint_id = current.bulk_in_endpoint_id;
    in_handle.max_packet = current.bulk_in_endpoint_max_packet;
    in_handle.max_burst = current.bulk_in_endpoint_max_burst;
    out_handle.endpoint_id = current.bulk_out_endpoint_id;
    out_handle.max_packet = current.bulk_out_endpoint_max_packet;
    out_handle.max_burst = current.bulk_out_endpoint_max_burst;
    return true;
}

fn setupBulkEpContext(
    input: [*]u32,
    ctx_dwords: usize,
    dci: u8,
    in_dir: bool,
    ring_dequeue: u64,
    max_packet: u16,
    max_burst: u8,
) endpoint_context.BulkFields {
    const ep = endpointContext(input, ctx_dwords, dci);
    const fields = endpoint_context.bulkFields(current.addressed_speed, in_dir, max_packet, max_burst);
    ep[0] = fields.dword0;
    ep[1] = fields.dword1;
    ep[2] = @truncate(ring_dequeue);
    ep[3] = @truncate(ring_dequeue >> 32);
    ep[4] = fields.dword4;
    return fields;
}

fn endpointContext(input: [*]u32, ctx_dwords: usize, dci: u8) [*]u32 {
    return input + (ctx_dwords * (@as(usize, dci) + 1));
}

fn endpointDci(endpoint_address: u8) u8 {
    const number: u8 = endpoint_address & 0x0F;
    if (number == 0) return 0;
    return number * 2 + if ((endpoint_address & 0x80) != 0) @as(u8, 1) else @as(u8, 0);
}

fn endpointRingBase(endpoint_address: u8) ?u64 {
    if (endpoint_address == current.bulk_in_endpoint_address) return current.bulk_in_ring_phys;
    if (endpoint_address == current.bulk_out_endpoint_address) return current.bulk_out_ring_phys;
    if (endpoint_address == current.interrupt_endpoint_address) return current.interrupt_ring_phys;
    return null;
}

fn endpointRingDequeue(endpoint_address: u8) ?u64 {
    const base = endpointRingBase(endpoint_address) orelse return null;
    if (endpoint_address == current.bulk_in_endpoint_address) {
        return base + @as(u64, current.bulk_in_enqueue) * @sizeOf(Trb) |
            @as(u64, if (current.bulk_in_cycle != 0) 1 else 0);
    } else if (endpoint_address == current.bulk_out_endpoint_address) {
        return base + @as(u64, current.bulk_out_enqueue) * @sizeOf(Trb) |
            @as(u64, if (current.bulk_out_cycle != 0) 1 else 0);
    } else if (endpoint_address == current.interrupt_endpoint_address) {
        return base + @as(u64, current.interrupt_enqueue) * @sizeOf(Trb) |
            @as(u64, if (current.interrupt_cycle != 0) 1 else 0);
    }
    return null;
}

fn controlRingDequeue() ?u64 {
    if (current.ep0_ring_phys == 0 or first_ep0_ring_virt == 0) return null;
    return current.ep0_ring_phys +
        @as(u64, current.ep0_enqueue) * @sizeOf(Trb) |
        @as(u64, if (current.ep0_cycle != 0) 1 else 0);
}

fn endpointState(dci: u8) ?endpoint_recovery.EndpointState {
    if (first_device_virt == 0 or dci == 0) return null;
    dmaFence();
    // The output context is DMA-owned by the xHC.  Every recovery-state
    // iteration must perform a real memory read; a normal pointer lets the
    // compiler reuse the value across STOP/RESET command completions.
    const context_address = first_device_virt +
        @as(u64, current.context_size) * @as(u64, dci);
    return endpoint_recovery.stateFromRaw(@truncate(volatileRead32(context_address) & 0x7));
}

const EndpointFaultClass = enum { control, interrupt, bulk };

fn markDciRecoveryFailed(dci: u8, fault_class: EndpointFaultClass) void {
    current.failures +%= 1;
    current.reason = "xHCI endpoint recovery failed";
    const incident_token = diag_screen.beginResolvableIncident();
    diag_screen.write("[XHCI] endpoint recovery failed slot=");
    diag_screen.writeDec(current.addressed_slot_id);
    diag_screen.write(" ep=");
    diag_screen.writeDec(dci);
    diag_screen.write(" state=");
    if (endpointState(dci)) |state| {
        diag_screen.writeDec(@intFromEnum(state));
    } else {
        diag_screen.write("unknown");
    }
    diag_screen.write(" cc=");
    diag_screen.writeDec(current.last_completion_code);
    diag_screen.endLine();
    _ = diag_screen.resolveIncident(incident_token);
    switch (fault_class) {
        .control => current.control_endpoint_faulted = true,
        .interrupt => current.interrupt_endpoint_faulted = true,
        .bulk => current.bulk_endpoints_faulted = true,
    }
}

fn markEndpointRecoveryFailed(endpoint_address: u8) void {
    const dci = endpointDci(endpoint_address);
    const fault_class: EndpointFaultClass =
        if (endpoint_address == current.interrupt_endpoint_address)
            .interrupt
        else
            .bulk;
    markDciRecoveryFailed(dci, fault_class);
}

fn endpointRecoveryCommandWithin(
    control: u32,
    parameter: u64,
    recovery_budget: ?*const usb_wait.Deadline,
) ?u8 {
    // A missing command completion must never inherit a successful/stall
    // completion code from an earlier recovery step.
    current.last_completion_code = 0xFF;
    const event = submitCommandWithin(parameter, 0, control, recovery_budget) orelse return null;
    current.last_completion_code = event.code;
    current.last_slot_id = event.slot_id;
    return event.code;
}

fn drainUnresolvedTransferDci(dci: u8) void {
    const owner = last_timed_out_sync_transfer orelse return;
    const belongs = switch (owner) {
        .transfer => |transfer| transfer.slot_id == current.addressed_slot_id and
            transfer.endpoint_id == dci,
        .command => false,
    };
    if (!belongs) return;
    _ = drainEventBatch(owner);
    _ = deferred_events.purge(owner);
    last_timed_out_sync_transfer = null;
    // USBMSC takes ownership before recovery. Any token still held here was
    // abandoned by a direct xHCI caller and reaches its terminal boundary
    // once the timed-out TD has been purged/skipped.
    closeLastSyncTransferIncident();
}

pub fn setFirstConfigurationForHid() bool {
    return setFirstConfiguration();
}

pub fn setConfigurationForHandle(handle: *DeviceHandle) bool {
    if (!selectDeviceHandle(handle)) return false;
    return setFirstConfiguration();
}

pub fn setFirstConfiguration() bool {
    current.set_configuration_attempted = true;
    current.set_configuration_ok = submitControlNoData(0x00, USB_REQ_SET_CONFIGURATION, current.config_value, 0);
    return current.set_configuration_ok;
}

fn recoveryPortReady(port: u8) bool {
    if (!current.present or
        !current.controller_running or
        current.mmio_virt == 0 or
        port == 0 or
        port > current.port_count_seen)
        return false;
    const portsc = readOp32(
        OP_PORT_BASE + @as(u64, port - 1) * OP_PORT_STRIDE,
    );
    return (portsc & PORTSC_CCS) != 0 and
        (portsc & PORTSC_PED) != 0 and
        portLinkState(portsc) == PORTSC_PLS_U0;
}

// Recovery must operate on the already-addressed device which failed. Never
// fall through to the normal selector: that path refreshes/reclaims runtimes
// and may enumerate a replacement on the same physical port.
fn selectExistingRuntimeForRecovery(
    handle: *DeviceHandle,
    recovery_budget: ?*const usb_wait.Deadline,
) bool {
    if (recovery_budget) |budget| {
        if (budget.expiredAny()) return false;
    }
    const index_value = runtimeIndexForIdentity(handle.slot_id, handle.port) orelse return false;
    if (!recoveryPortReady(handle.port)) return false;
    if (recovery_budget) |budget| {
        if (budget.expiredAny()) return false;
    }
    if (!activateRuntime(index_value)) return false;
    if (current.addressed_slot_id != handle.slot_id or
        current.addressed_port != handle.port)
        return false;
    if (recovery_budget) |budget| {
        if (budget.expiredAny()) return false;
    }
    refreshHandleFromCurrent(handle);
    return true;
}

pub fn resetMassStorageInterfaceForHandle(handle: *DeviceHandle, interface_number: u8) bool {
    if (!selectExistingRuntimeForRecovery(handle, null)) return false;
    return resetFirstMassStorageInterface(interface_number);
}

pub fn resetMassStorageInterfaceForHandleWithin(
    handle: *DeviceHandle,
    interface_number: u8,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    if (!selectExistingRuntimeForRecovery(handle, recovery_budget)) return false;
    return resetFirstMassStorageInterfaceWithin(interface_number, recovery_budget);
}

pub fn resetFirstMassStorageInterface(interface_number: u8) bool {
    return submitControlNoData(0x21, MSC_REQ_BULK_ONLY_RESET, 0, interface_number);
}

fn resetFirstMassStorageInterfaceWithin(
    interface_number: u8,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    return submitControlNoDataWithin(0x21, MSC_REQ_BULK_ONLY_RESET, 0, interface_number, recovery_budget);
}

pub fn clearEndpointHaltForHandle(handle: *DeviceHandle, endpoint_address: u8) bool {
    if (!selectExistingRuntimeForRecovery(handle, null)) return false;
    return clearFirstEndpointHalt(endpoint_address);
}

pub fn clearEndpointHaltForHandleWithin(
    handle: *DeviceHandle,
    endpoint_address: u8,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    if (!selectExistingRuntimeForRecovery(handle, recovery_budget)) return false;
    return clearFirstEndpointHaltWithin(endpoint_address, recovery_budget);
}

pub fn clearFirstEndpointHalt(endpoint_address: u8) bool {
    return submitControlNoData(0x02, USB_REQ_CLEAR_FEATURE, USB_FEATURE_ENDPOINT_HALT, endpoint_address);
}

fn clearFirstEndpointHaltWithin(
    endpoint_address: u8,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    return submitControlNoDataWithin(
        0x02,
        USB_REQ_CLEAR_FEATURE,
        USB_FEATURE_ENDPOINT_HALT,
        endpoint_address,
        recovery_budget,
    );
}

// BOT reset recovery clears the device-side halt/toggle, but STOP plus
// SET_TR_DEQUEUE_POINTER does not reset the xHC's data-toggle/sequence state.
// Once both bulk endpoints are safely stopped, one Configure Endpoint command
// with Drop+Add for both directions reinitializes that host-side state as one
// indivisible recovery step (xHCI 4.6.6).
pub fn reconfigureMassStorageBulkEndpointsForHandleWithin(
    handle: *DeviceHandle,
    in_endpoint_address: u8,
    out_endpoint_address: u8,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    if (!selectExistingRuntimeForRecovery(handle, recovery_budget)) return false;
    if (!reconfigureStoppedBulkEndpointsWithin(
        in_endpoint_address,
        out_endpoint_address,
        recovery_budget,
    )) return false;
    refreshHandleFromCurrent(handle);
    return true;
}

fn reconfigureStoppedBulkEndpointsWithin(
    in_endpoint_address: u8,
    out_endpoint_address: u8,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    // Every early validation/budget/state failure belongs to this Configure
    // attempt; never report a successful completion code left by an earlier
    // Stop Endpoint or Set TR Dequeue Pointer command.
    current.last_completion_code = 0xFF;
    const in_dci = endpointDci(in_endpoint_address);
    const out_dci = endpointDci(out_endpoint_address);
    if (current.addressed_slot_id == 0 or
        first_input_virt == 0 or
        first_device_virt == 0 or
        in_dci == 0 or
        out_dci == 0 or
        in_dci >= 32 or
        out_dci >= 32 or
        (in_endpoint_address & 0x80) == 0 or
        (out_endpoint_address & 0x80) != 0 or
        in_endpoint_address != current.bulk_in_endpoint_address or
        out_endpoint_address != current.bulk_out_endpoint_address)
    {
        return failBulkReconfigure("xHCI BOT bulk identity invalid");
    }
    if (recovery_budget.expiredAny()) {
        return failBulkReconfigure("xHCI BOT bulk reconfigure budget expired");
    }
    const in_state = endpointState(in_dci) orelse
        return failBulkReconfigure("xHCI BOT bulk IN state missing");
    const out_state = endpointState(out_dci) orelse
        return failBulkReconfigure("xHCI BOT bulk OUT state missing");
    if (in_state != .stopped or out_state != .stopped) {
        return failBulkReconfigure("xHCI BOT bulk endpoint not stopped");
    }
    const in_dequeue = endpointRingDequeue(in_endpoint_address) orelse
        return failBulkReconfigure("xHCI BOT bulk IN dequeue missing");
    const out_dequeue = endpointRingDequeue(out_endpoint_address) orelse
        return failBulkReconfigure("xHCI BOT bulk OUT dequeue missing");

    const input: [*]u32 = @ptrFromInt(first_input_virt);
    @memset(input[0 .. INPUT_CONTEXT_BYTES / 4], 0);
    const ctx_dwords = @as(usize, current.context_size) / 4;
    const in_bit = @as(u32, 1) << @as(u5, @intCast(in_dci));
    const out_bit = @as(u32, 1) << @as(u5, @intCast(out_dci));
    // D0/A0 follows the Configure Endpoint Drop+Add rule: Slot is Add-only;
    // EP0 must be absent; both bulk DCIs appear in both masks.
    input[0] = in_bit | out_bit;
    input[1] = 0x1 | in_bit | out_bit;

    dmaFence();
    copyOutputSlotContextToInput(input + ctx_dwords);
    const in_ep = endpointContext(input, ctx_dwords, in_dci);
    const out_ep = endpointContext(input, ctx_dwords, out_dci);
    copyOutputEndpointContextToInput(in_ep, in_dci);
    copyOutputEndpointContextToInput(out_ep, out_dci);
    in_ep[2] = @truncate(in_dequeue);
    in_ep[3] = @truncate(in_dequeue >> 32);
    out_ep[2] = @truncate(out_dequeue);
    out_ep[3] = @truncate(out_dequeue >> 32);

    current.bulk_endpoints_configured = false;
    current.bulk_endpoints_faulted = true;
    const control = trbType(TRB_TYPE_CONFIGURE_ENDPOINT) |
        (@as(u32, current.addressed_slot_id) << 24);
    const completion = submitCommandWithin(
        current.input_context_phys,
        0,
        control,
        recovery_budget,
    ) orelse return failBulkReconfigure("xHCI BOT bulk reconfigure timeout");
    current.last_completion_code = completion.code;
    current.last_slot_id = completion.slot_id;
    if (completion.code != COMPLETION_SUCCESS or
        completion.slot_id != current.addressed_slot_id)
    {
        return failBulkReconfigure("xHCI BOT bulk reconfigure rejected");
    }
    current.bulk_endpoints_configured = true;
    current.bulk_endpoints_faulted = false;
    return true;
}

fn copyOutputSlotContextToInput(destination: [*]u32) void {
    const source = first_device_virt;
    // Only architected Input Slot Context fields may cross from the xHC-owned
    // Output Context. Device Address, Slot State and every RsvdO byte are
    // output-only; RsvdZ fields must be submitted as zero.
    destination[0] = volatileRead32(source) & ~(@as(u32, 1) << 24);
    destination[1] = volatileRead32(source + 4);
    destination[2] = volatileRead32(source + 8) & ~(@as(u32, 0xF) << 18);
    destination[3] = 0;
}

fn copyOutputEndpointContextToInput(destination: [*]u32, dci: u8) void {
    const source = first_device_virt +
        @as(u64, current.context_size) * @as(u64, dci);
    // Endpoint State plus DW0[7:3] are not valid Input fields. DW1 bit 0 and
    // bit 6 are RsvdZ. Input storage is already zeroed, so copy only DW0..4
    // and leave the CSZ-dependent RsvdO tail untouched.
    destination[0] = volatileRead32(source) & 0xFFFF_FF00;
    destination[1] = volatileRead32(source + 4) & ~@as(u32, 0x41);
    destination[4] = volatileRead32(source + 16);
}

fn failBulkReconfigure(reason: []const u8) bool {
    current.failures +%= 1;
    current.reason = reason;
    current.bulk_endpoints_configured = false;
    current.bulk_endpoints_faulted = true;
    const incident_token = diag_screen.beginResolvableIncident();
    diag_screen.write("[XHCI] BOT bulk reconfigure failed slot=");
    diag_screen.writeDec(current.addressed_slot_id);
    diag_screen.write(" in=");
    diag_screen.writeDec(current.bulk_in_endpoint_id);
    diag_screen.write(" out=");
    diag_screen.writeDec(current.bulk_out_endpoint_id);
    diag_screen.write(" cc=");
    diag_screen.writeDec(current.last_completion_code);
    diag_screen.endLine();
    _ = diag_screen.resolveIncident(incident_token);
    return false;
}

pub fn resetEndpointStateForHandle(handle: *DeviceHandle, endpoint_address: u8) bool {
    if (!selectExistingRuntimeForRecovery(handle, null)) return false;
    return resetFirstEndpointState(endpoint_address);
}

pub fn resetEndpointStateForHandleWithin(
    handle: *DeviceHandle,
    endpoint_address: u8,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    if (!selectExistingRuntimeForRecovery(handle, recovery_budget)) return false;
    return resetFirstEndpointStateWithin(endpoint_address, recovery_budget);
}

pub fn resetFirstEndpointState(endpoint_address: u8) bool {
    return resetFirstEndpointStateWithinOptional(endpoint_address, null);
}

fn resetFirstEndpointStateWithin(
    endpoint_address: u8,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    return resetFirstEndpointStateWithinOptional(endpoint_address, recovery_budget);
}

fn resetFirstEndpointStateWithinOptional(
    endpoint_address: u8,
    recovery_budget: ?*const usb_wait.Deadline,
) bool {
    if (current.addressed_slot_id == 0) return false;
    const dci = endpointDci(endpoint_address);
    if (dci == 0 or dci >= 32) return false;
    const dequeue = endpointRingDequeue(endpoint_address) orelse {
        markEndpointRecoveryFailed(endpoint_address);
        return false;
    };
    const fault_class: EndpointFaultClass =
        if (endpoint_address == current.interrupt_endpoint_address)
            .interrupt
        else
            .bulk;
    return recoverEndpointDci(dci, dequeue, fault_class, recovery_budget);
}

fn recoverControlEndpoint(recovery_budget: ?*const usb_wait.Deadline) bool {
    if (current.addressed_slot_id == 0) return false;
    const dequeue = controlRingDequeue() orelse {
        markDciRecoveryFailed(1, .control);
        return false;
    };
    return recoverEndpointDci(1, dequeue, .control, recovery_budget);
}

fn recoverEndpointDci(
    dci: u8,
    dequeue: u64,
    fault_class: EndpointFaultClass,
    recovery_budget: ?*const usb_wait.Deadline,
) bool {
    if (dci == 0 or dci >= 32) return false;
    const ep_field = @as(u32, dci) << 16;
    const slot_field = @as(u32, current.addressed_slot_id) << 24;
    // Endpoint state is asynchronous.  In particular, a Running endpoint
    // may become Halted/Error while STOP_ENDPOINT is being executed; xHCI
    // reports that specified race as Context State Error.  Re-read and
    // continue the bounded state machine instead of invalidating USBMSC.
    var attempt: u8 = 0;
    while (attempt < ENDPOINT_RECOVERY_COMMAND_ATTEMPTS) : (attempt += 1) {
        if (recovery_budget) |budget| {
            if (budget.expiredAny()) break;
        }
        const state = endpointState(dci) orelse break;
        const action = endpoint_recovery.prepareAction(state);
        const control = switch (action) {
            .stop => trbType(TRB_TYPE_STOP_ENDPOINT) | ep_field | slot_field,
            .reset => trbType(TRB_TYPE_RESET_ENDPOINT) | ep_field | slot_field,
            .set_dequeue => trbType(TRB_TYPE_SET_TR_DEQUEUE_POINTER) | ep_field | slot_field,
            .reconfigure => break,
        };
        const parameter: u64 = if (action == .set_dequeue) dequeue else 0;
        const code = endpointRecoveryCommandWithin(control, parameter, recovery_budget) orelse break;
        if (endpoint_recovery.completionNeedsStateRefresh(code)) continue;
        if (code != COMPLETION_SUCCESS) break;
        if (action == .set_dequeue) {
            // Never rewind a live transfer ring.  STOP/RESET first, then
            // skip the failed TD by publishing the current producer+DCS.
            drainUnresolvedTransferDci(dci);
            switch (fault_class) {
                .control => current.control_endpoint_faulted = false,
                .interrupt => current.interrupt_endpoint_faulted = false,
                .bulk => current.bulk_endpoints_faulted = false,
            }
            return true;
        }
        // STOP and RESET publish the new state before their command
        // completion. Re-read it and finish with SET_TR_DEQUEUE_POINTER.
    }
    markDciRecoveryFailed(dci, fault_class);
    return false;
}

pub fn setHidBootProtocolForHandle(handle: *DeviceHandle, interface_number: u8) bool {
    if (!selectDeviceHandle(handle)) return false;
    return setFirstHidBootProtocol(interface_number);
}

pub fn setFirstHidBootProtocol(interface_number: u8) bool {
    current.hid_set_protocol_attempted = true;
    current.hid_set_protocol_ok = submitControlNoData(0x21, HID_REQ_SET_PROTOCOL, 0, interface_number);
    return current.hid_set_protocol_ok;
}

pub fn setHidIdleForHandle(handle: *DeviceHandle, interface_number: u8) bool {
    if (!selectDeviceHandle(handle)) return false;
    return setFirstHidIdle(interface_number);
}

pub fn setFirstHidIdle(interface_number: u8) bool {
    current.hid_set_idle_attempted = true;
    current.hid_set_idle_ok = submitControlNoData(0x21, HID_REQ_SET_IDLE, 0, interface_number);
    return current.hid_set_idle_ok;
}

pub fn getHidReportDescriptorForHandle(handle: *DeviceHandle, interface_number: u8, expected_len: u16, out: []u8) ?usize {
    if (!selectDeviceHandle(handle)) return null;
    if (expected_len == 0 or out.len == 0 or first_descriptor_virt == 0) return null;
    var len: u16 = expected_len;
    if (len > DESCRIPTOR_BYTES) len = @intCast(DESCRIPTOR_BYTES);
    if (len > out.len) len = @intCast(out.len);
    const desc: [*]u8 = @ptrFromInt(first_descriptor_virt);
    @memset(desc[0..len], 0);
    if (!submitControlInTyped(0x81, USB_REQ_GET_DESCRIPTOR, @as(u16, USB_DESC_REPORT) << 8, interface_number, len, current.descriptor_phys)) return null;
    // submitControlWithin rejects impossible residue and a Success event with
    // a non-zero residue, so this subtraction is now proof-carrying.
    const actual_u32 = @as(u32, len) - current.last_control_residue;
    const actual: usize = @intCast(actual_u32);
    var i: usize = 0;
    while (i < actual) : (i += 1) out[i] = desc[i];
    refreshHandleFromCurrent(handle);
    return actual;
}

pub fn pollFirstInterruptInReport(out: []u8) bool {
    return pollFirstInterruptInReportStatus(out) == .report;
}

pub fn pollFirstInterruptInReportStatus(out: []u8) InterruptPollResult {
    if (!current.interrupt_endpoint_configured or first_interrupt_ring_virt == 0 or first_interrupt_buffer_virt == 0) return .failed;
    if (out.len == 0 or out.len > INTERRUPT_BUFFER_BYTES) return .failed;
    current.interrupt_polls += 1;
    if (current.interrupt_pending and current.interrupt_pending_trb_phys == 0) {
        current.interrupt_pending = false;
        current.interrupt_pending_streak = 0;
    }
    if (!current.interrupt_pending) {
        const buf: [*]u8 = @ptrFromInt(first_interrupt_buffer_virt);
        @memset(buf[0..out.len], 0);
        current.last_interrupt_request_len = @intCast(out.len);
        current.last_interrupt_residue = 0;
        current.last_interrupt_actual_len = 0;
        current.last_interrupt_completion_code = 0;
        const submission = writeInterruptTrb(
            current.interrupt_buffer_phys,
            @intCast(out.len),
            trbType(TRB_TYPE_NORMAL) | TRB_IOC,
        );
        current.interrupt_pending_trb_phys = submission.trb_phys;
        dmaFence();
        current.interrupt_pending = true;
        current.interrupt_pending_streak = 0;
        writeDoorbell(current.addressed_slot_id, current.interrupt_endpoint_id);
        if (submission.advance.wrapped) {
            current.interrupt_ring_wraps += 1;
            emitInterruptWrapMarker(submission.enqueue, submission.producer_cycle, submission.advance);
        }
    }
    const expected = event_router.transferMatch(
        current.addressed_slot_id,
        current.interrupt_endpoint_id,
        current.interrupt_pending_trb_phys,
    );
    const completion = pollInterruptCompletion(expected) orelse {
        current.interrupt_no_report += 1;
        // 0.56.16: Proaktiver Halted-Check (Befund 7.2/7.3). Ein lange
        // pendender Transfer ist bei HID normal (SET_IDLE=0: Report nur
        // bei Aenderung) - ein Fehlerfall liegt erst vor, wenn der EP im
        // Output-Device-Context Halted/Error meldet. Dann Recovery und
        // frisch aufsetzen; ohne Befund bleibt der Transfer stehen.
        current.interrupt_pending_streak +%= 1;
        if ((current.interrupt_pending_streak & PENDING_HALT_CHECK_MASK) == 0) {
            if (interruptEndpointState()) |state| {
                current.last_interrupt_ep_state = state;
                if (state == EP_STATE_HALTED or state == EP_STATE_ERROR) {
                    _ = recoverInterruptEndpoint(true);
                }
            }
        }
        return .no_report;
    };
    current.last_transfer_completion_code = completion.code;
    current.last_interrupt_completion_code = completion.code;
    current.last_interrupt_residue = completion.length;
    current.interrupt_pending = false;
    current.interrupt_pending_trb_phys = 0;
    current.interrupt_pending_streak = 0;
    // Short Packet ist fuer Interrupt-IN kein Endpoint-Fehler. Die Residue
    // beschreibt dabei genau die kuerzere Nutzlaenge des HID-Reports.
    const requested = current.last_interrupt_request_len;
    const completion_ok =
        (completion.code == COMPLETION_SUCCESS and completion.length == 0) or
        (completion.code == COMPLETION_SHORT_PACKET and completion.length <= requested);
    if (!completion_ok) {
        current.last_interrupt_actual_len = 0;
        current.failures += 1;
        // 0.56.16: Fehler-Completion (Stall/Babble/Transaction-Error)
        // haltet den Endpoint. RESET_ENDPOINT + SET_TR_DEQUEUE_POINTER
        // statt dauerhaft .failed - der naechste Poll setzt frisch auf
        // (Maus-tot-Root-Cause vom Produktionspfad).
        _ = recoverInterruptEndpoint(false);
        return .no_report;
    }
    current.last_interrupt_actual_len = requested - completion.length;
    if (current.last_interrupt_actual_len == 0) {
        current.interrupt_no_report += 1;
        return .no_report;
    }
    const buf: [*]u8 = @ptrFromInt(first_interrupt_buffer_virt);
    const copy_len: usize = @min(out.len, @as(usize, @intCast(current.last_interrupt_actual_len)));
    var i: usize = 0;
    while (i < copy_len) : (i += 1) out[i] = buf[i];
    current.interrupt_reports += 1;
    return .report;
}

pub fn lastInterruptActualLength() usize {
    return @intCast(current.last_interrupt_actual_len);
}

pub fn lastInterruptRequestedLength() usize {
    return @intCast(current.last_interrupt_request_len);
}

pub fn lastInterruptResidue() u32 {
    return current.last_interrupt_residue;
}

/// Transfers ownership of the most recent synchronous-transfer timeout
/// generation to the protocol layer. Once taken, a later xHCI operation must
/// not close that exact incident behind the protocol owner's back.
pub fn takeLastSyncTransferIncidentToken() diag_screen.IncidentToken {
    const token = last_sync_transfer_incident;
    last_sync_transfer_incident = .{};
    return token;
}

fn closeLastSyncTransferIncident() void {
    if (last_sync_transfer_incident.valid()) {
        _ = diag_screen.resolveIncident(last_sync_transfer_incident);
        last_sync_transfer_incident = .{};
    }
}

pub fn pollInterruptInReport(handle: *EndpointHandle, out: []u8) bool {
    return pollInterruptInReportStatus(handle, out) == .report;
}

pub fn pollInterruptInReportStatus(handle: *EndpointHandle, out: []u8) InterruptPollResult {
    if (handle.kind != .interrupt_in) return .failed;
    if (!selectDeviceHandle(&handle.device)) return .failed;
    if (current.interrupt_endpoint_faulted) return .failed;
    if (!current.interrupt_endpoint_configured or current.interrupt_endpoint_address != handle.address) {
        if (!configureInterruptInEndpointHandle(handle)) return .failed;
    }
    return pollFirstInterruptInReportStatus(out);
}

pub fn bulkOut(data: []const u8) bool {
    beginBulkRecord(false, data.len);
    if (!current.bulk_endpoints_configured or current.bulk_endpoints_faulted or first_bulk_out_ring_virt == 0 or first_bulk_buffer_virt == 0) {
        current.last_bulk_result = "not-ready";
        return false;
    }
    if (data.len == 0 or data.len > INTERRUPT_BUFFER_BYTES) {
        current.last_bulk_result = "bad-length";
        return false;
    }
    if (active_sync_transfer != null) {
        current.last_bulk_result = "busy";
        return false;
    }
    const buf: [*]u8 = @ptrFromInt(first_bulk_buffer_virt);
    @memcpy(buf[0..data.len], data);
    const submission = writeBulkTrb(false, current.bulk_buffer_phys, @intCast(data.len), trbType(TRB_TYPE_NORMAL) | TRB_IOC);
    const expected = event_router.transferMatch(current.addressed_slot_id, current.bulk_out_endpoint_id, submission.trb_phys);
    active_sync_transfer = expected;
    defer active_sync_transfer = null;
    dmaFence();
    current.bulk_transfers += 1;
    writeDoorbell(current.addressed_slot_id, current.bulk_out_endpoint_id);
    const completion = waitTransferCompletion(expected) orelse {
        last_timed_out_sync_transfer = expected;
        current.bulk_failures += 1;
        current.last_bulk_result = "timeout";
        return false;
    };
    current.last_transfer_completion_code = completion.code;
    recordBulkCompletion(false, @intCast(data.len), completion);
    const evaluated = bulk_completion.evaluate(false, @intCast(data.len), completion.code, completion.length);
    if (!evaluated.accepted) {
        current.bulk_failures += 1;
        return false;
    }
    return true;
}

pub fn bulkOutForHandle(handle: *EndpointHandle, data: []const u8) bool {
    if (handle.kind != .bulk_out) return false;
    if (!selectDeviceHandle(&handle.device)) return false;
    if (!current.bulk_endpoints_configured or current.bulk_out_endpoint_address != handle.address) return false;
    return bulkOut(data);
}

pub fn bulkIn(out: []u8) bool {
    beginBulkRecord(true, out.len);
    if (!current.bulk_endpoints_configured or current.bulk_endpoints_faulted or first_bulk_in_ring_virt == 0 or first_bulk_buffer_virt == 0) {
        current.last_bulk_result = "not-ready";
        return false;
    }
    if (out.len == 0 or out.len > INTERRUPT_BUFFER_BYTES) {
        current.last_bulk_result = "bad-length";
        return false;
    }
    if (active_sync_transfer != null) {
        current.last_bulk_result = "busy";
        return false;
    }
    const buf: [*]u8 = @ptrFromInt(first_bulk_buffer_virt);
    @memset(buf[0..out.len], 0);
    const submission = writeBulkTrb(true, current.bulk_buffer_phys, @intCast(out.len), trbType(TRB_TYPE_NORMAL) | TRB_IOC);
    const expected = event_router.transferMatch(current.addressed_slot_id, current.bulk_in_endpoint_id, submission.trb_phys);
    active_sync_transfer = expected;
    defer active_sync_transfer = null;
    dmaFence();
    current.bulk_transfers += 1;
    writeDoorbell(current.addressed_slot_id, current.bulk_in_endpoint_id);
    const completion = waitTransferCompletion(expected) orelse {
        last_timed_out_sync_transfer = expected;
        current.bulk_failures += 1;
        current.last_bulk_result = "timeout";
        return false;
    };
    current.last_transfer_completion_code = completion.code;
    recordBulkCompletion(true, @intCast(out.len), completion);
    const evaluated = bulk_completion.evaluate(true, @intCast(out.len), completion.code, completion.length);
    if (!evaluated.accepted) {
        current.bulk_failures += 1;
        return false;
    }
    const actual: usize = @intCast(evaluated.actual_len);
    var i: usize = 0;
    while (i < actual) : (i += 1) out[i] = buf[i];
    return true;
}

pub fn bulkInForHandle(handle: *EndpointHandle, out: []u8) bool {
    if (handle.kind != .bulk_in) return false;
    if (!selectDeviceHandle(&handle.device)) return false;
    if (!current.bulk_endpoints_configured or current.bulk_in_endpoint_address != handle.address) return false;
    return bulkIn(out);
}

fn pollInterruptCompletion(expected: event_router.Match) ?XhciEvent {
    return pollMatchingEvent(expected);
}

const BulkSubmission = struct {
    trb_phys: u64,
};

fn writeBulkTrb(in_dir: bool, parameter: u64, status_value: u32, control_extra: u32) BulkSubmission {
    const ring: [*]Trb = @ptrFromInt(if (in_dir) first_bulk_in_ring_virt else first_bulk_out_ring_virt);
    const enqueue = if (in_dir) current.bulk_in_enqueue else current.bulk_out_enqueue;
    const ring_phys = if (in_dir) current.bulk_in_ring_phys else current.bulk_out_ring_phys;
    const trb_phys = ring_phys + @as(u64, enqueue) * @sizeOf(Trb);
    const cycle = if ((if (in_dir) current.bulk_in_cycle else current.bulk_out_cycle) != 0) TRB_CYCLE else 0;
    ring[enqueue] = .{ .parameter = parameter, .status = status_value, .control = control_extra | cycle };
    const producer_cycle = if (in_dir) current.bulk_in_cycle else current.bulk_out_cycle;
    const advance = ring_cycle.afterSubmission(enqueue, producer_cycle, TRANSFER_TRB_COUNT);
    if (advance.wrapped) {
        updateLinkTrbCycle(
            if (in_dir) first_bulk_in_ring_virt else first_bulk_out_ring_virt,
            TRANSFER_TRB_COUNT,
            advance.link_cycle,
        );
    }
    if (in_dir) {
        current.bulk_in_enqueue = advance.next_enqueue;
        current.bulk_in_cycle = advance.next_producer_cycle;
        current.bulk_in_link_update_pending = false;
        if (advance.wrapped) {
            current.bulk_in_ring_wraps += 1;
        }
    } else {
        current.bulk_out_enqueue = advance.next_enqueue;
        current.bulk_out_cycle = advance.next_producer_cycle;
        current.bulk_out_link_update_pending = false;
        if (advance.wrapped) {
            current.bulk_out_ring_wraps += 1;
        }
    }
    return .{ .trb_phys = trb_phys };
}

fn beginBulkRecord(in_dir: bool, requested: usize) void {
    // A caller must take a timeout token before starting another transfer.
    // If it did not, the previous operation has reached a terminal boundary;
    // close its generation instead of silently discarding the only owner.
    closeLastSyncTransferIncident();
    current.last_bulk_direction = if (in_dir) "in" else "out";
    current.last_bulk_result = "submitted";
    current.last_bulk_request_len = @intCast(@min(requested, @as(usize, 0xFFFF_FFFF)));
    current.last_bulk_residue = 0;
    current.last_bulk_actual_len = 0;
    current.last_bulk_completion_code = 0;
}

fn recordBulkCompletion(in_dir: bool, requested: u32, completion: XhciEvent) void {
    current.last_bulk_completion_code = completion.code;
    current.last_bulk_residue = completion.length;
    const evaluated = bulk_completion.evaluate(in_dir, requested, completion.code, completion.length);
    current.last_bulk_actual_len = evaluated.actual_len;
    current.last_bulk_result = if (evaluated.accepted)
        (if (completion.code == COMPLETION_SHORT_PACKET) "short" else "ok")
    else if (completion.length > requested or
        (completion.code == COMPLETION_SUCCESS and completion.length != 0))
        "bad-residue"
    else
        "completion-error";
}

fn controlDirectionName(direction: ControlDirection) []const u8 {
    return switch (direction) {
        .none => "none",
        .in => "in",
        .out => "out",
    };
}

fn recordControlRequest(req: *const ControlRequest) void {
    current.last_control_request_type = req.request_type;
    current.last_control_request = req.request;
    current.last_control_value = req.value;
    current.last_control_index = req.index_value;
    current.last_control_length = req.length;
    current.last_control_direction = controlDirectionName(req.direction);
    current.last_control_completion_code = 0;
    current.last_control_residue = 0;
    current.last_control_ok = false;
}

fn submitControl(req: ControlRequest) bool {
    return submitControlWithin(req, null);
}

fn submitControlWithin(
    req: ControlRequest,
    recovery_budget: ?*const usb_wait.Deadline,
) bool {
    // Starting another synchronous transfer abandons an unclaimed earlier
    // timeout token. USBMSC takes its token before entering BOT recovery.
    closeLastSyncTransferIncident();
    if (current.addressed_slot_id == 0 or first_ep0_ring_virt == 0) return false;
    if (current.control_endpoint_faulted) return false;
    if (req.direction != .none and (req.length == 0 or req.data_phys == 0)) return false;
    if (active_sync_transfer != null) return false;
    if (recovery_budget) |budget| {
        if (budget.expiredAny()) return false;
    }
    recordControlRequest(&req);

    const setup_transfer_type: u32 = switch (req.direction) {
        .none => 0,
        .in => SETUP_TRT_IN_DATA,
        .out => SETUP_TRT_OUT_DATA,
    };
    const setup = (@as(u64, req.request_type)) |
        (@as(u64, req.request) << 8) |
        (@as(u64, req.value) << 16) |
        (@as(u64, req.index_value) << 32) |
        (@as(u64, req.length) << 48);
    var control_trb_phys: [event_router.MAX_TRANSFER_TRB_POINTERS]u64 = .{0} ** event_router.MAX_TRANSFER_TRB_POINTERS;
    var control_trb_count: u8 = 0;
    control_trb_phys[control_trb_count] = writeTransferTrb(setup, 8, trbType(TRB_TYPE_SETUP_STAGE) | TRB_IDT | setup_transfer_type);
    control_trb_count += 1;

    if (req.direction != .none) {
        const data_control = trbType(TRB_TYPE_DATA_STAGE) | if (req.direction == .in) DATA_STAGE_DIR_IN else 0;
        control_trb_phys[control_trb_count] = writeTransferTrb(req.data_phys, req.length, data_control);
        control_trb_count += 1;
    }

    const status_direction = req.direction != .in;
    const status_control = trbType(TRB_TYPE_STATUS_STAGE) | TRB_IOC | if (status_direction) STATUS_STAGE_DIR_IN else 0;
    control_trb_phys[control_trb_count] = writeTransferTrb(0, 0, status_control);
    control_trb_count += 1;
    const expected = event_router.transferTdMatch(current.addressed_slot_id, 1, control_trb_phys, control_trb_count);
    active_sync_transfer = expected;
    defer active_sync_transfer = null;
    dmaFence();
    current.control_transfers += 1;
    writeDoorbell(current.addressed_slot_id, 1);
    const completion = waitTransferCompletionWithin(expected, recovery_budget) orelse {
        last_timed_out_sync_transfer = expected;
        current.last_control_completion_code = 0xFF;
        current.control_timeouts += 1;
        current.control_failures += 1;
        _ = recoverControlEndpoint(recovery_budget);
        // EP0 has no protocol-layer timeout owner. The recovery diagnostics
        // have already joined this generation, so retain their evidence and
        // close it at the control-operation boundary.
        closeLastSyncTransferIncident();
        return false;
    };
    current.last_transfer_completion_code = completion.code;
    current.last_control_completion_code = completion.code;
    current.last_control_residue = completion.length;
    const residue_valid = completion.length <= req.length;
    current.last_control_ok =
        (completion.code == COMPLETION_SUCCESS and completion.length == 0) or
        (req.direction == .in and
            completion.code == COMPLETION_SHORT_PACKET and
            residue_valid);
    if (current.last_control_ok) {
        if (req.length != 0 and completion.length != 0) current.control_short_packets += 1;
        return true;
    }

    current.control_failures += 1;
    if (completion.code == COMPLETION_STALL_ERROR) current.control_stalls += 1;
    current.reason = "xHCI control transfer failed";
    _ = recoverControlEndpoint(recovery_budget);
    return false;
}

fn submitControlNoData(request_type: u8, request: u8, value: u16, index_value: u16) bool {
    return submitControlNoDataWithinOptional(request_type, request, value, index_value, null);
}

fn submitControlNoDataWithin(
    request_type: u8,
    request: u8,
    value: u16,
    index_value: u16,
    recovery_budget: *const usb_wait.Deadline,
) bool {
    return submitControlNoDataWithinOptional(
        request_type,
        request,
        value,
        index_value,
        recovery_budget,
    );
}

fn submitControlNoDataWithinOptional(
    request_type: u8,
    request: u8,
    value: u16,
    index_value: u16,
    recovery_budget: ?*const usb_wait.Deadline,
) bool {
    return submitControlWithin(.{
        .request_type = request_type,
        .request = request,
        .value = value,
        .index_value = index_value,
        .direction = .none,
    }, recovery_budget);
}

// 0.56.16: Endpoint-Halt-Recovery (Port der Backup-XHCI.R4D-Phase-4).
const EP_STATE_HALTED: u8 = 2;
const EP_STATE_ERROR: u8 = 4;
const PENDING_HALT_CHECK_MASK: u64 = 63; // EP-State alle 64 Pending-Polls lesen

// EP-State aus dem OUTPUT-Device-Context lesen. OFFSET-FALLE: der
// Device-Context hat KEINEN Input-Control-Context - Slot liegt bei 0,
// Endpoint-DCI n bei ctx_dwords*n (NICHT n+1 wie im Input-Context,
// vgl. endpointContext()). Ein falscher Offset laese den falschen EP
// und wuerde spurious resetten (Maus sofort tot).
fn interruptEndpointState() ?u8 {
    if (current.interrupt_endpoint_id == 0) return null;
    current.interrupt_halted_checks +%= 1;
    const state = endpointState(current.interrupt_endpoint_id) orelse return null;
    return @intFromEnum(state);
}

fn readInterruptHardwareDequeue() u64 {
    if (first_device_virt == 0 or current.interrupt_endpoint_id == 0) return 0;
    const context_bytes = @as(u64, current.context_size) * @as(u64, current.interrupt_endpoint_id);
    const endpoint = first_device_virt + context_bytes;
    const low = volatileRead32(endpoint + 8);
    const high = volatileRead32(endpoint + 12);
    return (@as(u64, high) << 32) | low;
}

// RESET_ENDPOINT holt den EP aus Halted, SET_TR_DEQUEUE_POINTER setzt den
// Controller-Cursor konsistent auf unsere aktuelle Enqueue-Position (mit
// DCS=aktuellem Cycle-Bit). Danach ist interrupt_pending falsch-frei und
// der naechste Poll setzt einen frischen Transfer auf.
fn recoverInterruptEndpoint(from_pending: bool) bool {
    const slot = current.addressed_slot_id;
    const ep_id = current.interrupt_endpoint_id;
    if (slot == 0 or ep_id == 0) return false;
    const pending_owner = currentInterruptOwnerMatch();
    if (pending_owner) |owner| _ = deferred_events.purge(owner);
    current.interrupt_recoveries +%= 1;
    if (from_pending) current.interrupt_pending_timeouts +%= 1;
    const ok = resetFirstEndpointState(current.interrupt_endpoint_address);
    if (pending_owner) |owner| _ = deferred_events.purge(owner);
    current.interrupt_pending = false;
    current.interrupt_pending_trb_phys = 0;
    current.interrupt_pending_streak = 0;
    dmaFence();
    return ok;
}

const InterruptSubmission = struct {
    trb_phys: u64,
    enqueue: u16,
    producer_cycle: u8,
    advance: ring_cycle.Advance,
};

fn writeInterruptTrb(parameter: u64, status_value: u32, control_extra: u32) InterruptSubmission {
    const ring: [*]Trb = @ptrFromInt(first_interrupt_ring_virt);
    const enqueue = current.interrupt_enqueue;
    const trb_phys = current.interrupt_ring_phys + @as(u64, enqueue) * @sizeOf(Trb);
    const producer_cycle = current.interrupt_cycle;
    const cycle = if (producer_cycle != 0) TRB_CYCLE else 0;
    ring[enqueue] = .{ .parameter = parameter, .status = status_value, .control = control_extra | cycle };
    const advance = ring_cycle.afterSubmission(enqueue, producer_cycle, TRANSFER_TRB_COUNT);
    if (advance.wrapped) {
        // Der Link gehoert noch zum alten Umlauf: erst mit dem alten PCS an
        // den xHC uebergeben, dann lokal auf das PCS des neuen Umlaufs gehen.
        updateLinkTrbCycle(first_interrupt_ring_virt, TRANSFER_TRB_COUNT, advance.link_cycle);
    }
    current.interrupt_enqueue = advance.next_enqueue;
    current.interrupt_cycle = advance.next_producer_cycle;
    return .{
        .trb_phys = trb_phys,
        .enqueue = enqueue,
        .producer_cycle = producer_cycle,
        .advance = advance,
    };
}

fn emitInterruptWrapMarker(enqueue: u16, producer_cycle: u8, advance: ring_cycle.Advance) void {
    const deferred = deferred_events.snapshot();
    k.puts("[USBHIDWRAP] wraps=");
    k.putDec(current.interrupt_ring_wraps);
    k.puts(" submitted_enqueue=");
    k.putDec(enqueue);
    k.puts(" enqueue=");
    k.putDec(current.interrupt_enqueue);
    k.puts(" next_enqueue=");
    k.putDec(advance.next_enqueue);
    k.puts(" submitted_pcs=");
    k.putDec(producer_cycle);
    k.puts(" pcs=");
    k.putDec(current.interrupt_cycle);
    k.puts(" next_producer_cycle=");
    k.putDec(advance.next_producer_cycle);
    k.puts(" link_c=");
    k.putDec(advance.link_cycle);
    k.puts(" reports=");
    k.putDec(current.interrupt_reports);
    k.puts(" recoveries=");
    k.putDec(current.interrupt_recoveries);
    k.puts(" pending_timeouts=");
    k.putDec(current.interrupt_pending_timeouts);
    k.puts(" deferred_overflows=");
    k.putDec(deferred.overflows);
    k.puts(" event_wraps=");
    k.putDec(current.event_ring_wraps);
    k.puts(" event_commits=");
    k.putDec(current.event_erdp_commits);
    k.puts(" ring_full=");
    k.putDec(current.ring_full);
    k.puts(" stale_events=");
    k.putDec(current.stale_events);
    k.puts("\r\n");
}

fn submitControlIn(request: u8, value: u16, index_value: u16, length: u16, data_phys: u64) bool {
    return submitControlInTyped(0x80, request, value, index_value, length, data_phys);
}

fn submitControlInTyped(request_type: u8, request: u8, value: u16, index_value: u16, length: u16, data_phys: u64) bool {
    return submitControl(.{
        .request_type = request_type,
        .request = request,
        .value = value,
        .index_value = index_value,
        .length = length,
        .data_phys = data_phys,
        .direction = .in,
    });
}

fn writeTransferTrb(parameter: u64, status_value: u32, control_extra: u32) u64 {
    const ring: [*]Trb = @ptrFromInt(first_ep0_ring_virt);
    const enqueue = current.ep0_enqueue;
    const trb_phys = current.ep0_ring_phys + @as(u64, enqueue) * @sizeOf(Trb);
    const cycle = if (current.ep0_cycle != 0) TRB_CYCLE else 0;
    ring[enqueue] = .{ .parameter = parameter, .status = status_value, .control = control_extra | cycle };
    const advance = ring_cycle.afterSubmission(enqueue, current.ep0_cycle, TRANSFER_TRB_COUNT);
    if (advance.wrapped) {
        updateLinkTrbCycle(first_ep0_ring_virt, TRANSFER_TRB_COUNT, advance.link_cycle);
        current.ep0_ring_wraps += 1;
    }
    current.ep0_enqueue = advance.next_enqueue;
    current.ep0_cycle = advance.next_producer_cycle;
    return trb_phys;
}

fn waitTransferCompletion(expected: event_router.Match) ?XhciEvent {
    return waitTransferCompletionWithin(expected, null);
}

fn waitTransferCompletionWithin(
    expected: event_router.Match,
    recovery_budget: ?*const usb_wait.Deadline,
) ?XhciEvent {
    if (waitMatchingEventWithin(expected, recovery_budget)) |event| return event;
    current.timeouts += 1;
    current.reason = "timeout waiting for xHCI transfer event";
    return null;
}

fn parseDeviceDescriptor() void {
    const d: [*]u8 = @ptrFromInt(first_descriptor_virt);
    current.descriptor_len = d[0];
    current.descriptor_type = d[1];
    current.usb_version_bcd = readLe16(d, 2);
    current.device_class = d[4];
    current.device_subclass = d[5];
    current.device_protocol = d[6];
    current.device_max_packet0 = d[7];
    current.device_vendor_id = readLe16(d, 8);
    current.device_product_id = readLe16(d, 10);
    current.device_version_bcd = readLe16(d, 12);
    current.manufacturer_index = d[14];
    current.product_index = d[15];
    current.serial_index = d[16];
}

fn bumpDescriptorCounter(counter: *u8) void {
    if (counter.* != 0xFF) counter.* += 1;
}

fn isMassStorageCandidate(candidate: *const DescriptorInterface) bool {
    return candidate.valid and
        candidate.class_code == 0x08 and
        candidate.subclass == 0x06 and
        candidate.protocol == 0x50 and
        candidate.bulk_in_endpoint_address != 0 and
        candidate.bulk_out_endpoint_address != 0;
}

fn isBootHidCandidate(candidate: *const DescriptorInterface) bool {
    return candidate.valid and
        candidate.class_code == 0x03 and
        candidate.subclass == 0x01 and
        (candidate.protocol == 0x01 or candidate.protocol == 0x02) and
        candidate.interrupt_in_endpoint_address != 0;
}

fn descriptorCandidateRank(candidate: *const DescriptorInterface) u8 {
    if (isMassStorageCandidate(candidate)) return 4;
    if (isBootHidCandidate(candidate)) return 3;
    if (candidate.valid and candidate.first_endpoint_address != 0) return 2;
    if (candidate.valid) return 1;
    return 0;
}

fn descriptorCandidateReason(candidate: *const DescriptorInterface) []const u8 {
    if (isMassStorageCandidate(candidate)) return "mass-storage-bulk";
    if (isBootHidCandidate(candidate)) {
        if (candidate.protocol == 0x01) return "hid-boot-keyboard";
        if (candidate.protocol == 0x02) return "hid-boot-mouse";
        return "hid-boot";
    }
    if (candidate.valid and candidate.first_endpoint_address != 0) return "first-endpoint-interface";
    if (candidate.valid) return "first-interface";
    return "none";
}

fn selectDescriptorCandidate(candidate: *const DescriptorInterface, selected: *DescriptorInterface) void {
    if (!candidate.valid) return;
    if (descriptorCandidateRank(candidate) > descriptorCandidateRank(selected)) selected.* = candidate.*;
}

fn recordEndpointForCandidate(candidate: *DescriptorInterface, address: u8, attributes: u8, max_packet: u16, interval: u8) void {
    if (!candidate.valid) {
        bumpDescriptorCounter(&current.descriptor_malformed);
        return;
    }
    bumpDescriptorCounter(&candidate.endpoint_count);
    candidate.last_endpoint_address = address;
    if (candidate.first_endpoint_address == 0) {
        candidate.first_endpoint_address = address;
        candidate.first_endpoint_attributes = attributes;
        candidate.first_endpoint_max_packet = max_packet;
        candidate.first_endpoint_interval = interval;
    }

    const transfer_type = attributes & 0x03;
    if (transfer_type == 0x03 and (address & 0x80) != 0 and candidate.interrupt_in_endpoint_address == 0) {
        candidate.interrupt_in_endpoint_address = address;
        candidate.interrupt_in_endpoint_max_packet = max_packet;
        candidate.interrupt_in_endpoint_interval = interval;
    } else if (transfer_type == 0x02) {
        if ((address & 0x80) != 0 and candidate.bulk_in_endpoint_address == 0) {
            candidate.bulk_in_endpoint_address = address;
            candidate.bulk_in_endpoint_max_packet = max_packet;
        } else if ((address & 0x80) == 0 and candidate.bulk_out_endpoint_address == 0) {
            candidate.bulk_out_endpoint_address = address;
            candidate.bulk_out_endpoint_max_packet = max_packet;
        }
    }
}

fn recordSsEndpointCompanionForCandidate(candidate: *DescriptorInterface, descriptor: [*]const u8, len: u8) void {
    if (!candidate.valid or len < 6 or candidate.last_endpoint_address == 0) {
        bumpDescriptorCounter(&current.descriptor_malformed);
        candidate.last_endpoint_address = 0;
        return;
    }

    const max_burst = descriptor[2];
    if (max_burst > 15) {
        bumpDescriptorCounter(&current.descriptor_malformed);
        candidate.last_endpoint_address = 0;
        return;
    }

    const address = candidate.last_endpoint_address;
    if (address == candidate.bulk_in_endpoint_address) {
        candidate.bulk_in_endpoint_max_burst = max_burst;
    } else if (address == candidate.bulk_out_endpoint_address) {
        candidate.bulk_out_endpoint_max_burst = max_burst;
    }
    candidate.last_endpoint_address = 0;
}

fn recordHidDescriptorForCandidate(candidate: *DescriptorInterface, descriptor: [*]const u8, len: u8) void {
    if (!candidate.valid) {
        bumpDescriptorCounter(&current.descriptor_malformed);
        return;
    }
    candidate.hid_descriptor_len = len;
    if (len < 9) {
        bumpDescriptorCounter(&current.descriptor_malformed);
        return;
    }
    const descriptor_count = descriptor[5];
    var offset: usize = 6;
    var seen: u8 = 0;
    while (seen < descriptor_count and offset + 3 <= len) : (seen += 1) {
        const kind = descriptor[offset];
        const desc_len = readLe16(descriptor, offset + 1);
        if (kind == USB_DESC_REPORT and candidate.hid_report_descriptor_len == 0) {
            candidate.hid_report_descriptor_len = desc_len;
        }
        offset += 3;
    }
}

fn publishSelectedDescriptorInterface(selected: *const DescriptorInterface) void {
    current.selected_interface_reason = descriptorCandidateReason(selected);
    current.first_interface_number = selected.number;
    current.first_interface_class = selected.class_code;
    current.first_interface_subclass = selected.subclass;
    current.first_interface_protocol = selected.protocol;
    current.first_endpoint_address = selected.first_endpoint_address;
    current.first_endpoint_attributes = selected.first_endpoint_attributes;
    current.first_endpoint_max_packet = selected.first_endpoint_max_packet;
    current.first_endpoint_interval = selected.first_endpoint_interval;
    current.first_hid_descriptor_len = selected.hid_descriptor_len;
    current.first_hid_report_descriptor_len = selected.hid_report_descriptor_len;
    if (isBootHidCandidate(selected)) {
        current.first_endpoint_address = selected.interrupt_in_endpoint_address;
        current.first_endpoint_attributes = 0x03;
        current.first_endpoint_max_packet = selected.interrupt_in_endpoint_max_packet;
        current.first_endpoint_interval = selected.interrupt_in_endpoint_interval;
    }
    if (isMassStorageCandidate(selected)) {
        current.bulk_in_endpoint_address = selected.bulk_in_endpoint_address;
        current.bulk_in_endpoint_max_packet = selected.bulk_in_endpoint_max_packet;
        current.bulk_in_endpoint_max_burst = selected.bulk_in_endpoint_max_burst;
        current.bulk_out_endpoint_address = selected.bulk_out_endpoint_address;
        current.bulk_out_endpoint_max_packet = selected.bulk_out_endpoint_max_packet;
        current.bulk_out_endpoint_max_burst = selected.bulk_out_endpoint_max_burst;
    }
}

fn parseConfigDescriptor(total: u16) void {
    const d: [*]u8 = @ptrFromInt(first_descriptor_virt);
    current.config_total_length = readLe16(d, 2);
    current.interface_count = d[4];
    current.interface_record_count = 0;
    current.interface_records = .{usb_core.Interface{}} ** MAX_USB_INTERFACES;
    current.config_value = d[5];
    current.config_attributes = d[7];
    current.config_max_power_ma = @as(u16, d[8]) * 2;
    current.endpoint_count = 0;
    current.descriptor_records = 0;
    current.descriptor_unknown = 0;
    current.descriptor_malformed = 0;
    current.hid_descriptor_count = 0;
    current.ss_endpoint_companion_count = 0;
    current.selected_interface_reason = "none";
    current.first_interface_number = 0;
    current.first_interface_class = 0;
    current.first_interface_subclass = 0;
    current.first_interface_protocol = 0;
    current.first_endpoint_address = 0;
    current.first_endpoint_attributes = 0;
    current.first_endpoint_max_packet = 0;
    current.first_endpoint_interval = 0;
    current.first_hid_descriptor_len = 0;
    current.first_hid_report_descriptor_len = 0;
    current.bulk_in_endpoint_address = 0;
    current.bulk_in_endpoint_max_packet = 0;
    current.bulk_in_endpoint_max_burst = 0;
    current.bulk_out_endpoint_address = 0;
    current.bulk_out_endpoint_max_packet = 0;
    current.bulk_out_endpoint_max_burst = 0;

    var offset: usize = CONFIG_DESCRIPTOR_HEADER_LEN;
    const limit: usize = total;
    var active: DescriptorInterface = .{};
    var selected: DescriptorInterface = .{};
    while (offset + 2 <= limit) {
        const len = d[offset];
        const dtype = d[offset + 1];
        if (len < 2 or offset + len > limit) {
            bumpDescriptorCounter(&current.descriptor_malformed);
            break;
        }
        bumpDescriptorCounter(&current.descriptor_records);
        if (dtype != USB_DESC_ENDPOINT and dtype != USB_DESC_SS_ENDPOINT_COMPANION) {
            active.last_endpoint_address = 0;
        }

        if (dtype == USB_DESC_INTERFACE) {
            if (len >= 9) {
                selectDescriptorCandidate(&active, &selected);
                recordDescriptorInterface(&active);
                active = .{
                    .valid = true,
                    .number = d[offset + 2],
                    .class_code = d[offset + 5],
                    .subclass = d[offset + 6],
                    .protocol = d[offset + 7],
                };
            } else {
                bumpDescriptorCounter(&current.descriptor_malformed);
            }
        } else if (dtype == USB_DESC_ENDPOINT) {
            if (len < 7) {
                bumpDescriptorCounter(&current.descriptor_malformed);
                offset += len;
                continue;
            }
            bumpDescriptorCounter(&current.endpoint_count);
            const address = d[offset + 2];
            const attributes = d[offset + 3];
            const max_packet = readLe16(d, offset + 4);
            recordEndpointForCandidate(&active, address, attributes, max_packet, d[offset + 6]);
        } else if (dtype == USB_DESC_HID) {
            bumpDescriptorCounter(&current.hid_descriptor_count);
            recordHidDescriptorForCandidate(&active, d + offset, len);
        } else if (dtype == USB_DESC_SS_ENDPOINT_COMPANION) {
            bumpDescriptorCounter(&current.ss_endpoint_companion_count);
            recordSsEndpointCompanionForCandidate(&active, d + offset, len);
        } else if (dtype != USB_DESC_CONFIGURATION and dtype != USB_DESC_STRING and dtype != USB_DESC_DEVICE) {
            bumpDescriptorCounter(&current.descriptor_unknown);
        }
        offset += len;
    }
    selectDescriptorCandidate(&active, &selected);
    recordDescriptorInterface(&active);
    publishSelectedDescriptorInterface(&selected);
}

fn recordDescriptorInterface(candidate: *const DescriptorInterface) void {
    if (!candidate.valid) return;
    if (current.interface_record_count >= MAX_USB_INTERFACES) {
        bumpDescriptorCounter(&current.descriptor_malformed);
        return;
    }
    const index: usize = @intCast(current.interface_record_count);
    current.interface_records[index] = .{
        .active = true,
        .number = candidate.number,
        .class_code = candidate.class_code,
        .subclass = candidate.subclass,
        .protocol = candidate.protocol,
        .endpoint_count = candidate.endpoint_count,
        .first_endpoint_address = candidate.first_endpoint_address,
        .first_endpoint_attributes = candidate.first_endpoint_attributes,
        .first_endpoint_max_packet = candidate.first_endpoint_max_packet,
        .first_endpoint_interval = candidate.first_endpoint_interval,
        .hid_descriptor_len = candidate.hid_descriptor_len,
        .hid_report_descriptor_len = candidate.hid_report_descriptor_len,
        .interrupt_in_endpoint_address = candidate.interrupt_in_endpoint_address,
        .interrupt_in_endpoint_max_packet = candidate.interrupt_in_endpoint_max_packet,
        .interrupt_in_endpoint_interval = candidate.interrupt_in_endpoint_interval,
        .bulk_in_endpoint_address = candidate.bulk_in_endpoint_address,
        .bulk_in_endpoint_max_burst = candidate.bulk_in_endpoint_max_burst,
        .bulk_out_endpoint_address = candidate.bulk_out_endpoint_address,
        .bulk_out_endpoint_max_burst = candidate.bulk_out_endpoint_max_burst,
    };
    current.interface_record_count += 1;
}

const XhciEvent = event_router.Event;

fn currentInterruptOwnerMatch() ?event_router.Match {
    if (!current.interrupt_pending or current.interrupt_pending_trb_phys == 0) return null;
    if (current.addressed_slot_id == 0 or current.interrupt_endpoint_id == 0) return null;
    return event_router.transferMatch(
        current.addressed_slot_id,
        current.interrupt_endpoint_id,
        current.interrupt_pending_trb_phys,
    );
}

fn runtimeInterruptOwnerMatch(rt: *const DeviceRuntime) ?event_router.Match {
    if (!rt.active or !rt.interrupt_pending or rt.interrupt_pending_trb_phys == 0) return null;
    if (rt.slot_id == 0 or rt.interrupt_endpoint_id == 0) return null;
    return event_router.transferMatch(rt.slot_id, rt.interrupt_endpoint_id, rt.interrupt_pending_trb_phys);
}

fn eventHasLiveOwner(event: XhciEvent) bool {
    if (active_sync_transfer) |owner| {
        if (event_router.matches(event, owner)) return true;
    }
    if (active_command) |owner| {
        if (event_router.matches(event, owner)) return true;
    }
    if (currentInterruptOwnerMatch()) |owner| {
        if (event_router.matches(event, owner)) return true;
    }
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (active_runtime_index != null and active_runtime_index.? == i) continue;
        if (runtimeInterruptOwnerMatch(&runtimes[i])) |owner| {
            if (event_router.matches(event, owner)) return true;
        }
    }
    return false;
}

fn clearOverflowedInterruptOwner(event: XhciEvent) bool {
    if (currentInterruptOwnerMatch()) |owner| {
        if (event_router.matches(event, owner)) {
            current.interrupt_pending = false;
            current.interrupt_pending_trb_phys = 0;
            current.interrupt_pending_streak = 0;
            return true;
        }
    }
    var i: usize = 0;
    while (i < runtimes.len) : (i += 1) {
        if (active_runtime_index != null and active_runtime_index.? == i) continue;
        if (runtimeInterruptOwnerMatch(&runtimes[i])) |owner| {
            if (!event_router.matches(event, owner)) continue;
            runtimes[i].interrupt_pending = false;
            runtimes[i].interrupt_pending_trb_phys = 0;
            runtimes[i].interrupt_pending_streak = 0;
            return true;
        }
    }
    return false;
}

fn routeForeignEvent(event: XhciEvent) void {
    if (eventHasLiveOwner(event)) {
        switch (deferred_events.enqueue(event)) {
            .queued => return,
            .overflow => {
                _ = clearOverflowedInterruptOwner(event);
            },
        }
    }
    recordStaleEvent(event);
    current.stale_events += 1;
}

fn pollMatchingEvent(expected: event_router.Match) ?XhciEvent {
    if (deferred_events.take(expected)) |event| return event;
    return drainEventBatch(expected);
}

// Ein Event-Batch wird bis zum Cycle-Mismatch geleert. Erst danach geben wir
// die konsumierten Slots gemeinsam per ERDP an den xHC zurueck. Das folgt der
// xHCI-Empfehlung fuer Event-Ring-Full-Fortschritt und vermeidet einen MMIO-
// Write pro Event. Das gesuchte Event wird lokal behalten, fremde Events
// werden waehrend desselben Batches weiter sauber geroutet.
fn drainEventBatch(expected: event_router.Match) ?XhciEvent {
    var matched: ?XhciEvent = null;
    var processed: u16 = 0;
    var guard: u32 = 0;
    while (guard < 1024) : (guard += 1) {
        const event = pollEvent() orelse break;
        processed +%= 1;
        if (matched == null and event_router.matches(event, expected)) {
            matched = event;
        } else {
            routeForeignEvent(event);
        }
    }
    commitEventBatch(processed);
    return matched;
}

fn waitMatchingEvent(expected: event_router.Match) ?XhciEvent {
    return waitMatchingEventWithin(expected, null);
}

fn waitMatchingEventWithin(
    expected: event_router.Match,
    recovery_budget: ?*const usb_wait.Deadline,
) ?XhciEvent {
    if (deferred_events.take(expected)) |event| return event;
    if (drainEventBatch(expected)) |event| return event;
    var wait = usb_wait.Wait.begin(
        usb_timing.XHCI_EVENT_TIMEOUT_MS,
        "xhci-event",
        0,
    );
    const deadline = &wait.deadline;
    const uses_tick_deadline = deadline.usesTickDeadline();
    const uses_hpet_deadline = deadline.usesHpetDeadline();
    const uses_tsc_deadline = deadline.usesTscDeadline();
    if (uses_tick_deadline) {
        current.event_tick_deadline_waits +%= 1;
    } else if (uses_hpet_deadline) {
        current.event_hpet_deadline_waits +%= 1;
    } else if (uses_tsc_deadline) {
        current.event_tsc_deadline_waits +%= 1;
    } else {
        current.event_guard_waits +%= 1;
    }
    var guard: u32 = 0;
    var timeout_clock: TimeoutClock = .cpu_guard;
    while (true) : (guard +%= 1) {
        if (recovery_budget) |budget| {
            if (budget.expiredAny()) {
                timeout_clock = .recovery_budget;
                break;
            }
        }
        if (deadline.expired()) {
            timeout_clock = .ticks;
            break;
        }
        if (deadline.hpetExpired()) {
            timeout_clock = .hpet;
            break;
        }
        if (deadline.tscExpired()) {
            timeout_clock = if (deadline.usesFallbackTsc()) .tsc_fallback else .tsc;
            break;
        }
        if (drainEventBatch(expected)) |event| {
            _ = wait.finish(true);
            return event;
        }
        if (deferred_events.take(expected)) |event| {
            _ = wait.finish(true);
            return event;
        }
        if (guard >= COMMAND_WAIT_GUARD) {
            timeout_clock = .cpu_guard;
            break;
        }
        wait.idle();
    }
    const elapsed_ticks = deadline.elapsedTicks();
    const elapsed_hpet = deadline.elapsedHpet();
    const elapsed_tsc = deadline.elapsedTsc();
    _ = wait.finish(false);
    if (timeout_clock == .ticks) {
        current.event_tick_timeouts +%= 1;
    } else if (timeout_clock == .hpet) {
        current.event_hpet_timeouts +%= 1;
    } else if (timeout_clock == .recovery_budget) {
        current.event_recovery_budget_timeouts +%= 1;
    } else if (timeout_clock == .tsc or timeout_clock == .tsc_fallback) {
        current.event_tsc_timeouts +%= 1;
    } else {
        current.event_guard_timeouts +%= 1;
        current.event_cpu_guard_timeouts +%= 1;
    }
    // This executes in the same non-preemptible kernel path that timed out,
    // so it remains visible even when the scheduler and desktop are wedged.
    // A generation-bound token lets the exact recovered transfer close this
    // incident without allowing a stale completion to clear newer evidence.
    const incident_token = diag_screen.beginResolvableIncident();
    switch (expected) {
        .transfer => {
            // Do not overwrite an older exact owner with an invalid token
            // merely because another reporter currently owns the screen.
            if (incident_token.valid()) {
                closeLastSyncTransferIncident();
                last_sync_transfer_incident = incident_token;
            }
        },
        .command => {},
    }
    diag_screen.write("[XHCI] event timeout owner=");
    switch (expected) {
        .transfer => |transfer| {
            diag_screen.write("transfer slot=");
            diag_screen.writeDec(transfer.slot_id);
            diag_screen.write(" ep=");
            diag_screen.writeDec(transfer.endpoint_id);
            diag_screen.write(" trb=");
            diag_screen.writeHex(transfer.trb_phys[0]);
        },
        .command => |command| {
            diag_screen.write("command trb=");
            diag_screen.writeHex(command.trb_phys);
        },
    }
    diag_screen.write(" clock=");
    diag_screen.write(switch (timeout_clock) {
        .ticks => "ticks",
        .hpet => "hpet",
        .tsc => "tsc",
        .tsc_fallback => "tsc-fallback",
        .recovery_budget => "recovery-budget",
        .cpu_guard => "cpu-guard",
    });
    diag_screen.write(" elapsed_ticks=");
    diag_screen.writeDec(elapsed_ticks);
    diag_screen.write(" elapsed_hpet=");
    diag_screen.writeDec(elapsed_hpet);
    diag_screen.write(" elapsed_tsc=");
    diag_screen.writeDec(elapsed_tsc);
    diag_screen.write(" guard=");
    diag_screen.writeDec(guard);
    diag_screen.endLine();
    persistXhciTimeout(expected, timeout_clock, elapsed_ticks, elapsed_hpet, elapsed_tsc, guard);
    diagXhciTimeoutState(expected);
    switch (expected) {
        .transfer => {},
        .command => {
            // Command submission has no higher protocol owner. Its timeout
            // and hardware snapshot are complete when this call returns.
            _ = diag_screen.resolveIncident(incident_token);
        },
    }
    return null;
}

fn persistentDiagPuts(text: []const u8) void {
    // boot_status keeps a non-visible output hook installed throughout the
    // runtime, so every log byte is already mirrored into bootlog. Writing
    // bootlog directly here duplicated and fragmented the same timeout line.
    k.puts(text);
}

fn persistentDiagDec(value: u64) void {
    k.putDec(value);
}

fn persistentDiagHex(value: u64, digits: u8) void {
    k.putHex(value, digits);
}

fn persistXhciTimeout(
    expected: event_router.Match,
    timeout_clock: TimeoutClock,
    elapsed_ticks: u64,
    elapsed_hpet: u64,
    elapsed_tsc: u64,
    guard: u32,
) void {
    persistentDiagPuts("[XHCI] event timeout owner=");
    switch (expected) {
        .transfer => |transfer| {
            persistentDiagPuts("transfer slot=");
            persistentDiagDec(transfer.slot_id);
            persistentDiagPuts(" ep=");
            persistentDiagDec(transfer.endpoint_id);
            persistentDiagPuts(" trb=0x");
            persistentDiagHex(transfer.trb_phys[0], 16);
        },
        .command => |command| {
            persistentDiagPuts("command trb=0x");
            persistentDiagHex(command.trb_phys, 16);
        },
    }
    persistentDiagPuts(" clock=");
    persistentDiagPuts(switch (timeout_clock) {
        .ticks => "ticks",
        .hpet => "hpet",
        .tsc => "tsc",
        .tsc_fallback => "tsc-fallback",
        .recovery_budget => "recovery-budget",
        .cpu_guard => "cpu-guard",
    });
    persistentDiagPuts(" elapsed_ticks=");
    persistentDiagDec(elapsed_ticks);
    persistentDiagPuts(" elapsed_hpet=");
    persistentDiagDec(elapsed_hpet);
    persistentDiagPuts(" elapsed_tsc=");
    persistentDiagDec(elapsed_tsc);
    persistentDiagPuts(" guard=");
    persistentDiagDec(guard);
    persistentDiagPuts("\r\n");

    persistentDiagPuts("[XHCI] hw usbcmd=0x");
    persistentDiagHex(readOp32(OP_USBCMD), 8);
    persistentDiagPuts(" usbsts=0x");
    persistentDiagHex(readOp32(OP_USBSTS), 8);
    persistentDiagPuts(" iman=0x");
    persistentDiagHex(readRt32(RT_IR0_IMAN), 8);
    persistentDiagPuts(" erdp=0x");
    persistentDiagHex(readRt64(RT_IR0_ERDP), 16);
    persistentDiagPuts("\r\n");
}

fn diagXhciTimeoutState(expected: event_router.Match) void {
    const next_event_control = if (event_ring_virt != 0)
        volatileRead32(event_ring_virt + @as(u64, current.event_dequeue) * @sizeOf(Trb) + 12)
    else
        0;
    diag_screen.write("[XHCI] hw usbcmd=");
    diag_screen.writeHex(readOp32(OP_USBCMD));
    diag_screen.write(" usbsts=");
    diag_screen.writeHex(readOp32(OP_USBSTS));
    diag_screen.write(" iman=");
    diag_screen.writeHex(readRt32(RT_IR0_IMAN));
    diag_screen.write(" erdp=");
    diag_screen.writeHex(readRt64(RT_IR0_ERDP));
    diag_screen.endLine();

    diag_screen.write("[XHCI] ring deq=");
    diag_screen.writeDec(current.event_dequeue);
    diag_screen.write(" cycle=");
    diag_screen.writeDec(current.event_cycle);
    diag_screen.write(" next=");
    diag_screen.writeHex(next_event_control);
    diag_screen.write(" events=");
    diag_screen.writeDec(current.events);
    diag_screen.write(" last=");
    diag_screen.writeDec(current.last_event_type);
    diag_screen.write("/");
    diag_screen.writeDec(current.last_event_code);
    diag_screen.endLine();

    switch (expected) {
        .transfer => |transfer| {
            diag_screen.write("[XHCI] target epstate=");
            if (endpointState(transfer.endpoint_id)) |state| {
                diag_screen.writeDec(@intFromEnum(state));
            } else {
                diag_screen.write("unknown");
            }
            if (current.addressed_port != 0) {
                const port_offset = OP_PORT_BASE +
                    @as(u64, current.addressed_port - 1) * OP_PORT_STRIDE;
                diag_screen.write(" portsc=");
                diag_screen.writeHex(readOp32(port_offset));
            }
            diag_screen.endLine();
        },
        .command => {},
    }
}

fn submitCommand(parameter: u64, status_value: u32, control_extra: u32) ?XhciEvent {
    return submitCommandWithin(parameter, status_value, control_extra, null);
}

fn submitCommandWithin(
    parameter: u64,
    status_value: u32,
    control_extra: u32,
    recovery_budget: ?*const usb_wait.Deadline,
) ?XhciEvent {
    if (!current.controller_running or command_ring_virt == 0) return null;
    if (active_command != null) return null;
    if (recovery_budget) |budget| {
        if (budget.expiredAny()) return null;
    }
    const enqueue = current.command_enqueue;
    if (enqueue >= COMMAND_TRB_COUNT - 1) {
        current.ring_full += 1;
        return null;
    }
    const ring: [*]Trb = @ptrFromInt(command_ring_virt);
    const command_trb_phys = current.command_ring_phys + @as(u64, enqueue) * @sizeOf(Trb);
    const cycle = if (current.command_cycle != 0) TRB_CYCLE else 0;
    ring[enqueue] = .{ .parameter = parameter, .status = status_value, .control = control_extra | cycle };
    dmaFence();
    current.last_command_type = @truncate((control_extra >> TRB_TYPE_SHIFT) & 0x3F);
    current.commands += 1;
    const advance = ring_cycle.afterSubmission(enqueue, current.command_cycle, COMMAND_TRB_COUNT);
    if (advance.wrapped) {
        updateLinkTrbCycle(command_ring_virt, COMMAND_TRB_COUNT, advance.link_cycle);
        current.command_ring_wraps += 1;
    }
    current.command_enqueue = advance.next_enqueue;
    current.command_cycle = advance.next_producer_cycle;
    const expected = event_router.commandMatch(command_trb_phys);
    active_command = expected;
    defer active_command = null;
    writeDoorbell(0, 0);
    return waitCommandCompletionWithin(expected, recovery_budget);
}

fn toggleCycle(cycle: *u8) void {
    cycle.* = if (cycle.* == 0) 1 else 0;
}

fn updateLinkTrbCycle(ring_virt: u64, count: usize, cycle: u8) void {
    if (ring_virt == 0 or count == 0) return;
    const ring: [*]Trb = @ptrFromInt(ring_virt);
    const idx = count - 1;
    // Nutz-TRBs muessen vollstaendig sichtbar sein, bevor das Cycle-Bit den
    // Link an den xHC uebergibt (xHCI Producer-Publish-Reihenfolge).
    dmaFence();
    if (cycle == 0) {
        ring[idx].control &= ~TRB_CYCLE;
    } else {
        ring[idx].control |= TRB_CYCLE;
    }
    dmaFence();
}

fn waitCommandCompletion(expected: event_router.Match) ?XhciEvent {
    return waitCommandCompletionWithin(expected, null);
}

fn waitCommandCompletionWithin(
    expected: event_router.Match,
    recovery_budget: ?*const usb_wait.Deadline,
) ?XhciEvent {
    if (waitMatchingEventWithin(expected, recovery_budget)) |event| return event;
    current.timeouts += 1;
    current.reason = "timeout waiting for xHCI command completion";
    return null;
}

fn pollEvent() ?XhciEvent {
    const event_addr = event_ring_virt + @as(u64, current.event_dequeue) * @sizeOf(Trb);
    const expected = if (current.event_cycle != 0) TRB_CYCLE else 0;

    // Der xHC publiziert ein Event ueber dessen Cycle-Bit. Dieses Bit muss
    // deshalb volatil zuerst gelesen werden; erst nach der DMA-Barriere sind
    // Parameter und Status ein gueltiger Snapshot. Ein zweiter Control-Read
    // schliesst aus, dass wir einen noch nicht vollstaendig sichtbaren Slot
    // auswerten.
    const published_control = volatileRead32(event_addr + 12);
    if ((published_control & TRB_CYCLE) != expected) return null;
    dmaFence();
    const parameter = volatileRead64(event_addr);
    const status_value = volatileRead32(event_addr + 8);
    const control = volatileRead32(event_addr + 12);
    if ((control & TRB_CYCLE) != expected) return null;

    current.events += 1;
    advanceEventRing();
    const event_type: u8 = @truncate((control >> TRB_TYPE_SHIFT) & 0x3F);
    const code: u8 = @truncate((status_value >> 24) & 0xFF);
    if (code == COMPLETION_EVENT_RING_FULL_ERROR) current.ring_full += 1;
    if (event_type == TRB_TYPE_TRANSFER_EVENT) current.transfer_events += 1;
    const out = XhciEvent{
        .event_type = event_type,
        .code = code,
        .slot_id = @truncate((control >> 24) & 0xFF),
        .endpoint_id = @truncate((control >> 16) & 0x1F),
        .parameter = parameter,
        .length = status_value & 0x00FF_FFFF,
        .control = control,
    };
    recordLastEvent(out);
    return out;
}

fn advanceEventRing() void {
    current.event_dequeue += 1;
    if (current.event_dequeue >= EVENT_TRB_COUNT) {
        current.event_dequeue = 0;
        current.event_cycle = if (current.event_cycle == 0) 1 else 0;
        current.event_ring_wraps += 1;
    }
}

fn commitEventBatch(processed: u16) void {
    if (processed == 0) return;
    if (processed > current.event_max_batch) current.event_max_batch = processed;

    // Polling-Betrieb ohne USBCMD.INTE: Status/IP trotzdem W1C quittieren,
    // damit EHB und die Interrupter-State-Machine auf realer Hardware nicht
    // ueber viele Ringumlaeufe in einem alten Pending-Zustand bleiben.
    dmaFence();
    if ((readOp32(OP_USBSTS) & USBSTS_EINT) != 0) {
        writeOp32(OP_USBSTS, USBSTS_EINT);
        _ = readOp32(OP_USBSTS);
    }
    const iman = readRt32(RT_IR0_IMAN);
    if ((iman & IMAN_IP) != 0) {
        writeRt32(RT_IR0_IMAN, iman | IMAN_IP);
        _ = readRt32(RT_IR0_IMAN);
    }
    const dequeue = current.event_ring_phys + @as(u64, current.event_dequeue) * @sizeOf(Trb);
    writeRt64(RT_IR0_ERDP, dequeue | ERDP_EHB);
    _ = readRt64(RT_IR0_ERDP);
    current.event_erdp_commits +%= 1;
}

fn recordLastEvent(event: XhciEvent) void {
    current.last_event_type = event.event_type;
    current.last_event_code = event.code;
    current.last_event_slot = event.slot_id;
    current.last_event_endpoint = event.endpoint_id;
    current.last_event_parameter = event.parameter;
    current.last_event_length = event.length;
}

fn recordStaleEvent(event: XhciEvent) void {
    current.last_stale_event_type = event.event_type;
    current.last_stale_event_code = event.code;
    current.last_stale_event_slot = event.slot_id;
    current.last_stale_event_endpoint = event.endpoint_id;
}

fn readOperational() void {
    current.usbcmd = readOp32(OP_USBCMD);
    current.usbsts = readOp32(OP_USBSTS);
    current.pagesize = readOp32(OP_PAGESIZE);
    current.dnctrl = readOp32(OP_DNCTRL);
    current.crcr = readOp64(OP_CRCR);
    current.dcbaap = readOp64(OP_DCBAAP);
    current.config = readOp32(OP_CONFIG);
}

fn readRuntime() void {
    if (current.rtsoff == 0) return;
    current.mfindex = readRt32(RT_MFINDEX);
    current.iman0 = readRt32(RT_IR0_IMAN);
    current.imod0 = readRt32(RT_IR0_IMOD);
    current.erstsz0 = readRt32(RT_IR0_ERSTSZ);
    current.erstba0 = readRt64(RT_IR0_ERSTBA);
    current.erdp0 = readRt64(RT_IR0_ERDP);
}

fn readPorts() void {
    const count = if (current.max_ports < MAX_PORTS) @as(usize, current.max_ports) else MAX_PORTS;
    current.port_count_seen = @intCast(count);
    current.connected_ports = 0;
    current.enabled_ports = 0;
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const base = OP_PORT_BASE + (@as(u64, i) * OP_PORT_STRIDE);
        const old = current.first_ports[i];
        const portsc = readOp32(base);
        const connected = (portsc & PORTSC_CCS) != 0;
        const enabled = (portsc & PORTSC_PED) != 0;
        const powered = (portsc & PORTSC_PP) != 0;
        if (connected) current.connected_ports += 1;
        if (enabled) current.enabled_ports += 1;
        current.first_ports[i] = .{
            .index = @intCast(i + 1),
            .portsc = portsc,
            .portpmsc = readOp32(base + 0x04),
            .portli = readOp32(base + 0x08),
            .porthlpmc = readOp32(base + 0x0C),
            .change_bits = portsc & PORTSC_CHANGE_MASK,
            .link_state = @truncate(portLinkState(portsc)),
            .connected = connected,
            .enabled = enabled,
            .powered = powered,
            .debounce_ok = old.debounce_ok,
            .reset_attempted = old.reset_attempted,
            .reset_ok = old.reset_ok,
            .reset_reason = old.reset_reason,
        };
    }
}

fn readOp32(offset: u64) u32 {
    return read32(@as(u64, current.caplength) + offset);
}

fn writeOp32(offset: u64, value: u32) void {
    write32(@as(u64, current.caplength) + offset, value);
}

fn readOp64(offset: u64) u64 {
    const low = readOp32(offset);
    const high = readOp32(offset + 4);
    return (@as(u64, high) << 32) | low;
}

fn writeOp64(offset: u64, value: u64) void {
    writeOp32(offset, @truncate(value));
    writeOp32(offset + 4, @truncate(value >> 32));
}

fn readRt32(offset: u64) u32 {
    return read32((current.rtsoff & 0xFFFF_FFE0) + offset);
}

fn writeRt32(offset: u64, value: u32) void {
    write32((current.rtsoff & 0xFFFF_FFE0) + offset, value);
}

fn readRt64(offset: u64) u64 {
    const low = readRt32(offset);
    const high = readRt32(offset + 4);
    return (@as(u64, high) << 32) | low;
}

fn writeRt64(offset: u64, value: u64) void {
    writeRt32(offset, @truncate(value));
    writeRt32(offset + 4, @truncate(value >> 32));
}

fn writeDoorbell(index_value: u8, target: u32) void {
    write32((current.dboff & 0xFFFF_FFFC) + @as(u64, index_value) * 4, target);
}

fn waitOpSet(offset: u64, mask: u32, timeout_ms: u32, reason: []const u8) bool {
    var wait = usb_wait.Wait.begin(timeout_ms, reason, 0);
    while (!wait.expired()) {
        if ((readOp32(offset) & mask) == mask) {
            _ = wait.finish(true);
            return true;
        }
        wait.idle();
    }
    _ = wait.finish(false);
    return false;
}

fn waitOpClear(offset: u64, mask: u32, timeout_ms: u32, reason: []const u8) bool {
    var wait = usb_wait.Wait.begin(timeout_ms, reason, 0);
    while (!wait.expired()) {
        if ((readOp32(offset) & mask) == 0) {
            _ = wait.finish(true);
            return true;
        }
        wait.idle();
    }
    _ = wait.finish(false);
    return false;
}

fn scratchpadCount() u8 {
    const low = (current.hcsparams2 >> 27) & 0x1F;
    const high = (current.hcsparams2 >> 21) & 0x1F;
    return @truncate((high << 5) | low);
}

fn maxPacketForSpeed(speed: u8) u16 {
    return switch (speed) {
        4 => 512, // SuperSpeed control endpoint 0.
        3 => 64, // High-speed control endpoint 0.
        2 => 8, // Low-speed control endpoint 0.
        1 => 64, // Full-speed devices may legally use up to 64 bytes.
        else => 8,
    };
}

fn readLe16(bytes: [*]const u8, offset: usize) u16 {
    return @as(u16, bytes[offset]) | (@as(u16, bytes[offset + 1]) << 8);
}

fn trbType(value: u32) u32 {
    return value << TRB_TYPE_SHIFT;
}

fn dmaFence() void {
    asm volatile ("mfence");
}

fn allocFrameZero() ?u64 {
    const frame = phys.allocFrame() orelse return null;
    zeroFrame(phys.physToVirt(frame));
    return frame;
}

fn zeroFrame(virt: u64) void {
    const bytes: [*]u8 = @ptrFromInt(virt);
    @memset(bytes[0..@intCast(phys.FRAME_SIZE)], 0);
}

fn mapMmio(base: u64, bytes: u64) bool {
    var mapped: u64 = 0;
    while (mapped < bytes) : (mapped += paging.PAGE_SIZE) {
        const phys_page = (base + mapped) & ~(paging.PAGE_SIZE - 1);
        const virt_page = phys.physToVirt(phys_page);
        if (!paging.isMapped(virt_page)) {
            if (!paging.mapPage(virt_page, phys_page, paging.WRITABLE | paging.CACHE_DISABLE | paging.NO_EXECUTE)) return false;
        }
    }
    return true;
}

fn read8(offset: u64) u8 {
    const ptr: *volatile u8 = @ptrFromInt(current.mmio_virt + offset);
    return ptr.*;
}

fn read16(offset: u64) u16 {
    const ptr: *volatile u16 = @ptrFromInt(current.mmio_virt + offset);
    return ptr.*;
}

fn read32(offset: u64) u32 {
    const ptr: *volatile u32 = @ptrFromInt(current.mmio_virt + offset);
    return ptr.*;
}

fn write32(offset: u64, value: u32) void {
    const ptr: *volatile u32 = @ptrFromInt(current.mmio_virt + offset);
    ptr.* = value;
}

fn volatileRead32(address: u64) u32 {
    const ptr: *const volatile u32 = @ptrFromInt(address);
    return ptr.*;
}

fn volatileRead64(address: u64) u64 {
    const ptr: *const volatile u64 = @ptrFromInt(address);
    return ptr.*;
}

fn logSummary() void {
    bootlog.puts("[XHCI] ");
    bootlog.putDec(current.device.bus);
    bootlog.puts(":");
    bootlog.putDec(current.device.device);
    bootlog.puts(".");
    bootlog.putDec(current.device.function);
    bootlog.puts(" mmio=0x");
    bootlog.putHex(current.mmio_phys, 16);
    bootlog.puts(" hci=0x");
    bootlog.putHex(current.hciversion, 4);
    bootlog.puts(" slots=");
    bootlog.putDec(current.max_slots);
    bootlog.puts(" ports=");
    bootlog.putDec(current.max_ports);
    bootlog.puts(" sts=0x");
    bootlog.putHex(current.usbsts, 8);
    bootlog.puts(" handoff=");
    bootlog.puts(current.bios_handoff);
    bootlog.puts("\r\n");
}
