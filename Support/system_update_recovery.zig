// Shared, allocation-free SYSUPD journal and terminal-recovery core.
//
// This module uses only std and the shared, allocation-free path policy.
// The R4X updater and the
// pre-runtime kernel recovery seam compile it for different execution
// environments and provide thin I/O adapters.  Durable format parsing,
// validation, slot ordering, rollback and cleanup policy must never diverge
// between those two consumers.

const std = @import("std");

pub const journal_max: usize = 192 * 1024;
pub const max_package_payloads: usize = 32;
// A single-package transaction adds MODULES.JSON.  A v5 restart batch may
// append VERSION.R4S only after successful post-boot validation.
pub const max_payloads: usize = max_package_payloads + 2;
pub const max_path: usize = 1024;
pub const max_version: usize = 32;
pub const checksum_seed: u32 = 2166136261;

pub const JournalPhase = enum(u32) {
    prepare,
    stage,
    commit,
    verify,
    inventory,
    applied,
    post_boot,
    cleanup,
    rollback,
    rolled_back,
};

pub const PathState = enum(u32) {
    match,
    not_found,
    other,
    io,
};

pub const PresenceState = enum(u32) {
    file,
    other,
    not_found,
    io,
};

pub const MutationStatus = enum(u32) {
    ok,
    conflict,
    io,
};

pub const ReplayStatus = enum(u32) {
    ok,
    invalid,
    conflict,
    io,
};

pub const SlotSelection = union(enum) {
    selected: u8,
    not_found,
    invalid,
};

pub const JournalPayload = extern struct {
    target_path: [max_path:0]u8 = .{0} ** max_path,
    target_len: usize = 0,
    stage_path: [max_path:0]u8 = .{0} ** max_path,
    stage_len: usize = 0,
    backup_path: [max_path:0]u8 = .{0} ** max_path,
    backup_len: usize = 0,
    previous_backup_path: [max_path:0]u8 = .{0} ** max_path,
    previous_backup_len: usize = 0,
    previous_backup_known: bool = false,
    previous_backup_size: u64 = 0,
    previous_backup_checksum: u32 = 0,
    size: u64 = 0,
    checksum: u32 = 0,
    target_existed: bool = false,
    old_known: bool = false,
    old_size: u64 = 0,
    old_checksum: u32 = 0,
    committed: bool = false,
    rolled_back: bool = false,
    replace_required: bool = true,

    pub fn targetText(self: *const JournalPayload) []const u8 {
        return self.target_path[0..self.target_len];
    }

    pub fn stageText(self: *const JournalPayload) []const u8 {
        return self.stage_path[0..self.stage_len];
    }

    pub fn backupText(self: *const JournalPayload) []const u8 {
        return self.backup_path[0..self.backup_len];
    }

    pub fn previousBackupText(self: *const JournalPayload) []const u8 {
        return self.previous_backup_path[0..self.previous_backup_len];
    }

    pub fn targetPtr(self: *const JournalPayload) [*:0]const u8 {
        return @ptrCast(self.target_path[0..].ptr);
    }

    pub fn stagePtr(self: *const JournalPayload) [*:0]const u8 {
        return @ptrCast(self.stage_path[0..].ptr);
    }

    pub fn backupPtr(self: *const JournalPayload) [*:0]const u8 {
        return @ptrCast(self.backup_path[0..].ptr);
    }

    pub fn previousBackupPtr(self: *const JournalPayload) [*:0]const u8 {
        return @ptrCast(self.previous_backup_path[0..].ptr);
    }
};

pub const JournalComponentKind = enum(u32) {
    kernel,
    r4x,
    r4l,
    r4d,
    r4p,

    pub fn text(self: JournalComponentKind) []const u8 {
        return nativeText(1,@intFromEnum(self));
}

    pub fn parse(value: []const u8) ?JournalComponentKind {
        const value_code=r4os_rust_journal_value(8,&.{.a=bytes(value)});return if(value_code==5)null else @enumFromInt(@as(u32,@intCast(value_code)));
}
};

pub const JournalComponent = extern struct {
    payload_index: u32 = 0,
    kind: JournalComponentKind = .r4x,
    name: [49:0]u8 = .{0} ** 49,
    name_len: usize = 0,
    target: [max_path:0]u8 = .{0} ** max_path,
    target_len: usize = 0,
    version: [max_version:0]u8 = .{0} ** max_version,
    version_len: usize = 0,

    pub fn nameText(self: *const JournalComponent) []const u8 {
        return self.name[0..self.name_len];
    }

    pub fn targetText(self: *const JournalComponent) []const u8 {
        return self.target[0..self.target_len];
    }

    pub fn versionText(self: *const JournalComponent) []const u8 {
        return self.version[0..self.version_len];
    }
};

pub const TransactionJournal = extern struct {
    valid: bool = false,
    slot: u8 = 0,
    transaction_generation: u64 = 0,
    journal_generation: u64 = 0,
    phase: JournalPhase = .prepare,
    package_digest: u32 = 0,
    package_length: u64 = 0,
    manifest_checksum: u32 = 0,
    component_digest: u32 = 0,
    component_count: u32 = 0,
    components_bound: bool = true,
    components: [max_package_payloads]JournalComponent = .{JournalComponent{}} ** max_package_payloads,
    foundation: bool = false,
    package_version: [max_version:0]u8 = .{0} ** max_version,
    package_version_len: usize = 0,
    source_path: [max_path:0]u8 = .{0} ** max_path,
    source_len: usize = 0,
    source_version: [max_version:0]u8 = .{0} ** max_version,
    source_version_len: usize = 0,
    target_version: [max_version:0]u8 = .{0} ** max_version,
    target_version_len: usize = 0,
    package_payload_count: u32 = 0,
    payload_count: u32 = 0,
    committed_count: u32 = 0,
    rollback_count: u32 = 0,
    reboot: bool = false,
    /// v5: one journal spans the complete ordered restart batch.  Boot
    /// recovery may continue this transaction forward to `post_boot`; normal
    /// single-package journals retain the historical rollback-on-boot policy.
    batch: bool = false,
    // In-memory compatibility marker only; the canonical serializer always
    // writes per-payload rolled_back fields.
    rollback_markers_present: bool = true,
    payloads: [max_payloads]JournalPayload = .{JournalPayload{}} ** max_payloads,

    pub fn sourceText(self: *const TransactionJournal) []const u8 {
        return self.source_path[0..self.source_len];
    }

    pub fn sourcePtr(self: *const TransactionJournal) [*:0]const u8 {
        return @ptrCast(self.source_path[0..].ptr);
    }

    pub fn sourceVersionText(self: *const TransactionJournal) []const u8 {
        return self.source_version[0..self.source_version_len];
    }

    pub fn targetVersionText(self: *const TransactionJournal) []const u8 {
        return self.target_version[0..self.target_version_len];
    }

    pub fn sourceReleaseText(self: *const TransactionJournal) []const u8 {
        return self.sourceVersionText();
    }

    pub fn releaseText(self: *const TransactionJournal) []const u8 {
        return self.targetVersionText();
    }

    pub fn packageVersionText(self: *const TransactionJournal) []const u8 {
        return self.package_version[0..self.package_version_len];
    }
};


// Fixed-layout transport; no parser, replay state or filesystem policy here.
const Bytes=extern struct{data:?[*]const u8=null,len:usize=0};
fn bytes(v:[]const u8)Bytes{return .{.data=v.ptr,.len=v.len};}
const ValueArgs=extern struct{a:Bytes=.{},b:Bytes=.{},out:?[*]u8=null,capacity:usize=0,generation:u64=0,index:usize=0,value:u32=0,present:[2]bool=.{false,false},valid:[2]bool=.{false,false},generations:[2]u64=.{0,0}};
const Io=extern struct{context:*anyopaque,call:*const fn(*anyopaque,u32,?*TransactionJournal,?*const JournalPayload,Bytes,u64,u32)callconv(.c)u32};
extern fn r4os_rust_journal_value(op:u32,args:*const ValueArgs)u64;
extern fn r4os_rust_journal_record(op:u32,j:*TransactionJournal,data:?[*]u8,len:usize,slot:u8)usize;
extern fn r4os_rust_journal_text(op:u32,value:u32,out:*Bytes)void;
extern fn r4os_rust_recovery_run(op:u32,io:*Io,j:*TransactionJournal)u32;
fn nativeText(op:u32,value:u32)[]const u8{var out:Bytes=.{};r4os_rust_journal_text(op,value,&out);return out.data.?[0..out.len];}
pub fn phaseTerminal(phase:JournalPhase)bool{return r4os_rust_journal_value(7,&.{.value=@intFromEnum(phase)})!=0;}
pub fn phaseName(phase:JournalPhase)[]const u8{return nativeText(0,@intFromEnum(phase));}
pub fn replayStatusName(status:ReplayStatus)[]const u8{return nativeText(2,@intFromEnum(status));}
pub fn selectNewestSlot(present:[2]bool,valid:[2]bool,generation:[2]u64)SlotSelection{return switch(r4os_rust_journal_value(6,&.{.present=present,.valid=valid,.generations=generation})){0=>.{.selected=0},1=>.{.selected=1},2=>.not_found,3=>.invalid,else=>unreachable};}
pub fn serializeJournal(j:*const TransactionJournal,out:[]u8)?[]const u8{const n=r4os_rust_journal_record(2,@constCast(j),out.ptr,out.len,0);return if(n==std.math.maxInt(usize))null else out[0..n];}
pub fn parseJournalInto(data:[]const u8,slot:u8,j:*TransactionJournal)bool{return r4os_rust_journal_record(3,j,@constCast(data.ptr),data.len,slot)!=0;}
pub fn journalValid(j:*const TransactionJournal)bool{return r4os_rust_journal_record(0,@constCast(j),null,0,0)!=0;}
pub fn journalPathsValid(j:*const TransactionJournal)bool{return r4os_rust_journal_record(1,@constCast(j),null,0,0)!=0;}
pub fn normalizeAbsolutePath(out:[]u8,value:[]const u8)?[]const u8{const n=r4os_rust_journal_value(4,&.{.a=bytes(value),.out=out.ptr,.capacity=out.len});return if(n==std.math.maxInt(u64))null else out[0..@intCast(n)];}
pub fn buildInternal83Name(out:[]u8,target:[]const u8,prefix:u8,transaction_generation:u64,index:usize)?usize{const n=r4os_rust_journal_value(5,&.{.a=bytes(target),.out=out.ptr,.capacity=out.len,.value=prefix,.generation=transaction_generation,.index=index});return if(n==std.math.maxInt(u64))null else @intCast(n);}
pub fn validateShortName83Text(v:[]const u8)bool{return r4os_rust_journal_value(3,&.{.a=bytes(v)})!=0;}
pub fn pathEqualsIgnoreCase(a:[]const u8,b:[]const u8)bool{return r4os_rust_journal_value(2,&.{.a=bytes(a),.b=bytes(b)})!=0;}
pub fn versionTextValid(v:[]const u8)bool{return r4os_rust_journal_value(1,&.{.a=bytes(v)})!=0;}
pub fn checksum(v:[]const u8)u32{return checksumUpdate(checksum_seed,v);}
pub fn checksumUpdate(seed:u32,v:[]const u8)u32{return @intCast(r4os_rust_journal_value(0,&.{.a=bytes(v),.value=seed}));}
fn copyTextZ(out:[]u8,v:[]const u8)?usize{const n=r4os_rust_journal_value(9,&.{.a=bytes(v),.out=out.ptr,.capacity=out.len});return if(n==std.math.maxInt(u64))null else @intCast(n);}
fn Adapter(comptime T:type)type{return struct{
 fn invoke(context:*anyopaque,op:u32,j:?*TransactionJournal,e:?*const JournalPayload,path:Bytes,size:u64,sum:u32)callconv(.c)u32{
  const io:T=@ptrCast(@alignCast(context));const V=@typeInfo(T).pointer.child;
  return switch(op){
   0=>if(comptime @hasDecl(V,"pathState"))@intFromEnum(io.pathState(path.data.?[0..path.len],size,sum))else unreachable,
   1=>if(comptime @hasDecl(V,"presence"))@intFromEnum(io.presence(path.data.?[0..path.len]))else unreachable,
   2=>if(comptime @hasDecl(V,"rollbackPayload"))@intFromEnum(io.rollbackPayload(e.?))else unreachable,
   3=>if(comptime @hasDecl(V,"commitPayload"))@intFromEnum(io.commitPayload(e.?))else unreachable,
   4=>if(comptime @hasDecl(V,"cleanupPayload"))@intFromEnum(io.cleanupPayload(e.?))else unreachable,
   5=>if(comptime @hasDecl(V,"persist"))@intFromBool(io.persist(j.?))else unreachable,
   else=>unreachable};
 }
};}
fn replay(op:u32,io:anytype,j:*TransactionJournal)ReplayStatus{var adapter=Io{.context=@ptrCast(@constCast(io)),.call=&Adapter(@TypeOf(io)).invoke};return @enumFromInt(r4os_rust_recovery_run(op,&adapter,j));}
pub fn checkLastGood(io:anytype,j:*const TransactionJournal)ReplayStatus{return replay(0,io,@constCast(j));}
pub fn rollbackToTerminal(io:anytype,j:*TransactionJournal)ReplayStatus{return replay(1,io,j);}
pub fn resumeBatchForward(io:anytype,j:*TransactionJournal)ReplayStatus{return replay(2,io,j);}
pub fn cleanupToTerminal(io:anytype,j:*TransactionJournal)ReplayStatus{return replay(3,io,j);}

comptime{
 if(@sizeOf(ValueArgs)!=88 or @offsetOf(ValueArgs,"generations")!=72 or @sizeOf(Io)!=16)@compileError("Journal scalar/callback C layout changed");
 if(@sizeOf(JournalPayload)!=4216 or @alignOf(JournalPayload)!=8)@compileError("Native JournalPayload size changed");
 if(@offsetOf(JournalPayload,"target_path")!=0)@compileError("Native JournalPayload.target_path offset changed");
 if(@offsetOf(JournalPayload,"target_len")!=1032)@compileError("Native JournalPayload.target_len offset changed");
 if(@offsetOf(JournalPayload,"stage_path")!=1040)@compileError("Native JournalPayload.stage_path offset changed");
 if(@offsetOf(JournalPayload,"stage_len")!=2072)@compileError("Native JournalPayload.stage_len offset changed");
 if(@offsetOf(JournalPayload,"backup_path")!=2080)@compileError("Native JournalPayload.backup_path offset changed");
 if(@offsetOf(JournalPayload,"backup_len")!=3112)@compileError("Native JournalPayload.backup_len offset changed");
 if(@offsetOf(JournalPayload,"previous_backup_path")!=3120)@compileError("Native JournalPayload.previous_backup_path offset changed");
 if(@offsetOf(JournalPayload,"previous_backup_len")!=4152)@compileError("Native JournalPayload.previous_backup_len offset changed");
 if(@offsetOf(JournalPayload,"previous_backup_known")!=4160)@compileError("Native JournalPayload.previous_backup_known offset changed");
 if(@offsetOf(JournalPayload,"previous_backup_size")!=4168)@compileError("Native JournalPayload.previous_backup_size offset changed");
 if(@offsetOf(JournalPayload,"previous_backup_checksum")!=4176)@compileError("Native JournalPayload.previous_backup_checksum offset changed");
 if(@offsetOf(JournalPayload,"size")!=4184)@compileError("Native JournalPayload.size offset changed");
 if(@offsetOf(JournalPayload,"checksum")!=4192)@compileError("Native JournalPayload.checksum offset changed");
 if(@offsetOf(JournalPayload,"target_existed")!=4196)@compileError("Native JournalPayload.target_existed offset changed");
 if(@offsetOf(JournalPayload,"old_known")!=4197)@compileError("Native JournalPayload.old_known offset changed");
 if(@offsetOf(JournalPayload,"old_size")!=4200)@compileError("Native JournalPayload.old_size offset changed");
 if(@offsetOf(JournalPayload,"old_checksum")!=4208)@compileError("Native JournalPayload.old_checksum offset changed");
 if(@offsetOf(JournalPayload,"committed")!=4212)@compileError("Native JournalPayload.committed offset changed");
 if(@offsetOf(JournalPayload,"rolled_back")!=4213)@compileError("Native JournalPayload.rolled_back offset changed");
 if(@offsetOf(JournalPayload,"replace_required")!=4214)@compileError("Native JournalPayload.replace_required offset changed");
 if(@sizeOf(JournalComponent)!=1160 or @alignOf(JournalComponent)!=8)@compileError("Native JournalComponent size changed");
 if(@offsetOf(JournalComponent,"payload_index")!=0)@compileError("Native JournalComponent.payload_index offset changed");
 if(@offsetOf(JournalComponent,"kind")!=4)@compileError("Native JournalComponent.kind offset changed");
 if(@offsetOf(JournalComponent,"name")!=8)@compileError("Native JournalComponent.name offset changed");
 if(@offsetOf(JournalComponent,"name_len")!=64)@compileError("Native JournalComponent.name_len offset changed");
 if(@offsetOf(JournalComponent,"target")!=72)@compileError("Native JournalComponent.target offset changed");
 if(@offsetOf(JournalComponent,"target_len")!=1104)@compileError("Native JournalComponent.target_len offset changed");
 if(@offsetOf(JournalComponent,"version")!=1112)@compileError("Native JournalComponent.version offset changed");
 if(@offsetOf(JournalComponent,"version_len")!=1152)@compileError("Native JournalComponent.version_len offset changed");
 if(@sizeOf(TransactionJournal)!=181728 or @alignOf(TransactionJournal)!=8)@compileError("Native TransactionJournal size changed");
 if(@offsetOf(TransactionJournal,"valid")!=0)@compileError("Native TransactionJournal.valid offset changed");
 if(@offsetOf(TransactionJournal,"slot")!=1)@compileError("Native TransactionJournal.slot offset changed");
 if(@offsetOf(TransactionJournal,"transaction_generation")!=8)@compileError("Native TransactionJournal.transaction_generation offset changed");
 if(@offsetOf(TransactionJournal,"journal_generation")!=16)@compileError("Native TransactionJournal.journal_generation offset changed");
 if(@offsetOf(TransactionJournal,"phase")!=24)@compileError("Native TransactionJournal.phase offset changed");
 if(@offsetOf(TransactionJournal,"package_digest")!=28)@compileError("Native TransactionJournal.package_digest offset changed");
 if(@offsetOf(TransactionJournal,"package_length")!=32)@compileError("Native TransactionJournal.package_length offset changed");
 if(@offsetOf(TransactionJournal,"manifest_checksum")!=40)@compileError("Native TransactionJournal.manifest_checksum offset changed");
 if(@offsetOf(TransactionJournal,"component_digest")!=44)@compileError("Native TransactionJournal.component_digest offset changed");
 if(@offsetOf(TransactionJournal,"component_count")!=48)@compileError("Native TransactionJournal.component_count offset changed");
 if(@offsetOf(TransactionJournal,"components_bound")!=52)@compileError("Native TransactionJournal.components_bound offset changed");
 if(@offsetOf(TransactionJournal,"components")!=56)@compileError("Native TransactionJournal.components offset changed");
 if(@offsetOf(TransactionJournal,"foundation")!=37176)@compileError("Native TransactionJournal.foundation offset changed");
 if(@offsetOf(TransactionJournal,"package_version")!=37177)@compileError("Native TransactionJournal.package_version offset changed");
 if(@offsetOf(TransactionJournal,"package_version_len")!=37216)@compileError("Native TransactionJournal.package_version_len offset changed");
 if(@offsetOf(TransactionJournal,"source_path")!=37224)@compileError("Native TransactionJournal.source_path offset changed");
 if(@offsetOf(TransactionJournal,"source_len")!=38256)@compileError("Native TransactionJournal.source_len offset changed");
 if(@offsetOf(TransactionJournal,"source_version")!=38264)@compileError("Native TransactionJournal.source_version offset changed");
 if(@offsetOf(TransactionJournal,"source_version_len")!=38304)@compileError("Native TransactionJournal.source_version_len offset changed");
 if(@offsetOf(TransactionJournal,"target_version")!=38312)@compileError("Native TransactionJournal.target_version offset changed");
 if(@offsetOf(TransactionJournal,"target_version_len")!=38352)@compileError("Native TransactionJournal.target_version_len offset changed");
 if(@offsetOf(TransactionJournal,"package_payload_count")!=38360)@compileError("Native TransactionJournal.package_payload_count offset changed");
 if(@offsetOf(TransactionJournal,"payload_count")!=38364)@compileError("Native TransactionJournal.payload_count offset changed");
 if(@offsetOf(TransactionJournal,"committed_count")!=38368)@compileError("Native TransactionJournal.committed_count offset changed");
 if(@offsetOf(TransactionJournal,"rollback_count")!=38372)@compileError("Native TransactionJournal.rollback_count offset changed");
 if(@offsetOf(TransactionJournal,"reboot")!=38376)@compileError("Native TransactionJournal.reboot offset changed");
 if(@offsetOf(TransactionJournal,"batch")!=38377)@compileError("Native TransactionJournal.batch offset changed");
 if(@offsetOf(TransactionJournal,"rollback_markers_present")!=38378)@compileError("Native TransactionJournal.rollback_markers_present offset changed");
 if(@offsetOf(TransactionJournal,"payloads")!=38384)@compileError("Native TransactionJournal.payloads offset changed");
}

const testing = std.testing;

fn testJournalWithTarget(journal: *TransactionJournal, target: []const u8) void {
    journal.* = .{};
    journal.valid = true;
    journal.transaction_generation = 5;
    journal.journal_generation = 1;
    journal.package_payload_count = 1;
    journal.payload_count = 1;

    const source = "C:\\R4OS\\UPDATE\\INBOX\\PKG.R4U";
    @memcpy(journal.source_path[0..source.len], source);
    journal.source_len = source.len;

    const entry = &journal.payloads[0];
    @memcpy(entry.target_path[0..target.len], target);
    entry.target_len = target.len;
    entry.stage_len = buildInternal83Name(entry.stage_path[0..], target, 'S', 5, 0) orelse 0;
    entry.backup_len = buildInternal83Name(entry.backup_path[0..], target, 'B', 5, 0) orelse 0;
}

test "v5 journal roundtrip preserves v4 component bindings and batch marker" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SOFTWARE\\TERMINAL\\TERMINAL.R4X");
    journal.package_digest = 1234;
    journal.package_length = 5678;
    journal.manifest_checksum = 9012;
    journal.component_digest = 3456;
    journal.component_count = 1;
    const component = &journal.components[0];
    component.kind = .r4x;
    component.name_len = copyTextZ(component.name[0..], "TERMINAL") orelse 0;
    component.target_len = copyTextZ(component.target[0..], "/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X") orelse 0;
    component.version_len = copyTextZ(component.version[0..], "0.1.3") orelse 0;
    journal.foundation = true;
    journal.payloads[0].size = 42;
    journal.payloads[0].checksum = 99;
    journal.source_version_len = copyTextZ(journal.source_version[0..], "0.63.10") orelse 0;
    journal.target_version_len = copyTextZ(journal.target_version[0..], "0.63.11") orelse 0;
    journal.package_version_len = copyTextZ(journal.package_version[0..], "2.1.0") orelse 0;
    var buffer: [8192]u8 = undefined;
    const encoded = serializeJournal(&journal, buffer[0..]) orelse return error.TestUnexpectedResult;
    try testing.expect(std.mem.startsWith(u8, encoded, "R4U_JOURNAL=5\n"));
    var parsed: TransactionJournal = .{};
    try testing.expect(parseJournalInto(encoded, 1, &parsed));
    try testing.expectEqualStrings("0.63.10", parsed.sourceReleaseText());
    try testing.expectEqualStrings("0.63.11", parsed.releaseText());
    try testing.expectEqualStrings("2.1.0", parsed.packageVersionText());
    try testing.expectEqual(@as(u32, 3456), parsed.component_digest);
    try testing.expectEqual(@as(u32, 1), parsed.component_count);
    try testing.expect(parsed.components_bound);
    try testing.expectEqualStrings("TERMINAL", parsed.components[0].nameText());
    try testing.expectEqualStrings("0.1.3", parsed.components[0].versionText());
    try testing.expect(parsed.foundation);
    try testing.expect(!parsed.batch);
}

test "restart batch recovery commits every bound payload before post boot" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SOFTWARE\\APP\\APP.R4X");
    journal.batch = true;
    journal.reboot = true;
    journal.phase = .stage;
    journal.package_digest = 10;
    journal.package_length = 1000;
    journal.manifest_checksum = 20;
    journal.component_digest = 30;
    journal.source_version_len = copyTextZ(journal.source_version[0..], "0.63.12") orelse 0;
    journal.target_version_len = copyTextZ(journal.target_version[0..], "0.63.13") orelse 0;
    journal.package_version_len = copyTextZ(journal.package_version[0..], "1.0.0") orelse 0;
    journal.payloads[0].size = 50;
    journal.payloads[0].checksum = 60;
    journal.payloads[0].target_existed = true;
    journal.payloads[0].old_known = true;
    journal.payloads[0].old_size = 40;
    journal.payloads[0].old_checksum = 41;

    const MockIo = struct {
        commit_count: usize = 0,
        persist_count: usize = 0,
        reads: usize = 0,
        fail_read: usize = 0,
        stage_present: bool = false,

        pub fn persist(self: *@This(), _: *TransactionJournal) bool {
            self.persist_count += 1;
            return true;
        }

        pub fn commitPayload(self: *@This(), _: *const JournalPayload) MutationStatus {
            self.commit_count += 1;
            return .ok;
        }

        pub fn pathState(self: *@This(), _: []const u8, _: u64, _: u32) PathState {
            self.reads += 1;
            return if (self.reads == self.fail_read) .other else .match;
        }
        pub fn presence(self: *@This(), _: []const u8) PresenceState {
            return if (self.stage_present) .file else .not_found;
        }
    };
    var io: MockIo = .{};
    try testing.expectEqual(ReplayStatus.ok, resumeBatchForward(&io, &journal));
    try testing.expectEqual(JournalPhase.post_boot, journal.phase);
    try testing.expectEqual(@as(u32, 1), journal.committed_count);
    try testing.expectEqual(@as(usize, 1), io.commit_count);
    try testing.expect(io.persist_count >= 3);
    const persisted = io.persist_count;
    try testing.expectEqual(ReplayStatus.ok, checkLastGood(&io, &journal));
    journal.phase = .cleanup;
    try testing.expectEqual(ReplayStatus.ok, checkLastGood(&io, &journal));
    for ([_]usize{ 1, 2 }) |fault| {
        io.reads = 0;
        io.fail_read = fault;
        try testing.expectEqual(ReplayStatus.conflict, checkLastGood(&io, &journal));
        try testing.expectEqual(JournalPhase.cleanup, journal.phase);
    }
    io.fail_read = 0;
    io.stage_present = true;
    try testing.expectEqual(ReplayStatus.conflict, checkLastGood(&io, &journal));
    io.stage_present = false;
    journal.payloads[0].old_known = false;
    try testing.expectEqual(ReplayStatus.invalid, checkLastGood(&io, &journal));
    try testing.expectEqual(persisted, io.persist_count);
}

test "an ASCII target set stays acceptable" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SERVICES\\SSHD.R4X");
    try testing.expect(journalPathsValid(&journal));
    for ([_][]const u8{ "C:\\R4OS\\LICENSES\\GFX\\NOTICE.TXT", "C:\\R4OS\\SOURCES\\R4VIDEO\\SOURCE.ZIP" }) |target| {
        testJournalWithTarget(&journal, target);
        try testing.expect(journalPathsValid(&journal));
    }
    for ([_][]const u8{ "\\boot\\preload.r4i", "\\boot\\preload\\hidreport.r4p", "\\boot\\preload\\usbhid.r4p", "\\boot\\preload\\usbbot.r4p", "\\boot\\preload\\usbscsi.r4p" }) |target| {
        testJournalWithTarget(&journal, target);
        try testing.expect(journalPathsValid(&journal));
    }
    for ([_][]const u8{ "\\boot\\limine.conf", "\\boot\\preload\\other.r4p", "C:\\boot\\preload.r4i" }) |target| {
        testJournalWithTarget(&journal, target);
        try testing.expect(!journalPathsValid(&journal));
    }
    testJournalWithTarget(&journal, "C:\\R4OS\\LICENSES-OTHER\\NOTICE.TXT");
    try testing.expect(!journalPathsValid(&journal));
}

test "a subsystem R4X target is durable recovery state but subsystem data is not" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SUBSYSTEMS\\r4os.gb\\R4GB.R4X");
    try testing.expect(journalPathsValid(&journal));

    testJournalWithTarget(&journal, "C:\\R4OS\\SUBSYSTEMS\\r4os.gb\\SAVE\\GAME.SAV");
    try testing.expect(!journalPathsValid(&journal));
}

test "a non-ASCII target is installable again now that collation is backend-exact" {
    // Before 0.60.24 these were refused outright, because a byte-wise ASCII
    // fold cannot prove non-ASCII identity against NTFS `$UpCase`.  The
    // authoritative check now happens in the updater against the volume's own
    // collation before the prepare journal, so the structural validator no
    // longer has to fail closed here.
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SOFTWARE\\GR\xC3\x9CN.R4X");
    try testing.expect(journalPathsValid(&journal));

    // The package source path may carry non-ASCII as well.
    testJournalWithTarget(&journal, "C:\\R4OS\\SERVICES\\SSHD.R4X");
    const source = "C:\\R4OS\\UPDATE\\INBOX\\P\xC3\x84K.R4U";
    @memset(journal.source_path[0..], 0);
    @memcpy(journal.source_path[0..source.len], source);
    journal.source_len = source.len;
    try testing.expect(journalPathsValid(&journal));
}

test "the structural fold still catches every ASCII-visible duplicate" {
    // The fold is deliberately conservative: it may report FEWER collisions
    // than the backend would, never more.  What it does see must still be
    // rejected here, so a package that is obviously self-colliding never
    // reaches the prepare journal.
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SOFTWARE\\MIXED.R4X");
    journal.payload_count = 2;
    const second = &journal.payloads[1];
    const target = "C:\\R4OS\\SOFTWARE\\mixed.r4x";
    @memcpy(second.target_path[0..target.len], target);
    second.target_len = target.len;
    second.stage_len = buildInternal83Name(second.stage_path[0..], target, 'S', 5, 1) orelse 0;
    second.backup_len = buildInternal83Name(second.backup_path[0..], target, 'B', 5, 1) orelse 0;
    try testing.expect(!journalPathsValid(&journal));
}

test "duplicate payload targets are still rejected package wide" {
    var journal: TransactionJournal = undefined;
    testJournalWithTarget(&journal, "C:\\R4OS\\SERVICES\\SSHD.R4X");
    journal.payload_count = 2;
    const second = &journal.payloads[1];
    const target = "c:\\r4os\\services\\sshd.r4x"; // same object, different case
    @memcpy(second.target_path[0..target.len], target);
    second.target_len = target.len;
    second.stage_len = buildInternal83Name(second.stage_path[0..], target, 'S', 5, 1) orelse 0;
    second.backup_len = buildInternal83Name(second.backup_path[0..], target, 'B', 5, 1) orelse 0;
    try testing.expect(!journalPathsValid(&journal));
}
