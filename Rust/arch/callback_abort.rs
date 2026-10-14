//! Original SysV callback/abort boundary, with no Rust frame or Drop between
//! assembly and the callback. Admission and exact live frame ownership belong
//! to the existing DriverThread owner; no cleanup is implied by a transfer.
pub type Handler = unsafe extern "sysv64" fn(usize) -> i32;
#[repr(C)]
#[derive(Default)]
pub struct Frame {
    pub rsp: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub mxcsr: u32,
    pub x87_control: u16,
    pub reserved: u16,
}
const _: () = {
    assert!(core::mem::size_of::<Frame>() == 64);
    assert!(core::mem::offset_of!(Frame, mxcsr) == 56);
    assert!(core::mem::offset_of!(Frame, x87_control) == 60);
};
core::arch::global_asm!(include_str!("callback_abort.S"), options(att_syntax));
unsafe extern "sysv64" {
    /// # Safety
    /// Callback and frame remain valid through return. The exact calling task
    /// owns this frame and retains its existing lifetime/abort admission.
    pub fn r4os_rust_callback_invoke(handler: Handler, context: usize, frame: *mut Frame) -> i32;
    /// # Safety
    /// Only the same task may resume this still-live frame, after all kernel
    /// owners have returned. Skipped frames have no implicit foreign cleanup.
    pub fn r4os_rust_callback_resume(frame: *const Frame, result: i32) -> !;
}
