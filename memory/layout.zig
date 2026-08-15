const k = @import("../kernel/log.zig");

pub const KiB: u64 = 1024;
pub const MiB: u64 = 1024 * KiB;
pub const GiB: u64 = 1024 * MiB;
pub const TiB: u64 = 1024 * GiB;

pub const PAGE_SIZE: u64 = 4096;
pub const ENTRIES_PER_TABLE: usize = 512;

pub const CANONICAL_LOW_BASE: u64 = 0x0000_0000_0000_0000;
pub const CANONICAL_LOW_END: u64 = 0x0000_8000_0000_0000;
pub const CANONICAL_HIGH_BASE: u64 = 0xffff_8000_0000_0000;
pub const CANONICAL_HIGH_END: u64 = 0;

pub const DIRECT_MAP_BASE: u64 = 0xffff_8000_0000_0000;
pub const DIRECT_MAP_BYTES: u64 = 64 * TiB;
pub const DIRECT_MAP_END: u64 = DIRECT_MAP_BASE + DIRECT_MAP_BYTES;

pub const KERNEL_DYNAMIC_BASE: u64 = 0xffff_c000_0000_0000;
pub const KERNEL_DYNAMIC_BYTES: u64 = 32 * TiB;
pub const KERNEL_DYNAMIC_END: u64 = KERNEL_DYNAMIC_BASE + KERNEL_DYNAMIC_BYTES;

pub const MMIO_BASE: u64 = 0xffff_e000_0000_0000;
pub const MMIO_BYTES: u64 = 16 * TiB;
pub const MMIO_END: u64 = MMIO_BASE + MMIO_BYTES;

pub const FRAMEBUFFER_BASE: u64 = 0xffff_f000_0000_0000;
pub const FRAMEBUFFER_BYTES: u64 = 1 * TiB;
pub const FRAMEBUFFER_END: u64 = FRAMEBUFFER_BASE + FRAMEBUFFER_BYTES;

pub const KERNEL_IMAGE_BASE: u64 = 0xffff_ffff_8000_0000;
pub const KERNEL_IMAGE_WINDOW_BYTES: u64 = 512 * MiB;
pub const KERNEL_IMAGE_WINDOW_END: u64 = KERNEL_IMAGE_BASE + KERNEL_IMAGE_WINDOW_BYTES;

pub const PageFlags = struct {
    pub const present: u64 = 1 << 0;
    pub const writable: u64 = 1 << 1;
    pub const user: u64 = 1 << 2;
    pub const write_through: u64 = 1 << 3;
    pub const cache_disable: u64 = 1 << 4;
    pub const accessed: u64 = 1 << 5;
    pub const dirty: u64 = 1 << 6;
    pub const page_attribute_table: u64 = 1 << 7;
    pub const global: u64 = 1 << 8;
    pub const no_execute: u64 = 1 << 63;
};

pub const CachePolicy = enum(u8) {
    normal_ram,
    framebuffer_write_combining,
    mmio_uncached,
    acpi_firmware,
    unknown,
};

pub const RegionKind = enum(u8) {
    unused_low,
    direct_map,
    kernel_dynamic,
    mmio,
    framebuffer,
    kernel_image,
};

pub const Region = struct {
    name: []const u8,
    kind: RegionKind,
    base: u64,
    end: u64,
    cache_policy: CachePolicy,
    executable: bool,
    writable: bool,
    dynamic: bool = false,
    note: []const u8 = "",
};

pub const planned_regions = [_]Region{
    .{
        .name = "low-canonical-unused",
        .kind = .unused_low,
        .base = CANONICAL_LOW_BASE,
        .end = CANONICAL_LOW_END,
        .cache_policy = .unknown,
        .executable = false,
        .writable = false,
        .note = "no user/app address range in 0.25.X",
    },
    .{
        .name = "hhdm-direct-map",
        .kind = .direct_map,
        .base = DIRECT_MAP_BASE,
        .end = DIRECT_MAP_END,
        .cache_policy = .normal_ram,
        .executable = false,
        .writable = true,
        .dynamic = true,
        .note = "physical -> virtual direct map; BootInfo HHDM remains the source until R4OS maps it itself",
    },
    .{
        .name = "kernel-dynamic",
        .kind = .kernel_dynamic,
        .base = KERNEL_DYNAMIC_BASE,
        .end = KERNEL_DYNAMIC_END,
        .cache_policy = .normal_ram,
        .executable = false,
        .writable = true,
        .note = "Heap, VirtualRanges, PageTables, stacks, later kernel objects",
    },
    .{
        .name = "mmio-window",
        .kind = .mmio,
        .base = MMIO_BASE,
        .end = MMIO_END,
        .cache_policy = .mmio_uncached,
        .executable = false,
        .writable = true,
        .note = "APIC, IOAPIC, HPET, PCIe/ECAM and Device-MMIO",
    },
    .{
        .name = "framebuffer-window",
        .kind = .framebuffer,
        .base = FRAMEBUFFER_BASE,
        .end = FRAMEBUFFER_END,
        .cache_policy = .framebuffer_write_combining,
        .executable = false,
        .writable = true,
        .note = "visible display backends; WC when stable",
    },
    .{
        .name = "kernel-image",
        .kind = .kernel_image,
        .base = KERNEL_IMAGE_BASE,
        .end = KERNEL_IMAGE_WINDOW_END,
        .cache_policy = .normal_ram,
        .executable = true,
        .writable = false,
        .note = "current kernel link window; exact segment boundaries follow from linker symbols",
    },
};

pub fn dumpPlan() void {
    k.puts("  R4OS address layout plan:\r\n");
    var i: usize = 0;
    while (i < planned_regions.len) : (i += 1) {
        const r = planned_regions[i];
        k.puts("    ");
        k.puts(r.name);
        k.puts(" 0x");
        k.putHex(r.base, 16);
        k.puts("-0x");
        k.putHex(r.end, 16);
        k.puts(" cache=");
        k.puts(cachePolicyName(r.cache_policy));
        k.puts(" x=");
        k.puts(if (r.executable) "yes" else "no");
        k.puts(" w=");
        k.puts(if (r.writable) "yes" else "no");
        if (r.dynamic) k.puts(" dynamic=yes");
        k.puts("\r\n");
    }

    k.puts("  Page flags: present,writable,user,wt,cd,pat,global,nx\r\n");
}

pub fn cachePolicyName(policy: CachePolicy) []const u8 {
    return switch (policy) {
        .normal_ram => "normal-ram",
        .framebuffer_write_combining => "framebuffer-wc",
        .mmio_uncached => "mmio-uncached",
        .acpi_firmware => "acpi-firmware",
        .unknown => "unknown",
    };
}
