//! Hardware tables have one resident Rust owner. The fixed per-CPU capacity,
//! selectors, IST assignments and assembly frame layout match the Zig baseline.
use core::ffi::c_void;
use core::mem::{offset_of, size_of};
use core::ptr::{addr_of, addr_of_mut};

pub use crate::arch::smp_policy::MAX_CPUS;
const STACK_SIZE: usize = 16 * 1024;
#[repr(C, packed)]
struct TablePointer {
    limit: u16,
    base: u64,
}
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TssWire {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist1: u64,
    ist2: u64,
    ist3: u64,
    ist4: u64,
    ist5: u64,
    ist6: u64,
    ist7: u64,
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}
impl TssWire {
    const ZERO: Self = Self {
        reserved0: 0,
        rsp0: 0,
        rsp1: 0,
        rsp2: 0,
        reserved1: 0,
        ist1: 0,
        ist2: 0,
        ist3: 0,
        ist4: 0,
        ist5: 0,
        ist6: 0,
        ist7: 0,
        reserved2: 0,
        reserved3: 0,
        iomap_base: 112,
    };
}
// Preserve the former packed-Zig object's 112-byte stride/limit and I/O-map
// base. Hardware uses the first 104 bytes; no language-specific packed type
// is passed to another language.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct TssSlot {
    wire: TssWire,
    tail: [u8; 8],
}
#[repr(C, align(16))]
struct Stacks([[u8; STACK_SIZE]; MAX_CPUS]);
static mut TSS: [TssSlot; MAX_CPUS] = [TssSlot {
    wire: TssWire::ZERO,
    tail: [0; 8],
}; MAX_CPUS];
static mut DOUBLE_FAULT_STACKS: Stacks = Stacks([[0; STACK_SIZE]; MAX_CPUS]);
static mut FAULT_STACKS: Stacks = Stacks([[0; STACK_SIZE]; MAX_CPUS]);
// #MC must survive a nested protected-read #GP on IST2 and must never share
// the NMI/double-fault stack or normal task/runtime stack.
static mut MACHINE_CHECK_STACKS: Stacks = Stacks([[0; STACK_SIZE]; MAX_CPUS]);
const INITIAL_GDT: [u64; 5] = [0, 0x00AF9A000000FFFF, 0x00CF92000000FFFF, 0, 0];
static mut GDT: [[u64; 5]; MAX_CPUS] = [INITIAL_GDT; MAX_CPUS];
#[repr(C)]
#[derive(Clone, Copy)]
struct IdtEntry {
    low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    mid: u16,
    high: u32,
    zero: u32,
}
impl IdtEntry {
    const ZERO: Self = Self {
        low: 0,
        selector: 0,
        ist: 0,
        attributes: 0,
        mid: 0,
        high: 0,
        zero: 0,
    };
}
#[repr(C, align(16))]
struct Idt([IdtEntry; 256]);
static mut IDT: Idt = Idt([IdtEntry::ZERO; 256]);

pub use super::interrupt_frame::InterruptFrame;
const _: () = {
    assert!(size_of::<TablePointer>() == 10 && offset_of!(TablePointer, base) == 2);
    assert!(
        size_of::<TssWire>() == 104
            && offset_of!(TssWire, rsp0) == 4
            && offset_of!(TssWire, ist1) == 36
    );
    assert!(size_of::<TssSlot>() == 112 && offset_of!(TssWire, iomap_base) == 102);
    assert!(
        size_of::<IdtEntry>() == 16
            && offset_of!(IdtEntry, ist) == 4
            && offset_of!(IdtEntry, high) == 8
    );
    assert!(size_of::<InterruptFrame>() == 160 && offset_of!(InterruptFrame, vector) == 120);
    assert!(
        offset_of!(InterruptFrame, error_code) == 128 && offset_of!(InterruptFrame, rip) == 136
    );
    assert!(offset_of!(InterruptFrame, rflags) == 152);
};
unsafe extern "C" {
    fn r4os_load_gdt(pointer: *const TablePointer);
    fn r4os_load_tss(selector: u16);
    fn r4os_load_idt(pointer: *const TablePointer);

    fn isr0();
    fn isr1();
    fn isr2();
    fn isr3();
    fn isr4();
    fn isr5();
    fn isr6();
    fn isr7();
    fn isr8();
    fn isr9();
    fn isr10();
    fn isr11();
    fn isr12();
    fn isr13();
    fn isr14();
    fn isr15();
    fn isr16();
    fn isr17();
    fn isr18();
    fn isr19();
    fn isr20();
    fn isr21();
    fn isr22();
    fn isr23();
    fn isr24();
    fn isr25();
    fn isr26();
    fn isr27();
    fn isr28();
    fn isr29();
    fn isr30();
    fn isr31();
    fn irq0();
    fn irq1();
    fn irq2();
    fn irq3();
    fn irq4();
    fn irq5();
    fn irq6();
    fn irq7();
    fn irq8();
    fn irq9();
    fn irq10();
    fn irq11();
    fn irq12();
    fn irq13();
    fn irq14();
    fn irq15();
    fn irq16();
    fn irq17();
    fn irq18();
    fn irq19();
    fn irq20();
    fn irq21();
    fn irq22();
    fn irq23();
    fn irq24();
    fn irq25();
    fn irq26();
    fn irq27();
    fn irq28();
    fn irq29();
    fn irq30();
    fn irq31();
    fn ipi_reschedule();
    fn ipi_stop();
    fn ipi_tlb();
    fn lapic_spurious();
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gdt_init(index: u32) {
    if index as usize >= MAX_CPUS {
        return;
    }
    // SAFETY: each CPU initializes only its own slot while interrupts are
    // disabled, before exposing that slot to hardware. No global &mut is made.
    unsafe {
        let slot = index as usize;
        let fault_top = addr_of!(FAULT_STACKS.0)
            .cast::<u8>()
            .add((slot + 1) * STACK_SIZE) as u64;
        let double_top = addr_of!(DOUBLE_FAULT_STACKS.0)
            .cast::<u8>()
            .add((slot + 1) * STACK_SIZE) as u64;
        let tss = addr_of_mut!(TSS).cast::<TssSlot>().add(slot);
        let mut wire = TssWire::ZERO;
        wire.rsp0 = fault_top;
        wire.ist1 = double_top;
        wire.ist2 = fault_top;
        wire.ist3 = addr_of!(MACHINE_CHECK_STACKS.0)
            .cast::<u8>()
            .add((slot + 1) * STACK_SIZE) as u64;
        tss.write(TssSlot { wire, tail: [0; 8] });
        let base = tss as u64;
        let limit = size_of::<TssSlot>() as u64 - 1;
        let mut table = INITIAL_GDT;
        table[3] = (limit & 0xffff)
            | ((base & 0xffffff) << 16)
            | (0x89 << 40)
            | (((limit >> 16) & 0xf) << 48)
            | (((base >> 24) & 0xff) << 56);
        table[4] = base >> 32;
        let gdt = addr_of_mut!(GDT).cast::<[u64; 5]>().add(slot);
        gdt.write(table);
        let pointer = TablePointer {
            limit: 39,
            base: gdt as u64,
        };
        r4os_load_gdt(&pointer);
        r4os_load_tss(0x18);
    }
}
fn ist(vector: u64) -> u8 {
    match vector {
        2 | 8 => 1,
        18 => 3,
        10 | 11 | 12 | 13 | 14 | 17 | 21 | 29 | 30 => 2,
        _ => 0,
    }
}
unsafe fn set_gate(vector: usize, handler: unsafe extern "C" fn(), stack: u8) {
    let address = handler as *const c_void as u64;
    unsafe {
        addr_of_mut!(IDT.0)
            .cast::<IdtEntry>()
            .add(vector)
            .write(IdtEntry {
                low: address as u16,
                selector: 8,
                ist: stack & 7,
                attributes: 0x8e,
                mid: (address >> 16) as u16,
                high: (address >> 32) as u32,
                zero: 0,
            })
    };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_idt_init() {
    // SAFETY: the BSP fills the one resident table before interrupts or APs
    // are enabled. Later CPUs only load this immutable table.
    unsafe {
        let exceptions: [unsafe extern "C" fn(); 32] = [
            isr0, isr1, isr2, isr3, isr4, isr5, isr6, isr7, isr8, isr9, isr10, isr11, isr12, isr13,
            isr14, isr15, isr16, isr17, isr18, isr19, isr20, isr21, isr22, isr23, isr24, isr25,
            isr26, isr27, isr28, isr29, isr30, isr31,
        ];
        let irqs: [unsafe extern "C" fn(); 32] = [
            irq0, irq1, irq2, irq3, irq4, irq5, irq6, irq7, irq8, irq9, irq10, irq11, irq12, irq13,
            irq14, irq15, irq16, irq17, irq18, irq19, irq20, irq21, irq22, irq23, irq24, irq25,
            irq26, irq27, irq28, irq29, irq30, irq31,
        ];
        for (vector, handler) in exceptions.into_iter().enumerate() {
            set_gate(vector, handler, ist(vector as u64));
        }
        for (irq, handler) in irqs.into_iter().enumerate() {
            set_gate(32 + irq, handler, 0);
        }
        set_gate(0xf0, ipi_reschedule, 0);
        set_gate(0xf1, ipi_stop, 0);
        set_gate(0xf2, ipi_tlb, 0);
        set_gate(0xff, lapic_spurious, 0);
    }
    r4os_rust_idt_load();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_idt_load() {
    let pointer = TablePointer {
        limit: (size_of::<Idt>() - 1) as u16,
        base: addr_of!(IDT) as u64,
    };
    unsafe { r4os_load_idt(&pointer) };
}
/// # Safety
/// `frame` points at the complete assembly frame, including hardware RSP/SS
/// for vectors assigned an IST, and remains on its exact interrupted stack.
pub unsafe fn interrupted_rsp(frame: *const InterruptFrame) -> u64 {
    let after_flags = unsafe { frame.cast::<u8>().add(size_of::<InterruptFrame>()) };
    if unsafe { (*frame).vector } < 256 && ist(unsafe { (*frame).vector }) != 0 {
        unsafe { after_flags.cast::<u64>().read() }
    } else {
        after_flags as u64
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// The caller retains the full resident assembly frame for the whole call.
pub unsafe extern "C" fn r4os_rust_interrupted_rsp(frame: *const InterruptFrame) -> u64 {
    unsafe { interrupted_rsp(frame) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Called by isr.S with the unchanged resident IRQ frame and aligned stack.
pub unsafe extern "C" fn irqDispatch(frame: *const InterruptFrame) {
    unsafe { super::irq::dispatch(frame) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Called by isr.S with the unchanged resident IPI frame and aligned stack.
pub unsafe extern "C" fn ipiDispatch(frame: *const InterruptFrame) {
    unsafe { super::irq::dispatch_ipi(frame) }
}
#[unsafe(no_mangle)]
/// # Safety
/// The exception assembly owns and retains this complete register frame.
pub unsafe extern "C" fn exceptionDispatch(frame: *const InterruptFrame) {
    if unsafe { super::machine_check::recover(frame.cast_mut()) } {
        return;
    }
    if unsafe { (*frame).vector } == 18 {
        unsafe { super::machine_check::fatal(frame) };
    }
    if super::machine_check::terminal_active() {
        crate::boot::fatal::halt();
    }
    if unsafe { super::safe_msr::recover(frame.cast_mut()) } {
        return;
    }
    if let Some(snapshot) =
        super::fpu_fault::capture(unsafe { (*frame).vector as u32 }, unsafe { (*frame).rip })
    {
        super::fpu_fault::emit(snapshot);
    }
    let address: u64;
    if unsafe { (*frame).vector } == 14 {
        unsafe {
            core::arch::asm!("mov {}, cr2", out(reg) address, options(nomem, nostack, preserves_flags))
        };
    } else {
        address = 0;
    }
    unsafe { exception_inner(frame, address, false) }
}
#[unsafe(no_mangle)]
/// # Safety
/// `frame` includes hardware RSP/SS on the fault IST. The stack owner returns
/// exclusively reserved resident space on the exact interrupted task stack.
pub unsafe extern "C" fn preparePageFaultFrame(
    frame: *const InterruptFrame,
    address: u64,
) -> usize {
    if super::machine_check::terminal_active() {
        crate::boot::fatal::halt();
    }
    crate::program::stack_fault::begin_fault();
    let error = unsafe { (*frame).error_code };
    if error & 1 != 0 && error != 3 {
        return 0;
    }
    const BYTES: usize = size_of::<InterruptFrame>() + 16;
    let destination = unsafe { prepare_fault_stack(address, interrupted_rsp(frame), BYTES) };
    if destination != 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(frame.cast::<u8>(), destination as *mut u8, BYTES)
        };
    }
    destination
}
#[inline(never)]
unsafe fn page_fault_private(frame: *const InterruptFrame, address: u64) {
    // This separate task-stack image survives blocking page-in and SIMD-capable
    // module calls. A scheduler image or the shared exception IST cannot do so.
    let mut image = super::fpu::Image::filled(0);
    unsafe {
        fault_fpu_save(&mut image);
        exception_inner(frame, address, true);
        fault_fpu_restore(&image);
    }
}
// An image-capacity extension must explicitly requalify this private task-stack
// owner. The allocation itself uses Image; this is the independently audited
// existing stack budget, not another state-size definition.
const PRIVATE_FAULT_FP_STACK_BUDGET: usize = 4 * 1024;
const _: () = assert!(size_of::<super::fpu::Image>() <= PRIVATE_FAULT_FP_STACK_BUDGET);
#[unsafe(no_mangle)]
/// # Safety
/// isr.S has switched to the exact resident private frame when private != 0.
pub unsafe extern "C" fn pageFaultDispatch(
    frame: *const InterruptFrame,
    address: u64,
    private: u32,
) {
    if super::machine_check::terminal_active() {
        crate::boot::fatal::halt();
    }
    if private == 0 {
        unsafe { exception_inner(frame, address, false) };
    } else {
        unsafe { page_fault_private(frame, address) };
    }
}

unsafe fn fault_fpu_save(state: &mut super::fpu::Image) {
    assert!(
        unsafe { super::fpu::r4os_rust_fpu_buffer(1, state.bytes.as_mut_ptr(), state.bytes.len()) },
        "Private fault FPU save rejected"
    );
}

unsafe fn fault_fpu_restore(state: &super::fpu::Image) {
    assert!(
        unsafe {
            super::fpu::r4os_rust_fpu_buffer(2, state.bytes.as_ptr().cast_mut(), state.bytes.len())
        },
        "Private fault FPU restore rejected"
    );
}

unsafe fn prepare_fault_stack(address: u64, rsp: u64, bytes: usize) -> usize {
    let mut out = 0;
    if unsafe {
        crate::program::fault::r4os_rust_program_fault_prepare(address, rsp, bytes as u64, &mut out)
    } {
        out as usize
    } else {
        0
    }
}

unsafe fn exception_inner(frame: *const InterruptFrame, address: u64, private_stack: bool) {
    unsafe {
        use crate::{boot::crash, boot::log_route as l, display::crash_screen, sched::scheduler};
        if (*frame).vector == 14
            && crate::program::fault::r4os_rust_program_fault_handle(
                address,
                (*frame).error_code,
                private_stack,
                interrupted_rsp(frame),
            )
        {
            return;
        }
        crate::program::stack_fault::exception(
            (*frame).vector as u32,
            address,
            interrupted_rsp(frame),
            (*frame).error_code,
        );
        crate::program::stack_fault::emit();
        if (*frame).vector == 14
            && !private_stack
            && let Some(boundary) = crate::program::fault::abort_exhausted(
                address,
                (*frame).error_code,
                interrupted_rsp(frame),
                (*frame).rip,
                (*frame).rflags,
            )
        {
            let target = frame.cast_mut();
            (*target).rip = super::callback_abort::r4os_rust_callback_resume as *const () as u64;
            (*target).rdi = boundary as u64;
            (*target).rsi = crate::program::fault::STACK_EXHAUSTED_EXIT as i64 as u64;
            // Resume with IF retained and the SysV direction/control contract.
            // The existing boundary restores MXCSR/x87 and callee registers.
            (*target).rflags &= !((1 << 8) | (1 << 10) | (1 << 16) | (1 << 18));
            target
                .cast::<u8>()
                .add(size_of::<InterruptFrame>())
                .cast::<u64>()
                .write((*boundary).rsp);
            return;
        }
        l::puts(b"[SMP] exception cpu=");
        l::dec(u64::from(crate::arch::percpu::current_index()));
        let current = scheduler::current().cast::<crate::sched::task::task_types::Task>();
        if !current.is_null() {
            let t = (*current).core.get();
            l::puts(b" task=");
            l::dec(u64::from(core::ptr::addr_of!((*t).id).read()));
            l::puts(b"/");
            let name = core::ptr::addr_of!((*t).name).read();
            l::puts(core::slice::from_raw_parts(name.pointer, name.len));
            l::puts(b" state_raw=");
            l::dec(core::ptr::addr_of!((*t).state).read() as u64);
            l::puts(b" running_cpu=");
            l::dec(u64::from(core::ptr::addr_of!((*t).running_cpu).read()));
            l::puts(b" saved_rsp=0x");
            l::hex(core::ptr::addr_of!((*t).rsp).read(), 16);
            l::puts(b" stack=0x");
            l::hex(core::ptr::addr_of!((*t).stack_base).read(), 16);
            l::puts(b"..0x");
            l::hex(core::ptr::addr_of!((*t).stack_top).read(), 16);
        }
        let f = frame.read();
        let rsp = interrupted_rsp(frame);
        l::puts(b" rip=0x");
        l::hex(f.rip, 16);
        l::puts(b" interrupted_rsp=0x");
        l::hex(rsp, 16);
        l::puts(b"\r\n");
        let reentrant = crash::r4os_rust_crash_enter() != 0;
        let cpu = crash::CpuFrameSnapshot {
            registers: crash::CpuRegisters {
                r15: f.r15,
                r14: f.r14,
                r13: f.r13,
                r12: f.r12,
                r11: f.r11,
                r10: f.r10,
                r9: f.r9,
                r8: f.r8,
                rsi: f.rsi,
                rdi: f.rdi,
                rbp: f.rbp,
                rdx: f.rdx,
                rcx: f.rcx,
                rbx: f.rbx,
                rax: f.rax,
            },
            vector: f.vector,
            error_code: f.error_code,
            rip: f.rip,
            rsp,
            cs: f.cs,
            rflags: f.rflags,
        };
        // Fatal attribution never acquires the physical owner or scans its
        // 8192-entry catalogue. Proven stack bounds are already resident;
        // unavailable general MemoryBlock attribution must stay unavailable.
        let memory = crash::FaultMemoryInfo::default();
        let mut context = crash::ExecutionContext::default();
        crate::program::peer_lifetime::r4os_rust_program_peer_crash(
            core::ptr::null(),
            0,
            f.rip,
            true,
            &mut context,
        );
        let message: &[u8] = if reentrant {
            b"Reentrant CPU exception while handling crash"
        } else {
            b"Unhandled CPU exception"
        };
        let input = crash::BuildInput {
            cause: crash::r4os_rust_crash_vector_cause(f.vector),
            phase: 0,
            ticks: crate::platform::timer::tick_count(),
            cpu: &cpu,
            cr2: address,
            memory: &memory,
            context: &context,
            message: message.as_ptr(),
            message_len: message.len(),
        };
        let report = crash::build(&input);
        if reentrant {
            crash_screen::r4os_rust_crash_screen_serial(&report);
        } else {
            crash_screen::r4os_rust_crash_screen_render(&report);
        }
        loop {
            core::arch::asm!("cli", "hlt", options(nomem, nostack));
        }
    }
}

/// Resident exception stacks have no guard reservation. Only an interrupted
/// RSP in the exact current CPU's static arrays establishes these bounds.
pub(crate) fn exception_stack_bounds(rsp: u64) -> Option<(u64, u64)> {
    let cpu = crate::arch::percpu::current_index() as usize;
    let normal = unsafe {
        core::ptr::addr_of!(FAULT_STACKS.0)
            .cast::<[u8; STACK_SIZE]>()
            .add(cpu)
    } as u64;
    let double = unsafe {
        core::ptr::addr_of!(DOUBLE_FAULT_STACKS.0)
            .cast::<[u8; STACK_SIZE]>()
            .add(cpu)
    } as u64;
    let machine = unsafe {
        core::ptr::addr_of!(MACHINE_CHECK_STACKS.0)
            .cast::<[u8; STACK_SIZE]>()
            .add(cpu)
    } as u64;
    for base in [normal, double, machine] {
        let top = base + STACK_SIZE as u64;
        if rsp > base && rsp <= top {
            return Some((base, top));
        }
    }
    None
}
