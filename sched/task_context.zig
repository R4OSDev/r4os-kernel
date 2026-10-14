// Private C facade. Rust owns the exact per-CPU unwind-counter bindings.
pub const UnwindToken = extern struct {
    counter: ?*u32 = null,
    context_present: bool = false,
    active: bool = false,

    pub fn admitted(self: UnwindToken) bool {
        return !self.context_present or self.active;
    }
};
extern fn r4os_rust_task_context_bind(counter: ?*u32) void;
extern fn r4os_rust_task_context_enter(output: *UnwindToken) void;
extern fn r4os_rust_task_context_leave(token: *const UnwindToken) bool;
comptime {
    if (@sizeOf(UnwindToken) != 16 or @offsetOf(UnwindToken, "context_present") != 8 or
        @offsetOf(UnwindToken, "active") != 9) @compileError("Native unwind C layout differs");
}
pub fn bind(unwind_counter: *u32) void {
    r4os_rust_task_context_bind(unwind_counter);
}
pub fn clear() void {
    r4os_rust_task_context_bind(null);
}
pub fn enterUnwind() UnwindToken {
    var value: UnwindToken = undefined;
    r4os_rust_task_context_enter(&value);
    return value;
}
pub fn leaveUnwind(token: UnwindToken) bool {
    return r4os_rust_task_context_leave(&token);
}
