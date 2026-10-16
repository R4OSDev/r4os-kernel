//! Kernel-private R4R1 facade. Algorithms live in Rust; SDK/host/recovery copies stay Zig.
pub const magic = "R4R1";
pub const header_size: usize = 96;
pub const key_record_size: usize = 32;
pub const value_record_size: usize = 32;
pub const invalid_index: u32 = 0xffff_ffff;

pub const HiveKind = enum(u16) {
    system=1, software=2, desktop=3, user=4,
    pub fn fromInt(value:u16) ?HiveKind { return if(r4os_rust_registry_enum(value,true)) @enumFromInt(value) else null; }
    pub fn shortRoot(self:HiveKind) []const u8 { return r4os_rust_registry_root_name(@intFromEnum(self)).slice(); }
    pub fn longRoot(self:HiveKind) []const u8 { return self.shortRoot(); }
};
pub const ValueType = enum(u16) {
    string=1, u32=2, u64=3, bool=4, binary=5, multi_string=6,
    pub fn fromInt(value:u16) ?ValueType { return if(r4os_rust_registry_enum(value,false)) @enumFromInt(value) else null; }
};
pub const Error = error{
    FileTooSmall,
    BadMagic,
    BadVersion,
    BadHeaderSize,
    BadEndian,
    BadHiveKind,
    BadFlags,
    BadFileSize,
    BadChecksumType,
    BadChecksum,
    BadReserved,
    BadRange,
    BadRootKey,
    BadKey,
    BadValue,
    BadName,
    BadData,
    DuplicateKey,
    DuplicateValue,
    RootMismatch,
    InvalidPath,
    OutOfMemory,
    TooManyEntries,
};

pub const Header = extern struct {
    hive_kind: HiveKind,
    generation: u64,
    key_table_offset: u32,
    key_count: u32,
    value_table_offset: u32,
    value_count: u32,
    string_heap_offset: u32,
    string_heap_size: u32,
    data_heap_offset: u32,
    data_heap_size: u32,
    checksum_type: u32,
    checksum: u32,
};

pub const KeyRecord = extern struct {
    parent_index: u32,
    name_offset: u32,
    name_len: u16,
    first_value_index: u32,
    value_count: u32,
    first_child_index: u32,
    child_count: u32,
};

pub const ValueRecord = extern struct {
    owner_key_index: u32,
    name_offset: u32,
    name_len: u16,
    value_type: ValueType,
    data_offset: u32,
    data_len: u32,
};

pub const Value = struct {
    value_type:ValueType, data:[]const u8,
    fn native(self:Value) NativeValue { return .{.value_type=self.value_type,.data=NativeBytes.from(self.data)}; }
    pub fn asString(self:Value) ?[]const u8 {
        const value=self.native();var bytes:NativeBytes=undefined;
        return if(r4os_rust_registry_scalar(&value,0,null,&bytes)) bytes.slice() else null;
    }
    pub fn asU32(self:Value) ?u32 { const result=self.scalar(1) orelse return null;return @intCast(result); }
    pub fn asU64(self:Value) ?u64 { return self.scalar(2); }
    pub fn asBool(self:Value) ?bool { return (self.scalar(3) orelse return null)!=0; }
    fn scalar(self:Value,operation:u8) ?u64 {
        const value=self.native();var result:u64=undefined;
        return if(r4os_rust_registry_scalar(&value,operation,&result,null)) result else null;
    }
};
pub const ParsedRoot = struct { kind:HiveKind, rest:[]const u8 };
pub const HiveView = struct {
    bytes:[]const u8, header:Header, key_table:[]const u8, value_table:[]const u8,
    string_heap:[]const u8, data_heap:[]const u8,
    fn native(self:HiveView) NativeView { return .{
        .bytes=NativeBytes.from(self.bytes),.header=self.header,.key_table=NativeBytes.from(self.key_table),
        .value_table=NativeBytes.from(self.value_table),.string_heap=NativeBytes.from(self.string_heap),.data_heap=NativeBytes.from(self.data_heap),
    }; }
    fn fromNative(self:NativeView) HiveView { return .{
        .bytes=self.bytes.slice(),.header=self.header,.key_table=self.key_table.slice(),
        .value_table=self.value_table.slice(),.string_heap=self.string_heap.slice(),.data_heap=self.data_heap.slice(),
    }; }
    pub fn parse(bytes:[]const u8) Error!HiveView { return parseHive(bytes); }
    pub fn keyAt(self:HiveView,index:u32) KeyRecord {
        const view=self.native();var result:KeyRecord=undefined;
        r4os_rust_registry_record(&view,index,true,&result,null);return result;
    }
    pub fn valueAt(self:HiveView,index:u32) ValueRecord {
        const view=self.native();var result:ValueRecord=undefined;
        r4os_rust_registry_record(&view,index,false,null,&result);return result;
    }
    pub fn keyName(self:HiveView,key:KeyRecord) []const u8 {
        const view=self.native();return r4os_rust_registry_heap(&view,0,&key,null).slice();
    }
    pub fn valueName(self:HiveView,record:ValueRecord) []const u8 {
        const view=self.native();return r4os_rust_registry_heap(&view,1,null,&record).slice();
    }
    pub fn valueData(self:HiveView,record:ValueRecord) []const u8 {
        const view=self.native();return r4os_rust_registry_heap(&view,2,null,&record).slice();
    }
    pub fn findKey(self:HiveView,path:[]const u8) ?u32 {return self.find(0,0,path);}
    pub fn findChild(self:HiveView,parent:u32,name:[]const u8) ?u32 {return self.find(1,parent,name);}
    pub fn findValue(self:HiveView,key:u32,name:[]const u8) ?u32 {return self.find(2,key,name);}
    fn find(self:HiveView,operation:u8,owner:u32,name:[]const u8) ?u32 {
        const view=self.native();var result:u32=undefined;
        return if(r4os_rust_registry_find(&view,operation,owner,NativeBytes.from(name),&result)) result else null;
    }
    pub fn getValue(self:HiveView,path:[]const u8,name:[]const u8) ?Value {
        const view=self.native();var result:NativeValue=undefined;
        return if(r4os_rust_registry_get(&view,NativeBytes.from(path),NativeBytes.from(name),&result))
            .{.value_type=result.value_type,.data=result.data.slice()} else null;
    }
};
pub const BuildValue = struct {
    key_path: []const u8,
    name: []const u8,
    value_type: ValueType,
    data: []const u8,
};

pub const BuildKey = struct {
    parent: u32 = invalid_index,
    name: []const u8 = "",
    flat_index: u32 = invalid_index,
    first_child: u32 = invalid_index,
    last_child: u32 = invalid_index,
    next_sibling: u32 = invalid_index,
    child_count: u32 = 0,
    value_count: u32 = 0,
    value_first: u32 = 0,
    value_cursor: u32 = 0,
};

pub const BuildScratch = struct {
    keys: []BuildKey,
    value_key_indices: []u32,
    flat_key_order: []u32,
    /// Optional stable grouping workspace; old callers retain a no-allocation
    /// fallback, while runtime mutation owners supply one index per value.
    value_order: []u32 = &.{},
};

// Private C transports contain only scalars, fixed-layout records and spans.
const NativeBytes=extern struct {
    data:[*]const u8,len:usize,
    fn from(bytes:[]const u8) NativeBytes{return .{.data=bytes.ptr,.len=bytes.len};}
    fn slice(self:NativeBytes) []const u8{return self.data[0..self.len];}
};
const NativeView=extern struct {
    bytes:NativeBytes,header:Header,key_table:NativeBytes,value_table:NativeBytes,string_heap:NativeBytes,data_heap:NativeBytes,
};
const NativeValue=extern struct {value_type:ValueType,data:NativeBytes};
const NativeRoot=extern struct {kind:HiveKind,rest:NativeBytes};
const NativeBuildValue=extern struct {key_path:NativeBytes,name:NativeBytes,value_type:ValueType,data:NativeBytes};
const NativeBuildKey=extern struct {
    parent:u32,name:NativeBytes,flat_index:u32,first_child:u32,last_child:u32,next_sibling:u32,
    child_count:u32,value_count:u32,value_first:u32,value_cursor:u32,
};
const NativeBuildInput=extern struct {
    output:[*]u8,output_capacity:usize,keys:*anyopaque,key_capacity:usize,
    read_key:?*const fn(*const anyopaque,usize,*NativeBuildKey) callconv(.c) void,
    write_key:?*const fn(*anyopaque,usize,*const NativeBuildKey) callconv(.c) void,
    values:*const anyopaque,values_len:usize,
    read_value:?*const fn(*const anyopaque,usize,*NativeBuildValue) callconv(.c) void,
    owners:[*]u32,owners_capacity:usize,flat:[*]u32,flat_capacity:usize,
    grouped:[*]u32,grouped_capacity:usize,kind:HiveKind,generation:u64,
};
fn readKey(context:*const anyopaque,index:usize,out:*NativeBuildKey) callconv(.c) void {
    const keys:[*]const BuildKey=@ptrCast(@alignCast(context));const key=keys[index];
    out.*=.{.parent=key.parent,.name=NativeBytes.from(key.name),.flat_index=key.flat_index,
        .first_child=key.first_child,.last_child=key.last_child,.next_sibling=key.next_sibling,
        .child_count=key.child_count,.value_count=key.value_count,.value_first=key.value_first,.value_cursor=key.value_cursor};
}
fn writeKey(context:*anyopaque,index:usize,input:*const NativeBuildKey) callconv(.c) void {
    const keys:[*]BuildKey=@ptrCast(@alignCast(context));const key=input.*;
    keys[index]=.{.parent=key.parent,.name=key.name.slice(),.flat_index=key.flat_index,
        .first_child=key.first_child,.last_child=key.last_child,.next_sibling=key.next_sibling,
        .child_count=key.child_count,.value_count=key.value_count,.value_first=key.value_first,.value_cursor=key.value_cursor};
}
fn readValue(context:*const anyopaque,index:usize,out:*NativeBuildValue) callconv(.c) void {
    const values:[*]const BuildValue=@ptrCast(@alignCast(context));const value=values[index];
    out.*=.{.key_path=NativeBytes.from(value.key_path),.name=NativeBytes.from(value.name),
        .value_type=value.value_type,.data=NativeBytes.from(value.data)};
}
pub fn parseHive(bytes:[]const u8) Error!HiveView {
    var view:NativeView=undefined;try nativeError(r4os_rust_registry_parse(NativeBytes.from(bytes),&view));
    return HiveView.fromNative(view);
}
pub fn parseRoot(path:[]const u8) ?ParsedRoot {
    var result:NativeRoot=undefined;return if(r4os_rust_registry_root(NativeBytes.from(path),&result))
        .{.kind=result.kind,.rest=result.rest.slice()} else null;
}
pub fn buildHiveInto(out:[]u8,scratch:BuildScratch,kind:HiveKind,generation:u64,values:[]const BuildValue) Error![]const u8 {
    return (try buildHiveViewInto(out,scratch,kind,generation,values)).bytes;
}
pub fn buildHiveViewInto(out:[]u8,scratch:BuildScratch,kind:HiveKind,generation:u64,values:[]const BuildValue) Error!HiveView {
    const input:NativeBuildInput=.{.output=out.ptr,.output_capacity=out.len,.keys=@ptrCast(scratch.keys.ptr),.key_capacity=scratch.keys.len,
        .read_key=readKey,.write_key=writeKey,.values=@ptrCast(values.ptr),.values_len=values.len,.read_value=readValue,
        .owners=scratch.value_key_indices.ptr,.owners_capacity=scratch.value_key_indices.len,
        .flat=scratch.flat_key_order.ptr,.flat_capacity=scratch.flat_key_order.len,
        .grouped=scratch.value_order.ptr,.grouped_capacity=scratch.value_order.len,.kind=kind,.generation=generation};
    var view:NativeView=undefined;try nativeError(r4os_rust_registry_build(&input,&view));return HiveView.fromNative(view);
}
fn nativeError(code:u32) Error!void {return switch(code){
    0=>{},
    1=>Error.FileTooSmall,
    2=>Error.BadMagic,
    3=>Error.BadVersion,
    4=>Error.BadHeaderSize,
    5=>Error.BadEndian,
    6=>Error.BadHiveKind,
    7=>Error.BadFlags,
    8=>Error.BadFileSize,
    9=>Error.BadChecksumType,
    10=>Error.BadChecksum,
    11=>Error.BadReserved,
    12=>Error.BadRange,
    13=>Error.BadRootKey,
    14=>Error.BadKey,
    15=>Error.BadValue,
    16=>Error.BadName,
    17=>Error.BadData,
    18=>Error.DuplicateKey,
    19=>Error.DuplicateValue,
    20=>Error.RootMismatch,
    21=>Error.InvalidPath,
    22=>Error.OutOfMemory,
    23=>Error.TooManyEntries,
    else=>unreachable,
};}
extern fn r4os_rust_registry_parse(NativeBytes,*NativeView) callconv(.c) u32;
extern fn r4os_rust_registry_build(*const NativeBuildInput,*NativeView) callconv(.c) u32;
extern fn r4os_rust_registry_root(NativeBytes,*NativeRoot) callconv(.c) bool;
extern fn r4os_rust_registry_enum(u16,bool) callconv(.c) bool;
extern fn r4os_rust_registry_root_name(u16) callconv(.c) NativeBytes;
extern fn r4os_rust_registry_record(*const NativeView,u32,bool,?*KeyRecord,?*ValueRecord) callconv(.c) void;
extern fn r4os_rust_registry_heap(*const NativeView,u8,?*const KeyRecord,?*const ValueRecord) callconv(.c) NativeBytes;
extern fn r4os_rust_registry_find(*const NativeView,u8,u32,NativeBytes,*u32) callconv(.c) bool;
extern fn r4os_rust_registry_get(*const NativeView,NativeBytes,NativeBytes,*NativeValue) callconv(.c) bool;
extern fn r4os_rust_registry_scalar(*const NativeValue,u8,?*u64,?*NativeBytes) callconv(.c) bool;
comptime { if(@import("builtin").is_test) {_ = @import("rust_owner_test");} }

comptime {
    if(@sizeOf(NativeBytes)!=16 or @alignOf(NativeBytes)!=8) @compileError("Registry C NativeBytes size/alignment");
    if(@offsetOf(NativeBytes,"data")!=0) @compileError("Registry C NativeBytes.data");
    if(@offsetOf(NativeBytes,"len")!=8) @compileError("Registry C NativeBytes.len");
    if(@sizeOf(Header)!=56 or @alignOf(Header)!=8) @compileError("Registry C Header size/alignment");
    if(@offsetOf(Header,"hive_kind")!=0) @compileError("Registry C Header.hive_kind");
    if(@offsetOf(Header,"generation")!=8) @compileError("Registry C Header.generation");
    if(@offsetOf(Header,"key_table_offset")!=16) @compileError("Registry C Header.key_table_offset");
    if(@offsetOf(Header,"key_count")!=20) @compileError("Registry C Header.key_count");
    if(@offsetOf(Header,"value_table_offset")!=24) @compileError("Registry C Header.value_table_offset");
    if(@offsetOf(Header,"value_count")!=28) @compileError("Registry C Header.value_count");
    if(@offsetOf(Header,"string_heap_offset")!=32) @compileError("Registry C Header.string_heap_offset");
    if(@offsetOf(Header,"string_heap_size")!=36) @compileError("Registry C Header.string_heap_size");
    if(@offsetOf(Header,"data_heap_offset")!=40) @compileError("Registry C Header.data_heap_offset");
    if(@offsetOf(Header,"data_heap_size")!=44) @compileError("Registry C Header.data_heap_size");
    if(@offsetOf(Header,"checksum_type")!=48) @compileError("Registry C Header.checksum_type");
    if(@offsetOf(Header,"checksum")!=52) @compileError("Registry C Header.checksum");
    if(@sizeOf(KeyRecord)!=28 or @alignOf(KeyRecord)!=4) @compileError("Registry C KeyRecord size/alignment");
    if(@offsetOf(KeyRecord,"parent_index")!=0) @compileError("Registry C KeyRecord.parent_index");
    if(@offsetOf(KeyRecord,"name_offset")!=4) @compileError("Registry C KeyRecord.name_offset");
    if(@offsetOf(KeyRecord,"name_len")!=8) @compileError("Registry C KeyRecord.name_len");
    if(@offsetOf(KeyRecord,"first_value_index")!=12) @compileError("Registry C KeyRecord.first_value_index");
    if(@offsetOf(KeyRecord,"value_count")!=16) @compileError("Registry C KeyRecord.value_count");
    if(@offsetOf(KeyRecord,"first_child_index")!=20) @compileError("Registry C KeyRecord.first_child_index");
    if(@offsetOf(KeyRecord,"child_count")!=24) @compileError("Registry C KeyRecord.child_count");
    if(@sizeOf(ValueRecord)!=20 or @alignOf(ValueRecord)!=4) @compileError("Registry C ValueRecord size/alignment");
    if(@offsetOf(ValueRecord,"owner_key_index")!=0) @compileError("Registry C ValueRecord.owner_key_index");
    if(@offsetOf(ValueRecord,"name_offset")!=4) @compileError("Registry C ValueRecord.name_offset");
    if(@offsetOf(ValueRecord,"name_len")!=8) @compileError("Registry C ValueRecord.name_len");
    if(@offsetOf(ValueRecord,"value_type")!=10) @compileError("Registry C ValueRecord.value_type");
    if(@offsetOf(ValueRecord,"data_offset")!=12) @compileError("Registry C ValueRecord.data_offset");
    if(@offsetOf(ValueRecord,"data_len")!=16) @compileError("Registry C ValueRecord.data_len");
    if(@sizeOf(NativeView)!=136 or @alignOf(NativeView)!=8) @compileError("Registry C NativeView size/alignment");
    if(@offsetOf(NativeView,"bytes")!=0) @compileError("Registry C NativeView.bytes");
    if(@offsetOf(NativeView,"header")!=16) @compileError("Registry C NativeView.header");
    if(@offsetOf(NativeView,"key_table")!=72) @compileError("Registry C NativeView.key_table");
    if(@offsetOf(NativeView,"value_table")!=88) @compileError("Registry C NativeView.value_table");
    if(@offsetOf(NativeView,"string_heap")!=104) @compileError("Registry C NativeView.string_heap");
    if(@offsetOf(NativeView,"data_heap")!=120) @compileError("Registry C NativeView.data_heap");
    if(@sizeOf(NativeValue)!=24 or @alignOf(NativeValue)!=8) @compileError("Registry C NativeValue size/alignment");
    if(@offsetOf(NativeValue,"value_type")!=0) @compileError("Registry C NativeValue.value_type");
    if(@offsetOf(NativeValue,"data")!=8) @compileError("Registry C NativeValue.data");
    if(@sizeOf(NativeRoot)!=24 or @alignOf(NativeRoot)!=8) @compileError("Registry C NativeRoot size/alignment");
    if(@offsetOf(NativeRoot,"kind")!=0) @compileError("Registry C NativeRoot.kind");
    if(@offsetOf(NativeRoot,"rest")!=8) @compileError("Registry C NativeRoot.rest");
    if(@sizeOf(NativeBuildValue)!=56 or @alignOf(NativeBuildValue)!=8) @compileError("Registry C NativeBuildValue size/alignment");
    if(@offsetOf(NativeBuildValue,"key_path")!=0) @compileError("Registry C NativeBuildValue.key_path");
    if(@offsetOf(NativeBuildValue,"name")!=16) @compileError("Registry C NativeBuildValue.name");
    if(@offsetOf(NativeBuildValue,"value_type")!=32) @compileError("Registry C NativeBuildValue.value_type");
    if(@offsetOf(NativeBuildValue,"data")!=40) @compileError("Registry C NativeBuildValue.data");
    if(@sizeOf(NativeBuildKey)!=56 or @alignOf(NativeBuildKey)!=8) @compileError("Registry C NativeBuildKey size/alignment");
    if(@offsetOf(NativeBuildKey,"parent")!=0) @compileError("Registry C NativeBuildKey.parent");
    if(@offsetOf(NativeBuildKey,"name")!=8) @compileError("Registry C NativeBuildKey.name");
    if(@offsetOf(NativeBuildKey,"flat_index")!=24) @compileError("Registry C NativeBuildKey.flat_index");
    if(@offsetOf(NativeBuildKey,"first_child")!=28) @compileError("Registry C NativeBuildKey.first_child");
    if(@offsetOf(NativeBuildKey,"last_child")!=32) @compileError("Registry C NativeBuildKey.last_child");
    if(@offsetOf(NativeBuildKey,"next_sibling")!=36) @compileError("Registry C NativeBuildKey.next_sibling");
    if(@offsetOf(NativeBuildKey,"child_count")!=40) @compileError("Registry C NativeBuildKey.child_count");
    if(@offsetOf(NativeBuildKey,"value_count")!=44) @compileError("Registry C NativeBuildKey.value_count");
    if(@offsetOf(NativeBuildKey,"value_first")!=48) @compileError("Registry C NativeBuildKey.value_first");
    if(@offsetOf(NativeBuildKey,"value_cursor")!=52) @compileError("Registry C NativeBuildKey.value_cursor");
    if(@sizeOf(NativeBuildInput)!=136 or @alignOf(NativeBuildInput)!=8) @compileError("Registry C NativeBuildInput size/alignment");
    if(@offsetOf(NativeBuildInput,"output")!=0) @compileError("Registry C NativeBuildInput.output");
    if(@offsetOf(NativeBuildInput,"output_capacity")!=8) @compileError("Registry C NativeBuildInput.output_capacity");
    if(@offsetOf(NativeBuildInput,"keys")!=16) @compileError("Registry C NativeBuildInput.keys");
    if(@offsetOf(NativeBuildInput,"key_capacity")!=24) @compileError("Registry C NativeBuildInput.key_capacity");
    if(@offsetOf(NativeBuildInput,"read_key")!=32) @compileError("Registry C NativeBuildInput.read_key");
    if(@offsetOf(NativeBuildInput,"write_key")!=40) @compileError("Registry C NativeBuildInput.write_key");
    if(@offsetOf(NativeBuildInput,"values")!=48) @compileError("Registry C NativeBuildInput.values");
    if(@offsetOf(NativeBuildInput,"values_len")!=56) @compileError("Registry C NativeBuildInput.values_len");
    if(@offsetOf(NativeBuildInput,"read_value")!=64) @compileError("Registry C NativeBuildInput.read_value");
    if(@offsetOf(NativeBuildInput,"owners")!=72) @compileError("Registry C NativeBuildInput.owners");
    if(@offsetOf(NativeBuildInput,"owners_capacity")!=80) @compileError("Registry C NativeBuildInput.owners_capacity");
    if(@offsetOf(NativeBuildInput,"flat")!=88) @compileError("Registry C NativeBuildInput.flat");
    if(@offsetOf(NativeBuildInput,"flat_capacity")!=96) @compileError("Registry C NativeBuildInput.flat_capacity");
    if(@offsetOf(NativeBuildInput,"grouped")!=104) @compileError("Registry C NativeBuildInput.grouped");
    if(@offsetOf(NativeBuildInput,"grouped_capacity")!=112) @compileError("Registry C NativeBuildInput.grouped_capacity");
    if(@offsetOf(NativeBuildInput,"kind")!=120) @compileError("Registry C NativeBuildInput.kind");
    if(@offsetOf(NativeBuildInput,"generation")!=128) @compileError("Registry C NativeBuildInput.generation");
}
