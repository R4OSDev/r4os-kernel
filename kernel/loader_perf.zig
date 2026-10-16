// Native loader observations; fixed private C record/call facade.
pub const service_boot_not_attempted: u32 = 0;
pub const service_boot_ran: u32 = 1;
pub const service_boot_not_found: u32 = 2;
pub const service_boot_failed: u32 = 3;

const audited_boot_critical_count: u32 = 8;
const audited_lazy_candidate_count: u32 = 9;

pub const Summary = extern struct {
    initialized: u32 = 0,
    loader_started: u32 = 0,
    loader_completed: u32 = 0,
    r4p_runtime_started: u32 = 0,
    r4p_runtime_completed: u32 = 0,
    service_boot_status: u32 = service_boot_not_attempted,
    boot_critical_count: u32 = audited_boot_critical_count,
    lazy_candidate_count: u32 = audited_lazy_candidate_count,

    loader_total_ticks: u64 = 0,
    r4p_runtime_total_ticks: u64 = 0,
    service_boot_ticks: u64 = 0,

    config_load_ticks: u64 = 0,
    config_bytes: u64 = 0,
    config_driver_count: u32 = 0,
    config_disabled_count: u32 = 0,
    config_option_count: u32 = 0,
    config_reserved0: u32 = 0,

    r4l_scan_entries: u64 = 0,
    r4l_candidates: u64 = 0,
    r4l_loaded: u64 = 0,
    r4l_failed: u64 = 0,
    r4l_scan_ticks: u64 = 0,
    r4l_read_ticks: u64 = 0,
    r4l_resolve_ticks: u64 = 0,
    r4l_load_ticks: u64 = 0,

    r4d_scan_entries: u64 = 0,
    r4d_candidates: u64 = 0,
    r4d_discovered: u64 = 0,
    r4d_failed: u64 = 0,
    r4d_scan_ticks: u64 = 0,
    r4d_read_ticks: u64 = 0,
    r4d_probe_ticks: u64 = 0,

    r4p_scan_entries: u64 = 0,
    r4p_candidates: u64 = 0,
    r4p_discovered: u64 = 0,
    r4p_active: u64 = 0,
    r4p_blocked: u64 = 0,
    r4p_failed: u64 = 0,
    r4p_scan_ticks: u64 = 0,
    r4p_read_ticks: u64 = 0,
    r4p_resolve_ticks: u64 = 0,
    r4p_init_ticks: u64 = 0,

    clock_flags: u32 = 0,
    clock_source: u32 = 0,
    clock_generation: u32 = 0,
    timing_valid_spans: u32 = 0,
    timing_unavailable_spans: u32 = 0,
    timing_reserved0: u32 = 0,
    clock_resolution_ns: u64 = 0,
    loader_total_ns: u64 = 0,
    r4p_runtime_total_ns: u64 = 0,
    service_boot_ns: u64 = 0,
    config_load_ns: u64 = 0,
};

extern fn r4os_rust_loader_performance_snapshot(*Summary) void;
extern fn r4os_rust_loader_performance_control(u8,u64,u64,u64,u64,u64) u64;
pub fn init() void {_ = r4os_rust_loader_performance_control(0,0,0,0,0,0);}
pub fn snapshot() Summary {var out:Summary=undefined;r4os_rust_loader_performance_snapshot(&out);return out;}
pub fn beginFilesystemLoader() void {_ = r4os_rust_loader_performance_control(1,0,0,0,0,0);}
pub fn finishFilesystemLoader() void {_ = r4os_rust_loader_performance_control(2,0,0,0,0,0);}
pub fn beginR4pRuntime() u64 {return r4os_rust_loader_performance_control(3,0,0,0,0,0);}
pub fn finishR4pRuntime(start:u64) void {_ = r4os_rust_loader_performance_control(4,start,0,0,0,0);}
pub fn beginServiceBoot() u64 {return r4os_rust_loader_performance_control(5,0,0,0,0,0);}
pub fn finishServiceBoot(start:u64,status:u32) void {_ = r4os_rust_loader_performance_control(6,start,status,0,0,0);}
pub fn beginConfigLoad() u64 {return r4os_rust_loader_performance_control(7,0,0,0,0,0);}
pub fn recordConfigLoad(start:u64,bytes:usize,drivers:usize,disabled:usize,options:usize) void {_ = r4os_rust_loader_performance_control(8,start,bytes,drivers,disabled,options);}
pub fn recordR4lScanEntry() void {_ = r4os_rust_loader_performance_control(9,0,0,0,0,0);}
pub fn recordR4lCandidate() void {_ = r4os_rust_loader_performance_control(10,0,0,0,0,0);}
pub fn addR4lScanTicks(start:u64) void {_ = r4os_rust_loader_performance_control(11,start,0,0,0,0);}
pub fn addR4lReadTicks(start:u64) void {_ = r4os_rust_loader_performance_control(12,start,0,0,0,0);}
pub fn addR4lResolveTicks(start:u64) void {_ = r4os_rust_loader_performance_control(13,start,0,0,0,0);}
pub fn addR4lLoadTicks(start:u64) void {_ = r4os_rust_loader_performance_control(14,start,0,0,0,0);}
pub fn recordR4dScanEntry() void {_ = r4os_rust_loader_performance_control(15,0,0,0,0,0);}
pub fn recordR4dCandidate() void {_ = r4os_rust_loader_performance_control(16,0,0,0,0,0);}
pub fn addR4dScanTicks(start:u64) void {_ = r4os_rust_loader_performance_control(17,start,0,0,0,0);}
pub fn addR4dReadTicks(start:u64) void {_ = r4os_rust_loader_performance_control(18,start,0,0,0,0);}
pub fn addR4dProbeTicks(start:u64) void {_ = r4os_rust_loader_performance_control(19,start,0,0,0,0);}
pub fn recordR4pScanEntry() void {_ = r4os_rust_loader_performance_control(20,0,0,0,0,0);}
pub fn recordR4pCandidate() void {_ = r4os_rust_loader_performance_control(21,0,0,0,0,0);}
pub fn addR4pScanTicks(start:u64) void {_ = r4os_rust_loader_performance_control(22,start,0,0,0,0);}
pub fn addR4pReadTicks(start:u64) void {_ = r4os_rust_loader_performance_control(23,start,0,0,0,0);}
pub fn addR4pResolveTicks(start:u64) void {_ = r4os_rust_loader_performance_control(24,start,0,0,0,0);}
pub fn addR4pInitTicks(start:u64) void {_ = r4os_rust_loader_performance_control(25,start,0,0,0,0);}
pub fn recordR4lResults(loaded:usize,candidates:usize) void {_ = r4os_rust_loader_performance_control(26,loaded,candidates,0,0,0);}
pub fn recordR4dResults(discovered:usize) void {_ = r4os_rust_loader_performance_control(27,discovered,0,0,0,0);}
pub fn recordR4pResults(discovered:usize,active:usize,blocked:usize,failed:usize) void {_ = r4os_rust_loader_performance_control(28,discovered,active,blocked,failed,0);}
pub fn now() u64 {return r4os_rust_loader_performance_control(29,0,0,0,0,0);}
pub fn elapsedSince(start:u64) u64 {return r4os_rust_loader_performance_control(30,start,0,0,0,0);}

comptime {
    if(@sizeOf(Summary)!=352 or @alignOf(Summary)!=8)@compileError("Native Summary C geometry");
    if(@offsetOf(Summary,"initialized")!=0)@compileError("Native Summary.initialized C offset");
    if(@offsetOf(Summary,"loader_started")!=4)@compileError("Native Summary.loader_started C offset");
    if(@offsetOf(Summary,"loader_completed")!=8)@compileError("Native Summary.loader_completed C offset");
    if(@offsetOf(Summary,"r4p_runtime_started")!=12)@compileError("Native Summary.r4p_runtime_started C offset");
    if(@offsetOf(Summary,"r4p_runtime_completed")!=16)@compileError("Native Summary.r4p_runtime_completed C offset");
    if(@offsetOf(Summary,"service_boot_status")!=20)@compileError("Native Summary.service_boot_status C offset");
    if(@offsetOf(Summary,"boot_critical_count")!=24)@compileError("Native Summary.boot_critical_count C offset");
    if(@offsetOf(Summary,"lazy_candidate_count")!=28)@compileError("Native Summary.lazy_candidate_count C offset");
    if(@offsetOf(Summary,"loader_total_ticks")!=32)@compileError("Native Summary.loader_total_ticks C offset");
    if(@offsetOf(Summary,"r4p_runtime_total_ticks")!=40)@compileError("Native Summary.r4p_runtime_total_ticks C offset");
    if(@offsetOf(Summary,"service_boot_ticks")!=48)@compileError("Native Summary.service_boot_ticks C offset");
    if(@offsetOf(Summary,"config_load_ticks")!=56)@compileError("Native Summary.config_load_ticks C offset");
    if(@offsetOf(Summary,"config_bytes")!=64)@compileError("Native Summary.config_bytes C offset");
    if(@offsetOf(Summary,"config_driver_count")!=72)@compileError("Native Summary.config_driver_count C offset");
    if(@offsetOf(Summary,"config_disabled_count")!=76)@compileError("Native Summary.config_disabled_count C offset");
    if(@offsetOf(Summary,"config_option_count")!=80)@compileError("Native Summary.config_option_count C offset");
    if(@offsetOf(Summary,"config_reserved0")!=84)@compileError("Native Summary.config_reserved0 C offset");
    if(@offsetOf(Summary,"r4l_scan_entries")!=88)@compileError("Native Summary.r4l_scan_entries C offset");
    if(@offsetOf(Summary,"r4l_candidates")!=96)@compileError("Native Summary.r4l_candidates C offset");
    if(@offsetOf(Summary,"r4l_loaded")!=104)@compileError("Native Summary.r4l_loaded C offset");
    if(@offsetOf(Summary,"r4l_failed")!=112)@compileError("Native Summary.r4l_failed C offset");
    if(@offsetOf(Summary,"r4l_scan_ticks")!=120)@compileError("Native Summary.r4l_scan_ticks C offset");
    if(@offsetOf(Summary,"r4l_read_ticks")!=128)@compileError("Native Summary.r4l_read_ticks C offset");
    if(@offsetOf(Summary,"r4l_resolve_ticks")!=136)@compileError("Native Summary.r4l_resolve_ticks C offset");
    if(@offsetOf(Summary,"r4l_load_ticks")!=144)@compileError("Native Summary.r4l_load_ticks C offset");
    if(@offsetOf(Summary,"r4d_scan_entries")!=152)@compileError("Native Summary.r4d_scan_entries C offset");
    if(@offsetOf(Summary,"r4d_candidates")!=160)@compileError("Native Summary.r4d_candidates C offset");
    if(@offsetOf(Summary,"r4d_discovered")!=168)@compileError("Native Summary.r4d_discovered C offset");
    if(@offsetOf(Summary,"r4d_failed")!=176)@compileError("Native Summary.r4d_failed C offset");
    if(@offsetOf(Summary,"r4d_scan_ticks")!=184)@compileError("Native Summary.r4d_scan_ticks C offset");
    if(@offsetOf(Summary,"r4d_read_ticks")!=192)@compileError("Native Summary.r4d_read_ticks C offset");
    if(@offsetOf(Summary,"r4d_probe_ticks")!=200)@compileError("Native Summary.r4d_probe_ticks C offset");
    if(@offsetOf(Summary,"r4p_scan_entries")!=208)@compileError("Native Summary.r4p_scan_entries C offset");
    if(@offsetOf(Summary,"r4p_candidates")!=216)@compileError("Native Summary.r4p_candidates C offset");
    if(@offsetOf(Summary,"r4p_discovered")!=224)@compileError("Native Summary.r4p_discovered C offset");
    if(@offsetOf(Summary,"r4p_active")!=232)@compileError("Native Summary.r4p_active C offset");
    if(@offsetOf(Summary,"r4p_blocked")!=240)@compileError("Native Summary.r4p_blocked C offset");
    if(@offsetOf(Summary,"r4p_failed")!=248)@compileError("Native Summary.r4p_failed C offset");
    if(@offsetOf(Summary,"r4p_scan_ticks")!=256)@compileError("Native Summary.r4p_scan_ticks C offset");
    if(@offsetOf(Summary,"r4p_read_ticks")!=264)@compileError("Native Summary.r4p_read_ticks C offset");
    if(@offsetOf(Summary,"r4p_resolve_ticks")!=272)@compileError("Native Summary.r4p_resolve_ticks C offset");
    if(@offsetOf(Summary,"r4p_init_ticks")!=280)@compileError("Native Summary.r4p_init_ticks C offset");
    if(@offsetOf(Summary,"clock_flags")!=288)@compileError("Native Summary.clock_flags C offset");
    if(@offsetOf(Summary,"clock_source")!=292)@compileError("Native Summary.clock_source C offset");
    if(@offsetOf(Summary,"clock_generation")!=296)@compileError("Native Summary.clock_generation C offset");
    if(@offsetOf(Summary,"timing_valid_spans")!=300)@compileError("Native Summary.timing_valid_spans C offset");
    if(@offsetOf(Summary,"timing_unavailable_spans")!=304)@compileError("Native Summary.timing_unavailable_spans C offset");
    if(@offsetOf(Summary,"timing_reserved0")!=308)@compileError("Native Summary.timing_reserved0 C offset");
    if(@offsetOf(Summary,"clock_resolution_ns")!=312)@compileError("Native Summary.clock_resolution_ns C offset");
    if(@offsetOf(Summary,"loader_total_ns")!=320)@compileError("Native Summary.loader_total_ns C offset");
    if(@offsetOf(Summary,"r4p_runtime_total_ns")!=328)@compileError("Native Summary.r4p_runtime_total_ns C offset");
    if(@offsetOf(Summary,"service_boot_ns")!=336)@compileError("Native Summary.service_boot_ns C offset");
    if(@offsetOf(Summary,"config_load_ns")!=344)@compileError("Native Summary.config_load_ns C offset");
}
