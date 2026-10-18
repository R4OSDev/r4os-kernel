//! Native xHCI MMIO/DMA primitives, retaining the original ordering, mapping
//! flags and partial-map behavior. Callers own the exact controller/runtime.
//! Only raw status identities cross a wait; no Rust status borrow survives it.
use super::{
    wait_types::Wait,
    xhci_types::{PortStatus, Status},
};
use crate::{
    memory::{paging, phys},
    sched::sync_types::Text,
};
use core::ptr;
pub(super) unsafe fn volatile_read32(address: u64) -> u32 {
    unsafe { ptr::with_exposed_provenance::<u32>(address as usize).read_volatile() }
}
pub(super) unsafe fn volatile_read64(address: u64) -> u64 {
    unsafe { ptr::with_exposed_provenance::<u64>(address as usize).read_volatile() }
}
pub(super) unsafe fn read8(status: *const Status, offset: u64) -> u8 {
    let base = unsafe { (*status).mmio_virt };
    unsafe { ptr::with_exposed_provenance::<u8>((base + offset) as usize).read_volatile() }
}
pub(super) unsafe fn read16(status: *const Status, offset: u64) -> u16 {
    let base = unsafe { (*status).mmio_virt };
    unsafe { ptr::with_exposed_provenance::<u16>((base + offset) as usize).read_volatile() }
}
pub(super) unsafe fn read32(status: *const Status, offset: u64) -> u32 {
    let base = unsafe { (*status).mmio_virt };
    unsafe { volatile_read32(base + offset) }
}
pub(super) unsafe fn write32(status: *const Status, offset: u64, value: u32) {
    let base = unsafe { (*status).mmio_virt };
    unsafe {
        ptr::with_exposed_provenance_mut::<u32>((base + offset) as usize).write_volatile(value)
    }
}
pub(super) unsafe fn read_op32(status: *const Status, offset: u64) -> u32 {
    unsafe { read32(status, u64::from((*status).caplength) + offset) }
}
pub(super) unsafe fn write_op32(status: *const Status, offset: u64, value: u32) {
    unsafe { write32(status, u64::from((*status).caplength) + offset, value) }
}
pub(super) unsafe fn read_op64(status: *const Status, offset: u64) -> u64 {
    let low = unsafe { read_op32(status, offset) };
    let high = unsafe { read_op32(status, offset + 4) };
    (u64::from(high) << 32) | u64::from(low)
}
pub(super) unsafe fn write_op64(status: *const Status, offset: u64, value: u64) {
    unsafe {
        write_op32(status, offset, value as u32);
        write_op32(status, offset + 4, (value >> 32) as u32)
    };
}
pub(super) unsafe fn read_rt32(status: *const Status, offset: u64) -> u32 {
    unsafe { read32(status, u64::from((*status).rtsoff & 0xffff_ffe0) + offset) }
}
pub(super) unsafe fn write_rt32(status: *const Status, offset: u64, value: u32) {
    unsafe {
        write32(
            status,
            u64::from((*status).rtsoff & 0xffff_ffe0) + offset,
            value,
        )
    }
}
pub(super) unsafe fn read_rt64(status: *const Status, offset: u64) -> u64 {
    let low = unsafe { read_rt32(status, offset) };
    let high = unsafe { read_rt32(status, offset + 4) };
    (u64::from(high) << 32) | u64::from(low)
}
pub(super) unsafe fn write_rt64(status: *const Status, offset: u64, value: u64) {
    unsafe {
        write_rt32(status, offset, value as u32);
        write_rt32(status, offset + 4, (value >> 32) as u32)
    };
}
pub(super) unsafe fn write_doorbell(status: *const Status, index: u8, target: u32) {
    unsafe {
        write32(
            status,
            u64::from((*status).dboff & 0xffff_fffc) + u64::from(index) * 4,
            target,
        )
    }
}
pub(super) fn dma_fence() {
    unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)) };
}
pub(super) fn max_packet_for_speed(speed: u8) -> u16 {
    match speed {
        4 => 512,
        3 | 1 => 64,
        2 => 8,
        _ => 8,
    }
}
pub(super) fn trb_type(value: u32) -> u32 {
    value << 10
}
pub(super) unsafe fn read_le16(bytes: *const u8, offset: usize) -> u16 {
    unsafe { u16::from(bytes.add(offset).read()) | (u16::from(bytes.add(offset + 1).read()) << 8) }
}
pub(super) unsafe fn scratchpad_count(status: *const Status) -> u8 {
    let value = unsafe { (*status).hcsparams2 };
    let low = (value >> 27) & 31;
    let high = (value >> 21) & 31;
    ((high << 5) | low) as u8
}
pub(super) unsafe fn zero_frame(address: u64) {
    unsafe {
        ptr::write_bytes(
            ptr::with_exposed_provenance_mut::<u8>(address as usize),
            0,
            4096,
        )
    }
}
pub(super) fn alloc_frame_zero() -> Option<u64> {
    let mut frame = 0;
    if !unsafe { phys::r4os_rust_phys_alloc(&mut frame) } {
        return None;
    }
    unsafe { zero_frame(phys::r4os_rust_phys_to_virt(frame)) };
    Some(frame)
}
pub(super) fn alloc_contiguous_zero(count: u16) -> Option<u64> {
    if count == 0 {
        return None;
    }
    let mut base = 0;
    if !unsafe { phys::r4os_rust_phys_contiguous(u64::from(count), u64::MAX, true, &mut base) } {
        return None;
    }
    unsafe {
        ptr::write_bytes(
            ptr::with_exposed_provenance_mut::<u8>(phys::r4os_rust_phys_to_virt(base) as usize),
            0,
            usize::from(count) * 4096,
        )
    };
    Some(base)
}
pub(super) fn map_mmio(base: u64, bytes: u64) -> bool {
    let mut mapped = 0;
    while mapped < bytes {
        let physical = (base + mapped) & !4095;
        let virtual_page = phys::r4os_rust_phys_to_virt(physical);
        if !paging::ensure_mmio(virtual_page, physical) {
            return false;
        }
        mapped += 4096;
    }
    true
}
pub(super) unsafe fn wait_op_set(
    status: *const Status,
    offset: u64,
    mask: u32,
    timeout: u32,
    reason: Text,
) -> bool {
    let mut wait = Wait::begin(timeout, reason, 0);
    while !wait.expired() {
        if unsafe { read_op32(status, offset) } & mask == mask {
            wait.finish(true);
            return true;
        }
        wait.idle();
    }
    wait.finish(false);
    false
}
pub(super) unsafe fn wait_op_clear(
    status: *const Status,
    offset: u64,
    mask: u32,
    timeout: u32,
    reason: Text,
) -> bool {
    let mut wait = Wait::begin(timeout, reason, 0);
    while !wait.expired() {
        if unsafe { read_op32(status, offset) } & mask == 0 {
            wait.finish(true);
            return true;
        }
        wait.idle();
    }
    wait.finish(false);
    false
}
pub(super) unsafe fn read_operational(status: *mut Status) {
    unsafe {
        (*status).usbcmd = read_op32(status, 0);
        (*status).usbsts = read_op32(status, 4);
        (*status).pagesize = read_op32(status, 8);
        (*status).dnctrl = read_op32(status, 0x14);
        (*status).crcr = read_op64(status, 0x18);
        (*status).dcbaap = read_op64(status, 0x30);
        (*status).config = read_op32(status, 0x38);
    }
}
pub(super) unsafe fn read_runtime(status: *mut Status) {
    unsafe {
        if (*status).rtsoff == 0 {
            return;
        }
        (*status).mfindex = read_rt32(status, 0);
        (*status).iman0 = read_rt32(status, 0x20);
        (*status).imod0 = read_rt32(status, 0x24);
        (*status).erstsz0 = read_rt32(status, 0x28);
        (*status).erstba0 = read_rt64(status, 0x30);
        (*status).erdp0 = read_rt64(status, 0x38);
    }
}
pub(super) unsafe fn read_ports(status: *mut Status) {
    unsafe {
        let count = usize::from((*status).max_ports).min(16);
        (*status).port_count_seen = count as u8;
        (*status).connected_ports = 0;
        (*status).enabled_ports = 0;
        for i in 0..count {
            let base = 0x400 + i as u64 * 0x10;
            let old = ptr::addr_of!((*status).first_ports[i]).read();
            let portsc = read_op32(status, base);
            let connected = portsc & 1 != 0;
            let enabled = portsc & 2 != 0;
            let powered = portsc & (1 << 9) != 0;
            if connected {
                (*status).connected_ports += 1;
            }
            if enabled {
                (*status).enabled_ports += 1;
            }
            ptr::addr_of_mut!((*status).first_ports[i]).write(PortStatus {
                index: (i + 1) as u8,
                portsc,
                portpmsc: read_op32(status, base + 4),
                portli: read_op32(status, base + 8),
                porthlpmc: read_op32(status, base + 12),
                change_bits: portsc & 0x00fe_0000,
                link_state: ((portsc >> 5) & 15) as u8,
                connected,
                enabled,
                powered,
                debounce_ok: old.debounce_ok,
                reset_attempted: old.reset_attempted,
                reset_ok: old.reset_ok,
                reset_reason: old.reset_reason,
            });
        }
    }
}

pub(super) unsafe fn volatile_write32(address: u64, value: u32) {
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u32>(address as usize).write_volatile(value);
    }
}
