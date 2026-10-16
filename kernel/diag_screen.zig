// Private C facade. Native diagnostic band retains the exact first incident.
pub const IncidentToken = struct {
    generation: u64 = 0,
    pub fn valid(self: IncidentToken) bool {
        return self.generation != 0;
    }
};
const TextView = extern struct { pointer: [*]const u8, len: usize };
extern fn r4os_rust_diag_begin(bool) u64;
extern fn r4os_rust_diag_resolve(u64) bool;
extern fn r4os_rust_diag_reset() void;
extern fn r4os_rust_diag_end_line() void;
extern fn r4os_rust_diag_number(u64, bool) void;
extern fn r4os_rust_diag_write([*]const u8, usize) void;
extern fn r4os_rust_diag_text(*TextView) void;
pub fn incidentText() []const u8 {
    var out: TextView = undefined;
    r4os_rust_diag_text(&out);
    return out.pointer[0..out.len];
}
pub fn beginIncident() void {
    _ = r4os_rust_diag_begin(false);
}
pub fn beginResolvableIncident() IncidentToken {
    return .{ .generation = r4os_rust_diag_begin(true) };
}
pub fn resolveIncident(token: IncidentToken) bool {
    return r4os_rust_diag_resolve(token.generation);
}
pub fn resetForExternalDisplay() void {
    r4os_rust_diag_reset();
}
pub fn line(text: []const u8) void {
    write(text);
    endLine();
}
pub fn writeDec(value: u64) void {
    r4os_rust_diag_number(value, false);
}
pub fn writeHex(value: u64) void {
    r4os_rust_diag_number(value, true);
}
pub fn write(text: []const u8) void {
    r4os_rust_diag_write(text.ptr, text.len);
}
pub fn endLine() void {
    r4os_rust_diag_end_line();
}
comptime {
    if (@sizeOf(TextView) != 16 or @alignOf(TextView) != 8 or @offsetOf(TextView, "pointer") != 0 or @offsetOf(TextView, "len") != 8) @compileError("Native diagnostic view C layout");
}
