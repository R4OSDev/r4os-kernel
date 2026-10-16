//! Exact copied output routing identity. Adapter zero is not a wildcard.
use crate::contract as a;
use core::mem::size_of;
pub fn valid(value: &a::GfxOutputTarget) -> bool {
    value.version == 1
        && value.size as usize == size_of::<a::GfxOutputTarget>()
        && value.reserved0 == 0
        && value.connector_id != 0
        && value.device_generation != 0
        && value.connection_generation != 0
        && value.display_generation != 0
        && value.head_id < a::gfx_output_max_assignments
}
pub fn same(left: &a::GfxOutputTarget, right: &a::GfxOutputTarget) -> bool {
    valid(left) && valid(right) && super::equal::same(left, right)
}
pub fn from_output(identity: a::GfxOutputId, head: u32, generation: u64) -> a::GfxOutputTarget {
    a::GfxOutputTarget {
        adapter_id: identity.adapter_id,
        connector_id: identity.connector_id,
        device_generation: identity.device_generation,
        connection_generation: identity.connection_generation,
        head_id: head,
        display_generation: generation,
        ..a::GfxOutputTarget::default()
    }
}
/// # Safety
/// Exact initialized Contract record remains readable through this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_target_valid(value: *const a::GfxOutputTarget) -> bool {
    valid(unsafe { &*value })
}
/// # Safety
/// Both initialized Contract records remain readable through this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_target_same(
    left: *const a::GfxOutputTarget,
    right: *const a::GfxOutputTarget,
) -> bool {
    same(unsafe { &*left }, unsafe { &*right })
}
/// # Safety
/// Exact initialized input and exclusive output Contract records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_target_from(
    identity: *const a::GfxOutputId,
    head: u32,
    generation: u64,
    out: *mut a::GfxOutputTarget,
) {
    unsafe { out.write(from_output(identity.read(), head, generation)) }
}
