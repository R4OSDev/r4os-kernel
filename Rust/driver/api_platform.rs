//! Original R4D platform adapters. Canonical PCI inventory and mapping owners
//! remain in their native domains; public descriptor prefixes stay unchanged.
use crate::{
    arch::{io, runtime},
    contract as a,
    driver::{irq_router, work},
    memory::{paging, phys},
    platform::{pci_inventory as pci, pci_scan::Device, timer},
    sched::scheduler,
};
use core::ptr;
/// # Safety
/// The caller owns the selected hardware port and executes in kernel context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_port_inb(port: u16) -> u8 {
    unsafe { io::inb(port) }
}
/// # Safety
/// The caller owns the selected hardware port and executes in kernel context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_port_outb(port: u16, value: u8) {
    unsafe { io::outb(port, value) }
}
/// # Safety
/// The caller owns the selected hardware port and executes in kernel context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_port_inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        core::arch::asm!("in ax,dx",in("dx")port,out("ax")value,options(nomem,nostack,preserves_flags))
    };
    value
}
/// # Safety
/// The caller owns the selected hardware port and executes in kernel context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_port_outw(port: u16, value: u16) {
    unsafe {
        core::arch::asm!("out dx,ax",in("dx")port,in("ax")value,options(nomem,nostack,preserves_flags))
    };
}
/// # Safety
/// The caller owns the selected hardware port and executes in kernel context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_port_inl(port: u16) -> u32 {
    unsafe { io::inl(port) }
}
/// # Safety
/// The caller owns the selected hardware port and executes in kernel context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_port_outl(port: u16, value: u32) {
    unsafe { io::outl(port, value) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_wait_ticks(ticks: u64) {
    if ticks == 0 {
        return;
    }
    if irq_router::r4os_rust_irq_router_local(false) != 0 {
        work::r4os_rust_driver_work_sleep_note(true, 0);
        return;
    }
    work::r4os_rust_driver_work_sleep_note(false, ticks);
    if !scheduler::current().is_null() {
        unsafe {
            scheduler::r4os_rust_scheduler_task(
                4,
                ptr::null_mut(),
                ticks,
                0,
                b"driver-wait".as_ptr(),
                b"driver-wait".len(),
            )
        };
        return;
    }
    let start = timer::tick_count();
    runtime::r4os_rust_runtime_control(1, 0);
    while timer::tick_count() - start < ticks {
        core::hint::spin_loop()
    }
    runtime::r4os_rust_runtime_control(0, 0);
}
fn device(kind: u8, bus: u8, slot: u8, function: u8) -> Device {
    Device {
        bus_kind: kind,
        bus,
        device: slot,
        function,
        ..Device::EMPTY
    }
}
fn info(dev: Device) -> a::PciDeviceInfo {
    let route = pci::read_interrupt_route(dev);
    a::PciDeviceInfo {
        bus_kind: dev.bus_kind,
        bus: dev.bus,
        device: dev.device,
        function: dev.function,
        vendor_id: dev.vendor_id,
        device_id: dev.device_id,
        class_code: dev.class_code,
        subclass: dev.subclass,
        prog_if: dev.prog_if,
        interrupt_line: route.line,
        interrupt_pin: route.pin,
        command: pci::read_command(dev),
        ..a::PciDeviceInfo::default()
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_pci_count() -> u32 {
    u32::try_from(pci::count()).expect("Original bounded PCI count")
}
/// # Safety
/// The output is an aligned resident private C record under the original
/// driver hardware admission and stays valid through configuration readback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_pci_at(index: u32, out: *mut a::PciDeviceInfo) -> i32 {
    unsafe { ptr::write(out, a::PciDeviceInfo::default()) };
    let Some(dev) = pci::device_at(index as usize) else {
        return -1;
    };
    unsafe { ptr::write(out, info(dev)) };
    pci::note_detail_materialization();
    0
}
/// # Safety
/// The output is an aligned resident private C record under the original
/// driver hardware admission and stays valid through configuration readback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_pci_find(
    class: u8,
    subclass: u8,
    start: u32,
    out: *mut a::PciDeviceInfo,
) -> i32 {
    unsafe { ptr::write(out, a::PciDeviceInfo::default()) };
    let Some(index) = pci::find_by_class(class, subclass, start as usize) else {
        return -1;
    };
    let Some(dev) = pci::device_at(index) else {
        return -1;
    };
    unsafe { ptr::write(out, info(dev)) };
    pci::note_detail_materialization();
    i32::try_from(index).expect("Original bounded PCI index")
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_pci_read(
    kind: u8,
    bus: u8,
    slot: u8,
    function: u8,
    offset: u16,
) -> u32 {
    pci::read_config32_at(kind, bus, slot, function, offset)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_pci_write(
    kind: u8,
    bus: u8,
    slot: u8,
    function: u8,
    offset: u16,
    value: u32,
) -> i32 {
    if pci::write_config32_at(kind, bus, slot, function, offset, value) {
        0
    } else {
        -1
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_pci_bar(
    kind: u8,
    bus: u8,
    slot: u8,
    function: u8,
    index: u8,
    wide: bool,
) -> u64 {
    if index >= 6 || kind != 1 && kind != 2 {
        return 0;
    }
    let d = device(kind, bus, slot, function);
    if wide {
        pci::read_bar64(d, index)
    } else {
        u64::from(pci::read_bar(d, index))
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_pci_bus_master(
    kind: u8,
    bus: u8,
    slot: u8,
    function: u8,
    flags: u32,
) -> i32 {
    let raw = pci::read_config32_at(kind, bus, slot, function, 4);
    if raw == u32::MAX {
        return -1;
    }
    let mut command = raw as u16;
    if flags & 1 != 0 {
        command |= 1
    }
    if flags & 2 != 0 {
        command |= 2
    }
    command |= 4;
    command &= !0x400;
    if !pci::write_config32_at(kind, bus, slot, function, 4, u32::from(command)) {
        return -2;
    }
    let verified = pci::read_config32_at(kind, bus, slot, function, 4);
    if verified == u32::MAX {
        return -3;
    }
    let required = 4 | (flags & 3) as u16;
    if verified as u16 & required != required || verified as u16 & 0x400 != 0 {
        return -4;
    }
    0
}
/// # Safety
/// The output is an aligned resident private C record under the original
/// driver hardware admission and stays valid through configuration readback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_pci_map(
    kind: u8,
    bus: u8,
    slot: u8,
    function: u8,
    index: u8,
    bytes: u32,
    flags: u32,
    out: *mut a::MmioRegion,
) -> i32 {
    unsafe { ptr::write(out, a::MmioRegion::default()) };
    if index >= 6 {
        return -1;
    }
    let raw = r4os_rust_driver_pci_bar(kind, bus, slot, function, index, false) as u32;
    if raw == 0 || raw == u32::MAX {
        return -2;
    }
    if raw & 1 != 0 {
        return -3;
    }
    let base = r4os_rust_driver_pci_bar(kind, bus, slot, function, index, true) & !15;
    if base == 0 {
        return -4;
    }
    let requested = if bytes == 0 { 4096 } else { u64::from(bytes) };
    if requested > 16 * 1024 * 1024 {
        return -5;
    }
    let page = base & !4095;
    let offset = base - page;
    let total = (offset + requested + 4095) & !4095;
    let mut mapped = 0;
    while mapped < total {
        let physical = page + mapped;
        let address = phys::r4os_rust_phys_to_virt(physical);
        let mut value = 0;
        if !unsafe { paging::r4os_rust_paging_read(address, 3, false, &mut value) }
            && !paging::r4os_rust_paging_map(address, physical, 2 | (1 << 4) | (1 << 63), false)
        {
            return -6;
        }
        mapped += 4096
    }
    let address = phys::r4os_rust_phys_to_virt(base);
    if flags & 1 != 0 {
        paging::r4os_rust_paging_write_combining(address, requested);
    }
    unsafe {
        ptr::write(
            out,
            a::MmioRegion {
                phys_addr: base,
                virt_addr: address,
                bytes: requested as u32,
                mapped_bytes: total as u32,
                bar_index: index,
                flags: flags as u8,
                ..a::MmioRegion::default()
            },
        )
    };
    0
}
