// Native durable claim store. This facade projects the current VFS union;
// publisher admission and exact identity remain with the existing caller.
const upc=@import("upload_publish_claim");
const vfs=@import("vfs.zig");
pub const claim_directory="/R4OS/UPDATE/CLAIMS";
pub const ReplaySummary=upc.ReplaySummary;
pub const WriteResult=extern struct{ok:bool,generation:u64};
const Bytes=extern struct{data:[*]const u8,len:usize};
fn bytes(v:[]const u8)Bytes{return .{.data=v.ptr,.len=v.len};}
const WriteArgs=extern struct{volume:vfs.NativeVolume,parent:Bytes,stage:Bytes,target:Bytes,backup:Bytes,identity:upc.FileIdentity,protocol:upc.Protocol,reuse_generation:u64};
extern fn r4os_rust_claim_volume(v:*const vfs.NativeVolume)u64;
extern fn r4os_rust_claim_store_write(refresh:bool,a:*const WriteArgs,out:*WriteResult)void;
extern fn r4os_rust_claim_store_retire(generation:u64)bool;
extern fn r4os_rust_claim_store_recover(summary:*ReplaySummary)bool;
pub fn volumeToken(v:vfs.Volume)u64{const native=vfs.nativeVolume(v);return r4os_rust_claim_volume(&native);}
fn write(v:vfs.Volume,parent:[]const u8,stage:[]const u8,target:[]const u8,backup:[]const u8,identity:upc.FileIdentity,protocol:upc.Protocol,generation:u64,refresh:bool)WriteResult{
 var result:WriteResult=undefined;r4os_rust_claim_store_write(refresh,&.{.volume=vfs.nativeVolume(v),.parent=bytes(parent),.stage=bytes(stage),.target=bytes(target),.backup=bytes(backup),.identity=identity,.protocol=protocol,.reuse_generation=generation},&result);return result;
}
pub fn beginPublish(v:vfs.Volume,parent:[]const u8,stage:[]const u8,target:[]const u8,backup:[]const u8,identity:upc.FileIdentity,protocol:upc.Protocol)WriteResult{return write(v,parent,stage,target,backup,identity,protocol,0,false);}
pub fn refreshIdentity(v:vfs.Volume,parent:[]const u8,stage:[]const u8,target:[]const u8,backup:[]const u8,identity:upc.FileIdentity,protocol:upc.Protocol,generation:u64)bool{
 return write(v,parent,stage,target,backup,identity,protocol,generation,true).ok;
}
pub fn retire(generation:u64)bool{return r4os_rust_claim_store_retire(generation);}
pub fn recoverBeforeRuntime(summary:*ReplaySummary)bool{return r4os_rust_claim_store_recover(summary);}
comptime{if(@sizeOf(WriteResult)!=16 or @sizeOf(WriteArgs)!=192 or @offsetOf(WriteArgs,"identity")!=152 or @offsetOf(WriteArgs,"reuse_generation")!=184)@compileError("Claim write receipt layout changed");}
