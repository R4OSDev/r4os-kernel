const std = @import("std");
const io = @import("../arch/x86_64/io.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const audio = @import("../audio/core.zig");
const display_blit = @import("../display/blit_backend.zig");
const display = @import("../display/display.zig");
const gfx_buffers = @import("../memory/gfx_buffers.zig");
const gfx_api = @import("../program/gfx_buffer_api.zig");
const gfx_memory = @import("gfx_driver_memory.zig");
const bootlog = @import("bootlog.zig");
const boot_config = @import("boot_config.zig");
const net = @import("../net/core.zig");
const net_backend = @import("../net/backend_contract.zig");
const pci_inventory = @import("../platform/pci_inventory.zig");
const pci_interrupt_policy = @import("pci_interrupt_policy.zig");
const smp = @import("smp.zig");
const paging = @import("../memory/paging.zig");
const phys = @import("../memory/phys.zig");
const memory_layout = @import("../memory/layout.zig");
const dma_segments = @import("dma_segments.zig");
const dma_sync_range = @import("dma_sync_range.zig");
const protocol_api = @import("protocol_api.zig");
const r4p = @import("../program/r4p.zig");
const irq_router = @import("irq_router.zig");
const driver_work = @import("driver_work.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const storage = @import("../storage/block.zig");
const timer = @import("timer.zig");
const usb_host = @import("../driver/usb/host_controller.zig");
const xhci = @import("../driver/usb/xhci.zig");
const outputs_contract = @import("r4os_kernel_contract");
const driver_resources = @import("driver_resources.zig");
const driver_heap = @import("driver_heap.zig");
const driver_threads = @import("driver_threads.zig");
const driver_semaphores = @import("driver_semaphores.zig");

pub const MAGIC: u32 = 0x31495044; // "DPI1" little endian
// Version 35: owned Task-to-Work admission, unchanged v34 table layout.
pub const VERSION: u32 = 36;

const AUDIO_BACKEND_VERSION: u32 = 2;
const AUDIO_BACKEND_FORMAT_S16LE: u32 = 1 << 0;
const AUDIO_BACKEND_FORMAT_U8: u32 = 1 << 1;
const SYNTH_ENGINE_VERSION: u32 = 1;
const STORAGE_BACKEND_VERSION: u32 = 2;
const STORAGE_BACKEND_FLAG_REMOVABLE: u32 = 1 << 0;
const STORAGE_BACKEND_FLAG_WRITABLE: u32 = 1 << 1;
const STORAGE_SOURCE_BUILTIN: u32 = 0;
const STORAGE_SOURCE_PRELOAD: u32 = 1;
const STORAGE_SOURCE_DISK: u32 = 2;
const STORAGE_BUS_ATA: u32 = 1;
const STORAGE_BUS_AHCI: u32 = 2;
const STORAGE_BUS_NVME: u32 = 3;
const STORAGE_BUS_USB: u32 = 4;
const STORAGE_BUS_RAM: u32 = 5;
const STORAGE_BUS_VIRTIO: u32 = 6;
const USB_HOST_BACKEND_VERSION: u32 = usb_host.BACKEND_VERSION;
const USB_HOST_SOURCE_BUILTIN: u32 = 0;
const USB_HOST_SOURCE_PRELOAD: u32 = 1;
const USB_HOST_SOURCE_DISK: u32 = 2;
const NET_BACKEND_VERSION: u32 = net_backend.version;
const NET_BACKEND_FLAG_LINK_UP: u32 = 1 << 0;
const NET_BACKEND_FLAG_BROADCAST: u32 = 1 << 1;
const NET_BACKEND_FLAG_TRUSTED: u32 = 1 << 2;
const NET_BUS_PCI: u8 = 1;
const NET_BUS_PCIE: u8 = 2;
const NET_BUS_SERIAL: u8 = 3;
const MAX_R4D_NET_BACKENDS: usize = 8;
const MAX_R4D_NET_NAME: usize = 32;
const MAX_R4D_STORAGE_BACKENDS: usize = 8;
const MAX_R4D_STORAGE_NAME: usize = 32;
const STORAGE_CALLBACK_OWNER_CAPACITY: usize = 8;
const MAX_R4D_AUDIO_BACKENDS: usize = 8;
const MAX_R4D_AUDIO_NAME: usize = 32;
// Eight block backends can each expose the block core's full asynchronous
// depth. Segment mappings are therefore capacity-matched to 8 * 16 rather
// than retaining the old single-controller diagnostic limit of 32.
const MAX_R4D_DMA_ALLOCATIONS: usize = 128;
const MAX_R4D_DMA_PINS: usize = 128;
const MAX_R4D_DMA_MAPPINGS: usize = 128;
const MAX_R4D_DMA_MAP_BYTES: u32 = 16 * 1024 * 1024;
const MAX_MMIO_MAP_BYTES: u64 = 16 * 1024 * 1024;
const MMIO_MAP_WRITE_COMBINING: u32 = 1 << 0;

pub const NetBackendStatus = net.BackendStatus;
pub const NetBufferSegment = net_backend.BufferSegment;
pub const NetPacket = net_backend.Packet;
pub const NetTxComplete = net_backend.TxComplete;
pub const NetTxRequest = net_backend.TxRequest;
pub const NetTransmitPacketFn = net_backend.TransmitPacketFn;
pub const NetBackendNegotiation = net_backend.Negotiation;

pub const DmaBuffer = extern struct {
    phys_addr: u64 = 0,
    virt_addr: u64 = 0,
    bytes: u32 = 0,
    alignment: u32 = 0,
    flags: u32 = 0,
    reserved: u32 = 0,
};

pub const DMA_ABI_VERSION: u32 = 1;
pub const DMA_DIRECTION_BIDIRECTIONAL: u32 = 0;
pub const DMA_DIRECTION_TO_DEVICE: u32 = 1;
pub const DMA_DIRECTION_FROM_DEVICE: u32 = 2;
pub const DMA_FLAG_COHERENT: u16 = 1 << 0;
pub const DMA_FLAG_STREAMING: u16 = 1 << 1;
pub const DMA_FLAG_ALLOW_BOUNCE: u16 = 1 << 2;
pub const DMA_MAPPING_FLAG_BOUNCED: u32 = 1 << 16;

pub const DmaSegment = dma_segments.Segment;

pub const DmaPinnedBuffer = extern struct {
    version: u32 = DMA_ABI_VERSION,
    size: u32 = @sizeOf(DmaPinnedBuffer),
    handle: u64 = 0,
    virt_addr: u64 = 0,
    bytes: u32 = 0,
    page_count: u32 = 0,
    flags: u32 = 0,
    reserved: u32 = 0,
};

pub const DmaConstraints = extern struct {
    version: u32 = DMA_ABI_VERSION,
    size: u32 = @sizeOf(DmaConstraints),
    dma_mask: u64 = std.math.maxInt(u64),
    boundary: u64 = 0,
    max_segment_bytes: u32 = 0,
    alignment: u32 = 1,
    max_segments: u16 = dma_segments.max_segments,
    flags: u16 = DMA_FLAG_COHERENT | DMA_FLAG_ALLOW_BOUNCE,
    reserved: u32 = 0,
};

pub const DmaMapping = extern struct {
    version: u32 = DMA_ABI_VERSION,
    size: u32 = @sizeOf(DmaMapping),
    handle: u64 = 0,
    pin_handle: u64 = 0,
    requested_bytes: u32 = 0,
    mapped_bytes: u32 = 0,
    direction: u32 = DMA_DIRECTION_BIDIRECTIONAL,
    flags: u32 = 0,
    segment_count: u16 = 0,
    reserved0: u16 = 0,
    reserved1: u32 = 0,
    segments: [dma_segments.max_segments]DmaSegment = .{DmaSegment{}} ** dma_segments.max_segments,
};

pub const PciDeviceInfo = extern struct {
    bus_kind: u8 = 0,
    bus: u8 = 0,
    device: u8 = 0,
    function: u8 = 0,
    vendor_id: u16 = 0,
    device_id: u16 = 0,
    class_code: u8 = 0,
    subclass: u8 = 0,
    prog_if: u8 = 0,
    interrupt_line: u8 = 0xFF,
    interrupt_pin: u8 = 0,
    command: u16 = 0,
    reserved: u16 = 0,
};

pub const IrqHandler = irq_router.IrqHandler;
pub const IrqStats = irq_router.IrqStats;
pub const DriverWorkHandler = driver_work.WorkHandler;
pub const DriverWorkRequest = driver_work.WorkRequest;
pub const DriverCompletionStatus = driver_work.CompletionStatus;
pub const DriverWorkSummary = driver_work.Summary;

pub const MmioRegion = extern struct {
    phys_addr: u64 = 0,
    virt_addr: u64 = 0,
    bytes: u32 = 0,
    mapped_bytes: u32 = 0,
    bar_index: u8 = 0,
    flags: u8 = 0,
    reserved: u16 = 0,
};

pub const AudioBackendStatus = audio.BackendStatus;
pub const SynthEngineStatus = audio.SynthStatus;

pub const AudioBackendDescriptor = extern struct {
    version: u32,
    size: u32,
    flags: u32,
    formats: u32,
    min_rate: u32,
    max_rate: u32,
    preferred_rate: u32,
    max_channels: u16,
    reserved: u16,
    context: ?*anyopaque,
    write_pcm: ?audio.WritePcmCtxFn,
    stop: ?audio.StopPcmCtxFn,
    shutdown: ?*const fn (?*anyopaque) callconv(.c) i32,
    status: ?audio.StatusCtxFn,
};

const AudioOutputsDescriptor = extern struct {
    base: AudioBackendDescriptor,
    outputs: outputs_contract.AudioOutputExtension,
};

const AudioIdleDescriptor = extern struct {
    base: AudioOutputsDescriptor,
    idle: ?audio.StopPcmCtxFn,
};

comptime {
    if (@sizeOf(AudioBackendDescriptor) != 72 or @offsetOf(AudioOutputsDescriptor, "outputs") != 72 or @sizeOf(AudioOutputsDescriptor) != 96)
        @compileError("audio output backend prefix drift");
    if (@offsetOf(AudioIdleDescriptor, "idle") != 96 or @sizeOf(AudioIdleDescriptor) != 104)
        @compileError("audio idle backend prefix drift");
}

pub const SynthEngineDescriptor = extern struct {
    version: u32,
    size: u32,
    flags: u32,
    reserved: u32,
    context: ?*anyopaque,
    midi_send: ?audio.SynthMidiSendCtxFn,
    render: ?audio.SynthRenderCtxFn,
    stop: ?audio.SynthStopCtxFn,
    status: ?audio.SynthStatusCtxFn,
    opl3_reset: ?*const fn (?*anyopaque) callconv(.c) i32,
    opl3_write_register: ?*const fn (?*anyopaque, u8, u8, u8) callconv(.c) i32,
    sid_acquire: ?*const fn (?*anyopaque) callconv(.c) i32,
    sid_release: ?*const fn (?*anyopaque, u32) callconv(.c) i32,
    sid_set_model: ?*const fn (?*anyopaque, u32) callconv(.c) i32,
    sid_write_register: ?*const fn (?*anyopaque, u32, u32, u32) callconv(.c) i32,
    sid_load_data: ?*const fn (?*anyopaque, u32, u32, [*]const u8, u32) callconv(.c) i32,
    sid_init: ?*const fn (?*anyopaque, u32, u32, u32) callconv(.c) i32,
    sid_play_frame: ?*const fn (?*anyopaque, u32, u32, u32) callconv(.c) i32,
    sid_render_pcm: ?*const fn (?*anyopaque, u32, [*]u8, u32) callconv(.c) i32,
    render_pcm: ?audio.SynthRenderPcmCtxFn,
};

const SYNTH_ENGINE_V1_SIZE: u32 = @intCast(@offsetOf(SynthEngineDescriptor, "render_pcm"));

pub const StorageBackendStatus = extern struct {
    state: u32,
    last_error: u32,
    last_lba: u64,
    last_sectors: u32,
    recoveries: u64,
    recovery_failures: u64,
};

pub const StorageRequestComplete = storage.AsyncCompleteFn;

pub const StorageRequest = extern struct {
    version: u32 = 1,
    size: u32 = @sizeOf(StorageRequest),
    handle: u64 = 0,
    operation: u32 = 0,
    flags: u32 = 0,
    lba: u64 = 0,
    sectors: u32 = 0,
    buffer_bytes: u32 = 0,
    buffer_addr: u64 = 0,
    complete: StorageRequestComplete,
};

pub const StorageSubmitFn = *const fn (?*anyopaque, *const StorageRequest) callconv(.c) i32;
pub const StorageCancelFn = *const fn (?*anyopaque, u64, u32) callconv(.c) i32;
pub const StorageResetFn = *const fn (?*anyopaque, u32) callconv(.c) i32;

pub const StorageBackendDescriptor = extern struct {
    version: u32,
    size: u32,
    flags: u32,
    source: u32,
    bus: u32,
    controller: [32]u8,
    sector_size: u32,
    max_sectors_per_request: u16,
    queue_depth: u16,
    timeout_ticks: u64,
    sector_count: u64,
    context: ?*anyopaque,
    read: ?*const fn (?*anyopaque, u64, u32, [*]u8, u32) callconv(.c) i32,
    write: ?*const fn (?*anyopaque, u64, u32, [*]const u8, u32) callconv(.c) i32,
    flush: ?*const fn (?*anyopaque) callconv(.c) i32,
    shutdown: ?*const fn (?*anyopaque) callconv(.c) i32,
    status: ?*const fn (?*anyopaque, *StorageBackendStatus) callconv(.c) i32,
    // Version 2 append-only async contract. A missing submit callback keeps
    // the complete v1 prefix on the synchronous depth-one adapter.
    submit: ?StorageSubmitFn,
    cancel: ?StorageCancelFn,
    reset: ?StorageResetFn,
};

const STORAGE_BACKEND_V1_SIZE: u32 = @intCast(@offsetOf(StorageBackendDescriptor, "submit"));

pub const UsbHostStatus = usb_host.Status;
pub const UsbDeviceHandle = usb_host.DeviceHandle;
pub const UsbEndpointHandle = usb_host.EndpointHandle;
pub const UsbControlRequest = usb_host.ControlRequest;
pub const UsbHostControllerDescriptor = usb_host.Descriptor;

pub const NetBackendDescriptor = extern struct {
    version: u32,
    size: u32,
    flags: u32,
    mtu: u16,
    bus_kind: u8,
    reserved0: u8,
    bus: u8,
    device: u8,
    function: u8,
    reserved1: u8,
    vendor_id: u16,
    device_id: u16,
    mac: [6]u8,
    context: ?*anyopaque,
    transmit: ?*const fn (?*anyopaque, [*]const u8, u32) callconv(.c) i32,
    poll: ?*const fn (?*anyopaque) callconv(.c) void,
    shutdown: ?*const fn (?*anyopaque) callconv(.c) i32,
    status: ?*const fn (?*anyopaque, *NetBackendStatus) callconv(.c) i32,
    offered_capabilities: u64,
    required_capabilities: u64,
    rx_queue_count: u16,
    tx_queue_count: u16,
    max_rx_segments: u16,
    max_tx_segments: u16,
    rx_ownership: u32,
    tx_ownership: u32,
    interrupt_moderation_us: u32,
    reserved2: u32,
    transmit_packet: ?NetTransmitPacketFn,
};

const NET_BACKEND_V1_SIZE: u32 = @intCast(@offsetOf(NetBackendDescriptor, "offered_capabilities"));

const MsiOwnerCleanupResult = extern struct {
    removed: u32 = 0,
    failed: bool = false,
};

const StorageCallbackToken = extern struct { slot: usize, task_id: u32 };
pub const OwnerCleanupToken = extern struct {
    owner: u32 = 0,
    storage_plan: StorageCleanupPlan = .{},
    net_mutation_active: bool = false,
    display_blit_prepared: bool = false,
    audio_prepared: bool = false,
    shutdown_started: bool = false,
    active: bool = false,
};

const NetOwnerCleanupResult = extern struct {
    removed: u32 = 0,
    failed: bool = false,
};

pub fn enterOwner(owner: u32) bool {
    return r4os_rust_driver_api_enter(owner, sync.WAIT_FOREVER);
}

pub fn enterOwnerBounded(owner: u32, ticks: u64) bool {
    return r4os_rust_driver_api_enter(owner, ticks);
}

pub fn leaveOwner() bool {
    return r4os_rust_driver_api_leave();
}

pub fn bindModuleResources(owner: u32, slot: usize) bool {
    return r4os_rust_driver_api_bind(owner, slot);
}

pub fn prepareOwnerCleanup(owner: u32) ?OwnerCleanupToken {
    var token: OwnerCleanupToken = undefined;
    return if (r4os_rust_driver_cleanup_prepare(owner, &token)) token else null;
}

pub fn cancelOwnerCleanup(token: *OwnerCleanupToken) bool {
    return r4os_rust_driver_cleanup_cancel(token);
}

pub fn beginOwnerShutdown(token: *OwnerCleanupToken) void {
    r4os_rust_driver_cleanup_begin(token);
}

// Unlike unload, a terminal system transition must let DriverShutdown stop
// an active or partially initialized GPU while its display and DMA remain
// retained. This path never issues a generic memory/module cleanup permit.
pub fn beginDisplaySystemTransition(owner: u32) bool {
    return r4os_rust_driver_system_transition(owner, 0);
}

// Close cleanup-task admission even when Shutdown failed. This is terminal:
// a live callback still vetoes reset and no generic backing may be freed.
pub fn endDisplaySystemTransition(owner: u32) bool {
    return r4os_rust_driver_system_transition(owner, 1);
}

// Called only after the driver's explicit successful hardware shutdown.
// A remaining display, GPU job, mapping or dedicated callback vetoes warm
// reset. Shared Work is cancelled/drained before it can restart the device.
pub fn displaySystemTransitionQuiesced(owner: u32) bool {
    return r4os_rust_driver_system_transition(owner, 2);
}

pub fn quarantineOwnerCleanup(token: *OwnerCleanupToken) bool {
    return r4os_rust_driver_cleanup_quarantine(token);
}

pub fn commitOwnerCleanup(token: *OwnerCleanupToken) bool {
    return r4os_rust_driver_cleanup_commit(token);
}

pub fn cleanupOwner(owner: u32) bool {
    return r4os_rust_driver_cleanup(owner);
}

/// Snapshot the owner-provided status for a registered R4D block backend.
/// This is an internal diagnostic projection: it neither changes backend
/// admission nor exposes another public ABI surface.
pub fn queryStorageBackendStatus(index: usize, out: *StorageBackendStatus) bool {
    return r4os_rust_driver_storage_status(index, out);
}

pub const Table = extern struct {
    magic: u32,
    version: u32,
    size: u32,
    reserved: u32,
    log_info: *const fn ([*:0]const u8) callconv(.c) void,
    log_warn: *const fn ([*:0]const u8) callconv(.c) void,
    log_error: *const fn ([*:0]const u8) callconv(.c) void,
    port_inb: *const fn (u16) callconv(.c) u8,
    port_outb: *const fn (u16, u8) callconv(.c) void,
    alloc_dma_buffer: *const fn (u32, u32) callconv(.c) u64,
    free_dma_buffer: *const fn (u64, u32) callconv(.c) void,
    request_irq: *const fn (u8, *const anyopaque) callconv(.c) i32,
    release_irq: *const fn (u8) callconv(.c) i32,
    get_option: *const fn ([*:0]const u8, [*:0]const u8) callconv(.c) [*:0]const u8,
    register_audio_backend: *const fn ([*:0]const u8, *const anyopaque) callconv(.c) i32,
    register_storage_backend: *const fn ([*:0]const u8, *const anyopaque) callconv(.c) i32,
    register_input_backend: *const fn ([*:0]const u8, *const anyopaque) callconv(.c) i32,
    register_synth_engine: *const fn ([*:0]const u8, *const anyopaque) callconv(.c) i32,
    register_mixer_backend: *const fn ([*:0]const u8, *const anyopaque) callconv(.c) i32,
    register_net_backend: *const fn ([*:0]const u8, *const anyopaque) callconv(.c) i32,
    alloc_dma_region: *const fn (u32, u32, *DmaBuffer) callconv(.c) i32,
    free_dma_region: *const fn (*DmaBuffer) callconv(.c) void,
    pci_device_count: *const fn () callconv(.c) u32,
    pci_device_at: *const fn (u32, *PciDeviceInfo) callconv(.c) i32,
    pci_find_by_class: *const fn (u8, u8, u32, *PciDeviceInfo) callconv(.c) i32,
    pci_read_config32: *const fn (u8, u8, u8, u8, u16) callconv(.c) u32,
    pci_write_config32: *const fn (u8, u8, u8, u8, u16, u32) callconv(.c) i32,
    pci_read_bar: *const fn (u8, u8, u8, u8, u8) callconv(.c) u32,
    pci_enable_bus_master: *const fn (u8, u8, u8, u8, u32) callconv(.c) i32,
    irq_register: *const fn (u8, IrqHandler, usize, u32) callconv(.c) i32,
    irq_unregister: *const fn (u8, IrqHandler, usize) callconv(.c) i32,
    irq_stats: *const fn (u8, *IrqStats) callconv(.c) i32,
    pci_map_bar: *const fn (u8, u8, u8, u8, u8, u32, u32, *MmioRegion) callconv(.c) i32,
    port_inw: *const fn (u16) callconv(.c) u16,
    port_outw: *const fn (u16, u16) callconv(.c) void,
    port_inl: *const fn (u16) callconv(.c) u32,
    port_outl: *const fn (u16, u32) callconv(.c) void,
    net_receive_frame: *const fn (i32, [*]const u8, u32) callconv(.c) i32,
    register_audio_output_backend: *const fn ([*:0]const u8, *const anyopaque) callconv(.c) i32,
    unregister_audio_backend: *const fn ([*:0]const u8) callconv(.c) i32,
    tick_count: *const fn () callconv(.c) u64,
    timer_frequency: *const fn () callconv(.c) u32,
    wait_ticks: *const fn (u64) callconv(.c) void,
    register_synth_engine_v2: *const fn ([*:0]const u8, *const SynthEngineDescriptor) callconv(.c) i32,
    unregister_storage_backend: *const fn ([*:0]const u8) callconv(.c) i32,
    storage_backend_recovery_begin: *const fn ([*:0]const u8) callconv(.c) i32,
    storage_backend_recovery_finish: *const fn ([*:0]const u8, i32) callconv(.c) i32,
    register_usb_host_controller: *const fn ([*:0]const u8, *const UsbHostControllerDescriptor) callconv(.c) i32,
    unregister_usb_host_controller: *const fn ([*:0]const u8) callconv(.c) i32,
    driver_work_submit: *const fn (DriverWorkHandler, usize, u32, *u32) callconv(.c) i32,
    driver_work_cancel: *const fn (u32) callconv(.c) i32,
    driver_completion_wait: *const fn (u32, u64, *i32) callconv(.c) i32,
    driver_completion_status: *const fn (u32, *DriverCompletionStatus) callconv(.c) i32,
    driver_completion_release: *const fn (u32) callconv(.c) i32,
    driver_work_summary: *const fn (*DriverWorkSummary) callconv(.c) i32,
    // 0.59.19 (Version 16, append-only): MSI-Aktivierung fuer Geraete ohne
    // verlaessliches INTx-Routing. Rueckgabe ist die Router-IRQ (>= 0) aus
    // dem festen MSI-Fenster oder ein negativer Fehlercode.
    pci_enable_msi: *const fn (u8, u8, u8, u8) callconv(.c) i32,
    // 0.69.17 (Version 17, append-only): DMA fuer Geraete mit begrenzter
    // Adressbreite und explizites MSI-Rollback fuer Fehler-/Unloadpfade.
    alloc_dma_region_constrained: *const fn (u32, u32, u64, *DmaBuffer) callconv(.c) i32,
    pci_disable_msi: *const fn (u8, u8, u8, u8) callconv(.c) i32,
    // 0.69.28 (Version 18, append-only): synchroner R4P-Dispatch fuer
    // Treiber-Hotpaths, die den installierten Protokollvertrag benoetigen.
    protocol_dispatch: *const fn ([*:0]const u8, u32, *const protocol_api.ProtocolBuffer, *protocol_api.ProtocolBuffer) callconv(.c) i32,
    // 0.69.39 (Version 19, append-only): vorhandene residente Puffer werden
    // erst ownergebunden gepinnt, danach unter harten Hardwaregrenzen auf
    // eine feste Segmentliste oder einen kontrollierten Bouncepfad gemappt.
    dma_pin_buffer: *const fn (u64, u32, u32, *DmaPinnedBuffer) callconv(.c) i32,
    dma_map_pinned: *const fn (*const DmaPinnedBuffer, *const DmaConstraints, u32, *DmaMapping) callconv(.c) i32,
    dma_sync_for_device: *const fn (*const DmaMapping) callconv(.c) i32,
    dma_sync_for_cpu: *const fn (*const DmaMapping) callconv(.c) i32,
    dma_unmap: *const fn (*DmaMapping) callconv(.c) i32,
    dma_unpin_buffer: *const fn (*DmaPinnedBuffer) callconv(.c) i32,
    // 0.69.42 (Version 20, append-only): deadline-isolierte Audiorefills.
    driver_work_submit_request: *const fn (*const DriverWorkRequest, *u32) callconv(.c) i32,
    // 0.69.43 (Version 21, append-only): ein R4D aktiviert genau ein
    // kernelresidentes USB-Hostbackend und bleibt dessen Registry-Owner.
    activate_usb_host_controller: *const fn ([*:0]const u8, u32) callconv(.c) i32,
    // 0.69.48 (Version 22, append-only): genau ein externer synchroner
    // Display-Blitpfad; Ziel, Fallback und Fence bleiben Kernelbesitz.
    register_display_blit_backend: *const fn ([*:0]const u8, *const display_blit.Descriptor) callconv(.c) i32,
    unregister_display_blit_backend: *const fn ([*:0]const u8) callconv(.c) i32,
    // 0.69.49 (Version 23, append-only): IRQ-sicheres, adapterbezogenes
    // Wakeup fuer den begrenzten Netcore-RX-Handoff.
    net_schedule_rx: *const fn (i32) callconv(.c) i32,
    // 0.69.50 (Version 24, append-only): ausgehandelte Backendfaehigkeiten
    // und RX-Metadaten mit verpflichtendem kanonischem Flat-Fallback.
    net_backend_query: *const fn (i32, *NetBackendNegotiation) callconv(.c) i32,
    net_receive_packet: *const fn (i32, *const NetPacket) callconv(.c) i32,
    // v25: one optional versioned memory table; all previous offsets survive.
    gfx_memory_query: *const fn (*outputs_contract.GfxDriverMemoryApi) callconv(.c) i32,
    gfx_queue_query: *const fn (*outputs_contract.GfxDriverQueueApi) callconv(.c) i32,
    gfx_output_query: *const fn (*outputs_contract.GfxDriverOutputApi) callconv(.c) i32,
    gfx_display_query: *const fn (*outputs_contract.GfxDriverDisplayApi) callconv(.c) i32,
    resource_query: *const fn (*outputs_contract.DriverResourceApi) callconv(.c) i32,
    heap_query: *const fn (*outputs_contract.DriverHeapApi) callconv(.c) i32,
    monotonic_clock: *const fn (*outputs_contract.MonotonicClockInfo) callconv(.c) i32,
    thread_query: *const fn (*outputs_contract.DriverThreadApi) callconv(.c) i32,
    semaphore_query: *const fn (*outputs_contract.DriverSemaphoreApi) callconv(.c) i32,
    dma_sync_range_for_device: *const fn (*const DmaMapping, u32, u32) callconv(.c) i32,
    dma_sync_range_for_cpu: *const fn (*const DmaMapping, u32, u32) callconv(.c) i32,
    driver_work_submit_owned: *const fn (DriverWorkHandler, usize, *u32) callconv(.c) i32,
};

comptime {
    if (VERSION != outputs_contract.driver_api_version or @offsetOf(Table, "resource_query") != 592 or
        @offsetOf(Table, "heap_query") != 600 or @offsetOf(Table, "monotonic_clock") != 608 or
        @offsetOf(Table, "thread_query") != 616 or @offsetOf(Table, "semaphore_query") != 624 or
        @offsetOf(Table, "dma_sync_range_for_device") != 632 or @offsetOf(Table, "dma_sync_range_for_cpu") != 640 or
        @offsetOf(Table, "driver_work_submit_owned") != 648 or @sizeOf(Table) != 656)
        @compileError("DriverApi v36 appended owned Work layout drift");
}

const table_template = Table{
    .magic = MAGIC,
    .version = VERSION,
    .gfx_memory_query = gfxMemoryQuery,
    .gfx_queue_query = gfxQueueQuery,
    .gfx_output_query = gfxOutputQuery,
    .gfx_display_query = gfxDisplayQuery,
    .resource_query = resourceQuery,
    .heap_query = heapQuery,
    .monotonic_clock = @import("monotonic_api.zig").monotonicClock,
    .thread_query = threadQuery,
    .semaphore_query = semaphoreQuery,
    .dma_sync_range_for_device = dmaSyncRangeForDevice,
    .dma_sync_range_for_cpu = dmaSyncRangeForCpu,
    .driver_work_submit_owned = driverWorkSubmitOwned,
    .size = @sizeOf(Table),
    .reserved = 0,
    .log_info = logInfo,
    .log_warn = logWarn,
    .log_error = logError,
    .port_inb = portInb,
    .port_outb = portOutb,
    .alloc_dma_buffer = allocDmaBuffer,
    .free_dma_buffer = freeDmaBuffer,
    .request_irq = requestIrq,
    .release_irq = releaseIrq,
    .get_option = getOption,
    .register_audio_backend = registerAudioBackend,
    .register_storage_backend = registerStorageBackend,
    .register_input_backend = registerInputBackend,
    .register_synth_engine = registerSynthEngine,
    .register_mixer_backend = registerMixerBackend,
    .register_net_backend = registerNetBackend,
    .alloc_dma_region = allocDmaRegion,
    .free_dma_region = freeDmaRegion,
    .pci_device_count = pciDeviceCount,
    .pci_device_at = pciDeviceAt,
    .pci_find_by_class = pciFindByClass,
    .pci_read_config32 = pciReadConfig32,
    .pci_write_config32 = pciWriteConfig32,
    .pci_read_bar = pciReadBar,
    .pci_enable_bus_master = pciEnableBusMaster,
    .irq_register = irqRegister,
    .irq_unregister = irqUnregister,
    .irq_stats = irqStats,
    .pci_map_bar = pciMapBar,
    .port_inw = portInw,
    .port_outw = portOutw,
    .port_inl = portInl,
    .port_outl = portOutl,
    .net_receive_frame = netReceiveFrame,
    .register_audio_output_backend = registerAudioOutputBackend,
    .unregister_audio_backend = unregisterAudioBackend,
    .tick_count = tickCount,
    .timer_frequency = timerFrequency,
    .wait_ticks = waitTicks,
    .register_synth_engine_v2 = registerSynthEngineV2,
    .unregister_storage_backend = unregisterStorageBackend,
    .storage_backend_recovery_begin = storageBackendRecoveryBegin,
    .storage_backend_recovery_finish = storageBackendRecoveryFinish,
    .register_usb_host_controller = registerUsbHostController,
    .unregister_usb_host_controller = unregisterUsbHostController,
    .driver_work_submit = driverWorkSubmit,
    .driver_work_cancel = driverWorkCancel,
    .driver_completion_wait = driverCompletionWait,
    .driver_completion_status = driverCompletionStatus,
    .driver_completion_release = driverCompletionRelease,
    .driver_work_summary = driverWorkSummary,
    .pci_enable_msi = pciEnableMsi,
    .alloc_dma_region_constrained = allocDmaRegionConstrained,
    .pci_disable_msi = pciDisableMsi,
    .protocol_dispatch = protocolDispatch,
    .dma_pin_buffer = dmaPinBuffer,
    .dma_map_pinned = dmaMapPinned,
    .dma_sync_for_device = dmaSyncForDevice,
    .dma_sync_for_cpu = dmaSyncForCpu,
    .dma_unmap = dmaUnmap,
    .dma_unpin_buffer = dmaUnpinBuffer,
    .driver_work_submit_request = driverWorkSubmitRequest,
    .activate_usb_host_controller = activateUsbHostController,
    .register_display_blit_backend = registerDisplayBlitBackend,
    .unregister_display_blit_backend = unregisterDisplayBlitBackend,
    .net_schedule_rx = netScheduleRx,
    .net_backend_query = netBackendQuery,
    .net_receive_packet = netReceivePacket,
};

fn logInfo(text: [*:0]const u8) callconv(.c) void {
    r4os_rust_driver_log(0, text);
}

fn logWarn(text: [*:0]const u8) callconv(.c) void {
    r4os_rust_driver_log(1, text);
}

fn logError(text: [*:0]const u8) callconv(.c) void {
    r4os_rust_driver_log(2, text);
}

fn portInb(port: u16) callconv(.c) u8 {
    return r4os_rust_driver_port_inb(port);
}

fn portOutb(port: u16, value: u8) callconv(.c) void {
    r4os_rust_driver_port_outb(port, value);
}

fn portInw(port: u16) callconv(.c) u16 {
    return r4os_rust_driver_port_inw(port);
}

fn portOutw(port: u16, value: u16) callconv(.c) void {
    r4os_rust_driver_port_outw(port, value);
}

fn portInl(port: u16) callconv(.c) u32 {
    return r4os_rust_driver_port_inl(port);
}

fn portOutl(port: u16, value: u32) callconv(.c) void {
    r4os_rust_driver_port_outl(port, value);
}

fn tickCount() callconv(.c) u64 {
    return timer.tickCount();
}

fn timerFrequency() callconv(.c) u32 {
    return timer.frequency();
}

fn waitTicks(ticks: u64) callconv(.c) void {
    r4os_rust_driver_wait_ticks(ticks);
}

fn allocDmaBuffer(bytes: u32, alignment: u32) callconv(.c) u64 {
    return r4os_rust_driver_dma_allocate_legacy(bytes, alignment);
}
extern fn r4os_rust_driver_dma_allocate_legacy(bytes: u32, alignment: u32) u64;

fn freeDmaBuffer(physical: u64, bytes: u32) callconv(.c) void {
    _ = bytes;
    r4os_rust_driver_dma_free_physical(physical);
}

fn allocDmaRegion(bytes: u32, alignment: u32, out: *DmaBuffer) callconv(.c) i32 {
    return allocDmaRegionConstrained(bytes, alignment, std.math.maxInt(u64), out);
}

fn allocDmaRegionConstrained(bytes: u32, alignment: u32, maximum: u64, out: *DmaBuffer) callconv(.c) i32 {
    return r4os_rust_driver_dma_allocate(bytes, alignment, maximum, out);
}

fn freeDmaRegion(out: *DmaBuffer) callconv(.c) void {
    r4os_rust_driver_dma_free(out);
}

fn dmaPinBuffer(address: u64, bytes: u32, flags: u32, out: *DmaPinnedBuffer) callconv(.c) i32 {
    return r4os_rust_driver_dma_pin(address, bytes, flags, out);
}

fn dmaMapPinned(pin: *const DmaPinnedBuffer, c: *const DmaConstraints, direction: u32, out: *DmaMapping) callconv(.c) i32 {
    return r4os_rust_driver_dma_map(pin, c, direction, out);
}

fn dmaSyncForDevice(mapping: *const DmaMapping) callconv(.c) i32 {
    return r4os_rust_driver_dma_sync(mapping, false, false, 0, 0);
}

fn dmaSyncForCpu(mapping: *const DmaMapping) callconv(.c) i32 {
    return r4os_rust_driver_dma_sync(mapping, true, false, 0, 0);
}

fn dmaSyncRangeForDevice(mapping: *const DmaMapping, offset: u32, bytes: u32) callconv(.c) i32 {
    return dmaSyncRange(mapping, offset, bytes, .device);
}

fn dmaSyncRangeForCpu(mapping: *const DmaMapping, offset: u32, bytes: u32) callconv(.c) i32 {
    return dmaSyncRange(mapping, offset, bytes, .cpu);
}

fn dmaSyncRange(mapping: *const DmaMapping, offset: u32, bytes: u32, phase: dma_sync_range.Phase) i32 {
    return r4os_rust_driver_dma_sync(mapping, phase == .cpu, true, offset, bytes);
}

fn dmaUnmap(out: *DmaMapping) callconv(.c) i32 {
    return r4os_rust_driver_dma_unmap(out);
}

fn dmaUnpinBuffer(out: *DmaPinnedBuffer) callconv(.c) i32 {
    return r4os_rust_driver_dma_unpin(out);
}

fn requestIrq(irq: u8, handler: *const anyopaque) callconv(.c) i32 {
    _ = irq;
    _ = handler;
    return r4os_rust_driver_irq_legacy(false);
}

fn releaseIrq(irq: u8) callconv(.c) i32 {
    _ = irq;
    return r4os_rust_driver_irq_legacy(true);
}

fn irqRegister(irq: u8, handler: IrqHandler, context: usize, flags: u32) callconv(.c) i32 {
    return r4os_rust_driver_irq_register(irq, handler, context, flags);
}

fn irqUnregister(irq: u8, handler: IrqHandler, context: usize) callconv(.c) i32 {
    return irq_router.unregister(irq, handler, context);
}

fn irqStats(irq: u8, out: *IrqStats) callconv(.c) i32 {
    return irq_router.stats(irq, out);
}

fn driverWorkSubmit(handler: DriverWorkHandler, context: usize, flags: u32, out: *u32) callconv(.c) i32 {
    return r4os_rust_driver_api_work_submit(handler, context, flags, out);
}

fn driverWorkSubmitRequest(request: *const DriverWorkRequest, out: *u32) callconv(.c) i32 {
    return r4os_rust_driver_api_work_request(request, out);
}

// A dedicated CPU task may request a bounded hardware-owner slice on the
// existing BSP Work lane. It never borrows legacy PCI/MMIO/display admission
// on its own stack or keeps the lifecycle guard across its pacing waits.
fn driverWorkSubmitOwned(handler: DriverWorkHandler, context: usize, out: *u32) callconv(.c) i32 {
    return r4os_rust_driver_api_work_owned(handler, context, out);
}

pub fn invokeOwnedWork(owner: u32, handler: DriverWorkHandler, context: usize) i32 {
    return r4os_rust_driver_api_invoke_owned(owner, handler, context);
}

fn driverWorkCancel(handle: u32) callconv(.c) i32 {
    return driver_work.cancel(handle);
}

fn driverCompletionWait(handle: u32, timeout_ticks: u64, out_result: *i32) callconv(.c) i32 {
    return driver_work.completionWait(handle, timeout_ticks, out_result);
}

fn driverCompletionStatus(handle: u32, out: *DriverCompletionStatus) callconv(.c) i32 {
    return driver_work.completionStatus(handle, out);
}

fn driverCompletionRelease(handle: u32) callconv(.c) i32 {
    return driver_work.completionRelease(handle);
}

fn driverWorkSummary(out: *DriverWorkSummary) callconv(.c) i32 {
    out.* = driver_work.summary();
    return 0;
}

fn activeOwner() u32 {
    return r4os_rust_driver_api_runtime_owner(false);
}

// Dedicated tasks admit the audited CPU heap, clock, thread and semaphore
// services, plus the explicit normal-Work submission bridge above. Do not
// extend legacy PCI/DMA/backend entrypoints to parallel callers by changing
// activeOwner's admission contract.

fn pciDeviceCount() callconv(.c) u32 {
    return r4os_rust_driver_pci_count();
}

fn pciDeviceAt(index: u32, out: *PciDeviceInfo) callconv(.c) i32 {
    return r4os_rust_driver_pci_at(index, out);
}

fn pciFindByClass(class: u8, subclass: u8, start: u32, out: *PciDeviceInfo) callconv(.c) i32 {
    return r4os_rust_driver_pci_find(class, subclass, start, out);
}

// 0.59.19: MSI-Fenster. Router-IRQs 24..31 haben feste IDT-Vektoren 56..63,
// keine IOAPIC-Pins und werden ausschliesslich hier vergeben. GSI-basierte
// INTx-Registrierungen der Treiber nutzen 0..23 und kollidieren nicht.
const MSI_IRQ_BASE: u8 = 24;
const MSI_IRQ_COUNT: u8 = 8;
const IDT_IRQ_VECTOR_BASE: u32 = 32;

fn pciEnableMsi(kind: u8, bus: u8, device: u8, function: u8) callconv(.c) i32 {
    return r4os_rust_driver_msi_enable(kind, bus, device, function);
}

fn pciDisableMsi(kind: u8, bus: u8, device: u8, function: u8) callconv(.c) i32 {
    return r4os_rust_driver_msi_disable(kind, bus, device, function);
}

fn pciReadConfig32(kind: u8, bus: u8, device: u8, function: u8, offset: u16) callconv(.c) u32 {
    return r4os_rust_driver_pci_read(kind, bus, device, function, offset);
}

fn pciWriteConfig32(kind: u8, bus: u8, device: u8, function: u8, offset: u16, value: u32) callconv(.c) i32 {
    return r4os_rust_driver_pci_write(kind, bus, device, function, offset, value);
}

fn pciReadBar(kind: u8, bus: u8, device: u8, function: u8, index: u8) callconv(.c) u32 {
    return @intCast(r4os_rust_driver_pci_bar(kind, bus, device, function, index, false));
}

fn pciEnableBusMaster(kind: u8, bus: u8, device: u8, function: u8, flags: u32) callconv(.c) i32 {
    return r4os_rust_driver_pci_bus_master(kind, bus, device, function, flags);
}

fn pciMapBar(kind: u8, bus: u8, device: u8, function: u8, index: u8, bytes: u32, flags: u32, out: *MmioRegion) callconv(.c) i32 {
    return r4os_rust_driver_pci_map(kind, bus, device, function, index, bytes, flags, out);
}

fn getOption(driver: [*:0]const u8, key: [*:0]const u8) callconv(.c) [*:0]const u8 {
    return r4os_rust_driver_option(driver, key);
}

fn registerAudioBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    return audio.registerAudioBackendZ(name, backend);
}

fn registerAudioOutputBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    return r4os_rust_driver_audio_register(name, @ptrCast(@alignCast(backend)));
}

fn unregisterAudioBackend(name: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_driver_audio_unregister(name);
}

// Audio callbacks use the existing task-local device-callback owner ledger.
// Keeping that identity through a wait also makes DMA/work completion calls
// belong to the actual R4D instead of the caller's program or owner zero.

fn registerStorageBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    return r4os_rust_driver_storage_register(name, @ptrCast(@alignCast(backend)));
}

fn unregisterStorageBackend(name: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_driver_storage_unregister(name);
}

fn storageBackendRecoveryBegin(name: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_driver_storage_recovery(name, false, 0);
}

fn storageBackendRecoveryFinish(name: [*:0]const u8, ok: i32) callconv(.c) i32 {
    return r4os_rust_driver_storage_recovery(name, true, ok);
}

fn registerUsbHostController(name: [*:0]const u8, d: *const UsbHostControllerDescriptor) callconv(.c) i32 {
    return r4os_rust_driver_usb_register(name, d);
}

fn unregisterUsbHostController(name: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_driver_usb_unregister(name);
}

fn activateUsbHostController(name: [*:0]const u8, source: u32) callconv(.c) i32 {
    return r4os_rust_driver_usb_activate(name, source);
}

fn registerDisplayBlitBackend(name: [*:0]const u8, d: *const display_blit.Descriptor) callconv(.c) i32 {
    return r4os_rust_driver_blit(true, name, d);
}

fn unregisterDisplayBlitBackend(name: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_driver_blit(false, name, null);
}

fn registerInputBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    _ = backend;
    return r4os_rust_driver_input(name);
}

fn registerSynthEngine(name: [*:0]const u8, engine: *const anyopaque) callconv(.c) i32 {
    return audio.registerSynthEngineZ(name, engine);
}

fn registerSynthEngineV2(name: [*:0]const u8, d: *const SynthEngineDescriptor) callconv(.c) i32 {
    return r4os_rust_driver_synth(name, d);
}

fn protocolDispatch(role: [*:0]const u8, op: u32, in_buffer: *const protocol_api.ProtocolBuffer, out_buffer: *protocol_api.ProtocolBuffer) callconv(.c) i32 {
    return r4p.dispatch(zSlice(role), op, in_buffer, out_buffer);
}

fn registerMixerBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    return audio.registerMixerBackendZ(name, backend);
}

fn registerNetBackend(name: [*:0]const u8, descriptor: *const anyopaque) callconv(.c) i32 {
    return r4os_rust_driver_net_register(name, @ptrCast(@alignCast(descriptor)));
}

fn synthRenderPcm(d: *const SynthEngineDescriptor) ?audio.SynthRenderPcmCtxFn {
    return r4os_rust_driver_synth_pcm(d);
}

fn storageBackendSubmit(d: *const StorageBackendDescriptor) ?StorageSubmitFn {
    return r4os_rust_driver_storage_submit(d);
}

fn storageBackendCancel(d: *const StorageBackendDescriptor) ?StorageCancelFn {
    return r4os_rust_driver_storage_cancel(d);
}

fn storageBackendReset(d: *const StorageBackendDescriptor) ?StorageResetFn {
    return r4os_rust_driver_storage_reset(d);
}

fn r4dNetTransmit(index: usize, frame: []const u8) net.TxResult {
    return @enumFromInt(r4os_rust_driver_net_transmit(index, frame.ptr, @intCast(frame.len)));
}

fn r4dNetPoll(index: usize) void {
    r4os_rust_driver_net_poll(index);
}

fn r4dNetStatus(index: usize, out: *NetBackendStatus) i32 {
    return r4os_rust_driver_net_status(index, out);
}

fn netReceiveFrame(id: i32, frame: [*]const u8, len: u32) callconv(.c) i32 {
    return r4os_rust_driver_net_receive(id, frame, len);
}

fn netScheduleRx(id: i32) callconv(.c) i32 {
    return r4os_rust_driver_net_schedule(id);
}

fn netBackendQuery(id: i32, out: *NetBackendNegotiation) callconv(.c) i32 {
    return r4os_rust_driver_net_query(id, out);
}

fn netReceivePacket(id: i32, packet: *const NetPacket) callconv(.c) i32 {
    return r4os_rust_driver_net_receive_packet(id, packet);
}

fn r4dStorageRead(ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool {
    return r4os_rust_driver_storage_io(ctx, false, lba, sectors, out.ptr, @intCast(out.len));
}

fn r4dStorageWrite(ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool {
    return r4os_rust_driver_storage_io(ctx, true, lba, sectors, @constCast(data.ptr), @intCast(data.len));
}

fn r4dStorageFlush(ctx: ?*anyopaque) bool {
    return r4os_rust_driver_storage_flush(ctx);
}

fn r4dStorageSubmit(ctx: ?*anyopaque, request: *const storage.AsyncRequest) i32 {
    const buffer_addr: u64 = switch (request.kind) {
        .read => if (request.buffer) |p| @intFromPtr(p) else 0,
        .write => if (request.const_buffer) |p| @intFromPtr(p) else 0,
        .flush, .none => 0,
    };
    const public_request = StorageRequest{ .handle = request.handle, .operation = @intFromEnum(request.kind), .lba = request.lba, .sectors = request.sectors, .buffer_bytes = @intCast(request.buffer_len), .buffer_addr = buffer_addr, .complete = request.complete };
    return r4os_rust_driver_storage_async(ctx, &public_request);
}

fn r4dStorageCancel(ctx: ?*anyopaque, handle: u64, reason: u32) i32 {
    return r4os_rust_driver_storage_control(ctx, false, handle, reason);
}

fn r4dStorageReset(ctx: ?*anyopaque, reason: u32) i32 {
    return r4os_rust_driver_storage_control(ctx, true, 0, reason);
}

const StorageCleanupResult = extern struct {
    removed: u32 = 0,
    remaining: u32 = 0,
};

const StorageCleanupEntry = extern struct {
    backend_index: usize = 0,
    token: NativeStorageToken = .{},
};

const StorageCleanupPlan = extern struct {
    entries: [MAX_R4D_STORAGE_BACKENDS]StorageCleanupEntry = .{StorageCleanupEntry{}} ** MAX_R4D_STORAGE_BACKENDS,
    count: usize = 0,
};

const AudioOwnerCleanupResult = extern struct { removed: u32 = 0, failed: bool = false };

fn netBus(kind: u8) net.Bus {
    return switch (kind) {
        NET_BUS_PCI => .pci,
        NET_BUS_PCIE => .pcie,
        NET_BUS_SERIAL => .serial,
        else => .unknown,
    };
}

fn storageBus(kind: u32) storage.Bus {
    return switch (kind) {
        STORAGE_BUS_ATA => .ata,
        STORAGE_BUS_AHCI => .ahci,
        STORAGE_BUS_NVME => .nvme,
        STORAGE_BUS_USB => .usb,
        STORAGE_BUS_RAM => .ram,
        STORAGE_BUS_VIRTIO => .virtio,
        else => .unknown,
    };
}

fn storageSource(source: u32) storage.Source {
    return switch (source) {
        STORAGE_SOURCE_PRELOAD => .preload,
        STORAGE_SOURCE_DISK => .disk,
        else => .builtin,
    };
}

fn storageController(descriptor: *const StorageBackendDescriptor) []const u8 {
    var len: usize = 0;
    while (len < descriptor.controller.len and descriptor.controller[len] != 0) : (len += 1) {}
    if (len == 0) return "R4D";
    return descriptor.controller[0..len];
}

fn zSlice(src: [*:0]const u8) []const u8 {
    var len: usize = 0;
    while (len < 512 and src[len] != 0) : (len += 1) {}
    return src[0..len];
}

// Common BOs support normal Work. The real driver-start binding is installed
// before Init, closed under the same BO metadata owner and retired only after
// work and hardware leases quiesce. A cached table conveys no caller identity.
fn currentBufferOwner(admission: bool) gfx_buffers.Error!gfx_buffers.Owner {
    return nativeGfxOwner(0, admission);
}

// Receiver metadata supports authenticated Init/Work/Shutdown without the
// wait-spanning lifecycle guard. Read the already-bound start epoch through
// the resident runtime owner; do not widen PCI/DMA/display admission or use
// any caller-supplied driver identity.

const gfx_queue_api = @import("gfx_driver_queue.zig");

fn semaphoreQuery(out: *outputs_contract.DriverSemaphoreApi) callconv(.c) i32 {
    return r4os_rust_driver_semaphore_api(out);
}

fn threadQuery(out: *outputs_contract.DriverThreadApi) callconv(.c) i32 {
    return r4os_rust_driver_thread_api(out);
}

fn heapQuery(out: *outputs_contract.DriverHeapApi) callconv(.c) i32 {
    return r4os_rust_driver_heap_api(out);
}

// Work callbacks already carry an authenticated owner, but must join the
// lifecycle guard before resource I/O. Never wait here: another owner may
// itself be waiting for this worker. The caller can reschedule on BUSY.

fn resourceQuery(out: *outputs_contract.DriverResourceApi) callconv(.c) i32 {
    return r4os_rust_driver_resource_api(out);
}

const native_display = @import("../display/native_driver.zig");
fn gfxDisplayQuery(output: *outputs_contract.GfxDriverDisplayApi) callconv(.c) i32 {
    return r4os_rust_driver_gfx_display_query(output);
}
extern fn r4os_rust_driver_gfx_display_query(output: *outputs_contract.GfxDriverDisplayApi) i32;

extern fn r4os_rust_driver_gfx_boot_hold(input: *const outputs_contract.GfxBootHoldRequest, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_presentation_stats(input: *const outputs_contract.DisplayPresentationStats) i32;

extern fn r4os_rust_driver_gfx_display_presentation_info(input: *const outputs_contract.DisplayPresentationInfo) i32;

extern fn r4os_rust_driver_gfx_display_output_register(input: *const outputs_contract.GfxAdditionalOutput, output: *outputs_contract.GfxOutputTarget) i32;

extern fn r4os_rust_driver_gfx_display_output_transition(input: *const outputs_contract.GfxOutputTarget, operation: u32, quiesced: u32) i32;

extern fn r4os_rust_driver_gfx_cursor_configure(input: *const outputs_contract.DisplayCursorInfo) i32;

extern fn r4os_rust_driver_gfx_cursor_take(backend: *const outputs_contract.GfxBackendBinding, output: *outputs_contract.GfxDriverCursorJob) i32;

extern fn r4os_rust_driver_gfx_cursor_complete(input: *const outputs_contract.GfxDriverCursorCompletion) i32;

extern fn r4os_rust_driver_gfx_boot_finish(generation: u64, operation: u32, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_boot_info(output: *outputs_contract.GfxNativeBootInfo) i32;

extern fn r4os_rust_driver_gfx_display_prepare(input: *const outputs_contract.GfxNativeRegistration, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_prepare_held(input: *const outputs_contract.GfxNativeRegistration, generation: u64, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_transition(generation: u64, operation: u32, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_device_reset(input: *const outputs_contract.GfxBackendBinding, generation: u64, quiesced: u32, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_resume_headless(input: *const outputs_contract.GfxBackendBinding, generation: u64, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_terminal_release(input: *const outputs_contract.GfxBackendBinding, generation: u64, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_prepare_reset(input: *const outputs_contract.GfxNativeRegistration, held_generation: u64, reset_generation: u64, output: *outputs_contract.GfxNativeState) i32;

extern fn r4os_rust_driver_gfx_display_schedule(input: *const outputs_contract.GfxBackendBinding) i32;
fn gfxOutputQuery(output: *outputs_contract.GfxDriverOutputApi) callconv(.c) i32 {
    return r4os_rust_driver_gfx_output_query(output);
}
extern fn r4os_rust_driver_gfx_output_query(output: *outputs_contract.GfxDriverOutputApi) i32;
fn gfxOutputPowerPublish(input: *const outputs_contract.GfxOutputPower) callconv(.c) i32 {
    return r4os_rust_driver_gfx_output_power_publish(input);
}
extern fn r4os_rust_driver_gfx_output_power_publish(input: *const outputs_contract.GfxOutputPower) i32;
fn gfxPowerRead(input: *const outputs_contract.GfxOutputId, output: *outputs_contract.GfxPowerRequest) callconv(.c) i32 {
    return r4os_rust_driver_gfx_power_read(input, output);
}
extern fn r4os_rust_driver_gfx_power_read(input: *const outputs_contract.GfxOutputId, output: *outputs_contract.GfxPowerRequest) i32;
fn gfxOutputRefreshPublish(input: *const outputs_contract.GfxOutputRefresh) callconv(.c) i32 {
    return r4os_rust_driver_gfx_output_refresh_publish(input);
}
extern fn r4os_rust_driver_gfx_output_refresh_publish(input: *const outputs_contract.GfxOutputRefresh) i32;
fn gfxRefreshRead(input: *const outputs_contract.GfxOutputTarget, output: *outputs_contract.GfxRefreshRequest) callconv(.c) i32 {
    return r4os_rust_driver_gfx_refresh_read(input, output);
}
extern fn r4os_rust_driver_gfx_refresh_read(input: *const outputs_contract.GfxOutputTarget, output: *outputs_contract.GfxRefreshRequest) i32;
fn gfxModeReadColor(ticket: u64, sequence: u64, output: *outputs_contract.GfxDriverModeColor) callconv(.c) i32 {
    return r4os_rust_driver_gfx_mode_read_color(ticket, sequence, output);
}
extern fn r4os_rust_driver_gfx_mode_read_color(ticket: u64, sequence: u64, output: *outputs_contract.GfxDriverModeColor) i32;
fn gfxOutputColorPublish(input: *const outputs_contract.GfxOutputColorState) callconv(.c) i32 {
    return r4os_rust_driver_gfx_output_color_publish(input);
}
extern fn r4os_rust_driver_gfx_output_color_publish(input: *const outputs_contract.GfxOutputColorState) i32;
fn gfxOutputPause(input: *const outputs_contract.GfxOutputId, paused: u32) callconv(.c) i32 {
    return r4os_rust_driver_gfx_output_pause(input, paused);
}
extern fn r4os_rust_driver_gfx_output_pause(input: *const outputs_contract.GfxOutputId, paused: u32) i32;
fn gfxModeRestore(input: *const outputs_contract.GfxAtomicState, output: *outputs_contract.GfxModeStatus) callconv(.c) i32 {
    return r4os_rust_driver_gfx_mode_restore(input, output);
}
extern fn r4os_rust_driver_gfx_mode_restore(input: *const outputs_contract.GfxAtomicState, output: *outputs_contract.GfxModeStatus) i32;
fn gfxModeStatus(ticket: u64, output: *outputs_contract.GfxModeStatus) callconv(.c) i32 {
    return r4os_rust_driver_gfx_mode_status(ticket, output);
}
extern fn r4os_rust_driver_gfx_mode_status(ticket: u64, output: *outputs_contract.GfxModeStatus) i32;
fn gfxAudioPublish(input: *const outputs_contract.GfxAudioRoute) callconv(.c) i32 {
    return r4os_rust_driver_gfx_audio_publish(input);
}
extern fn r4os_rust_driver_gfx_audio_publish(input: *const outputs_contract.GfxAudioRoute) i32;
fn gfxAudioQuery(location: u32, device: u32, index: u32, output: *outputs_contract.GfxAudioRoute) callconv(.c) i32 {
    return r4os_rust_driver_gfx_audio_query(location, device, index, output);
}
extern fn r4os_rust_driver_gfx_audio_query(location: u32, device: u32, index: u32, output: *outputs_contract.GfxAudioRoute) i32;
const gfx_modes = @import("../display/mode_work.zig");
fn gfxModeEnable(input: *const outputs_contract.GfxBackendBinding) callconv(.c) i32 {
    return r4os_rust_driver_gfx_mode_enable(input);
}
extern fn r4os_rust_driver_gfx_mode_enable(input: *const outputs_contract.GfxBackendBinding) i32;
fn gfxModeTake(input: *const outputs_contract.GfxBackendBinding, output: *outputs_contract.GfxDriverModeJob) callconv(.c) i32 {
    return r4os_rust_driver_gfx_mode_take(input, output);
}
extern fn r4os_rust_driver_gfx_mode_take(input: *const outputs_contract.GfxBackendBinding, output: *outputs_contract.GfxDriverModeJob) i32;
fn gfxModeComplete(input: *const outputs_contract.GfxDriverModeCompletion) callconv(.c) i32 {
    return r4os_rust_driver_gfx_mode_complete(input);
}
extern fn r4os_rust_driver_gfx_mode_complete(input: *const outputs_contract.GfxDriverModeCompletion) i32;
fn gfxReceiverRegister(adapter: u32, output: *outputs_contract.GfxReceiverSource) callconv(.c) i32 {
    return r4os_rust_driver_gfx_receiver_register(adapter, output);
}
extern fn r4os_rust_driver_gfx_receiver_register(adapter: u32, output: *outputs_contract.GfxReceiverSource) i32;
fn gfxReceiverReplace(input: *const outputs_contract.GfxReceiverUpdate) callconv(.c) i32 {
    return r4os_rust_driver_gfx_receiver_replace(input);
}
extern fn r4os_rust_driver_gfx_receiver_replace(input: *const outputs_contract.GfxReceiverUpdate) i32;
fn gfxReceiverClose(input: *const outputs_contract.GfxReceiverSource) callconv(.c) i32 {
    return r4os_rust_driver_gfx_receiver_close(input);
}
extern fn r4os_rust_driver_gfx_receiver_close(input: *const outputs_contract.GfxReceiverSource) i32;
fn gfxOutputPublish(input: *const outputs_contract.GfxOutputPublication, output: *outputs_contract.GfxOutputId) callconv(.c) i32 {
    return r4os_rust_driver_gfx_output_publish(input, output);
}
extern fn r4os_rust_driver_gfx_output_publish(input: *const outputs_contract.GfxOutputPublication, output: *outputs_contract.GfxOutputId) i32;
fn gfxOutputWithdraw(input: *const outputs_contract.GfxOutputId) callconv(.c) i32 {
    return r4os_rust_driver_gfx_output_withdraw(input);
}
extern fn r4os_rust_driver_gfx_output_withdraw(input: *const outputs_contract.GfxOutputId) i32;
fn gfxQueueQuery(output: *outputs_contract.GfxDriverQueueApi) callconv(.c) i32 {
    return r4os_rust_driver_gfx_queue_query(output);
}
extern fn r4os_rust_driver_gfx_queue_query(output: *outputs_contract.GfxDriverQueueApi) i32;

extern fn r4os_rust_driver_gfx_queue_register(input: *const outputs_contract.GfxBackendRegistration, output: *outputs_contract.GfxBackendBinding) i32;

extern fn r4os_rust_driver_gfx_queue_register_profile(input: *const outputs_contract.GfxBackendRegistration, profile: *const outputs_contract.GfxBackendProfile, output: *outputs_contract.GfxBackendBinding) i32;

extern fn r4os_rust_driver_gfx_queue_publish_properties(input: *const outputs_contract.GfxBackendBinding, properties: *const outputs_contract.GfxBackendProperties) i32;

extern fn r4os_rust_driver_gfx_queue_read_native_info(input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxNativeJobInfo) i32;

extern fn r4os_rust_driver_gfx_queue_read_native_data(input: *const outputs_contract.GfxFence, offset: u32, output: [*]u8, count: u32) i32;

extern fn r4os_rust_driver_gfx_queue_read_native_binding(input: *const outputs_contract.GfxFence, index: u32, output: *outputs_contract.GfxNativeBinding) i32;

extern fn r4os_rust_driver_gfx_queue_owner_info(input: *const outputs_contract.GfxBackendBinding, timeline: u64, output: *outputs_contract.GfxQueueOwnerInfo) i32;

extern fn r4os_rust_driver_gfx_queue_unregister(input: *const outputs_contract.GfxBackendBinding, quiesced: u32) i32;

extern fn r4os_rust_driver_gfx_queue_take(input: *const outputs_contract.GfxBackendBinding, output: *outputs_contract.GfxDriverJob) i32;

extern fn r4os_rust_driver_gfx_queue_read_render_list(input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxRenderList) i32;

extern fn r4os_rust_driver_gfx_queue_read_render_grid_list(input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxRenderGridList) i32;

extern fn r4os_rust_driver_gfx_queue_update_operations(input: *const outputs_contract.GfxBackendBinding, operations: u64) i32;

extern fn r4os_rust_driver_gfx_queue_read_render_color_list(input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxRenderColorList) i32;

extern fn r4os_rust_driver_gfx_queue_complete(input: *const outputs_contract.GfxFence, result: u32, quiesced: u32) i32;

extern fn r4os_rust_driver_gfx_queue_reset(input: *const outputs_contract.GfxBackendBinding, quiesced: u32, output: *outputs_contract.GfxBackendBinding) i32;

extern fn r4os_rust_driver_gfx_queue_segment(input: *const outputs_contract.GfxFence, which: u32, offset: u64, mask: u64, output: *outputs_contract.GfxDmaSegment) i32;

extern fn r4os_rust_driver_gfx_queue_retain(input: *const outputs_contract.GfxFence, which: u32, output: *outputs_contract.GfxBufferReference) i32;

extern fn r4os_rust_driver_gfx_queue_retain_scanout(input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxBufferReference) i32;

extern fn r4os_rust_driver_gfx_queue_begin_scanout(input: *const outputs_contract.GfxFence) i32;

extern fn r4os_rust_driver_gfx_queue_scanout_retire_requested(input: *const outputs_contract.GfxFence) i32;

extern fn r4os_rust_driver_gfx_buffer_create(input: *const outputs_contract.GfxBufferDescriptor, output: *outputs_contract.GfxBufferReference) i32;

extern fn r4os_rust_driver_gfx_buffer_describe(input: *const outputs_contract.GfxBufferHandle, output: *outputs_contract.GfxBufferDescriptor) i32;

extern fn r4os_rust_driver_gfx_buffer_import(input: *const outputs_contract.GfxBufferHandle, output: *outputs_contract.GfxBufferReference) i32;

extern fn r4os_rust_driver_gfx_buffer_release(input: *const outputs_contract.GfxBufferHandle) i32;

extern fn r4os_rust_driver_gfx_buffer_map(input: *const outputs_contract.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *outputs_contract.GfxBufferMap) i32;

extern fn r4os_rust_driver_gfx_buffer_unmap(input: *const outputs_contract.GfxBufferHandle) i32;

extern fn r4os_rust_driver_gfx_device_acquire(reference: *const outputs_contract.GfxBufferHandle, request: *const outputs_contract.GfxDeviceRequest, output: *outputs_contract.GfxDeviceLease) i32;

extern fn r4os_rust_driver_gfx_device_segment(lease: *const outputs_contract.GfxDeviceLease, offset: u64, output: *outputs_contract.GfxDmaSegment) i32;

extern fn r4os_rust_driver_gfx_device_release(lease: *const outputs_contract.GfxDeviceLease, quiesced: u32) i32;

extern fn r4os_rust_driver_gfx_mmio_map(request: *const outputs_contract.GfxMmioRequest, output: *outputs_contract.GfxMmioWindow) i32;

extern fn r4os_rust_driver_gfx_mmio_unmap(window: *const outputs_contract.GfxBufferHandle, quiesced: u32) i32;

extern fn r4os_rust_driver_gfx_collect() i32;

extern fn r4os_rust_driver_gfx_owned_reserve(input: *const outputs_contract.GfxBufferDescriptor, cookie: u64, output: *outputs_contract.GfxOwnedBufferReservation) i32;

extern fn r4os_rust_driver_gfx_owned_commit(input: *const outputs_contract.GfxOwnedBufferReservation, output: *outputs_contract.GfxBufferReference) i32;

extern fn r4os_rust_driver_gfx_owned_abort(input: *const outputs_contract.GfxOwnedBufferReservation, quiesced: u32) i32;

extern fn r4os_rust_driver_gfx_owned_take_release(adapter: u32, generation: u64, output: *outputs_contract.GfxOwnedBufferRelease) i32;

extern fn r4os_rust_driver_gfx_owned_finish_release(input: *const outputs_contract.GfxOwnedBufferRelease, quiesced: u32) i32;

extern fn r4os_rust_driver_gfx_reserved_span(base: u64, bytes: u64) i32;

extern fn r4os_rust_driver_gfx_unmanaged_span(base: u64, bytes: u64) i32;

fn gfxMemoryQuery(output: *outputs_contract.GfxDriverMemoryApi) callconv(.c) i32 {
    return r4os_rust_driver_gfx_memory_query(output);
}
extern fn r4os_rust_driver_gfx_memory_query(output: *outputs_contract.GfxDriverMemoryApi) i32;

extern fn r4os_rust_driver_gfx_virtual_register(input: *const outputs_contract.GfxNativeProvider, output: *outputs_contract.GfxBufferHandle) i32;

extern fn r4os_rust_driver_gfx_virtual_unregister(input: *const outputs_contract.GfxBufferHandle) i32;

extern fn r4os_rust_driver_gfx_virtual_take(input: *const outputs_contract.GfxBufferHandle, output: *outputs_contract.GfxVirtualJob) i32;

extern fn r4os_rust_driver_gfx_virtual_complete(input: *const outputs_contract.GfxBufferHandle, completion: *const outputs_contract.GfxVirtualCompletion) i32;

extern fn r4os_rust_driver_gfx_native_register(input: *const outputs_contract.GfxNativeProvider, output: *outputs_contract.GfxBufferHandle) i32;

extern fn r4os_rust_driver_gfx_native_unregister(input: *const outputs_contract.GfxBufferHandle) i32;

extern fn r4os_rust_driver_gfx_native_take(input: *const outputs_contract.GfxBufferHandle, output: *outputs_contract.GfxNativeJob) i32;

extern fn r4os_rust_driver_gfx_native_complete(provider: *const outputs_contract.GfxBufferHandle, request: *const outputs_contract.GfxBufferHandle, result: i32, reference: *const outputs_contract.GfxBufferHandle) i32;

extern fn r4os_rust_driver_gfx_memory_stats(output: *outputs_contract.GfxBufferStats) i32;

extern fn r4os_rust_driver_gfx_memory_budget(input: *const outputs_contract.GfxDeviceBudgetRequest, output: *outputs_contract.GfxDeviceBudgetState) i32;

extern fn r4os_rust_driver_gfx_telemetry_exchange(input: *const outputs_contract.GfxTelemetryState, output: *outputs_contract.GfxTelemetryDemand) i32;

extern fn r4os_rust_driver_gfx_device_lost(adapter: u32, generation: u64, quiesced: u32) i32;

fn gfxOutputBrightnessPublish(input: *const outputs_contract.GfxOutputBrightness) callconv(.c) i32 {
    return r4os_rust_driver_gfx_output_brightness_publish(input);
}
extern fn r4os_rust_driver_gfx_output_brightness_publish(input: *const outputs_contract.GfxOutputBrightness) i32;
fn gfxBrightnessRead(input: *const outputs_contract.GfxOutputId, output: *outputs_contract.GfxBrightnessRequest) callconv(.c) i32 {
    return r4os_rust_driver_gfx_brightness_read(input, output);
}
extern fn r4os_rust_driver_gfx_brightness_read(input: *const outputs_contract.GfxOutputId, output: *outputs_contract.GfxBrightnessRequest) i32;

extern fn r4os_rust_driver_api_enter(owner: u32, ticks: u64) bool;
extern fn r4os_rust_driver_api_leave() bool;
extern fn r4os_rust_driver_api_owner_scalar(op: u8) u32;
extern fn r4os_rust_driver_api_bind(owner: u32, slot: usize) bool;
extern fn r4os_rust_driver_api_storage_enter(owner: u32, out: *StorageCallbackToken) bool;
extern fn r4os_rust_driver_api_storage_leave(token: *const StorageCallbackToken) void;
extern fn r4os_rust_driver_api_storage_owner() u32;
extern fn r4os_rust_driver_api_runtime_owner(runtime: bool) u32;
extern fn r4os_rust_driver_api_work_submit(handler: DriverWorkHandler, context: usize, flags: u32, out: *u32) i32;
extern fn r4os_rust_driver_api_work_owned(handler: DriverWorkHandler, context: usize, out: *u32) i32;
extern fn r4os_rust_driver_api_invoke_owned(owner: u32, handler: DriverWorkHandler, context: usize) i32;

export fn r4os_zig_driver_module_identity(slot: usize, generation: *u32) callconv(.c) bool {
    const module = @import("modules.zig").entryAt(slot) orelse return false;
    if (module.kind != .r4d) return false;
    generation.* = module.generation;
    return true;
}
export fn r4os_zig_driver_gfx_bind(owner: u32, epoch: u64) callconv(.c) bool {
    return gfx_memory.bind(owner, epoch);
}
export fn r4os_zig_driver_gfx_discard(owner: u32, epoch: u64) callconv(.c) bool {
    return gfx_memory.discardEmptyBinding(owner, epoch);
}
export fn r4os_zig_driver_owned_buffer_admit() callconv(.c) i32 {
    _ = currentBufferOwner(true) catch |err| return gfx_api.status(err);
    return 0;
}

extern fn r4os_rust_driver_dma_allocate(bytes: u32, alignment: u32, maximum: u64, out: *DmaBuffer) i32;
extern fn r4os_rust_driver_dma_free_physical(physical: u64) void;
extern fn r4os_rust_driver_dma_free(out: *DmaBuffer) void;
extern fn r4os_rust_driver_dma_pin(address: u64, bytes: u32, flags: u32, out: *DmaPinnedBuffer) i32;
extern fn r4os_rust_driver_dma_map(pin: *const DmaPinnedBuffer, c: *const DmaConstraints, direction: u32, out: *DmaMapping) i32;
extern fn r4os_rust_driver_dma_sync(mapping: *const DmaMapping, cpu: bool, range: bool, offset: u32, bytes: u32) i32;
extern fn r4os_rust_driver_dma_unmap(out: *DmaMapping) i32;
extern fn r4os_rust_driver_dma_unpin(out: *DmaPinnedBuffer) i32;
extern fn r4os_rust_driver_dma_cleanup(owner: u32) u32;

extern fn r4os_rust_driver_api_table(template: *const Table) *Table;
pub fn table() *Table {
    return r4os_rust_driver_api_table(&table_template);
}

extern fn r4os_rust_driver_port_inb(port: u16) u8;
extern fn r4os_rust_driver_port_outb(port: u16, value: u8) void;
extern fn r4os_rust_driver_port_inw(port: u16) u16;
extern fn r4os_rust_driver_port_outw(port: u16, value: u16) void;
extern fn r4os_rust_driver_port_inl(port: u16) u32;
extern fn r4os_rust_driver_port_outl(port: u16, value: u32) void;
extern fn r4os_rust_driver_wait_ticks(ticks: u64) void;
extern fn r4os_rust_driver_pci_count() u32;
extern fn r4os_rust_driver_pci_at(index: u32, out: *PciDeviceInfo) i32;
extern fn r4os_rust_driver_pci_find(class: u8, subclass: u8, start: u32, out: *PciDeviceInfo) i32;
extern fn r4os_rust_driver_pci_read(kind: u8, bus: u8, device: u8, function: u8, offset: u16) u32;
extern fn r4os_rust_driver_pci_write(kind: u8, bus: u8, device: u8, function: u8, offset: u16, value: u32) i32;
extern fn r4os_rust_driver_pci_bar(kind: u8, bus: u8, device: u8, function: u8, index: u8, wide: bool) u64;
extern fn r4os_rust_driver_pci_bus_master(kind: u8, bus: u8, device: u8, function: u8, flags: u32) i32;
extern fn r4os_rust_driver_pci_map(kind: u8, bus: u8, device: u8, function: u8, index: u8, bytes: u32, flags: u32, out: *MmioRegion) i32;

extern fn r4os_rust_driver_msi_enable(kind: u8, bus: u8, device: u8, function: u8) i32;
extern fn r4os_rust_driver_msi_disable(kind: u8, bus: u8, device: u8, function: u8) i32;
extern fn r4os_rust_driver_msi_unmask(owner: u32, irq: u8) bool;
extern fn r4os_rust_driver_msi_cleanup(owner: u32, out: *MsiOwnerCleanupResult) void;
export fn r4os_zig_driver_msi_log(msix: bool, irq: u8, vector: u32) callconv(.c) void {
    bootlog.puts(if (msix) "[R4D] MSI-X prepared irq=" else "[R4D] MSI enabled irq=");
    bootlog.putDec(irq);
    bootlog.puts(" vector=");
    bootlog.putDec(vector);
    bootlog.puts("\r\n");
}

extern fn r4os_rust_driver_heap_owner() u32;
extern fn r4os_rust_driver_semaphore_context() u32;
extern fn r4os_rust_driver_heap_api(out: *outputs_contract.DriverHeapApi) i32;
extern fn r4os_rust_driver_thread_api(out: *outputs_contract.DriverThreadApi) i32;
extern fn r4os_rust_driver_semaphore_api(out: *outputs_contract.DriverSemaphoreApi) i32;

extern fn r4os_rust_driver_resource_api(out: *outputs_contract.DriverResourceApi) i32;
export fn r4os_zig_driver_resource_read_owned(owner: u32, id: u64, offset: u64, out: [*]u8, len: u32, deadline: u64) callconv(.c) i32 {
    return driver_resources.readAt(owner, id, offset, out[0..len], deadline);
}
export fn r4os_zig_driver_platform_input(owner: u32, epoch: u64, kind: u32, value: u32) callconv(.c) i32 {
    return @import("../driver/input/platform_input.zig").submit(owner, epoch, kind, value);
}

const OptionSpan = extern struct { pointer: [*]const u8, len: usize };
const OptionView = extern struct { driver: OptionSpan, key: OptionSpan, value: OptionSpan };
extern fn r4os_rust_driver_option(driver: [*:0]const u8, key: [*:0]const u8) [*:0]const u8;
export fn r4os_zig_driver_option_count() callconv(.c) usize {
    return boot_config.get().option_count;
}
export fn r4os_zig_driver_option(index: usize, out: *OptionView) callconv(.c) void {
    const opt = &boot_config.get().options[index];
    out.* = .{ .driver = .{ .pointer = &opt.driver, .len = opt.driver_len }, .key = .{ .pointer = &opt.key, .len = opt.key_len }, .value = .{ .pointer = &opt.value, .len = opt.value_len } };
}

extern fn r4os_rust_driver_audio_register(name: [*:0]const u8, descriptor: *const AudioBackendDescriptor) i32;
extern fn r4os_rust_driver_audio_unregister(name: [*:0]const u8) i32;
extern fn r4os_rust_driver_audio_closing(owner: u32, closing: bool) bool;
extern fn r4os_rust_driver_audio_cleanup(owner: u32, out: *AudioOwnerCleanupResult) void;
extern fn r4os_rust_driver_storage_valid(d: *const StorageBackendDescriptor) bool;
extern fn r4os_rust_driver_storage_submit(d: *const StorageBackendDescriptor) ?StorageSubmitFn;
extern fn r4os_rust_driver_storage_cancel(d: *const StorageBackendDescriptor) ?StorageCancelFn;
extern fn r4os_rust_driver_storage_reset(d: *const StorageBackendDescriptor) ?StorageResetFn;
extern fn r4os_rust_driver_synth_valid(d: *const SynthEngineDescriptor) bool;
extern fn r4os_rust_driver_synth_pcm(d: *const SynthEngineDescriptor) ?audio.SynthRenderPcmCtxFn;
extern fn r4os_rust_driver_usb_valid(d: *const UsbHostControllerDescriptor) bool;
extern fn r4os_rust_driver_net_valid(d: *const NetBackendDescriptor) bool;
extern fn r4os_rust_driver_net_packet(d: *const NetBackendDescriptor) ?NetTransmitPacketFn;

const NativeStorageToken = extern struct { index: usize = 0, generation: u64 = 0, active: bool = false };
extern fn r4os_rust_driver_storage_register(name: [*:0]const u8, d: *const StorageBackendDescriptor) i32;
extern fn r4os_rust_driver_storage_unregister(name: [*:0]const u8) i32;
extern fn r4os_rust_driver_storage_recovery(name: [*:0]const u8, finish: bool, ok: i32) i32;
extern fn r4os_rust_driver_storage_status(index: usize, out: *StorageBackendStatus) bool;
extern fn r4os_rust_driver_storage_prepare(owner: u32, plan: *StorageCleanupPlan) bool;
extern fn r4os_rust_driver_storage_cancel_plan(plan: *StorageCleanupPlan) void;
extern fn r4os_rust_driver_storage_commit(plan: *StorageCleanupPlan, out: *StorageCleanupResult) void;
extern fn r4os_rust_driver_storage_io(context: ?*anyopaque, write: bool, lba: u64, sectors: u16, bytes: [*]u8, len: u32) bool;
extern fn r4os_rust_driver_storage_flush(context: ?*anyopaque) bool;
extern fn r4os_rust_driver_storage_async(context: ?*anyopaque, request: *const StorageRequest) i32;
extern fn r4os_rust_driver_storage_control(context: ?*anyopaque, reset: bool, handle: u64, reason: u32) i32;
export fn r4os_zig_block_find(name: [*]const u8, len: usize) callconv(.c) bool {
    return storage.findByName(name[0..len]) != null;
}
export fn r4os_zig_block_register(name: [*]const u8, len: usize, owner: u32, d: *const StorageBackendDescriptor, context: ?*anyopaque) callconv(.c) isize {
    const asynchronous = storageBackendSubmit(d) != null;
    const index = storage.register(.{ .name = name[0..len], .driver = "R4D", .bus = storageBus(d.bus), .controller = storageController(d), .port = 0, .sector_size = d.sector_size, .sector_count = d.sector_count, .max_sectors_per_request = d.max_sectors_per_request, .queue_depth = if (asynchronous) d.queue_depth else 1, .timeout_ticks = d.timeout_ticks, .removable = (d.flags & STORAGE_BACKEND_FLAG_REMOVABLE) != 0, .writable = (d.flags & STORAGE_BACKEND_FLAG_WRITABLE) != 0, .source = storageSource(d.source), .owner_id = owner, .ctx = context, .read_fn = r4dStorageRead, .write_fn = if (d.write != null) r4dStorageWrite else null, .flush_fn = if (d.flush != null) r4dStorageFlush else null, .async_submit_fn = if (asynchronous) r4dStorageSubmit else null, .async_cancel_fn = if (storageBackendCancel(d) != null) r4dStorageCancel else null, .async_reset_fn = if (storageBackendReset(d) != null) r4dStorageReset else null }) orelse return -1;
    return @intCast(index);
}
export fn r4os_zig_block_unregister(index: usize) callconv(.c) bool {
    return storage.unregister(index);
}
export fn r4os_zig_block_recovery(index: usize, finish: bool, success: bool) callconv(.c) void {
    if (finish) storage.finishBackendRecovery(index, success) else storage.beginBackendRecovery(index);
}
export fn r4os_zig_block_prepare(index: usize, out: *NativeStorageToken) callconv(.c) bool {
    const token = storage.prepareUnregister(index) orelse return false;
    out.* = .{ .index = token.index, .generation = token.generation, .active = token.active };
    return true;
}
export fn r4os_zig_block_token(input: *NativeStorageToken, commit: bool) callconv(.c) bool {
    var token = storage.UnregisterToken{ .index = input.index, .generation = input.generation, .active = input.active };
    const result = if (commit) storage.commitUnregister(&token) else storage.cancelUnregister(&token);
    input.* = .{ .index = token.index, .generation = token.generation, .active = token.active };
    return result;
}
export fn r4os_zig_block_registration_log(name: [*]const u8, len: usize, index: usize, source: u32) callconv(.c) void {
    bootlog.puts("[R4D] register storage backend ");
    bootlog.puts(name[0..len]);
    bootlog.puts(" block=");
    bootlog.putDec(index);
    bootlog.puts(" source=");
    bootlog.puts(storage.sourceLabel(storageSource(source)));
    bootlog.puts("\r\n");
}
comptime {
    if (@sizeOf(NativeStorageToken) != 24 or @sizeOf(StorageCleanupEntry) != 32 or @sizeOf(StorageCleanupPlan) != 264) @compileError("private storage cleanup C layout drift");
}

extern fn r4os_rust_driver_net_register(name: [*:0]const u8, d: *const NetBackendDescriptor) i32;
extern fn r4os_rust_driver_net_has_owner(owner: u32) bool;
extern fn r4os_rust_driver_net_cleanup(owner: u32, out: *NetOwnerCleanupResult) void;
extern fn r4os_rust_driver_net_transmit(index: usize, frame: [*]const u8, len: u32) u8;
extern fn r4os_rust_driver_net_poll(index: usize) void;
extern fn r4os_rust_driver_net_status(index: usize, out: *NetBackendStatus) i32;
extern fn r4os_rust_driver_net_receive(id: i32, frame: [*]const u8, len: u32) i32;
extern fn r4os_rust_driver_net_schedule(id: i32) i32;
extern fn r4os_rust_driver_net_query(id: i32, out: *NetBackendNegotiation) i32;
extern fn r4os_rust_driver_net_receive_packet(id: i32, packet: *const NetPacket) i32;
const NativeNetOffer = extern struct { offered: u64, required: u64, rx_queues: u16, tx_queues: u16, rx_segments: u16, tx_segments: u16, rx_owner: u32, tx_owner: u32, moderation: u32 };
export fn r4os_zig_net_mutation(op: u8) callconv(.c) bool {
    switch (op) {
        0 => return net.beginBackendMutation(),
        1 => net.endBackendMutation(),
        2 => net.quarantineBackendMutation(),
        else => unreachable,
    }
    return true;
}
export fn r4os_zig_net_callback(enter: bool) callconv(.c) bool {
    if (enter) return net.enterBackendCallback();
    net.leaveBackendCallback();
    return true;
}
export fn r4os_zig_net_negotiate(input: *const NativeNetOffer, out: *NetBackendNegotiation) callconv(.c) i32 {
    out.* = net_backend.negotiate(.{ .offered = input.offered, .required = input.required, .rx_queue_count = input.rx_queues, .tx_queue_count = input.tx_queues, .max_rx_segments = input.rx_segments, .max_tx_segments = input.tx_segments, .rx_ownership = input.rx_owner, .tx_ownership = input.tx_owner, .interrupt_moderation_us = input.moderation }) catch |err| return switch (err) {
        error.InvalidOffer => 1,
        error.RequiredNotOffered => 2,
        error.RequiredUnsupported => 3,
    };
    return 0;
}
export fn r4os_zig_net_register(name: [*]const u8, len: usize, d: *const NetBackendDescriptor, selected: *const NetBackendNegotiation) callconv(.c) isize {
    const flags = net.ADAPTER_FLAG_TRUSTED_BACKEND | (if ((d.flags & NET_BACKEND_FLAG_BROADCAST) != 0) net.ADAPTER_FLAG_BROADCAST else 0);
    const index = net.register(.{ .name = name[0..len], .driver = "R4D", .bus = netBus(d.bus_kind), .bus_no = d.bus, .device_no = d.device, .function_no = d.function, .vendor_id = d.vendor_id, .device_id = d.device_id, .mac = d.mac, .mtu = d.mtu, .flags = flags, .backend = selected.*, .link = if ((d.flags & NET_BACKEND_FLAG_LINK_UP) != 0) .up else .unknown, .ops = .{ .transmit = if (d.transmit != null) r4dNetTransmit else null, .poll = if (d.poll != null) r4dNetPoll else null, .status = if (d.status != null) r4dNetStatus else null } }) orelse return -1;
    return @intCast(index);
}
export fn r4os_zig_net_lookup(op: u8, value: usize) callconv(.c) isize {
    return switch (op) {
        0 => @intCast((net.get(value) orelse return -1).registration_id),
        1 => @intCast(net.indexForRegistration(@intCast(value)) orelse return -1),
        2 => @intCast(net.count()),
        else => unreachable,
    };
}
export fn r4os_zig_net_unregister(index: usize) callconv(.c) bool {
    return net.unregister(index);
}
export fn r4os_zig_net_receive(index: usize, packet: bool, queue: u16, flags: u64, frame: [*]const u8, len: u32) callconv(.c) u8 {
    return @intFromEnum(if (packet) net.receivePacket(index, queue, flags, frame[0..len]) else net.receiveFrame(index, frame[0..len]));
}
export fn r4os_zig_net_schedule(index: usize) callconv(.c) bool {
    return net.scheduleRxWork(index);
}
export fn r4os_zig_net_packet_valid(packet: *const NetPacket, selected: *const NetBackendNegotiation) callconv(.c) bool {
    return net_backend.validRxPacket(packet, selected.*, net.MAX_PACKET_SIZE);
}
export fn r4os_zig_net_registered_log(name: [*]const u8, len: usize, index: usize, n: *const NetBackendNegotiation) callconv(.c) void {
    bootlog.puts("[R4D] register net backend ");
    bootlog.puts(name[0..len]);
    bootlog.puts(" adapter=");
    bootlog.putDec(index);
    bootlog.puts(" caps=");
    bootlog.putHex(n.accepted, 16);
    bootlog.puts("/");
    bootlog.putHex(n.offered, 16);
    bootlog.puts(" queues=");
    bootlog.putDec(n.rx_queue_count);
    bootlog.puts("/");
    bootlog.putDec(n.tx_queue_count);
    bootlog.puts("\r\n");
}
comptime {
    if (@sizeOf(NativeNetOffer) != 40 or net.MAX_PACKET_SIZE != 1536 or @intFromEnum(net.TxResult.backend_error) != 6 or @intFromEnum(net.RxAdmission.irq_context) != 6) @compileError("private Net runtime C layout drift");
}

extern fn r4os_rust_driver_cleanup_prepare(owner: u32, out: *OwnerCleanupToken) bool;
extern fn r4os_rust_driver_cleanup_cancel(token: *OwnerCleanupToken) bool;
extern fn r4os_rust_driver_cleanup_begin(token: *OwnerCleanupToken) void;
extern fn r4os_rust_driver_cleanup_quarantine(token: *OwnerCleanupToken) bool;
extern fn r4os_rust_driver_cleanup_commit(token: *OwnerCleanupToken) bool;
extern fn r4os_rust_driver_cleanup(owner: u32) bool;
extern fn r4os_rust_driver_system_transition(owner: u32, op: u8) bool;
const NativeCleanupBackend = extern struct { removed: u32, failed: bool };
const NativeCleanupCounts = extern struct { irq: u32, work: u32, threads: u64, semaphores: u64, dma: u32, msi: u32, cpu_heap: u64, cpu_bytes: u64, audio: u32, storage: u32, usb_host: u32, display_blit: u32, net: u32 };
export fn r4os_zig_driver_cleanup_owner(op: u8, owner: u32) callconv(.c) u64 {
    switch (op) {
        0 => return @intFromBool(display.retainsDriverOwner(owner)),
        1 => return @intFromBool(@import("../display/native_driver.zig").retained(owner)),
        2 => return @intFromBool(gfx_memory.retained(owner)),
        3 => return @intFromBool(@import("../display/queue.zig").retainsDriver(owner)),
        4 => return @intFromBool(display_blit.prepareOwnerCleanup(owner)),
        5 => display_blit.cancelOwnerCleanup(owner),
        6 => @import("../display/queue.zig").closingDriver(owner),
        7 => gfx_memory.beginClose(owner),
        8 => gfx_memory.finishOwner(owner),
        9 => @import("../driver/input/platform_input.zig").remove(owner),
        10 => return display_blit.cleanupOwner(owner),
        else => unreachable,
    }
    return 0;
}
export fn r4os_zig_driver_usb_cleanup(owner: u32, out: *NativeCleanupBackend) callconv(.c) void {
    const result = usb_host.cleanupOwner(owner);
    out.* = .{ .removed = result.removed, .failed = result.failed };
}
export fn r4os_zig_driver_cleanup_log(event: u8, owner: u32, counts: ?*const NativeCleanupCounts) callconv(.c) void {
    switch (event) {
        0 => {
            const c = counts.?;
            const entry = @import("../driver/registry.zig").get(owner - 1);
            const name = if (entry) |*item| item.name[0..item.name_len] else "unknown";
            var message: [640]u8 = undefined;
            const line = std.fmt.bufPrint(&message, "[R4D] cleanup owner={d} irq={d} work={d} threads={d} semaphores={d} dma={d} cpu-heap={d} cpu-bytes={d} msi={d} audio={d} storage={d} usb-host={d} display-blit={d} net={d} name={s} result=complete\r\n", .{ owner, c.irq, c.work, c.threads, c.semaphores, c.dma, c.cpu_heap, c.cpu_bytes, c.msi, c.audio, c.storage, c.usb_host, c.display_blit, c.net, name }) catch unreachable;
            bootlog.puts(line);
        },
        1 => {
            var message: [128]u8 = undefined;
            bootlog.puts(std.fmt.bufPrint(&message, "[R4D] cleanup owner={d} thread-callbacks=active resources=quarantined\r\n", .{owner}) catch unreachable);
        },
        3 => bootlog.puts("[R4D] audio-finalize=FAILED resources=quarantined\r\n"),
        7 => bootlog.puts("[R4D] cleanup work=quiesced graphics=retained resources=quarantined\r\n"),
        11 => {
            bootlog.puts("[R4D] cleanup veto owner=");
            bootlog.putDec(owner);
            bootlog.puts(" backend-busy\r\n");
        },
        2, 4, 5, 6, 8, 9, 10 => {
            bootlog.puts("[R4D] cleanup owner=");
            bootlog.putDec(owner);
            bootlog.puts(switch (event) {
                2 => " net-finalize=FAILED resources=quarantined\r\n",
                4 => " usb-host-finalize=FAILED resources=quarantined\r\n",
                5 => " msi-disable=FAILED resources=quarantined\r\n",
                6 => " work-quiesce=FAILED dma=retained resources=quarantined\r\n",
                8 => " threads=retained dma=retained cpu-heap=retained resources=quarantined\r\n",
                9 => " semaphores=retained dma=retained cpu-heap=retained resources=quarantined\r\n",
                10 => " cpu-heap=retained resources=quarantined\r\n",
                else => unreachable,
            });
        },
        else => unreachable,
    }
}
comptime {
    if (@sizeOf(OwnerCleanupToken) != 280 or @offsetOf(OwnerCleanupToken, "storage_plan") != 8 or @sizeOf(NativeCleanupCounts) != 72) @compileError("private driver cleanup C layout drift");
}

extern fn r4os_rust_driver_log(level: u8, text: [*:0]const u8) void;
extern fn r4os_rust_driver_irq_legacy(release: bool) i32;
extern fn r4os_rust_driver_irq_register(irq: u8, handler: IrqHandler, context: usize, flags: u32) i32;
extern fn r4os_rust_driver_api_work_request(request: *const DriverWorkRequest, out: *u32) i32;
extern fn r4os_rust_driver_usb_register(name: [*:0]const u8, d: *const UsbHostControllerDescriptor) i32;
extern fn r4os_rust_driver_usb_unregister(name: [*:0]const u8) i32;
extern fn r4os_rust_driver_usb_activate(name: [*:0]const u8, source: u32) i32;
extern fn r4os_rust_driver_blit(register: bool, name: [*:0]const u8, d: ?*const anyopaque) i32;
extern fn r4os_rust_driver_synth(name: [*:0]const u8, d: *const SynthEngineDescriptor) i32;
extern fn r4os_rust_driver_input(name: [*:0]const u8) i32;
export fn r4os_zig_driver_usb(op: u8, name: ?[*]const u8, len: usize, owner: u32, source: u32, descriptor: ?*const UsbHostControllerDescriptor) callconv(.c) isize {
    return switch (op) {
        0 => @intFromBool(usb_host.findByName(name.?[0..len]) != null),
        1 => @intCast(usb_host.register(name.?[0..len], descriptor.?, owner) orelse return -1),
        2 => @intFromBool(usb_host.unregisterByName(name.?[0..len], owner)),
        3 => xhci.activate(owner, source),
        else => unreachable,
    };
}
export fn r4os_zig_driver_blit(register: bool, name: [*]const u8, len: usize, owner: u32, descriptor: ?*const anyopaque) callconv(.c) i32 {
    return if (register) display_blit.register(owner, name[0..len], @ptrCast(@alignCast(descriptor.?))) else display_blit.unregister(owner, name[0..len]);
}
const NativeGfxIdentity = extern struct { id: u64, generation: u64 };
fn gfxIdentity(value: NativeGfxIdentity) gfx_buffers.Owner {
    return .{ .kind = .driver, .id = value.id, .generation = value.generation };
}
extern fn r4os_rust_driver_gfx_identity(mode: u8, admit: bool, out: *NativeGfxIdentity) i32;
fn nativeGfxOwner(mode: u8, admit: bool) gfx_buffers.Error!gfx_buffers.Owner {
    var value: NativeGfxIdentity = undefined;
    const code = r4os_rust_driver_gfx_identity(mode, admit, &value);
    if (code == 0) return gfxIdentity(value);
    return switch (code) {
        outputs_contract.gfx_buffer_error_invalid => error.WrongOwner,
        outputs_contract.gfx_buffer_error_overflow => error.Overflow,
        outputs_contract.gfx_buffer_error_unsupported => error.Unsupported,
        outputs_contract.gfx_buffer_error_oom => error.OutOfMemory,
        outputs_contract.gfx_buffer_error_budget => error.Budget,
        outputs_contract.gfx_buffer_error_capacity => error.Capacity,
        outputs_contract.gfx_buffer_error_stale => error.Stale,
        outputs_contract.gfx_buffer_error_busy => error.Busy,
        outputs_contract.gfx_buffer_error_closed => error.Closed,
        else => unreachable,
    };
}
export fn r4os_zig_driver_buffer_identity(owner: u32, admit: bool, out: *NativeGfxIdentity) callconv(.c) i32 {
    const identity = gfx_memory.owner(owner, admit) catch |err| return gfx_api.status(err);
    out.* = .{ .id = identity.id, .generation = identity.generation };
    return 0;
}

export fn r4os_zig_driver_gfx_boot_hold(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBootHoldRequest, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../display/boot_driver.zig").hold(identity, input, output);
}
export fn r4os_zig_driver_gfx_cursor_configure(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.DisplayCursorInfo) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../display/cursor_work.zig").configure(identity, input);
}
export fn r4os_zig_driver_gfx_cursor_take(identity_view: *const NativeGfxIdentity, backend: *const outputs_contract.GfxBackendBinding, output: *outputs_contract.GfxDriverCursorJob) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../display/cursor_work.zig").take(identity, backend, output);
}
export fn r4os_zig_driver_gfx_cursor_complete(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxDriverCursorCompletion) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../display/cursor_work.zig").complete(identity, input);
}
export fn r4os_zig_driver_gfx_boot_finish(identity_view: *const NativeGfxIdentity, generation: u64, operation: u32, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../display/boot_driver.zig").finish(identity, generation, operation, output);
}
export fn r4os_zig_driver_gfx_display_boot_info(identity_view: *const NativeGfxIdentity, output: *outputs_contract.GfxNativeBootInfo) callconv(.c) i32 {
    _ = identity_view;
    return native_display.bootInfo(output);
}
export fn r4os_zig_driver_gfx_display_prepare(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxNativeRegistration, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return native_display.prepare(identity, input, output);
}
export fn r4os_zig_driver_gfx_display_prepare_held(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxNativeRegistration, generation: u64, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../display/boot_driver.zig").prepareHeld(identity, input, generation, output);
}
export fn r4os_zig_driver_gfx_display_transition(identity_view: *const NativeGfxIdentity, generation: u64, operation: u32, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return native_display.transition(@intCast(identity.id), generation, operation, output);
}
export fn r4os_zig_driver_gfx_display_device_reset(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBackendBinding, generation: u64, quiesced: u32, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return native_display.deviceReset(identity, input, generation, quiesced, output);
}
export fn r4os_zig_driver_gfx_display_resume_headless(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBackendBinding, generation: u64, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return native_display.resumeHeadless(identity, input, generation, output);
}
export fn r4os_zig_driver_gfx_display_terminal_release(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBackendBinding, generation: u64, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return native_display.releaseTerminal(identity, input, generation, output);
}
export fn r4os_zig_driver_gfx_display_prepare_reset(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxNativeRegistration, held_generation: u64, reset_generation: u64, output: *outputs_contract.GfxNativeState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../display/boot_driver.zig").prepareReset(identity, input, held_generation, reset_generation, output);
}
export fn r4os_zig_driver_gfx_output_query(identity_view: *const NativeGfxIdentity, output: *outputs_contract.GfxDriverOutputApi) callconv(.c) i32 {
    _ = identity_view;
    return @import("../display/output_state.zig").driverTable(output, .{ .publish = @intFromPtr(&gfxOutputPublish), .withdraw = @intFromPtr(&gfxOutputWithdraw), .register_source = @intFromPtr(&gfxReceiverRegister), .replace_receivers = @intFromPtr(&gfxReceiverReplace), .close_source = @intFromPtr(&gfxReceiverClose), .mode_enable = @intFromPtr(&gfxModeEnable), .mode_take = @intFromPtr(&gfxModeTake), .mode_complete = @intFromPtr(&gfxModeComplete), .audio_publish = @intFromPtr(&gfxAudioPublish), .audio_query = @intFromPtr(&gfxAudioQuery), .output_pause = @intFromPtr(&gfxOutputPause), .mode_restore = @intFromPtr(&gfxModeRestore), .mode_status = @intFromPtr(&gfxModeStatus), .color_publish = @intFromPtr(&gfxOutputColorPublish), .mode_read_color = @intFromPtr(&gfxModeReadColor), .refresh_publish = @intFromPtr(&gfxOutputRefreshPublish), .refresh_read = @intFromPtr(&gfxRefreshRead), .power_publish = @intFromPtr(&gfxOutputPowerPublish), .power_read = @intFromPtr(&gfxPowerRead), .brightness_publish = @intFromPtr(&gfxOutputBrightnessPublish), .brightness_read = @intFromPtr(&gfxBrightnessRead) });
}
export fn r4os_zig_driver_gfx_queue_register(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBackendRegistration, output: *outputs_contract.GfxBackendBinding) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_queue_api.register(identity, input, output);
}
export fn r4os_zig_driver_gfx_queue_register_profile(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBackendRegistration, profile: *const outputs_contract.GfxBackendProfile, output: *outputs_contract.GfxBackendBinding) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_queue_api.registerProfile(identity, input, profile, output);
}
export fn r4os_zig_driver_gfx_queue_publish_properties(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBackendBinding, properties: *const outputs_contract.GfxBackendProperties) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_queue_api.publishProperties(identity, input, properties);
}
export fn r4os_zig_driver_gfx_queue_read_native_info(owner: u32, input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxNativeJobInfo) callconv(.c) i32 {
    return gfx_queue_api.readNativeInfo(owner, input, output);
}
export fn r4os_zig_driver_gfx_queue_read_native_data(owner: u32, input: *const outputs_contract.GfxFence, offset: u32, output: [*]u8, count: u32) callconv(.c) i32 {
    return gfx_queue_api.readNativeData(owner, input, offset, output, count);
}
export fn r4os_zig_driver_gfx_queue_read_native_binding(owner: u32, input: *const outputs_contract.GfxFence, index: u32, output: *outputs_contract.GfxNativeBinding) callconv(.c) i32 {
    return gfx_queue_api.readNativeBinding(owner, input, index, output);
}
export fn r4os_zig_driver_gfx_queue_owner_info(owner: u32, input: *const outputs_contract.GfxBackendBinding, timeline: u64, output: *outputs_contract.GfxQueueOwnerInfo) callconv(.c) i32 {
    return gfx_queue_api.queueOwnerInfo(owner, input, timeline, output);
}
export fn r4os_zig_driver_gfx_queue_unregister(owner: u32, input: *const outputs_contract.GfxBackendBinding, quiesced: u32) callconv(.c) i32 {
    return gfx_queue_api.unregister(owner, input, quiesced);
}
export fn r4os_zig_driver_gfx_queue_take(owner: u32, input: *const outputs_contract.GfxBackendBinding, output: *outputs_contract.GfxDriverJob) callconv(.c) i32 {
    return gfx_queue_api.take(owner, input, output);
}
export fn r4os_zig_driver_gfx_queue_read_render_list(owner: u32, input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxRenderList) callconv(.c) i32 {
    return gfx_queue_api.readRenderList(owner, input, output);
}
export fn r4os_zig_driver_gfx_queue_read_render_grid_list(owner: u32, input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxRenderGridList) callconv(.c) i32 {
    return gfx_queue_api.readRenderGridList(owner, input, output);
}
export fn r4os_zig_driver_gfx_queue_update_operations(owner: u32, input: *const outputs_contract.GfxBackendBinding, operations: u64) callconv(.c) i32 {
    return gfx_queue_api.updateOperations(owner, input, operations);
}
export fn r4os_zig_driver_gfx_queue_read_render_color_list(owner: u32, input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxRenderColorList) callconv(.c) i32 {
    return gfx_queue_api.readRenderColorList(owner, input, output);
}
export fn r4os_zig_driver_gfx_queue_complete(owner: u32, input: *const outputs_contract.GfxFence, result: u32, quiesced: u32) callconv(.c) i32 {
    return gfx_queue_api.complete(owner, input, result, quiesced);
}
export fn r4os_zig_driver_gfx_queue_reset(owner: u32, input: *const outputs_contract.GfxBackendBinding, quiesced: u32, output: *outputs_contract.GfxBackendBinding) callconv(.c) i32 {
    return gfx_queue_api.reset(owner, input, quiesced, output);
}
export fn r4os_zig_driver_gfx_queue_segment(owner: u32, input: *const outputs_contract.GfxFence, which: u32, offset: u64, mask: u64, output: *outputs_contract.GfxDmaSegment) callconv(.c) i32 {
    return gfx_queue_api.segment(owner, input, which, offset, mask, output);
}
export fn r4os_zig_driver_gfx_queue_retain(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxFence, which: u32, output: *outputs_contract.GfxBufferReference) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_queue_api.retain(identity, input, which, output);
}
export fn r4os_zig_driver_gfx_queue_retain_scanout(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxFence, output: *outputs_contract.GfxBufferReference) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_queue_api.retainScanout(identity, input, output);
}
export fn r4os_zig_driver_gfx_queue_begin_scanout(owner: u32, input: *const outputs_contract.GfxFence) callconv(.c) i32 {
    return gfx_queue_api.beginScanout(owner, input);
}
export fn r4os_zig_driver_gfx_queue_scanout_retire_requested(owner: u32, input: *const outputs_contract.GfxFence) callconv(.c) i32 {
    return gfx_queue_api.scanoutRetireRequested(owner, input);
}
export fn r4os_zig_driver_gfx_buffer_create(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferDescriptor, output: *outputs_contract.GfxBufferReference) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_api.create(identity, input, output);
}
export fn r4os_zig_driver_gfx_buffer_describe(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle, output: *outputs_contract.GfxBufferDescriptor) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_api.describe(identity, input, output);
}
export fn r4os_zig_driver_gfx_buffer_import(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle, output: *outputs_contract.GfxBufferReference) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_api.import(identity, input, output);
}
export fn r4os_zig_driver_gfx_buffer_release(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_api.release(identity, input);
}
export fn r4os_zig_driver_gfx_buffer_map(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle, access: u32, offset: u64, bytes: u64, output: *outputs_contract.GfxBufferMap) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_api.map(identity, input, access, offset, bytes, output);
}
export fn r4os_zig_driver_gfx_buffer_unmap(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_api.unmap(identity, input);
}
export fn r4os_zig_driver_gfx_device_acquire(identity_view: *const NativeGfxIdentity, reference: *const outputs_contract.GfxBufferHandle, request: *const outputs_contract.GfxDeviceRequest, output: *outputs_contract.GfxDeviceLease) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_memory.acquire(identity, reference, request, output);
}
export fn r4os_zig_driver_gfx_device_segment(identity_view: *const NativeGfxIdentity, lease: *const outputs_contract.GfxDeviceLease, offset: u64, output: *outputs_contract.GfxDmaSegment) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_memory.segment(identity, lease, offset, output);
}
export fn r4os_zig_driver_gfx_device_release(identity_view: *const NativeGfxIdentity, lease: *const outputs_contract.GfxDeviceLease, quiesced: u32) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_memory.release(identity, lease, quiesced);
}
export fn r4os_zig_driver_gfx_mmio_map(identity_view: *const NativeGfxIdentity, request: *const outputs_contract.GfxMmioRequest, output: *outputs_contract.GfxMmioWindow) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_memory.mapWindow(identity, request, output);
}
export fn r4os_zig_driver_gfx_mmio_unmap(identity_view: *const NativeGfxIdentity, window: *const outputs_contract.GfxBufferHandle, quiesced: u32) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return gfx_memory.unmapWindow(identity, window, quiesced);
}
export fn r4os_zig_driver_gfx_collect(identity_view: *const NativeGfxIdentity, full: bool) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return if (full) gfx_memory.collect(identity) else gfx_memory.collectBuffers(identity);
}
export fn r4os_zig_driver_gfx_owned_reserve(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferDescriptor, cookie: u64, output: *outputs_contract.GfxOwnedBufferReservation) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_owned_buffers.zig").reserve(identity, input, cookie, output);
}
export fn r4os_zig_driver_gfx_owned_commit(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOwnedBufferReservation, output: *outputs_contract.GfxBufferReference) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_owned_buffers.zig").commit(identity, input, output);
}
export fn r4os_zig_driver_gfx_owned_abort(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOwnedBufferReservation, quiesced: u32) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_owned_buffers.zig").abort(identity, input, quiesced);
}
export fn r4os_zig_driver_gfx_owned_take_release(identity_view: *const NativeGfxIdentity, adapter: u32, generation: u64, output: *outputs_contract.GfxOwnedBufferRelease) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_owned_buffers.zig").take(identity, adapter, generation, output);
}
export fn r4os_zig_driver_gfx_owned_finish_release(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOwnedBufferRelease, quiesced: u32) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_owned_buffers.zig").finish(identity, input, quiesced);
}
export fn r4os_zig_driver_gfx_virtual_register(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxNativeProvider, output: *outputs_contract.GfxBufferHandle) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_virtual_driver_api.zig").register(identity, input, output);
}
export fn r4os_zig_driver_gfx_virtual_unregister(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_virtual_driver_api.zig").unregister(identity, input);
}
export fn r4os_zig_driver_gfx_virtual_take(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle, output: *outputs_contract.GfxVirtualJob) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_virtual_driver_api.zig").take(identity, input, output);
}
export fn r4os_zig_driver_gfx_virtual_complete(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle, completion: *const outputs_contract.GfxVirtualCompletion) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_virtual_driver_api.zig").complete(identity, input, completion);
}
export fn r4os_zig_driver_gfx_native_register(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxNativeProvider, output: *outputs_contract.GfxBufferHandle) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_allocation_driver_api.zig").register(identity, input, output);
}
export fn r4os_zig_driver_gfx_native_unregister(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_allocation_driver_api.zig").unregister(identity, input);
}
export fn r4os_zig_driver_gfx_native_take(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBufferHandle, output: *outputs_contract.GfxNativeJob) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_allocation_driver_api.zig").take(identity, input, output);
}
export fn r4os_zig_driver_gfx_native_complete(identity_view: *const NativeGfxIdentity, provider: *const outputs_contract.GfxBufferHandle, request: *const outputs_contract.GfxBufferHandle, result: i32, reference: *const outputs_contract.GfxBufferHandle) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_allocation_driver_api.zig").complete(identity, provider, request, result, reference);
}
export fn r4os_zig_driver_gfx_memory_stats(identity_view: *const NativeGfxIdentity, output: *outputs_contract.GfxBufferStats) callconv(.c) i32 {
    _ = identity_view;
    return gfx_api.stats(output);
}
export fn r4os_zig_driver_gfx_memory_budget(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxDeviceBudgetRequest, output: *outputs_contract.GfxDeviceBudgetState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../program/gfx_memory_budget_api.zig").provider(identity, input, output);
}
export fn r4os_zig_driver_gfx_telemetry_exchange(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxTelemetryState, output: *outputs_contract.GfxTelemetryDemand) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("../program/gfx_telemetry_api.zig").publish(identity, input, output);
}
export fn r4os_zig_driver_gfx_device_lost(identity_view: *const NativeGfxIdentity, adapter: u32, generation: u64, quiesced: u32) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    return @import("gfx_owned_buffers.zig").lose(identity, adapter, generation, quiesced);
}

export fn r4os_zig_driver_gfx_output_power_publish(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputPower) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/outputs.zig").publishPower(@intCast(identity.id), input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_output_refresh_publish(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputRefresh) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/outputs.zig").publishRefresh(@intCast(identity.id), input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_output_brightness_publish(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputBrightness) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/outputs.zig").publishBrightness(@intCast(identity.id), input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_mode_enable(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBackendBinding) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    gfx_modes.enable(identity, input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_mode_complete(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxDriverModeCompletion) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    gfx_modes.complete(identity, input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_receiver_close(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxReceiverSource) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/outputs.zig").closeReceiverSource(identity, input.*) catch |err| return @import("../program/gfx_output_api.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_power_read(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputId, output: *outputs_contract.GfxPowerRequest) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    output.* = @import("../display/outputs.zig").readPower(@intCast(identity.id), input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_refresh_read(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputTarget, output: *outputs_contract.GfxRefreshRequest) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    output.* = @import("../display/outputs.zig").readRefresh(@intCast(identity.id), input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_brightness_read(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputId, output: *outputs_contract.GfxBrightnessRequest) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    output.* = @import("../display/outputs.zig").readBrightness(@intCast(identity.id), input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_display_output_register(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxAdditionalOutput, output: *outputs_contract.GfxOutputTarget) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    const value = input.*;
    output.* = @import("../display/output_runtime.zig").register(identity, value) catch |err| return @import("../program/gfx_output_api.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_display_output_transition(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputTarget, operation: u32, quiesced: u32) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/output_runtime.zig").transition(identity, input.*, operation, quiesced) catch |err| return @import("../program/gfx_output_api.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_mode_read_color(identity_view: *const NativeGfxIdentity, ticket: u64, sequence: u64, output: *outputs_contract.GfxDriverModeColor) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    const value = gfx_modes.readColor(identity, ticket, sequence) catch |err| return gfx_modes.code(err);
    output.* = value orelse return 0;
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_output_pause(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputId, paused: u32) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/outputs.zig").pause(@intCast(identity.id), input.*, paused != 0) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_mode_restore(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxAtomicState, output: *outputs_contract.GfxModeStatus) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    output.* = gfx_modes.restore(identity, input) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_mode_status(identity_view: *const NativeGfxIdentity, ticket: u64, output: *outputs_contract.GfxModeStatus) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    output.* = gfx_modes.driverStatus(identity, ticket) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_audio_publish(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxAudioRoute) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/outputs.zig").publishAudio(identity, input) catch |err| return @import("../program/gfx_output_api.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_audio_query(identity_view: *const NativeGfxIdentity, location: u32, device: u32, index: u32, output: *outputs_contract.GfxAudioRoute) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    _ = identity;
    output.* = @import("../display/outputs.zig").queryAudio(location, device, index) orelse return 0;
    return 1;
}

export fn r4os_zig_driver_gfx_mode_take(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxBackendBinding, output: *outputs_contract.GfxDriverModeJob) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    const value = gfx_modes.take(identity, input.*) catch |err| return gfx_modes.code(err);
    output.* = value orelse return 0;
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_receiver_replace(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxReceiverUpdate) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/outputs.zig").replaceReceivers(identity, input) catch |err| return @import("../program/gfx_output_api.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_output_publish(owner: u32, input: *const outputs_contract.GfxOutputPublication, output: *outputs_contract.GfxOutputId) callconv(.c) i32 {
    output.* = @import("../display/outputs.zig").publish(owner, input) catch |err| return @import("../program/gfx_output_api.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_output_withdraw(owner: u32, input: *const outputs_contract.GfxOutputId) callconv(.c) i32 {
    @import("../display/outputs.zig").withdraw(owner, input.*) catch |err| return @import("../program/gfx_output_api.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_display_schedule(owner: u32, input: *const outputs_contract.GfxBackendBinding) callconv(.c) i32 {
    @import("../display/queue.zig").wakeNative(owner, .{ .adapter = input.adapter_id, .device_generation = input.device_generation, .reset_generation = input.reset_generation }) catch |err| return @import("../program/gfx_queue_api.zig").errorCode(err);
    return outputs_contract.gfx_queue_ok;
}

export fn r4os_zig_driver_receiver_epoch() callconv(.c) u64 {
    return @import("../display/outputs.zig").receiverEpoch();
}

export fn r4os_zig_driver_gfx_receiver_register(identity_view: *const NativeGfxIdentity, epoch: u64, adapter: u32, output: *outputs_contract.GfxReceiverSource) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    output.* = @import("../display/outputs.zig").registerReceiverSource(identity, adapter, epoch) catch |err| return @import("../program/gfx_output_api.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_display_presentation_stats(identity_view: *const NativeGfxIdentity, stage: u8, input: *const outputs_contract.DisplayPresentationStats) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    if (stage == 0) {
        const handled = @import("../display/output_runtime.zig").publishStats(identity, input.*) catch |err| return @import("../program/gfx_output_api.zig").code(err);
        return @intFromBool(handled);
    }
    @import("../display/display.zig").publishPresentationStats(identity.id, identity.generation, input.*) catch |err| return @import("../display/presentation_stats.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_display_presentation_info(identity_view: *const NativeGfxIdentity, stage: u8, input: *const outputs_contract.DisplayPresentationInfo) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    if (stage == 0) {
        const handled = @import("../display/output_runtime.zig").publishInfo(identity, input.*) catch |err| return @import("../program/gfx_output_api.zig").code(err);
        return @intFromBool(handled);
    }
    @import("../display/display.zig").publishPresentationInfo(identity.id, identity.generation, input.*) catch |err| return @import("../display/presentation_stats.zig").code(err);
    return outputs_contract.gfx_output_ok;
}

export fn r4os_zig_driver_gfx_span(base: u64, bytes: u64, unmanaged: bool) callconv(.c) bool {
    return if (unmanaged) @import("../memory/device_reserved.zig").bootExcludesSystemMemory(base, bytes, @import("../bootloader/boot_info.zig").get()) else @import("../memory/device_reserved.zig").bootCovers(base, bytes, @import("../bootloader/boot_info.zig").get());
}

export fn r4os_zig_driver_gfx_output_color_publish(identity_view: *const NativeGfxIdentity, input: *const outputs_contract.GfxOutputColorState) callconv(.c) i32 {
    const identity = gfxIdentity(identity_view.*);
    @import("../display/outputs.zig").publishColor(@intCast(identity.id), input.*) catch |err| return gfx_modes.code(err);
    return outputs_contract.gfx_output_ok;
}
