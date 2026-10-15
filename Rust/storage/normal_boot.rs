//! Original normal installation mount policy. Current FS/drive owners receive
//! only fixed metadata and exact opaque volume tokens; no Rust record borrow
//! survives probing, mount publication or generic log callbacks.
use crate::storage::{
    boot_source, guid,
    installation::TableView,
    media_inventory as media,
    partition_table::{Label, Partition},
};
use core::{ffi::c_void, mem::MaybeUninit, ptr, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootIdentity {
    pub loaded: boot_source::Identity,
    pub mbr_disk_id: u32,
    pub partition_index: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResultRecord {
    pub kind: u8,
    pub index: usize,
    pub reason: Label,
}
#[repr(C)]
pub struct DriveRequest {
    pub letter: u8,
    pub kind: u8,
    pub role: u8,
    pub bytes: usize,
    pub index: usize,
    pub name: Label,
    pub volume: *mut c_void,
}
unsafe extern "C" {
    fn r4os_zig_normal_identity(out: *mut BootIdentity);
    fn r4os_zig_normal_drive(operation: u8, input: *const DriveRequest) -> bool;
    fn r4os_zig_normal_log(text: *const u8, len: usize);
    fn r4os_zig_normal_decimal(value: u64);
}
fn put(text: &[u8]) {
    unsafe {
        r4os_zig_normal_log(text.as_ptr(), text.len());
    }
}
fn raw(text: Label) {
    unsafe {
        r4os_zig_normal_log(text.data, text.len);
    }
}
fn decimal(value: u64) {
    unsafe {
        r4os_zig_normal_decimal(value);
    }
}
fn print_guid(value: [u8; 16]) {
    let mut out = [0; 36];
    unsafe {
        guid::r4os_rust_storage_guid_format(value.as_ptr(), out.as_mut_ptr());
    }
    put(&out);
}
fn rejected(reason: &'static [u8]) -> ResultRecord {
    ResultRecord {
        kind: 2,
        index: 0,
        reason: Label::new(reason),
    }
}
fn drive(operation: u8, letter: u8) -> bool {
    let input = DriveRequest {
        letter,
        kind: 0,
        role: 0,
        bytes: 0,
        index: 0,
        name: Label::new(b""),
        volume: ptr::null_mut(),
    };
    unsafe { r4os_zig_normal_drive(operation, &input) }
}
unsafe extern "C" fn getter(raw: *const c_void, index: usize, out: *mut boot_source::DeviceView) {
    unsafe {
        out.write(raw.cast::<boot_source::DeviceView>().add(index).read());
    }
}
unsafe fn find(index: usize, id: [u8; 16]) -> Option<usize> {
    let p = unsafe { media::record(index) };
    let count = unsafe { (*p).table.count };
    for i in 0..count {
        let part = unsafe {
            ptr::addr_of!((*p).partitions)
                .cast::<Partition>()
                .add(i)
                .read()
        };
        if part.unique_guid == id {
            return Some(i);
        }
    }
    None
}
fn free_letter() -> Option<u8> {
    (b'E'..=b'Z').find(|&letter| !drive(0, letter))
}
unsafe fn mount_letter(index: usize, partition: usize, letter: u8, role: u8) -> bool {
    if drive(0, letter) {
        return false;
    }
    let volume = unsafe { media::probe(index, partition) };
    if volume.token.is_null() {
        return false;
    }
    let p = unsafe { media::record(index) };
    let part = unsafe {
        ptr::addr_of!((*p).partitions)
            .cast::<Partition>()
            .add(partition)
            .read()
    };
    if part.sector_count > usize::MAX as u64 / 512 {
        return false;
    }
    let info = unsafe { (*p).info };
    let kind = match volume.filesystem {
        3 => 2,
        4 => 3,
        _ => unreachable!(),
    };
    let input = DriveRequest {
        letter,
        kind,
        role,
        bytes: (part.sector_count * 512) as usize,
        index: info.index,
        name: info.name,
        volume: volume.token,
    };
    if !unsafe { r4os_zig_normal_drive(1, &input) } {
        return false;
    }
    if !unsafe { r4os_zig_normal_drive(2, &input) } {
        unsafe {
            r4os_zig_normal_drive(3, &input);
        }
        return false;
    }
    unsafe {
        (*ptr::addr_of_mut!((*p).volumes)
            .cast::<media::Volume>()
            .add(partition))
        .letter = letter;
    }
    true
}
/// # Safety
/// Exclusive original early boot mount admission. Boot info and inventory stay
/// resident; FS tokens keep their owner's lifetime. Output is one exclusive C
/// tagged result; the public Zig Result facade keeps its original vocabulary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_normal_mount(out: *mut ResultRecord) {
    unsafe {
        out.write(mount());
    }
}
unsafe fn mount() -> ResultRecord {
    unsafe {
        media::r4os_rust_media_scan();
    }
    let mut views: [MaybeUninit<boot_source::DeviceView>; 16] =
        [const { MaybeUninit::uninit() }; 16];
    let mut count = 0;
    for index in 0..media::MAX_DEVICES {
        let p = unsafe { media::record(index) };
        if !unsafe { (*p).used } {
            continue;
        }
        unsafe {
            media::read_installation(index);
        }
        let info = unsafe { (*p).info };
        let table = unsafe {
            TableView {
                valid: (*p).table.valid,
                kind: (*p).table.kind,
                disk_guid: (*p).table.disk_guid,
                partitions: ptr::addr_of!((*p).partitions).cast(),
                count: (*p).table.count,
            }
        };
        views[count].write(boot_source::DeviceView {
            index: info.index,
            usb: info.bus == 4,
            local: matches!(info.bus, 1 | 2 | 3 | 6),
            table,
            installation: if unsafe { (*p).has_manifest } {
                unsafe { ptr::addr_of!((*p).manifest) }
            } else {
                ptr::null()
            },
            installation_conflict: unsafe { (*p).installation_conflict },
        });
        count += 1;
    }
    let mut identity = MaybeUninit::uninit();
    unsafe {
        r4os_zig_normal_identity(identity.as_mut_ptr());
    }
    let identity = unsafe { identity.assume_init() };
    put(b"[INSTALLBOOT] executable=");
    raw(identity.loaded.path);
    put(b" disk=");
    print_guid(identity.loaded.disk_guid);
    put(b" partition=");
    print_guid(identity.loaded.partition_guid);
    put(b" mbr-id=");
    decimal(u64::from(identity.mbr_disk_id));
    put(b" partition-index=");
    decimal(u64::from(identity.partition_index));
    put(b"\r\n");
    let path = if identity.loaded.path.len == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(identity.loaded.path.data, identity.loaded.path.len) }
    };
    if identity.loaded.present
        && !identity.loaded.path_truncated
        && identity.loaded.generic_media
        && identity.loaded.disk_guid == [0; 16]
        && identity.loaded.partition_guid == [0; 16]
        && identity.mbr_disk_id != 0
        && (path.eq_ignore_ascii_case(b"/boot/r4os.elf")
            || path.eq_ignore_ascii_case(b"/boot/r4os-prev.elf"))
    {
        let mut selected = None;
        for index in 0..media::MAX_DEVICES {
            let p = unsafe { media::record(index) };
            if !unsafe { (*p).used } || unsafe { (*p).table.mbr_disk_id } != identity.mbr_disk_id {
                continue;
            }
            if selected.is_some() {
                return rejected(b"Duplicate legacy boot disk signature");
            }
            if unsafe { !(*p).table.valid || (*p).table.kind != 1 } {
                return rejected(b"Invalid legacy boot table");
            }
            selected = Some(unsafe { (*p).info.index });
        }
        if let Some(index) = selected {
            put(b"[INSTALLBOOT] source=legacy-mbr device=");
            decimal(index as u64);
            put(b"\r\n");
            return ResultRecord {
                kind: 1,
                index,
                reason: Label::new(b""),
            };
        }
        return rejected(b"Legacy boot disk unavailable");
    }
    let mut source = MaybeUninit::uninit();
    unsafe {
        boot_source::r4os_rust_boot_source(
            &identity.loaded,
            views.as_ptr().cast(),
            count,
            getter,
            true,
            source.as_mut_ptr(),
        );
    }
    let source = unsafe { source.assume_init() };
    put(b"[INSTALLBOOT] source=");
    raw(source.reason);
    put(b"\r\n");
    if !source.confirmed {
        return ResultRecord {
            kind: 2,
            index: 0,
            reason: source.reason,
        };
    }
    let Some(boot) = (unsafe { find(source.device_index, source.boot_guid) }) else {
        return rejected(b"BOOT role missing");
    };
    let Some(system) = (unsafe { find(source.device_index, source.system_guid) }) else {
        return rejected(b"SYSTEM role missing");
    };
    let Some(data) = (unsafe { find(source.device_index, source.data_guid) }) else {
        return rejected(b"DATA role missing");
    };
    let boot_volume = unsafe { media::probe(source.device_index, boot) };
    let system_volume = unsafe { media::probe(source.device_index, system) };
    let data_volume = unsafe { media::probe(source.device_index, data) };
    if boot_volume.filesystem != 3 || system_volume.filesystem != 4 || data_volume.filesystem != 4 {
        return rejected(b"Installation filesystem unavailable (BOOT/SYSTEM/DATA)");
    }
    assert!(!boot_volume.token.is_null());
    let input = DriveRequest {
        letter: 0,
        kind: 0,
        role: 0,
        bytes: 0,
        index: 0,
        name: Label::new(b""),
        volume: boot_volume.token,
    };
    if !unsafe { r4os_zig_normal_drive(5, &input) }
        || !unsafe { mount_letter(source.device_index, system, b'C', 1) }
        || !unsafe { mount_letter(source.device_index, data, b'D', 2) }
    {
        return rejected(b"Installation mount rejected");
    }
    drive(4, b'C');
    put(b"[INSTALLBOOT] installation=");
    print_guid(source.installation_id);
    put(b" C=");
    print_guid(source.system_guid);
    put(b" BOOT=");
    print_guid(source.boot_guid);
    put(b" D=");
    print_guid(source.data_guid);
    put(b"\r\n");
    for index in 0..media::MAX_DEVICES {
        let p = unsafe { media::record(index) };
        if unsafe { !(*p).used || !(*p).table.valid } {
            continue;
        }
        let part_count = unsafe { (*p).table.count };
        let device_index = unsafe { (*p).info.index };
        for i in 0..part_count {
            if device_index == source.device_index && (i == boot || i == system || i == data) {
                continue;
            }
            let Some(letter) = free_letter() else {
                break;
            };
            unsafe {
                mount_letter(index, i, letter, 0);
            }
        }
    }
    put(b"[INSTALLBOOT] mapping=verified C=SYSTEM D=DATA BOOT=unlettered\r\n");
    ResultRecord {
        kind: 0,
        index: 0,
        reason: Label::new(b""),
    }
}
const _: () = {
    assert!(core::mem::size_of::<BootIdentity>() == 72);
    assert!(core::mem::size_of::<ResultRecord>() == 32);
    assert!(core::mem::size_of::<DriveRequest>() == 48);
};
