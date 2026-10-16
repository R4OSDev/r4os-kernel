// Native boot/completion owner; fixed private C records only.
const crash = @import("crash.zig");

pub const MAX_PHASES: usize = 32;

pub const CompletionState = enum(u32) {
    uninitialized = 0,
    running = 1,
    ready = 2,
    fallback_ready = 3,
    failed = 4,
};

pub const CompletionReason = enum(u32) {
    none = 0,
    configured_shell_ready = 1,
    terminal_fallback_ready = 2,
    recovery_fallback_ready = 3,
    no_shell = 4,
    fatal_error = 5,
    shell_exited_before_ready = 6,
};

pub const ShellKind = enum(u32) {
    none = 0,
    configured = 1,
    terminal_fallback = 2,
    recovery_fallback = 3,
};

pub const ready_result_completed: i32 = 0;
pub const ready_result_already_completed: i32 = 1;
pub const ready_error_not_expected_shell: i32 = -1;
pub const ready_error_boot_failed: i32 = -2;

pub const PhaseInfo = extern struct {
    phase: crash.BootPhase = .unknown,
    first_tick: u64 = 0,
    last_tick: u64 = 0,
    total_ticks: u64 = 0,
    transitions: u32 = 0,
    first_ns: u64 = 0,
    last_ns: u64 = 0,
    total_ns: u64 = 0,
    timing_valid: bool = false,
    timing_unavailable_spans: u32 = 0,
};

pub const Summary = extern struct {
    initialized: bool = false,
    state: CompletionState = .uninitialized,
    completion_reason: CompletionReason = .none,
    boot_start_tick: u64 = 0,
    now_tick: u64 = 0,
    total_ticks: u64 = 0,
    phase_count: u32 = 0,
    transition_count: u64 = 0,
    current_phase: crash.BootPhase = .unknown,
    now_ns: u64 = 0,
    total_ns: u64 = 0,
    clock_flags: u32 = 0,
    clock_source: u32 = 0,
    clock_generation: u32 = 0,
    clock_resolution_ns: u64 = 0,
    timing_valid: bool = false,
    timing_span_count: u32 = 0,
    timing_unavailable_spans: u32 = 0,
    timing_dropped_spans: u32 = 0,
    configured_attempts: u32 = 0,
    fallback_attempts: u32 = 0,
    launch_failures: u32 = 0,
    shell_instance_id: u32 = 0,
};

extern fn r4os_rust_boot_performance_control(u8,u32) i32;
extern fn r4os_rust_boot_performance_snapshot(*Summary) void;
extern fn r4os_rust_boot_performance_phase(u32,*PhaseInfo) bool;
pub fn init() void {_ = r4os_rust_boot_performance_control(0,0);}
pub fn record(phase:crash.BootPhase) void {_ = r4os_rust_boot_performance_control(1,@intFromEnum(phase));}
pub fn beginShellAttempt(kind:ShellKind) void {_ = r4os_rust_boot_performance_control(2,@intFromEnum(kind));}
pub fn noteShellLaunched(id:u32) void {_ = r4os_rust_boot_performance_control(3,id);}
pub fn noteShellLaunchFailure() void {_ = r4os_rust_boot_performance_control(4,0);}
pub fn completeReady(id:u32) i32 {return r4os_rust_boot_performance_control(5,id);}
pub fn failNoShell() void {_ = r4os_rust_boot_performance_control(6,0);}
pub fn failFatal() void {_ = r4os_rust_boot_performance_control(7,0);}
pub fn failShellExited(id:u32) void {_ = r4os_rust_boot_performance_control(8,id);}
pub fn snapshot() Summary {var out:Summary=undefined;r4os_rust_boot_performance_snapshot(&out);return out;}
pub fn phaseAt(index:u32) ?PhaseInfo {var out:PhaseInfo=undefined;return if(r4os_rust_boot_performance_phase(index,&out))out else null;}

comptime {
    if(@sizeOf(PhaseInfo)!=72 or @alignOf(PhaseInfo)!=8)@compileError("Native PhaseInfo C geometry");
    if(@offsetOf(PhaseInfo,"phase")!=0)@compileError("Native PhaseInfo.phase C offset");
    if(@offsetOf(PhaseInfo,"first_tick")!=8)@compileError("Native PhaseInfo.first_tick C offset");
    if(@offsetOf(PhaseInfo,"last_tick")!=16)@compileError("Native PhaseInfo.last_tick C offset");
    if(@offsetOf(PhaseInfo,"total_ticks")!=24)@compileError("Native PhaseInfo.total_ticks C offset");
    if(@offsetOf(PhaseInfo,"transitions")!=32)@compileError("Native PhaseInfo.transitions C offset");
    if(@offsetOf(PhaseInfo,"first_ns")!=40)@compileError("Native PhaseInfo.first_ns C offset");
    if(@offsetOf(PhaseInfo,"last_ns")!=48)@compileError("Native PhaseInfo.last_ns C offset");
    if(@offsetOf(PhaseInfo,"total_ns")!=56)@compileError("Native PhaseInfo.total_ns C offset");
    if(@offsetOf(PhaseInfo,"timing_valid")!=64)@compileError("Native PhaseInfo.timing_valid C offset");
    if(@offsetOf(PhaseInfo,"timing_unavailable_spans")!=68)@compileError("Native PhaseInfo.timing_unavailable_spans C offset");
    if(@sizeOf(Summary)!=136 or @alignOf(Summary)!=8)@compileError("Native Summary C geometry");
    if(@offsetOf(Summary,"initialized")!=0)@compileError("Native Summary.initialized C offset");
    if(@offsetOf(Summary,"state")!=4)@compileError("Native Summary.state C offset");
    if(@offsetOf(Summary,"completion_reason")!=8)@compileError("Native Summary.completion_reason C offset");
    if(@offsetOf(Summary,"boot_start_tick")!=16)@compileError("Native Summary.boot_start_tick C offset");
    if(@offsetOf(Summary,"now_tick")!=24)@compileError("Native Summary.now_tick C offset");
    if(@offsetOf(Summary,"total_ticks")!=32)@compileError("Native Summary.total_ticks C offset");
    if(@offsetOf(Summary,"phase_count")!=40)@compileError("Native Summary.phase_count C offset");
    if(@offsetOf(Summary,"transition_count")!=48)@compileError("Native Summary.transition_count C offset");
    if(@offsetOf(Summary,"current_phase")!=56)@compileError("Native Summary.current_phase C offset");
    if(@offsetOf(Summary,"now_ns")!=64)@compileError("Native Summary.now_ns C offset");
    if(@offsetOf(Summary,"total_ns")!=72)@compileError("Native Summary.total_ns C offset");
    if(@offsetOf(Summary,"clock_flags")!=80)@compileError("Native Summary.clock_flags C offset");
    if(@offsetOf(Summary,"clock_source")!=84)@compileError("Native Summary.clock_source C offset");
    if(@offsetOf(Summary,"clock_generation")!=88)@compileError("Native Summary.clock_generation C offset");
    if(@offsetOf(Summary,"clock_resolution_ns")!=96)@compileError("Native Summary.clock_resolution_ns C offset");
    if(@offsetOf(Summary,"timing_valid")!=104)@compileError("Native Summary.timing_valid C offset");
    if(@offsetOf(Summary,"timing_span_count")!=108)@compileError("Native Summary.timing_span_count C offset");
    if(@offsetOf(Summary,"timing_unavailable_spans")!=112)@compileError("Native Summary.timing_unavailable_spans C offset");
    if(@offsetOf(Summary,"timing_dropped_spans")!=116)@compileError("Native Summary.timing_dropped_spans C offset");
    if(@offsetOf(Summary,"configured_attempts")!=120)@compileError("Native Summary.configured_attempts C offset");
    if(@offsetOf(Summary,"fallback_attempts")!=124)@compileError("Native Summary.fallback_attempts C offset");
    if(@offsetOf(Summary,"launch_failures")!=128)@compileError("Native Summary.launch_failures C offset");
    if(@offsetOf(Summary,"shell_instance_id")!=132)@compileError("Native Summary.shell_instance_id C offset");
}
