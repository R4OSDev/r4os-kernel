const std = @import("std");
const r = @import("r4os");
const a = r.abi;
var worker_sys: r.r4sys.Context = undefined;
var worker_go = false;
var io_buffer: [8192]u8 = undefined;
fn identify(sys: *const r.r4sys.Context, mode: u8) void {
    var h: a.ProgramJoinHandle = .{};
    const rc = sys.threadCurrentHandle(&h);
    var buf: [256]u8 = undefined;
    const text = std.fmt.bufPrint(&buf, "FAULT87 mode={c} current={d} owner={d}:{d} thread={d}:{d}", .{mode, rc, h.instance_id, h.instance_generation, h.thread_id, h.thread_generation}) catch "FAULT87 overflow";
    sys.println(text);
    _ = sys.logServiceWrite(1, "FAULT87", text);
    sys.sleepTicks(sys.ticksFromMilliseconds(30));
}
noinline fn trap() noreturn {
    asm volatile ("ud2");
    unreachable;
}
fn worker(_: u64) callconv(.c) i32 {
    while (!@atomicLoad(bool, &worker_go, .acquire)) worker_sys.sleepTicks(1);
    identify(&worker_sys, 'T');
    trap();
}
pub fn r4_app_main(app: *r.App) i32 {
    const sys = app.system();
    const audio = app.audioLowLevel() orelse return 2;
    const args = app.args();
    if (args.len != 1) return 3;
    const mode = args[0];
    identify(&sys, mode);
    const resource = if (mode == 'T') audio.midiOpenSynth("OPL3") else audio.sidAcquire();
    sys.print("FAULT87 acquired=");sys.printI32(resource);sys.write("\r\n");
    if (resource != 1) return 4;
    var request: u32 = 0;
    const submitted = sys.ioFileRead("C:\\TEMP\\IO87.BIN", &io_buffer, 0, &request);
    var info: a.ProgramIoInfo = .{};
    const queried = if (submitted == 0) sys.ioStatus(request, &info) else -99;
    var io_text: [192]u8 = undefined;
    sys.println(std.fmt.bufPrint(&io_text, "IO87 submit={d} request={d} status_rc={d}; deliberately retained through CPU fault", .{submitted, request, queried}) catch "IO87 overflow");
    if (submitted != 0 or queried != 0) return 7;
    if (mode == 'T') {
        worker_sys = sys;
        @atomicStore(bool, &worker_go, false, .release);
        var h: a.ProgramJoinHandle = .{};
        if (sys.threadCreateHandle(worker, 0, 128*1024, 0, &h) != a.thread_ok) return 5;
        @atomicStore(bool, &worker_go, true, .release);
        var result: i32 = -99;
        _ = sys.threadHandleJoin(&h, sys.ticksFromMilliseconds(5000), &result);
        return 6;
    }
    sys.sleepTicks(sys.ticksFromMilliseconds(30));
    if (mode == 'I') asm volatile ("cli");
    trap();
}
