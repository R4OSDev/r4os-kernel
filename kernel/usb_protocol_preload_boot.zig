const boot_info = @import("../bootloader/boot_info.zig");
const log = @import("log.zig");
const r4d = @import("../program/r4d.zig");
const r4p = @import("../program/r4p.zig");

const R4I_MAGIC = "R4I0";
const R4I_VERSION: u16 = 1;
const R4I_HEADER_SIZE: usize = 64;
const R4I_ENTRY_SIZE: usize = 128;
const R4I_MAX_ENTRIES: usize = 32;
const R4I_NAME_OFF: usize = 8;
const R4I_ROLE_OFF: usize = 40;
const R4I_DATA_OFF_FIELD: usize = 88;

const PreloadKind = enum {
    library,
    protocol,
    driver,
};

const R4IEntry = struct {
    name: []const u8,
    role: []const u8,
    kind: u16,
    bytes: []const u8,
};

const R4IView = struct {
    entries: []const R4IEntry,
    r4l_count: usize = 0,
    r4p_count: usize = 0,
    r4d_count: usize = 0,
};

const RequiredModule = struct {
    name: []const u8,
    marker: []const u8,
    kind: PreloadKind,
};

const required = [_]RequiredModule{
    .{ .name = "R4DEV.R4L", .marker = "r4os.preload.r4l=R4DEV", .kind = .library },
    .{ .name = "HIDREPORT.R4P", .marker = "r4os.preload.usb-r4p=HIDREPORT", .kind = .protocol },
    .{ .name = "USBHID.R4P", .marker = "r4os.preload.usb-r4p=USBHID", .kind = .protocol },
    .{ .name = "USBBOT.R4P", .marker = "r4os.preload.usb-r4p=USBBOT", .kind = .protocol },
    .{ .name = "USBSCSI.R4P", .marker = "r4os.preload.usb-r4p=USBSCSI", .kind = .protocol },
    .{ .name = "XHCI.R4D", .marker = "r4os.preload.usb-r4d=XHCI", .kind = .driver },
    .{ .name = "USBMSC.R4D", .marker = "r4os.preload.usb-r4d=USBMSC", .kind = .driver },
    .{ .name = "AHCI.R4D", .marker = "r4os.preload.storage-r4d=AHCI", .kind = .driver },
    .{ .name = "NVME.R4D", .marker = "r4os.preload.storage-r4d=NVME", .kind = .driver },
    .{ .name = "ATAPIO.R4D", .marker = "r4os.preload.storage-r4d=ATAPIO", .kind = .driver },
};

var r4i_entries_storage: [R4I_MAX_ENTRIES]R4IEntry = .{R4IEntry{
    .name = "",
    .role = "",
    .kind = 0,
    .bytes = &.{},
}} ** R4I_MAX_ENTRIES;

pub fn init() bool {
    const modules = boot_info.bootModules();
    const r4i = findPreloadImage(modules);
    var loaded: usize = 0;
    var missing: usize = 0;
    var failed: usize = 0;

    for (required) |req| {
        const bytes = findR4IEntryBytes(r4i, req.name) orelse findLegacyModuleBytes(modules, req.marker) orelse {
            missing += 1;
            log.puts("[USBPRELOAD] missing ");
            log.puts(req.name);
            log.puts("\r\n");
            continue;
        };
        const ok = switch (req.kind) {
            .library => r4p.loadPreloadLibrary(req.name, bytes),
            .protocol => r4p.loadPreloadModule(req.name, bytes),
            .driver => r4d.runtimeLoadSucceeded(r4d.loadPreloadModule(req.name, bytes)),
        };
        if (ok) {
            loaded += 1;
        } else {
            failed += 1;
            log.puts("[USBPRELOAD] failed ");
            log.puts(req.name);
            log.puts("\r\n");
        }
    }

    r4p.finishPreload();
    if (r4i) |image| {
        log.puts("[PRELOAD] R4I entries=");
        log.putDec(image.entries.len);
        log.puts(" r4l=");
        log.putDec(image.r4l_count);
        log.puts(" r4p=");
        log.putDec(image.r4p_count);
        log.puts(" r4d=");
        log.putDec(image.r4d_count);
        log.puts("\r\n");
    }
    log.puts("[USBPRELOAD] status loaded=");
    log.putDec(loaded);
    log.puts("/");
    log.putDec(required.len);
    log.puts(" missing=");
    log.putDec(missing);
    log.puts(" failed=");
    log.putDec(failed);
    log.puts("\r\n");
    return true;
}

fn findPreloadImage(modules: []const boot_info.BootModule) ?R4IView {
    const module = findModule(modules, "r4os.preload.image=PRELOAD.R4I") orelse return null;
    const bytes = module.address[0..module.size];
    const view = parseR4I(bytes) orelse {
        log.puts("[PRELOAD] invalid PRELOAD.R4I\r\n");
        return null;
    };
    return view;
}

fn parseR4I(bytes: []const u8) ?R4IView {
    if (bytes.len < R4I_HEADER_SIZE) return null;
    if (!textEq(bytes[0..4], R4I_MAGIC)) return null;
    if (readLe16(bytes[4..6]) != R4I_VERSION) return null;
    if (readLe16(bytes[6..8]) != R4I_HEADER_SIZE) return null;
    const entry_count: usize = @intCast(readLe32(bytes[8..12]));
    const entries_off: usize = @intCast(readLe32(bytes[12..16]));
    const data_off: usize = @intCast(readLe32(bytes[16..20]));
    const total_size: usize = @intCast(readLe32(bytes[20..24]));
    if (entry_count > R4I_MAX_ENTRIES) return null;
    if (total_size > bytes.len or data_off > total_size) return null;
    const table_size = entry_count * R4I_ENTRY_SIZE;
    if (entries_off + table_size > data_off) return null;

    var view = R4IView{ .entries = r4i_entries_storage[0..entry_count] };
    var i: usize = 0;
    while (i < entry_count) : (i += 1) {
        const off = entries_off + i * R4I_ENTRY_SIZE;
        const record = bytes[off .. off + R4I_ENTRY_SIZE];
        const kind = readLe16(record[0..2]);
        const name_len: usize = readLe16(record[4..6]);
        const role_len: usize = readLe16(record[6..8]);
        const payload_off: usize = @intCast(readLe32(record[R4I_DATA_OFF_FIELD .. R4I_DATA_OFF_FIELD + 4]));
        const payload_len: usize = @intCast(readLe32(record[R4I_DATA_OFF_FIELD + 4 .. R4I_DATA_OFF_FIELD + 8]));
        if (name_len == 0 or name_len > 32 or role_len > 48) return null;
        if (payload_off < data_off or payload_off + payload_len > total_size) return null;
        r4i_entries_storage[i] = .{
            .name = record[R4I_NAME_OFF .. R4I_NAME_OFF + name_len],
            .role = record[R4I_ROLE_OFF .. R4I_ROLE_OFF + role_len],
            .kind = kind,
            .bytes = bytes[payload_off .. payload_off + payload_len],
        };
        switch (kind) {
            1 => view.r4l_count += 1,
            2 => view.r4p_count += 1,
            3 => view.r4d_count += 1,
            else => return null,
        }
    }
    return view;
}

fn findR4IEntryBytes(view: ?R4IView, name: []const u8) ?[]const u8 {
    const image = view orelse return null;
    for (image.entries) |entry| {
        if (textEq(entry.name, name)) return entry.bytes;
    }
    return null;
}

fn findLegacyModuleBytes(modules: []const boot_info.BootModule, marker: []const u8) ?[]const u8 {
    const module = findModule(modules, marker) orelse return null;
    return module.address[0..module.size];
}

fn findModule(modules: []const boot_info.BootModule, marker: []const u8) ?boot_info.BootModule {
    for (modules) |module| {
        if (!module.valid) continue;
        if (textEq(module.cmdline, marker)) return module;
    }
    return null;
}

fn textEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn readLe16(bytes: []const u8) u16 {
    return @as(u16, bytes[0]) | (@as(u16, bytes[1]) << 8);
}

fn readLe32(bytes: []const u8) u32 {
    return @as(u32, bytes[0]) |
        (@as(u32, bytes[1]) << 8) |
        (@as(u32, bytes[2]) << 16) |
        (@as(u32, bytes[3]) << 24);
}
