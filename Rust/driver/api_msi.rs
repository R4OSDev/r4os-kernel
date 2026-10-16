//! Original eight single-message MSI/MSI-X ownership and explicit rollback.
//! No hardware release is published until its exact configuration readback.
use crate::{
    arch::smp,
    contract as a,
    driver::{api_admission as admission, api_platform as platform, utilities},
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr};
#[derive(Clone, Copy)]
struct Device {
    kind: u8,
    bus: u8,
    slot: u8,
    function: u8,
}
impl Device {
    fn read(self, offset: u16) -> u32 {
        platform::r4os_rust_driver_pci_read(self.kind, self.bus, self.slot, self.function, offset)
    }
    fn write(self, offset: u16, value: u32) -> bool {
        platform::r4os_rust_driver_pci_write(
            self.kind,
            self.bus,
            self.slot,
            self.function,
            offset,
            value,
        ) == 0
    }
}
#[derive(Clone, Copy)]
struct Record {
    used: bool,
    msix: bool,
    owner: u32,
    device: Device,
    irq: u8,
    capability: u16,
    control: u16,
    command: u16,
    table: u64,
    address_low: u32,
    address_high: u32,
    data: u32,
    vector_control: u32,
}
impl Record {
    const EMPTY: Self = Self {
        used: false,
        msix: false,
        owner: 0,
        device: Device {
            kind: 0,
            bus: 0,
            slot: 0,
            function: 0,
        },
        irq: 0,
        capability: 0,
        control: 0,
        command: 0,
        table: 0,
        address_low: 0,
        address_high: 0,
        data: 0,
        vector_control: 0,
    };
}
struct Storage(UnsafeCell<[Record; 8]>);
// SAFETY: the historical R4D execution owner serializes these hardware calls.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new([Record::EMPTY; 8]));

fn owner() -> u32 {
    admission::r4os_rust_driver_api_runtime_owner(false)
}
fn read_table(base: u64, offset: u32) -> u32 {
    unsafe { ptr::read_volatile((base + u64::from(offset)) as *const u32) }
}
fn write_table(base: u64, offset: u32, value: u32) {
    unsafe { ptr::write_volatile((base + u64::from(offset)) as *mut u32, value) };
}
fn fence() {
    unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)) };
}
fn write_control(d: Device, cap: u16, header: u32, control: u16) -> bool {
    d.write(cap, (header & 0xffff) | (u32::from(control) << 16))
}
fn restore_msi(r: Record) -> bool {
    let d = r.device;
    let header = d.read(r.capability);
    if header == u32::MAX {
        return false;
    }
    let disabled = (header >> 16) as u16 & !1;
    if !write_control(d, r.capability, header, disabled) {
        return false;
    }
    let verified = d.read(r.capability);
    if verified == u32::MAX || verified >> 16 & 1 != 0 {
        return false;
    }
    if !write_control(d, r.capability, verified, r.control) {
        return false;
    }
    let command = d.read(4);
    if command == u32::MAX {
        return false;
    }
    let restored = (command & 0xffff & !(1 << 10)) | (u32::from(r.command) & (1 << 10));
    if !d.write(4, restored) {
        return false;
    }
    let header = d.read(r.capability);
    let command = d.read(4);
    header != u32::MAX
        && header >> 16 & 1 == 0
        && command != u32::MAX
        && command & (1 << 10) == u32::from(r.command) & (1 << 10)
}
fn restore_msix(r: Record) -> bool {
    let d = r.device;
    let header = d.read(r.capability);
    if header == u32::MAX || r.table == 0 {
        return false;
    }
    let control = (header >> 16) as u16;
    if !write_control(d, r.capability, header, (control & !0x8000) | 0x4000) {
        return false;
    }
    write_table(r.table, 12, read_table(r.table, 12) | 1);
    fence();
    write_table(r.table, 0, r.address_low);
    write_table(r.table, 4, r.address_high);
    write_table(r.table, 8, r.data);
    write_table(r.table, 12, r.vector_control);
    fence();
    if !write_control(d, r.capability, header, r.control) {
        return false;
    }
    let command = d.read(4);
    if command == u32::MAX {
        return false;
    }
    let restored = (command & 0xffff & !(1 << 10)) | (u32::from(r.command) & (1 << 10));
    if !d.write(4, restored) {
        return false;
    }
    let header = d.read(r.capability);
    let command = d.read(4);
    header != u32::MAX
        && (header >> 16) as u16 & 0xc000 == r.control & 0xc000
        && command != u32::MAX
        && command & (1 << 10) == u32::from(r.command) & (1 << 10)
}
fn restore(r: Record) -> bool {
    if r.msix {
        restore_msix(r)
    } else {
        restore_msi(r)
    }
}
fn enable_msix(
    d: Device,
    id: u32,
    slot: usize,
    irq: u8,
    vector: u32,
    cap: u16,
    status: u32,
) -> i32 {
    let header = d.read(cap);
    if header == u32::MAX {
        return -1;
    }
    let control = (header >> 16) as u16;
    if control & 0x8000 != 0 {
        return -7;
    }
    let descriptor = d.read(cap + 4);
    if descriptor == u32::MAX {
        return -1;
    }
    let mut geometry = MaybeUninit::uninit();
    if !unsafe {
        utilities::r4os_rust_driver_msix_geometry(
            descriptor,
            16 * 1024 * 1024,
            geometry.as_mut_ptr(),
        )
    } {
        return -8;
    }
    let g = unsafe { geometry.assume_init() };
    let mut region = a::MmioRegion::default();
    if unsafe {
        platform::r4os_rust_driver_pci_map(
            d.kind,
            d.bus,
            d.slot,
            d.function,
            g.bir,
            g.mapping_bytes,
            0,
            &mut region,
        )
    } != 0
    {
        return -8;
    }
    let table = region.virt_addr + u64::from(g.offset);
    let candidate = Record {
        used: true,
        msix: true,
        owner: id,
        device: d,
        irq,
        capability: cap,
        control,
        command: status as u16,
        table,
        address_low: read_table(table, 0),
        address_high: read_table(table, 4),
        data: read_table(table, 8),
        vector_control: read_table(table, 12),
    };
    let masked = (control & !0x8000) | 0x4000;
    if !write_control(d, cap, header, masked) {
        return -4;
    }
    write_table(table, 12, candidate.vector_control | 1);
    let target = smp::r4os_rust_smp_control(5, (slot + 1) as u32) as u32;
    write_table(table, 0, 0xfee0_0000 | (target << 12));
    write_table(table, 4, 0);
    write_table(table, 8, vector);
    fence();
    if !write_control(d, cap, header, masked | 0x8000)
        || !write_control(d, cap, header, (control | 0x8000) & !0x4000)
    {
        restore_msix(candidate);
        return -4;
    }
    let verified = d.read(cap);
    let entry = read_table(table, 12);
    if verified == u32::MAX
        || verified >> 16 & 0x8000 == 0
        || verified >> 16 & 0x4000 != 0
        || entry & 1 == 0
    {
        restore_msix(candidate);
        return -5;
    }
    let command = d.read(4);
    if command == u32::MAX || !d.write(4, (command & 0xffff) | (1 << 10)) {
        restore_msix(candidate);
        return -4;
    }
    let command = d.read(4);
    if command == u32::MAX || command & (1 << 10) == 0 {
        restore_msix(candidate);
        return -5;
    }
    unsafe {
        (*STATE.0.get())[slot] = candidate;
        driver_msi_log(true, irq, vector)
    };
    i32::from(irq)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_msi_enable(kind: u8, bus: u8, device: u8, function: u8) -> i32 {
    let id = owner();
    if id == 0 {
        return -6;
    }
    let d = Device {
        kind,
        bus,
        slot: device,
        function,
    };
    for i in 0..8 {
        let r = unsafe { (*STATE.0.get())[i] };
        if !r.used
            || r.device.kind != kind
            || r.device.bus != bus
            || r.device.slot != device
            || r.device.function != function
        {
            continue;
        }
        return if r.owner == id { i32::from(r.irq) } else { -6 };
    }
    let status = d.read(4);
    if status == u32::MAX || status & (1 << 20) == 0 {
        return -1;
    }
    let mut offset = (d.read(0x34) as u8 as u16) & 0xfc;
    let mut cap = 0;
    let mut msix = 0;
    let mut visited = 0u64;
    let mut steps = 0;
    while (0x40..=0xfc).contains(&offset) && steps < 48 {
        let bit = 1u64 << ((offset - 0x40) / 4);
        if visited & bit != 0 {
            return -1;
        }
        visited |= bit;
        let header = d.read(offset);
        if header == u32::MAX {
            return -1;
        }
        let kind = header as u8;
        if kind == 5 {
            cap = offset;
            break;
        }
        if kind == 0x11 && msix == 0 {
            msix = offset
        }
        let next = ((header >> 8) as u8 as u16) & 0xfc;
        if next == 0 || next == offset {
            break;
        }
        offset = next;
        steps += 1;
    }
    if cap == 0 && msix == 0 {
        return -2;
    }
    let mut slot = 0;
    while slot < 8 && unsafe { (*STATE.0.get())[slot].used } {
        slot += 1
    }
    if slot == 8 {
        return -3;
    }
    let irq = 24 + slot as u8;
    let vector = 32 + u32::from(irq);
    if cap == 0 {
        return enable_msix(d, id, slot, irq, vector, msix, status);
    }
    let header = d.read(cap);
    if header == u32::MAX {
        return -1;
    }
    let control = (header >> 16) as u16;
    if control & 1 != 0 {
        return -7;
    }
    let wide = control & (1 << 7) != 0;
    let address = 0xfee0_0000 | ((smp::r4os_rust_smp_control(5, (slot + 1) as u32) as u32) << 12);
    if !d.write(cap + 4, address) {
        return -4;
    }
    if wide {
        if !d.write(cap + 8, 0) || !d.write(cap + 12, vector) {
            return -4;
        }
    } else if !d.write(cap + 8, vector) {
        return -4;
    }
    let candidate = Record {
        used: true,
        owner: id,
        device: d,
        irq,
        capability: cap,
        control,
        command: status as u16,
        ..Record::EMPTY
    };
    if !d.write(
        cap,
        (header & 0xffff) | ((u32::from(control) & !0x70 | 1) << 16),
    ) {
        restore_msi(candidate);
        return -4;
    }
    let verified = d.read(cap);
    if verified == u32::MAX || verified >> 16 & 1 == 0 {
        restore_msi(candidate);
        return -5;
    }
    let command = d.read(4);
    if command != u32::MAX {
        if !d.write(4, (command & 0xffff) | (1 << 10)) {
            restore_msi(candidate);
            return -4;
        }
        let verify = d.read(4);
        if verify == u32::MAX || verify & (1 << 10) == 0 {
            restore_msi(candidate);
            return -5;
        }
    }
    unsafe {
        (*STATE.0.get())[slot] = candidate;
        driver_msi_log(false, irq, vector)
    };
    i32::from(irq)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_msi_unmask(id: u32, irq: u8) -> bool {
    for i in 0..8 {
        let r = unsafe { (*STATE.0.get())[i] };
        if !r.used || !r.msix || r.owner != id || r.irq != irq {
            continue;
        }
        let control = read_table(r.table, 12);
        write_table(r.table, 12, control & !1);
        fence();
        return read_table(r.table, 12) & 1 == 0;
    }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_msi_disable(kind: u8, bus: u8, device: u8, function: u8) -> i32 {
    let id = owner();
    for i in 0..8 {
        let r = unsafe { (*STATE.0.get())[i] };
        if !r.used
            || r.device.kind != kind
            || r.device.bus != bus
            || r.device.slot != device
            || r.device.function != function
        {
            continue;
        }
        if id != 0 && r.owner != id {
            return -2;
        }
        if !restore(r) {
            return -3;
        }
        unsafe { (*STATE.0.get())[i] = Record::EMPTY };
        return 0;
    }
    0
}
#[repr(C)]
pub struct Cleanup {
    pub removed: u32,
    pub failed: bool,
}
/// # Safety
/// The output is an aligned resident private C record under the original
/// driver hardware admission and stays valid through configuration readback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_msi_cleanup(id: u32, out: *mut Cleanup) {
    let mut result = Cleanup {
        removed: 0,
        failed: false,
    };
    for i in 0..8 {
        let r = unsafe { (*STATE.0.get())[i] };
        if !r.used || r.owner != id {
            continue;
        }
        if !restore(r) {
            result.failed = true;
            continue;
        }
        unsafe { (*STATE.0.get())[i] = Record::EMPTY };
        result.removed += 1
    }
    unsafe { ptr::write(out, result) };
}

unsafe fn driver_msi_log(msix: bool, irq: u8, vector: u32) {
    use crate::boot::log as l;
    l::text(if msix {
        b"[R4D] MSI-X prepared irq="
    } else {
        b"[R4D] MSI enabled irq="
    });
    l::r4os_rust_bootlog_dec(irq.into());
    l::text(b" vector=");
    l::r4os_rust_bootlog_dec(vector.into());
    l::text(b"\r\n");
}
