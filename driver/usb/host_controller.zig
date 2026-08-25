const bootlog = @import("../../kernel/bootlog.zig");
const builtin = @import("builtin");

pub const MAX_CONTROLLERS: usize = 4;
pub const MAX_NAME: usize = 32;

pub const BACKEND_VERSION: u32 = 2;
pub const FLAG_PORT_SCAN: u32 = 1 << 0;
pub const FLAG_CONTROL: u32 = 1 << 1;
pub const FLAG_BULK: u32 = 1 << 2;
pub const FLAG_INTERRUPT: u32 = 1 << 3;
pub const FLAG_EVENT_IRQ: u32 = 1 << 4;
pub const FLAG_POLL_FALLBACK: u32 = 1 << 5;
pub const FLAG_MULTI_TRANSFER: u32 = 1 << 6;
pub const FLAG_HOTPLUG: u32 = 1 << 7;

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
    flags: u32 = 0,
    queue_depth: u16 = 0,
    reserved0: u16 = 0,
    max_transfer_bytes: u32 = 0,
    active_transfers: u32 = 0,
    completions: u64 = 0,
    interrupts: u64 = 0,
    polls: u64 = 0,
    cancellations: u64 = 0,
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
    max_burst: u8 = 0,
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
    version: u32 = BACKEND_VERSION,
    size: u32 = @sizeOf(Descriptor),
    flags: u32 = 0,
    source: u32 = 0,
    context: ?*anyopaque = null,
    port_scan: ?*const fn (?*anyopaque) callconv(.c) i32 = null,
    address_device: ?*const fn (?*anyopaque, u8, *DeviceHandle) callconv(.c) i32 = null,
    configure_device: ?*const fn (?*anyopaque, *const DeviceHandle, u8) callconv(.c) i32 = null,
    control_transfer: ?*const fn (?*anyopaque, *const DeviceHandle, *const ControlRequest, [*]u8, u32) callconv(.c) i32 = null,
    bulk_transfer: ?*const fn (?*anyopaque, *const EndpointHandle, [*]u8, u32, u32) callconv(.c) i32 = null,
    interrupt_transfer: ?*const fn (?*anyopaque, *const EndpointHandle, [*]u8, u32, *u32) callconv(.c) i32 = null,
    reset_port: ?*const fn (?*anyopaque, u8) callconv(.c) i32 = null,
    clear_halt: ?*const fn (?*anyopaque, *const EndpointHandle) callconv(.c) i32 = null,
    reset_endpoint: ?*const fn (?*anyopaque, *const EndpointHandle) callconv(.c) i32 = null,
    poll: ?*const fn (?*anyopaque) callconv(.c) i32 = null,
    shutdown: ?*const fn (?*anyopaque) callconv(.c) i32 = null,
    status: ?*const fn (?*anyopaque, *Status) callconv(.c) i32 = null,
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
    polls: u64 = 0,
};

pub const CleanupResult = struct {
    removed: u32 = 0,
    failed: bool = false,
};

var controllers: [MAX_CONTROLLERS]Controller = .{Controller{}} ** MAX_CONTROLLERS;

pub fn reset() void {
    controllers = .{Controller{}} ** MAX_CONTROLLERS;
}

pub fn registerBuiltIn(name: []const u8, descriptor: *const Descriptor) ?usize {
    if (findByName(name) != null) return null;
    const index = freeSlot() orelse return null;
    const slot = &controllers[index];
    slot.* = .{
        .used = true,
        .source = .builtin,
        .state = .active,
        .descriptor = descriptor,
    };
    copyName(name, slot);
    if (!builtin.is_test) {
        bootlog.puts("[USBHC] register built-in ");
        bootlog.puts(slot.name[0..slot.name_len]);
        bootlog.puts("\r\n");
    }
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
    if (!builtin.is_test) {
        bootlog.puts("[USBHC] register backend ");
        bootlog.puts(slot.name[0..slot.name_len]);
        bootlog.puts(" source=");
        bootlog.puts(sourceLabel(slot.source));
        bootlog.puts("\r\n");
    }
    return index;
}

pub fn setState(index: usize, state: State) bool {
    if (index >= controllers.len or !controllers[index].used) return false;
    controllers[index].state = state;
    return true;
}

pub fn controllerId(index: usize) u32 {
    return @intCast(index + 1);
}

pub fn indexFromControllerId(id: u32) ?usize {
    if (id == 0 or id > controllers.len) return null;
    const index: usize = @intCast(id - 1);
    if (!controllers[index].used) return null;
    return index;
}

pub fn unregister(index: usize) bool {
    if (index >= controllers.len or !controllers[index].used) return false;
    if (controllers[index].descriptor) |descriptor| {
        if (descriptor.shutdown) |shutdown| {
            if (shutdown(descriptor.context) < 0) {
                controllers[index].state = .failed;
                controllers[index].failures +%= 1;
                return false;
            }
        }
    }
    controllers[index] = .{};
    return true;
}

pub fn unregisterByName(name: []const u8, owner_id: u32) bool {
    const index = findByName(name) orelse return false;
    if (controllers[index].owner_id != owner_id) return false;
    return unregister(index);
}

pub fn cleanupOwner(owner_id: u32) CleanupResult {
    if (owner_id == 0) return .{};
    var result: CleanupResult = .{};
    var index: usize = 0;
    while (index < controllers.len) : (index += 1) {
        if (!controllers[index].used or controllers[index].owner_id != owner_id) continue;
        if (unregister(index)) {
            result.removed += 1;
        } else {
            result.failed = true;
        }
    }
    return result;
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

pub fn statusAt(index: usize, out: *Status) i32 {
    const slot = mutableAt(index) orelse return -1;
    out.* = .{
        .state = stateRaw(slot.state),
        .source = sourceRaw(slot.source),
        .transfers = slot.transfers,
        .failures = slot.failures,
        .polls = slot.polls,
    };
    const descriptor = slot.descriptor orelse return 0;
    if (descriptor.status) |callback| {
        const result = callback(descriptor.context, out);
        if (result < 0) {
            slot.failures +%= 1;
            return result;
        }
    }
    out.state = stateRaw(slot.state);
    out.source = sourceRaw(slot.source);
    out.flags |= descriptor.flags;
    if (out.transfers < slot.transfers) out.transfers = slot.transfers;
    if (out.failures < slot.failures) out.failures = slot.failures;
    if (out.polls < slot.polls) out.polls = slot.polls;
    return 0;
}

pub fn scan(index: usize) i32 {
    const slot = mutableAt(index) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.port_scan orelse return fail(slot, -3);
    const result = callback(descriptor.context);
    return account(slot, result, false);
}

pub fn addressDevice(index: usize, port: u8, out: *DeviceHandle) i32 {
    const slot = mutableAt(index) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.address_device orelse return fail(slot, -3);
    const result = callback(descriptor.context, port, out);
    if (result >= 0) out.controller_id = controllerId(index);
    return account(slot, result, false);
}

pub fn configureDevice(device: *const DeviceHandle, configuration: u8) i32 {
    const slot = controllerForDevice(device) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.configure_device orelse return fail(slot, -3);
    return account(slot, callback(descriptor.context, device, configuration), false);
}

pub fn controlTransfer(device: *const DeviceHandle, request: *const ControlRequest, buffer: []u8) i32 {
    const slot = controllerForDevice(device) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.control_transfer orelse return fail(slot, -3);
    return account(slot, callback(descriptor.context, device, request, buffer.ptr, @intCast(buffer.len)), true);
}

pub fn bulkTransfer(endpoint: *const EndpointHandle, buffer: []u8, direction: u32) i32 {
    const slot = controllerForDevice(&endpoint.device) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.bulk_transfer orelse return fail(slot, -3);
    return account(slot, callback(descriptor.context, endpoint, buffer.ptr, @intCast(buffer.len), direction), true);
}

pub fn interruptTransfer(endpoint: *const EndpointHandle, buffer: []u8, actual: *u32) i32 {
    const slot = controllerForDevice(&endpoint.device) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.interrupt_transfer orelse return fail(slot, -3);
    return account(slot, callback(descriptor.context, endpoint, buffer.ptr, @intCast(buffer.len), actual), true);
}

pub fn resetPort(index: usize, port: u8) i32 {
    const slot = mutableAt(index) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.reset_port orelse return fail(slot, -3);
    return account(slot, callback(descriptor.context, port), false);
}

pub fn clearHalt(endpoint: *const EndpointHandle) i32 {
    const slot = controllerForDevice(&endpoint.device) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.clear_halt orelse return fail(slot, -3);
    return account(slot, callback(descriptor.context, endpoint), false);
}

pub fn resetEndpoint(endpoint: *const EndpointHandle) i32 {
    const slot = controllerForDevice(&endpoint.device) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.reset_endpoint orelse return fail(slot, -3);
    return account(slot, callback(descriptor.context, endpoint), false);
}

pub fn poll(index: usize) i32 {
    const slot = mutableAt(index) orelse return -1;
    const descriptor = slot.descriptor orelse return fail(slot, -2);
    const callback = descriptor.poll orelse return fail(slot, -3);
    slot.polls +%= 1;
    return account(slot, callback(descriptor.context), false);
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

fn mutableAt(index: usize) ?*Controller {
    if (index >= controllers.len or !controllers[index].used) return null;
    return &controllers[index];
}

fn controllerForDevice(device: *const DeviceHandle) ?*Controller {
    const index = indexFromControllerId(device.controller_id) orelse return null;
    return mutableAt(index);
}

fn account(slot: *Controller, result: i32, transfer: bool) i32 {
    if (transfer) slot.transfers +%= 1;
    if (result < 0) slot.failures +%= 1;
    return result;
}

fn fail(slot: *Controller, result: i32) i32 {
    slot.failures +%= 1;
    return result;
}

fn stateRaw(state: State) u32 {
    return switch (state) {
        .registered => 0,
        .active => 1,
        .failed => 2,
    };
}

fn sourceRaw(source: Source) u32 {
    return switch (source) {
        .builtin => 0,
        .preload => 1,
        .disk => 2,
    };
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

var test_shutdown_ok = true;
var test_shutdowns: u32 = 0;

fn testPortScan(_: ?*anyopaque) callconv(.c) i32 {
    return 1;
}

fn testAddress(_: ?*anyopaque, port: u8, out: *DeviceHandle) callconv(.c) i32 {
    out.* = .{ .port = port, .slot_id = 7, .speed = 3 };
    return 0;
}

fn testConfigure(_: ?*anyopaque, _: *const DeviceHandle, _: u8) callconv(.c) i32 {
    return 0;
}

fn testControl(_: ?*anyopaque, _: *const DeviceHandle, _: *const ControlRequest, _: [*]u8, len: u32) callconv(.c) i32 {
    return @intCast(len);
}

fn testBulk(_: ?*anyopaque, _: *const EndpointHandle, _: [*]u8, len: u32, _: u32) callconv(.c) i32 {
    return @intCast(len);
}

fn testInterrupt(_: ?*anyopaque, _: *const EndpointHandle, _: [*]u8, len: u32, actual: *u32) callconv(.c) i32 {
    actual.* = len;
    return 1;
}

fn testEndpoint(_: ?*anyopaque, _: *const EndpointHandle) callconv(.c) i32 {
    return 0;
}

fn testPort(_: ?*anyopaque, _: u8) callconv(.c) i32 {
    return 0;
}

fn testPoll(_: ?*anyopaque) callconv(.c) i32 {
    return 1;
}

fn testShutdown(_: ?*anyopaque) callconv(.c) i32 {
    test_shutdowns += 1;
    return if (test_shutdown_ok) 0 else -1;
}

fn testStatus(_: ?*anyopaque, out: *Status) callconv(.c) i32 {
    out.queue_depth = 8;
    out.max_transfer_bytes = 65_536;
    out.active_transfers = 2;
    out.completions = 9;
    return 0;
}

fn testDescriptor() Descriptor {
    return .{
        .flags = FLAG_PORT_SCAN | FLAG_CONTROL | FLAG_BULK | FLAG_INTERRUPT | FLAG_POLL_FALLBACK,
        .source = 1,
        .port_scan = testPortScan,
        .address_device = testAddress,
        .configure_device = testConfigure,
        .control_transfer = testControl,
        .bulk_transfer = testBulk,
        .interrupt_transfer = testInterrupt,
        .reset_port = testPort,
        .clear_halt = testEndpoint,
        .reset_endpoint = testEndpoint,
        .poll = testPoll,
        .shutdown = testShutdown,
        .status = testStatus,
    };
}

test "registered USB host dispatches every productive operation and status" {
    const testing = @import("std").testing;
    reset();
    test_shutdown_ok = true;
    test_shutdowns = 0;
    var descriptor = testDescriptor();
    const index = register("TESTHC", &descriptor, 77) orelse return error.NoControllerSlot;
    try testing.expect(setState(index, .active));
    try testing.expectEqual(@as(i32, 1), scan(index));

    var device: DeviceHandle = .{};
    try testing.expectEqual(@as(i32, 0), addressDevice(index, 4, &device));
    try testing.expectEqual(controllerId(index), device.controller_id);
    try testing.expectEqual(@as(u8, 7), device.slot_id);
    try testing.expectEqual(@as(i32, 0), configureDevice(&device, 1));

    var endpoint: EndpointHandle = .{ .device = device, .kind = 2, .endpoint_id = 3 };
    var request: ControlRequest = .{};
    var bytes: [32]u8 = .{0} ** 32;
    var actual: u32 = 0;
    try testing.expectEqual(@as(i32, 32), controlTransfer(&device, &request, bytes[0..]));
    try testing.expectEqual(@as(i32, 32), bulkTransfer(&endpoint, bytes[0..], 1));
    try testing.expectEqual(@as(i32, 1), interruptTransfer(&endpoint, bytes[0..], &actual));
    try testing.expectEqual(@as(u32, 32), actual);
    try testing.expectEqual(@as(i32, 0), resetPort(index, 4));
    try testing.expectEqual(@as(i32, 0), clearHalt(&endpoint));
    try testing.expectEqual(@as(i32, 0), resetEndpoint(&endpoint));
    try testing.expectEqual(@as(i32, 1), poll(index));

    var host_status: Status = .{};
    try testing.expectEqual(@as(i32, 0), statusAt(index, &host_status));
    try testing.expectEqual(@as(u16, 8), host_status.queue_depth);
    try testing.expectEqual(@as(u32, 65_536), host_status.max_transfer_bytes);
    try testing.expectEqual(@as(u64, 3), host_status.transfers);
    try testing.expectEqual(@as(u64, 1), host_status.polls);
    try testing.expect((host_status.flags & FLAG_BULK) != 0);
    try testing.expect(unregisterByName("testhc", 77));
    try testing.expectEqual(@as(u32, 1), test_shutdowns);
    try testing.expectEqual(@as(usize, 0), count());
}

test "failed host shutdown vetoes unregister and owner cleanup" {
    const testing = @import("std").testing;
    reset();
    test_shutdown_ok = false;
    test_shutdowns = 0;
    var descriptor = testDescriptor();
    const index = register("TESTHC", &descriptor, 88) orelse return error.NoControllerSlot;
    try testing.expect(!unregisterByName("TESTHC", 77));
    try testing.expect(!unregister(index));
    try testing.expectEqual(State.failed, at(index).?.state);
    const failed_cleanup = cleanupOwner(88);
    try testing.expect(failed_cleanup.failed);
    try testing.expectEqual(@as(u32, 0), failed_cleanup.removed);
    try testing.expectEqual(@as(usize, 1), count());

    test_shutdown_ok = true;
    const cleanup = cleanupOwner(88);
    try testing.expect(!cleanup.failed);
    try testing.expectEqual(@as(u32, 1), cleanup.removed);
    try testing.expectEqual(@as(usize, 0), count());
}
