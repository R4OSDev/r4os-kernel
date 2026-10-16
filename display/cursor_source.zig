//! Fixed cursor image owner projection. The caller holds the existing BO owner.
const lifetime = @import("../memory/gfx_buffer_owner.zig");
const a = @import("r4os_kernel_contract");
pub const owner: lifetime.Owner = .{ .kind = .kernel, .id = 4, .generation = 1 };
extern fn r4os_rust_display_cursor_source(u32, *Source, ?*const lifetime.NativeView, *const lifetime.Owner, *const lifetime.Owner, *const a.DisplayCursorRequest) i32;
pub const Source = extern struct {
    reference: lifetime.Handle = .{}, borrowed: lifetime.Handle = .{}, lease: lifetime.Handle = .{},
    driver: lifetime.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    pub fn open(self: *Source, store: anytype, caller: lifetime.Owner, driver: lifetime.Owner, request: a.DisplayCursorRequest) lifetime.Error!void {
        if (@hasDecl(@typeInfo(@TypeOf(store)).pointer.child, "nativeView")) {
            const view = store.nativeView(); try status(r4os_rust_display_cursor_source(0, self, &view, &caller, &driver, &request));
        } else { try status(r4os_rust_display_cursor_source(0, self, null, &caller, &driver, &request)); }
    }
    pub fn close(self: *Source, store: anytype) lifetime.Error!void {
        if (@hasDecl(@typeInfo(@TypeOf(store)).pointer.child, "nativeView")) {
            const view = store.nativeView(); try status(r4os_rust_display_cursor_source(1, self, &view, &owner, &owner, &.{}));
        } else { try status(r4os_rust_display_cursor_source(1, self, null, &owner, &owner, &.{})); }
    }
    pub fn wire(self: *const Source) a.GfxBufferHandle { return .{ .id = self.borrowed.id, .generation = self.borrowed.generation }; }
};
fn status(code: i32) lifetime.Error!void { return switch (code) { 0 => {}, 1 => error.Invalid, 2 => error.Overflow, 3 => error.Unsupported, 4 => error.Exhausted, 5 => error.Budget, 6 => error.Capacity, 7 => error.Stale, 8 => error.Busy, 9 => error.Closed, 10 => error.WrongOwner, else => unreachable }; }
comptime { if (@sizeOf(Source) != 72 or @offsetOf(Source, "driver") != 48) @compileError("Native cursor source C layout"); }
