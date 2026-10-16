//! Private copied display values. Names are borrowed resident spans; only
//! fixed C representations cross the temporary legacy owner facade.
use super::{blit_backend, framebuffer::Framebuffer};
use core::ptr;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Text {
    pub pointer: *const u8,
    pub len: usize,
}
impl Text {
    pub const fn literal(value: &'static [u8]) -> Self {
        Self {
            pointer: value.as_ptr(),
            len: value.len(),
        }
    }
    /// # Safety
    /// The exact resident span is retained by its enclosing display lifetime.
    pub unsafe fn bytes<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(self.pointer, self.len) }
        }
    }
}
impl Default for Text {
    fn default() -> Self {
        Self::literal(b"none")
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Mapping {
    pub kind: u8,
    pub cache_policy: u8,
    pub virt_base: u64,
    pub byte_len: u64,
    pub volatile_cpu_writes: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Target {
    pub name: Text,
    pub kind: u8,
    pub flags: u32,
    pub mode: Mode,
    pub mapping: Mapping,
    pub framebuffer: *mut Framebuffer,
}
impl Default for Target {
    fn default() -> Self {
        Self {
            name: Text::default(),
            kind: 0,
            flags: 0,
            mode: Mode::default(),
            mapping: Mapping::default(),
            framebuffer: ptr::null_mut(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootSnapshot {
    pub mode: Mode,
    pub mapping: Mapping,
    pub framebuffer: Framebuffer,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub registered: bool,
    pub name: Text,
    pub kind: u8,
    pub flags: u32,
    pub mode: Mode,
    pub mapping: Mapping,
    pub present_count: u64,
    pub present_pixels_total: u64,
    pub present_bytes_total: u64,
    pub last_present_pixels: u64,
    pub last_present_bytes: u64,
    pub last_present_rect: Rect,
    pub last_present_reason: u8,
    pub last_present_converted: bool,
    pub full_present_count: u64,
    pub partial_present_count: u64,
    pub fill_present_count: u64,
    pub rect_present_count: u64,
    pub packed32_present_count: u64,
    pub xrgb32_present_count: u64,
    pub conversion_present_count: u64,
    pub present_total_ticks: u64,
    pub present_max_ticks: u64,
    pub present_last_ticks: u64,
    pub present_slow_count: u64,
}
impl Stats {
    pub fn target(target: Target) -> Self {
        Self {
            name: target.name,
            kind: target.kind,
            flags: target.flags,
            mode: target.mode,
            mapping: target.mapping,
            ..Self::default()
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PresentOutcome {
    pub success: bool,
    pub accelerated: bool,
    pub fallback: bool,
    pub source_generation: u64,
    pub present_generation: u64,
    pub fence: u64,
    pub completed_fence: u64,
    pub region_count: u32,
    pub pixel_count: u32,
    pub fallback_regions: u32,
    pub backend_error: i32,
    pub present_tick: u64,
    pub elapsed_ticks: u64,
    pub backend_name: [u8; blit_backend::NAME_BYTES],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PresentCapabilities {
    pub flags: u32,
    pub formats: u32,
    pub max_regions: u32,
    pub backend_kind: u32,
    pub backend_name: [u8; blit_backend::NAME_BYTES],
    pub fallback_name: [u8; blit_backend::NAME_BYTES],
}
impl Default for PresentCapabilities {
    fn default() -> Self {
        Self {
            flags: 0,
            formats: 1,
            max_regions: 8,
            backend_kind: 1,
            backend_name: [0; blit_backend::NAME_BYTES],
            fallback_name: [0; blit_backend::NAME_BYTES],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LegacyBackend {
    pub owner: usize,
    pub adapter_id: u32,
    pub target: Target,
    pub context: usize,
    pub commit: usize,
    pub restore: usize,
    pub begin_cpu: usize,
    pub end_cpu: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LegacyHolder {
    pub owner: usize,
    pub adapter_id: u32,
    pub expected_generation: u64,
    pub context: usize,
    pub capture: usize,
    pub restore: usize,
    pub restore_adopted: usize,
    pub release: usize,
    pub release_adopted: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BootHoldResult {
    pub generation: u64,
    pub captured: bool,
    pub retained: bool,
}
// Native callbacks are typed Rust/C function pointers. The temporary legacy
// variants invoke a typed Zig thunk with its unchanged Zig signature; no
// function is called through a cast between incompatible calling conventions.
#[derive(Clone, Copy)]
pub enum Commit {
    Native(unsafe extern "C" fn(usize, u64, *const BootSnapshot) -> u32),
    Legacy(usize),
}
#[derive(Clone, Copy)]
pub enum Restore {
    Native(unsafe extern "C" fn(usize, u64, *const BootSnapshot) -> bool),
    Legacy(usize),
}
#[derive(Clone, Copy)]
pub enum BeginCpu {
    Native(unsafe extern "C" fn(usize) -> bool),
    Legacy(usize),
}
#[derive(Clone, Copy)]
pub enum EndCpu {
    Native(unsafe extern "C" fn(usize, bool, *const Rect) -> u32),
    Legacy(usize),
}
#[derive(Clone, Copy)]
pub enum Capture {
    Native(unsafe extern "C" fn(usize, *const BootSnapshot) -> bool),
    Legacy(usize),
}
#[derive(Clone, Copy)]
pub enum Release {
    Native(unsafe extern "C" fn(usize) -> bool),
    Legacy(usize),
}
#[derive(Clone, Copy)]
pub struct Backend {
    pub owner: usize,
    pub adapter_id: u32,
    pub target: Target,
    pub context: usize,
    pub commit: Commit,
    pub restore: Restore,
    pub begin_cpu: Option<BeginCpu>,
    pub end_cpu: Option<EndCpu>,
}
#[derive(Clone, Copy)]
pub struct Holder {
    pub owner: usize,
    pub adapter_id: u32,
    pub expected_generation: u64,
    pub context: usize,
    pub capture: Capture,
    pub restore: Restore,
    pub restore_adopted: Option<Restore>,
    pub release: Release,
    pub release_adopted: Option<Release>,
}
impl From<LegacyBackend> for Backend {
    fn from(value: LegacyBackend) -> Self {
        Self {
            owner: value.owner,
            adapter_id: value.adapter_id,
            target: value.target,
            context: value.context,
            commit: Commit::Legacy(value.commit),
            restore: Restore::Legacy(value.restore),
            begin_cpu: if value.begin_cpu == 0 {
                None
            } else {
                Some(BeginCpu::Legacy(value.begin_cpu))
            },
            end_cpu: if value.end_cpu == 0 {
                None
            } else {
                Some(EndCpu::Legacy(value.end_cpu))
            },
        }
    }
}
impl From<LegacyHolder> for Holder {
    fn from(value: LegacyHolder) -> Self {
        Self {
            owner: value.owner,
            adapter_id: value.adapter_id,
            expected_generation: value.expected_generation,
            context: value.context,
            capture: Capture::Legacy(value.capture),
            restore: Restore::Legacy(value.restore),
            restore_adopted: if value.restore_adopted == 0 {
                None
            } else {
                Some(Restore::Legacy(value.restore_adopted))
            },
            release: Release::Legacy(value.release),
            release_adopted: if value.release_adopted == 0 {
                None
            } else {
                Some(Release::Legacy(value.release_adopted))
            },
        }
    }
}
unsafe extern "C" {
    fn r4os_zig_display_callback(
        op: u32,
        code: usize,
        context: usize,
        generation: u64,
        saved: *const BootSnapshot,
        changed: bool,
        damage: *const Rect,
    ) -> u32;
}
impl Commit {
    /// # Safety
    /// Exact callback, resident context and admitted snapshot/damage match their original retained lifetime.
    pub unsafe fn call(self, context: usize, generation: u64, saved: *const BootSnapshot) -> u32 {
        match self {
            Self::Native(f) => unsafe { f(context, generation, saved) },
            Self::Legacy(code) => unsafe {
                r4os_zig_display_callback(0, code, context, generation, saved, false, ptr::null())
            },
        }
    }
}
impl Restore {
    /// # Safety
    /// Exact callback, resident context and admitted snapshot/damage match their original retained lifetime.
    pub unsafe fn call(self, context: usize, generation: u64, saved: *const BootSnapshot) -> bool {
        match self {
            Self::Native(f) => unsafe { f(context, generation, saved) },
            Self::Legacy(code) => unsafe {
                r4os_zig_display_callback(1, code, context, generation, saved, false, ptr::null())
                    != 0
            },
        }
    }
}
impl BeginCpu {
    /// # Safety
    /// Exact callback, resident context and admitted snapshot/damage match their original retained lifetime.
    pub unsafe fn call(self, context: usize) -> bool {
        match self {
            Self::Native(f) => unsafe { f(context) },
            Self::Legacy(code) => unsafe {
                r4os_zig_display_callback(2, code, context, 0, ptr::null(), false, ptr::null()) != 0
            },
        }
    }
}
impl EndCpu {
    /// # Safety
    /// Exact callback, resident context and admitted snapshot/damage match their original retained lifetime.
    pub unsafe fn call(self, context: usize, changed: bool, damage: *const Rect) -> u32 {
        match self {
            Self::Native(f) => unsafe { f(context, changed, damage) },
            Self::Legacy(code) => unsafe {
                r4os_zig_display_callback(3, code, context, 0, ptr::null(), changed, damage)
            },
        }
    }
}
impl Capture {
    /// # Safety
    /// Exact callback, resident context and admitted snapshot/damage match their original retained lifetime.
    pub unsafe fn call(self, context: usize, saved: *const BootSnapshot) -> bool {
        match self {
            Self::Native(f) => unsafe { f(context, saved) },
            Self::Legacy(code) => unsafe {
                r4os_zig_display_callback(4, code, context, 0, saved, false, ptr::null()) != 0
            },
        }
    }
}
impl Release {
    /// # Safety
    /// Exact callback, resident context and admitted snapshot/damage match their original retained lifetime.
    pub unsafe fn call(self, context: usize) -> bool {
        match self {
            Self::Native(f) => unsafe { f(context) },
            Self::Legacy(code) => unsafe {
                r4os_zig_display_callback(5, code, context, 0, ptr::null(), false, ptr::null()) != 0
            },
        }
    }
}
const _: () = {
    assert!(
        size_of::<Mode>() == 24 && size_of::<Mapping>() == 32 && size_of::<BootSnapshot>() == 120
    );
};
use core::mem::size_of;

impl Mode {
    pub const EMPTY: Self = Self {
        width: 0,
        height: 0,
        pitch: 0,
        bpp: 0,
        memory_model: 0,
        red_mask_size: 0,
        red_mask_shift: 0,
        green_mask_size: 0,
        green_mask_shift: 0,
        blue_mask_size: 0,
        blue_mask_shift: 0,
    };
}

impl Rect {
    pub const EMPTY: Self = Self {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };
}

impl Mapping {
    pub const EMPTY: Self = Self {
        kind: 0,
        cache_policy: 0,
        virt_base: 0,
        byte_len: 0,
        volatile_cpu_writes: false,
    };
}

impl Stats {
    pub const EMPTY: Self = Self {
        registered: false,
        name: Text::literal(b"none"),
        kind: 0,
        flags: 0,
        mode: Mode::EMPTY,
        mapping: Mapping::EMPTY,
        present_count: 0,
        present_pixels_total: 0,
        present_bytes_total: 0,
        last_present_pixels: 0,
        last_present_bytes: 0,
        last_present_rect: Rect::EMPTY,
        last_present_reason: 0,
        last_present_converted: false,
        full_present_count: 0,
        partial_present_count: 0,
        fill_present_count: 0,
        rect_present_count: 0,
        packed32_present_count: 0,
        xrgb32_present_count: 0,
        conversion_present_count: 0,
        present_total_ticks: 0,
        present_max_ticks: 0,
        present_last_ticks: 0,
        present_slow_count: 0,
    };
}
