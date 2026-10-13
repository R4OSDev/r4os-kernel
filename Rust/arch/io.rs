//! Privileged x86 port operations. Callers own the corresponding device lock.
#[inline]
/// # Safety
/// Caller owns this device port and executes at the kernel privilege level.
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags))
    };
}
#[inline]
/// # Safety
/// Caller owns this device port and executes at the kernel privilege level.
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags))
    };
    value
}
