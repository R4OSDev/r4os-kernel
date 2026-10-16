// Sole installed-font scan/scratch belong to the native catalogue writer.
pub const MAX_R4F_FILE_BYTES: usize = 64 * 1024;
pub const ReloadResult = extern struct { scanned: u32 = 0, registered: u32 = 0, rejected: u32 = 0, unavailable: bool = false };
extern fn r4os_rust_font_reload_installed(*ReloadResult) void;
pub fn reloadInstalled() ReloadResult {
    var out: ReloadResult = undefined;
    r4os_rust_font_reload_installed(&out);
    return out;
}
comptime {
    if (@sizeOf(ReloadResult) != 16 or @alignOf(ReloadResult) != 4 or @offsetOf(ReloadResult, "registered") != 4 or @offsetOf(ReloadResult, "rejected") != 8 or @offsetOf(ReloadResult, "unavailable") != 12) @compileError("Native font catalogue result C layout");
}
