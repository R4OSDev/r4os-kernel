// Native boot-display/console owner; fixed copied C records only.
pub const ConsoleMetrics = extern struct {
    cols: u32 = 0,
    rows: u32 = 0,
    scale: u32 = 0,
};

pub const State = extern struct {
    console_metrics: ConsoleMetrics,
};

pub fn init() ?State {
    var out: State = undefined;
    return if (r4os_rust_boot_display_init(&out)) out else null;
}

pub fn get() ?State {
    var out: State = undefined;
    return if (r4os_rust_boot_display_get(&out)) out else null;
}

// Called once after heap initialization, outside the boot-log owner. Storage
// stays resident for later console/fatal use; sink callbacks never allocate.
pub fn prepareConsolePixelBacking(memory_budget: u64) bool {
    return r4os_rust_boot_display_prepare(memory_budget);
}

pub fn invalidateConsoleBackingForExternalDisplay() void {
    r4os_rust_boot_display_invalidate();
}

extern fn r4os_rust_boot_display_init(*State) bool;
extern fn r4os_rust_boot_display_get(*State) bool;
extern fn r4os_rust_boot_display_prepare(u64) bool;
extern fn r4os_rust_boot_display_invalidate() void;

comptime {
    if (@sizeOf(ConsoleMetrics) != 12 or @alignOf(ConsoleMetrics) != 4 or @offsetOf(ConsoleMetrics, "rows") != 4 or @offsetOf(ConsoleMetrics, "scale") != 8 or @sizeOf(State) != 12 or @alignOf(State) != 4 or @offsetOf(State, "console_metrics") != 0) @compileError("Native boot-display snapshot C layout");
}
