const std = @import("std");
const r4os = @import("r4os");
const a = r4os.abi;
var rows: [257]a.AudioOutputInfo = @splat(.{});
var receipt: a.AudioOutputCatalog = .{};
var text: [768]u8 = undefined;
fn capture(sys: r4os.r4sys.Context, audio: r4os.r4audio.Context) i32 {
    for (0..100) |_| {
        receipt = .{};
        const result = audio.audioOutputCatalog(&rows, &receipt);
        if (result != a.service_api_result_busy) return result;
        sys.sleepTicks(@max(sys.ticksFromMilliseconds(10), 1));
    }
    return a.service_api_result_busy;
}
pub fn r4_app_main(app: *r4os.App) i32 {
    const sys = app.system();
    const audio = app.audioLowLevel() orelse return 2;
    const service = app.audio() orelse return 3;
    const rc = capture(sys, audio);
    sys.println(std.fmt.bufPrint(&text, "REAL56 CATALOG result={d} count={d} providers={d} registry={d} activeRegistration={d}", .{ rc, receipt.count, receipt.provider_count, receipt.registry_generation, receipt.active_registration }) catch unreachable);
    if (rc != 1 or receipt.count == 0 or receipt.provider_count != 1 or receipt.active_registration == 0) return 4;
    const witness = receipt.providers[0];
    if (witness.registration != receipt.active_registration or witness.generation == 0 or witness.count != receipt.count) return 5;
    var active: usize = 0;
    var available: ?usize = null;
    for (rows[0..receipt.count], 0..) |row, index| {
        var scalar: a.AudioOutputInfo = .{};
        const result = audio.audioOutputInfo(@intCast(index), &scalar);
        if (result != 1 or !std.mem.eql(u8, std.mem.asBytes(&scalar), std.mem.asBytes(&row))) return 6;
        if (row.flags & a.audio_output_flag_active != 0) active += 1;
        if (row.availability == a.audio_output_available) available = index;
        const end = std.mem.indexOfScalar(u8, &row.id, 0) orelse return 7;
        sys.println(std.fmt.bufPrint(&text, "REAL56 ROW index={d} id={s} available={d} active={d} generation={d} registration={d}", .{ index, row.id[0..end], row.availability, @intFromBool(row.flags & a.audio_output_flag_active != 0), witness.generation, witness.registration }) catch unreachable);
    }
    if (active != 1 or available == null) return 8;
    const id = rows[available.?].id;
    const selected = audio.audioSelectOutput(&id);
    if (selected != 0 or capture(sys, audio) != 1 or receipt.providers[0].registration != witness.registration) return 9;
    var request: a.AudioServiceOutputRequest = .{};
    var page: a.AudioServiceOutputState = .{};
    if (service.outputs(&request, r4os.time_contract.timeoutFinite(.{ .nanoseconds = 2_000_000_000 }), &page) != .ok or page.total != receipt.count or page.count != receipt.count) return 10;
    for (page.outputs[0..page.count], rows[0..receipt.count]) |item, row| if (!std.mem.eql(u8, &item.id, &row.id)) return 11;
    const service_epoch = page.service_epoch;
    request = .{ .id = id };
    if (service.selectOutput(&request, r4os.time_contract.timeoutFinite(.{ .nanoseconds = 2_000_000_000 }), &page) != .ok or
        page.service_epoch != service_epoch or !std.mem.eql(u8, &page.desired_id, &id) or !std.mem.eql(u8, &page.active_id, &id)) return 12;
    var status: a.AudioServiceStatus = .{};
    if (service.status(r4os.time_contract.timeoutFinite(.{ .nanoseconds = 2_000_000_000 }), &status) != .ok or
        status.flags & a.audio_service_flag_backend_present == 0) return 13;
    sys.println(std.fmt.bufPrint(&text, "REAL56 SERVICE_SELECT acknowledged=1 serviceEpoch={d} revision={d} desiredMatch=1 activeMatch=1 backendPresent=1; actual existing Kernel/driver through service-owned worker", .{ page.service_epoch, page.revision }) catch unreachable);
    sys.println(std.fmt.bufPrint(&text, "REAL56 PASS scalarRows={d} activeRows={d} selection={d} freshProviderEpoch={d} serviceEpoch={d} serviceRevision={d} serviceTotal={d}; actual QEMU driver contract, no physical receiver/speaker claim", .{ receipt.count, active, selected, receipt.providers[0].generation, page.service_epoch, page.revision, page.total }) catch unreachable);
    return 0;
}
