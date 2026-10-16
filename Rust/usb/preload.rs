//! Original BSP USB/storage preload coordinator. R4I entries borrow immutable
//! Limine module bytes; loader calls receive copied identities, never a borrow
//! into the mutable entry catalogue.
use crate::{
    boot::{foundation, info},
    fs::vfs_types::Bytes,
    loader::{r4d, r4p},
};
use core::{cell::UnsafeCell, slice};
const MAX_ENTRIES: usize = 32;
#[derive(Clone, Copy)]
struct Entry {
    name: &'static [u8],
    _role: &'static [u8],
    _kind: u16,
    bytes: &'static [u8],
}
impl Entry {
    const EMPTY: Self = Self {
        name: b"",
        _role: b"",
        _kind: 0,
        bytes: b"",
    };
}
#[derive(Clone, Copy)]
struct View {
    count: usize,
    r4p_count: usize,
    r4d_count: usize,
}
struct Storage(UnsafeCell<[Entry; MAX_ENTRIES]>);
// SAFETY: only the linear BSP preload coordinator mutates this resident array.
unsafe impl Sync for Storage {}
static ENTRIES: Storage = Storage(UnsafeCell::new([Entry::EMPTY; MAX_ENTRIES]));
struct Required {
    name: &'static [u8],
    marker: &'static [u8],
    driver: bool,
}
const REQUIRED: [Required; 9] = [
    Required {
        name: b"HIDREPORT.R4P",
        marker: b"r4os.preload.usb-r4p=HIDREPORT",
        driver: false,
    },
    Required {
        name: b"USBHID.R4P",
        marker: b"r4os.preload.usb-r4p=USBHID",
        driver: false,
    },
    Required {
        name: b"USBBOT.R4P",
        marker: b"r4os.preload.usb-r4p=USBBOT",
        driver: false,
    },
    Required {
        name: b"USBSCSI.R4P",
        marker: b"r4os.preload.usb-r4p=USBSCSI",
        driver: false,
    },
    Required {
        name: b"XHCI.R4D",
        marker: b"r4os.preload.usb-r4d=XHCI",
        driver: true,
    },
    Required {
        name: b"USBMSC.R4D",
        marker: b"r4os.preload.usb-r4d=USBMSC",
        driver: true,
    },
    Required {
        name: b"AHCI.R4D",
        marker: b"r4os.preload.storage-r4d=AHCI",
        driver: true,
    },
    Required {
        name: b"NVME.R4D",
        marker: b"r4os.preload.storage-r4d=NVME",
        driver: true,
    },
    Required {
        name: b"ATAPIO.R4D",
        marker: b"r4os.preload.storage-r4d=ATAPIO",
        driver: true,
    },
];
fn span(bytes: &[u8]) -> Bytes {
    Bytes {
        data: bytes.as_ptr(),
        len: bytes.len(),
    }
}
fn le16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes[..2].try_into().expect("R4I u16"))
}
fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().expect("R4I u32"))
}
unsafe fn parse(bytes: &'static [u8]) -> Option<View> {
    if bytes.len() < 64
        || &bytes[..4] != b"R4I0"
        || le16(&bytes[4..6]) != 1
        || le16(&bytes[6..8]) != 64
    {
        return None;
    }
    let count = le32(&bytes[8..12]) as usize;
    let entries_off = le32(&bytes[12..16]) as usize;
    let data_off = le32(&bytes[16..20]) as usize;
    let total = le32(&bytes[20..24]) as usize;
    if count > MAX_ENTRIES || total > bytes.len() || data_off > total {
        return None;
    }
    let table_size = count * 128;
    if entries_off + table_size > data_off {
        return None;
    }
    let mut view = View {
        count,
        r4p_count: 0,
        r4d_count: 0,
    };
    for index in 0..count {
        let off = entries_off + index * 128;
        let record = &bytes[off..off + 128];
        let kind = le16(&record[..2]);
        let name_len = usize::from(le16(&record[4..6]));
        let role_len = usize::from(le16(&record[6..8]));
        let payload_off = le32(&record[88..92]) as usize;
        let payload_len = le32(&record[92..96]) as usize;
        if name_len == 0
            || name_len > 32
            || role_len > 48
            || payload_off < data_off
            || payload_off + payload_len > total
        {
            return None;
        }
        let entry = Entry {
            name: &record[8..8 + name_len],
            _role: &record[40..40 + role_len],
            _kind: kind,
            bytes: &bytes[payload_off..payload_off + payload_len],
        };
        unsafe { ENTRIES.0.get().cast::<Entry>().add(index).write(entry) };
        match kind {
            2 => view.r4p_count += 1,
            3 => view.r4d_count += 1,
            _ => return None,
        }
    }
    Some(view)
}
unsafe fn modules() -> &'static [info::BootModule] {
    let snapshot = unsafe { &*info::r4os_rust_boot_info() };
    if snapshot.module_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(snapshot.modules, snapshot.module_count) }
    }
}
unsafe fn find_module(marker: &[u8]) -> Option<&'static [u8]> {
    for module in unsafe { modules() } {
        if !module.valid {
            continue;
        }
        let command = if module.cmdline_len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(module.cmdline, module.cmdline_len) }
        };
        if command == marker {
            return Some(if module.size == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(module.address, module.size) }
            });
        }
    }
    None
}
unsafe fn find_image() -> Option<View> {
    let bytes = unsafe { find_module(b"r4os.preload.image=PRELOAD.R4I") }?;
    let image = unsafe { parse(bytes) };
    if image.is_none() {
        foundation::puts(b"[PRELOAD] invalid PRELOAD.R4I\r\n");
    }
    image
}
unsafe fn find_entry(view: Option<View>, name: &[u8]) -> Option<&'static [u8]> {
    let view = view?;
    for index in 0..view.count {
        let entry = unsafe { ENTRIES.0.get().cast::<Entry>().add(index).read() };
        if entry.name == name {
            return Some(entry.bytes);
        }
    }
    None
}
/// # Safety
/// Linear BSP bootstrap after BootInfo initialization; boot modules remain
/// immutable and resident through every synchronous R4D/R4P loader callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_preload_init() -> bool {
    let image = unsafe { find_image() };
    let (mut loaded, mut missing, mut failed) = (0u64, 0u64, 0u64);
    for req in &REQUIRED {
        let Some(bytes) =
            unsafe { find_entry(image, req.name) }.or_else(|| unsafe { find_module(req.marker) })
        else {
            missing += 1;
            foundation::puts(b"[USBPRELOAD] missing ");
            foundation::puts(req.name);
            foundation::puts(b"\r\n");
            continue;
        };
        let ok = if req.driver {
            r4d::r4os_rust_r4d_succeeded(unsafe {
                r4d::r4os_rust_r4d_preload(span(req.name), span(bytes))
            })
        } else {
            unsafe { r4p::r4os_rust_r4p_preload(span(req.name), span(bytes)) }
        };
        if ok {
            loaded += 1;
        } else {
            failed += 1;
            foundation::puts(b"[USBPRELOAD] failed ");
            foundation::puts(req.name);
            foundation::puts(b"\r\n");
        }
    }
    r4p::r4os_rust_r4p_finish_preload();
    if let Some(view) = image {
        foundation::puts(b"[PRELOAD] R4I entries=");
        foundation::dec(view.count as u64);
        foundation::puts(b" r4p=");
        foundation::dec(view.r4p_count as u64);
        foundation::puts(b" r4d=");
        foundation::dec(view.r4d_count as u64);
        foundation::puts(b"\r\n");
    }
    foundation::puts(b"[USBPRELOAD] status loaded=");
    foundation::dec(loaded);
    foundation::puts(b"/");
    foundation::dec(REQUIRED.len() as u64);
    foundation::puts(b" missing=");
    foundation::dec(missing);
    foundation::puts(b" failed=");
    foundation::dec(failed);
    foundation::puts(b"\r\n");
    true
}
