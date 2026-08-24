const std = @import("std");
const io = @import("../arch/x86_64/io.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const audio = @import("../audio/core.zig");
const bootlog = @import("bootlog.zig");
const boot_config = @import("boot_config.zig");
const log_event = @import("log_event.zig");
const net = @import("../net/core.zig");
const pci_inventory = @import("../platform/pci_inventory.zig");
const platform_cpu = @import("../platform/cpu.zig");
const paging = @import("../memory/paging.zig");
const phys = @import("../memory/phys.zig");
const irq_router = @import("irq_router.zig");
const driver_work = @import("driver_work.zig");
const scheduler = @import("../sched/scheduler.zig");
const sync = @import("../sched/sync.zig");
const storage = @import("../storage/block.zig");
const timer = @import("timer.zig");
const usb_host = @import("../driver/usb/host_controller.zig");

pub const MAGIC: u32 = 0x31495044; // "DPI1" little endian
// Version 17 (0.69.17): append-only um begrenzte DMA-Allokation und
// explizites MSI-Rollback erweitert.
pub const VERSION: u32 = 17;

const AUDIO_BACKEND_VERSION: u32 = 2;
const AUDIO_BACKEND_FORMAT_S16LE: u32 = 1 << 0;
const AUDIO_BACKEND_FORMAT_U8: u32 = 1 << 1;
const SYNTH_ENGINE_VERSION: u32 = 1;
const STORAGE_BACKEND_VERSION: u32 = 1;
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
const USB_HOST_BACKEND_VERSION: u32 = 1;
const USB_HOST_SOURCE_BUILTIN: u32 = 0;
const USB_HOST_SOURCE_PRELOAD: u32 = 1;
const USB_HOST_SOURCE_DISK: u32 = 2;
const NET_BACKEND_VERSION: u32 = 1;
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
const MAX_R4D_AUDIO_BACKENDS: usize = 4;
const MAX_R4D_AUDIO_NAME: usize = 32;
const MAX_R4D_DMA_ALLOCATIONS: usize = 32;
const MAX_MMIO_MAP_BYTES: u64 = 16 * 1024 * 1024;
const MMIO_MAP_WRITE_COMBINING: u32 = 1 << 0;

const empty_z: [1:0]u8 = .{0};
var option_value_z: [64:0]u8 = .{0} ** 64;

pub const NetBackendStatus = net.BackendStatus;

pub const DmaBuffer = extern struct {
    phys_addr: u64 = 0,
    virt_addr: u64 = 0,
    bytes: u32 = 0,
    alignment: u32 = 0,
    flags: u32 = 0,
    reserved: u32 = 0,
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
};

pub const StorageBackendStatus = extern struct {
    state: u32,
    last_error: u32,
    last_lba: u64,
    last_sectors: u32,
    recoveries: u64,
    recovery_failures: u64,
};

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
};

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
};

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
var dma_allocations: [MAX_R4D_DMA_ALLOCATIONS]DmaAllocation = .{DmaAllocation{}} ** MAX_R4D_DMA_ALLOCATIONS;

pub const OwnerCleanupToken = struct {
    owner: u32 = 0,
    storage_plan: StorageCleanupPlan = .{},
    net_mutation_active: bool = false,
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
    token.active = true;
    return token;
}

pub fn cancelOwnerCleanup(token: *OwnerCleanupToken) bool {
    if (!token.active or token.shutdown_started or current_owner != token.owner or !current_owner_guard.ownedByCurrent()) return false;
    cancelStorageOwnerCleanup(&token.storage_plan);
    finishOwnerNetMutation(token);
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
    const audio_count = cleanupAudioOwner(owner);
    const usb_host_count = usb_host.cleanupOwner(owner);
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
        usb_host_count == 0 and
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
    bootlog.putDec(usb_host_count);
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
        bootlog.puts(" storage-busy\r\n");
        return false;
    };
    return commitOwnerCleanup(&token);
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
    if (phys_addr == 0) return;
    const frames = frameCount(bytes) orelse 1;
    untrackDmaAllocation(phys_addr);
    phys.freeContiguousFrames(phys_addr, frames);
}

fn allocDmaRegion(bytes: u32, alignment: u32, out: *DmaBuffer) callconv(.c) i32 {
    return allocDmaRegionConstrained(bytes, alignment, std.math.maxInt(u64), out);
}

fn allocDmaRegionConstrained(bytes: u32, alignment: u32, max_phys_addr: u64, out: *DmaBuffer) callconv(.c) i32 {
    out.* = .{};
    const dma_alignment = if (alignment == 0) @as(u32, @intCast(phys.FRAME_SIZE)) else alignment;
    if (bytes == 0) return -1;
    if (@as(u64, dma_alignment) > phys.FRAME_SIZE) return -2;
    const frames = frameCount(bytes) orelse return -3;
    const phys_addr = phys.allocContiguousFramesBelow(frames, max_phys_addr) orelse return -4;
    const virt_addr = phys.physToVirt(phys_addr);
    const total_bytes = frames * phys.FRAME_SIZE;
    const data: [*]u8 = @ptrFromInt(virt_addr);
    @memset(data[0..@intCast(total_bytes)], 0);
    if (!trackDmaAllocation(current_owner, phys_addr, @intCast(total_bytes))) {
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
    const frames = frameCount(buffer.bytes) orelse 0;
    untrackDmaAllocation(buffer.phys_addr);
    if (frames > 0) phys.freeContiguousFrames(buffer.phys_addr, frames);
    buffer.* = .{};
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
    if (current_owner != 0) return current_owner;
    return irq_router.currentOwner();
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

    // Fixed/Edge an den BSP; Multiple Message Enable bleibt auf einer
    // Nachricht. Message Data traegt direkt den IDT-Vektor.
    const address: u32 = 0xFEE0_0000 | (platform_cpu.bootApicId() << 12);
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
        .queue_depth = descriptor.queue_depth,
        .timeout_ticks = descriptor.timeout_ticks,
        .removable = (descriptor.flags & STORAGE_BACKEND_FLAG_REMOVABLE) != 0,
        .writable = (descriptor.flags & STORAGE_BACKEND_FLAG_WRITABLE) != 0,
        .source = storageSource(descriptor.source),
        .owner_id = current_owner,
        .ctx = slot,
        .read_fn = r4dStorageRead,
        .write_fn = if (descriptor.write != null) r4dStorageWrite else null,
        .flush_fn = if (descriptor.flush != null) r4dStorageFlush else null,
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
    return if (usb_host.unregisterByName(zSlice(name))) 0 else -1;
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
    return audio.registerExternalSynthEngineZ(name, engine.context, engine.midi_send, engine.render, engine.stop, engine.status, engine.opl3_reset, engine.opl3_write_register, engine.sid_acquire, engine.sid_release, engine.sid_set_model, engine.sid_write_register, engine.sid_load_data, engine.sid_init, engine.sid_play_frame, engine.sid_render_pcm);
}

fn registerMixerBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    return audio.registerMixerBackendZ(name, backend);
}

fn registerNetBackend(name: [*:0]const u8, backend: *const anyopaque) callconv(.c) i32 {
    const descriptor: *const NetBackendDescriptor = @ptrCast(@alignCast(backend));
    if (!validNetBackend(descriptor)) return -1;
    const slot_index = freeR4DNetBackendSlot() orelse {
        bootlog.puts("[R4D][ERROR] net backend table full\r\n");
        return -2;
    };
    const slot = &r4d_net_backends[slot_index];
    slot.* = .{
        .used = true,
        .owner = current_owner,
        .descriptor = descriptor,
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
    if (descriptor.size < @sizeOf(SynthEngineDescriptor)) {
        bootlog.puts("[R4D][ERROR] synth engine descriptor too small\r\n");
        return false;
    }
    if (descriptor.midi_send == null and descriptor.render == null and descriptor.stop == null and descriptor.status == null and
        descriptor.opl3_reset == null and descriptor.opl3_write_register == null and descriptor.sid_acquire == null and
        descriptor.sid_release == null and descriptor.sid_set_model == null and descriptor.sid_write_register == null and descriptor.sid_load_data == null and
        descriptor.sid_init == null and descriptor.sid_play_frame == null and descriptor.sid_render_pcm == null)
    {
        bootlog.puts("[R4D][ERROR] synth engine has no operations\r\n");
        return false;
    }
    return true;
}

fn validStorageBackend(descriptor: *const StorageBackendDescriptor) bool {
    if (descriptor.version != STORAGE_BACKEND_VERSION) {
        bootlog.puts("[R4D][ERROR] storage backend version mismatch\r\n");
        return false;
    }
    if (descriptor.size < @sizeOf(StorageBackendDescriptor)) {
        bootlog.puts("[R4D][ERROR] storage backend descriptor too small\r\n");
        return false;
    }
    if (descriptor.read == null) {
        bootlog.puts("[R4D][ERROR] storage backend missing read callback\r\n");
        return false;
    }
    if ((descriptor.flags & STORAGE_BACKEND_FLAG_WRITABLE) != 0 and descriptor.write == null) {
        bootlog.puts("[R4D][ERROR] storage backend writable without write callback\r\n");
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
    return true;
}

fn validNetBackend(descriptor: *const NetBackendDescriptor) bool {
    if (descriptor.version != NET_BACKEND_VERSION) {
        bootlog.puts("[R4D][ERROR] net backend version mismatch\r\n");
        return false;
    }
    if (descriptor.size < @sizeOf(NetBackendDescriptor)) {
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
    if (!fnptr_ok) {
        bootlog.puts("[R4D][ERROR] NETBACKEND BAD FNPTR (unrelocated?)\r\n");
        return false;
    }
    return true;
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
    return if (net.receiveFrame(index, slice)) 0 else -3;
}

fn r4dStorageRead(ctx: ?*anyopaque, lba: u64, sectors: u16, out: []u8) bool {
    const raw = ctx orelse return false;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const read = backend.descriptor.read orelse return false;
    return read(backend.descriptor.context, lba, sectors, out.ptr, @intCast(out.len)) == 0;
}

fn r4dStorageWrite(ctx: ?*anyopaque, lba: u64, sectors: u16, data: []const u8) bool {
    const raw = ctx orelse return false;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const write = backend.descriptor.write orelse return false;
    return write(backend.descriptor.context, lba, sectors, data.ptr, @intCast(data.len)) == 0;
}

fn r4dStorageFlush(ctx: ?*anyopaque) bool {
    const raw = ctx orelse return false;
    const backend: *R4DStorageBackend = @ptrCast(@alignCast(raw));
    const flush = backend.descriptor.flush orelse return true;
    return flush(backend.descriptor.context) == 0;
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
    for (r4d_net_backends) |backend| {
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

fn untrackDmaAllocation(phys_addr: u64) void {
    var index: usize = 0;
    while (index < dma_allocations.len) : (index += 1) {
        if (!dma_allocations[index].used or dma_allocations[index].phys_addr != phys_addr) continue;
        dma_allocations[index] = .{};
        return;
    }
}

fn cleanupDmaOwner(owner: u32) u32 {
    var removed: u32 = 0;
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
