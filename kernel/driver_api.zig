const std = @import("std");
const io = @import("../arch/x86_64/io.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const audio = @import("../audio/core.zig");
const display_blit = @import("../display/blit_backend.zig");
const bootlog = @import("bootlog.zig");
const boot_config = @import("boot_config.zig");
const log_event = @import("log_event.zig");
const net = @import("../net/core.zig");
const net_backend = @import("../net/backend_contract.zig");
const pci_inventory = @import("../platform/pci_inventory.zig");
const smp = @import("smp.zig");
const paging = @import("../memory/paging.zig");
const phys = @import("../memory/phys.zig");
const memory_layout = @import("../memory/layout.zig");
const dma_segments = @import("dma_segments.zig");
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

pub const MAGIC: u32 = 0x31495044; // "DPI1" little endian
// Version 24 (0.69.50): append-only um ausgehandelte Netzwerkfaehigkeiten
// und Metadatenpakete mit kanonischem Flat-Fallback erweitert.
pub const VERSION: u32 = 24;

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
const MAX_R4D_AUDIO_BACKENDS: usize = 4;
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

const empty_z: [1:0]u8 = .{0};
var option_value_z: [64:0]u8 = .{0} ** 64;

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

const R4DStorageBackend = struct {
    used: bool = false,
    owner: u32 = 0,
    block_index: usize = 0,
    name: [MAX_R4D_STORAGE_NAME]u8 = .{0} ** MAX_R4D_STORAGE_NAME,
    name_len: usize = 0,
    descriptor: *const StorageBackendDescriptor = undefined,
};

const R4DNetBackend = struct {
    used: bool = false,
    owner: u32 = 0,
    adapter_index: usize = 0,
    name: [MAX_R4D_NET_NAME]u8 = .{0} ** MAX_R4D_NET_NAME,
    name_len: usize = 0,
    descriptor: *const NetBackendDescriptor = undefined,
    negotiation: NetBackendNegotiation = .{},
};

const R4DAudioBackend = struct {
    used: bool = false,
    owner: u32 = 0,
    name: [MAX_R4D_AUDIO_NAME]u8 = .{0} ** MAX_R4D_AUDIO_NAME,
    name_len: usize = 0,
    descriptor: *const AudioBackendDescriptor = undefined,
};

var r4d_storage_backends: [MAX_R4D_STORAGE_BACKENDS]R4DStorageBackend = .{R4DStorageBackend{}} ** MAX_R4D_STORAGE_BACKENDS;
var r4d_net_backends: [MAX_R4D_NET_BACKENDS]R4DNetBackend = .{R4DNetBackend{}} ** MAX_R4D_NET_BACKENDS;
var r4d_audio_backends: [MAX_R4D_AUDIO_BACKENDS]R4DAudioBackend = .{R4DAudioBackend{}} ** MAX_R4D_AUDIO_BACKENDS;

const DmaAllocation = struct {
    used: bool = false,
    owner: u32 = 0,
    phys_addr: u64 = 0,
    bytes: u32 = 0,
};

const DmaPinRecord = struct {
    used: bool = false,
    generation: u64 = 0,
    owner: u32 = 0,
    virt_addr: u64 = 0,
    bytes: u32 = 0,
    page_count: u32 = 0,
    map_count: u32 = 0,
};

const DmaMappingRecord = struct {
    used: bool = false,
    generation: u64 = 0,
    owner: u32 = 0,
    pin_slot: usize = 0,
    pin_handle: u64 = 0,
    direction: u32 = DMA_DIRECTION_BIDIRECTIONAL,
    flags: u32 = 0,
    original_virt: u64 = 0,
    requested_bytes: u32 = 0,
    bounce_phys: u64 = 0,
    bounce_virt: u64 = 0,
    bounce_bytes: u32 = 0,
    device_owned: bool = false,
    segments: dma_segments.SegmentList = .{},
};

const MsiAllocation = struct {
    used: bool = false,
    owner: u32 = 0,
    bus_kind: u8 = 0,
    bus: u8 = 0,
    device: u8 = 0,
    function: u8 = 0,
    irq: u8 = 0,
    capability: u16 = 0,
    original_control: u16 = 0,
    original_command: u16 = 0,
};

const MsiOwnerCleanupResult = struct {
    removed: u32 = 0,
    failed: bool = false,
};

var current_owner: u32 = 0;
var current_owner_guard = sync.UnwindGuard.init("r4d-owner");

const StorageCallbackOwner = struct {
    used: bool = false,
    task_id: u32 = 0,
    owner: u32 = 0,
    depth: u32 = 0,
};

const StorageCallbackToken = struct {
    slot: usize,
    task_id: u32,
};

var storage_callback_owners: [STORAGE_CALLBACK_OWNER_CAPACITY]StorageCallbackOwner =
    .{StorageCallbackOwner{}} ** STORAGE_CALLBACK_OWNER_CAPACITY;
var dma_allocations: [MAX_R4D_DMA_ALLOCATIONS]DmaAllocation = .{DmaAllocation{}} ** MAX_R4D_DMA_ALLOCATIONS;
var dma_pins: [MAX_R4D_DMA_PINS]DmaPinRecord = .{DmaPinRecord{}} ** MAX_R4D_DMA_PINS;
var dma_pin_generations: [MAX_R4D_DMA_PINS]u64 = .{0} ** MAX_R4D_DMA_PINS;
var dma_mappings: [MAX_R4D_DMA_MAPPINGS]DmaMappingRecord = .{DmaMappingRecord{}} ** MAX_R4D_DMA_MAPPINGS;
var dma_mapping_generations: [MAX_R4D_DMA_MAPPINGS]u64 = .{0} ** MAX_R4D_DMA_MAPPINGS;

pub const OwnerCleanupToken = struct {
    owner: u32 = 0,
    storage_plan: StorageCleanupPlan = .{},
    net_mutation_active: bool = false,
    display_blit_prepared: bool = false,
    shutdown_started: bool = false,
    active: bool = false,
};

const NetOwnerCleanupResult = struct {
    removed: u32 = 0,
    failed: bool = false,
};

pub fn enterOwner(owner: u32) bool {
    return enterOwnerBounded(owner, sync.WAIT_FOREVER);
}

pub fn enterOwnerBounded(owner: u32, timeout_ticks: u64) bool {
    if (owner == 0) return false;
    if (!current_owner_guard.enter(timeout_ticks)) return false;
    if (current_owner != 0 and current_owner != owner) {
        _ = current_owner_guard.leave();
        return false;
    }
    current_owner = owner;
    return true;
}

pub fn leaveOwner() bool {
    if (!current_owner_guard.ownedByCurrent()) return false;
    if (current_owner_guard.depth == 1) current_owner = 0;
    return current_owner_guard.leave();
}

pub fn prepareOwnerCleanup(owner: u32) ?OwnerCleanupToken {
    if (owner == 0 or current_owner != owner or !current_owner_guard.ownedByCurrent()) return null;
    var token = OwnerCleanupToken{ .owner = owner };
    if (!prepareStorageOwnerCleanup(owner, &token.storage_plan)) return null;
    if (ownerHasNetBackend(owner)) {
        if (!net.beginBackendMutation()) {
            cancelStorageOwnerCleanup(&token.storage_plan);
            return null;
        }
        token.net_mutation_active = true;
    }
    if (!display_blit.prepareOwnerCleanup(owner)) {
        cancelStorageOwnerCleanup(&token.storage_plan);
        finishOwnerNetMutation(&token);
        return null;
    }
    token.display_blit_prepared = true;
    token.active = true;
    return token;
}

pub fn cancelOwnerCleanup(token: *OwnerCleanupToken) bool {
    if (!token.active or token.shutdown_started or current_owner != token.owner or !current_owner_guard.ownedByCurrent()) return false;
    cancelStorageOwnerCleanup(&token.storage_plan);
    finishOwnerNetMutation(token);
    if (token.display_blit_prepared) display_blit.cancelOwnerCleanup(token.owner);
    token.active = false;
    return true;
}

pub fn beginOwnerShutdown(token: *OwnerCleanupToken) void {
    token.shutdown_started = true;
}

pub fn quarantineOwnerCleanup(token: *OwnerCleanupToken) bool {
    if (!token.active or !token.shutdown_started or current_owner != token.owner or !current_owner_guard.ownedByCurrent()) return false;
    cancelStorageOwnerCleanup(&token.storage_plan);
    quarantineOwnerNetMutation(token);
    token.active = false;
    return true;
}

fn finishOwnerNetMutation(token: *OwnerCleanupToken) void {
    if (!token.net_mutation_active) return;
    net.endBackendMutation();
    token.net_mutation_active = false;
}

fn quarantineOwnerNetMutation(token: *OwnerCleanupToken) void {
    if (!token.net_mutation_active) return;
    net.quarantineBackendMutation();
    token.net_mutation_active = false;
}

pub fn commitOwnerCleanup(token: *OwnerCleanupToken) bool {
    if (!token.active or token.owner == 0 or current_owner != token.owner or !current_owner_guard.ownedByCurrent()) return false;
    const owner = token.owner;

    // A prepared token only closes admissions. Destructive cleanup is legal
    // after the owning R4D has explicitly started its top-level shutdown.
    if (!token.shutdown_started) {
        cancelStorageOwnerCleanup(&token.storage_plan);
        finishOwnerNetMutation(token);
        if (token.display_blit_prepared) display_blit.cancelOwnerCleanup(owner);
        token.active = false;
        return false;
    }

    // Backend finalizers run while every storage admission is stopped and all
    // generic owner resources (IRQ, work, DMA, USB and network) still exist.
    // The top-level R4D shutdown is the first unload veto. Backend finalizers
    // are checked again under closed callback admission; any failure stops
    // generic IRQ/work/DMA release and quarantines the remaining ownership.
    const storage_cleanup = commitStorageOwnerCleanup(&token.storage_plan);
    if (storage_cleanup.remaining != 0) {
        quarantineOwnerNetMutation(token);
        token.active = false;
        return false;
    }

    // Backend finalizers must run before their generic IRQ/work/DMA resources
    // disappear. In particular, a network backend may still need its MMIO,
    // IRQ registration and tracked DMA region to prove a safe device stop.
    const net_cleanup = cleanupNetOwner(owner);
    if (net_cleanup.failed) {
        quarantineOwnerNetMutation(token);
        token.active = false;
        bootlog.puts("[R4D] cleanup owner=");
        bootlog.putDec(owner);
        bootlog.puts(" net-finalize=FAILED resources=quarantined\r\n");
        return false;
    }
    finishOwnerNetMutation(token);
    const display_blit_count = display_blit.cleanupOwner(owner);
    const audio_count = cleanupAudioOwner(owner);
    const usb_host_cleanup = usb_host.cleanupOwner(owner);
    if (usb_host_cleanup.failed) {
        token.active = false;
        bootlog.puts("[R4D] cleanup owner=");
        bootlog.putDec(owner);
        bootlog.puts(" usb-host-finalize=FAILED resources=quarantined\r\n");
        return false;
    }
    const msi_cleanup = cleanupMsiOwner(owner);
    if (msi_cleanup.failed) {
        token.active = false;
        bootlog.puts("[R4D] cleanup owner=");
        bootlog.putDec(owner);
        bootlog.puts(" msi-disable=FAILED resources=quarantined\r\n");
        return false;
    }
    const irq_count = irq_router.cleanupOwner(owner);
    const work_cleanup = driver_work.cleanupOwner(owner);
    if (!work_cleanup.quiesced) {
        token.active = false;
        bootlog.puts("[R4D] cleanup owner=");
        bootlog.putDec(owner);
        bootlog.puts(" work-quiesce=FAILED dma=retained resources=quarantined\r\n");
        return false;
    }
    const work_count = work_cleanup.removed;
    const dma_count = cleanupDmaOwner(owner);
    token.active = false;

    if (irq_count == 0 and
        work_count == 0 and
        dma_count == 0 and
        msi_cleanup.removed == 0 and
        audio_count == 0 and
        storage_cleanup.removed == 0 and
        usb_host_cleanup.removed == 0 and
        display_blit_count == 0 and
        net_cleanup.removed == 0)
    {
        return true;
    }
    bootlog.puts("[R4D] cleanup owner=");
    bootlog.putDec(owner);
    bootlog.puts(" irq=");
    bootlog.putDec(irq_count);
    bootlog.puts(" work=");
    bootlog.putDec(work_count);
    bootlog.puts(" dma=");
    bootlog.putDec(dma_count);
    bootlog.puts(" msi=");
    bootlog.putDec(msi_cleanup.removed);
    bootlog.puts(" audio=");
    bootlog.putDec(audio_count);
    bootlog.puts(" storage=");
    bootlog.putDec(storage_cleanup.removed);
    bootlog.puts(" usb-host=");
    bootlog.putDec(usb_host_cleanup.removed);
    bootlog.puts(" display-blit=");
    bootlog.putDec(display_blit_count);
    bootlog.puts(" net=");
    bootlog.putDec(net_cleanup.removed);
    bootlog.puts("\r\n");
    return true;
}

pub fn cleanupOwner(owner: u32) bool {
    if (owner == 0) return true;
    if (!enterOwner(owner)) return false;
    defer _ = leaveOwner();
    var token = prepareOwnerCleanup(owner) orelse {
        bootlog.puts("[R4D] cleanup veto owner=");
        bootlog.putDec(owner);
        bootlog.puts(" backend-busy\r\n");
        return false;
    };
    return commitOwnerCleanup(&token);
}

/// Snapshot the owner-provided status for a registered R4D block backend.
/// This is an internal diagnostic projection: it neither changes backend
/// admission nor exposes another public ABI surface.
pub fn queryStorageBackendStatus(block_index: usize, out: *StorageBackendStatus) bool {
    out.* = .{
        .state = 0,
        .last_error = 0,
        .last_lba = 0,
        .last_sectors = 0,
        .recoveries = 0,
        .recovery_failures = 0,
    };
    for (&r4d_storage_backends) |*backend| {
        if (!backend.used or backend.block_index != block_index) continue;
        const status = backend.descriptor.status orelse return false;
        const callback = enterStorageCallback(backend.owner) orelse return false;
        defer leaveStorageCallback(callback);
        return status(backend.descriptor.context, out) == 0;
    }
    return false;
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
};

pub var table = Table{
    .magic = MAGIC,
    .version = VERSION,
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
    log_event.driver(log_event.Severity.info, current_owner, text);
}

fn logWarn(text: [*:0]const u8) callconv(.c) void {
    log_event.driver(log_event.Severity.warn, current_owner, text);
}

fn logError(text: [*:0]const u8) callconv(.c) void {
    log_event.driver(log_event.Severity.err, current_owner, text);
}

fn portInb(port: u16) callconv(.c) u8 {
    return io.inb(port);
}

fn portOutb(port: u16, value: u8) callconv(.c) void {
    io.outb(port, value);
}

fn portInw(port: u16) callconv(.c) u16 {
    return io.inw(port);
}

fn portOutw(port: u16, value: u16) callconv(.c) void {
    io.outw(port, value);
}

fn portInl(port: u16) callconv(.c) u32 {
    return io.inl(port);
}

fn portOutl(port: u16, value: u32) callconv(.c) void {
    io.outl(port, value);
}

fn tickCount() callconv(.c) u64 {
    return timer.tickCount();
}

fn timerFrequency() callconv(.c) u32 {
    return timer.frequency();
}

fn waitTicks(ticks: u64) callconv(.c) void {
    if (ticks == 0) return;
    if (irq_router.inDispatch()) {
        driver_work.noteSleepDeniedFromIrq();
        return;
    }
    driver_work.noteSleepWait(ticks);
    if (scheduler.current() != null) {
        scheduler.sleepTicksWithReason(ticks, "driver-wait");
        return;
    }
    waitBootTicks(ticks);
}

fn waitBootTicks(ticks: u64) void {
    const start = timer.tickCount();
    interrupts.enable();
    while (timer.tickCount() - start < ticks) {
        asm volatile ("pause");
    }
    interrupts.disable();
}

fn allocDmaBuffer(bytes: u32, alignment: u32) callconv(.c) u64 {
    var buffer: DmaBuffer = .{};
    if (allocDmaRegion(bytes, alignment, &buffer) != 0) return 0;
    return buffer.phys_addr;
}

fn freeDmaBuffer(phys_addr: u64, bytes: u32) callconv(.c) void {
    _ = bytes;
    if (phys_addr == 0) return;
    const allocation = takeDmaAllocation(activeOwner(), phys_addr) orelse return;
    const frames = frameCount(allocation.bytes) orelse return;
    phys.freeContiguousFrames(allocation.phys_addr, frames);
}

fn allocDmaRegion(bytes: u32, alignment: u32, out: *DmaBuffer) callconv(.c) i32 {
    return allocDmaRegionConstrained(bytes, alignment, std.math.maxInt(u64), out);
}

fn allocDmaRegionConstrained(bytes: u32, alignment: u32, max_phys_addr: u64, out: *DmaBuffer) callconv(.c) i32 {
    out.* = .{};
    const owner = activeOwner();
    if (owner == 0 or irq_router.inDispatch()) return -6;
    const dma_alignment = if (alignment == 0) @as(u32, @intCast(phys.FRAME_SIZE)) else alignment;
    if (bytes == 0) return -1;
    if (@as(u64, dma_alignment) > phys.FRAME_SIZE) return -2;
    const frames = frameCount(bytes) orelse return -3;
    const phys_addr = phys.allocContiguousFramesBelow(frames, max_phys_addr) orelse return -4;
    const virt_addr = phys.physToVirt(phys_addr);
    const total_bytes = frames * phys.FRAME_SIZE;
    const data: [*]u8 = @ptrFromInt(virt_addr);
    @memset(data[0..@intCast(total_bytes)], 0);
    if (!trackDmaAllocation(owner, phys_addr, @intCast(total_bytes))) {
        phys.freeContiguousFrames(phys_addr, frames);
        return -5;
    }
    out.* = .{
        .phys_addr = phys_addr,
        .virt_addr = virt_addr,
        .bytes = @intCast(total_bytes),
        .alignment = dma_alignment,
        .flags = 0,
        .reserved = 0,
    };
    return 0;
}

fn freeDmaRegion(buffer: *DmaBuffer) callconv(.c) void {
    if (buffer.phys_addr == 0 or buffer.bytes == 0) {
        buffer.* = .{};
        return;
    }
    if (irq_router.inDispatch()) return;
    const allocation = takeDmaAllocation(activeOwner(), buffer.phys_addr) orelse return;
    const frames = frameCount(allocation.bytes) orelse 0;
    if (frames > 0) phys.freeContiguousFrames(allocation.phys_addr, frames);
    buffer.* = .{};
}

fn dmaPinBuffer(virt_addr: u64, bytes: u32, flags: u32, out: *DmaPinnedBuffer) callconv(.c) i32 {
    out.* = .{};
    const owner = activeOwner();
    if (owner == 0 or irq_router.inDispatch()) return -1;
    if (virt_addr == 0 or bytes == 0 or bytes > MAX_R4D_DMA_MAP_BYTES or flags != 0) return -2;
    const end = std.math.add(u64, virt_addr, bytes) catch return -2;
    if (end > std.math.maxInt(u64) - (phys.FRAME_SIZE - 1)) return -2;
    const first_page = alignDown(virt_addr, phys.FRAME_SIZE);
    const last_page_end = alignUp(end, phys.FRAME_SIZE);
    if (last_page_end <= first_page) return -2;
    const page_count_u64 = (last_page_end - first_page) / phys.FRAME_SIZE;
    if (page_count_u64 == 0 or page_count_u64 > std.math.maxInt(u32)) return -2;

    var page_index: u64 = 0;
    while (page_index < page_count_u64) : (page_index += 1) {
        const virt_page = first_page + page_index * phys.FRAME_SIZE;
        _ = dmaPhysicalPage(virt_page) orelse return -3;
    }

    const slot = freeDmaPinSlot() orelse return -5;
    dma_pin_generations[slot] = dma_segments.nextGeneration(dma_pin_generations[slot]);
    const handle = dma_segments.makeHandle(slot, dma_pin_generations[slot]) orelse return -5;
    dma_pins[slot] = .{
        .used = true,
        .generation = dma_pin_generations[slot],
        .owner = owner,
        .virt_addr = virt_addr,
        .bytes = bytes,
        .page_count = @intCast(page_count_u64),
    };
    out.* = .{
        .handle = handle,
        .virt_addr = virt_addr,
        .bytes = bytes,
        .page_count = @intCast(page_count_u64),
        .flags = flags,
    };
    return 0;
}

fn dmaMapPinned(pin: *const DmaPinnedBuffer, constraints: *const DmaConstraints, direction: u32, out: *DmaMapping) callconv(.c) i32 {
    out.* = .{};
    const owner = activeOwner();
    if (owner == 0 or irq_router.inDispatch()) return -1;
    if (!validDmaPinHeader(pin) or !validDmaConstraints(constraints) or !validDmaDirection(direction)) return -2;
    const pin_slot = dma_segments.handleSlot(pin.handle, dma_pins.len) orelse return -3;
    var pinned = &dma_pins[pin_slot];
    if (!pinned.used or pinned.owner != owner or pinned.generation != dma_segments.handleGeneration(pin.handle) or
        pinned.virt_addr != pin.virt_addr or pinned.bytes != pin.bytes)
    {
        return -3;
    }
    if (pinned.map_count == std.math.maxInt(u32)) return -4;
    const mapping_slot = freeDmaMappingSlot() orelse return -5;
    const normalized = dmaSegmentConstraints(constraints.*) orelse return -2;

    var mapped_segments = buildPinnedDmaSegments(pinned, normalized) catch |err| switch (err) {
        error.InvalidArgument, error.Overflow => return -2,
        error.AddressLimit, error.Alignment, error.TooManySegments => blk: {
            if ((constraints.flags & DMA_FLAG_ALLOW_BOUNCE) == 0) return -6;
            break :blk dma_segments.SegmentList{};
        },
    };
    var bounce_phys: u64 = 0;
    var bounce_virt: u64 = 0;
    var bounce_bytes: u32 = 0;
    var mapping_flags: u32 = constraints.flags;
    if (mapped_segments.count == 0) {
        const frames = frameCount(pinned.bytes) orelse return -2;
        bounce_phys = phys.allocContiguousFramesBelow(frames, constraints.dma_mask) orelse return -7;
        const allocated_bytes = frames * phys.FRAME_SIZE;
        if (allocated_bytes > std.math.maxInt(u32)) {
            phys.freeContiguousFrames(bounce_phys, frames);
            return -7;
        }
        bounce_bytes = @intCast(allocated_bytes);
        bounce_virt = phys.physToVirt(bounce_phys);
        mapped_segments = dma_segments.singleRange(bounce_phys, pinned.bytes, normalized) catch {
            phys.freeContiguousFrames(bounce_phys, frames);
            return -7;
        };
        const bounce: [*]u8 = @ptrFromInt(bounce_virt);
        @memset(bounce[0..bounce_bytes], 0);
        mapping_flags |= DMA_MAPPING_FLAG_BOUNCED;
    }

    dma_mapping_generations[mapping_slot] = dma_segments.nextGeneration(dma_mapping_generations[mapping_slot]);
    const handle = dma_segments.makeHandle(mapping_slot, dma_mapping_generations[mapping_slot]) orelse {
        if (bounce_phys != 0) phys.freeContiguousFrames(bounce_phys, frameCount(bounce_bytes) orelse 0);
        return -5;
    };
    dma_mappings[mapping_slot] = .{
        .used = true,
        .generation = dma_mapping_generations[mapping_slot],
        .owner = owner,
        .pin_slot = pin_slot,
        .pin_handle = pin.handle,
        .direction = direction,
        .flags = mapping_flags,
        .original_virt = pinned.virt_addr,
        .requested_bytes = pinned.bytes,
        .bounce_phys = bounce_phys,
        .bounce_virt = bounce_virt,
        .bounce_bytes = bounce_bytes,
        .segments = mapped_segments,
    };
    pinned.map_count += 1;
    if (!syncMappingForDevice(&dma_mappings[mapping_slot])) {
        releaseDmaMapping(mapping_slot, false);
        return -8;
    }
    out.* = mappingDescriptor(handle, &dma_mappings[mapping_slot]);
    return 0;
}

fn dmaSyncForDevice(mapping: *const DmaMapping) callconv(.c) i32 {
    if (irq_router.inDispatch()) return -1;
    const slot = dmaMappingSlotForOwner(mapping.handle, activeOwner()) orelse return -2;
    return if (syncMappingForDevice(&dma_mappings[slot])) 0 else -3;
}

fn dmaSyncForCpu(mapping: *const DmaMapping) callconv(.c) i32 {
    if (irq_router.inDispatch()) return -1;
    const slot = dmaMappingSlotForOwner(mapping.handle, activeOwner()) orelse return -2;
    return if (syncMappingForCpu(&dma_mappings[slot])) 0 else -3;
}

fn dmaUnmap(mapping: *DmaMapping) callconv(.c) i32 {
    if (irq_router.inDispatch()) return -1;
    const slot = dmaMappingSlotForOwner(mapping.handle, activeOwner()) orelse return -2;
    releaseDmaMapping(slot, true);
    mapping.* = .{};
    return 0;
}

fn dmaUnpinBuffer(pin: *DmaPinnedBuffer) callconv(.c) i32 {
    if (irq_router.inDispatch()) return -1;
    const owner = activeOwner();
    const slot = dma_segments.handleSlot(pin.handle, dma_pins.len) orelse return -2;
    const record = &dma_pins[slot];
    if (!record.used or record.owner != owner or record.generation != dma_segments.handleGeneration(pin.handle)) return -2;
    if (record.map_count != 0) return -3;
    record.* = .{};
    pin.* = .{};
    return 0;
}

fn validDmaPinHeader(pin: *const DmaPinnedBuffer) bool {
    return pin.version == DMA_ABI_VERSION and pin.size >= @sizeOf(DmaPinnedBuffer) and pin.handle != 0;
}

fn validDmaConstraints(constraints: *const DmaConstraints) bool {
    const mode = constraints.flags & (DMA_FLAG_COHERENT | DMA_FLAG_STREAMING);
    const known = DMA_FLAG_COHERENT | DMA_FLAG_STREAMING | DMA_FLAG_ALLOW_BOUNCE;
    return constraints.version == DMA_ABI_VERSION and
        constraints.size >= @sizeOf(DmaConstraints) and
        constraints.reserved == 0 and
        constraints.max_segments != 0 and
        constraints.max_segments <= dma_segments.max_segments and
        constraints.alignment != 0 and
        constraints.alignment <= phys.FRAME_SIZE and
        mode != 0 and mode != (DMA_FLAG_COHERENT | DMA_FLAG_STREAMING) and
        (constraints.flags & ~known) == 0;
}

fn validDmaDirection(direction: u32) bool {
    return direction == DMA_DIRECTION_BIDIRECTIONAL or
        direction == DMA_DIRECTION_TO_DEVICE or
        direction == DMA_DIRECTION_FROM_DEVICE;
}

fn dmaSegmentConstraints(constraints: DmaConstraints) ?dma_segments.Constraints {
    if (constraints.boundary != 0 and (constraints.boundary & (constraints.boundary - 1)) != 0) return null;
    if ((constraints.alignment & (constraints.alignment - 1)) != 0) return null;
    return .{
        .dma_mask = constraints.dma_mask,
        .boundary = constraints.boundary,
        .max_segment_bytes = constraints.max_segment_bytes,
        .alignment = constraints.alignment,
        .max_segment_count = constraints.max_segments,
    };
}

fn buildPinnedDmaSegments(
    pinned: *const DmaPinRecord,
    constraints: dma_segments.Constraints,
) dma_segments.Error!dma_segments.SegmentList {
    const end = std.math.add(u64, pinned.virt_addr, pinned.bytes) catch return error.Overflow;
    if (end > std.math.maxInt(u64) - (phys.FRAME_SIZE - 1)) return error.Overflow;
    const first_page = alignDown(pinned.virt_addr, phys.FRAME_SIZE);
    const last_page_end = alignUp(end, phys.FRAME_SIZE);
    const page_count = (last_page_end - first_page) / phys.FRAME_SIZE;
    if (page_count != pinned.page_count) return error.InvalidArgument;

    var result: dma_segments.SegmentList = .{};
    var remaining: u64 = pinned.bytes;
    var page_index: u64 = 0;
    while (page_index < page_count) : (page_index += 1) {
        const virt_page = first_page + page_index * phys.FRAME_SIZE;
        const frame = dmaPhysicalPage(virt_page) orelse return error.InvalidArgument;
        const offset: u64 = if (page_index == 0) pinned.virt_addr - first_page else 0;
        const take: u32 = @intCast(@min(remaining, phys.FRAME_SIZE - offset));
        try dma_segments.appendRange(&result, frame + offset, take, constraints);
        remaining -= take;
    }
    if (remaining != 0) return error.InvalidArgument;
    return result;
}

fn dmaPhysicalPage(virt_page: u64) ?u64 {
    if ((virt_page & (phys.FRAME_SIZE - 1)) != 0) return null;
    const direct_base = phys.physToVirt(0);
    const physical_bytes = std.math.mul(u64, phys.stats().total_frames, phys.FRAME_SIZE) catch return null;
    const direct_end = std.math.add(u64, direct_base, physical_bytes) catch return null;
    if (virt_page >= direct_base and virt_page < direct_end) return virt_page - direct_base;
    if (virt_page >= memory_layout.MMIO_BASE and virt_page < memory_layout.FRAMEBUFFER_END) return null;
    return paging.mappedFrame(virt_page);
}

fn freeDmaPinSlot() ?usize {
    // Iterate the resident tables by reference. Iterating these fixed arrays
    // by value asks Zig to materialize a complete snapshot in the caller's
    // stack frame; with the v19 mapping capacity that can exceed a runtime
    // worker stack before the first DMA mapping is even inspected.
    for (&dma_pins, 0..) |*pin, index| if (!pin.used) return index;
    return null;
}

fn freeDmaMappingSlot() ?usize {
    for (&dma_mappings, 0..) |*mapping, index| if (!mapping.used) return index;
    return null;
}

fn dmaMappingSlotForOwner(handle: u64, owner: u32) ?usize {
    if (owner == 0) return null;
    const slot = dma_segments.handleSlot(handle, dma_mappings.len) orelse return null;
    const mapping = &dma_mappings[slot];
    if (!mapping.used or mapping.owner != owner or mapping.generation != dma_segments.handleGeneration(handle)) return null;
    return slot;
}

fn mappingDescriptor(handle: u64, mapping: *const DmaMappingRecord) DmaMapping {
    var out = DmaMapping{
        .handle = handle,
        .pin_handle = mapping.pin_handle,
        .requested_bytes = mapping.requested_bytes,
        .mapped_bytes = mapping.requested_bytes,
        .direction = mapping.direction,
        .flags = mapping.flags,
        .segment_count = mapping.segments.count,
    };
    @memcpy(out.segments[0..], mapping.segments.segments[0..]);
    return out;
}

fn syncMappingForDevice(mapping: *DmaMappingRecord) bool {
    if (!mapping.used) return false;
    if (mapping.bounce_virt != 0 and
        (mapping.direction == DMA_DIRECTION_TO_DEVICE or mapping.direction == DMA_DIRECTION_BIDIRECTIONAL))
    {
        const source: [*]const u8 = @ptrFromInt(mapping.original_virt);
        const target: [*]u8 = @ptrFromInt(mapping.bounce_virt);
        @memcpy(target[0..mapping.requested_bytes], source[0..mapping.requested_bytes]);
    }
    asm volatile ("mfence" ::: .{ .memory = true });
    mapping.device_owned = true;
    return true;
}

fn syncMappingForCpu(mapping: *DmaMappingRecord) bool {
    if (!mapping.used) return false;
    asm volatile ("mfence" ::: .{ .memory = true });
    if (mapping.bounce_virt != 0 and
        (mapping.direction == DMA_DIRECTION_FROM_DEVICE or mapping.direction == DMA_DIRECTION_BIDIRECTIONAL))
    {
        const source: [*]const u8 = @ptrFromInt(mapping.bounce_virt);
        const target: [*]u8 = @ptrFromInt(mapping.original_virt);
        @memcpy(target[0..mapping.requested_bytes], source[0..mapping.requested_bytes]);
    }
    mapping.device_owned = false;
    return true;
}

fn releaseDmaMapping(slot: usize, copy_back: bool) void {
    if (slot >= dma_mappings.len or !dma_mappings[slot].used) return;
    const mapping = &dma_mappings[slot];
    if (copy_back) _ = syncMappingForCpu(mapping);
    if (mapping.bounce_phys != 0 and mapping.bounce_bytes != 0) {
        const frames = frameCount(mapping.bounce_bytes) orelse 0;
        if (frames != 0) phys.freeContiguousFrames(mapping.bounce_phys, frames);
    }
    if (mapping.pin_slot < dma_pins.len and dma_pins[mapping.pin_slot].used and dma_pins[mapping.pin_slot].map_count != 0) {
        dma_pins[mapping.pin_slot].map_count -= 1;
    }
    mapping.* = .{};
}

fn frameCount(bytes: u32) ?u64 {
    if (bytes == 0) return null;
    const total = alignUp(@as(u64, bytes), phys.FRAME_SIZE);
    return total / phys.FRAME_SIZE;
}

fn requestIrq(irq: u8, handler: *const anyopaque) callconv(.c) i32 {
    _ = irq;
    _ = handler;
    bootlog.puts("[R4D][WARN] request_irq not implemented yet\r\n");
    return -1;
}

fn releaseIrq(irq: u8) callconv(.c) i32 {
    _ = irq;
    bootlog.puts("[R4D][WARN] release_irq not implemented yet\r\n");
    return -1;
}

fn irqRegister(irq: u8, handler: IrqHandler, context: usize, flags: u32) callconv(.c) i32 {
    return irq_router.register(irq, handler, context, flags, activeOwner());
}

fn irqUnregister(irq: u8, handler: IrqHandler, context: usize) callconv(.c) i32 {
    return irq_router.unregister(irq, handler, context);
}

fn irqStats(irq: u8, out: *IrqStats) callconv(.c) i32 {
    return irq_router.stats(irq, out);
}

fn driverWorkSubmit(handler: DriverWorkHandler, context: usize, flags: u32, out_handle: *u32) callconv(.c) i32 {
    return driver_work.submit(activeOwner(), handler, context, flags, out_handle);
}

fn driverWorkSubmitRequest(request: *const DriverWorkRequest, out_handle: *u32) callconv(.c) i32 {
    return driver_work.submitRequest(activeOwner(), request, out_handle);
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
    const irq_owner = irq_router.currentOwner();
    if (irq_owner != 0) return irq_owner;
    const storage_owner = currentStorageCallbackOwner();
    if (storage_owner != 0) return storage_owner;
    const work_owner = driver_work.currentOwner();
    if (work_owner != 0) return work_owner;
    if (current_owner != 0 and current_owner_guard.ownedByCurrent()) return current_owner;
    return 0;
}

fn enterStorageCallback(owner: u32) ?StorageCallbackToken {
    if (owner == 0) return null;
    // Before scheduler admission the boot path is the only execution owner;
    // task ID zero is therefore a unique and safe callback identity.
    const task_id = scheduler.currentId() orelse 0;
    var free_slot: ?usize = null;
    for (&storage_callback_owners, 0..) |*entry, index| {
        if (!entry.used) {
            if (free_slot == null) free_slot = index;
            continue;
        }
        if (entry.task_id != task_id) continue;
        if (entry.owner != owner or entry.depth == std.math.maxInt(u32)) return null;
        entry.depth += 1;
        return .{ .slot = index, .task_id = task_id };
    }
    const slot = free_slot orelse return null;
    storage_callback_owners[slot] = .{
        .used = true,
        .task_id = task_id,
        .owner = owner,
        .depth = 1,
    };
    return .{ .slot = slot, .task_id = task_id };
}

fn leaveStorageCallback(token: StorageCallbackToken) void {
    if (token.slot >= storage_callback_owners.len) return;
    const entry = &storage_callback_owners[token.slot];
    if (!entry.used or entry.task_id != token.task_id or entry.depth == 0) return;
    entry.depth -= 1;
    if (entry.depth == 0) entry.* = .{};
}

fn currentStorageCallbackOwner() u32 {
    const task_id = scheduler.currentId() orelse 0;
    for (storage_callback_owners) |entry| {
        if (entry.used and entry.task_id == task_id and entry.depth != 0) return entry.owner;
    }
    return 0;
}

fn pciDeviceCount() callconv(.c) u32 {
    return @intCast(pci_inventory.count());
}

fn pciDeviceAt(index: u32, out: *PciDeviceInfo) callconv(.c) i32 {
    out.* = .{};
    const dev = pci_inventory.deviceAt(@intCast(index)) orelse return -1;
    out.* = pciInfoFromDevice(dev);
    pci_inventory.noteDetailMaterialization();
    return 0;
}

fn pciFindByClass(class_code: u8, subclass: u8, start_index: u32, out: *PciDeviceInfo) callconv(.c) i32 {
    out.* = .{};
    const index = pci_inventory.findByClass(class_code, subclass, @intCast(start_index)) orelse return -1;
    const dev = pci_inventory.deviceAt(index) orelse return -1;
    out.* = pciInfoFromDevice(dev);
    pci_inventory.noteDetailMaterialization();
    return @intCast(index);
}

// 0.59.19: MSI-Fenster. Router-IRQs 24..31 haben feste IDT-Vektoren 56..63,
// keine IOAPIC-Pins und werden ausschliesslich hier vergeben. GSI-basierte
// INTx-Registrierungen der Treiber nutzen 0..23 und kollidieren nicht.
const MSI_IRQ_BASE: u8 = 24;
const MSI_IRQ_COUNT: u8 = 8;
const IDT_IRQ_VECTOR_BASE: u32 = 32;
var msi_allocations: [MSI_IRQ_COUNT]MsiAllocation = .{MsiAllocation{}} ** MSI_IRQ_COUNT;

fn pciEnableMsi(bus_kind: u8, bus: u8, device: u8, function: u8) callconv(.c) i32 {
    const owner = activeOwner();
    if (owner == 0) return -6;
    for (msi_allocations) |allocation| {
        if (!allocation.used or allocation.bus_kind != bus_kind or allocation.bus != bus or allocation.device != device or allocation.function != function) continue;
        return if (allocation.owner == owner) @intCast(allocation.irq) else -6;
    }
    const status_command = pciReadConfig32(bus_kind, bus, device, function, 0x04);
    if (status_command == 0xFFFF_FFFF or (status_command & (@as(u32, 1) << 20)) == 0) return -1;

    // Begrenzter, zyklussicherer Capability-Walk nach Cap-ID 0x05.
    var offset: u16 = @as(u16, @as(u8, @truncate(pciReadConfig32(bus_kind, bus, device, function, 0x34)))) & 0xFC;
    var cap: u16 = 0;
    var steps: u8 = 0;
    while (offset >= 0x40 and offset <= 0xFC and steps < 48) : (steps += 1) {
        const header = pciReadConfig32(bus_kind, bus, device, function, offset);
        if (header == 0xFFFF_FFFF) return -1;
        if (@as(u8, @truncate(header)) == 0x05) {
            cap = offset;
            break;
        }
        const next = @as(u16, @as(u8, @truncate(header >> 8))) & 0xFC;
        if (next == 0 or next == offset) break;
        offset = next;
    }
    if (cap == 0) return -2;

    var slot: u8 = 0;
    while (slot < MSI_IRQ_COUNT and msi_allocations[slot].used) : (slot += 1) {}
    if (slot >= MSI_IRQ_COUNT) return -3;
    const irq: u8 = MSI_IRQ_BASE + slot;
    const vector: u32 = IDT_IRQ_VECTOR_BASE + irq;

    const cap_header = pciReadConfig32(bus_kind, bus, device, function, cap);
    if (cap_header == 0xFFFF_FFFF) return -1;
    const control: u16 = @truncate(cap_header >> 16);
    if ((control & 1) != 0) return -7;
    const is_64bit = (control & (1 << 7)) != 0;

    // Fixed/Edge to one internal scheduler target; Multiple Message Enable
    // remains one. No public affinity or multi-vector ABI is introduced.
    const msi_target = smp.irqTarget(slot + 1);
    const address: u32 = 0xFEE0_0000 | (msi_target << 12);
    if (pciWriteConfig32(bus_kind, bus, device, function, cap + 0x04, address) != 0) return -4;
    if (is_64bit) {
        if (pciWriteConfig32(bus_kind, bus, device, function, cap + 0x08, 0) != 0) return -4;
        if (pciWriteConfig32(bus_kind, bus, device, function, cap + 0x0C, vector) != 0) return -4;
    } else {
        if (pciWriteConfig32(bus_kind, bus, device, function, cap + 0x08, vector) != 0) return -4;
    }
    const new_control: u32 = (@as(u32, control) & ~@as(u32, 0x0070)) | 0x0001;
    const cap_write = (cap_header & 0x0000_FFFF) | (new_control << 16);
    if (pciWriteConfig32(bus_kind, bus, device, function, cap, cap_write) != 0) {
        _ = restoreMsiHardware(bus_kind, bus, device, function, cap, control, @truncate(status_command));
        return -4;
    }
    const verified = pciReadConfig32(bus_kind, bus, device, function, cap);
    if (verified == 0xFFFF_FFFF or ((verified >> 16) & 0x0001) == 0) {
        _ = restoreMsiHardware(bus_kind, bus, device, function, cap, control, @truncate(status_command));
        return -5;
    }

    // INTx am Endpunkt deaktivieren; der W1C-Statusanteil wird nie geechot.
    const command_raw = pciReadConfig32(bus_kind, bus, device, function, 0x04);
    if (command_raw != 0xFFFF_FFFF) {
        const command = (command_raw & 0x0000_FFFF) | (@as(u32, 1) << 10);
        if (pciWriteConfig32(bus_kind, bus, device, function, 0x04, command) != 0) {
            _ = restoreMsiHardware(bus_kind, bus, device, function, cap, control, @truncate(status_command));
            return -4;
        }
        const command_verify = pciReadConfig32(bus_kind, bus, device, function, 0x04);
        if (command_verify == 0xFFFF_FFFF or (command_verify & (@as(u32, 1) << 10)) == 0) {
            _ = restoreMsiHardware(bus_kind, bus, device, function, cap, control, @truncate(status_command));
            return -5;
        }
    }

    msi_allocations[slot] = .{
        .used = true,
        .owner = owner,
        .bus_kind = bus_kind,
        .bus = bus,
        .device = device,
        .function = function,
        .irq = irq,
        .capability = cap,
        .original_control = control,
        .original_command = @truncate(status_command),
    };
    bootlog.puts("[R4D] MSI enabled irq=");
    bootlog.putDec(irq);
    bootlog.puts(" vector=");
    bootlog.putDec(vector);
    bootlog.puts("\r\n");
    return @intCast(irq);
}

fn pciDisableMsi(bus_kind: u8, bus: u8, device: u8, function: u8) callconv(.c) i32 {
    const owner = activeOwner();
    for (&msi_allocations) |*allocation| {
        if (!allocation.used or allocation.bus_kind != bus_kind or allocation.bus != bus or allocation.device != device or allocation.function != function) continue;
        if (owner != 0 and allocation.owner != owner) return -2;
        if (!restoreMsiAllocation(allocation)) return -3;
        allocation.* = .{};
        return 0;
    }
    return 0;
}

fn restoreMsiAllocation(allocation: *const MsiAllocation) bool {
    return restoreMsiHardware(
        allocation.bus_kind,
        allocation.bus,
        allocation.device,
        allocation.function,
        allocation.capability,
        allocation.original_control,
        allocation.original_command,
    );
}

fn restoreMsiHardware(bus_kind: u8, bus: u8, device: u8, function: u8, capability: u16, original_control: u16, original_command: u16) bool {
    const current_header = pciReadConfig32(bus_kind, bus, device, function, capability);
    if (current_header == 0xFFFF_FFFF) return false;
    const disabled_control = @as(u16, @truncate(current_header >> 16)) & ~@as(u16, 1);
    const disabled_header = (current_header & 0x0000_FFFF) | (@as(u32, disabled_control) << 16);
    if (pciWriteConfig32(bus_kind, bus, device, function, capability, disabled_header) != 0) return false;
    const disabled_verify = pciReadConfig32(bus_kind, bus, device, function, capability);
    if (disabled_verify == 0xFFFF_FFFF or ((disabled_verify >> 16) & 1) != 0) return false;

    const original_header = (disabled_verify & 0x0000_FFFF) | (@as(u32, original_control) << 16);
    if (pciWriteConfig32(bus_kind, bus, device, function, capability, original_header) != 0) return false;
    const current_command = pciReadConfig32(bus_kind, bus, device, function, 0x04);
    if (current_command == 0xFFFF_FFFF) return false;
    const intx_mask: u32 = @as(u32, 1) << 10;
    const restored_command = (current_command & 0x0000_FFFF & ~intx_mask) | (@as(u32, original_command) & intx_mask);
    if (pciWriteConfig32(bus_kind, bus, device, function, 0x04, restored_command) != 0) return false;
    const final_header = pciReadConfig32(bus_kind, bus, device, function, capability);
    const final_command = pciReadConfig32(bus_kind, bus, device, function, 0x04);
    return final_header != 0xFFFF_FFFF and
        ((final_header >> 16) & 1) == 0 and
        final_command != 0xFFFF_FFFF and
        (final_command & intx_mask) == (@as(u32, original_command) & intx_mask);
}

fn cleanupMsiOwner(owner: u32) MsiOwnerCleanupResult {
    var result: MsiOwnerCleanupResult = .{};
    for (&msi_allocations) |*allocation| {
        if (!allocation.used or allocation.owner != owner) continue;
        if (!restoreMsiAllocation(allocation)) {
            result.failed = true;
            continue;
        }
        allocation.* = .{};
        result.removed += 1;
    }
    return result;
}

fn pciReadConfig32(bus_kind: u8, bus: u8, device: u8, function: u8, offset: u16) callconv(.c) u32 {
    return pci_inventory.readConfig32At(bus_kind, bus, device, function, offset);
}

fn pciWriteConfig32(bus_kind: u8, bus: u8, device: u8, function: u8, offset: u16, value: u32) callconv(.c) i32 {
    return if (pci_inventory.writeConfig32At(bus_kind, bus, device, function, offset, value)) 0 else -1;
}

fn pciReadBar(bus_kind: u8, bus: u8, device: u8, function: u8, index: u8) callconv(.c) u32 {
    if (index >= 6) return 0;
    if (bus_kind != NET_BUS_PCIE and bus_kind != NET_BUS_PCI) return 0;
    return pci_inventory.readBar(.{ .bus_kind = bus_kind, .bus = bus, .device = device, .function = function }, index);
}

fn pciEnableBusMaster(bus_kind: u8, bus: u8, device: u8, function: u8, flags: u32) callconv(.c) i32 {
    const raw = pciReadConfig32(bus_kind, bus, device, function, 0x04);
    if (raw == 0xFFFF_FFFF) return -1;
    var command: u16 = @truncate(raw & 0xFFFF);
    if ((flags & 1) != 0) command |= 0x0001;
    if ((flags & 2) != 0) command |= 0x0002;
    command |= 0x0004;
    command &= ~@as(u16, 0x0400);
    // PCI Status occupies the upper half of this dword and contains W1C
    // fields.  Never echo a status snapshot while changing Command.  The
    // readback is also the ordering boundary required before a freshly
    // enabled device is allowed to fetch DMA descriptors.
    if (pciWriteConfig32(bus_kind, bus, device, function, 0x04, @as(u32, command)) != 0) return -2;
    const verified = pciReadConfig32(bus_kind, bus, device, function, 0x04);
    if (verified == 0xFFFF_FFFF) return -3;
    var required: u16 = 0x0004;
    if ((flags & 1) != 0) required |= 0x0001;
    if ((flags & 2) != 0) required |= 0x0002;
    const verified_command: u16 = @truncate(verified);
    if ((verified_command & required) != required or (verified_command & 0x0400) != 0) return -4;
    return 0;
}

fn pciMapBar(bus_kind: u8, bus: u8, device: u8, function: u8, index: u8, bytes: u32, flags: u32, out: *MmioRegion) callconv(.c) i32 {
    out.* = .{};
    if (index >= 6) return -1;
    const raw = pciReadBar(bus_kind, bus, device, function, index);
    if (raw == 0 or raw == 0xFFFF_FFFF) return -2;
    if ((raw & 1) != 0) return -3;

    const full_bar = pciReadBar64(bus_kind, bus, device, function, index);
    const base = full_bar & 0xFFFF_FFFF_FFFF_FFF0;
    if (base == 0) return -4;

    const requested: u64 = if (bytes == 0) paging.PAGE_SIZE else @as(u64, bytes);
    if (requested > MAX_MMIO_MAP_BYTES) return -5;

    const page = alignDown(base, paging.PAGE_SIZE);
    const offset = base - page;
    const total = alignUp(offset + requested, paging.PAGE_SIZE);
    var mapped: u64 = 0;
    while (mapped < total) : (mapped += paging.PAGE_SIZE) {
        const phys_page = page + mapped;
        const virt_page = phys.physToVirt(phys_page);
        if (!paging.isMapped(virt_page)) {
            if (!paging.mapPage(virt_page, phys_page, paging.WRITABLE | paging.CACHE_DISABLE | paging.NO_EXECUTE)) return -6;
        }
    }

    const virt_addr = phys.physToVirt(base);
    if ((flags & MMIO_MAP_WRITE_COMBINING) != 0) {
        _ = paging.setWriteCombiningRange(virt_addr, requested);
    }

    out.* = .{
        .phys_addr = base,
        .virt_addr = virt_addr,
        .bytes = @intCast(requested),
        .mapped_bytes = @intCast(total),
        .bar_index = index,
        .flags = @truncate(flags),
        .reserved = 0,
    };
    return 0;
}

fn pciReadBar64(bus_kind: u8, bus: u8, device: u8, function: u8, index: u8) u64 {
    if (index >= 6) return 0;
    if (bus_kind != NET_BUS_PCIE and bus_kind != NET_BUS_PCI) return 0;
    return pci_inventory.readBar64(.{ .bus_kind = bus_kind, .bus = bus, .device = device, .function = function }, index);
}

fn getOption(driver: [*:0]const u8, key: [*:0]const u8) callconv(.c) [*:0]const u8 {
    const config = boot_config.get();
    var i: usize = 0;
    while (i < config.option_count) : (i += 1) {
        const opt = &config.options[i];
        if (!zEqSlice(driver, opt.driver[0..opt.driver_len])) continue;
        if (!zEqSlice(key, opt.key[0..opt.key_len])) continue;
        copyOptionValue(opt.value[0..opt.value_len]);
        return &option_value_z;
    }
    return &empty_z;
}

fn registerAudioBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    return audio.registerAudioBackendZ(name, backend);
}

fn registerAudioOutputBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    const descriptor: *const AudioBackendDescriptor = @ptrCast(@alignCast(backend));
    if (!validAudioBackend(descriptor)) return -1;
    const slot_index = freeR4DAudioBackendSlot() orelse {
        bootlog.puts("[R4D][ERROR] audio backend table full\r\n");
        return -2;
    };
    const slot = &r4d_audio_backends[slot_index];
    slot.* = .{
        .used = true,
        .owner = current_owner,
        .descriptor = descriptor,
    };
    copyZName(name, slot.name[0..], &slot.name_len);

    const result = audio.registerExternalAudioBackendZ(name, .{
        .formats = descriptor.formats,
        .min_rate = descriptor.min_rate,
        .max_rate = descriptor.max_rate,
        .max_channels = descriptor.max_channels,
    }, descriptor.context, descriptor.write_pcm.?, descriptor.stop, descriptor.status);
    if (result != 0) {
        slot.* = R4DAudioBackend{};
        return -3;
    }

    bootlog.puts("[R4D] register audio backend ");
    bootlog.puts(slot.name[0..slot.name_len]);
    bootlog.puts("\r\n");
    return 0;
}

fn unregisterAudioBackend(name: [*:0]const u8) callconv(.c) i32 {
    const result = audio.unregisterAudioBackendZ(name);
    if (result != 0) return result;
    var index: usize = 0;
    while (index < r4d_audio_backends.len) : (index += 1) {
        const slot = &r4d_audio_backends[index];
        if (!slot.used or !zEqSlice(name, slot.name[0..slot.name_len])) continue;
        slot.* = R4DAudioBackend{};
        return 0;
    }
    return 0;
}

fn registerStorageBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    const descriptor: *const StorageBackendDescriptor = @ptrCast(@alignCast(backend));
    if (!validStorageBackend(descriptor)) return -1;
    const async_submit = storageBackendSubmit(descriptor);
    if (storage.findByName(zSlice(name)) != null) {
        bootlog.puts("[R4D][ERROR] storage backend duplicate name ");
        putZ(name);
        bootlog.puts("\r\n");
        return -2;
    }
    const slot_index = freeR4DStorageBackendSlot() orelse {
        bootlog.puts("[R4D][ERROR] storage backend table full\r\n");
        return -3;
    };
    const slot = &r4d_storage_backends[slot_index];
    slot.* = .{
        .used = true,
        .owner = current_owner,
        .descriptor = descriptor,
    };
    copyZName(name, slot.name[0..], &slot.name_len);

    const block_index = storage.register(.{
        .name = slot.name[0..slot.name_len],
        .driver = "R4D",
        .bus = storageBus(descriptor.bus),
        .controller = storageController(descriptor),
        .port = 0,
        .sector_size = descriptor.sector_size,
        .sector_count = descriptor.sector_count,
        .max_sectors_per_request = descriptor.max_sectors_per_request,
        .queue_depth = if (async_submit != null) descriptor.queue_depth else 1,
        .timeout_ticks = descriptor.timeout_ticks,
        .removable = (descriptor.flags & STORAGE_BACKEND_FLAG_REMOVABLE) != 0,
        .writable = (descriptor.flags & STORAGE_BACKEND_FLAG_WRITABLE) != 0,
        .source = storageSource(descriptor.source),
        .owner_id = current_owner,
        .ctx = slot,
        .read_fn = r4dStorageRead,
        .write_fn = if (descriptor.write != null) r4dStorageWrite else null,
        .flush_fn = if (descriptor.flush != null) r4dStorageFlush else null,
        .async_submit_fn = if (async_submit != null) r4dStorageSubmit else null,
        .async_cancel_fn = if (storageBackendCancel(descriptor) != null) r4dStorageCancel else null,
        .async_reset_fn = if (storageBackendReset(descriptor) != null) r4dStorageReset else null,
    }) orelse {
        slot.* = R4DStorageBackend{};
        bootlog.puts("[R4D][ERROR] storage block table full\r\n");
        return -4;
    };

    slot.block_index = block_index;
    bootlog.puts("[R4D] register storage backend ");
    bootlog.puts(slot.name[0..slot.name_len]);
    bootlog.puts(" block=");
    bootlog.putDec(block_index);
    bootlog.puts(" source=");
    bootlog.puts(storage.sourceLabel(storageSource(descriptor.source)));
    bootlog.puts("\r\n");
    return @intCast(block_index);
}

fn unregisterStorageBackend(name: [*:0]const u8) callconv(.c) i32 {
    const backend = findR4DStorageBackendByName(name) orelse return -1;
    const removed = storage.unregister(backend.block_index);
    if (!removed) return -2;
    if (backend.descriptor.shutdown) |shutdown| _ = shutdown(backend.descriptor.context);
    backend.* = R4DStorageBackend{};
    return 0;
}

fn storageBackendRecoveryBegin(name: [*:0]const u8) callconv(.c) i32 {
    const backend = findR4DStorageBackendByName(name) orelse return -1;
    storage.beginBackendRecovery(backend.block_index);
    return 0;
}

fn storageBackendRecoveryFinish(name: [*:0]const u8, ok: i32) callconv(.c) i32 {
    const backend = findR4DStorageBackendByName(name) orelse return -1;
    storage.finishBackendRecovery(backend.block_index, ok != 0);
    return 0;
}

fn registerUsbHostController(name: [*:0]const u8, descriptor: *const UsbHostControllerDescriptor) callconv(.c) i32 {
    if (!validUsbHostController(descriptor)) return -1;
    if (usb_host.findByName(zSlice(name)) != null) {
        bootlog.puts("[R4D][ERROR] usb host duplicate name ");
        putZ(name);
        bootlog.puts("\r\n");
        return -2;
    }
    const index = usb_host.register(zSlice(name), descriptor, current_owner) orelse return -3;
    return @intCast(index);
}

fn unregisterUsbHostController(name: [*:0]const u8) callconv(.c) i32 {
    return if (usb_host.unregisterByName(zSlice(name), activeOwner())) 0 else -1;
}

fn activateUsbHostController(name: [*:0]const u8, source: u32) callconv(.c) i32 {
    if (!zEqSlice(name, "XHCI")) return -1;
    if (source != USB_HOST_SOURCE_PRELOAD and source != USB_HOST_SOURCE_DISK) return -2;
    return xhci.activate(activeOwner(), source);
}

fn registerDisplayBlitBackend(name: [*:0]const u8, descriptor: *const display_blit.Descriptor) callconv(.c) i32 {
    const owner = activeOwner();
    if (owner == 0 or @intFromPtr(descriptor) == 0) return -1;
    const result = display_blit.register(owner, zSlice(name), descriptor);
    if (result == 0) {
        bootlog.puts("[R4D] register display blit backend ");
        putZ(name);
        bootlog.puts("\r\n");
    }
    return result;
}

fn unregisterDisplayBlitBackend(name: [*:0]const u8) callconv(.c) i32 {
    return display_blit.unregister(activeOwner(), zSlice(name));
}

fn registerInputBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    _ = backend;
    return logRegister("input", name);
}

fn registerSynthEngine(name: [*:0]const u8, engine: *const anyopaque) callconv(.c) i32 {
    return audio.registerSynthEngineZ(name, engine);
}

fn registerSynthEngineV2(name: [*:0]const u8, engine: *const SynthEngineDescriptor) callconv(.c) i32 {
    if (!validSynthEngine(engine)) return -1;
    return audio.registerExternalSynthEngineZ(name, engine.flags, engine.context, engine.midi_send, engine.render, engine.stop, engine.status, engine.opl3_reset, engine.opl3_write_register, engine.sid_acquire, engine.sid_release, engine.sid_set_model, engine.sid_write_register, engine.sid_load_data, engine.sid_init, engine.sid_play_frame, engine.sid_render_pcm, synthRenderPcm(engine));
}

fn protocolDispatch(role: [*:0]const u8, op: u32, in_buffer: *const protocol_api.ProtocolBuffer, out_buffer: *protocol_api.ProtocolBuffer) callconv(.c) i32 {
    return r4p.dispatch(zSlice(role), op, in_buffer, out_buffer);
}

fn registerMixerBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    return audio.registerMixerBackendZ(name, backend);
}

fn registerNetBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    const descriptor: *const NetBackendDescriptor = @ptrCast(@alignCast(backend));
    if (!validNetBackend(descriptor)) return -1;
    const negotiation = net_backend.negotiate(netBackendOffer(descriptor)) catch |err| {
        bootlog.puts("[R4D][ERROR] net backend capability negotiation failed: ");
        bootlog.puts(switch (err) {
            error.InvalidOffer => "invalid-offer",
            error.RequiredNotOffered => "required-not-offered",
            error.RequiredUnsupported => "required-unsupported",
        });
        bootlog.puts("\r\n");
        return -4;
    };
    const slot_index = freeR4DNetBackendSlot() orelse {
        bootlog.puts("[R4D][ERROR] net backend table full\r\n");
        return -2;
    };
    const slot = &r4d_net_backends[slot_index];
    slot.* = .{
        .used = true,
        .owner = current_owner,
        .descriptor = descriptor,
        .negotiation = negotiation,
    };
    copyZName(name, slot.name[0..], &slot.name_len);

    const flags = net.ADAPTER_FLAG_TRUSTED_BACKEND |
        (if ((descriptor.flags & NET_BACKEND_FLAG_BROADCAST) != 0) net.ADAPTER_FLAG_BROADCAST else 0);
    const adapter_index = net.register(.{
        .name = slot.name[0..slot.name_len],
        .driver = "R4D",
        .bus = netBus(descriptor.bus_kind),
        .bus_no = descriptor.bus,
        .device_no = descriptor.device,
        .function_no = descriptor.function,
        .vendor_id = descriptor.vendor_id,
        .device_id = descriptor.device_id,
        .mac = descriptor.mac,
        .mtu = descriptor.mtu,
        .flags = flags,
        .backend = negotiation,
        .link = if ((descriptor.flags & NET_BACKEND_FLAG_LINK_UP) != 0) .up else .unknown,
        .ops = .{
            .transmit = if (descriptor.transmit != null) r4dNetTransmit else null,
            .poll = if (descriptor.poll != null) r4dNetPoll else null,
            .status = if (descriptor.status != null) r4dNetStatus else null,
        },
    }) orelse {
        slot.* = R4DNetBackend{};
        bootlog.puts("[R4D][ERROR] net core adapter table full\r\n");
        return -3;
    };

    slot.adapter_index = adapter_index;
    bootlog.puts("[R4D] register net backend ");
    bootlog.puts(slot.name[0..slot.name_len]);
    bootlog.puts(" adapter=");
    bootlog.putDec(adapter_index);
    bootlog.puts(" caps=");
    bootlog.putHex(negotiation.accepted, 16);
    bootlog.puts("/");
    bootlog.putHex(negotiation.offered, 16);
    bootlog.puts(" queues=");
    bootlog.putDec(negotiation.rx_queue_count);
    bootlog.puts("/");
    bootlog.putDec(negotiation.tx_queue_count);
    bootlog.puts("\r\n");
    return @intCast(adapter_index);
}

fn validAudioBackend(descriptor: *const AudioBackendDescriptor) bool {
    if (descriptor.version != AUDIO_BACKEND_VERSION) {
        bootlog.puts("[R4D][ERROR] audio backend version mismatch\r\n");
        return false;
    }
    if (descriptor.size < @sizeOf(AudioBackendDescriptor)) {
        bootlog.puts("[R4D][ERROR] audio backend descriptor too small\r\n");
        return false;
    }
    if (descriptor.write_pcm == null) {
        bootlog.puts("[R4D][ERROR] audio backend missing write_pcm\r\n");
        return false;
    }
    if ((descriptor.formats & (AUDIO_BACKEND_FORMAT_S16LE | AUDIO_BACKEND_FORMAT_U8)) == 0) {
        bootlog.puts("[R4D][ERROR] audio backend unsupported format mask\r\n");
        return false;
    }
    if (descriptor.max_channels == 0 or descriptor.max_channels > 8) {
        bootlog.puts("[R4D][ERROR] audio backend invalid channel count\r\n");
        return false;
    }
    if (descriptor.min_rate != 0 and descriptor.max_rate != 0 and descriptor.min_rate > descriptor.max_rate) {
        bootlog.puts("[R4D][ERROR] audio backend invalid rate range\r\n");
        return false;
    }
    return true;
}

fn validSynthEngine(descriptor: *const SynthEngineDescriptor) bool {
    if (descriptor.version != SYNTH_ENGINE_VERSION) {
        bootlog.puts("[R4D][ERROR] synth engine version mismatch\r\n");
        return false;
    }
    if (descriptor.size < SYNTH_ENGINE_V1_SIZE) {
        bootlog.puts("[R4D][ERROR] synth engine descriptor too small\r\n");
        return false;
    }
    if (descriptor.midi_send == null and descriptor.render == null and descriptor.stop == null and descriptor.status == null and
        descriptor.opl3_reset == null and descriptor.opl3_write_register == null and descriptor.sid_acquire == null and
        descriptor.sid_release == null and descriptor.sid_set_model == null and descriptor.sid_write_register == null and descriptor.sid_load_data == null and
        descriptor.sid_init == null and descriptor.sid_play_frame == null and descriptor.sid_render_pcm == null and synthRenderPcm(descriptor) == null)
    {
        bootlog.puts("[R4D][ERROR] synth engine has no operations\r\n");
        return false;
    }
    return true;
}

fn synthRenderPcm(descriptor: *const SynthEngineDescriptor) ?audio.SynthRenderPcmCtxFn {
    const required_size = @offsetOf(SynthEngineDescriptor, "render_pcm") + @sizeOf(?audio.SynthRenderPcmCtxFn);
    if (descriptor.size < required_size) return null;
    return descriptor.render_pcm;
}

fn validStorageBackend(descriptor: *const StorageBackendDescriptor) bool {
    if (descriptor.version == 0 or descriptor.version > STORAGE_BACKEND_VERSION) {
        bootlog.puts("[R4D][ERROR] storage backend version mismatch\r\n");
        return false;
    }
    if (descriptor.size < STORAGE_BACKEND_V1_SIZE) {
        bootlog.puts("[R4D][ERROR] storage backend descriptor too small\r\n");
        return false;
    }
    const async_submit = storageBackendSubmit(descriptor);
    if (async_submit == null and descriptor.read == null) {
        bootlog.puts("[R4D][ERROR] storage backend missing read callback\r\n");
        return false;
    }
    if (async_submit == null and (descriptor.flags & STORAGE_BACKEND_FLAG_WRITABLE) != 0 and descriptor.write == null) {
        bootlog.puts("[R4D][ERROR] storage backend writable without write callback\r\n");
        return false;
    }
    if (async_submit == null and (storageBackendCancel(descriptor) != null or storageBackendReset(descriptor) != null)) {
        bootlog.puts("[R4D][ERROR] storage backend async controls without submit\r\n");
        return false;
    }
    if (descriptor.sector_count == 0) {
        bootlog.puts("[R4D][ERROR] storage backend empty device\r\n");
        return false;
    }
    if (!validSectorSize(descriptor.sector_size)) {
        bootlog.puts("[R4D][ERROR] storage backend invalid sector size\r\n");
        return false;
    }
    if (descriptor.queue_depth == 0) {
        bootlog.puts("[R4D][ERROR] storage backend invalid queue depth\r\n");
        return false;
    }
    if (@as(usize, descriptor.queue_depth) > storage.MAX_REQUEST_QUEUE_DEPTH) {
        bootlog.puts("[R4D][ERROR] storage backend queue depth exceeds block queue\r\n");
        return false;
    }
    if (descriptor.source != STORAGE_SOURCE_BUILTIN and descriptor.source != STORAGE_SOURCE_PRELOAD and descriptor.source != STORAGE_SOURCE_DISK) {
        bootlog.puts("[R4D][ERROR] storage backend invalid source\r\n");
        return false;
    }
    return true;
}

fn storageBackendSubmit(descriptor: *const StorageBackendDescriptor) ?StorageSubmitFn {
    const required: u32 = @intCast(@offsetOf(StorageBackendDescriptor, "submit") + @sizeOf(?StorageSubmitFn));
    if (descriptor.version < 2 or descriptor.size < required) return null;
    return descriptor.submit;
}

fn storageBackendCancel(descriptor: *const StorageBackendDescriptor) ?StorageCancelFn {
    const required: u32 = @intCast(@offsetOf(StorageBackendDescriptor, "cancel") + @sizeOf(?StorageCancelFn));
    if (descriptor.version < 2 or descriptor.size < required) return null;
    return descriptor.cancel;
}

fn storageBackendReset(descriptor: *const StorageBackendDescriptor) ?StorageResetFn {
    const required: u32 = @intCast(@offsetOf(StorageBackendDescriptor, "reset") + @sizeOf(?StorageResetFn));
    if (descriptor.version < 2 or descriptor.size < required) return null;
    return descriptor.reset;
}

fn validUsbHostController(descriptor: *const UsbHostControllerDescriptor) bool {
    if (descriptor.version != USB_HOST_BACKEND_VERSION) {
        bootlog.puts("[R4D][ERROR] usb host version mismatch\r\n");
        return false;
    }
    if (descriptor.size < @sizeOf(UsbHostControllerDescriptor)) {
        bootlog.puts("[R4D][ERROR] usb host descriptor too small\r\n");
        return false;
    }
    if (descriptor.source != USB_HOST_SOURCE_BUILTIN and descriptor.source != USB_HOST_SOURCE_PRELOAD and descriptor.source != USB_HOST_SOURCE_DISK) {
        bootlog.puts("[R4D][ERROR] usb host invalid source\r\n");
        return false;
    }
    if (descriptor.port_scan == null) {
        bootlog.puts("[R4D][ERROR] usb host missing port_scan\r\n");
        return false;
    }
    if (descriptor.control_transfer == null) {
        bootlog.puts("[R4D][ERROR] usb host missing control_transfer\r\n");
        return false;
    }
    const known_flags = usb_host.FLAG_PORT_SCAN | usb_host.FLAG_CONTROL |
        usb_host.FLAG_BULK | usb_host.FLAG_INTERRUPT | usb_host.FLAG_EVENT_IRQ |
        usb_host.FLAG_POLL_FALLBACK | usb_host.FLAG_MULTI_TRANSFER | usb_host.FLAG_HOTPLUG;
    if ((descriptor.flags & ~known_flags) != 0) {
        bootlog.puts("[R4D][ERROR] usb host unknown capability flags\r\n");
        return false;
    }
    if ((descriptor.flags & usb_host.FLAG_PORT_SCAN) == 0 or
        (descriptor.flags & usb_host.FLAG_CONTROL) == 0 or
        (descriptor.flags & usb_host.FLAG_BULK) == 0 or
        (descriptor.flags & usb_host.FLAG_INTERRUPT) == 0 or
        (descriptor.flags & usb_host.FLAG_POLL_FALLBACK) == 0 or
        descriptor.address_device == null or
        descriptor.configure_device == null or
        descriptor.bulk_transfer == null or
        descriptor.interrupt_transfer == null or
        descriptor.reset_port == null or
        descriptor.clear_halt == null or
        descriptor.reset_endpoint == null or
        descriptor.poll == null or
        descriptor.shutdown == null or
        descriptor.status == null)
    {
        bootlog.puts("[R4D][ERROR] usb host v2 capability/callback mismatch\r\n");
        return false;
    }
    return true;
}

fn validNetBackend(descriptor: *const NetBackendDescriptor) bool {
    if (descriptor.version != 1 and descriptor.version != NET_BACKEND_VERSION) {
        bootlog.puts("[R4D][ERROR] net backend version mismatch\r\n");
        return false;
    }
    const required_size: u32 = if (descriptor.version == 1) NET_BACKEND_V1_SIZE else @sizeOf(NetBackendDescriptor);
    if (descriptor.size < required_size) {
        bootlog.puts("[R4D][ERROR] net backend descriptor too small\r\n");
        return false;
    }
    if (descriptor.mtu < 576 or descriptor.mtu > 1500) {
        bootlog.puts("[R4D][ERROR] net backend invalid mtu\r\n");
        return false;
    }
    if (macIsZero(descriptor.mac)) {
        bootlog.puts("[R4D][ERROR] net backend missing mac\r\n");
        return false;
    }
    // 0.56.9: Funktionszeiger des Deskriptors muessen im Kernel-Space
    // liegen. Ein unrelozierter Zeiger (Modul-LINK-Base 0x4_00000000
    // oder null-nahe Werte) wuerde spaeter aus dem Netz-/IRQ-Pfad ins
    // Leere gerufen (rip=0-Crashklasse) - hier laut abweisen.
    var fnptr_ok = true;
    if (descriptor.transmit) |p| fnptr_ok = fnptr_ok and netFnPlausible(@intFromPtr(p));
    if (descriptor.poll) |p| fnptr_ok = fnptr_ok and netFnPlausible(@intFromPtr(p));
    if (descriptor.status) |p| fnptr_ok = fnptr_ok and netFnPlausible(@intFromPtr(p));
    if (descriptor.shutdown) |p| fnptr_ok = fnptr_ok and netFnPlausible(@intFromPtr(p));
    if (netBackendTransmitPacket(descriptor)) |p| fnptr_ok = fnptr_ok and netFnPlausible(@intFromPtr(p));
    if (!fnptr_ok) {
        bootlog.puts("[R4D][ERROR] NETBACKEND BAD FNPTR (unrelocated?)\r\n");
        return false;
    }
    return true;
}

fn netBackendOffer(descriptor: *const NetBackendDescriptor) net_backend.Offer {
    if (descriptor.version < 2 or descriptor.size < @sizeOf(NetBackendDescriptor)) return .{};
    return .{
        .offered = descriptor.offered_capabilities,
        .required = descriptor.required_capabilities,
        .rx_queue_count = descriptor.rx_queue_count,
        .tx_queue_count = descriptor.tx_queue_count,
        .max_rx_segments = descriptor.max_rx_segments,
        .max_tx_segments = descriptor.max_tx_segments,
        .rx_ownership = descriptor.rx_ownership,
        .tx_ownership = descriptor.tx_ownership,
        .interrupt_moderation_us = descriptor.interrupt_moderation_us,
    };
}

fn netBackendTransmitPacket(descriptor: *const NetBackendDescriptor) ?NetTransmitPacketFn {
    if (descriptor.version < 2 or descriptor.size < @sizeOf(NetBackendDescriptor)) return null;
    return descriptor.transmit_packet;
}

fn netFnPlausible(ptr: u64) bool {
    return ptr >= 0xFFFF_8000_0000_0000;
}

fn r4dNetTransmit(adapter_index: usize, frame: []const u8) net.TxResult {
    const backend = findR4DNetBackend(adapter_index) orelse return .backend_error;
    const tx = backend.descriptor.transmit orelse return .unsupported;
    const result = tx(backend.descriptor.context, frame.ptr, @intCast(frame.len));
    return switch (result) {
        0 => .ok,
        1 => .busy,
        2 => .too_large,
        3 => .link_down,
        4 => .unsupported,
        else => .backend_error,
    };
}

fn r4dNetPoll(adapter_index: usize) void {
    const backend = findR4DNetBackend(adapter_index) orelse return;
    if (backend.descriptor.poll) |poll| poll(backend.descriptor.context);
}

fn r4dNetStatus(adapter_index: usize, out: *net.BackendStatus) i32 {
    const backend = findR4DNetBackend(adapter_index) orelse return -1;
    const status = backend.descriptor.status orelse return -2;
    return status(backend.descriptor.context, out);
}

fn netReceiveFrame(adapter_index: i32, frame: [*]const u8, len: u32) callconv(.c) i32 {
    if (adapter_index < 0) return -1;
    if (len == 0 or len > net.MAX_PACKET_SIZE) return -2;
    const index: usize = @intCast(adapter_index);
    const slice = frame[0..@intCast(len)];
    return switch (net.receiveFrame(index, slice)) {
        .accepted => 0,
        .accepted_fallback => 0,
        .invalid_adapter => -1,
        .invalid_frame => -2,
        .unavailable => -3,
        .queue_busy => -4,
        .irq_context => -5,
    };
}

fn netScheduleRx(adapter_index: i32) callconv(.c) i32 {
    if (adapter_index < 0) return -1;
    const index: usize = @intCast(adapter_index);
    if (index >= net.count()) return -1;
    return if (net.scheduleRxWork(index)) 0 else -3;
}

fn netBackendQuery(adapter_index: i32, out: *NetBackendNegotiation) callconv(.c) i32 {
    if (adapter_index < 0) return -1;
    if (!net_backend.validNegotiationQuery(out)) return -2;
    const backend = findR4DNetBackend(@intCast(adapter_index)) orelse return -1;
    out.* = backend.negotiation;
    return 0;
}

fn netReceivePacket(adapter_index: i32, packet: *const NetPacket) callconv(.c) i32 {
    if (adapter_index < 0) return -1;
    const index: usize = @intCast(adapter_index);
    const backend = findR4DNetBackend(index) orelse return -1;
    if (!net_backend.validRxPacket(packet, backend.negotiation, net.MAX_PACKET_SIZE)) return -2;
    const frame_ptr: [*]const u8 = @ptrFromInt(packet.fallback_addr);
    const frame = frame_ptr[0..@intCast(packet.fallback_bytes)];
    return switch (net.receivePacket(index, packet.queue_index, packet.flags, frame)) {
        .accepted => 0,
        .accepted_fallback => 1,
        .invalid_adapter => -1,
        .invalid_frame => -2,
        .unavailable => -3,
        .queue_busy => -4,
        .irq_context => -5,
    };
}

fn r4dStorageRead(ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool {
    const raw = ctx orelse return false;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const read = backend.descriptor.read orelse return false;
    const callback = enterStorageCallback(backend.owner) orelse return false;
    defer leaveStorageCallback(callback);
    return read(backend.descriptor.context, lba, sectors, out.ptr, @intCast(out.len)) == 0;
}

fn r4dStorageWrite(ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool {
    const raw = ctx orelse return false;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const write = backend.descriptor.write orelse return false;
    const callback = enterStorageCallback(backend.owner) orelse return false;
    defer leaveStorageCallback(callback);
    return write(backend.descriptor.context, lba, sectors, data.ptr, @intCast(data.len)) == 0;
}

fn r4dStorageFlush(ctx: ?*anyopaque) bool {
    const raw = ctx orelse return false;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const flush = backend.descriptor.flush orelse return true;
    const callback = enterStorageCallback(backend.owner) orelse return false;
    defer leaveStorageCallback(callback);
    return flush(backend.descriptor.context) == 0;
}

fn r4dStorageSubmit(ctx: ?*anyopaque, request: *const storage.AsyncRequest) i32 {
    const raw = ctx orelse return -1;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const submit = storageBackendSubmit(backend.descriptor) orelse return -1;
    const buffer_addr: u64 = switch (request.kind) {
        .read => if (request.buffer) |ptr| @intFromPtr(ptr) else 0,
        .write => if (request.const_buffer) |ptr| @intFromPtr(ptr) else 0,
        .flush, .none => 0,
    };
    const public_request = StorageRequest{
        .handle = request.handle,
        .operation = @intFromEnum(request.kind),
        .lba = request.lba,
        .sectors = request.sectors,
        .buffer_bytes = @intCast(request.buffer_len),
        .buffer_addr = buffer_addr,
        .complete = request.complete,
    };
    const callback = enterStorageCallback(backend.owner) orelse return -1;
    defer leaveStorageCallback(callback);
    return submit(backend.descriptor.context, &public_request);
}

fn r4dStorageCancel(ctx: ?*anyopaque, handle: u64, reason: u32) i32 {
    const raw = ctx orelse return -1;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const cancel = storageBackendCancel(backend.descriptor) orelse return -1;
    const callback = enterStorageCallback(backend.owner) orelse return -1;
    defer leaveStorageCallback(callback);
    return cancel(backend.descriptor.context, handle, reason);
}

fn r4dStorageReset(ctx: ?*anyopaque, reason: u32) i32 {
    const raw = ctx orelse return -1;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const reset_fn = storageBackendReset(backend.descriptor) orelse return -1;
    const callback = enterStorageCallback(backend.owner) orelse return -1;
    defer leaveStorageCallback(callback);
    return reset_fn(backend.descriptor.context, reason);
}

const StorageCleanupResult = struct {
    removed: u32 = 0,
    remaining: u32 = 0,
};

const StorageCleanupEntry = struct {
    backend_index: usize = 0,
    token: storage.UnregisterToken = .{},
};

const StorageCleanupPlan = struct {
    entries: [MAX_R4D_STORAGE_BACKENDS]StorageCleanupEntry = .{StorageCleanupEntry{}} ** MAX_R4D_STORAGE_BACKENDS,
    count: usize = 0,
};

fn prepareStorageOwnerCleanup(owner: u32, plan: *StorageCleanupPlan) bool {
    plan.* = .{};
    var index: usize = 0;
    while (index < r4d_storage_backends.len) : (index += 1) {
        const backend = &r4d_storage_backends[index];
        if (!backend.used or backend.owner != owner) continue;
        const token = storage.prepareUnregister(backend.block_index) orelse {
            cancelStorageOwnerCleanup(plan);
            return false;
        };
        plan.entries[plan.count] = .{
            .backend_index = index,
            .token = token,
        };
        plan.count += 1;
    }
    return true;
}

fn cancelStorageOwnerCleanup(plan: *StorageCleanupPlan) void {
    while (plan.count != 0) {
        plan.count -= 1;
        _ = storage.cancelUnregister(&plan.entries[plan.count].token);
    }
}

fn commitStorageOwnerCleanup(plan: *StorageCleanupPlan) StorageCleanupResult {
    var result = StorageCleanupResult{};
    var prepared_index: usize = 0;
    while (prepared_index < plan.count) : (prepared_index += 1) {
        const entry = &plan.entries[prepared_index];
        const backend = &r4d_storage_backends[entry.backend_index];
        if (!backend.used) {
            _ = storage.cancelUnregister(&entry.token);
            result.remaining +|= 1;
            continue;
        }
        if (backend.descriptor.shutdown) |shutdown| {
            const shutdown_result = shutdown(backend.descriptor.context);
            if (shutdown_result != 0) {
                bootlog.puts("[R4D] storage finalizer failed owner=");
                bootlog.putDec(backend.owner);
                bootlog.puts(" block=");
                bootlog.putDec(backend.block_index);
                bootlog.puts(" code=");
                const signed_result: i64 = shutdown_result;
                const magnitude: u64 = @intCast(if (signed_result < 0) -signed_result else signed_result);
                bootlog.putDec(magnitude);
                bootlog.puts("\r\n");
                _ = storage.cancelUnregister(&entry.token);
                result.remaining +|= 1;
                continue;
            }
        }
        if (!storage.commitUnregister(&entry.token)) {
            _ = storage.cancelUnregister(&entry.token);
            result.remaining +|= 1;
            continue;
        }
        backend.* = R4DStorageBackend{};
        result.removed +|= 1;
    }
    plan.count = 0;
    return result;
}

fn cleanupAudioOwner(owner: u32) u32 {
    var removed: u32 = 0;
    var index: usize = 0;
    while (index < r4d_audio_backends.len) : (index += 1) {
        const backend = &r4d_audio_backends[index];
        if (!backend.used or backend.owner != owner) continue;
        if (backend.descriptor.shutdown) |shutdown| _ = shutdown(backend.descriptor.context);
        _ = audio.unregisterAudioBackendByName(backend.name[0..backend.name_len]);
        backend.* = R4DAudioBackend{};
        removed += 1;
    }
    return removed;
}

fn cleanupNetOwner(owner: u32) NetOwnerCleanupResult {
    var result: NetOwnerCleanupResult = .{};
    var index: usize = 0;
    while (index < r4d_net_backends.len) : (index += 1) {
        const backend = &r4d_net_backends[index];
        if (!backend.used or backend.owner != owner) continue;
        if (backend.descriptor.shutdown) |shutdown| {
            if (shutdown(backend.descriptor.context) != 0) {
                result.failed = true;
                continue;
            }
        }
        const removed_adapter = net.unregister(backend.adapter_index);
        if (!removed_adapter) {
            result.failed = true;
            continue;
        }
        fixR4DNetAdapterIndexes(backend.adapter_index);
        backend.* = R4DNetBackend{};
        result.removed += 1;
    }
    return result;
}

fn ownerHasNetBackend(owner: u32) bool {
    for (&r4d_net_backends) |*backend| {
        if (backend.used and backend.owner == owner) return true;
    }
    return false;
}

fn fixR4DNetAdapterIndexes(removed_index: usize) void {
    var index: usize = 0;
    while (index < r4d_net_backends.len) : (index += 1) {
        const backend = &r4d_net_backends[index];
        if (!backend.used or backend.adapter_index <= removed_index) continue;
        backend.adapter_index -= 1;
    }
}

fn findR4DStorageBackendByName(name: [*:0]const u8) ?*R4DStorageBackend {
    var index: usize = 0;
    while (index < r4d_storage_backends.len) : (index += 1) {
        const backend = &r4d_storage_backends[index];
        if (backend.used and zEqSlice(name, backend.name[0..backend.name_len])) return backend;
    }
    return null;
}

fn findR4DNetBackend(adapter_index: usize) ?*R4DNetBackend {
    var index: usize = 0;
    while (index < r4d_net_backends.len) : (index += 1) {
        if (r4d_net_backends[index].used and r4d_net_backends[index].adapter_index == adapter_index) return &r4d_net_backends[index];
    }
    return null;
}

fn freeR4DStorageBackendSlot() ?usize {
    var index: usize = 0;
    while (index < r4d_storage_backends.len) : (index += 1) {
        if (!r4d_storage_backends[index].used) return index;
    }
    return null;
}

fn freeR4DNetBackendSlot() ?usize {
    var index: usize = 0;
    while (index < r4d_net_backends.len) : (index += 1) {
        if (!r4d_net_backends[index].used) return index;
    }
    return null;
}

fn freeR4DAudioBackendSlot() ?usize {
    var index: usize = 0;
    while (index < r4d_audio_backends.len) : (index += 1) {
        if (!r4d_audio_backends[index].used) return index;
    }
    return null;
}

fn trackDmaAllocation(owner: u32, phys_addr: u64, bytes: u32) bool {
    var index: usize = 0;
    while (index < dma_allocations.len) : (index += 1) {
        if (dma_allocations[index].used) continue;
        dma_allocations[index] = .{
            .used = true,
            .owner = owner,
            .phys_addr = phys_addr,
            .bytes = bytes,
        };
        return true;
    }
    return false;
}

fn takeDmaAllocation(owner: u32, phys_addr: u64) ?DmaAllocation {
    if (owner == 0) return null;
    var index: usize = 0;
    while (index < dma_allocations.len) : (index += 1) {
        if (!dma_allocations[index].used or dma_allocations[index].owner != owner or dma_allocations[index].phys_addr != phys_addr) continue;
        const allocation = dma_allocations[index];
        dma_allocations[index] = .{};
        return allocation;
    }
    return null;
}

fn cleanupDmaOwner(owner: u32) u32 {
    var removed: u32 = 0;
    var mapping_index: usize = 0;
    while (mapping_index < dma_mappings.len) : (mapping_index += 1) {
        if (!dma_mappings[mapping_index].used or dma_mappings[mapping_index].owner != owner) continue;
        releaseDmaMapping(mapping_index, true);
        removed += 1;
    }
    var pin_index: usize = 0;
    while (pin_index < dma_pins.len) : (pin_index += 1) {
        if (!dma_pins[pin_index].used or dma_pins[pin_index].owner != owner) continue;
        if (dma_pins[pin_index].map_count != 0) continue;
        dma_pins[pin_index] = .{};
        removed += 1;
    }
    var index: usize = 0;
    while (index < dma_allocations.len) : (index += 1) {
        const allocation = dma_allocations[index];
        if (!allocation.used or allocation.owner != owner) continue;
        const frames = frameCount(allocation.bytes) orelse 0;
        if (frames > 0) phys.freeContiguousFrames(allocation.phys_addr, frames);
        dma_allocations[index] = .{};
        removed += 1;
    }
    return removed;
}

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

fn validSectorSize(size: u32) bool {
    return size == 512 or size == 1024 or size == 2048 or size == 4096;
}

fn pciInfoFromDevice(dev: pci_inventory.Device) PciDeviceInfo {
    const route = pci_inventory.readInterruptRoute(dev);
    return .{
        .bus_kind = dev.bus_kind,
        .bus = dev.bus,
        .device = dev.device,
        .function = dev.function,
        .vendor_id = dev.vendor_id,
        .device_id = dev.device_id,
        .class_code = dev.class_code,
        .subclass = dev.subclass,
        .prog_if = dev.prog_if,
        .interrupt_line = route.line,
        .interrupt_pin = route.pin,
        .command = pci_inventory.readCommand(dev),
    };
}

fn macIsZero(mac: [6]u8) bool {
    var index: usize = 0;
    while (index < mac.len) : (index += 1) {
        if (mac[index] != 0) return false;
    }
    return true;
}

fn logRegister(kind: []const u8, name: [*:0]const u8) i32 {
    bootlog.puts("[R4D] register ");
    bootlog.puts(kind);
    bootlog.puts(" backend ");
    putZ(name);
    bootlog.puts("\r\n");
    return 0;
}

fn putZ(text: [*:0]const u8) void {
    var i: usize = 0;
    while (text[i] != 0 and i < 512) : (i += 1) {
        bootlog.putc(text[i]);
    }
}

fn copyOptionValue(value: []const u8) void {
    @memset(option_value_z[0..], 0);
    const n = if (value.len < option_value_z.len) value.len else option_value_z.len - 1;
    if (n > 0) @memcpy(option_value_z[0..n], value[0..n]);
}

fn copyZName(src: [*:0]const u8, dst: []u8, len_out: *usize) void {
    var len: usize = 0;
    while (len + 1 < dst.len and src[len] != 0) : (len += 1) {
        dst[len] = src[len];
    }
    if (len < dst.len) dst[len] = 0;
    len_out.* = len;
}

fn zSlice(src: [*:0]const u8) []const u8 {
    var len: usize = 0;
    while (len < 512 and src[len] != 0) : (len += 1) {}
    return src[0..len];
}

fn alignUp(value: u64, alignment: u64) u64 {
    return (value + alignment - 1) & ~(alignment - 1);
}

fn alignDown(value: u64, alignment: u64) u64 {
    return value & ~(alignment - 1);
}

fn zEqSlice(z: [*:0]const u8, slice: []const u8) bool {
    var i: usize = 0;
    while (i < slice.len) : (i += 1) {
        if (upper(z[i]) != upper(slice[i])) return false;
    }
    return z[i] == 0;
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}
