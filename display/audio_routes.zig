//! Fixed native receiver/HDA metadata owner. No audio engine or device I/O.
const a = @import("r4os_kernel_contract");
pub const Error = error{ Invalid, Stale, Capacity, Routing };
extern fn r4os_rust_display_audio_routes(u32, *const Store, ?*const a.GfxAudioRoute, u32, u32, u32, u64, u64, ?*a.GfxAudioRoute) i32;
pub const Store = extern struct {
    records: [a.gfx_output_catalog_capacity]a.GfxAudioRoute = @splat(.{}),
    pub fn publish(self: *Store, value: *const a.GfxAudioRoute) Error!void {
        return switch (r4os_rust_display_audio_routes(0, self, value, 0, 0, 0, 0, 0, null)) {
            0 => {}, 1 => error.Invalid, 6 => error.Capacity, 7 => error.Stale, 18 => error.Routing, else => unreachable,
        };
    }
    pub fn query(self: *const Store, location: u32, device: u32, index: u32) ?a.GfxAudioRoute {
        var out: a.GfxAudioRoute = undefined; return if (r4os_rust_display_audio_routes(1, self, null, location, device, index, 0, 0, &out) != 0) out else null;
    }
    pub fn invalidate(self: *Store, generation: u64, sequence: u64) void { _ = r4os_rust_display_audio_routes(2, self, null, 0, 0, 0, generation, sequence, null); }
    pub fn erase(self: *Store, generation: u64) bool { return r4os_rust_display_audio_routes(3, self, null, 0, 0, 0, generation, 0, null) != 0; }
};
comptime { if (@sizeOf(Store) != 11264 or @alignOf(Store) != 8) @compileError("Native receiver audio route C layout"); }
