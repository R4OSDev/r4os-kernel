pub fn matches(pattern_raw: []const u8, name: []const u8) bool {
    const pattern = if (pattern_raw.len == 0) "*.*" else pattern_raw;
    if (isAllPattern(pattern)) return true;
    return matchAt(pattern, 0, name, 0);
}

fn matchAt(pattern: []const u8, pi: usize, name: []const u8, ni: usize) bool {
    if (pi >= pattern.len) return ni >= name.len;

    const pc = pattern[pi];
    if (pc == '*') {
        var cursor = ni;
        while (cursor <= name.len) : (cursor += 1) {
            if (matchAt(pattern, pi + 1, name, cursor)) return true;
        }
        return false;
    }

    if (ni >= name.len) return false;
    if (pc == '?' or upper(pc) == upper(name[ni])) {
        return matchAt(pattern, pi + 1, name, ni + 1);
    }
    return false;
}

fn isAllPattern(pattern: []const u8) bool {
    return pattern.len == 1 and pattern[0] == '*' or
        pattern.len == 3 and pattern[0] == '*' and pattern[1] == '.' and pattern[2] == '*';
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}
