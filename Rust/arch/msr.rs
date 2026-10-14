//! Original MSR operations; availability and privilege remain caller-owned.
/// # Safety
/// Caller executes at CPL0 and established availability of this exact MSR.
pub unsafe fn read(id: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("rdmsr", in("ecx") id, out("eax") low, out("edx") high, options(nostack, preserves_flags));
    }
    u64::from(high) << 32 | u64::from(low)
}
/// # Safety
/// Caller owns this MSR's machine state, executes at CPL0 and supplies a valid value.
pub unsafe fn write(id: u32, value: u64) {
    unsafe {
        core::arch::asm!("wrmsr", in("ecx") id, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nostack, preserves_flags));
    }
}
/// # Safety
/// Same availability and privilege contract as read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_msr_read(id: u32) -> u64 {
    unsafe { read(id) }
}
/// # Safety
/// Same exact machine-state ownership contract as write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_msr_write(id: u32, value: u64) {
    unsafe { write(id, value) };
}
