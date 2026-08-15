const bootlog = @import("../../kernel/bootlog.zig");

pub const MAX_CONTROLLERS: usize = 4;
pub const MAX_NAME: usize = 32;

pub const Source = enum {
    builtin,
    preload,
    disk,
};

pub const State = enum {
    registered,
    active,
    failed,
};

pub const Status = extern struct {
    state: u32 = 0,
    source: u32 = 0,
    ports: u32 = 0,
    devices: u32 = 0,
    transfers: u64 = 0,
    failures: u64 = 0,
};

pub const DeviceHandle = extern struct {
    controller_id: u32 = 0,
    port: u8 = 0,
    slot_id: u8 = 0,
    speed: u8 = 0,
    config_value: u8 = 0,
    vendor_id: u16 = 0,
    product_id: u16 = 0,
};

pub const EndpointHandle = extern struct {
    device: DeviceHandle = .{},
    kind: u32 = 0,
    address: u8 = 0,
    endpoint_id: u8 = 0,
    max_packet: u16 = 0,
    interval: u8 = 0,
};

pub const ControlRequest = extern struct {
    request_type: u8 = 0,
    request: u8 = 0,
    value: u16 = 0,
    index_value: u16 = 0,
    length: u16 = 0,
    direction: u32 = 0,
};

pub const Descriptor = extern struct {
    version: u32,
    size: u32,
    flags: u32,
    source: u32,
    context: ?*anyopaque,
    port_scan: ?*const fn (?*anyopaque) callconv(.c) i32,
    address_device: ?*const fn (?*anyopaque, u8, *DeviceHandle) callconv(.c) i32,
    configure_device: ?*const fn (?*anyopaque, *const DeviceHandle, u8) callconv(.c) i32,
    control_transfer: ?*const fn (?*anyopaque, *const DeviceHandle, *const ControlRequest, [*]u8, u32) callconv(.c) i32,
    bulk_transfer: ?*const fn (?*anyopaque, *const EndpointHandle, [*]u8, u32, u32) callconv(.c) i32,
    interrupt_transfer: ?*const fn (?*anyopaque, *const EndpointHandle, [*]u8, u32, *u32) callconv(.c) i32,
    reset_port: ?*const fn (?*anyopaque, u8) callconv(.c) i32,
    clear_halt: ?*const fn (?*anyopaque, *const EndpointHandle) callconv(.c) i32,
    reset_endpoint: ?*const fn (?*anyopaque, *const EndpointHandle) callconv(.c) i32,
    poll: ?*const fn (?*anyopaque) callconv(.c) i32,
    shutdown: ?*const fn (?*anyopaque) callconv(.c) i32,
    status: ?*const fn (?*anyopaque, *Status) callconv(.c) i32,
};

pub const Controller = struct {
    used: bool = false,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    source: Source = .builtin,
    owner_id: u32 = 0,
    state: State = .registered,
    descriptor: ?*const Descriptor = null,
    transfers: u64 = 0,
    failures: u64 = 0,
};

var controllers: [MAX_CONTROLLERS]Controller = .{Controller{}} ** MAX_CONTROLLERS;

pub fn reset() void {
    controllers = .{Controller{}} ** MAX_CONTROLLERS;
}

pub fn registerBuiltIn(name: []const u8) ?usize {
    if (findByName(name) != null) return null;
    const index = freeSlot() orelse return null;
    const slot = &controllers[index];
    slot.* = .{
        .used = true,
        .source = .builtin,
        .state = .active,
    };
    copyName(name, slot);
    bootlog.puts("[USBHC] register built-in ");
    bootlog.puts(slot.name[0..slot.name_len]);
    bootlog.puts("\r\n");
    return index;
}

pub fn register(name: []const u8, descriptor: *const Descriptor, owner_id: u32) ?usize {
    if (findByName(name) != null) return null;
    const index = freeSlot() orelse return null;
    const slot = &controllers[index];
    slot.* = .{
        .used = true,
        .source = sourceFromRaw(descriptor.source),
        .owner_id = owner_id,
        .state = .registered,
        .descriptor = descriptor,
    };
    copyName(name, slot);
    bootlog.puts("[USBHC] register backend ");
    bootlog.puts(slot.name[0..slot.name_len]);
    bootlog.puts(" source=");
    bootlog.puts(sourceLabel(slot.source));
    bootlog.puts("\r\n");
    return index;
}

pub fn unregister(index: usize) bool {
    if (index >= controllers.len or !controllers[index].used) return false;
    if (controllers[index].descriptor) |descriptor| {
        if (descriptor.shutdown) |shutdown| _ = shutdown(descriptor.context);
    }
    controllers[index] = .{};
    return true;
}

pub fn unregisterByName(name: []const u8) bool {
    const index = findByName(name) orelse return false;
    return unregister(index);
}

pub fn cleanupOwner(owner_id: u32) u32 {
    if (owner_id == 0) return 0;
    var removed: u32 = 0;
    var index: usize = 0;
    while (index < controllers.len) : (index += 1) {
        if (!controllers[index].used or controllers[index].owner_id != owner_id) continue;
        _ = unregister(index);
        removed += 1;
    }
    return removed;
}

pub fn count() usize {
    var n: usize = 0;
    for (controllers) |controller| {
        if (controller.used) n += 1;
    }
    return n;
}

pub fn at(index: usize) ?*const Controller {
    if (index >= controllers.len or !controllers[index].used) return null;
    return &controllers[index];
}

pub fn findByName(name: []const u8) ?usize {
    var index: usize = 0;
    while (index < controllers.len) : (index += 1) {
        const controller = &controllers[index];
        if (!controller.used) continue;
        if (eqIgnoreCase(controller.name[0..controller.name_len], name)) return index;
    }
    return null;
}

pub fn sourceLabel(source: Source) []const u8 {
    return switch (source) {
        .builtin => "built-in",
        .preload => "preload",
        .disk => "disk",
    };
}

fn freeSlot() ?usize {
    var index: usize = 0;
    while (index < controllers.len) : (index += 1) {
        if (!controllers[index].used) return index;
    }
    return null;
}

fn sourceFromRaw(source: u32) Source {
    return switch (source) {
        1 => .preload,
        2 => .disk,
        else => .builtin,
    };
}

fn copyName(name: []const u8, slot: *Controller) void {
    const n = if (name.len < slot.name.len) name.len else slot.name.len - 1;
    if (n > 0) @memcpy(slot.name[0..n], name[0..n]);
    slot.name[n] = 0;
    slot.name_len = n;
}

fn eqIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn upper(ch: u8) u8 {
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
}
