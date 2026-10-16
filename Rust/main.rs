#![no_std]
#![no_main]

// Assemble the unchanged architecture entry points with pinned Rust LLVM.
core::arch::global_asm!(include_str!("../arch/x86_64/cpu.S"), options(att_syntax));
core::arch::global_asm!(include_str!("../arch/x86_64/isr.S"), options(att_syntax));
core::arch::global_asm!(
    include_str!("../arch/x86_64/ap_trampoline.S"),
    options(att_syntax)
);

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    r4os_kernel::boot::entry::start()
}
