//! Process-local context facade. Native Rust owns association records,
//! publication, the exact unwind anchor and retained failed frees.
const Tree = extern struct {
    root: ?*anyopaque = null,
    xorshift: usize = 0,
};
pub const Owner = extern struct {
    tree: Tree = .{},
    retired: ?*anyopaque = null,
    closing: bool = false,
};
extern fn r4os_rust_program_local_get(*Owner, u64, *u64) i32;
extern fn r4os_rust_program_local_publish(*Owner, u64, u64, *u64) i32;
extern fn r4os_rust_program_local_cleanup(*Owner) bool;
pub fn get(owner: *Owner, key: u64, output: *u64) i32 {
    return r4os_rust_program_local_get(owner, key, output);
}
pub fn publish(owner: *Owner, key: u64, candidate_value: u64, output: *u64) i32 {
    return r4os_rust_program_local_publish(owner, key, candidate_value, output);
}
pub fn cleanup(owner: *Owner) bool {
    return r4os_rust_program_local_cleanup(owner);
}
comptime {
    if (@sizeOf(Owner) != 32 or @alignOf(Owner) != 8 or
        @offsetOf(Owner, "retired") != 16 or @offsetOf(Owner, "closing") != 24)
        @compileError("Native process-local Owner C layout drift");
}
