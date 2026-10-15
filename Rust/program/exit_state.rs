//! Single foreground/shell and last-exit state. Legacy domain peers borrow
//! the resident C record under the same registry/boot/BSP admission; no second
//! mutable copy and no Rust reference spans a wait or peer call.
use core::cell::UnsafeCell;
#[repr(C)]
pub struct State {
    pub foreground_generation: u64,
    pub shell_generation: u64,
    pub foreground_id: u32,
    pub shell_id: u32,
    pub last_exit_code: i32,
    pub foreground_present: bool,
    pub shell_present: bool,
    pub last_display_used: bool,
    padding: [u8; 1],
}
struct Cell(UnsafeCell<State>);
// Access retains the original registry/boot/BSP ownership of these globals.
unsafe impl Sync for Cell {}
static STATE: Cell = Cell(UnsafeCell::new(State {
    foreground_generation: 0,
    shell_generation: 0,
    foreground_id: 0,
    shell_id: 0,
    last_exit_code: 0,
    foreground_present: false,
    shell_present: false,
    last_display_used: false,
    padding: [0; 1],
}));
pub(super) fn state() -> *mut State {
    STATE.0.get()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_exit_state() -> *mut State {
    state()
}
const _: () = {
    assert!(core::mem::size_of::<State>() == 32);
    assert!(core::mem::offset_of!(State, last_exit_code) == 24);
    assert!(core::mem::offset_of!(State, last_display_used) == 30);
};
