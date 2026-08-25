const acpi = @import("../../platform/acpi.zig");
const bootlog = @import("../../kernel/bootlog.zig");
const k = @import("../../kernel/log.zig");
const lapic = @import("lapic.zig");
const paging = @import("../../memory/paging.zig");
const phys = @import("../../memory/phys.zig");
const percpu = @import("percpu.zig");

const REGSEL: u64 = 0x00;
const IOWIN: u64 = 0x10;

const REG_ID: u8 = 0x00;
const REG_VERSION: u8 = 0x01;
const REDIR_BASE: u8 = 0x10;

const REDIR_MASKED: u32 = 1 << 16;
const REDIR_POLARITY_LOW: u32 = 1 << 13;
const REDIR_TRIGGER_LEVEL: u32 = 1 << 15;
const IRQ_VECTOR_BASE: u8 = 0x20;
const PCI_INTX_FLAGS: u16 = 0x000F;

pub const RedirectionEntry = struct {
    low: u32 = 0,
    high: u32 = 0,
    present: bool = false,
};

pub const IrqRoute = struct {
    irq: u8 = 0,
    gsi: u32 = 0,
    vector: u8 = 0,
    flags: u16 = 0,
    target_lapic: u8 = 0,
    redir_index: u8 = 0,
    present: bool = false,
    in_range: bool = false,
    override: bool = false,
};

pub const Status = struct {
    available: bool = false,
    mapped: bool = false,
    madt_id: u8 = 0,
    reg_id: u8 = 0,
    version: u8 = 0,
    max_redirection_entry: u8 = 0,
    phys_base: u64 = 0,
    virt_base: u64 = 0,
    gsi_base: u32 = 0,
    redir0: RedirectionEntry = .{},
    redir1: RedirectionEntry = .{},
    redir2: RedirectionEntry = .{},
    redir12: RedirectionEntry = .{},
    route0: IrqRoute = .{},
    route1: IrqRoute = .{},
    route2: IrqRoute = .{},
    route12: IrqRoute = .{},
    routing_active: bool = false,
    programmed_routes: u8 = 0,
    reason: []const u8 = "not initialized",
};

var current: Status = .{};
var base_virt: u64 = 0;
var next_runtime_target: u32 = 1;

pub fn initFromAcpi(info: acpi.Info) Status {
    current = .{
        .available = info.madt_ioapic_count > 0 and info.madt_first_ioapic_address != 0,
        .madt_id = info.madt_first_ioapic_id,
        .phys_base = info.madt_first_ioapic_address,
        .gsi_base = info.madt_first_ioapic_gsi_base,
        .reason = "MADT has no IOAPIC",
    };
    base_virt = 0;
    if (!current.available) {
        logStatus();
        return current;
    }
    if (!mapMmio(current.phys_base)) {
        current.reason = "IOAPIC MMIO mapping failed";
        logStatus();
        return current;
    }

    const id_reg = readReg(REG_ID);
    const version_reg = readReg(REG_VERSION);
    current.reg_id = @truncate((id_reg >> 24) & 0x0F);
    current.version = @truncate(version_reg);
    current.max_redirection_entry = @truncate((version_reg >> 16) & 0xFF);
    current.mapped = true;
    current.redir0 = readRedirectionEntryIfPresent(0);
    current.redir1 = readRedirectionEntryIfPresent(1);
    current.redir2 = readRedirectionEntryIfPresent(2);
    current.redir12 = readRedirectionEntryIfPresent(12);
    current.route0 = buildRoute(info, 0);
    current.route1 = buildRoute(info, 1);
    current.route2 = buildRoute(info, 2);
    current.route12 = buildRoute(info, 12);
    current.reason = "MMIO mapped, redirection table visible, routing unchanged";
    logStatus();
    return current;
}

pub fn status() Status {
    return current;
}

pub fn isRoutingActive() bool {
    return current.routing_active;
}

pub fn dumpStatus() void {
    const s = current;
    k.puts("IOAPIC status\r\n");
    k.puts("  Available: ");
    k.puts(if (s.available) "yes" else "no");
    k.puts(" mapped=");
    k.puts(if (s.mapped) "yes" else "no");
    k.puts(" routing=");
    k.puts(if (s.routing_active) "active" else "off");
    k.puts(" programmed=");
    k.putDec(s.programmed_routes);
    k.puts("\r\n");
    k.puts("  ID: madt=");
    k.putDec(s.madt_id);
    k.puts(" reg=");
    k.putDec(s.reg_id);
    k.puts(" version=0x");
    k.putHex(s.version, 2);
    k.puts(" redirs=");
    if (s.mapped) {
        k.putDec(@as(u64, s.max_redirection_entry) + 1);
    } else {
        k.putDec(0);
    }
    k.puts("\r\n");
    k.puts("  Base: phys=0x");
    k.putHex(s.phys_base, 16);
    k.puts(" virt=0x");
    k.putHex(s.virt_base, 16);
    k.puts(" gsi_base=");
    k.putDec(s.gsi_base);
    k.puts("\r\n");
    dumpRedir("  Redir 0:  ", s.redir0);
    dumpRedir("  Redir 1:  ", s.redir1);
    dumpRedir("  Redir 2:  ", s.redir2);
    dumpRedir("  Redir 12: ", s.redir12);
    dumpRoute("  Route IRQ0:  ", s.route0);
    dumpRoute("  Route IRQ1:  ", s.route1);
    dumpRoute("  Route IRQ2:  ", s.route2);
    dumpRoute("  Route IRQ12: ", s.route12);
    k.puts("  Note: ");
    k.puts(s.reason);
    k.puts("\r\n");
}

pub fn readRedirectionEntry(index: u8) RedirectionEntry {
    if (!current.mapped or index > current.max_redirection_entry) return .{};
    return .{
        .low = readReg(REDIR_BASE + @as(u8, index * 2)),
        .high = readReg(REDIR_BASE + @as(u8, index * 2) + 1),
        .present = true,
    };
}

pub fn setLegacyIrqMasked(irq: u8, masked: bool) bool {
    if (!current.mapped) return false;
    const route = routeForIrq(irq);
    if (!route.present or !route.in_range) return false;
    const low_reg = REDIR_BASE + @as(u8, route.redir_index * 2);
    var low = readReg(low_reg);
    if (masked) {
        low |= REDIR_MASKED;
    } else {
        low &= ~@as(u32, REDIR_MASKED);
    }
    writeReg(low_reg, low);
    refreshKnownRedirection(route.redir_index);
    return true;
}

pub fn activateLegacyIrq(irq: u8) bool {
    if (!current.mapped) return false;
    const route = routeForIrq(irq);
    if (!route.present or !route.in_range) return false;
    if (!programRoute(route)) return false;
    refreshKnownRedirection(route.redir_index);
    return true;
}

pub fn activatePciIntxIrq(irq: u8) bool {
    if (!current.mapped) return false;
    var route = routeForIrq(irq);
    if (!route.present or !route.in_range) return false;
    route.flags = PCI_INTX_FLAGS;
    route.target_lapic = chooseRuntimeTarget();
    if (!programRoute(route)) return false;
    refreshKnownRedirection(route.redir_index);
    return true;
}

fn chooseRuntimeTarget() u8 {
    const mask = percpu.schedulableMask();
    if (@popCount(mask) <= 1) return lapic.status().id;
    var attempts: u32 = 0;
    while (attempts < percpu.max_cpus) : (attempts += 1) {
        const index = next_runtime_target % @as(u32, @intCast(percpu.max_cpus));
        next_runtime_target +%= 1;
        if (index == 0 or !percpu.isSchedulable(index)) continue;
        const apic_id = percpu.apicId(index) orelse continue;
        if (apic_id <= 0xFF) return @intCast(apic_id);
    }
    return lapic.status().id;
}

pub fn activateLegacyRoutes() bool {
    if (!current.mapped) {
        bootlog.puts("[IOAPIC][WARN] legacy route activation skipped, IOAPIC not mapped\r\n");
        return false;
    }
    if (!current.route0.in_range or !current.route1.in_range or !current.route12.in_range) {
        bootlog.puts("[IOAPIC][WARN] legacy route activation skipped, route out of range\r\n");
        return false;
    }

    maskAllRedirections();
    current.programmed_routes = 0;
    if (programRoute(current.route0)) current.programmed_routes += 1;
    if (programRoute(current.route1)) current.programmed_routes += 1;
    if (programRoute(current.route12)) current.programmed_routes += 1;
    current.redir0 = readRedirectionEntryIfPresent(0);
    current.redir1 = readRedirectionEntryIfPresent(1);
    current.redir2 = readRedirectionEntryIfPresent(2);
    current.redir12 = readRedirectionEntryIfPresent(12);
    current.routing_active = current.programmed_routes == 3;
    current.reason = if (current.routing_active)
        "legacy IRQ0/1/12 routed through IOAPIC"
    else
        "IOAPIC routing incomplete, PIC fallback required";
    bootlog.puts("[IOAPIC] routing=");
    bootlog.puts(if (current.routing_active) "active" else "incomplete");
    bootlog.puts(" programmed=");
    bootlog.putDec(current.programmed_routes);
    bootlog.puts(" irq0=");
    bootlog.putDec(current.route0.gsi);
    bootlog.puts(" irq1=");
    bootlog.putDec(current.route1.gsi);
    bootlog.puts(" irq12=");
    bootlog.putDec(current.route12.gsi);
    bootlog.puts("\r\n");
    return current.routing_active;
}

pub fn routeForIrq(irq: u8) IrqRoute {
    return switch (irq) {
        0 => current.route0,
        1 => current.route1,
        2 => current.route2,
        12 => current.route12,
        else => .{
            .irq = irq,
            .gsi = irq,
            .vector = vectorForIrq(irq),
            .target_lapic = lapic.status().id,
            .present = current.mapped,
            .in_range = gsiInRange(irq),
        },
    };
}

fn readRedirectionEntryIfPresent(index: u8) RedirectionEntry {
    if (index > current.max_redirection_entry) return .{};
    return readRedirectionEntry(index);
}

fn buildRoute(info: acpi.Info, irq: u8) IrqRoute {
    var route = IrqRoute{
        .irq = irq,
        .gsi = irq,
        .vector = vectorForIrq(irq),
        .target_lapic = lapic.status().id,
        .present = current.mapped,
    };
    const max_iso = minU32(info.madt_iso_count, acpi.MAX_MADT_ISO);
    var i: usize = 0;
    while (i < max_iso) : (i += 1) {
        const iso = info.madt_isos[i];
        if (iso.bus == 0 and iso.source == irq) {
            route.gsi = iso.gsi;
            route.flags = iso.flags;
            route.override = true;
            break;
        }
    }
    if (gsiInRange(route.gsi)) {
        route.in_range = true;
        route.redir_index = @intCast(route.gsi - current.gsi_base);
    }
    return route;
}

fn gsiInRange(gsi: u32) bool {
    if (!current.mapped or gsi < current.gsi_base) return false;
    const index = gsi - current.gsi_base;
    return index <= current.max_redirection_entry;
}

fn vectorForIrq(irq: u8) u8 {
    if (irq <= 0xDF) return IRQ_VECTOR_BASE + irq;
    return 0xFF;
}

fn mapMmio(base_phys: u64) bool {
    const page = base_phys & ~(paging.PAGE_SIZE - 1);
    const virt_page = phys.physToVirt(page);
    if (!paging.isMapped(virt_page)) {
        if (!paging.mapPage(virt_page, page, paging.WRITABLE | paging.CACHE_DISABLE | paging.NO_EXECUTE)) {
            return false;
        }
    }
    base_virt = phys.physToVirt(base_phys);
    current.virt_base = base_virt;
    return true;
}

fn readReg(reg: u8) u32 {
    const select: *volatile u32 = @ptrFromInt(base_virt + REGSEL);
    const window: *volatile u32 = @ptrFromInt(base_virt + IOWIN);
    select.* = reg;
    return window.*;
}

fn writeReg(reg: u8, value: u32) void {
    const select: *volatile u32 = @ptrFromInt(base_virt + REGSEL);
    const window: *volatile u32 = @ptrFromInt(base_virt + IOWIN);
    select.* = reg;
    window.* = value;
}

fn maskAllRedirections() void {
    var index: u8 = 0;
    while (index <= current.max_redirection_entry) : (index += 1) {
        const low_reg = REDIR_BASE + @as(u8, index * 2);
        writeReg(low_reg, readReg(low_reg) | REDIR_MASKED);
    }
}

fn programRoute(route: IrqRoute) bool {
    if (!route.present or !route.in_range) return false;
    const low_reg = REDIR_BASE + @as(u8, route.redir_index * 2);
    const high_reg = low_reg + 1;
    var low: u32 = route.vector;
    if (isActiveLow(route.flags)) low |= REDIR_POLARITY_LOW;
    if (isLevelTriggered(route.flags)) low |= REDIR_TRIGGER_LEVEL;
    writeReg(high_reg, @as(u32, route.target_lapic) << 24);
    writeReg(low_reg, low);
    return true;
}

fn refreshKnownRedirection(index: u8) void {
    if (current.route0.in_range and current.route0.redir_index == index) {
        current.redir0 = readRedirectionEntryIfPresent(index);
    }
    if (current.route1.in_range and current.route1.redir_index == index) {
        current.redir1 = readRedirectionEntryIfPresent(index);
    }
    if (current.route2.in_range and current.route2.redir_index == index) {
        current.redir2 = readRedirectionEntryIfPresent(index);
    }
    if (current.route12.in_range and current.route12.redir_index == index) {
        current.redir12 = readRedirectionEntryIfPresent(index);
    }
}

fn dumpRedir(label: []const u8, entry: RedirectionEntry) void {
    k.puts(label);
    if (!entry.present) {
        k.puts("n/a\r\n");
        return;
    }
    k.puts("vec=0x");
    k.putHex(entry.low & 0xFF, 2);
    k.puts(" masked=");
    k.puts(if ((entry.low & REDIR_MASKED) != 0) "yes" else "no");
    k.puts(" low=0x");
    k.putHex(entry.low, 8);
    k.puts(" high=0x");
    k.putHex(entry.high, 8);
    k.puts("\r\n");
}

fn dumpRoute(label: []const u8, route: IrqRoute) void {
    k.puts(label);
    if (!route.present) {
        k.puts("n/a\r\n");
        return;
    }
    k.puts("gsi=");
    k.putDec(route.gsi);
    k.puts(" redir=");
    if (route.in_range) {
        k.putDec(route.redir_index);
    } else {
        k.puts("out-of-range");
    }
    k.puts(" vec=0x");
    k.putHex(route.vector, 2);
    k.puts(" lapic=");
    k.putDec(route.target_lapic);
    k.puts(" pol=");
    k.puts(polarityName(route.flags));
    k.puts(" trig=");
    k.puts(triggerName(route.flags));
    k.puts(" src=");
    k.puts(if (route.override) "ISO" else "default");
    k.puts("\r\n");
}

fn polarityName(flags: u16) []const u8 {
    return switch (flags & 0x3) {
        0 => "bus",
        1 => "high",
        3 => "low",
        else => "reserved",
    };
}

fn triggerName(flags: u16) []const u8 {
    return switch ((flags >> 2) & 0x3) {
        0 => "bus",
        1 => "edge",
        3 => "level",
        else => "reserved",
    };
}

fn isActiveLow(flags: u16) bool {
    return (flags & 0x3) == 3;
}

fn isLevelTriggered(flags: u16) bool {
    return ((flags >> 2) & 0x3) == 3;
}

fn minU32(value: u32, comptime limit: usize) usize {
    return if (value < limit) @intCast(value) else limit;
}

fn logStatus() void {
    bootlog.puts("[IOAPIC] ");
    bootlog.puts(if (current.available) "available" else "missing");
    bootlog.puts(" mapped=");
    bootlog.puts(if (current.mapped) "yes" else "no");
    bootlog.puts(" routing=");
    bootlog.puts(if (current.routing_active) "active" else "off");
    bootlog.puts(" id=");
    bootlog.putDec(current.reg_id);
    bootlog.puts("/");
    bootlog.putDec(current.madt_id);
    bootlog.puts(" ver=0x");
    bootlog.putHex(current.version, 2);
    bootlog.puts(" redirs=");
    if (current.mapped) {
        bootlog.putDec(@as(u64, current.max_redirection_entry) + 1);
    } else {
        bootlog.putDec(0);
    }
    bootlog.puts(" gsi_base=");
    bootlog.putDec(current.gsi_base);
    bootlog.puts(" route_irq0_gsi=");
    bootlog.putDec(current.route0.gsi);
    bootlog.puts(" route_irq1_gsi=");
    bootlog.putDec(current.route1.gsi);
    bootlog.puts(" route_irq12_gsi=");
    bootlog.putDec(current.route12.gsi);
    bootlog.puts(" reason=");
    bootlog.puts(current.reason);
    bootlog.puts("\r\n");
}
