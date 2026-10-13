//! Transitional access to the existing ranked owners, with a fixed C token.
//! Replacing a data owner must not create a second lock or bypass kill protection.
#[repr(C)]
#[derive(Default)]
pub struct Token {
    flags: u64,
    cpu_index: u32,
    previous_rank: u8,
    outermost: bool,
}
unsafe extern "C" {
    fn r4os_zig_boot_lock(class: u8, token: *mut Token);
    fn r4os_zig_boot_unlock(class: u8, token: *const Token);
}
pub struct Guard {
    class: u8,
    token: Token,
}
impl Guard {
    pub fn acquire(class: u8) -> Self {
        let mut result = Self {
            class,
            token: Token::default(),
        };
        unsafe { r4os_zig_boot_lock(class, &mut result.token) };
        result
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe { r4os_zig_boot_unlock(self.class, &self.token) };
    }
}
const _: () = {
    assert!(core::mem::size_of::<Token>() == 16 && core::mem::offset_of!(Token, cpu_index) == 8);
};
