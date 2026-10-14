// Private C facade: Rust owns requests, generations, ACKs and diagnostics.
pub const VECTOR: u8 = 0xF2;
pub const SendFn = *const fn (cpu_index: u32, vector: u8) callconv(.c) bool;
pub const Result = extern struct {
    ok: bool = false,
    generation: u64 = 0,
    target_mask: u64 = 0,
    acknowledgement_mask: u64 = 0,
    offline_mask: u64 = 0,
    spins: u64 = 0,
};

pub const Stats = extern struct {
    requests: u64 = 0,
    successes: u64 = 0,
    timeouts: u64 = 0,
    expected_timeouts: u64 = 0,
    ipis_sent: u64 = 0,
    acknowledgements: u64 = 0,
    offline_exclusions: u64 = 0,
    local_page_flushes: u64 = 0,
    local_full_reloads: u64 = 0,
    max_wait_spins: u64 = 0,
    generation: u64 = 0,
};

extern fn r4os_rust_tlb_register(sender: ?SendFn) void;
extern fn r4os_rust_tlb_invalidate(base: u64, pages: u64, output: *Result) void;
extern fn r4os_rust_tlb_control(operation: u8, index: u32) bool;
extern fn r4os_rust_tlb_stats(output: *Stats) void;
comptime {
    if (@sizeOf(Result) != 48 or @offsetOf(Result, "generation") != 8 or
        @sizeOf(Stats) != 88) @compileError("Native TLB C layout differs");
}
pub fn registerSender(sender: SendFn) void {
    r4os_rust_tlb_register(sender);
}
pub fn invalidateRange(base: u64, page_count: u64) Result {
    var value: Result = undefined;
    r4os_rust_tlb_invalidate(base, page_count, &value);
    return value;
}
pub fn handleIpi() void {
    _ = r4os_rust_tlb_control(0, 0);
}
pub fn armDiagnosticMissingAck(cpu_index: u32) bool {
    return r4os_rust_tlb_control(1, cpu_index);
}
pub fn disarmDiagnosticMissingAck() void {
    _ = r4os_rust_tlb_control(2, 0);
}
pub fn stats() Stats {
    var value: Stats = undefined;
    r4os_rust_tlb_stats(&value);
    return value;
}
