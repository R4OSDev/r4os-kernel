pub const magic = "R4R1";
pub const header_size: usize = 96;
pub const key_record_size: usize = 32;
pub const value_record_size: usize = 32;
pub const invalid_index: u32 = 0xffff_ffff;

pub const HiveKind = enum(u16) {
    system = 1,
    software = 2,
    desktop = 3,
    user = 4,

    pub fn fromInt(value: u16) ?HiveKind {
        return switch (value) {
            1 => .system,
            2 => .software,
            3 => .desktop,
            4 => .user,
            else => null,
        };
    }

    pub fn shortRoot(self: HiveKind) []const u8 {
        return switch (self) {
            .system => "SYSTEM",
            .software => "SOFTWARE",
            .desktop => "DESKTOP",
            .user => "USER",
        };
    }

    pub fn longRoot(self: HiveKind) []const u8 {
        return switch (self) {
            .system => "SYSTEM",
            .software => "SOFTWARE",
            .desktop => "DESKTOP",
            .user => "USER",
        };
    }
};

pub const ValueType = enum(u16) {
    string = 1,
    u32 = 2,
    u64 = 3,
    bool = 4,
    binary = 5,
    multi_string = 6,

    pub fn fromInt(value: u16) ?ValueType {
        return switch (value) {
            1 => .string,
            2 => .u32,
            3 => .u64,
            4 => .bool,
            5 => .binary,
            6 => .multi_string,
            else => null,
        };
    }
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

pub const Header = struct {
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

pub const KeyRecord = struct {
    parent_index: u32,
    name_offset: u32,
    name_len: u16,
    first_value_index: u32,
    value_count: u32,
    first_child_index: u32,
    child_count: u32,
};

pub const ValueRecord = struct {
    owner_key_index: u32,
    name_offset: u32,
    name_len: u16,
    value_type: ValueType,
    data_offset: u32,
    data_len: u32,
};

pub const Value = struct {
    value_type: ValueType,
    data: []const u8,

    pub fn asString(self: Value) ?[]const u8 {
        return if (self.value_type == .string) self.data else null;
    }

    pub fn asU32(self: Value) ?u32 {
        if (self.value_type != .u32 or self.data.len != 4) return null;
        return readU32(self.data, 0);
    }

    pub fn asU64(self: Value) ?u64 {
        if (self.value_type != .u64 or self.data.len != 8) return null;
        return readU64(self.data, 0);
    }

    pub fn asBool(self: Value) ?bool {
        if (self.value_type != .bool or self.data.len != 1) return null;
        return switch (self.data[0]) {
            0 => false,
            1 => true,
            else => null,
        };
    }
};

pub const ParsedRoot = struct {
    kind: HiveKind,
    rest: []const u8,
};

pub const HiveView = struct {
    bytes: []const u8,
    header: Header,
    key_table: []const u8,
    value_table: []const u8,
    string_heap: []const u8,
    data_heap: []const u8,

    pub fn parse(bytes: []const u8) Error!HiveView {
        return parseHive(bytes);
    }

    pub fn keyAt(self: HiveView, index: u32) KeyRecord {
        return readKeyRecord(self.key_table, index);
    }

    pub fn valueAt(self: HiveView, index: u32) ValueRecord {
        return readValueRecord(self.value_table, index);
    }

    pub fn keyName(self: HiveView, key: KeyRecord) []const u8 {
        return sliceFromHeap(self.string_heap, key.name_offset, key.name_len);
    }

    pub fn valueName(self: HiveView, value: ValueRecord) []const u8 {
        return sliceFromHeap(self.string_heap, value.name_offset, value.name_len);
    }

    pub fn valueData(self: HiveView, value: ValueRecord) []const u8 {
        return sliceFromHeap32(self.data_heap, value.data_offset, value.data_len);
    }

    pub fn findKey(self: HiveView, path: []const u8) ?u32 {
        const parsed = parseRoot(path) orelse return null;
        if (parsed.kind != self.header.hive_kind) return null;
        var current: u32 = 0;
        var rest = parsed.rest;
        while (nextComponent(&rest)) |component| {
            current = self.findChild(current, component) orelse return null;
        }
        return current;
    }

    pub fn findChild(self: HiveView, parent_index: u32, name: []const u8) ?u32 {
        if (parent_index >= self.header.key_count) return null;
        const parent = self.keyAt(parent_index);
        if (parent.child_count == 0 or parent.first_child_index == invalid_index) return null;
        const end = parent.first_child_index + parent.child_count;
        var index = parent.first_child_index;
        while (index < end) : (index += 1) {
            const key = self.keyAt(index);
            if (key.parent_index == parent_index and asciiEqlIgnoreCase(self.keyName(key), name)) return index;
        }
        return null;
    }

    pub fn findValue(self: HiveView, key_index: u32, name: []const u8) ?u32 {
        if (key_index >= self.header.key_count) return null;
        const key = self.keyAt(key_index);
        if (key.value_count == 0 or key.first_value_index == invalid_index) return null;
        const end = key.first_value_index + key.value_count;
        var index = key.first_value_index;
        while (index < end) : (index += 1) {
            const value = self.valueAt(index);
            if (value.owner_key_index == key_index and asciiEqlIgnoreCase(self.valueName(value), name)) return index;
        }
        return null;
    }

    pub fn getValue(self: HiveView, key_path: []const u8, value_name: []const u8) ?Value {
        const key_index = self.findKey(key_path) orelse return null;
        const value_index = self.findValue(key_index, value_name) orelse return null;
        const record = self.valueAt(value_index);
        return .{ .value_type = record.value_type, .data = self.valueData(record) };
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
};

pub const BuildScratch = struct {
    keys: []BuildKey,
    value_key_indices: []u32,
    flat_key_order: []u32,
};

pub fn parseHive(bytes: []const u8) Error!HiveView {
    if (bytes.len < header_size) return Error.FileTooSmall;
    if (!bytesEqual(bytes[0..4], magic)) return Error.BadMagic;
    if (readU16(bytes, 4) != 1) return Error.BadVersion;
    if (readU16(bytes, 6) != header_size) return Error.BadHeaderSize;
    if (readU16(bytes, 8) != 1) return Error.BadEndian;
    const hive_kind = HiveKind.fromInt(readU16(bytes, 10)) orelse return Error.BadHiveKind;
    if (readU32(bytes, 12) != 0) return Error.BadFlags;
    if (readU64(bytes, 16) != bytes.len) return Error.BadFileSize;
    if (readU32(bytes, 64) != 0 or readU32(bytes, 68) != 0) return Error.BadRange;
    const checksum_type = readU32(bytes, 72);
    if (checksum_type != 0 and checksum_type != 1) return Error.BadChecksumType;
    if (!allZero(bytes[80..96])) return Error.BadReserved;
    const checksum = readU32(bytes, 76);
    if (checksum_type == 1 and sum32(bytes) != checksum) return Error.BadChecksum;

    const header = Header{
        .hive_kind = hive_kind,
        .generation = readU64(bytes, 24),
        .key_table_offset = readU32(bytes, 32),
        .key_count = readU32(bytes, 36),
        .value_table_offset = readU32(bytes, 40),
        .value_count = readU32(bytes, 44),
        .string_heap_offset = readU32(bytes, 48),
        .string_heap_size = readU32(bytes, 52),
        .data_heap_offset = readU32(bytes, 56),
        .data_heap_size = readU32(bytes, 60),
        .checksum_type = checksum_type,
        .checksum = checksum,
    };

    if (header.key_count == 0) return Error.BadRootKey;
    const key_table = try tableRange(bytes, header.key_table_offset, header.key_count, key_record_size);
    const value_table = try tableRange(bytes, header.value_table_offset, header.value_count, value_record_size);
    const string_heap = try byteRange(bytes, header.string_heap_offset, header.string_heap_size);
    const data_heap = try byteRange(bytes, header.data_heap_offset, header.data_heap_size);

    const view = HiveView{
        .bytes = bytes,
        .header = header,
        .key_table = key_table,
        .value_table = value_table,
        .string_heap = string_heap,
        .data_heap = data_heap,
    };
    try validate(view);
    return view;
}

pub fn parseRoot(path: []const u8) ?ParsedRoot {
    var text = trimPath(path);
    if (text.len == 0) return null;
    const split = findRootEnd(text);
    const root = text[0..split];
    const kind = rootKind(root) orelse return null;
    if (split < text.len) {
        text = text[split + 1 ..];
    } else {
        text = text[split..];
    }
    return .{ .kind = kind, .rest = trimSeparators(text) };
}

pub fn buildHiveInto(out: []u8, scratch: BuildScratch, kind: HiveKind, generation: u64, values: []const BuildValue) Error![]const u8 {
    return (try buildHiveViewInto(out, scratch, kind, generation, values)).bytes;
}

pub fn buildHiveViewInto(out: []u8, scratch: BuildScratch, kind: HiveKind, generation: u64, values: []const BuildValue) Error!HiveView {
    if (scratch.keys.len == 0 or scratch.flat_key_order.len == 0) return Error.TooManyEntries;
    if (values.len > scratch.value_key_indices.len) return Error.TooManyEntries;

    var key_count: u32 = 1;
    scratch.keys[0] = .{ .parent = invalid_index, .name = "" };

    var value_index: usize = 0;
    while (value_index < values.len) : (value_index += 1) {
        const value = values[value_index];
        try validateValuePayload(value.value_type, value.data);
        if (!validValueName(value.name)) return Error.BadName;
        const key_index = try ensureBuildKey(scratch.keys, kind, value.key_path, &key_count);
        var prior: usize = 0;
        while (prior < value_index) : (prior += 1) {
            if (scratch.value_key_indices[prior] == key_index and asciiEqlIgnoreCase(values[prior].name, value.name)) {
                return Error.DuplicateValue;
            }
        }
        scratch.value_key_indices[value_index] = key_index;
    }

    var flat_count: u32 = 0;
    scratch.flat_key_order[flat_count] = 0;
    scratch.keys[0].flat_index = 0;
    flat_count += 1;
    var cursor: u32 = 0;
    while (cursor < flat_count) : (cursor += 1) {
        const parent = scratch.flat_key_order[cursor];
        var child: u32 = 1;
        while (child < key_count) : (child += 1) {
            if (scratch.keys[child].parent == parent) {
                if (flat_count >= scratch.flat_key_order.len) return Error.TooManyEntries;
                scratch.keys[child].flat_index = flat_count;
                scratch.flat_key_order[flat_count] = child;
                flat_count += 1;
            }
        }
    }

    var string_heap_size: usize = 0;
    var data_heap_size: usize = 0;
    var flat_i: u32 = 0;
    while (flat_i < flat_count) : (flat_i += 1) {
        const build_i = scratch.flat_key_order[flat_i];
        string_heap_size += scratch.keys[build_i].name.len;
        value_index = 0;
        while (value_index < values.len) : (value_index += 1) {
            if (scratch.value_key_indices[value_index] == build_i) {
                string_heap_size += values[value_index].name.len;
                data_heap_size += values[value_index].data.len;
            }
        }
    }

    const key_table_offset = header_size;
    const key_table_size = @as(usize, key_count) * key_record_size;
    const value_table_offset = key_table_offset + key_table_size;
    const value_table_size = values.len * value_record_size;
    const string_heap_offset = value_table_offset + value_table_size;
    const data_heap_offset = string_heap_offset + string_heap_size;
    const file_size = data_heap_offset + data_heap_size;
    if (file_size > 0xffff_ffff) return Error.TooManyEntries;
    if (file_size > out.len) return Error.OutOfMemory;

    @memset(out[0..file_size], 0);

    var string_cursor: usize = 0;
    var data_cursor: usize = 0;
    var flat_value_index: u32 = 0;
    flat_i = 0;
    while (flat_i < flat_count) : (flat_i += 1) {
        const build_i = scratch.flat_key_order[flat_i];
        const key = scratch.keys[build_i];
        const name_offset = try appendBuildString(out, key.name, string_heap_offset, &string_cursor);
        const child_info = childRangeForFlat(scratch.keys, build_i, key_count);
        const value_info = valueRangeForBuild(values, scratch.value_key_indices, build_i);
        const first_value = if (value_info.count == 0) invalid_index else flat_value_index;
        writeKeyRecord(
            out,
            key_table_offset + @as(usize, flat_i) * key_record_size,
            if (key.parent == invalid_index) invalid_index else scratch.keys[key.parent].flat_index,
            name_offset,
            @intCast(key.name.len),
            first_value,
            value_info.count,
            child_info.first,
            child_info.count,
        );

        value_index = 0;
        while (value_index < values.len) : (value_index += 1) {
            if (scratch.value_key_indices[value_index] != build_i) continue;
            const value = values[value_index];
            const value_name_offset = try appendBuildString(out, value.name, string_heap_offset, &string_cursor);
            const data_offset = try appendBuildData(out, value.data, data_heap_offset, &data_cursor);
            writeValueRecord(
                out,
                value_table_offset + @as(usize, flat_value_index) * value_record_size,
                flat_i,
                value_name_offset,
                @intCast(value.name.len),
                value.value_type,
                data_offset,
                @intCast(value.data.len),
            );
            flat_value_index += 1;
        }
    }

    @memcpy(out[0..4], magic);
    writeU16(out, 4, 1);
    writeU16(out, 6, @intCast(header_size));
    writeU16(out, 8, 1);
    writeU16(out, 10, @intFromEnum(kind));
    writeU64(out, 16, @intCast(file_size));
    writeU64(out, 24, generation);
    writeU32(out, 32, @intCast(key_table_offset));
    writeU32(out, 36, key_count);
    writeU32(out, 40, if (values.len == 0) 0 else @as(u32, @intCast(value_table_offset)));
    writeU32(out, 44, @intCast(values.len));
    writeU32(out, 48, if (string_heap_size == 0) 0 else @as(u32, @intCast(string_heap_offset)));
    writeU32(out, 52, @intCast(string_heap_size));
    writeU32(out, 56, if (data_heap_size == 0) 0 else @as(u32, @intCast(data_heap_offset)));
    writeU32(out, 60, @intCast(data_heap_size));

    return HiveView.parse(out[0..file_size]);
}

fn validate(view: HiveView) Error!void {
    const root = view.keyAt(0);
    if (root.parent_index != invalid_index or root.name_len != 0 or root.name_offset != 0) return Error.BadRootKey;

    var key_index: u32 = 0;
    while (key_index < view.header.key_count) : (key_index += 1) {
        try validateKeyReserved(view, key_index);
        const key = view.keyAt(key_index);
        if (key.parent_index != invalid_index and key.parent_index >= view.header.key_count) return Error.BadKey;
        if (key_index != 0 and key.parent_index == invalid_index) return Error.BadKey;
        if (key_index != 0 and !keyListedByParent(view, key_index, key.parent_index)) return Error.BadKey;
        if (key_index != 0) try validateNameSlice(view.string_heap, key.name_offset, key.name_len, false);
        try validateIndexRange(key.first_child_index, key.child_count, view.header.key_count);
        try validateIndexRange(key.first_value_index, key.value_count, view.header.value_count);

        var child_offset: u32 = 0;
        while (child_offset < key.child_count) : (child_offset += 1) {
            const child_index = key.first_child_index + child_offset;
            if (view.keyAt(child_index).parent_index != key_index) return Error.BadKey;
        }

        var value_offset: u32 = 0;
        while (value_offset < key.value_count) : (value_offset += 1) {
            const value_index = key.first_value_index + value_offset;
            if (view.valueAt(value_index).owner_key_index != key_index) return Error.BadValue;
        }
    }

    var value_index: u32 = 0;
    while (value_index < view.header.value_count) : (value_index += 1) {
        try validateValueReservedAndType(view, value_index);
        const value = view.valueAt(value_index);
        if (value.owner_key_index >= view.header.key_count) return Error.BadValue;
        if (!valueListedByOwner(view, value_index, value.owner_key_index)) return Error.BadValue;
        try validateNameSlice(view.string_heap, value.name_offset, value.name_len, true);
        const data = try checkedDataSlice(view.data_heap, value.data_offset, value.data_len);
        try validateValuePayload(value.value_type, data);
    }

    try validateDuplicateKeys(view);
    try validateDuplicateValues(view);
}

fn ensureBuildKey(keys: []BuildKey, kind: HiveKind, path: []const u8, key_count: *u32) Error!u32 {
    const parsed = parseRoot(path) orelse return Error.InvalidPath;
    if (parsed.kind != kind) return Error.RootMismatch;
    var current: u32 = 0;
    var rest = parsed.rest;
    while (nextComponent(&rest)) |component| {
        if (!validKeyName(component)) return Error.BadName;
        if (findBuildChild(keys, current, component, key_count.*)) |child| {
            current = child;
            continue;
        }
        if (key_count.* >= keys.len) return Error.TooManyEntries;
        const next_index = key_count.*;
        keys[next_index] = .{ .parent = current, .name = component };
        key_count.* += 1;
        current = next_index;
    }
    return current;
}

fn findBuildChild(keys: []const BuildKey, parent: u32, name: []const u8, key_count: u32) ?u32 {
    var index: u32 = 1;
    while (index < key_count) : (index += 1) {
        if (keys[index].parent == parent and asciiEqlIgnoreCase(keys[index].name, name)) return index;
    }
    return null;
}

const RangeInfo = struct {
    first: u32,
    count: u32,
};

fn childRangeForFlat(keys: []const BuildKey, build_index: u32, key_count: u32) RangeInfo {
    var first: u32 = invalid_index;
    var count: u32 = 0;
    var index: u32 = 1;
    while (index < key_count) : (index += 1) {
        if (keys[index].parent == build_index) {
            if (first == invalid_index) first = keys[index].flat_index;
            count += 1;
        }
    }
    return .{ .first = first, .count = count };
}

fn valueRangeForBuild(values: []const BuildValue, value_key_indices: []const u32, build_index: u32) RangeInfo {
    var count: u32 = 0;
    var index: usize = 0;
    while (index < values.len) : (index += 1) {
        if (value_key_indices[index] == build_index) count += 1;
    }
    return .{ .first = invalid_index, .count = count };
}

fn appendBuildString(out: []u8, text: []const u8, heap_offset: usize, cursor: *usize) Error!u32 {
    if (text.len > 0xffff) return Error.TooManyEntries;
    const start = cursor.*;
    if (heap_offset + start + text.len > out.len) return Error.OutOfMemory;
    if (text.len != 0) @memcpy(out[heap_offset + start .. heap_offset + start + text.len], text);
    cursor.* += text.len;
    return @intCast(start);
}

fn appendBuildData(out: []u8, data: []const u8, heap_offset: usize, cursor: *usize) Error!u32 {
    const start = cursor.*;
    if (heap_offset + start + data.len > out.len) return Error.OutOfMemory;
    if (data.len != 0) @memcpy(out[heap_offset + start .. heap_offset + start + data.len], data);
    cursor.* += data.len;
    return @intCast(start);
}

fn validateDuplicateKeys(view: HiveView) Error!void {
    var parent_index: u32 = 0;
    while (parent_index < view.header.key_count) : (parent_index += 1) {
        const parent = view.keyAt(parent_index);
        var a: u32 = 0;
        while (a < parent.child_count) : (a += 1) {
            const key_a = view.keyAt(parent.first_child_index + a);
            var b = a + 1;
            while (b < parent.child_count) : (b += 1) {
                const key_b = view.keyAt(parent.first_child_index + b);
                if (asciiEqlIgnoreCase(view.keyName(key_a), view.keyName(key_b))) return Error.DuplicateKey;
            }
        }
    }
}

fn validateDuplicateValues(view: HiveView) Error!void {
    var key_index: u32 = 0;
    while (key_index < view.header.key_count) : (key_index += 1) {
        const key = view.keyAt(key_index);
        var a: u32 = 0;
        while (a < key.value_count) : (a += 1) {
            const value_a = view.valueAt(key.first_value_index + a);
            var b = a + 1;
            while (b < key.value_count) : (b += 1) {
                const value_b = view.valueAt(key.first_value_index + b);
                if (asciiEqlIgnoreCase(view.valueName(value_a), view.valueName(value_b))) return Error.DuplicateValue;
            }
        }
    }
}

fn validateKeyReserved(view: HiveView, key_index: u32) Error!void {
    const offset = @as(usize, key_index) * key_record_size;
    if (readU16(view.key_table, offset + 10) != 0) return Error.BadReserved;
    if (readU32(view.key_table, offset + 28) != 0) return Error.BadReserved;
}

fn validateValueReservedAndType(view: HiveView, value_index: u32) Error!void {
    const offset = @as(usize, value_index) * value_record_size;
    if (ValueType.fromInt(readU16(view.value_table, offset + 10)) == null) return Error.BadValue;
    if (readU32(view.value_table, offset + 12) != 0) return Error.BadReserved;
    if (readU64(view.value_table, offset + 24) != 0) return Error.BadReserved;
}

fn keyListedByParent(view: HiveView, key_index: u32, parent_index: u32) bool {
    if (parent_index >= view.header.key_count) return false;
    const parent = view.keyAt(parent_index);
    return indexInRange(parent.first_child_index, parent.child_count, key_index);
}

fn valueListedByOwner(view: HiveView, value_index: u32, owner_key_index: u32) bool {
    if (owner_key_index >= view.header.key_count) return false;
    const owner = view.keyAt(owner_key_index);
    return indexInRange(owner.first_value_index, owner.value_count, value_index);
}

fn indexInRange(first: u32, count: u32, index: u32) bool {
    if (count == 0 or first == invalid_index or index < first) return false;
    return index - first < count;
}

fn readKeyRecord(table: []const u8, index: u32) KeyRecord {
    const offset = @as(usize, index) * key_record_size;
    return .{
        .parent_index = readU32(table, offset + 0),
        .name_offset = readU32(table, offset + 4),
        .name_len = readU16(table, offset + 8),
        .first_value_index = readU32(table, offset + 12),
        .value_count = readU32(table, offset + 16),
        .first_child_index = readU32(table, offset + 20),
        .child_count = readU32(table, offset + 24),
    };
}

fn readValueRecord(table: []const u8, index: u32) ValueRecord {
    const offset = @as(usize, index) * value_record_size;
    return .{
        .owner_key_index = readU32(table, offset + 0),
        .name_offset = readU32(table, offset + 4),
        .name_len = readU16(table, offset + 8),
        .value_type = ValueType.fromInt(readU16(table, offset + 10)) orelse .binary,
        .data_offset = readU32(table, offset + 16),
        .data_len = readU32(table, offset + 20),
    };
}

fn writeKeyRecord(out: []u8, offset: usize, parent: u32, name_offset: u32, name_len: u16, first_value: u32, value_count: u32, first_child: u32, child_count: u32) void {
    writeU32(out, offset + 0, parent);
    writeU32(out, offset + 4, name_offset);
    writeU16(out, offset + 8, name_len);
    writeU32(out, offset + 12, first_value);
    writeU32(out, offset + 16, value_count);
    writeU32(out, offset + 20, first_child);
    writeU32(out, offset + 24, child_count);
}

fn writeValueRecord(out: []u8, offset: usize, owner: u32, name_offset: u32, name_len: u16, value_type: ValueType, data_offset: u32, data_len: u32) void {
    writeU32(out, offset + 0, owner);
    writeU32(out, offset + 4, name_offset);
    writeU16(out, offset + 8, name_len);
    writeU16(out, offset + 10, @intFromEnum(value_type));
    writeU32(out, offset + 16, data_offset);
    writeU32(out, offset + 20, data_len);
}

fn tableRange(bytes: []const u8, offset: u32, count: u32, record_size: usize) Error![]const u8 {
    if (count == 0) {
        if (offset != 0) return Error.BadRange;
        return bytes[0..0];
    }
    const size = checkedMul(count, record_size) orelse return Error.BadRange;
    return byteRange(bytes, offset, size);
}

fn byteRange(bytes: []const u8, offset: u32, size: u32) Error![]const u8 {
    if (size == 0) {
        if (offset == 0) return bytes[0..0];
        if (offset > bytes.len) return Error.BadRange;
        return bytes[@intCast(offset)..@intCast(offset)];
    }
    const start: usize = @intCast(offset);
    const len: usize = @intCast(size);
    if (start > bytes.len or len > bytes.len - start) return Error.BadRange;
    return bytes[start .. start + len];
}

fn checkedDataSlice(heap: []const u8, offset: u32, size: u32) Error![]const u8 {
    return byteRange(heap, offset, size);
}

fn sliceFromHeap(heap: []const u8, offset: u32, len: u16) []const u8 {
    if (len == 0) return heap[0..0];
    const start: usize = @intCast(offset);
    return heap[start .. start + len];
}

fn sliceFromHeap32(heap: []const u8, offset: u32, len: u32) []const u8 {
    if (len == 0) return heap[0..0];
    const start: usize = @intCast(offset);
    const count: usize = @intCast(len);
    return heap[start .. start + count];
}

fn validateIndexRange(first: u32, count: u32, total: u32) Error!void {
    if (count == 0) {
        if (first != invalid_index) return Error.BadRange;
        return;
    }
    if (first == invalid_index or first >= total) return Error.BadRange;
    if (count > total - first) return Error.BadRange;
}

fn validateNameSlice(heap: []const u8, offset: u32, len: u16, allow_empty: bool) Error!void {
    if (len == 0) {
        if (!allow_empty and offset != 0) return Error.BadName;
        return;
    }
    const name = try byteRange(heap, offset, len);
    if (allow_empty) {
        if (!validValueName(name)) return Error.BadName;
    } else {
        if (!validKeyName(name)) return Error.BadName;
    }
}

fn validateValuePayload(value_type: ValueType, data: []const u8) Error!void {
    switch (value_type) {
        .string, .binary => {},
        .u32 => if (data.len != 4) return Error.BadData,
        .u64 => if (data.len != 8) return Error.BadData,
        .bool => if (data.len != 1 or (data[0] != 0 and data[0] != 1)) return Error.BadData,
        .multi_string => try validateMultiString(data),
    }
}

fn validateMultiString(data: []const u8) Error!void {
    if (data.len < 4) return Error.BadData;
    const count = readU32(data, 0);
    var offset: usize = 4;
    var index: u32 = 0;
    while (index < count) : (index += 1) {
        if (offset + 2 > data.len) return Error.BadData;
        const len = readU16(data, offset);
        offset += 2;
        if (offset + len > data.len) return Error.BadData;
        offset += len;
    }
    if (offset != data.len) return Error.BadData;
}

fn validKeyName(name: []const u8) bool {
    return validName(name, false);
}

fn validValueName(name: []const u8) bool {
    return validName(name, true);
}

fn validName(name: []const u8, allow_empty: bool) bool {
    if (name.len == 0) return allow_empty;
    if (name.len > 63) return false;
    for (name) |ch| {
        if (ch < 0x20 or ch == 0x7f or ch == 0 or ch == '\\' or ch == '/' or ch == '=') return false;
    }
    return true;
}

fn nextComponent(rest: *[]const u8) ?[]const u8 {
    rest.* = trimSeparators(rest.*);
    if (rest.*.len == 0) return null;
    const split = findRootEnd(rest.*);
    const component = rest.*[0..split];
    if (split < rest.*.len) {
        rest.* = rest.*[split + 1 ..];
    } else {
        rest.* = rest.*[split..];
    }
    return component;
}

fn findRootEnd(path: []const u8) usize {
    var index: usize = 0;
    while (index < path.len) : (index += 1) {
        if (path[index] == '\\' or path[index] == '/') return index;
    }
    return path.len;
}

fn trimPath(path: []const u8) []const u8 {
    var start: usize = 0;
    var end = path.len;
    while (start < end and isSpace(path[start])) : (start += 1) {}
    while (end > start and isSpace(path[end - 1])) : (end -= 1) {}
    return path[start..end];
}

fn trimSeparators(path: []const u8) []const u8 {
    var start: usize = 0;
    var end = path.len;
    while (start < end and (path[start] == '\\' or path[start] == '/')) : (start += 1) {}
    while (end > start and (path[end - 1] == '\\' or path[end - 1] == '/')) : (end -= 1) {}
    return path[start..end];
}

fn rootKind(root: []const u8) ?HiveKind {
    if (asciiEqlIgnoreCase(root, "SYSTEM")) return .system;
    if (asciiEqlIgnoreCase(root, "SOFTWARE")) return .software;
    if (asciiEqlIgnoreCase(root, "DESKTOP")) return .desktop;
    if (asciiEqlIgnoreCase(root, "USER")) return .user;
    return null;
}

fn checkedMul(count: u32, size: usize) ?u32 {
    const result = @as(u64, count) * @as(u64, @intCast(size));
    if (result > 0xffff_ffff) return null;
    return @intCast(result);
}

fn allZero(bytes: []const u8) bool {
    for (bytes) |byte| {
        if (byte != 0) return false;
    }
    return true;
}

fn sum32(bytes: []const u8) u32 {
    var sum: u32 = 0;
    for (bytes, 0..) |byte, index| {
        if (index >= 76 and index < 80) continue;
        sum +%= byte;
    }
    return sum;
}

fn bytesEqual(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        if (a[index] != b[index]) return false;
    }
    return true;
}

fn asciiEqlIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        if (asciiUpper(a[index]) != asciiUpper(b[index])) return false;
    }
    return true;
}

fn asciiUpper(ch: u8) u8 {
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
}

fn isSpace(ch: u8) bool {
    return ch == ' ' or ch == '\t' or ch == '\r' or ch == '\n';
}

fn readU16(bytes: []const u8, offset: usize) u16 {
    return @as(u16, bytes[offset]) |
        (@as(u16, bytes[offset + 1]) << 8);
}

fn readU32(bytes: []const u8, offset: usize) u32 {
    return @as(u32, bytes[offset]) |
        (@as(u32, bytes[offset + 1]) << 8) |
        (@as(u32, bytes[offset + 2]) << 16) |
        (@as(u32, bytes[offset + 3]) << 24);
}

fn readU64(bytes: []const u8, offset: usize) u64 {
    return @as(u64, readU32(bytes, offset)) |
        (@as(u64, readU32(bytes, offset + 4)) << 32);
}

fn writeU16(bytes: []u8, offset: usize, value: u16) void {
    bytes[offset + 0] = @intCast(value & 0xff);
    bytes[offset + 1] = @intCast((value >> 8) & 0xff);
}

fn writeU32(bytes: []u8, offset: usize, value: u32) void {
    bytes[offset + 0] = @intCast(value & 0xff);
    bytes[offset + 1] = @intCast((value >> 8) & 0xff);
    bytes[offset + 2] = @intCast((value >> 16) & 0xff);
    bytes[offset + 3] = @intCast((value >> 24) & 0xff);
}

fn writeU64(bytes: []u8, offset: usize, value: u64) void {
    writeU32(bytes, offset, @intCast(value & 0xffff_ffff));
    writeU32(bytes, offset + 4, @intCast((value >> 32) & 0xffff_ffff));
}
