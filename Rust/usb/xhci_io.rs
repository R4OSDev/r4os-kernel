//! Controller-owned thin bindings to the native MMIO/DMA primitives.
//! These functions store no second state and retain only raw exact identities.
use super::xhci_state::Context;
use super::{xhci_hardware as hw, xhci_state::current};
use crate::sched::sync_types::Text;
pub(super) unsafe fn read8(ctx: Context, offset: u64) -> u8 {
    unsafe { hw::read8(current(ctx), offset) }
}
pub(super) unsafe fn read16(ctx: Context, offset: u64) -> u16 {
    unsafe { hw::read16(current(ctx), offset) }
}
pub(super) unsafe fn read32(ctx: Context, offset: u64) -> u32 {
    unsafe { hw::read32(current(ctx), offset) }
}
pub(super) unsafe fn read_op32(ctx: Context, offset: u64) -> u32 {
    unsafe { hw::read_op32(current(ctx), offset) }
}
pub(super) unsafe fn read_op64(ctx: Context, offset: u64) -> u64 {
    unsafe { hw::read_op64(current(ctx), offset) }
}
pub(super) unsafe fn read_rt32(ctx: Context, offset: u64) -> u32 {
    unsafe { hw::read_rt32(current(ctx), offset) }
}
pub(super) unsafe fn read_rt64(ctx: Context, offset: u64) -> u64 {
    unsafe { hw::read_rt64(current(ctx), offset) }
}
pub(super) unsafe fn write32(ctx: Context, offset: u64, value: u32) {
    unsafe { hw::write32(current(ctx), offset, value) }
}
pub(super) unsafe fn write_op32(ctx: Context, offset: u64, value: u32) {
    unsafe { hw::write_op32(current(ctx), offset, value) }
}
pub(super) unsafe fn write_op64(ctx: Context, offset: u64, value: u64) {
    unsafe { hw::write_op64(current(ctx), offset, value) }
}
pub(super) unsafe fn write_rt32(ctx: Context, offset: u64, value: u32) {
    unsafe { hw::write_rt32(current(ctx), offset, value) }
}
pub(super) unsafe fn write_rt64(ctx: Context, offset: u64, value: u64) {
    unsafe { hw::write_rt64(current(ctx), offset, value) }
}
pub(super) unsafe fn write_doorbell(ctx: Context, index: u8, target: u32) {
    unsafe { hw::write_doorbell(current(ctx), index, target) }
}
pub(super) unsafe fn wait_op_set(
    ctx: Context,
    offset: u64,
    mask: u32,
    ms: u32,
    reason: Text,
) -> bool {
    unsafe { hw::wait_op_set(current(ctx), offset, mask, ms, reason) }
}
pub(super) unsafe fn wait_op_clear(
    ctx: Context,
    offset: u64,
    mask: u32,
    ms: u32,
    reason: Text,
) -> bool {
    unsafe { hw::wait_op_clear(current(ctx), offset, mask, ms, reason) }
}
pub(super) unsafe fn read_operational(ctx: Context) {
    unsafe { hw::read_operational(current(ctx)) }
}
pub(super) unsafe fn read_runtime(ctx: Context) {
    unsafe { hw::read_runtime(current(ctx)) }
}
pub(super) unsafe fn read_ports(ctx: Context) {
    unsafe { hw::read_ports(current(ctx)) }
}
