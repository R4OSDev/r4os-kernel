//! Bounded inventory preserves the complete existing classification and output.
fn known_name(
    class_code: u8,
    subclass: u8,
    prog_if: u8,
    vendor_id: u16,
    device_id: u16,
) -> &'static [u8] {
    if vendor_id == 0x8086 && device_id == 0x29C0 {
        return b"Intel Q35 host bridge";
    }
    if vendor_id == 0x8086 && device_id == 0x2918 {
        return b"Intel ICH9 LPC/ISA bridge";
    }
    if vendor_id == 0x8086 && device_id == 0x2930 {
        return b"Intel ICH9 SMBus";
    }
    if vendor_id == 0x8086 && device_id == 0x2922 {
        return b"Intel ICH9 AHCI";
    }
    if vendor_id == 0x8086 && device_id == 0x293E {
        return b"Intel ICH9 HDA";
    }
    if vendor_id == 0x8086 && device_id == 0x1237 {
        return b"Intel i440FX host bridge";
    }
    if vendor_id == 0x8086 && device_id == 0x7000 {
        return b"Intel PIIX3 ISA bridge";
    }
    if vendor_id == 0x8086 && device_id == 0x7010 {
        return b"Intel PIIX3 IDE";
    }
    if vendor_id == 0x8086 && device_id == 0x7113 {
        return b"Intel PIIX4 ACPI";
    }
    if vendor_id == 0x8086 && device_id == 0x100E {
        return b"Intel e1000 network";
    }
    if vendor_id == 0x8086 && device_id == 0x10D3 {
        return b"Intel e1000e network";
    }
    if vendor_id == 0x10EC && device_id == 0x8139 {
        return b"Realtek RTL8139 network";
    }
    if vendor_id == 0x8086 && device_id == 0x2668 {
        return b"Intel HDA";
    }
    if vendor_id == 0x1234 && device_id == 0x1111 {
        return b"QEMU Standard VGA";
    }
    if vendor_id == 0x1B36 && device_id == 0x0010 {
        return b"QEMU NVMe";
    }
    if class_code == 0x01 && subclass == 0x01 {
        return b"IDE controller";
    }
    if class_code == 0x01 && subclass == 0x06 {
        return b"AHCI SATA controller";
    }
    if class_code == 0x01 && subclass == 0x08 {
        return b"NVMe controller";
    }
    if class_code == 0x02 {
        return b"Network controller";
    }
    if class_code == 0x03 {
        return b"Display controller";
    }
    if class_code == 0x04 && subclass == 0x03 {
        return b"High Definition Audio";
    }
    if class_code == 0x06 && subclass == 0x00 {
        return b"Host bridge";
    }
    if class_code == 0x06 && subclass == 0x01 {
        return b"ISA bridge";
    }
    if class_code == 0x0C && subclass == 0x03 && prog_if == 0x30 {
        return b"xHCI USB controller";
    }
    if class_code == 0x0C && subclass == 0x05 {
        return b"SMBus controller";
    }
    b""
}

fn driver_for(
    binding: u8,
    class_code: u8,
    subclass: u8,
    vendor_id: u16,
    device_id: u16,
) -> &'static [u8] {
    if binding != WITH_DRIVER {
        return b"";
    }
    if vendor_id == 0x10EC && device_id == 0x8139 {
        return b"RTL8139";
    }
    if vendor_id == 0x10EC && device_id == 0x8168 {
        return b"RTL8168";
    }
    if class_code == 0x01 && subclass == 0x01 {
        return b"ATAPIO";
    }
    if class_code == 0x01 && subclass == 0x06 {
        return b"AHCI";
    }
    if class_code == 0x01 && subclass == 0x08 {
        return b"NVMe";
    }
    if class_code == 0x0C && subclass == 0x03 {
        return b"XHCI";
    }
    if class_code == 0x04 && subclass == 0x03 {
        return b"HDA";
    }
    if class_code == 0x06 {
        return b"platform/chipset";
    }
    b"platform"
}

fn block_note(d: &BlockInput) -> &'static [u8] {
    if d.bus == NVME {
        if d.source == PRELOAD && d.port == 1 {
            return b"NVME.R4D source=preload; namespace NSID 1; read/write blockdevice";
        }
        if d.source == PRELOAD && d.port == 2 {
            return b"NVME.R4D source=preload; namespace NSID 2; read/write blockdevice";
        }
        if d.source == PRELOAD && d.port == 3 {
            return b"NVME.R4D source=preload; namespace NSID 3; read/write blockdevice";
        }
        if d.source == PRELOAD && d.port == 4 {
            return b"NVME.R4D source=preload; namespace NSID 4; read/write blockdevice";
        }
        if d.source == PRELOAD {
            return b"NVME.R4D source=preload; namespace read/write blockdevice";
        }
        if d.port == 1 {
            return b"NVMe namespace NSID 1; read/write blockdevice";
        }
        if d.port == 2 {
            return b"NVMe namespace NSID 2; read/write blockdevice";
        }
        if d.port == 3 {
            return b"NVMe namespace NSID 3; read/write blockdevice";
        }
        if d.port == 4 {
            return b"NVMe namespace NSID 4; read/write blockdevice";
        }
        return b"NVMe namespace; read/write blockdevice";
    }
    if d.bus == ATA && d.source == PRELOAD {
        return b"ATAPIO.R4D source=preload; legacy IDE/ATA read/write blockdevice";
    }
    if d.driver.slice() == b"R4D" {
        if d.source == PRELOAD {
            return b"R4D StorageBackend source=preload; structured blockdevice";
        }
        if d.source == DISK {
            return b"R4D StorageBackend source=disk; structured blockdevice";
        }
        return b"R4D StorageBackend source=built-in; structured blockdevice";
    }
    if d.bus == USB_BLOCK {
        if d.source == PRELOAD && d.removable && d.writable {
            return b"USBMSC source=preload; USB removable read/write blockdevice";
        }
        if d.source == PRELOAD && d.removable {
            return b"USBMSC source=preload; USB removable read-only blockdevice";
        }
        if d.source == PRELOAD {
            return b"USBMSC source=preload; USB blockdevice";
        }
        if d.removable && d.writable {
            return b"USB source=built-in; removable read/write blockdevice";
        }
        if d.removable {
            return b"USB source=built-in; removable read-only blockdevice";
        }
        return b"USB source=built-in; blockdevice";
    }
    if d.bus == AHCI && d.source == PRELOAD {
        return b"AHCI.R4D source=preload; IDENTIFY-backed parallel read/write blockdevice";
    }
    if d.bus == AHCI {
        return b"AHCI source=built-in; port registered as read/write blockdevice";
    }
    if d.bus == ATA {
        return b"Legacy IDE/ATA source=built-in; device registered as blockdevice";
    }
    if d.bus == RAM {
        return b"RAM drive blockdevice";
    }
    b"registered blockdevice"
}

// Other migration waves retain their device metadata owners behind fixed C
// projections. The full snapshot and generated notes belong to this call.
use crate::{
    contract,
    platform::{acpi, cpu, irq, pci_inventory},
};
use core::{mem::MaybeUninit, ptr};
const MAX_ADAPTERS: usize = 8;
const WITH_DRIVER: u8 = 0;
const WITHOUT_DRIVER: u8 = 1;
const UNKNOWN: u8 = 2;
const PLATFORM: u8 = 0;
const ACPI: u8 = 1;
const PCI: u8 = 2;
const PCIE: u8 = 3;
const STORAGE: u8 = 4;
const DISPLAY: u8 = 5;
const DRIVER: u8 = 8;
const NETWORK: u8 = 9;
const USB: u8 = 10;
const PROTOCOL: u8 = 11;
const ATA: u8 = 1;
const AHCI: u8 = 2;
const NVME: u8 = 3;
const USB_BLOCK: u8 = 4;
const RAM: u8 = 5;
const PRELOAD: u8 = 1;
const DISK: u8 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Text {
    pub pointer: *const u8,
    pub bytes: u64,
}
impl Text {
    const EMPTY: Self = Self {
        pointer: ptr::null(),
        bytes: 0,
    };
    fn new(value: &[u8]) -> Self {
        Self {
            pointer: value.as_ptr(),
            bytes: value.len() as u64,
        }
    }
    fn slice<'a>(self) -> &'a [u8] {
        if self.bytes == 0 {
            &[]
        } else {
            // Private metadata producers supply live owner storage through the
            // synchronous inventory call; generated notes use this call's stack.
            unsafe { core::slice::from_raw_parts(self.pointer, self.bytes as usize) }
        }
    }
}
#[repr(C)]
pub struct DisplayInput {
    pub name: Text,
    pub policy: Text,
}
#[repr(C)]
pub struct BlockInput {
    pub name: Text,
    pub driver: Text,
    pub bus: u8,
    pub source: u8,
    pub port: u8,
    pub removable: bool,
    pub writable: bool,
}
#[repr(C)]
pub struct UsbInput {
    pub configured: bool,
    pub vendor_id: u16,
    pub product_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub identity: crate::usb::xhci_types::DeviceHandle,
}
#[repr(C)]
pub struct HidInput {
    pub keyboard: bool,
    pub mouse: bool,
    pub missing: u32,
    pub source: Text,
}
#[repr(C)]
pub struct MscInput {
    pub registered: bool,
    pub write_protected: bool,
    pub external: bool,
    pub missing: u64,
    pub bot: Text,
    pub scsi: Text,
}
#[repr(C)]
pub struct DriverInput {
    pub kind: u16,
    pub name: [u8; 32],
    pub name_len: u64,
    pub source: Text,
    pub state: Text,
    pub type_name: Text,
}
#[repr(C)]
pub struct ProtocolInput {
    pub category: u16,
    pub role: [u8; 64],
    pub role_len: u64,
    pub source: Text,
    pub state: Text,
    pub note: [u8; 96],
    pub note_len: u64,
}
#[repr(C)]
pub struct NetInput {
    pub name: Text,
    pub driver: Text,
    pub link: Text,
    pub last_error: Text,
    pub config_source: Text,
    pub rx: u64,
    pub tx: u64,
    pub errors: u64,
    pub mac: [u8; 6],
    pub ip: [u8; 4],
    pub gateway: [u8; 4],
    pub bus: u8,
    pub bus_no: u8,
    pub device_no: u8,
    pub function_no: u8,
    pub vendor_id: u16,
    pub device_id: u16,
}
#[repr(C)]
pub struct XhciInput {
    pub present: bool,
    pub exists: bool,
    pub status_ok: bool,
    pub source: u8,
    pub state: u8,
    pub source_label: Text,
    pub flags: u32,
    pub queue_depth: u16,
    pub max_transfer_bytes: u32,
    pub active_transfers: u32,
    pub completions: u64,
    pub interrupts: u64,
    pub polls: u64,
    pub cancellations: u64,
}

fn scalar(operation: u8, argument: u64) -> u64 {
    unsafe { inventory_scalar(operation, argument) }
}
fn block_at(index: usize) -> Option<BlockInput> {
    let mut value = MaybeUninit::uninit();
    if unsafe { inventory_block(index as u64, value.as_mut_ptr()) } {
        Some(unsafe { value.assume_init() })
    } else {
        None
    }
}
fn hid(device: &UsbInput) -> HidInput {
    let mut value = MaybeUninit::uninit();
    unsafe {
        inventory_hid(value.as_mut_ptr());
        let mut value = value.assume_init();
        (value.keyboard, value.mouse) = crate::usb::hid::binding_for(device.identity);
        value
    }
}
fn msc(device: &UsbInput) -> MscInput {
    let mut value = MaybeUninit::uninit();
    unsafe {
        inventory_msc(value.as_mut_ptr());
        let mut value = value.assume_init();
        let binding = crate::usb::msc::binding_for(device.identity);
        value.registered = binding.is_some();
        value.write_protected = binding.unwrap_or(false);
        value
    }
}
fn xhci(pci_function: crate::platform::pci_scan::Device, with_status: bool) -> XhciInput {
    let mut value = MaybeUninit::uninit();
    unsafe {
        inventory_xhci(pci_function, with_status, value.as_mut_ptr());
        value.assume_init()
    }
}
#[derive(Clone, Copy)]
struct Record {
    binding: u8,
    bus: u8,
    name: Text,
    driver: Text,
    status: Text,
    note: Text,
    bus_no: u8,
    device_no: u8,
    function_no: u8,
    vendor_id: u16,
    device_id: u16,
    class_code: u8,
    subclass: u8,
    prog_if: u8,
    has_pci_address: bool,
}
impl Record {
    const EMPTY: Self = Self {
        binding: UNKNOWN,
        bus: PLATFORM,
        name: Text::EMPTY,
        driver: Text::EMPTY,
        status: Text::EMPTY,
        note: Text::EMPTY,
        bus_no: 0,
        device_no: 0,
        function_no: 0,
        vendor_id: 0,
        device_id: 0,
        class_code: 0,
        subclass: 0,
        prog_if: 0,
        has_pci_address: false,
    };
    fn named(bus: u8, name: &[u8], driver: &[u8], status: &[u8], note: &[u8]) -> Self {
        Self {
            binding: WITH_DRIVER,
            bus,
            name: Text::new(name),
            driver: Text::new(driver),
            status: Text::new(status),
            note: Text::new(note),
            ..Self::EMPTY
        }
    }
}
struct Snapshot {
    selected: Record,
    bus_counts: Counts,
    count: usize,
    with_driver: u32,
    without_driver: u32,
    unknown: u32,
    truncated: bool,
    network_notes: [[u8; 96]; MAX_ADAPTERS],
    xhci_note: [u8; 256],
    // Driver registry snapshots copy names; retain each copy at this exact
    // stationary snapshot address until all public records have been emitted.
    driver_name: [u8; 32],
    // Only record() reads text bytes after collection; summary/counts consume
    // scalar classifications. Retain the requested protocol's text without
    // enlarging the existing 16-KB snapshot stack limit or allocating storage.
    requested_text_index: usize,
    protocol_role: [u8; 64],
    protocol_note: [u8; 96],
}
impl Snapshot {
    // Initialization uses exclusive caller storage and does not produce another
    // full array value on the bounded kernel stack. Only populated records are read.
    unsafe fn initialize(out: *mut Self, requested_text_index: usize) {
        unsafe {
            ptr::addr_of_mut!((*out).count).write(0);
            ptr::addr_of_mut!((*out).with_driver).write(0);
            ptr::addr_of_mut!((*out).without_driver).write(0);
            ptr::addr_of_mut!((*out).unknown).write(0);
            ptr::addr_of_mut!((*out).truncated).write(false);
            ptr::addr_of_mut!((*out).bus_counts).write(Counts {
                drivers: 0,
                protocols: 0,
                displays: 0,
            });
            ptr::addr_of_mut!((*out).requested_text_index).write(requested_text_index);
            ptr::addr_of_mut!((*out).network_notes)
                .cast::<u8>()
                .write_bytes(0, MAX_ADAPTERS * 96);
            ptr::addr_of_mut!((*out).xhci_note)
                .cast::<u8>()
                .write_bytes(0, 256);
        }
    }
}
// Stream all counts and retain only the requested record. No inventory-sized
// stack allocation or Snapshot reference spans a device callback.
fn add(snapshot: *mut Snapshot, record: Record) {
    unsafe {
        let count = ptr::addr_of!((*snapshot).count).read();
        if count == u32::MAX as usize {
            (*snapshot).truncated = true;
            return;
        }
        if count == (*snapshot).requested_text_index {
            ptr::addr_of_mut!((*snapshot).selected).write(record);
        }
        ptr::addr_of_mut!((*snapshot).count).write(count + 1);
        match record.bus {
            DRIVER => (*snapshot).bus_counts.drivers += 1,
            PROTOCOL => (*snapshot).bus_counts.protocols += 1,
            DISPLAY => (*snapshot).bus_counts.displays += 1,
            _ => {}
        }
        let counter = match record.binding {
            WITH_DRIVER => ptr::addr_of_mut!((*snapshot).with_driver),
            WITHOUT_DRIVER => ptr::addr_of_mut!((*snapshot).without_driver),
            UNKNOWN => ptr::addr_of_mut!((*snapshot).unknown),
            _ => panic!("Invalid inventory binding"),
        };
        counter.write(counter.read() + 1);
    }
}
#[inline(never)]
fn collect(snapshot: *mut Snapshot) {
    add_platform(snapshot);
    let mut index = 0;
    while let Some(device) = pci_inventory::device_at(index) {
        let record = pci_record(snapshot, device);
        add(snapshot, record);
        index += 1;
    }
    add_usb(snapshot);
    for index in 0..scalar(5, 0) as usize {
        let mut entry = MaybeUninit::uninit();
        if !unsafe { inventory_driver(index as u64, entry.as_mut_ptr()) } {
            continue;
        }
        let entry = unsafe { entry.assume_init() };
        if matches!(entry.kind, 2 | 3 | 5) {
            continue;
        }
        assert!(entry.name_len <= 32);
        let name = if unsafe { (*snapshot).count != (*snapshot).requested_text_index } {
            Text::EMPTY
        } else {
            unsafe {
                let destination = ptr::addr_of_mut!((*snapshot).driver_name).cast::<u8>();
                ptr::copy_nonoverlapping(entry.name.as_ptr(), destination, 32);
                Text {
                    pointer: destination,
                    bytes: entry.name_len,
                }
            }
        };
        add(
            snapshot,
            Record {
                binding: WITH_DRIVER,
                bus: DRIVER,
                name,
                driver: entry.source,
                status: entry.state,
                note: entry.type_name,
                ..Record::EMPTY
            },
        );
    }
    add_network(snapshot);
    for index in 0..scalar(6, 0) as usize {
        let mut entry = MaybeUninit::uninit();
        if !unsafe { inventory_protocol(index as u64, entry.as_mut_ptr()) } {
            continue;
        }
        let entry = unsafe { entry.assume_init() };
        assert!(
            index < crate::driver::protocol_registry::MAX_PROTOCOLS
                && entry.role_len <= 64
                && entry.note_len <= 96
        );
        let (role, note) = if unsafe { (*snapshot).count == (*snapshot).requested_text_index } {
            unsafe {
                let role = ptr::addr_of_mut!((*snapshot).protocol_role).cast::<u8>();
                let note = ptr::addr_of_mut!((*snapshot).protocol_note).cast::<u8>();
                ptr::copy_nonoverlapping(entry.role.as_ptr(), role, 64);
                ptr::copy_nonoverlapping(entry.note.as_ptr(), note, 96);
                (
                    Text {
                        pointer: role,
                        bytes: entry.role_len,
                    },
                    Text {
                        pointer: note,
                        bytes: entry.note_len,
                    },
                )
            }
        } else {
            (Text::EMPTY, Text::EMPTY)
        };
        add(
            snapshot,
            Record {
                binding: WITH_DRIVER,
                bus: PROTOCOL,
                name: role,
                driver: entry.source,
                status: entry.state,
                note,
                class_code: entry.category.min(255) as u8,
                ..Record::EMPTY
            },
        );
    }
}
fn add_platform(snapshot: *mut Snapshot) {
    let cpu = cpu::snapshot();
    let note: &[u8] = if cpu::r4os_rust_cpu_control(4, 0) != 0 {
        b"FPU/SSE task state active; module SIMD allowed"
    } else if cpu.features.xsave && !cpu.features.osxsave {
        b"XSAVE present but OSXSAVE off; module SIMD blocked"
    } else {
        b"feature gates active"
    };
    add(
        snapshot,
        Record::named(
            PLATFORM,
            b"x86_64 CPU",
            b"platform/cpu",
            b"diagnostic-active",
            note,
        ),
    );
    let acpi = acpi::snapshot();
    let mut record = Record::named(
        ACPI,
        b"ACPI tables",
        b"platform/acpi",
        if acpi.rsdp_phys != 0 {
            b"active"
        } else {
            b"missing"
        },
        if acpi.mcfg_base != 0 {
            b"RSDP/MADT/MCFG/HPET visible"
        } else {
            b"RSDP/MADT visible, MCFG missing or legacy profile"
        },
    );
    record.binding = if acpi.rsdp_phys != 0 {
        WITH_DRIVER
    } else {
        WITHOUT_DRIVER
    };
    add(snapshot, record);
    let irq = irq::snapshot();
    add(
        snapshot,
        Record::named(
            PLATFORM,
            b"Interrupt controller",
            b"platform/irq",
            irq::controller_name(irq.controller),
            irq::reason_name(irq.fallback_reason),
        ),
    );
    add(
        snapshot,
        Record::named(
            PLATFORM,
            b"Timer",
            b"platform/timer",
            irq::timer_name(irq.timer),
            if irq.hpet_available {
                b"HPET visible"
            } else {
                b"legacy timer fallback"
            },
        ),
    );
    let mut display = MaybeUninit::<DisplayInput>::uninit();
    unsafe { inventory_display(display.as_mut_ptr()) };
    let display = unsafe { display.assume_init() };
    add(
        snapshot,
        Record {
            binding: WITH_DRIVER,
            bus: DISPLAY,
            name: Text::new(b"Active display backend"),
            driver: display.name,
            status: Text::new(b"active"),
            note: display.policy,
            ..Record::EMPTY
        },
    );
    let mut record = Record::named(
        STORAGE,
        b"Block storage registry",
        b"storage/block",
        if scalar(0, 0) > 0 {
            b"active"
        } else {
            b"empty"
        },
        if scalar(2, u64::from(b'C')) != 0 && scalar(2, u64::from(b'D')) != 0 {
            b"C: and D: mounted"
        } else {
            b"mounts incomplete"
        },
    );
    record.binding = if scalar(0, 0) > 0 {
        WITH_DRIVER
    } else {
        WITHOUT_DRIVER
    };
    add(snapshot, record);
    let mut index = 0;
    while index < scalar(1, 0) as usize {
        if let Some(device) = block_at(index) {
            add(
                snapshot,
                Record {
                    binding: WITH_DRIVER,
                    bus: STORAGE,
                    name: device.name,
                    driver: device.driver,
                    status: Text::new(block_mount_status(index)),
                    note: Text::new(block_note(&device)),
                    ..Record::EMPTY
                },
            );
        }
        index += 1;
    }
}
fn is_mounted(index: usize, letter: u8) -> bool {
    scalar(3, ((index as u64) << 8) | u64::from(letter)) != 0
}
fn block_mount_status(index: usize) -> &'static [u8] {
    for (letter, status) in [
        (b'C', b"mounted-C".as_slice()),
        (b'D', b"mounted-D"),
        (b'E', b"mounted-E"),
    ] {
        if is_mounted(index, letter) {
            return status;
        }
    }
    for letter in b'A'..=b'Z' {
        if is_mounted(index, letter) {
            return b"mounted";
        }
    }
    b"active-unmounted"
}
fn block_count(bus: u8, preload_only: bool) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < scalar(1, 0) as usize {
        if let Some(device) = block_at(index)
            && device.bus == bus
            && (!preload_only || device.source == PRELOAD)
        {
            count += 1;
        }
        index += 1;
    }
    count
}
fn has_net(vendor: u16, device: u16) -> bool {
    scalar(7, (u64::from(vendor) << 16) | u64::from(device)) != 0
}
fn add_usb(snapshot: *mut Snapshot) {
    let mut index = 0;
    loop {
        let mut device = MaybeUninit::<UsbInput>::uninit();
        if !unsafe { inventory_usb(index, device.as_mut_ptr()) } {
            break;
        }
        index += 1;
        let device = unsafe { device.assume_init() };
        let (class, subclass, protocol) = if device.interface_class != 0 {
            (
                device.interface_class,
                device.interface_subclass,
                device.interface_protocol,
            )
        } else {
            (device.class, device.subclass, device.protocol)
        };
        if class == 8 && scalar(4, 0) == 0 {
            continue;
        }
        let name = usb_name(class, subclass, protocol);
        let driver = usb_driver(&device, class, subclass, protocol);
        let status = usb_status(&device, class, subclass, protocol);
        let note = usb_note(&device, class, subclass, protocol);
        add(
            snapshot,
            Record {
                binding: WITH_DRIVER,
                bus: USB,
                name: Text::new(name),
                driver: Text::new(driver),
                status: Text::new(status),
                note: Text::new(note),
                vendor_id: device.vendor_id,
                device_id: device.product_id,
                class_code: class,
                subclass,
                prog_if: protocol,
                ..Record::EMPTY
            },
        );
    }
    for mode in 0..5 {
        let name = match mode {
            0 => b"USB completion work".as_slice(),
            1 => b"USB IRQ work".as_slice(),
            2 => b"USB completion wake".as_slice(),
            3 => b"USB HID work".as_slice(),
            _ => b"USB buffer work".as_slice(),
        };
        let note = usb_work_note(snapshot, mode);
        add(
            snapshot,
            Record::named(USB, name, b"usb/xhci", b"counters", note.slice()),
        );
    }
}

// Like PCI notes, only the requested record owns text in this existing
// stationary buffer. This adds no ABI or 16-KB snapshot-stack allocation.
fn usb_work_note(snapshot: *mut Snapshot, mode: u8) -> Text {
    if unsafe { (*snapshot).count != (*snapshot).requested_text_index } {
        return Text::EMPTY;
    }
    let out = unsafe { &mut *ptr::addr_of_mut!((*snapshot).xhci_note) };
    out.fill(0);
    let mut w = NoteWriter::new(out);
    if mode == 4 {
        let Some(v) = crate::usb::xhci_state::buffer_work() else {
            return Text::new(b"unavailable");
        };
        for (name, value) in [
            (b"clear=".as_slice(), v[0]),
            (b" inbytes=", v[1]),
            (b" outbytes=", v[2]),
            (b" pages=", v[3]),
        ] {
            w.text(name);
            w.num(value);
        }
    } else if mode == 3 {
        let Some(v) = crate::usb::hid::work_counters() else {
            return Text::new(b"unavailable");
        };
        for (name, value) in [
            (b"tick=".as_slice(), v[0]),
            (b" hz=", v[1]),
            (b" svc=", v[2]),
            (b" worker=", v[3]),
            (b" sig=", v[8]),
            (b" empty=", v[4]),
            (b" keys=", v[5]),
            (b" mouse=", v[6]),
            (b" bp=", v[7]),
            (b" mods=", v[9]),
            (b" kb=", v[10]),
            (b" mb=", v[11]),
        ] {
            w.text(name);
            w.num(value);
        }
    } else if mode == 2 {
        let v = crate::usb::xhci_state::work_counters();
        for (name, value) in [
            (b"fast=".as_slice(), v[3]),
            (b" enrolled=", v[4]),
            (b" recheck=", v[5]),
            (b" signal=", v[6]),
            (b" fallbackwait=", v[7]),
            (b" parks=", v[8]),
        ] {
            w.text(name);
            w.num(value);
        }
    } else if mode == 1 {
        let v = crate::usb::xhci_state::work_counters();
        w.text(b"irq=");
        w.num(v[0]);
        w.text(b" wake=");
        w.num(v[1]);
        w.text(b" fallback=");
        w.num(v[2]);
    } else {
        let v = crate::usb::wait::metrics();
        w.text(b"calls=");
        w.num(v.calls);
        w.text(b" loops=");
        w.num(v.iterations);
        w.text(b" sleep=");
        w.num(v.blocked_ticks);
        w.text(b" to=");
        w.num(v.timeouts);
        w.text(b" ns=");
        w.num(v.elapsed_ns);
    }
    w.result()
}

fn r4p_source(source: Text) -> bool {
    matches!(source.slice(), b"r4p" | b"preload")
}
fn usb_name(class: u8, subclass: u8, protocol: u8) -> &'static [u8] {
    if class == 3 && subclass == 1 && protocol == 1 {
        return b"USB HID Boot Keyboard";
    }
    if class == 3 && subclass == 1 && protocol == 2 {
        return b"USB HID Boot Mouse";
    }
    if class == 3 {
        return b"USB HID device";
    }
    if class == 8 {
        return b"USB Mass Storage device";
    }
    b"USB device"
}
fn usb_driver(device: &UsbInput, class: u8, subclass: u8, protocol: u8) -> &'static [u8] {
    let hid = hid(device);
    let msc = msc(device);
    if class == 3
        && subclass == 1
        && ((protocol == 1 && hid.keyboard) || (protocol == 2 && hid.mouse))
    {
        return b"USBHID";
    }
    if class == 8 && msc.registered {
        return b"USBMSC";
    }
    b"usb/core"
}
fn usb_status(device: &UsbInput, class: u8, subclass: u8, protocol: u8) -> &'static [u8] {
    let hid = hid(device);
    let msc = msc(device);
    if class == 3 && subclass == 1 && protocol == 1 {
        if hid.keyboard {
            return b"bound-input";
        }
        if hid.missing != 0 {
            return b"blocked";
        }
    }
    if class == 3 && subclass == 1 && protocol == 2 {
        if hid.mouse {
            return b"bound-input";
        }
        if hid.missing != 0 {
            return b"blocked";
        }
    }
    if class == 8 && msc.registered {
        return b"bound-storage";
    }
    if class == 8 && msc.missing != 0 {
        return b"blocked";
    }
    if device.configured {
        b"configured"
    } else {
        b"addressed"
    }
}
fn usb_note(device: &UsbInput, class: u8, subclass: u8, protocol: u8) -> &'static [u8] {
    let hid = hid(device);
    let msc = msc(device);
    if class == 3 && subclass == 1 && protocol == 1 {
        if hid.keyboard {
            return if r4p_source(hid.source) {
                b"HID boot keyboard bound through USBHID; preload/R4P-required active; PS/2 remains active"
            } else {
                b"HID boot keyboard binding requires active USBHID.R4P; PS/2 remains active"
            };
        }
        if hid.missing != 0 {
            return b"HID keyboard binding blocked; source=none; HIDREPORT.R4P and USBHID.R4P required";
        }
        return b"descriptor parsed; HID keyboard binding pending";
    }
    if class == 3 && subclass == 1 && protocol == 2 {
        if hid.mouse {
            return if r4p_source(hid.source) {
                b"HID boot mouse bound through USBHID; preload/R4P-required active; PS/2 remains active"
            } else {
                b"HID boot mouse binding requires active USBHID.R4P; PS/2 remains active"
            };
        }
        if hid.missing != 0 {
            return b"HID mouse binding blocked; source=none; HIDREPORT.R4P and USBHID.R4P required";
        }
        return b"descriptor parsed; HID mouse binding follows after keyboard path";
    }
    if class == 3 {
        return b"HID descriptor parsed; non-boot HID handling planned later";
    }
    if class == 8 && msc.registered {
        if r4p_source(msc.bot) && r4p_source(msc.scsi) {
            if msc.external {
                return if msc.write_protected {
                    b"bound through USBMSC.R4D; preload/R4P-required active; rescue; read-only blockdevice registered"
                } else {
                    b"bound through USBMSC.R4D; preload/R4P-required active; rescue; writable blockdevice registered"
                };
            }
            return if msc.write_protected {
                b"Mass Storage bound through USBMSC; preload/R4P-required active; read-only blockdevice registered"
            } else {
                b"Mass Storage bound through USBMSC; preload/R4P-required active; writable blockdevice registered"
            };
        }
        return b"Mass Storage bound through USBMSC; protocol role status unavailable";
    }
    if class == 8 && msc.missing != 0 {
        return b"Mass Storage blocked; source=none; USBBOT.R4P and USBSCSI.R4P required";
    }
    if class == 8 {
        return b"Mass Storage descriptor parsed; USBMSC follows in 0.13.7";
    }
    b"USB descriptors parsed; class driver pending"
}
struct NoteWriter<'a> {
    out: &'a mut [u8],
    pos: usize,
}
impl<'a> NoteWriter<'a> {
    fn new(out: &'a mut [u8]) -> Self {
        Self { out, pos: 0 }
    }
    fn put(&mut self, byte: u8) {
        if self.pos + 1 >= self.out.len() {
            return;
        }
        self.out[self.pos] = byte;
        self.pos += 1;
        self.out[self.pos] = 0;
    }
    fn text(&mut self, value: &[u8]) {
        for &byte in value {
            if byte == 0 {
                return;
            }
            self.put(byte);
        }
    }
    fn num(&mut self, mut value: u64) {
        if value == 0 {
            self.put(b'0');
            return;
        }
        let mut bytes = [0; 20];
        let mut pos = bytes.len();
        while value > 0 {
            pos -= 1;
            bytes[pos] = b'0' + (value % 10) as u8;
            value /= 10;
        }
        self.text(&bytes[pos..]);
    }
    fn ip(&mut self, value: [u8; 4]) {
        for (index, byte) in value.into_iter().enumerate() {
            if index != 0 {
                self.put(b'.');
            }
            self.num(u64::from(byte));
        }
    }
    fn mac(&mut self, value: [u8; 6]) {
        for (index, byte) in value.into_iter().enumerate() {
            if index != 0 {
                self.put(b':');
            }
            for nibble in [byte >> 4, byte & 15] {
                self.put(if nibble < 10 {
                    b'0' + nibble
                } else {
                    b'A' + (nibble - 10)
                });
            }
        }
    }
    fn result(&self) -> Text {
        Text::new(&self.out[..self.pos])
    }
}
fn add_network(snapshot: *mut Snapshot) {
    let mut index = 0;
    while index < scalar(8, 0) as usize {
        let mut adapter = MaybeUninit::<NetInput>::uninit();
        if unsafe { inventory_net(index as u64, adapter.as_mut_ptr()) } {
            let adapter = unsafe { adapter.assume_init() };
            let note = network_note(snapshot, index, &adapter);
            add(
                snapshot,
                Record {
                    binding: WITH_DRIVER,
                    bus: NETWORK,
                    name: adapter.name,
                    driver: adapter.driver,
                    status: adapter.link,
                    note,
                    bus_no: adapter.bus_no,
                    device_no: adapter.device_no,
                    function_no: adapter.function_no,
                    vendor_id: adapter.vendor_id,
                    device_id: adapter.device_id,
                    class_code: 2,
                    has_pci_address: matches!(adapter.bus, 1 | 2),
                    ..Record::EMPTY
                },
            );
        }
        index += 1;
    }
}
fn network_note(snapshot: *mut Snapshot, index: usize, adapter: &NetInput) -> Text {
    if index >= MAX_ADAPTERS {
        return Text::new(b"registered NetBackend; trusted direct device access");
    }
    let out = unsafe {
        &mut *ptr::addr_of_mut!((*snapshot).network_notes)
            .cast::<[u8; 96]>()
            .add(index)
    };
    let mut writer = NoteWriter::new(out);
    writer.text(b"mac=");
    writer.mac(adapter.mac);
    writer.text(b" ip=");
    writer.ip(adapter.ip);
    writer.text(b" gw=");
    writer.ip(adapter.gateway);
    writer.text(b" src=");
    writer.text(adapter.config_source.slice());
    writer.text(b" rx=");
    writer.num(adapter.rx);
    writer.text(b" tx=");
    writer.num(adapter.tx);
    writer.text(b" err=");
    writer.num(adapter.errors);
    writer.text(b" last=");
    writer.text(adapter.last_error.slice());
    writer.result()
}
fn xhci_note(snapshot: *mut Snapshot, pci_function: crate::platform::pci_scan::Device) -> Text {
    // record() emits only the requested row's text. Preserve that exact
    // function's note while collecting subsequent controllers, without a
    // larger fixed-stack snapshot or borrowed shared note bytes.
    if unsafe { (*snapshot).count != (*snapshot).requested_text_index } {
        return Text::EMPTY;
    }
    let host = xhci(pci_function, true);
    if !host.exists {
        return Text::new(b"xHCI host registry entry unavailable");
    }
    let out = unsafe { &mut *ptr::addr_of_mut!((*snapshot).xhci_note) };
    out.fill(0);
    let mut writer = NoteWriter::new(out);
    writer.text(b"xHCI id=");
    writer.num(u64::from(crate::usb::xhci_state::host_id_for_pci(
        pci_function,
    )));
    writer.text(b" pci=");
    writer.num(u64::from(pci_function.bus));
    writer.put(b':');
    writer.num(u64::from(pci_function.device));
    writer.put(b'.');
    writer.num(u64::from(pci_function.function));
    writer.text(b" source=");
    writer.text(host.source_label.slice());
    writer.text(b" state=");
    writer.text(match host.state {
        0 => b"registered",
        1 => b"active",
        2 => b"failed",
        _ => panic!("Invalid xHCI state"),
    });
    if !host.status_ok {
        writer.text(b"; status=unavailable");
        return writer.result();
    }
    writer.text(b"; event=");
    writer.text(if host.flags & 16 != 0 {
        b"irq-cap+poll"
    } else {
        b"poll"
    });
    writer.text(b"; q=");
    writer.num(u64::from(host.queue_depth));
    writer.text(b" max=");
    writer.num(u64::from(host.max_transfer_bytes));
    writer.text(b" active=");
    writer.num(u64::from(host.active_transfers));
    writer.text(b" done=");
    writer.num(host.completions);
    writer.text(b" irq=");
    writer.num(host.interrupts);
    writer.text(b" polls=");
    writer.num(host.polls);
    writer.text(b" cancel=");
    writer.num(host.cancellations);
    writer.result()
}
fn pci_record(snapshot: *mut Snapshot, device: crate::platform::pci_scan::Device) -> Record {
    let class = device.class_code;
    let subclass = device.subclass;
    let prog_if = device.prog_if;
    let vendor = device.vendor_id;
    let id = device.device_id;
    let name = known_name(class, subclass, prog_if, vendor, id);
    let binding = binding_for(device, class, subclass, prog_if, vendor, id);
    let driver = driver_for(binding, class, subclass, vendor, id);
    let status = status_for(device, binding, class, subclass, vendor, id);
    let note = note_for(device, snapshot, class, subclass, prog_if, vendor, id);
    Record {
        binding,
        bus: if device.bus_kind == crate::platform::pci_scan::ECAM {
            PCIE
        } else {
            PCI
        },
        name: Text::new(if name.is_empty() {
            b"unclassified-device"
        } else {
            name
        }),
        driver: Text::new(driver),
        status: Text::new(status),
        note,
        bus_no: device.bus,
        device_no: device.device,
        function_no: device.function,
        vendor_id: vendor,
        device_id: id,
        class_code: class,
        subclass,
        prog_if,
        has_pci_address: true,
    }
}
#[repr(C)]
pub struct Counts {
    pub drivers: u32,
    pub protocols: u32,
    pub displays: u32,
}
unsafe fn copy_fixed(out: *mut u8, size: usize, text: Text) {
    unsafe {
        out.write_bytes(0, size);
        let count = text.bytes.min((size - 1) as u64) as usize;
        if count != 0 {
            ptr::copy_nonoverlapping(text.pointer, out, count);
        }
    }
}
/// # Safety
/// Output is exclusive aligned Contract storage for this synchronous call.
/// Each existing device owner supplies stable metadata, with no held inventory
/// lock and no inventory reference escaping into a callback or a wait.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn r4os_rust_device_inventory_summary(
    out: *mut contract::DeviceInventorySummary,
) -> i32 {
    let mut storage = MaybeUninit::<Snapshot>::uninit();
    let snapshot = storage.as_mut_ptr();
    unsafe { Snapshot::initialize(snapshot, usize::MAX) };
    collect(snapshot);
    unsafe {
        out.write(contract::DeviceInventorySummary {
            total: (*snapshot).count as u32,
            with_driver: (*snapshot).with_driver,
            without_driver: (*snapshot).without_driver,
            unknown: (*snapshot).unknown,
            truncated: u8::from((*snapshot).truncated),
            reserved: [0; 3],
        })
    };
    1
}
/// # Safety
/// Same exclusive Contract output and synchronous metadata lifetime as summary.
/// Invalid indices preserve the previous return0 and leave output untouched.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn r4os_rust_device_inventory_record(
    index: u32,
    out: *mut contract::DeviceInventoryRecord,
) -> i32 {
    let mut storage = MaybeUninit::<Snapshot>::uninit();
    let snapshot = storage.as_mut_ptr();
    unsafe { Snapshot::initialize(snapshot, index as usize) };
    collect(snapshot);
    if index as usize >= unsafe { (*snapshot).count } {
        return 0;
    }
    let record = unsafe { ptr::addr_of!((*snapshot).selected).read() };
    unsafe {
        out.write(contract::DeviceInventoryRecord {
            binding: record.binding,
            bus: record.bus,
            flags: u16::from(record.has_pci_address),
            bus_no: record.bus_no,
            device_no: record.device_no,
            function_no: record.function_no,
            class_code: record.class_code,
            subclass: record.subclass,
            prog_if: record.prog_if,
            vendor_id: record.vendor_id,
            device_id: record.device_id,
            ..contract::DeviceInventoryRecord::default()
        });
        copy_fixed(ptr::addr_of_mut!((*out).name).cast(), 48, record.name);
        copy_fixed(ptr::addr_of_mut!((*out).driver).cast(), 32, record.driver);
        copy_fixed(ptr::addr_of_mut!((*out).status).cast(), 32, record.status);
        copy_fixed(ptr::addr_of_mut!((*out).note).cast(), 96, record.note);
    }
    1
}
/// # Safety
/// Exclusive aligned private count output; follows the identical full collection
/// and counts every selected bus without a fixed record boundary.
#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn r4os_rust_device_inventory_counts(out: *mut Counts) {
    let mut storage = MaybeUninit::<Snapshot>::uninit();
    let snapshot = storage.as_mut_ptr();
    unsafe { Snapshot::initialize(snapshot, usize::MAX) };
    collect(snapshot);
    unsafe { out.write(ptr::addr_of!((*snapshot).bus_counts).read()) };
}
const _: () = {
    assert!(core::mem::size_of::<Text>() == 16);
    assert!(core::mem::size_of::<DisplayInput>() == 32);
    assert!(core::mem::size_of::<BlockInput>() == 40);
    assert!(core::mem::size_of::<UsbInput>() == 56);
    assert!(core::mem::size_of::<HidInput>() == 24);
    assert!(core::mem::size_of::<MscInput>() == 48);
    assert!(core::mem::size_of::<DriverInput>() == 96);
    assert!(core::mem::offset_of!(DriverInput, name_len) == 40);
    assert!(core::mem::size_of::<ProtocolInput>() == 216);
    assert!(core::mem::offset_of!(ProtocolInput, role_len) == 72);
    assert!(core::mem::offset_of!(ProtocolInput, note_len) == 208);
    assert!(core::mem::size_of::<NetInput>() == 128);
    assert!(core::mem::size_of::<XhciInput>() == 72);
    assert!(core::mem::size_of::<Counts>() == 12);
    assert!(core::mem::size_of::<Snapshot>() < 16 * 1024);
};

fn binding_for(
    pci_function: crate::platform::pci_scan::Device,
    class: u8,
    subclass: u8,
    prog_if: u8,
    vendor: u16,
    device: u16,
) -> u8 {
    if class == 1 && subclass == 1 && block_count(ATA, true) > 0 {
        return WITH_DRIVER;
    }
    if class == 1 && subclass == 6 && block_count(AHCI, false) > 0 {
        return WITH_DRIVER;
    }
    if class == 1 && subclass == 8 && block_count(NVME, true) > 0 {
        return WITH_DRIVER;
    }
    if class == 1 && subclass == 8 && block_count(NVME, false) > 0 {
        return WITH_DRIVER;
    }
    if class == 12 && subclass == 3 && prog_if == 0x30 && xhci(pci_function, false).present {
        return WITH_DRIVER;
    }
    if vendor == 0x10ec && matches!(device, 0x8139 | 0x8168) && has_net(vendor, device) {
        return WITH_DRIVER;
    }
    if class == 4 && subclass == 3 {
        return WITH_DRIVER;
    }
    if class == 6 && matches!(subclass, 0 | 1) {
        return WITH_DRIVER;
    }
    if vendor == 0x8086 && device == 0x7113 {
        return WITH_DRIVER;
    }
    if class == 1 && matches!(subclass, 6 | 8) {
        return WITHOUT_DRIVER;
    }
    if class == 12 && subclass == 3 && prog_if == 0x30 {
        return WITHOUT_DRIVER;
    }
    if matches!(class, 2 | 3) {
        return WITHOUT_DRIVER;
    }
    if class == 12 && subclass == 5 {
        return WITHOUT_DRIVER;
    }
    if !known_name(class, subclass, prog_if, vendor, device).is_empty() {
        return WITHOUT_DRIVER;
    }
    UNKNOWN
}
fn status_for(
    pci_function: crate::platform::pci_scan::Device,
    binding: u8,
    class: u8,
    subclass: u8,
    vendor: u16,
    device: u16,
) -> &'static [u8] {
    if crate::usb::xhci_state::allocation_failed(pci_function) {
        return b"failed-instance-memory";
    }
    if vendor == 0x10ec && matches!(device, 0x8139 | 0x8168) && has_net(vendor, device) {
        return b"active-netbackend";
    }
    if class == 1 && subclass == 1 && block_count(ATA, true) > 1 {
        return b"active-block-multi-preload";
    }
    if class == 1 && subclass == 1 && block_count(ATA, true) > 0 {
        return b"active-block-preload";
    }
    if class == 1 && subclass == 6 {
        let preload = block_count(AHCI, true);
        let total = block_count(AHCI, false);
        if preload > 1 {
            return b"active-block-multi-preload";
        }
        if preload > 0 {
            return b"active-block-preload";
        }
        if total > 1 {
            return b"active-block-multi-rescue";
        }
        if total > 0 {
            return b"active-block-rescue";
        }
    }
    if class == 1 && subclass == 8 {
        let preload = block_count(NVME, true);
        let total = block_count(NVME, false);
        if preload > 1 {
            return b"active-block-multi-preload";
        }
        if preload > 0 {
            return b"active-block-preload";
        }
        if total > 1 {
            return b"active-block-multi";
        }
        if total > 0 {
            return b"active-block";
        }
    }
    if class == 12 && subclass == 3 {
        let host = xhci(pci_function, false);
        if host.exists && host.source == PRELOAD {
            return if xhci(pci_function, false).state == 1 {
                b"host-preload-active"
            } else {
                b"host-preload-failed"
            };
        }
        if xhci(pci_function, false).present {
            let host = xhci(pci_function, false);
            return if host.exists && host.state == 1 {
                b"host-rescue-active"
            } else {
                b"host-rescue-failed"
            };
        }
    }
    match binding {
        WITH_DRIVER => b"bound-or-platform-active",
        WITHOUT_DRIVER => b"detected-no-driver",
        UNKNOWN => b"unknown",
        _ => panic!("Invalid inventory binding"),
    }
}
fn note_for(
    pci_function: crate::platform::pci_scan::Device,
    snapshot: *mut Snapshot,
    class: u8,
    subclass: u8,
    prog_if: u8,
    vendor: u16,
    device: u16,
) -> Text {
    let text: &[u8] = if class == 1 && subclass == 1 && block_count(ATA, true) > 1 {
        b"Legacy IDE/ATA source=preload; ATAPIO.R4D blockdevices registered"
    } else if class == 1 && subclass == 1 && block_count(ATA, true) > 0 {
        b"Legacy IDE/ATA source=preload; ATAPIO.R4D blockdevice registered"
    } else if class == 1 && subclass == 6 && block_count(AHCI, true) > 1 {
        b"AHCI source=preload; AHCI.R4D IDENTIFY-backed parallel block devices active"
    } else if class == 1 && subclass == 6 && block_count(AHCI, true) > 0 {
        b"AHCI source=preload; AHCI.R4D IDENTIFY-backed block device active"
    } else if class == 1 && subclass == 6 && block_count(AHCI, false) > 0 {
        b"AHCI source=legacy-rescue; block device registered outside standard owner"
    } else if class == 1 && subclass == 6 {
        b"AHCI candidate; standard owner is AHCI.R4D preload"
    } else if class == 1 && subclass == 8 && block_count(NVME, true) > 1 {
        b"NVMe source=preload; NVME.R4D active namespaces registered as read/write block devices"
    } else if class == 1 && subclass == 8 && block_count(NVME, true) > 0 {
        b"NVMe source=preload; NVME.R4D namespace registered as read/write block device"
    } else if class == 1 && subclass == 8 && block_count(NVME, false) > 1 {
        b"NVMe source=legacy-rescue; active namespaces registered as read/write block devices"
    } else if class == 1 && subclass == 8 && block_count(NVME, false) > 0 {
        b"NVMe source=legacy-rescue; namespace registered as read/write block device"
    } else if class == 1 && subclass == 8 {
        b"NVMe candidate; standard owner is NVME.R4D preload"
    } else if class == 12 && subclass == 3 && prog_if == 0x30 {
        if crate::usb::xhci_state::allocation_failed(pci_function) {
            return Text::new(b"xHCI instance allocation failed for this PCI function; other controllers continue");
        }
        if xhci(pci_function, false).present {
            return xhci_note(snapshot, pci_function);
        }
        b"xHCI USB candidate; standard owner is XHCI.R4D preload"
    } else if vendor == 0x10ec && device == 0x8139 && has_net(vendor, device) {
        b"RTL8139 netbackend registered; IRQ-first RX/TX active with poll fallback"
    } else if vendor == 0x10ec && device == 0x8139 {
        b"RTL8139 candidate, planned first R4OS network backend"
    } else if vendor == 0x10ec && device == 0x8168 && has_net(vendor, device) {
        b"RTL8168 netbackend registered; MSI RX/TX active, warm-reboot bridge repair at bind"
    } else if vendor == 0x10ec && device == 0x8168 {
        b"RTL8168 candidate; standard owner is RTL8168.R4D"
    } else if class == 2 {
        b"network candidate, no net driver or PCI IRQ/MSI route yet"
    } else if class == 3 {
        b"bootfb active; native display driver missing"
    } else if class == 12 && subclass == 5 {
        b"chipset-visible, no SMBus driver"
    } else if vendor == 0x8086 && device == 0x7113 {
        b"PIIX ACPI bridge visible via PCI"
    } else {
        b""
    };
    Text::new(text)
}

unsafe fn inventory_scalar(operation: u8, argument: u64) -> u64 {
    use crate::{fs::drive, net::adapters, storage::block_registry as block};
    match operation {
        0 => block::r4os_rust_block_registry_query(0, 0) as u64,
        1 => block::r4os_rust_block_registry_query(1, 0) as u64,
        2 => {
            u64::from(!unsafe { drive::r4os_rust_fs_drive_lookup(1, argument as usize) }.is_null())
        }
        3 => {
            let p = unsafe { drive::r4os_rust_fs_drive_lookup(1, (argument as u8) as usize) };
            u64::from(
                !p.is_null()
                    && unsafe { (*p).has_block && (*p).block_index == (argument >> 8) as usize },
            )
        }
        4 => block::r4os_rust_block_registry_query(2, 4) as u64,
        5 => crate::driver::registry::capacity() as u64,
        6 => crate::driver::protocol_registry::MAX_PROTOCOLS as u64,
        7 => u64::from(adapters::r4os_rust_net_adapter_has_device(
            (argument >> 16) as u16,
            argument as u16,
        )),
        8 => adapters::r4os_rust_net_adapter_count() as u64,
        _ => 0,
    }
}

unsafe fn inventory_block(index: u64, out: *mut BlockInput) -> bool {
    let Some(v) = crate::storage::block_device::config(index as usize) else {
        return false;
    };
    unsafe {
        out.write(BlockInput {
            name: Text {
                pointer: v.name.data,
                bytes: v.name.len as u64,
            },
            driver: Text {
                pointer: v.driver.data,
                bytes: v.driver.len as u64,
            },
            bus: v.bus,
            source: v.source,
            port: v.port,
            removable: v.removable,
            writable: v.writable,
        });
    }
    true
}

unsafe fn inventory_display(out: *mut DisplayInput) {
    let v = crate::display::manager::stats();
    unsafe {
        out.write(DisplayInput {
            name: Text {
                pointer: v.name.pointer,
                bytes: v.name.len as u64,
            },
            policy: Text::new(crate::display::manager::cache_policy_name(
                v.mapping.cache_policy,
            )),
        });
    }
}

unsafe fn inventory_usb(index: u64, out: *mut UsbInput) -> bool {
    let mut v = MaybeUninit::uninit();
    if !unsafe { crate::usb::core::r4os_rust_usb_core_at(index as usize, v.as_mut_ptr()) } {
        return false;
    }
    let v = unsafe { v.assume_init() };
    unsafe {
        out.write(UsbInput {
            configured: v.configured,
            vendor_id: v.vendor_id,
            product_id: v.product_id,
            class: v.device_class,
            subclass: v.device_subclass,
            protocol: v.device_protocol,
            interface_class: v.first_interface_class,
            interface_subclass: v.first_interface_subclass,
            interface_protocol: v.first_interface_protocol,
            identity: crate::usb::xhci_types::DeviceHandle {
                controller_id: v.controller_id,
                generation: v.generation,
                port: v.port,
                slot_id: v.slot_id,
                ..crate::usb::xhci_types::DeviceHandle::EMPTY
            },
        });
    }
    true
}

unsafe fn inventory_hid(out: *mut HidInput) {
    let v = crate::usb::hid::status();
    unsafe {
        out.write(HidInput {
            keyboard: v.keyboard_bound,
            mouse: v.mouse_bound,
            missing: u32::from(v.protocol_required_missing),
            source: Text {
                pointer: v.boot_source.pointer,
                bytes: v.boot_source.len as u64,
            },
        });
    }
}

unsafe fn inventory_msc(out: *mut MscInput) {
    let v = crate::usb::msc::snapshot().status;
    unsafe {
        out.write(MscInput {
            registered: v.block_registered,
            write_protected: v.write_protected,
            external: crate::usb::msc::has_external_owner(),
            missing: v.protocol_required_missing,
            bot: Text {
                pointer: v.bot_source.pointer,
                bytes: v.bot_source.len as u64,
            },
            scsi: Text {
                pointer: v.scsi_source.pointer,
                bytes: v.scsi_source.len as u64,
            },
        });
    }
}

unsafe fn inventory_driver(index: u64, out: *mut DriverInput) -> bool {
    use crate::driver::registry as r;
    let Some(v) = r::get(index as usize) else {
        return false;
    };
    unsafe {
        out.write(DriverInput {
            kind: v.driver_type,
            name: v.name,
            name_len: v.name_len as u64,
            source: Text::new(r::source_name(v.source)),
            state: Text::new(r::state_name(v.state)),
            type_name: Text::new(r::type_name(v.driver_type)),
        });
    }
    true
}

unsafe fn inventory_protocol(index: u64, out: *mut ProtocolInput) -> bool {
    use crate::driver::protocol_registry as r;
    let Some(v) = r::get(index as usize) else {
        return false;
    };
    unsafe {
        out.write(ProtocolInput {
            category: v.category,
            role: v.role,
            role_len: v.role_len as u64,
            source: Text::new(r::source_name(v.source)),
            state: Text::new(r::state_name(v.state)),
            note: v.note,
            note_len: v.note_len as u64,
        });
    }
    true
}

unsafe fn inventory_net(index: u64, out: *mut NetInput) -> bool {
    use crate::net::{adapters, config};
    let Some(v) = adapters::snapshot(index as usize) else {
        return false;
    };
    let settings = config::settings();
    let source = config::source_name();
    unsafe {
        out.write(NetInput {
            name: Text {
                pointer: v.name.pointer,
                bytes: v.name.len as u64,
            },
            driver: Text {
                pointer: v.driver.pointer,
                bytes: v.driver.len as u64,
            },
            link: Text::new(match v.link {
                0 => b"unknown",
                1 => b"down",
                2 => b"up",
                _ => unreachable!(),
            }),
            last_error: Text {
                pointer: v.stats.last_error.pointer,
                bytes: v.stats.last_error.len as u64,
            },
            config_source: Text {
                pointer: source.pointer,
                bytes: source.len as u64,
            },
            rx: v.stats.rx_packets,
            tx: v.stats.tx_packets,
            errors: v.stats.errors,
            mac: v.mac,
            ip: settings.local_ip,
            gateway: settings.gateway_ip,
            bus: v.bus,
            bus_no: v.bus_no,
            device_no: v.device_no,
            function_no: v.function_no,
            vendor_id: v.vendor_id,
            device_id: v.device_id,
        });
    }
    true
}

unsafe fn inventory_xhci(
    pci_function: crate::platform::pci_scan::Device,
    with_status: bool,
    out: *mut XhciInput,
) {
    use crate::usb::{host_controller as host, types};
    let mut result = XhciInput {
        present: false,
        exists: false,
        status_ok: false,
        source: 0,
        state: 0,
        source_label: Text::EMPTY,
        flags: 0,
        queue_depth: 0,
        max_transfer_bytes: 0,
        active_transfers: 0,
        completions: 0,
        interrupts: 0,
        polls: 0,
        cancellations: 0,
    };
    let id = crate::usb::xhci_state::host_id_for_pci(pci_function);
    let index = unsafe { host::r4os_rust_usb_host_index(id) };
    if index != usize::MAX {
        let mut value = MaybeUninit::uninit();
        if unsafe { host::r4os_rust_usb_host_at(index, value.as_mut_ptr()) } {
            let value = unsafe { value.assume_init() };
            if id == 0 || value.id != id {
                unsafe { out.write(result) };
                return;
            }
            result.present = true;
            result.exists = true;
            result.source = value.source;
            result.state = value.state;
            result.source_label = Text::new(match value.source {
                0 => b"built-in",
                1 => b"preload",
                2 => b"disk",
                _ => unreachable!(),
            });
            if with_status {
                let mut status = types::HostStatus::EMPTY;
                result.status_ok = unsafe { host::status_exact(id, &mut status) } == 0;
                result.flags = status.flags;
                result.queue_depth = status.queue_depth;
                result.max_transfer_bytes = status.max_transfer_bytes;
                result.active_transfers = status.active_transfers;
                result.completions = status.completions;
                result.interrupts = status.interrupts;
                result.polls = status.polls;
                result.cancellations = status.cancellations;
            }
        }
    }
    unsafe {
        out.write(result);
    }
}
