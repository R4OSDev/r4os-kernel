//! Compatibility of the standard-format image shared by all runnable CPUs.
//! Disabled optional components and CPU model/core type do not affect it.
#[derive(Clone, Copy, Default)]
pub(crate) struct Layout {
    pub ymm_bytes: u32,
    pub ymm_offset: u32,
    pub ymm_flags: u32,
}
impl Layout {
    pub(crate) fn compatible(self, local: Self, image_bytes: u32, storage_bytes: u32) -> bool {
        self.ymm_bytes == 256
            && self.ymm_offset >= 576
            && self.ymm_flags & 1 == 0
            && local.ymm_bytes == self.ymm_bytes
            && local.ymm_offset == self.ymm_offset
            && local.ymm_flags & 1 == 0
            && u64::from(local.ymm_offset) + u64::from(local.ymm_bytes) <= u64::from(image_bytes)
            && image_bytes <= storage_bytes
    }
}
