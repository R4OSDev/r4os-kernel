// Persistent create-only upload publish claim (0.60.22).
//
// SFTP, SCP and FTP all publish an upload the same way: the payload is
// written under a private stage name and then handed over to its target name
// by the create-only alias publisher.  That hand-over has durable windows in
// which a reset loses the only knowledge of what was going on:
//
//   stage-only            the stage object exists, nothing was published yet
//   half-published        the canonical $FILE_NAME already names the target
//                         while the index still only carries the stage name
//                         (the 0.60.21 window)
//   target+stage alias    both names index the same object
//   target-only           the hand-over is complete
//   missing               nothing of the operation is left
//
// Before 0.60.22 the recovery token lived only in RAM, so a power loss inside
// those windows left an object that no later run could confidently finish or
// remove.  This module is the persistent counterpart: a small text claim is
// made durable BEFORE the first durable visibility point and removed only
// after the terminal stage detach.
//
// Design notes:
//   * The claim is a TEXT record, exactly like the SYSUPD journal (0.60.19).
//     Field order is fixed, unknown trailing fields are rejected, and a
//     checksum covers the body, so a torn write is never mistaken for a
//     valid claim.
//   * Every decision is bound to volume, parent, all three names AND the
//     exact backend file identity.  A merely equal name never satisfies a
//     replay; that is what makes "foreign" a first-class outcome instead of
//     an accidental delete.
//   * All filesystem access goes through a caller-supplied `io` seam, so the
//     host model drives the REAL replay logic without an R4OS runtime.

const std = @import("std");

pub const claim_magic = "R4U_CLAIM=1";
pub const max_path: usize = 1024;
pub const max_name: usize = 768;
pub const max_claims: usize = 16;
pub const claim_max: usize = 4096;
pub const checksum_seed: u32 = 2166136261;

/// What the protocol layer asked for.  Only used for telemetry and visible
/// error attribution; the replay rules are identical for every producer.
pub const Protocol = enum(u8) {
    sftp,
    scp,
    ftp,
    storage,

    pub fn text(self: Protocol) []const u8 {
        return nativeText(0, @intFromEnum(self));
}

    pub fn parse(value: []const u8) ?Protocol {
        const result=r4os_rust_claim_value(4, bytes(value), .{}, 0);
        return if(result==4) null else @enumFromInt(@as(u8,@intCast(result)));
}
};

/// Backend-neutral file identity.  NTFS uses {record, sequence}; FAT32 uses
/// {first cluster, 0} and needs the size as well because an empty file has no
/// cluster chain of its own.
pub const FileIdentity = extern struct {
    node: u64 = 0,
    generation: u16 = 0,
    size: u64 = 0,

    pub fn eql(a: FileIdentity, b: FileIdentity) bool {
        return a.node == b.node and a.generation == b.generation and a.size == b.size;
    }
};

/// The durable state a replay found on disk.
pub const ObservedState = enum(u32) {
    /// Only the stage name resolves and it is still canonically the stage.
    stage_only,
    /// Only the stage name resolves, but the record already names the target
    /// canonically: the publish passed its point of no return.
    half_published,
    /// Both names index the same object.
    alias,
    /// Only the target name resolves and it is our object.
    target_only,
    /// Neither name resolves.
    missing,
    /// Something is there, but it is not the object this claim owns.
    foreign,
    io,
};

/// Result of driving one claim to a terminal state.
pub const ReplayStatus = enum(u32) {
    /// The publish was completed forward; the target owns the payload.
    published,
    /// The publish was rolled back; the stage object is gone.
    rolled_back,
    /// Nothing was owned any more; the claim was simply retired.
    retired,
    /// A foreign object holds the name.  Nothing was touched and the claim
    /// is retired with visible telemetry.
    foreign,
    /// The claim itself is unusable (torn, wrong magic, bad checksum).
    invalid,
    io,
};

pub fn replayStatusName(status: ReplayStatus) []const u8 {
    return nativeText(1, @intFromEnum(status));
}

pub fn observedStateName(state: ObservedState) []const u8 {
    return nativeText(2, @intFromEnum(state));
}

/// One in-flight create-only publish.
pub const Claim = extern struct {
    generation: u64 = 0,
    /// Stable mounted-volume token; a claim never acts across volumes.
    volume: u64 = 0,
    parent_path: [max_path]u8 = .{0} ** max_path,
    parent_len: usize = 0,
    stage_name: [max_name]u8 = .{0} ** max_name,
    stage_len: usize = 0,
    target_name: [max_name]u8 = .{0} ** max_name,
    target_len: usize = 0,
    backup_name: [max_name]u8 = .{0} ** max_name,
    backup_len: usize = 0,
    identity: FileIdentity = .{},
    protocol: Protocol = .sftp,

    pub fn parentText(self: *const Claim) []const u8 {
        return self.parent_path[0..self.parent_len];
    }
    pub fn stageText(self: *const Claim) []const u8 {
        return self.stage_name[0..self.stage_len];
    }
    pub fn targetText(self: *const Claim) []const u8 {
        return self.target_name[0..self.target_len];
    }
    pub fn backupText(self: *const Claim) []const u8 {
        return self.backup_name[0..self.backup_len];
    }
    pub fn hasBackup(self: *const Claim) bool {
        return self.backup_len != 0;
    }
};


// Fixed private C transport only. Replay and durable bytes have one native owner.
const Bytes=extern struct{data:?[*]const u8=null,len:usize=0};
fn bytes(v:[]const u8)Bytes{return .{.data=v.ptr,.len=v.len};}
const Build=extern struct{generation:u64,volume:u64,parent:Bytes,stage:Bytes,target:Bytes,backup:Bytes,identity:FileIdentity,protocol:Protocol};
const Io=extern struct{context:*anyopaque,call:*const fn(*anyopaque,u32,*const Claim)callconv(.c)u32};
extern fn r4os_rust_claim_build(out:*Claim,args:*const Build)bool;
extern fn r4os_rust_claim_value(op:u32,a:Bytes,b:Bytes,value:u32)u64;
extern fn r4os_rust_claim_record(op:u32,c:*Claim,data:?[*]u8,len:usize)usize;
extern fn r4os_rust_claim_replay(io:*Io,c:[*]const Claim,count:usize,summary:?*ReplaySummary)u32;
extern fn r4os_rust_claim_text(op:u32,value:u32,out:*Bytes)void;
fn nativeText(op:u32,value:u32)[]const u8{var out:Bytes=.{};r4os_rust_claim_text(op,value,&out);return out.data.?[0..out.len];}
pub fn checksum(v:[]const u8)u32{return @intCast(r4os_rust_claim_value(0,bytes(v),.{},0));}
pub fn nameValid(v:[]const u8)bool{return r4os_rust_claim_value(1,bytes(v),.{},0)!=0;}
pub fn parentValid(v:[]const u8)bool{return r4os_rust_claim_value(2,bytes(v),.{},0)!=0;}
pub fn eqlIgnoreAsciiCase(a:[]const u8,b:[]const u8)bool{return r4os_rust_claim_value(3,bytes(a),bytes(b),0)!=0;}
pub fn claimValid(c:*const Claim)bool{return r4os_rust_claim_record(0,@constCast(c),null,0)!=0;}
pub fn build(out:*Claim,generation:u64,volume:u64,parent:[]const u8,stage:[]const u8,target:[]const u8,backup:[]const u8,identity:FileIdentity,protocol:Protocol)bool{
 return r4os_rust_claim_build(out,&.{.generation=generation,.volume=volume,.parent=bytes(parent),.stage=bytes(stage),.target=bytes(target),.backup=bytes(backup),.identity=identity,.protocol=protocol});
}
pub fn serialize(c:*const Claim,out:[]u8)?[]const u8{const n=r4os_rust_claim_record(1,@constCast(c),out.ptr,out.len);return if(n==std.math.maxInt(usize))null else out[0..n];}
pub fn parse(data:[]const u8,out:*Claim)bool{return r4os_rust_claim_record(2,out,@constCast(data.ptr),data.len)!=0;}
fn Adapter(comptime T:type)type{return struct{
 fn invoke(context:*anyopaque,op:u32,c:*const Claim)callconv(.c)u32{
  const io:T=@ptrCast(@alignCast(context));
  return switch(op){0=>@intFromEnum(io.observe(c)),1=>@intFromBool(io.publish(c)),2=>@intFromBool(io.discardStage(c)),3=>@intFromBool(io.retire(c)),else=>unreachable};
 }
};}
pub fn replayClaim(io:anytype,c:*const Claim)ReplayStatus{var adapter=Io{.context=@ptrCast(@constCast(io)),.call=&Adapter(@TypeOf(io)).invoke};return @enumFromInt(r4os_rust_claim_replay(&adapter,@as([*]const Claim,@ptrCast(c)),1,null));}
pub const ReplaySummary=extern struct{
 published:usize=0,rolled_back:usize=0,retired:usize=0,foreign:usize=0,invalid:usize=0,failed:usize=0,
 pub fn total(self:ReplaySummary)usize{return self.published+self.rolled_back+self.retired+self.foreign+self.invalid+self.failed;}
};
pub fn replayAll(io:anytype,claims:[]const Claim)ReplaySummary{var adapter=Io{.context=@ptrCast(@constCast(io)),.call=&Adapter(@TypeOf(io)).invoke};var out:ReplaySummary=.{};_=r4os_rust_claim_replay(&adapter,claims.ptr,claims.len,&out);return out;}
comptime{
 if(@sizeOf(FileIdentity)!=24 or @sizeOf(Claim)!=3408 or @offsetOf(Claim,"identity")!=3376 or @offsetOf(Claim,"protocol")!=3400 or @sizeOf(ReplaySummary)!=48 or @sizeOf(Build)!=112)@compileError("Native upload claim transport layout changed");
}

const testing = std.testing;

fn sampleClaim() Claim {
    var claim: Claim = undefined;
    _ = build(
        &claim,
        7,
        0x1234,
        "C:\\R4OS\\UPDATE\\INBOX",
        "UPLOAD.STG",
        "PACKAGE.R4U",
        "PACKAGE.BAK",
        .{ .node = 42, .generation = 3, .size = 8_388_608 },
        .sftp,
    );
    return claim;
}

test "claim survives a serialize/parse roundtrip" {
    const claim = sampleClaim();
    var buf: [claim_max]u8 = undefined;
    const text = serialize(&claim, buf[0..]) orelse return error.SerializeFailed;

    var back: Claim = undefined;
    try testing.expect(parse(text, &back));
    try testing.expectEqual(claim.generation, back.generation);
    try testing.expectEqual(claim.volume, back.volume);
    try testing.expectEqualStrings(claim.parentText(), back.parentText());
    try testing.expectEqualStrings(claim.stageText(), back.stageText());
    try testing.expectEqualStrings(claim.targetText(), back.targetText());
    try testing.expectEqualStrings(claim.backupText(), back.backupText());
    try testing.expect(claim.identity.eql(back.identity));
    try testing.expectEqual(claim.protocol, back.protocol);
}

test "every producer attribution survives the durable format" {
    const protocols = [_]Protocol{ .sftp, .scp, .ftp, .storage };
    for (protocols) |protocol| {
        var claim = sampleClaim();
        claim.protocol = protocol;
        var buf: [claim_max]u8 = undefined;
        const text = serialize(&claim, buf[0..]) orelse return error.SerializeFailed;
        var back: Claim = undefined;
        try testing.expect(parse(text, &back));
        try testing.expectEqual(protocol, back.protocol);
    }
}

test "a torn or corrupted claim never parses" {
    const claim = sampleClaim();
    var buf: [claim_max]u8 = undefined;
    const text = serialize(&claim, buf[0..]) orelse return error.SerializeFailed;
    var back: Claim = undefined;

    // Every truncation is rejected: the checksum line is the only terminator.
    var cut: usize = 0;
    while (cut < text.len) : (cut += 1) {
        try testing.expect(!parse(text[0..cut], &back));
    }
    // A flipped body byte breaks the checksum.
    var damaged: [claim_max]u8 = undefined;
    @memcpy(damaged[0..text.len], text);
    damaged[text.len / 2] ^= 0x20;
    try testing.expect(!parse(damaged[0..text.len], &back));
    // Trailing junk is not accepted either.
    @memcpy(damaged[0..text.len], text);
    damaged[text.len] = 'x';
    try testing.expect(!parse(damaged[0 .. text.len + 1], &back));
}

test "structurally impossible claims are refused" {
    var claim: Claim = undefined;
    // stage == target would make the hand-over meaningless
    try testing.expect(!build(&claim, 1, 1, "C:\\X", "A.STG", "A.STG", "", .{}, .sftp));
    // names must stay inside the recorded parent
    try testing.expect(!build(&claim, 1, 1, "C:\\X", "sub\\A.STG", "A.BIN", "", .{}, .sftp));
    try testing.expect(!build(&claim, 1, 1, "C:\\X", "..", "A.BIN", "", .{}, .sftp));
    // parent must be absolute and traversal free
    try testing.expect(!build(&claim, 1, 1, "R4OS\\X", "A.STG", "A.BIN", "", .{}, .sftp));
    try testing.expect(!build(&claim, 1, 1, "C:\\X\\..\\Y", "A.STG", "A.BIN", "", .{}, .sftp));
    // generation zero is never a live claim
    try testing.expect(!build(&claim, 0, 1, "C:\\X", "A.STG", "A.BIN", "", .{}, .sftp));
    // a backup aliasing one of the other names is rejected
    try testing.expect(!build(&claim, 1, 1, "C:\\X", "A.STG", "A.BIN", "a.bin", .{}, .sftp));
}
