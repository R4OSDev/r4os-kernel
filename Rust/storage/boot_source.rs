//! Loaded executable's exact installation identity and original permission policy.
use crate::storage::{
    installation::{Manifest, TableView},
    partition_table::Label,
};
use core::{ffi::c_void, mem::MaybeUninit, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Identity {
    pub present: bool,
    pub generic_media: bool,
    pub path: Label,
    pub path_truncated: bool,
    pub disk_guid: [u8; 16],
    pub partition_guid: [u8; 16],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceView {
    pub index: usize,
    pub usb: bool,
    pub local: bool,
    pub table: TableView,
    pub installation: *const Manifest,
    pub installation_conflict: bool,
}
type Getter = unsafe extern "C" fn(*const c_void, usize, *mut DeviceView);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Source {
    pub confirmed: bool,
    pub reason: Label,
    pub device_index: usize,
    pub usb: bool,
    pub slot: u8,
    pub installation_id: [u8; 16],
    pub disk_guid: [u8; 16],
    pub boot_guid: [u8; 16],
    pub recovery_guid: [u8; 16],
    pub system_guid: [u8; 16],
    pub data_guid: [u8; 16],
}
impl Source {
    const EMPTY: Self = Self {
        confirmed: false,
        reason: Label::new(b"missing-identity"),
        device_index: 0,
        usb: false,
        slot: 0,
        installation_id: [0; 16],
        disk_guid: [0; 16],
        boot_guid: [0; 16],
        recovery_guid: [0; 16],
        system_guid: [0; 16],
        data_guid: [0; 16],
    };
}
unsafe fn get(ctx: *const c_void, index: usize, read: Getter) -> DeviceView {
    let mut output = MaybeUninit::uninit();
    unsafe {
        read(ctx, index, output.as_mut_ptr());
        output.assume_init()
    }
}
unsafe fn resolve(
    identity: Identity,
    ctx: *const c_void,
    len: usize,
    read: Getter,
    normal: bool,
) -> Source {
    let mut result = Source::EMPTY;
    if !identity.present
        || identity.path_truncated
        || !identity.generic_media
        || identity.disk_guid == [0; 16]
        || identity.partition_guid == [0; 16]
    {
        return result;
    }
    result.reason = Label::new(b"unknown-loaded-path");
    let path = if identity.path.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(identity.path.data, identity.path.len) }
    };
    if normal {
        if !path.eq_ignore_ascii_case(b"/boot/r4os.elf")
            && !path.eq_ignore_ascii_case(b"/boot/r4os-prev.elf")
        {
            return result;
        }
    } else if path.eq_ignore_ascii_case(b"/CURRENT/recovery.elf") {
        result.slot = 1;
    } else if path.eq_ignore_ascii_case(b"/PREVIOUS/recovery.elf") {
        result.slot = 2;
    } else {
        return result;
    }
    result.reason = Label::new(b"source-not-found");
    let mut selected = None;
    let mut disk_matches = 0;
    for i in 0..len {
        let device = unsafe { get(ctx, i, read) };
        if device.table.disk_guid != identity.disk_guid {
            continue;
        }
        disk_matches += 1;
        if device.table.valid && (device.usb || device.local) {
            selected = Some(device);
        }
    }
    if disk_matches > 1 {
        result.reason = Label::new(b"duplicate-disk-guid");
        return result;
    }
    let Some(device) = selected else {
        return result;
    };
    if device.installation.is_null() {
        result.reason = Label::new(b"installation-unavailable");
        return result;
    }
    if device.installation_conflict {
        result.reason = Label::new(b"installation-conflict");
        return result;
    }
    let manifest = unsafe { &*device.installation };
    if !unsafe { manifest.matches(device.table, manifest.partitions[1].partition_guid) } {
        result.reason = Label::new(b"installation-table-mismatch");
        return result;
    }
    if manifest.partitions[if normal { 1 } else { 3 }].partition_guid != identity.partition_guid {
        result.reason = Label::new(b"loaded-partition-mismatch");
        return result;
    }
    for i in 0..len {
        let other = unsafe { get(ctx, i, read) };
        if other.index == device.index {
            continue;
        }
        if !other.installation.is_null()
            && unsafe { (*other.installation).installation_id } == manifest.installation_id
        {
            result.reason = Label::new(b"duplicate-installation-id");
            return result;
        }
        let parts = if other.table.count == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(other.table.partitions, other.table.count) }
        };
        for p in parts {
            if p.unique_guid == identity.partition_guid {
                result.reason = Label::new(b"duplicate-source-partition-guid");
                return result;
            }
        }
    }
    result.confirmed = true;
    result.reason = Label::new(b"ok");
    result.device_index = device.index;
    result.usb = device.usb;
    result.installation_id = manifest.installation_id;
    result.disk_guid = manifest.disk_guid;
    result.boot_guid = manifest.partitions[1].partition_guid;
    result.recovery_guid = manifest.partitions[3].partition_guid;
    result.system_guid = manifest.partitions[2].partition_guid;
    result.data_guid = manifest.partitions[4].partition_guid;
    result
}
/// # Safety
/// Identity path and exactly len immutable device views/manifest/partition
/// values stay readable through this non-waiting call. Getter copies one fixed
/// C view, never mutates or performs I/O; no reference escapes. Output owns one
/// exclusive aligned C Source. Native owner alone chooses identity/permissions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_source(
    identity: *const Identity,
    ctx: *const c_void,
    len: usize,
    getter: Getter,
    normal: bool,
    out: *mut Source,
) {
    unsafe { out.write(resolve(identity.read(), ctx, len, getter, normal)) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_source_permit(
    op: u8,
    confirmed: bool,
    usb: bool,
    own: usize,
    candidate: usize,
) -> bool {
    match op {
        0 => confirmed && !(usb && own == candidate),
        1 => confirmed,
        2 => confirmed && usb,
        _ => unreachable!(),
    }
}
const _: () = {
    assert!(core::mem::size_of::<Identity>() == 64);
    assert!(core::mem::size_of::<DeviceView>() == 72);
    assert!(core::mem::size_of::<Source>() == 136);
    assert!(core::mem::offset_of!(Source, installation_id) == 34);
};
