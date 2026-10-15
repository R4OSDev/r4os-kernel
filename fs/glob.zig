// The original bytewise wildcard algorithm is owned by Rust/fs/glob.rs.
extern fn r4os_rust_fs_glob_matches([*]const u8, usize, [*]const u8, usize) callconv(.c) bool;
pub fn matches(pattern: []const u8, name: []const u8) bool {
    return r4os_rust_fs_glob_matches(pattern.ptr, pattern.len, name.ptr, name.len);
}
