//! Bounded crash records keep the original private C layout and stop codes.
use core::sync::atomic::{AtomicU8, Ordering};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CpuRegisters {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CpuFrameSnapshot {
    pub registers: CpuRegisters,
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub rsp: u64,
    pub cs: u64,
    pub rflags: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CpuExceptionInfo {
    pub vector: u16,
    pub kind: u8,
    pub error_code: u64,
    pub rip: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub cs: u64,
    pub rflags: u64,
    pub registers: CpuRegisters,
}
impl Default for CpuExceptionInfo {
    fn default() -> Self {
        Self {
            vector: 0,
            kind: 255,
            error_code: 0,
            rip: 0,
            rsp: 0,
            rbp: 0,
            cs: 0,
            rflags: 0,
            registers: CpuRegisters::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PageFaultInfo {
    pub fault_address: u64,
    pub raw_error_code: u64,
    pub present: u8,
    pub write: u8,
    pub user: u8,
    pub reserved_bit: u8,
    pub instruction_fetch: u8,
    pub protection_key: u8,
    pub shadow_stack: u8,
    pub sgx: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FaultMemoryInfo {
    pub state: u8,
    pub block_id: u32,
    pub owner: u8,
    pub owner_id: u64,
    pub kind: u8,
    pub status: u8,
    pub phys_base: u64,
    pub phys_len: u64,
    pub virt_base: u64,
    pub virt_len: u64,
}
impl Default for FaultMemoryInfo {
    fn default() -> Self {
        Self {
            state: 0,
            block_id: 0,
            owner: 255,
            owner_id: 0,
            kind: 255,
            status: 255,
            phys_base: 0,
            phys_len: 0,
            virt_base: 0,
            virt_len: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ExecutionContext {
    pub state: u8,
    pub r4x_instance_id: u32,
    pub task_id: u32,
    pub owner_id: u64,
    pub image_base: u64,
    pub image_size: u64,
    pub instruction_offset: u64,
    pub tag: [u8; 16],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FixedText {
    pub len: u16,
    pub bytes: [u8; 128],
}
impl Default for FixedText {
    fn default() -> Self {
        Self {
            len: 0,
            bytes: [0; 128],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CrashReport {
    pub magic: u64,
    pub version: u16,
    pub flags: u16,
    pub cause: u8,
    pub boot_phase: u8,
    pub reserved0: u16,
    pub stop_code: u32,
    pub ticks: u64,
    pub cpu: CpuExceptionInfo,
    pub page_fault: PageFaultInfo,
    pub memory: FaultMemoryInfo,
    pub context: ExecutionContext,
    pub message: FixedText,
}
impl Default for CrashReport {
    fn default() -> Self {
        Self {
            magic: 0x52344f5343525348,
            version: 1,
            flags: 0,
            cause: 7,
            boot_phase: 0,
            reserved0: 0,
            stop_code: 0,
            ticks: 0,
            cpu: CpuExceptionInfo::default(),
            page_fault: PageFaultInfo::default(),
            memory: FaultMemoryInfo::default(),
            context: ExecutionContext::default(),
            message: FixedText::default(),
        }
    }
}

#[repr(C)]
pub struct BuildInput {
    pub cause: u8,
    pub phase: u8,
    pub ticks: u64,
    pub cpu: *const CpuFrameSnapshot,
    pub cr2: u64,
    pub memory: *const FaultMemoryInfo,
    pub context: *const ExecutionContext,
    pub message: *const u8,
    pub message_len: usize,
}
static DEPTH: AtomicU8 = AtomicU8::new(0);
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_crash_reset() {
    DEPTH.store(0, Ordering::Relaxed);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_crash_depth() -> u8 {
    DEPTH.load(Ordering::Relaxed)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_crash_enter() -> u8 {
    let previous = DEPTH
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_add(1))
        })
        .unwrap_or_else(|n| n);
    u8::from(previous != 0)
}
impl FixedText {
    pub fn set(&mut self, bytes: &[u8]) {
        *self = Self::default();
        let count = bytes.len().min(128);
        self.bytes[..count].copy_from_slice(&bytes[..count]);
        self.len = count as u16;
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}
pub fn stop_cause(cause: u8) -> u32 {
    match cause {
        1 => 0x10000,
        2 => 0x1000e,
        3 => 0x10008,
        4 => 0x10012,
        5 => 0x20001,
        6 => 0x20002,
        7 => 0xf0001,
        _ => 0x20001,
    }
}
pub fn stop_vector(vector: u64) -> u32 {
    0x10000 | (vector & 0xffff) as u32
}
pub fn cause_vector(vector: u64) -> u8 {
    match vector {
        8 => 3,
        14 => 2,
        18 => 4,
        _ => 1,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_crash_code(cause: u8) -> u32 {
    stop_cause(cause)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_crash_vector_code(vector: u64) -> u32 {
    stop_vector(vector)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_crash_vector_cause(vector: u64) -> u8 {
    cause_vector(vector)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_crash_vector_kind(vector: u64) -> u8 {
    exception_kind(vector)
}
pub fn exception_kind(vector: u64) -> u8 {
    match vector {
        0..=14 | 16..=21 | 28..=30 => vector as u8,
        _ => 255,
    }
}
pub fn cpu_info(frame: CpuFrameSnapshot) -> CpuExceptionInfo {
    CpuExceptionInfo {
        vector: frame.vector.min(u16::MAX as u64) as u16,
        kind: exception_kind(frame.vector),
        error_code: frame.error_code,
        rip: frame.rip,
        rsp: frame.rsp,
        rbp: frame.registers.rbp,
        cs: frame.cs,
        rflags: frame.rflags,
        registers: frame.registers,
    }
}
pub fn page_fault(code: u64, address: u64) -> PageFaultInfo {
    PageFaultInfo {
        fault_address: address,
        raw_error_code: code,
        present: (code & 1) as u8,
        write: ((code >> 1) & 1) as u8,
        user: ((code >> 2) & 1) as u8,
        reserved_bit: ((code >> 3) & 1) as u8,
        instruction_fetch: ((code >> 4) & 1) as u8,
        protection_key: ((code >> 5) & 1) as u8,
        shadow_stack: ((code >> 6) & 1) as u8,
        sgx: ((code >> 15) & 1) as u8,
    }
}
/// # Safety
/// Every non-null pointer in input retains its exact readable C-layout value
/// until return. Message bytes are resident and may be arbitrary bytes.
pub unsafe fn build(input: &BuildInput) -> CrashReport {
    let mut report = CrashReport {
        cause: input.cause,
        boot_phase: input.phase,
        ticks: input.ticks,
        stop_code: stop_cause(input.cause),
        ..CrashReport::default()
    };
    if !input.cpu.is_null() {
        let frame = unsafe { input.cpu.read() };
        report.flags |= 1;
        report.cpu = cpu_info(frame);
        report.stop_code = stop_vector(frame.vector);
        if frame.vector == 14 || input.cause == 2 {
            report.flags |= 2;
            report.page_fault = page_fault(frame.error_code, input.cr2);
            if frame.vector != 14 {
                report.stop_code = stop_cause(2);
            }
        }
    }
    if !input.memory.is_null() && unsafe { (*input.memory).state } != 0 {
        report.flags |= 4;
        report.memory = unsafe { input.memory.read() };
    }
    if !input.context.is_null() && unsafe { (*input.context).state } == 1 {
        report.flags |= 8;
        report.context = unsafe { input.context.read() };
    }
    if input.message_len != 0 {
        report.flags |= 16;
        // Do not construct a slice covering more bytes than the actual bounded
        // copy needs. This is also safe for panic messages on emergency stacks.
        report
            .message
            .set(unsafe { core::slice::from_raw_parts(input.message, input.message_len.min(128)) });
    }
    report
}
#[unsafe(no_mangle)]
/// # Safety
/// input/output are valid aligned caller-owned C values, non-overlapping.
pub unsafe extern "C" fn r4os_rust_crash_build(output: *mut CrashReport, input: *const BuildInput) {
    unsafe { output.write(build(&*input)) };
}
#[unsafe(no_mangle)]
/// # Safety
/// The caller provides non-overlapping resident C-layout frame and output.
pub unsafe extern "C" fn r4os_rust_crash_cpu(
    output: *mut CpuExceptionInfo,
    frame: *const CpuFrameSnapshot,
) {
    unsafe { output.write(cpu_info(frame.read())) };
}
#[unsafe(no_mangle)]
/// # Safety
/// output is a valid aligned writable PageFaultInfo until return.
pub unsafe extern "C" fn r4os_rust_crash_pf(output: *mut PageFaultInfo, code: u64, address: u64) {
    unsafe { output.write(page_fault(code, address)) };
}
#[unsafe(no_mangle)]
/// # Safety
/// output is exclusive and bytes retain count readable bytes until return.
pub unsafe extern "C" fn r4os_rust_crash_text(
    output: *mut FixedText,
    bytes: *const u8,
    count: usize,
) {
    let mut value = FixedText::default();
    if count != 0 {
        value.set(unsafe { core::slice::from_raw_parts(bytes, count.min(128)) });
    }
    unsafe { output.write(value) };
}

pub fn cause_name(value: u8) -> &'static [u8] {
    match value {
        1 => b"CPU Exception",
        2 => b"Page Fault",
        3 => b"Double Fault",
        4 => b"Machine Check",
        5 => b"Kernel Fatal",
        6 => b"Zig Panic",
        7 => b"Manual Crash Test",
        _ => b"Unknown",
    }
}
pub fn phase_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"unknown",
        1 => b"entry",
        2 => b"cpu",
        3 => b"timer",
        4 => b"driver",
        5 => b"input",
        6 => b"memory",
        7 => b"storage",
        8 => b"module",
        9 => b"platform",
        10 => b"loader",
        11 => b"irq",
        12 => b"service",
        13 => b"runtime",
        14 => b"audio",
        15 => b"network",
        16 => b"usb",
        17 => b"driver_policy",
        18 => b"shell",
        19 => b"task_runtime",
        _ => b"Unknown",
    }
}
pub fn exception_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"Divide Error",
        1 => b"Debug",
        2 => b"Non-maskable Interrupt",
        3 => b"Breakpoint",
        4 => b"Overflow",
        5 => b"Bound Range Exceeded",
        6 => b"Invalid Opcode",
        7 => b"Device Not Available",
        8 => b"Double Fault",
        9 => b"Coprocessor Segment Overrun",
        10 => b"Invalid TSS",
        11 => b"Segment Not Present",
        12 => b"Stack-Segment Fault",
        13 => b"General Protection Fault",
        14 => b"Page Fault",
        16 => b"x87 Floating-Point Exception",
        17 => b"Alignment Check",
        18 => b"Machine Check",
        19 => b"SIMD Floating-Point Exception",
        20 => b"Virtualization Exception",
        21 => b"Control Protection Exception",
        28 => b"Hypervisor Injection Exception",
        29 => b"VMM Communication Exception",
        30 => b"Security Exception",
        255 => b"Unknown",
        _ => b"Unknown",
    }
}
pub fn memory_owner_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"kernel",
        1 => b"driver",
        2 => b"protocol",
        3 => b"r4x_instance",
        4 => b"task",
        5 => b"device",
        6 => b"bootloader",
        7 => b"system",
        254 => b"unknown",
        255 => b"unavailable",
        _ => b"Unknown",
    }
}
pub fn memory_kind_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"boot",
        1 => b"kernel",
        2 => b"kernel_heap",
        3 => b"page_table",
        4 => b"virtual_range",
        5 => b"program_image",
        6 => b"app_heap",
        7 => b"app_stack",
        8 => b"dma",
        9 => b"mmio",
        10 => b"framebuffer",
        11 => b"reserved",
        12 => b"free",
        13 => b"unknown",
        255 => b"unavailable",
        _ => b"Unknown",
    }
}
pub fn memory_status_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"free",
        1 => b"reserved",
        2 => b"committed",
        3 => b"guard",
        4 => b"mapped",
        5 => b"released",
        6 => b"error",
        254 => b"unknown",
        255 => b"unavailable",
        _ => b"Unknown",
    }
}

#[repr(C)]
pub struct TextView {
    data: *const u8,
    len: usize,
}
#[unsafe(no_mangle)]
/// # Safety
/// output is an exclusive writable C TextView; returned bytes are static.
pub unsafe extern "C" fn r4os_rust_crash_name(output: *mut TextView, group: u32, value: u8) {
    let text = match group {
        0 => cause_name(value),
        1 => phase_name(value),
        2 => exception_name(value),
        3 => memory_owner_name(value),
        4 => memory_kind_name(value),
        5 => memory_status_name(value),
        _ => b"Unknown",
    };
    unsafe {
        output.write(TextView {
            data: text.as_ptr(),
            len: text.len(),
        })
    };
}
const _: () = {
    assert!(core::mem::size_of::<CrashReport>() == 496);
    assert!(
        core::mem::offset_of!(CrashReport, cpu) == 32
            && core::mem::offset_of!(CrashReport, message) == 360
    );
    assert!(
        core::mem::size_of::<BuildInput>() == 64 && core::mem::offset_of!(BuildInput, cpu) == 16
    );
};
