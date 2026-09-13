// Bounded copied metadata only. GPU ELD writes, HDA verbs and policy belong
// to their R4D owners. The enclosing receiver catalog validates the source.
const std = @import("std");
const a = @import("r4os_kernel_contract");
pub const Error = error{ Invalid, Stale, Capacity, Routing };
pub const Store = struct {
    records: [a.gfx_output_catalog_capacity]a.GfxAudioRoute = @splat(.{}),

    pub fn publish(self: *Store, value: *const a.GfxAudioRoute) Error!void {
        if (value.version != 1 or value.size < @sizeOf(a.GfxAudioRoute) or value.reserved0 != 0 or
            value.source.adapter_id == 0 or value.source.generation == 0 or value.source.reserved0 != 0 or
            value.revision == 0 or value.connector_id == 0 or value.hda_location >> 24 != 1 or
            value.hda_location & 0x00ff0000 != 0 or value.hda_device & 0xffff == 0 or value.hda_device >> 16 == 0 or
            value.head_id >= 32 or value.device_entry > 3 or value.state > a.gfx_audio_route_failed) return error.Invalid;
        if (value.state == a.gfx_audio_route_ready) {
            if (value.receiver_sequence == 0 or value.eld_bytes < 20 or value.eld_bytes > value.eld.len or value.eld_bytes % 4 != 0 or
                !std.mem.eql(u8, &value.port_id, value.eld[8..16])) return error.Invalid;
        } else if (value.eld_bytes != 0) return error.Invalid;
        for (value.eld[value.eld_bytes..]) |byte| if (byte != 0) return error.Invalid;
        var chosen: ?*a.GfxAudioRoute = null;
        var vacant: ?*a.GfxAudioRoute = null;
        for (&self.records) |*record| {
            if (record.source.generation == 0) { if (vacant == null) vacant = record; continue; }
            if (std.meta.eql(record.source, value.source) and record.connector_id == value.connector_id) {
                if (record.hda_location != value.hda_location or record.hda_device != value.hda_device or
                    !std.mem.eql(u8, &record.port_id, &value.port_id)) return error.Routing;
                if (value.revision < record.revision) return error.Stale;
                if (value.revision == record.revision) {
                    if (std.meta.eql(record.*, value.*)) return;
                    return error.Stale;
                }
                chosen = record;
            } else if (record.hda_location == value.hda_location and record.hda_device == value.hda_device and
                std.mem.eql(u8, &record.port_id, &value.port_id)) return error.Routing;
        }
        (chosen orelse vacant orelse return error.Capacity).* = value.*;
    }
    pub fn query(self: *const Store, location: u32, device: u32, index: u32) ?a.GfxAudioRoute {
        var ordinal: u32 = 0;
        for (&self.records) |*record| {
            if (record.source.generation == 0 or record.hda_location != location or record.hda_device != device) continue;
            if (ordinal == index) return record.*;
            ordinal += 1;
        }
        return null;
    }
    pub fn invalidate(self: *Store, generation: u64, sequence: u64) void {
        for (&self.records) |*record| if (record.source.generation == generation) {
            record.receiver_sequence = sequence;
            record.state = a.gfx_audio_route_pending;
            record.eld_bytes = 0;
            @memset(&record.eld, 0);
        };
    }
    pub fn erase(self: *Store, generation: u64) bool {
        var changed = false;
        for (&self.records) |*record| if (record.source.generation == generation) {
            record.* = .{};
            changed = true;
        };
        return changed;
    }
};
