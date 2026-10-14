// Private C facade. MMIO, route construction and mutation have one Rust owner.
const acpi = @import("../../platform/acpi.zig");
const bootlog = @import("../../kernel/bootlog.zig");
const k = @import("../../kernel/log.zig");
const lapic = @import("lapic.zig");
const percpu = @import("percpu.zig");
const REDIR_MASKED: u32 = 1 << 16;
pub const RedirectionEntry = extern struct {
    low: u32 = 0,
    high: u32 = 0,
    present: bool = false,
};
pub const IrqRoute = extern struct {
    gsi: u32 = 0,
    flags: u16 = 0,
    irq: u8 = 0,
    vector: u8 = 0,
    target_lapic: u8 = 0,
    redir_index: u8 = 0,
    present: bool = false,
    in_range: bool = false,
    override: bool = false,
};
const Iso = extern struct { gsi: u32 = 0, flags: u16 = 0, bus: u8 = 0, source: u8 = 0 };
const Input = extern struct {
    phys_base: u32,
    gsi_base: u32,
    ioapic_count: u32,
    iso_count: u32,
    madt_id: u8,
    bsp_id: u8,
    reserved: [2]u8 = .{ 0, 0 },
    isos: [16]Iso = [_]Iso{.{}} ** 16,
};
pub const RawStatus = extern struct {
    phys_base: u64,
    virt_base: u64,
    gsi_base: u32,
    flags: u32,
    madt_id: u8,
    reg_id: u8,
    version: u8,
    max_redirection_entry: u8,
    programmed_routes: u8,
    reason: u8,
    reserved: [2]u8,
    redirections: [4]RedirectionEntry,
    routes: [4]IrqRoute,
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

extern fn r4os_rust_ioapic_init(input: *const Input, output: *RawStatus) void;
extern fn r4os_rust_ioapic_status(output: *RawStatus) void;
extern fn r4os_rust_ioapic_control(operation: u8, irq: u8, masked: bool) bool;
extern fn r4os_rust_ioapic_route(irq: u8, output: *IrqRoute) void;
extern fn r4os_rust_ioapic_redirection(index: u8, output: *RedirectionEntry) void;
pub fn project(raw: RawStatus) Status {
    return .{ .available = raw.flags & 1 != 0, .mapped = raw.flags & 2 != 0, .routing_active = raw.flags & 4 != 0, .madt_id = raw.madt_id, .reg_id = raw.reg_id, .version = raw.version, .max_redirection_entry = raw.max_redirection_entry, .phys_base = raw.phys_base, .virt_base = raw.virt_base, .gsi_base = raw.gsi_base, .programmed_routes = raw.programmed_routes, .redir0 = raw.redirections[0], .redir1 = raw.redirections[1], .redir2 = raw.redirections[2], .redir12 = raw.redirections[3], .route0 = raw.routes[0], .route1 = raw.routes[1], .route2 = raw.routes[2], .route12 = raw.routes[3], .reason = switch (raw.reason) {
        0 => "not initialized",
        1 => "MADT has no IOAPIC",
        2 => "IOAPIC MMIO mapping failed",
        3 => "MMIO mapped, redirection table visible, routing unchanged",
        4 => "legacy IRQ0/1/12 routed through IOAPIC",
        5 => "IOAPIC routing incomplete, PIC fallback required",
        else => unreachable,
    } };
}
fn makeInput(info: acpi.Info) Input {
    var input = Input{ .phys_base = info.madt_first_ioapic_address, .gsi_base = info.madt_first_ioapic_gsi_base, .ioapic_count = info.madt_ioapic_count, .iso_count = info.madt_iso_count, .madt_id = info.madt_first_ioapic_id, .bsp_id = lapic.status().id };
    for (0..@min(@as(usize, info.madt_iso_count), 16)) |index| {
        const iso = info.madt_isos[index];
        input.isos[index] = .{ .gsi = iso.gsi, .flags = iso.flags, .bus = iso.bus, .source = iso.source };
    }
    return input;
}
export fn r4os_zig_ioapic_inputs(output: *Input) callconv(.c) void {
    output.* = makeInput(acpi.info());
}
export fn r4os_zig_ioapic_cpu_mask() callconv(.c) u64 {
    return percpu.schedulableMask();
}
export fn r4os_zig_ioapic_cpu_id(index: u32, output: *u32) callconv(.c) bool {
    if (!percpu.isSchedulable(index)) return false;
    output.* = percpu.apicId(index) orelse return false;
    return true;
}
pub fn initFromAcpi(info: acpi.Info) Status {
    const input = makeInput(info);
    var raw: RawStatus = undefined;
    r4os_rust_ioapic_init(&input, &raw);
    return project(raw);
}
pub fn status() Status {
    var raw: RawStatus = undefined;
    r4os_rust_ioapic_status(&raw);
    return project(raw);
}
pub fn isRoutingActive() bool {
    return r4os_rust_ioapic_control(0, 0, false);
}
pub fn readRedirectionEntry(index: u8) RedirectionEntry {
    var entry: RedirectionEntry = undefined;
    r4os_rust_ioapic_redirection(index, &entry);
    return entry;
}
pub fn setLegacyIrqMasked(irq: u8, masked: bool) bool {
    return r4os_rust_ioapic_control(1, irq, masked);
}
pub fn activateLegacyIrq(irq: u8) bool {
    return r4os_rust_ioapic_control(2, irq, false);
}
pub fn activatePciIntxIrq(irq: u8) bool {
    return r4os_rust_ioapic_control(3, irq, false);
}
pub fn activateLegacyRoutes() bool {
    return r4os_rust_ioapic_control(4, 0, false);
}
pub fn routeForIrq(irq: u8) IrqRoute {
    var route: IrqRoute = undefined;
    r4os_rust_ioapic_route(irq, &route);
    return route;
}
comptime {
    if (percpu.max_cpus != 32 or acpi.MAX_MADT_ISO != 16 or @sizeOf(Input) != 148 or @offsetOf(Input, "isos") != 20 or
        @sizeOf(RedirectionEntry) != 12 or @sizeOf(IrqRoute) != 16 or @sizeOf(RawStatus) != 144 or @offsetOf(RawStatus, "routes") != 80)
        @compileError("Rust IOAPIC C layout or platform bound differs");
}

pub fn dumpStatus() void {
    const s = status();
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

fn logStatus(s: Status) void {
    bootlog.puts("[IOAPIC] ");
    bootlog.puts(if (s.available) "available" else "missing");
    bootlog.puts(" mapped=");
    bootlog.puts(if (s.mapped) "yes" else "no");
    bootlog.puts(" routing=");
    bootlog.puts(if (s.routing_active) "active" else "off");
    bootlog.puts(" id=");
    bootlog.putDec(s.reg_id);
    bootlog.puts("/");
    bootlog.putDec(s.madt_id);
    bootlog.puts(" ver=0x");
    bootlog.putHex(s.version, 2);
    bootlog.puts(" redirs=");
    if (s.mapped) {
        bootlog.putDec(@as(u64, s.max_redirection_entry) + 1);
    } else {
        bootlog.putDec(0);
    }
    bootlog.puts(" gsi_base=");
    bootlog.putDec(s.gsi_base);
    bootlog.puts(" route_irq0_gsi=");
    bootlog.putDec(s.route0.gsi);
    bootlog.puts(" route_irq1_gsi=");
    bootlog.putDec(s.route1.gsi);
    bootlog.puts(" route_irq12_gsi=");
    bootlog.putDec(s.route12.gsi);
    bootlog.puts(" reason=");
    bootlog.puts(s.reason);
    bootlog.puts("\r\n");
}

export fn r4os_zig_ioapic_log(event: u8, raw: *const RawStatus) callconv(.c) void {
    if (comptime @import("builtin").is_test) return;
    const s = project(raw.*);
    switch (event) {
        0 => logStatus(s),
        1 => {
            bootlog.puts("[IOAPIC] routing=");
            bootlog.puts(if (s.routing_active) "active" else "incomplete");
            bootlog.puts(" programmed=");
            bootlog.putDec(s.programmed_routes);
            bootlog.puts(" irq0=");
            bootlog.putDec(s.route0.gsi);
            bootlog.puts(" irq1=");
            bootlog.putDec(s.route1.gsi);
            bootlog.puts(" irq12=");
            bootlog.putDec(s.route12.gsi);
            bootlog.puts("\r\n");
        },
        2 => bootlog.puts("[IOAPIC][WARN] legacy route activation skipped, IOAPIC not mapped\r\n"),
        3 => bootlog.puts("[IOAPIC][WARN] legacy route activation skipped, route out of range\r\n"),
        else => unreachable,
    }
}
