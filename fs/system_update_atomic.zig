// Checked transitions belong to the native update owner. Caller keeps its
// original single filesystem-request gate; values are explicit C projections.
const vfs=@import("vfs.zig");
pub const Direction = enum(u8) {
    forward,
    rollback,
};

pub const Result = enum(u8) {
    ok,
    conflict,
    io,
    not_atomic,
};

pub const Spec = extern struct {
    long_stage: bool = false,
    target_existed: bool,
    old_known: bool,
    new_size: u64,
    new_checksum: u32,
    old_size: u64 = 0,
    old_checksum: u32 = 0,
};

pub const CleanupSpec = extern struct {
    /// Fingerprint the freshly installed target must still carry.
    new_size: u64,
    new_checksum: u32,
    /// Whether this payload replaced an existing object (so a backup exists).
    target_existed: bool,
    /// Fingerprint of that backup, when the journal recorded one.
    old_known: bool = false,
    old_size: u64 = 0,
    old_checksum: u32 = 0,
    /// Fingerprint of an inherited older backup that may be rotated away.
    /// Without it the object stays - one extra last-good file is safe, an
    /// unidentifiable delete is not.
    previous_known: bool = false,
    previous_size: u64 = 0,
    previous_checksum: u32 = 0,
};
const Bytes=extern struct{data:[*]const u8,len:usize};
fn bytes(v:[]const u8)Bytes{return .{.data=v.ptr,.len=v.len};}
extern fn r4os_rust_update_transition(v:*const vfs.NativeVolume,parent:u64,target:Bytes,stage:Bytes,backup:Bytes,direction:Direction,spec:*const Spec)u8;
extern fn r4os_rust_update_cleanup(v:*const vfs.NativeVolume,parent:u64,target:Bytes,stage:Bytes,backup:Bytes,previous:Bytes,spec:*const CleanupSpec)u8;
pub fn transition(v:vfs.Volume,parent:vfs.NodeRef,target:[]const u8,stage:[]const u8,backup:[]const u8,direction:Direction,spec:Spec)Result{
 const native=vfs.nativeVolume(v);return @enumFromInt(r4os_rust_update_transition(&native,parent,bytes(target),bytes(stage),bytes(backup),direction,&spec));
}
pub fn cleanupPayload(v:vfs.Volume,parent:vfs.NodeRef,target:[]const u8,stage:[]const u8,backup:[]const u8,previous:[]const u8,spec:CleanupSpec)Result{
 const native=vfs.nativeVolume(v);return @enumFromInt(r4os_rust_update_cleanup(&native,parent,bytes(target),bytes(stage),bytes(backup),bytes(previous),&spec));
}
comptime{if(@sizeOf(Spec)!=40 or @sizeOf(CleanupSpec)!=48 or @offsetOf(Spec,"new_size")!=8 or @offsetOf(CleanupSpec,"previous_size")!=32)@compileError("Checked update C transport changed");}
