pub const MAX_CELLS: usize = 65_536;

pub const Cell = struct {
    glyph: u8 = ' ',
    foreground: u32 = 0,
    background: u32 = 0,
};

pub fn requiredCellCount(cols: u32, rows: u32) ?usize {
    const count = @as(u64, cols) * @as(u64, rows);
    if (count > MAX_CELLS or count > @as(u64, @intCast(~@as(usize, 0)))) return null;
    return @intCast(count);
}

pub fn fill(cells: []Cell, value: Cell) void {
    for (cells) |*cell| cell.* = value;
}

// Verschiebt eine rechteckige Zellregion um genau eine Textzeile nach oben.
// Top-to-bottom ist fuer die ueberlappenden Quell-/Zielzeilen absichtlich die
// sichere Richtung. Linker/rechter Rand und Zellen ausserhalb der Region
// bleiben unveraendert.
pub fn scrollUp(
    cells: []Cell,
    cols: u32,
    rows: u32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    blank: Cell,
) bool {
    const required = requiredCellCount(cols, rows) orelse return false;
    if (required > cells.len or left >= right or top >= bottom or
        right > cols or bottom > rows or bottom - top <= 1)
    {
        return false;
    }

    var row = top;
    while (row + 1 < bottom) : (row += 1) {
        var col = left;
        while (col < right) : (col += 1) {
            cells[index(cols, row, col)] = cells[index(cols, row + 1, col)];
        }
    }

    var col = left;
    while (col < right) : (col += 1) {
        cells[index(cols, bottom - 1, col)] = blank;
    }
    return true;
}

fn index(cols: u32, row: u32, col: u32) usize {
    return @as(usize, row) * @as(usize, cols) + @as(usize, col);
}

test "scroll keeps margins and moves per-cell colors" {
    const testing = @import("std").testing;
    var cells: [20]Cell = undefined;
    var row: u32 = 0;
    while (row < 4) : (row += 1) {
        var col: u32 = 0;
        while (col < 5) : (col += 1) {
            const value: u8 = @intCast(row * 10 + col);
            cells[index(5, row, col)] = .{
                .glyph = value,
                .foreground = 0x1000 + @as(u32, value),
                .background = 0x2000 + @as(u32, value),
            };
        }
    }
    const before = cells;
    const blank: Cell = .{ .glyph = ' ', .foreground = 0xAABBCC, .background = 0x112233 };

    try testing.expect(scrollUp(cells[0..], 5, 4, 1, 1, 4, 4, blank));
    try expectCell(testing, cells[index(5, 1, 1)], before[index(5, 2, 1)]);
    try expectCell(testing, cells[index(5, 2, 3)], before[index(5, 3, 3)]);
    try expectCell(testing, cells[index(5, 3, 2)], blank);

    // Oberer, linker und rechter Rand gehoeren nicht zur Scrollregion.
    try expectCell(testing, cells[index(5, 0, 2)], before[index(5, 0, 2)]);
    try expectCell(testing, cells[index(5, 2, 0)], before[index(5, 2, 0)]);
    try expectCell(testing, cells[index(5, 2, 4)], before[index(5, 2, 4)]);
}

test "invalid scroll geometry leaves the grid untouched" {
    const testing = @import("std").testing;
    var cells = [_]Cell{.{ .glyph = 'X', .foreground = 1, .background = 2 }} ** 6;
    const before = cells;
    try testing.expect(!scrollUp(cells[0..], 3, 2, 0, 0, 4, 2, .{}));
    var i: usize = 0;
    while (i < cells.len) : (i += 1) try expectCell(testing, cells[i], before[i]);
}

fn expectCell(testing: type, actual: Cell, expected: Cell) !void {
    try testing.expectEqual(expected.glyph, actual.glyph);
    try testing.expectEqual(expected.foreground, actual.foreground);
    try testing.expectEqual(expected.background, actual.background);
}
