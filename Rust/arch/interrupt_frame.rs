//! The original resident assembly frame shared by exceptions, IRQs and IPIs.
//! CPU/stack/FPU ownership stays with the interrupted task until completion.
#[repr(C)]
pub struct InterruptFrame {
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
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
}
const _: () = {
    assert!(core::mem::size_of::<InterruptFrame>() == 160);
    assert!(core::mem::offset_of!(InterruptFrame, vector) == 120);
    assert!(core::mem::offset_of!(InterruptFrame, error_code) == 128);
    assert!(core::mem::offset_of!(InterruptFrame, rip) == 136);
    assert!(core::mem::offset_of!(InterruptFrame, rflags) == 152);
};
