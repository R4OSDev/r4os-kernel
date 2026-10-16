// Immutable layout table resides at the native owner.
pub fn definition() *const @import("../key_layout_types.zig").LayoutDefinition {
    return @import("../key_layout.zig").definition(.de_de);
}
