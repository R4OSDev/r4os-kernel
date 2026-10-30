pub const role = "test.audio.catalog55";
pub const magic: u32 = 0x35415443;
pub const ping: u32 = 0;
pub const register: u32 = 1;
pub const snapshot: u32 = 2;
pub const configure: u32 = 3;
pub const unregister: u32 = 4;
pub const rebind: u32 = 5;
pub const State = extern struct {
    magic: u32 = magic,
    size: u32 = @sizeOf(State),
    owner: u32 = 0,
    generation: u64 = 0x1_0000_0001,
    queries: u64 = 0,
    actives: u64 = 0,
    epochs: u64 = 0,
    selections: u64 = 0,
    writes: u64 = 0,
    bytes: u64 = 0,
    idles: u64 = 0,
    stops: u64 = 0,
    secondary_queries: u64 = 0,
    secondary_epochs: u64 = 0,
    secondary_generation: u64 = 0x2_0000_0001,
    reentrant_attempts: u64 = 0,
    reentrant_result: i32 = 0,
    output_count: u32 = 32,
    secondary_count: u32 = 0,
    active_index: i32 = 0,
    change_at_query: u64 = 0,
    fail_at_query: u64 = 0,
    reentrant_at_query: u64 = 0,
    cross_change: u32 = 0,
    bad_epoch: u32 = 0,
    rebind_request: u32 = 0,
    rebind_completed: u32 = 0,
    rebind_result: i32 = 0,
    terminal_stop: u32 = 0,
    thread_done: u32 = 0,
};
pub const Registration = extern struct { address: u64, bytes: u64 };
pub const Controls = extern struct {
    output_count: u32 = 32, secondary_count: u32 = 0,
    change_at_query: u64 = 0, fail_at_query: u64 = 0,
    reentrant_at_query: u64 = 0, cross_change: u32 = 0,
    bad_epoch: u32 = 0, rebind_request: u32 = 0, terminal_stop: u32 = 0,
};
pub fn load(pointer: anytype) @typeInfo(@TypeOf(pointer)).pointer.child {
    return @atomicLoad(@typeInfo(@TypeOf(pointer)).pointer.child, pointer, .acquire);
}
pub fn count(pointer: *u64) u64 { return @atomicRmw(u64, pointer, .Add, 1, .acq_rel) + 1; }
pub fn copy(state: *const State) State {
    var out: State = .{};
    inline for (@typeInfo(State).@"struct".fields) |field| @field(out, field.name) = load(&@field(state, field.name));
    return out;
}
pub fn acquire(state: *State) bool { return @cmpxchgStrong(u32, &state.owner, 0, 1, .acquire, .monotonic) == null; }
pub fn release(state: *State) void { @atomicStore(u32, &state.owner, 0, .release); }
pub fn set(state: *State, controls: Controls) bool {
    if (!acquire(state)) return false;
    defer release(state);
    inline for (@typeInfo(Controls).@"struct".fields) |field| @atomicStore(@TypeOf(@field(state, field.name)), &@field(state, field.name), @field(controls, field.name), .release);
    @atomicStore(i32, &state.active_index, 0, .release);
    _ = count(&state.generation); _ = count(&state.secondary_generation);
    inline for (.{ "queries", "actives", "epochs", "selections", "secondary_queries", "secondary_epochs", "reentrant_attempts" }) |name| @atomicStore(u64, &@field(state, name), 0, .release);
    return true;
}
