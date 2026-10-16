//! Private copied native-output bindings; no driver or image state is shared.
use crate::{contract as a, gfx::types::Owner};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AdditionalBinding {
    pub driver: Owner,
    pub backend: a::GfxBackendBinding,
    pub target: a::GfxOutputTarget,
    pub width: u32,
    pub height: u32,
    pub active: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ModeBinding {
    pub driver: Owner,
    pub backend: a::GfxBackendBinding,
    pub generation: u64,
    pub has_additional: bool,
    pub additional: AdditionalBinding,
}
