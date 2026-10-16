//! Original boot media inventory and manifest admission. Native resident
//! records own metadata; FS volume storage stays opaque at its current owner.
//! Boot-only reset/scan precedes user sessions. No Rust record/table reference
//! crosses a block/FS/JSON callback, allocation, free or mount operation.
use crate::{
    memory::heap,
    storage::{
        block_io, block_registry,
        installation::{Manifest, TableView},
        partition_table::{self, Label, Partition, Reader, Table},
    },
};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, ptr};
pub const MAX_DEVICES: usize = 16;
pub const MAX_PARTITIONS: usize = 128;
pub const ESP: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceInfo {
    pub index: usize,
    pub bus: u8,
    pub name: Label,
    pub model: Label,
    pub driver: Label,
    pub sector_bytes: u32,
    pub sectors: u64,
    pub writable: bool,
}
#[derive(Clone, Copy)]
pub struct Volume {
    pub filesystem: u8,
    pub token: *mut c_void,
    pub letter: u8,
}
pub struct Record {
    pub used: bool,
    pub info: DeviceInfo,
    pub table: Table,
    pub partitions: [Partition; MAX_PARTITIONS],
    pub volumes: [Volume; MAX_PARTITIONS],
    pub manifest: Manifest,
    pub has_manifest: bool,
    pub installation_conflict: bool,
    pub installation_reason: Label,
}
struct Inventory(UnsafeCell<[MaybeUninit<Record>; MAX_DEVICES]>);
// Exclusive boot inventory ownership. Later topology operations retain their
// own owner/generation gate; no new runtime rescan API is introduced here.
unsafe impl Sync for Inventory {}
static RECORDS: Inventory = Inventory(UnsafeCell::new(
    [const { MaybeUninit::uninit() }; MAX_DEVICES],
));
pub(crate) unsafe fn record(index: usize) -> *mut Record {
    assert!(index < MAX_DEVICES);
    unsafe { RECORDS.0.get().cast::<Record>().add(index) }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub name: [u8; 768],
    pub name_len: usize,
    pub attr: u8,
    pub node: u64,
    pub node_generation: u16,
    pub fat_lba: u64,
    pub has_fat_lba: bool,
    pub fat_offset: u16,
    pub reparse: bool,
    pub size: u64,
    pub created_time: u16,
    pub created_date: u16,
    pub access_date: u16,
    pub modified_time: u16,
    pub modified_date: u16,
}

unsafe extern "C" fn read_sector(ctx: *mut c_void, lba: u64, out: *mut u8) -> bool {
    let index = unsafe { ctx.cast::<usize>().read() };
    unsafe { block_io::r4os_rust_block_read(index, lba, 1, out, 512, false) }
}
/// # Safety
/// Original exclusive boot-only reset/scan: no user session, mounted volume
/// token or inventory caller may survive reset. Records stay at fixed native
/// addresses and no large Record is materialized on the shared kernel stack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_media_scan() {
    unsafe {
        media_volumes_reset();
    }
    for index in 0..MAX_DEVICES {
        let p = unsafe { record(index) };
        unsafe {
            ptr::write_bytes(p, 0, 1);
            ptr::addr_of_mut!((*p).table.reason).write(Label::new(b"not-scanned"));
            (*p).table.partitions = ptr::addr_of_mut!((*p).partitions).cast();
            ptr::addr_of_mut!((*p).installation_reason).write(Label::new(b"not-found"));
        }
    }
    let count = block_registry::r4os_rust_block_registry_query(1, 0).min(MAX_DEVICES);
    for index in 0..count {
        let mut info = MaybeUninit::uninit();
        if !unsafe { media_device(index, info.as_mut_ptr()) } {
            continue;
        }
        let info = unsafe { info.assume_init() };
        if info.bus == 5 {
            continue;
        }
        let p = unsafe { record(index) };
        unsafe {
            (*p).used = true;
            ptr::addr_of_mut!((*p).info).write(info);
        }
        let mut context = index;
        let input = Reader {
            ctx: ptr::addr_of_mut!(context).cast(),
            read: read_sector,
            sector_bytes: info.sector_bytes,
            sectors: info.sectors,
        };
        unsafe {
            partition_table::r4os_rust_partition_scan(&input, ptr::addr_of_mut!((*p).table));
        }
    }
}
/// # Safety
/// Index/partition are bounded original inventory selectors in the exclusive
/// boot lifetime. Only copied metadata remains through FS probing and block
/// reads; returned C token stays at its owning FS substrate until reset.
pub(crate) unsafe fn probe(index: usize, partition: usize) -> Volume {
    assert!(partition < MAX_PARTITIONS);
    let p = unsafe { record(index) };
    let v = unsafe {
        ptr::addr_of_mut!((*p).volumes)
            .cast::<Volume>()
            .add(partition)
    };
    let current = unsafe { v.read() };
    if current.filesystem != 0 {
        return current;
    }
    unsafe {
        (*v).filesystem = 5;
    }
    if unsafe { !(*p).table.valid || partition >= (*p).table.count } {
        return unsafe { v.read() };
    }
    let part = unsafe {
        ptr::addr_of!((*p).partitions)
            .cast::<Partition>()
            .add(partition)
            .read()
    };
    if part.type_guid == partition_table::BIOS {
        unsafe {
            (*v).filesystem = 1;
        }
        return unsafe { v.read() };
    }
    let info = unsafe { (*p).info };
    if info.sector_bytes != 512 || part.first_lba > u64::from(u32::MAX) {
        unsafe {
            (*v).filesystem = 6;
        }
        return unsafe { v.read() };
    }
    let mut boot = MaybeUninit::<[u8; 512]>::uninit();
    if !unsafe {
        block_io::r4os_rust_block_read(
            info.index,
            part.first_lba,
            1,
            boot.as_mut_ptr().cast(),
            512,
            false,
        )
    } {
        return unsafe { v.read() };
    }
    let boot = unsafe { boot.assume_init() };
    let kind = if &boot[3..11] == b"NTFS    " {
        4
    } else if &boot[82..90] == b"FAT32   " {
        3
    } else {
        2
    };
    if kind == 2 {
        unsafe {
            (*v).filesystem = 2;
        }
        return unsafe { v.read() };
    }
    let token = unsafe {
        media_probe(
            info.index,
            partition,
            kind,
            part.first_lba as u32,
            part.sector_count,
        )
    };
    if !token.is_null() {
        unsafe {
            (*v).token = token;
            (*v).filesystem = kind;
        }
    }
    unsafe { v.read() }
}
struct Buffer {
    data: *mut u8,
    len: usize,
}
impl Buffer {
    fn allocate(len: usize) -> Option<Self> {
        let data = heap::r4os_rust_heap_alloc(len, 8);
        if data.is_null() {
            None
        } else {
            Some(Self { data, len })
        }
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            heap::r4os_rust_heap_free(self.data, self.len);
        }
    }
}
/// # Safety
/// The exact record/table/FS tokens remain stable during original boot-only
/// manifest scanning. Heap buffers retain original lengths/alignment and are
/// freed only after their synchronous FS/JSON calls return.
pub(crate) unsafe fn read_installation(index: usize) {
    let p = unsafe { record(index) };
    if unsafe { !(*p).table.valid || (*p).table.kind != 2 } {
        return;
    }
    let count = unsafe { (*p).table.count };
    for i in 0..count {
        let part = unsafe {
            ptr::addr_of!((*p).partitions)
                .cast::<Partition>()
                .add(i)
                .read()
        };
        if part.type_guid != ESP {
            continue;
        }
        let volume = unsafe { probe(index, i) };
        if volume.token.is_null() {
            continue;
        }
        let path = b"/boot/r4os-installation.json";
        let mut entry = MaybeUninit::uninit();
        if !unsafe { media_entry(volume.token, path.as_ptr(), path.len(), entry.as_mut_ptr()) } {
            continue;
        }
        let entry = unsafe { entry.assume_init() };
        unsafe {
            (*p).installation_reason = Label::new(b"invalid-manifest");
        }
        if entry.attr & 0x10 != 0 || entry.size == 0 || entry.size > 16384 {
            continue;
        }
        let Some(bytes) = Buffer::allocate(entry.size as usize) else {
            unsafe {
                (*p).installation_reason = Label::new(b"out-of-memory");
            }
            continue;
        };
        if unsafe { media_read(volume.token, &entry, 0, bytes.data, bytes.len) } != bytes.len {
            continue;
        }
        let Some(scratch) = Buffer::allocate(128 * 1024) else {
            continue;
        };
        let mut manifest = MaybeUninit::uninit();
        let mut error = MaybeUninit::uninit();
        if !unsafe {
            media_manifest(
                bytes.data,
                bytes.len,
                scratch.data,
                scratch.len,
                manifest.as_mut_ptr(),
                error.as_mut_ptr(),
            )
        } {
            unsafe {
                (*p).installation_reason = error.assume_init();
            }
            continue;
        }
        let manifest = unsafe { manifest.assume_init() };
        let table = unsafe {
            TableView {
                valid: (*p).table.valid,
                kind: (*p).table.kind,
                disk_guid: (*p).table.disk_guid,
                partitions: ptr::addr_of!((*p).partitions).cast(),
                count: (*p).table.count,
            }
        };
        if !unsafe { manifest.matches(table, part.unique_guid) } {
            unsafe {
                (*p).installation_reason = Label::new(b"gpt-mismatch");
            }
            continue;
        }
        unsafe {
            if (*p).has_manifest {
                (*p).installation_conflict = true;
            }
            (*p).has_manifest = true;
            ptr::addr_of_mut!((*p).manifest).write(manifest);
            (*p).installation_reason = Label::new(b"ok");
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<DeviceInfo>() == 88);
    assert!(core::mem::size_of::<Entry>() == 840);
};

use crate::fs::{vfs, vfs_types as fs_types};
struct VolumeSlots([UnsafeCell<Option<fs_types::Volume>>; MAX_DEVICES * MAX_PARTITIONS]);
// SAFETY: exclusive boot scan precedes users; the stationary exact tokens
// survive every synchronous probe/consumer/mount and are reset only at boot.
unsafe impl Sync for VolumeSlots {}
static VOLUMES: VolumeSlots =
    VolumeSlots([const { UnsafeCell::new(None) }; MAX_DEVICES * MAX_PARTITIONS]);
pub(crate) unsafe fn volume(token: *mut c_void) -> fs_types::Volume {
    unsafe {
        token
            .cast::<Option<fs_types::Volume>>()
            .read()
            .expect("Live exact volume token")
    }
}
fn entry_value(v: fs_types::Entry) -> Entry {
    Entry {
        name: v.name,
        name_len: v.name_len,
        attr: v.attr,
        node: v.node,
        node_generation: v.generation,
        fat_lba: v.directory_lba,
        has_fat_lba: v.has_lba,
        fat_offset: v.directory_offset,
        reparse: v.reparse,
        size: v.size,
        created_time: v.times[0],
        created_date: v.times[1],
        access_date: v.times[2],
        modified_time: v.times[3],
        modified_date: v.times[4],
    }
}
fn vfs_entry(v: Entry) -> fs_types::Entry {
    fs_types::Entry {
        name: v.name,
        name_len: v.name_len,
        attr: v.attr,
        node: v.node,
        generation: v.node_generation,
        directory_lba: v.fat_lba,
        has_lba: v.has_fat_lba,
        directory_offset: v.fat_offset,
        reparse: v.reparse,
        size: v.size,
        times: [
            v.created_time,
            v.created_date,
            v.access_date,
            v.modified_time,
            v.modified_date,
        ],
    }
}

unsafe fn media_volumes_reset() {
    for slot in &VOLUMES.0 {
        unsafe {
            slot.get().write(None);
        }
    }
}

unsafe fn media_probe(
    index: usize,
    partition: usize,
    kind: u8,
    lba: u32,
    sectors: u64,
) -> *mut c_void {
    assert!(index < MAX_DEVICES && partition < MAX_PARTITIONS);
    assert!(matches!(kind, 3 | 4));
    let Some(v) =
        (unsafe { crate::fs::vfs_backends::probe(kind == 4, index, lba, sectors, false) })
    else {
        return ptr::null_mut();
    };
    let slot = VOLUMES.0[index * MAX_PARTITIONS + partition].get();
    unsafe {
        slot.write(Some(v));
    }
    slot.cast()
}

unsafe fn media_device(index: usize, out: *mut DeviceInfo) -> bool {
    let _guard = crate::memory::owner::Guard::acquire(1);
    let mut view = MaybeUninit::uninit();
    if !unsafe {
        crate::storage::block_registry::r4os_rust_block_registry_slot(
            index,
            1,
            0,
            true,
            view.as_mut_ptr(),
        )
    } {
        return false;
    }
    let view = unsafe { view.assume_init() };
    let c = unsafe {
        core::ptr::addr_of!((*view.device.cast::<super::block_device::Device>()).config).read()
    };
    unsafe {
        out.write(DeviceInfo {
            index,
            bus: c.bus,
            name: Label {
                data: c.name.data,
                len: c.name.len,
            },
            model: Label {
                data: c.model.data,
                len: c.model.len,
            },
            driver: Label {
                data: c.driver.data,
                len: c.driver.len,
            },
            sector_bytes: c.sector_size,
            sectors: c.sector_count,
            writable: c.writable,
        });
    }
    true
}

unsafe fn media_entry(volume: *mut c_void, path: *const u8, len: usize, out: *mut Entry) -> bool {
    unsafe {
        let v = crate::storage::media_inventory::volume(volume);
        let a = fs_types::Args {
            volume: v,
            name: fs_types::Bytes { data: path, len },
            ..Default::default()
        };
        let mut e = MaybeUninit::<fs_types::Entry>::uninit();
        let result = vfs::r4os_rust_vfs_run(1, &a, e.as_mut_ptr().cast());
        if result.status != 0 {
            return false;
        }
        out.write(entry_value(e.assume_init()));
        true
    }
}

unsafe fn media_read(
    volume: *mut c_void,
    entry: *const Entry,
    offset: usize,
    out: *mut u8,
    len: usize,
) -> usize {
    unsafe {
        let v = crate::storage::media_inventory::volume(volume);
        let e = vfs_entry(entry.read());
        let a = fs_types::Args {
            volume: v,
            entry: &e,
            offset: offset as u64,
            buffer: out,
            buffer_len: len,
            ..Default::default()
        };
        let result = vfs::r4os_rust_vfs_run(6, &a, ptr::null_mut());
        if result.present {
            result.value.try_into().unwrap()
        } else {
            usize::MAX
        }
    }
}

unsafe fn media_manifest(
    bytes: *const u8,
    len: usize,
    scratch: *mut u8,
    scratch_len: usize,
    out: *mut Manifest,
    error: *mut Label,
) -> bool {
    unsafe {
        let input = core::slice::from_raw_parts(bytes, len);
        let scratch = core::slice::from_raw_parts_mut(scratch, scratch_len);
        match crate::storage::installation::parse_bytes(input, scratch) {
            Ok(m) => {
                out.write(m);
                true
            }
            Err(e) => {
                error.write(Label::new(e));
                false
            }
        }
    }
}
