// Early memory core orchestration for kernel startup.
//
// This layer starts the memory foundation after BootInfo, display, CPU and
// interrupt basis, timer, driver registry, and early input. It only groups the
// boot order; memory domain logic stays in the modules under Code/Kernel/memory/.

const display = @import("../display/display.zig");
const config = @import("config");
const blocks = @import("../memory/blocks.zig");
const heap = @import("../memory/heap.zig");
const mem_map = @import("../memory/map.zig");
const page_tables = @import("../memory/page_tables.zig");
const paging = @import("../memory/paging.zig");
const phys = @import("../memory/phys.zig");
const virt = @import("../memory/virt.zig");
const boot_status = @import("boot_status.zig");
const fatal = @import("fatal.zig");
const k = @import("log.zig");
const smp = @import("smp.zig");

var core_initialized = false;
var summary_ready = false;
var cached_usable_bytes: u64 = 0;
var cached_largest_usable_base: u64 = 0;
var cached_largest_usable_len: u64 = 0;
var device_mappings_finalized = false;
var cr3_switch_done = false;

pub fn initCore() bool {
    if (core_initialized) return true;

    if (!initMemoryMap()) return false;
    if (!initMemoryBlocks()) return false;
    if (!initPhysicalAllocator()) return false;
    if (!initPagingCore()) return false;
    if (!initPageTableBuilder()) return false;
    if (!initVirtualRanges()) return false;
    initFramebufferWriteCombining();
    if (!initKernelHeap()) return false;

    boot_status.statusLine("  Memory [OK]\r\n");
    core_initialized = true;
    return true;
}

fn initMemoryMap() bool {
    beginPhase("Memory Map");
    mem_map.dump();
    const mem_summary = mem_map.summarize();
    cached_usable_bytes = mem_summary.usable_bytes;
    cached_largest_usable_base = mem_summary.largest_usable_base;
    cached_largest_usable_len = mem_summary.largest_usable_len;
    summary_ready = true;
    endPhase("Memory Map");
    return true;
}

fn initMemoryBlocks() bool {
    beginPhase("MemoryBlock table");
    if (!blocks.initFromBootInfo()) {
        return fail("MemoryBlock init failed");
    }
    blocks.dumpSummary();
    endPhase("MemoryBlock table");
    return true;
}

fn initPhysicalAllocator() bool {
    beginPhase("Physical allocator");
    if (!phys.init()) {
        return fail("Physical allocator init failed");
    }
    smp.reserveLowMemory();
    if (comptime config.enable_boot_selftests) {
        if (!blocks.indexInvariant()) return fail("MemoryBlock index invariant failed");
    }
    phys.dumpStats();
    endPhase("Physical allocator");
    return true;
}

fn initPagingCore() bool {
    beginPhase("Paging core");
    if (!paging.init()) {
        return fail("Paging init failed");
    }
    paging.dumpStats();
    endPhase("Paging core");
    return true;
}

fn initPageTableBuilder() bool {
    if (comptime config.enable_boot_selftests) {
        beginPhase("Page-table builder selftest");
        if (!page_tables.bootSanityCheck()) {
            return fail("Page-table builder selftest failed");
        }
        endPhase("Page-table builder selftest");
    } else {
        skipPhase("Page-table builder selftest");
    }

    beginPhase("R4OS PML4 dry-run");
    if (!page_tables.buildKernelSpaceDryRun()) {
        page_tables.dumpStats();
        return fail("R4OS PML4 dry-run failed");
    }
    page_tables.dumpStats();
    endPhase("R4OS PML4 dry-run");
    return true;
}

fn initVirtualRanges() bool {
    beginPhase("Virtual ranges");
    if (!virt.init()) {
        return fail("Virtual range init failed");
    }
    virt.dumpStats();
    endPhase("Virtual ranges");
    return true;
}

fn initFramebufferWriteCombining() void {
    beginPhase("Framebuffer PAT-WC");
    if (display.enableFramebufferWriteCombining()) {
        endPhase("Framebuffer PAT-WC");
    } else {
        skipPhase("Framebuffer PAT-WC");
    }
}

fn initKernelHeap() bool {
    beginPhase("Kernel heap");
    if (!heap.init()) {
        return fail("Kernel heap init failed");
    }
    heap.dumpStats();
    if (!heap.bootInvariant()) {
        return fail("Kernel heap invariant failed");
    }
    if (comptime config.enable_boot_selftests) {
        if (!heap.selfTest()) {
            return fail("Kernel heap selftest failed");
        }
        if (!blocks.indexInvariant() or !virt.metadataInvariant()) {
            return fail("Memory metadata index invariant failed");
        }
    }
    endPhase("Kernel heap");
    return true;
}

fn beginPhase(name: []const u8) void {
    k.puts("MemoryBoot: ");
    k.puts(name);
    k.puts(" [START]\r\n");
}

fn endPhase(name: []const u8) void {
    k.puts("MemoryBoot: ");
    k.puts(name);
    k.puts(" [OK]\r\n");
}

fn skipPhase(name: []const u8) void {
    k.puts("MemoryBoot: ");
    k.puts(name);
    k.puts(" [SKIP]\r\n");
}
pub fn isCoreInitialized() bool {
    return core_initialized;
}

pub fn usableBytes() u64 {
    return cached_usable_bytes;
}

pub fn hasSummary() bool {
    return summary_ready;
}

pub fn largestUsableBase() u64 {
    return cached_largest_usable_base;
}

pub fn largestUsableLen() u64 {
    return cached_largest_usable_len;
}

pub fn dumpBlockSummary() void {
    blocks.dumpSummary();
}

pub fn finalizeDeviceMappings(acpi_info: anytype) bool {
    if (device_mappings_finalized and cr3_switch_done) return true;
    if (!core_initialized) {
        return fail("Memory core not initialized before device mappings");
    }

    if (!page_tables.mapDeviceSpaceDryRun(acpi_info)) {
        return fail("R4OS device mappings dry-run failed");
    }
    device_mappings_finalized = true;
    k.puts("  R4OS device mappings ");
    k.puts("[OK]\r\n");
    page_tables.dumpDeviceMappings();

    if (!page_tables.switchToKernelSpaceIfReady()) {
        page_tables.dumpCr3SwitchStatus();
        return fail("R4OS CR3 switch failed");
    }
    cr3_switch_done = true;
    k.puts("  R4OS CR3 switch ");
    k.puts("[OK]\r\n");
    page_tables.dumpCr3SwitchStatus();

    return true;
}

pub fn deviceMappingsFinalized() bool {
    return device_mappings_finalized;
}

pub fn cr3SwitchDone() bool {
    return cr3_switch_done;
}

fn fail(message: []const u8) bool {
    return fatal.fail(.memory, message);
}
