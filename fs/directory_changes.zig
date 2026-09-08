// Bounded advisory directory changes. A cursor owns no kernel resource.
// Direct-directory identity uses the mounted generation and filesystem node,
// never a lossy case-folded path. Overflow requests a conservative reload.
const abi = @import("../program/r4x_api.zig");
const access = @import("../storage/access_runtime.zig");
const locks = @import("../memory/owner_locks.zig");
const desktop_events = @import("../kernel/desktop_events.zig");

pub const Cursor = abi.DirectoryChangeCursor;
pub const History = struct {
    const capacity = 256;
    const Entry = struct { mount: access.MountRef, node: u64 };
    sequence: u64 = 0,
    entries: [capacity]Entry = undefined,

    pub fn publish(self: *History, mount: access.MountRef, node: u64) void {
        self.entries[@intCast(self.sequence % capacity)] = .{ .mount = mount, .node = node };
        self.sequence +%= 1;
    }

    pub fn begin(self: *const History, mount: access.MountRef, node: u64) Cursor {
        return .{ .sequence = self.sequence, .node = node, .mount_slot = mount.slot, .mount_generation = mount.generation };
    }

    pub fn poll(self: *const History, cursor: *Cursor) bool {
        const distance = self.sequence -% cursor.sequence;
        defer cursor.sequence = self.sequence;
        if (distance > capacity) return true;
        var seq = cursor.sequence;
        while (seq != self.sequence) : (seq +%= 1) {
            const event = self.entries[@intCast(seq % capacity)];
            if (event.mount.slot == cursor.mount_slot and event.mount.generation == cursor.mount_generation and event.node == cursor.node) return true;
        }
        return false;
    }
};

var history: History = .{};

pub fn notify(mount: access.MountRef, node: u64) void {
    const guard = locks.storage.acquire();
    history.publish(mount, node);
    locks.storage.release(guard);
    // The RAM owner never spans the scheduler wake or filesystem work.
    desktop_events.signal();
}

pub fn begin(mount: access.MountRef, node: u64) Cursor {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return history.begin(mount, node);
}

pub fn poll(cursor: *Cursor) i32 {
    if (cursor.version != 1 or cursor.size != @sizeOf(Cursor) or cursor.reserved != 0 or cursor.mount_generation == 0) return -1;
    _ = access.mountSnapshot(.{ .slot = cursor.mount_slot, .generation = cursor.mount_generation }) catch return -3;
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return if (history.poll(cursor)) 1 else 0;
}
