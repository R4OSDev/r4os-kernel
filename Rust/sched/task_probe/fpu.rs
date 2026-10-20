//! Private integer-only guard-roundtrip probe, not hardware IRQ evidence.
//! The compiler remains no-SIMD; explicit
//! assembly performs only the same three probe operations as the old source.
use crate::{arch::runtime, sched::scheduler};
core::arch::global_asm!(include_str!("fpu.S"), options(att_syntax));
unsafe extern "C" {
    fn r4os_task_probe_ymm_load(source: *const u64);
    fn r4os_task_probe_ymm_store(destination: *mut u64);
    fn r4os_task_probe_initial_fpu(destination: *mut u64, control: *mut u32);
}
pub(super) unsafe fn load(value: &[u64; 4]) {
    unsafe { r4os_task_probe_ymm_load(value.as_ptr()) };
}
pub(super) unsafe fn read() -> [u64; 4] {
    let mut value = [0; 4];
    unsafe { r4os_task_probe_ymm_store(value.as_mut_ptr()) };
    value
}
pub(crate) unsafe extern "C" fn callback(lane: usize) -> i32 {
    unsafe {
        let expected = [
            0x13579bdf2468ace0u64,
            0x8877665544332211,
            0xa5a55a5af0f00f0f,
            0x123456789abcdef0 + lane as u64,
        ];
        let mut observed = [0; 4];
        let flags = runtime::r4os_rust_runtime_control(2, 0);
        r4os_task_probe_ymm_load(expected.as_ptr());
        let mut guard = core::mem::MaybeUninit::uninit();
        scheduler::r4os_rust_scheduler_irq_enter(guard.as_mut_ptr());
        scheduler::r4os_rust_scheduler_irq_leave(guard.as_ptr());
        r4os_task_probe_ymm_store(observed.as_mut_ptr());
        runtime::r4os_rust_runtime_control(3, flags);
        if expected != observed {
            return -1;
        }
        match crate::arch::fpu_probe::run(
            0x4600 + lane as u32,
            Some(super::fpu_environment::guard),
            0,
            64,
        ) {
            Ok(_) => 0,
            Err(reason) => {
                super::helpers::puts(b"[FPUENV46] guard reason=");
                super::helpers::puts(reason);
                super::helpers::puts(b"\r\n");
                -2
            }
        }
    }
}
pub(crate) unsafe fn initial() -> ([u64; 4], u32) {
    unsafe {
        let mut value = [0; 4];
        let mut mxcsr = 0;
        r4os_task_probe_initial_fpu(value.as_mut_ptr(), &mut mxcsr);
        (value, mxcsr)
    }
}
