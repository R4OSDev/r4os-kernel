const std = @import("std");
pub const role = "test.audio.stop52";
pub const magic: u32 = 0x35525350;
pub const ping: u32 = 0;
pub const register: u32 = 1;
pub const snapshot: u32 = 2;
pub const configure: u32 = 3;
pub const unregister: u32 = 4;
pub const State = extern struct {
    magic: u32 = magic,
    size: u32 = @sizeOf(State),
    write_calls: u64 = 0,
    writes: u64 = 0,
    bytes: u64 = 0,
    last_hash: u64 = 0,
    idle_calls: u64 = 0,
    idle_acks: u64 = 0,
    stop_calls: u64 = 0,
    stop_acks: u64 = 0,
    sid_stop_calls: u64 = 0,
    sid_stop_acks: u64 = 0,
    sid_plays: u64 = 0,
    sid_renders: u64 = 0,
    output_generation: u64 = 1,
    reset_generation: u64 = 1,
    synth_registrations: u64 = 0,
    midi_renders: u64 = 0,
    midi_stops: u64 = 0,
    active_output: u32 = 0,
    replace_requested: u32 = 0,
    replace_result: i32 = 0,
    write_result: i32 = 0,
    idle_result: i32 = 0,
    stop_result: i32 = 0,
    sid_stop_result: i32 = 0,
};
pub const Registration = extern struct { address: u64, bytes: u64 };
pub const Controls = extern struct { write_result: i32 = 0, idle_result: i32 = 0, stop_result: i32 = 0, sid_stop_result: i32 = 0, replace_requested: u32 = 0 };
pub fn load(pointer: anytype) @typeInfo(@TypeOf(pointer)).pointer.child {
    return @atomicLoad(@typeInfo(@TypeOf(pointer)).pointer.child, pointer, .acquire);
}
pub fn count(pointer: *u64) void { _ = @atomicRmw(u64, pointer, .Add, 1, .acq_rel); }
pub fn copy(state: *const State) State {
    var value: State = .{};
    inline for (@typeInfo(State).@"struct".fields) |field| @field(value, field.name) = load(&@field(state, field.name));
    return value;
}
pub fn set(state: *State, controls: Controls) void {
    inline for (@typeInfo(Controls).@"struct".fields) |field| @atomicStore(field.type, &@field(state, field.name), @field(controls, field.name), .release);
}
pub fn hash(bytes: []const u8) u64 {
    var value: u64 = 0xcbf29ce484222325;
    for (bytes) |b| value = (value ^ b) *% 0x100000001b3;
    return value;
}
