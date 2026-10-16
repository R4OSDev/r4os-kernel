//! Exact canonical frame target and atomic IRQ size projection. Short state
//! admission retains the existing program owner; no callback runs under it.
use super::ownership;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU64, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Target {
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
}
impl Target {
    fn valid(self) -> bool {
        self.width != 0 && self.height != 0
    }
}
#[derive(Default)]
struct State {
    target: Target,
    frame_active: bool,
    frame_mode: u8,
    last_frame_mode: u8,
}
struct Cell(UnsafeCell<State>);
// SAFETY: same program-state owner serializes target/frame metadata. IRQ users
// read only PIXEL_BOUNDS, published at the canonical target's transition.
unsafe impl Sync for Cell {}
static STATE: Cell = Cell(UnsafeCell::new(State {
    target: Target {
        width: 0,
        height: 0,
        pitch: 0,
        bpp: 0,
    },
    frame_active: false,
    frame_mode: 0,
    last_frame_mode: 0,
}));
static PIXEL_BOUNDS: AtomicU64 = AtomicU64::new(0);
pub fn pixel_bounds() -> u64 {
    PIXEL_BOUNDS.load(Ordering::Acquire)
}
fn reset_locked(target: Target) {
    let state = unsafe { &mut *STATE.0.get() };
    state.target = target;
    let width = u32::try_from(target.width).expect("surface target width");
    PIXEL_BOUNDS.store(u64::from(width) | (target.height << 32), Ordering::Release);
    state.frame_active = false;
    state.frame_mode = 0;
    state.last_frame_mode = 0;
}
pub fn init_target(target: Target) {
    let _state = ownership::enter_state();
    reset_locked(target)
}
pub fn publish_target_locked(target: Target) {
    if unsafe { (*STATE.0.get()).target } == target {
        return;
    }
    reset_locked(target)
}
pub fn width() -> u32 {
    let _state = ownership::enter_state();
    u32::try_from(unsafe { (*STATE.0.get()).target.width }).expect("surface target width")
}
pub fn height() -> u32 {
    let _state = ownership::enter_state();
    u32::try_from(unsafe { (*STATE.0.get()).target.height }).expect("surface target height")
}
pub fn begin_frame() -> i32 {
    let _state = ownership::enter_state();
    let state = unsafe { &mut *STATE.0.get() };
    if !state.target.valid() {
        return 0;
    }
    state.frame_active = true;
    state.frame_mode = 1;
    1
}
pub fn begin_frame_rect(w: u32, h: u32) -> i32 {
    let _state = ownership::enter_state();
    let state = unsafe { &mut *STATE.0.get() };
    if !state.target.valid() || w == 0 || h == 0 {
        return 0;
    }
    state.frame_active = true;
    state.frame_mode = 2;
    1
}
pub fn present() -> i32 {
    let _state = ownership::enter_state();
    let state = unsafe { &mut *STATE.0.get() };
    if !state.target.valid() {
        return 0;
    }
    if state.frame_mode != 0 {
        state.last_frame_mode = state.frame_mode
    }
    state.frame_active = false;
    state.frame_mode = 0;
    1
}
/// # Safety
/// Target is initialized for operations0/1. Publish requires caller's exact
/// program-state admission; other operations acquire their original admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_surface_pipeline(
    op: u32,
    target: *const Target,
    w: u32,
    h: u32,
) -> u64 {
    match op {
        0 => {
            init_target(unsafe { target.read() });
            0
        }
        1 => {
            publish_target_locked(unsafe { target.read() });
            0
        }
        2 => u64::from(width()),
        3 => u64::from(height()),
        4 => begin_frame() as u64,
        5 => begin_frame_rect(w, h) as u64,
        6 => present() as u64,
        7 => pixel_bounds(),
        _ => panic!("surface pipeline operation"),
    }
}
const _: () = {
    assert!(core::mem::size_of::<Target>() == 32 && core::mem::offset_of!(Target, bpp) == 24);
};
