// Private C facade; Rust owns the bounded history and runtime publication.
const abi = @import("../program/r4x_api.zig");
const access = @import("../storage/access_runtime.zig");
const desktop_events = @import("../kernel/desktop_events.zig");

pub const Cursor = abi.DirectoryChangeCursor;
extern fn r4os_rust_fs_directory_history(*History, u8, ?*const access.MountRef, u64, ?*Cursor) bool;
extern fn r4os_rust_fs_directory_changes(u8, ?*const access.MountRef, u64, ?*Cursor) i32;

pub const History = extern struct {
    const capacity = 256;
    const Entry = extern struct { mount: access.MountRef, node: u64 };
    sequence: u64 = 0,
    entries: [capacity]Entry = undefined,

    pub fn publish(self: *History, mount: access.MountRef, node: u64) void {
        _ = r4os_rust_fs_directory_history(self, 0, &mount, node, null);
    }
    pub fn begin(self: *const History, mount: access.MountRef, node: u64) Cursor {
        var out: Cursor = undefined;
        _ = r4os_rust_fs_directory_history(@constCast(self), 1, &mount, node, &out);
        return out;
    }
    pub fn poll(self: *const History, cursor: *Cursor) bool {
        return r4os_rust_fs_directory_history(@constCast(self), 2, null, 0, cursor);
    }
};

pub fn notify(mount: access.MountRef, node: u64) void {
    _ = r4os_rust_fs_directory_changes(0, &mount, node, null);
}
pub fn begin(mount: access.MountRef, node: u64) Cursor {
    var out: Cursor = undefined;
    _ = r4os_rust_fs_directory_changes(1, &mount, node, &out);
    return out;
}
pub fn poll(cursor: *Cursor) i32 {
    return r4os_rust_fs_directory_changes(2, null, 0, cursor);
}

// The desktop activity owner migrates with its later domain. It receives
// notification only after native history publication and storage unlock.
export fn r4os_zig_fs_directory_activity() callconv(.c) void {
    desktop_events.signal();
}
comptime {
    if (@sizeOf(History.Entry) != 24 or @offsetOf(History, "entries") != 8 or
        @sizeOf(History) != 6152 or @sizeOf(Cursor) != 40)
        @compileError("Private native directory-history layout changed");
}
