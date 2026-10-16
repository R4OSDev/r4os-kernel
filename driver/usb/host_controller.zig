comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
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

pub const Source = enum(u8) {
    builtin,
    preload,
    disk,
};

pub const State = enum(u8) {
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

pub const Controller = extern struct {
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

pub const CleanupResult = extern struct {
    removed: u32 = 0,
    failed: bool = false,
};

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

comptime {
    if (@sizeOf(Status) != 80 or
        @alignOf(Status) != 8 or
        @offsetOf(Status, "state") != 0 or
        @offsetOf(Status, "source") != 4 or
        @offsetOf(Status, "ports") != 8 or
        @offsetOf(Status, "devices") != 12 or
        @offsetOf(Status, "transfers") != 16 or
        @offsetOf(Status, "failures") != 24 or
        @offsetOf(Status, "flags") != 32 or
        @offsetOf(Status, "queue_depth") != 36 or
        @offsetOf(Status, "reserved0") != 38 or
        @offsetOf(Status, "max_transfer_bytes") != 40 or
        @offsetOf(Status, "active_transfers") != 44 or
        @offsetOf(Status, "completions") != 48 or
        @offsetOf(Status, "interrupts") != 56 or
        @offsetOf(Status, "polls") != 64 or
        @offsetOf(Status, "cancellations") != 72) @compileError("USB Status private C layout");
}

comptime {
    if (@sizeOf(DeviceHandle) != 12 or
        @alignOf(DeviceHandle) != 4 or
        @offsetOf(DeviceHandle, "controller_id") != 0 or
        @offsetOf(DeviceHandle, "port") != 4 or
        @offsetOf(DeviceHandle, "slot_id") != 5 or
        @offsetOf(DeviceHandle, "speed") != 6 or
        @offsetOf(DeviceHandle, "config_value") != 7 or
        @offsetOf(DeviceHandle, "vendor_id") != 8 or
        @offsetOf(DeviceHandle, "product_id") != 10) @compileError("USB DeviceHandle private C layout");
}

comptime {
    if (@sizeOf(EndpointHandle) != 24 or
        @alignOf(EndpointHandle) != 4 or
        @offsetOf(EndpointHandle, "device") != 0 or
        @offsetOf(EndpointHandle, "kind") != 12 or
        @offsetOf(EndpointHandle, "address") != 16 or
        @offsetOf(EndpointHandle, "endpoint_id") != 17 or
        @offsetOf(EndpointHandle, "max_packet") != 18 or
        @offsetOf(EndpointHandle, "interval") != 20 or
        @offsetOf(EndpointHandle, "max_burst") != 21) @compileError("USB EndpointHandle private C layout");
}

comptime {
    if (@sizeOf(ControlRequest) != 12 or
        @alignOf(ControlRequest) != 4 or
        @offsetOf(ControlRequest, "request_type") != 0 or
        @offsetOf(ControlRequest, "request") != 1 or
        @offsetOf(ControlRequest, "value") != 2 or
        @offsetOf(ControlRequest, "index_value") != 4 or
        @offsetOf(ControlRequest, "length") != 6 or
        @offsetOf(ControlRequest, "direction") != 8) @compileError("USB ControlRequest private C layout");
}

comptime {
    if (@sizeOf(Descriptor) != 120 or
        @alignOf(Descriptor) != 8 or
        @offsetOf(Descriptor, "version") != 0 or
        @offsetOf(Descriptor, "size") != 4 or
        @offsetOf(Descriptor, "flags") != 8 or
        @offsetOf(Descriptor, "source") != 12 or
        @offsetOf(Descriptor, "context") != 16 or
        @offsetOf(Descriptor, "port_scan") != 24 or
        @offsetOf(Descriptor, "address_device") != 32 or
        @offsetOf(Descriptor, "configure_device") != 40 or
        @offsetOf(Descriptor, "control_transfer") != 48 or
        @offsetOf(Descriptor, "bulk_transfer") != 56 or
        @offsetOf(Descriptor, "interrupt_transfer") != 64 or
        @offsetOf(Descriptor, "reset_port") != 72 or
        @offsetOf(Descriptor, "clear_halt") != 80 or
        @offsetOf(Descriptor, "reset_endpoint") != 88 or
        @offsetOf(Descriptor, "poll") != 96 or
        @offsetOf(Descriptor, "shutdown") != 104 or
        @offsetOf(Descriptor, "status") != 112) @compileError("USB Descriptor private C layout");
}

comptime {
    if (@sizeOf(Controller) != 96 or
        @alignOf(Controller) != 8 or
        @offsetOf(Controller, "used") != 0 or
        @offsetOf(Controller, "name") != 1 or
        @offsetOf(Controller, "name_len") != 40 or
        @offsetOf(Controller, "source") != 48 or
        @offsetOf(Controller, "owner_id") != 52 or
        @offsetOf(Controller, "state") != 56 or
        @offsetOf(Controller, "descriptor") != 64 or
        @offsetOf(Controller, "transfers") != 72 or
        @offsetOf(Controller, "failures") != 80 or
        @offsetOf(Controller, "polls") != 88) @compileError("USB Controller private C layout");
}

comptime {
    if (@sizeOf(CleanupResult) != 8 or
        @alignOf(CleanupResult) != 4 or
        @offsetOf(CleanupResult, "removed") != 0 or
        @offsetOf(CleanupResult, "failed") != 4) @compileError("USB CleanupResult private C layout");
}

extern fn r4os_rust_usb_host_reset() void;
extern fn r4os_rust_usb_host_register([*]const u8, usize, *const Descriptor, u32, bool, ?*const fn ([*]const u8, usize) callconv(.c) void) usize;
extern fn r4os_rust_usb_host_set_state(usize, u8) bool;
extern fn r4os_rust_usb_host_id(usize) u32;
extern fn r4os_rust_usb_host_index(u32) usize;
extern fn r4os_rust_usb_host_unregister(usize) bool;
extern fn r4os_rust_usb_host_unregister_name([*]const u8, usize, u32) bool;
extern fn r4os_rust_usb_host_cleanup(u32, *CleanupResult) void;
extern fn r4os_rust_usb_host_count() usize;
extern fn r4os_rust_usb_host_at(usize, *Controller) bool;
extern fn r4os_rust_usb_host_find([*]const u8, usize) usize;
extern fn r4os_rust_usb_host_status(usize, *Status) i32;
extern fn r4os_rust_usb_host_scan(usize) i32;
extern fn r4os_rust_usb_host_poll(usize) i32;
extern fn r4os_rust_usb_host_address(usize, u8, *DeviceHandle) i32;
extern fn r4os_rust_usb_host_configure(*const DeviceHandle, u8) i32;
extern fn r4os_rust_usb_host_control(*const DeviceHandle, *const ControlRequest, [*]u8, usize) i32;
extern fn r4os_rust_usb_host_bulk(*const EndpointHandle, [*]u8, usize, u32) i32;
extern fn r4os_rust_usb_host_interrupt(*const EndpointHandle, [*]u8, usize, *u32) i32;
extern fn r4os_rust_usb_host_reset_port(usize, u8) i32;
extern fn r4os_rust_usb_host_clear_halt(*const EndpointHandle) i32;
extern fn r4os_rust_usb_host_reset_endpoint(*const EndpointHandle) i32;
fn optionalIndex(index: usize) ?usize {
    return if (index == @import("std").math.maxInt(usize)) null else index;
}
pub fn reset() void {
    r4os_rust_usb_host_reset();
}
pub fn registerBuiltIn(name: []const u8, descriptor: *const Descriptor) ?usize {
    return optionalIndex(r4os_rust_usb_host_register(name.ptr, name.len, descriptor, 0, true, if (builtin.is_test) null else logRegistration));
}
pub fn register(name: []const u8, descriptor: *const Descriptor, owner_id: u32) ?usize {
    return optionalIndex(r4os_rust_usb_host_register(name.ptr, name.len, descriptor, owner_id, false, if (builtin.is_test) null else logRegistration));
}
pub fn setState(index: usize, state: State) bool {
    return r4os_rust_usb_host_set_state(index, @intFromEnum(state));
}
pub fn controllerId(index: usize) u32 {
    return r4os_rust_usb_host_id(index);
}
pub fn indexFromControllerId(id: u32) ?usize {
    return optionalIndex(r4os_rust_usb_host_index(id));
}
pub fn unregister(index: usize) bool {
    return r4os_rust_usb_host_unregister(index);
}
pub fn unregisterByName(name: []const u8, owner_id: u32) bool {
    return r4os_rust_usb_host_unregister_name(name.ptr, name.len, owner_id);
}
pub fn cleanupOwner(owner_id: u32) CleanupResult {
    var out: CleanupResult = undefined;
    r4os_rust_usb_host_cleanup(owner_id, &out);
    return out;
}
pub fn count() usize {
    return r4os_rust_usb_host_count();
}
pub fn at(index: usize) ?Controller {
    var out: Controller = undefined;
    return if (r4os_rust_usb_host_at(index, &out)) out else null;
}
pub fn statusAt(index: usize, out: *Status) i32 {
    return r4os_rust_usb_host_status(index, out);
}
pub fn scan(index: usize) i32 {
    return r4os_rust_usb_host_scan(index);
}
pub fn addressDevice(index: usize, port: u8, out: *DeviceHandle) i32 {
    return r4os_rust_usb_host_address(index, port, out);
}
pub fn configureDevice(device: *const DeviceHandle, configuration: u8) i32 {
    return r4os_rust_usb_host_configure(device, configuration);
}
pub fn controlTransfer(device: *const DeviceHandle, request: *const ControlRequest, buffer: []u8) i32 {
    return r4os_rust_usb_host_control(device, request, buffer.ptr, buffer.len);
}
pub fn bulkTransfer(endpoint: *const EndpointHandle, buffer: []u8, direction: u32) i32 {
    return r4os_rust_usb_host_bulk(endpoint, buffer.ptr, buffer.len, direction);
}
pub fn interruptTransfer(endpoint: *const EndpointHandle, buffer: []u8, actual: *u32) i32 {
    return r4os_rust_usb_host_interrupt(endpoint, buffer.ptr, buffer.len, actual);
}
pub fn resetPort(index: usize, port: u8) i32 {
    return r4os_rust_usb_host_reset_port(index, port);
}
pub fn clearHalt(endpoint: *const EndpointHandle) i32 {
    return r4os_rust_usb_host_clear_halt(endpoint);
}
pub fn resetEndpoint(endpoint: *const EndpointHandle) i32 {
    return r4os_rust_usb_host_reset_endpoint(endpoint);
}
pub fn poll(index: usize) i32 {
    return r4os_rust_usb_host_poll(index);
}
pub fn findByName(name: []const u8) ?usize {
    return optionalIndex(r4os_rust_usb_host_find(name.ptr, name.len));
}
pub fn sourceLabel(source: Source) []const u8 {
    return switch (source) {
        .builtin => "built-in",
        .preload => "preload",
        .disk => "disk",
    };
}

fn logRegistration(text: [*]const u8, len: usize) callconv(.c) void {
    @import("../../kernel/bootlog.zig").puts(text[0..len]);
}

// Private metadata admission only. Fixtures run the exact bounded algorithms
// in one host thread; they neither execute privileged guards nor simulate IRQs.
export fn r4os_zig_usb_host_metadata_enter() callconv(.c) u64 {
    if (builtin.is_test) return 0;
    return @import("../../arch/x86_64/interrupts.zig").saveAndDisableRuntime();
}
export fn r4os_zig_usb_host_metadata_leave(flags: u64) callconv(.c) void {
    if (!builtin.is_test) @import("../../arch/x86_64/interrupts.zig").restore(flags);
}
