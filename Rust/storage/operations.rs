//! Original kernel storage inventory/claim/durability/remount orchestration.
//! Fixed resident state is serialized by the existing unwind-aware sleepable
//! gate. No Rust reference to shared records or buffers spans an owner callback
//! or a physical I/O wait. Partition/format policy stays in external tools.
use crate::{
    contract as api,
    sched::{
        scheduler, sync_locks,
        sync_types::{Text, UnwindGuard, WaitQueue},
        task_context,
    },
    storage::{
        access_runtime as access,
        access_state::{Claim, Input, Mount, Owner, Reference, Region},
        block_io,
        block_registry::{self, Identity},
        normal_boot::DriveRequest,
        partition_table::{self as table, Label, Partition, Reader, Table},
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{self, MaybeUninit},
    ptr, slice,
};
type Result<T> = core::result::Result<T, i32>;
struct Record {
    ready: bool,
    identity: MaybeUninit<Identity>,
    generation: u64,
    layout: Table,
    partitions: [Partition; 128],
    hints: [u32; 128],
    errors: [i32; 128],
    last_error: i32,
}
#[derive(Clone, Copy)]
struct SavedMount {
    slot: u32,
    reference: Reference,
    region: Region,
    partition: Partition,
    disk_guid: [u8; 16],
    mbr_disk_id: u32,
    _kind: u8,
    role: u8,
    _bytes: usize,
    _name: [u8; 32],
}
struct Operation {
    id: u64,
    owner: Owner,
    region: Region,
    saved: [MaybeUninit<SavedMount>; 27],
    saved_count: usize,
    cleanup: bool,
}
struct State {
    gate: UnwindGuard,
    records: [MaybeUninit<Record>; 16],
    operations: [MaybeUninit<Operation>; 16],
    scratch: MaybeUninit<Record>,
    initialized: bool,
    cleanup_started: bool,
    recovery_cache_device: Option<Reference>,
}
struct Storage(UnsafeCell<State>);
// Metadata only under the original topology gate; recovery reservation retains
// its original exclusive boot boundary. Blocking leaves receive raw exact IDs.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    gate: UnwindGuard {
        owner: 0,
        owner_generation: 0,
        boot_owner: false,
        depth: 0,
        name: Text::literal(b"storage-topology"),
        queue: WaitQueue::EMPTY,
    },
    records: [const { MaybeUninit::uninit() }; 16],
    operations: [const { MaybeUninit::uninit() }; 16],
    scratch: MaybeUninit::uninit(),
    initialized: false,
    cleanup_started: false,
    recovery_cache_device: None,
}));
fn state() -> *mut State {
    STATE.0.get()
}
fn record(index: usize) -> *mut Record {
    unsafe {
        ptr::addr_of_mut!((*state()).records)
            .cast::<Record>()
            .add(index)
    }
}
fn operation(index: usize) -> *mut Operation {
    unsafe {
        ptr::addr_of_mut!((*state()).operations)
            .cast::<Operation>()
            .add(index)
    }
}
fn gate() -> *mut UnwindGuard {
    unsafe { ptr::addr_of_mut!((*state()).gate) }
}
#[repr(C)]
struct DriveMetadata {
    present: bool,
    has_vfs: bool,
    kind: u8,
    role: u8,
    bytes: usize,
    index: usize,
    name: [u8; 32],
}
#[repr(C)]
struct VolumeMetadata {
    filesystem: u32,
    role: u32,
}
#[repr(C)]
struct Probe {
    index: usize,
    lba: u32,
    sectors: u64,
    filesystem: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Args {
    pub generation: u64,
    pub id: u64,
    pub relative: u64,
    pub slot: u32,
    pub flags: u32,
    pub sectors: u32,
    pub len: u32,
    pub target: *const api::StorageTarget,
    pub device: *const api::StorageDeviceRef,
    pub data: *mut u8,
    pub program: u32,
    pub program_generation: u64,
    pub task: u32,
    pub task_generation: u64,
}

unsafe fn reset_record(r: *mut Record) {
    unsafe {
        ptr::write_bytes(r, 0, 1);
        (*r).layout.reason = Label::new(b"not-scanned");
        (*r).layout.partitions = ptr::addr_of_mut!((*r).partitions).cast();
    }
}
fn initialize() {
    unsafe {
        if (*state()).initialized {
            return;
        }
        for i in 0..16 {
            reset_record(record(i));
            ptr::write_bytes(operation(i), 0, 1)
        }
        reset_record(ptr::addr_of_mut!((*state()).scratch).cast());
        (*state()).initialized = true;
    }
}
struct Gate;
impl Gate {
    fn enter() -> Result<Self> {
        if unsafe { sync_locks::r4os_rust_sync_guard(gate(), 2, 2000) } {
            initialize();
            Ok(Self)
        } else {
            Err(api::storage_error_busy)
        }
    }
    fn try_enter() -> Option<Self> {
        if unsafe { sync_locks::r4os_rust_sync_guard(gate(), 1, 0) } {
            initialize();
            Some(Self)
        } else {
            None
        }
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        unsafe {
            sync_locks::r4os_rust_sync_guard(gate(), 3, 0);
        }
    }
}
struct Unwind(task_context::Unwind);
impl Unwind {
    fn enter() -> Result<Self> {
        let token = unsafe { task_context::enter() };
        if token.admitted() {
            Ok(Self(token))
        } else {
            Err(api::storage_error_capacity)
        }
    }
}
impl Drop for Unwind {
    fn drop(&mut self) {
        unsafe {
            task_context::leave(self.0);
        }
    }
}
fn access(op: u8, input: Input, snapshot: *mut c_void) -> Result<u64> {
    let mut value = 0;
    let status = unsafe {
        access::r4os_rust_storage_access_runtime(op, false, &input, &mut value, snapshot)
    };
    if status == 0 {
        Ok(value)
    } else {
        assert!((1..=6).contains(&status));
        Err(-status)
    }
}
fn scalar(op: u8, input: Input) -> Result<u64> {
    access(op, input, ptr::null_mut())
}
fn owner() -> Result<Owner> {
    let mut out = MaybeUninit::uninit();
    if unsafe { access::r4os_rust_storage_access_current(out.as_mut_ptr()) } {
        Ok(unsafe { out.assume_init() })
    } else {
        Err(api::storage_error_invalid)
    }
}
fn identity(index: usize) -> Option<Identity> {
    let mut out = MaybeUninit::uninit();
    if unsafe { block_registry::r4os_rust_block_registry_identity(index, out.as_mut_ptr()) } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
fn generation() -> u64 {
    scalar(19, Input::default()).expect("Storage topology scalar")
}
fn claimed(region: Region) -> bool {
    scalar(
        24,
        Input {
            region,
            ..Input::default()
        },
    )
    .expect("Storage claim scalar")
        != 0
}
fn mount_reference(slot: u32) -> Option<Reference> {
    scalar(
        21,
        Input {
            reference: Reference {
                slot,
                generation: 0,
            },
            ..Input::default()
        },
    )
    .ok()
    .map(|generation| Reference { slot, generation })
}
fn mount_snapshot(reference: Reference) -> Result<Mount> {
    let mut out = MaybeUninit::<Mount>::uninit();
    access(
        8,
        Input {
            reference,
            ..Input::default()
        },
        out.as_mut_ptr().cast(),
    )?;
    Ok(unsafe { out.assume_init() })
}
fn claim_snapshot(id: u64, owner: Owner) -> Result<Claim> {
    let mut out = MaybeUninit::<Claim>::uninit();
    access(
        14,
        Input {
            id,
            owner,
            ..Input::default()
        },
        out.as_mut_ptr().cast(),
    )?;
    Ok(unsafe { out.assume_init() })
}
struct Use {
    id: u64,
    owner: Owner,
}
impl Drop for Use {
    fn drop(&mut self) {
        let _ = scalar(
            12,
            Input {
                id: self.id,
                owner: self.owner,
                ..Input::default()
            },
        );
    }
}
fn raw_use(region: Region) -> Result<Use> {
    let owner = owner()?;
    let id = scalar(
        11,
        Input {
            region,
            owner,
            ..Input::default()
        },
    )?;
    Ok(Use { id, owner })
}
fn claim_use(id: u64, region: Region) -> Result<Use> {
    let owner = owner()?;
    let id = scalar(
        16,
        Input {
            id,
            region,
            owner,
            ..Input::default()
        },
    )?;
    Ok(Use { id, owner })
}
fn drive(letter: u8) -> DriveMetadata {
    let mut out = MaybeUninit::uninit();
    unsafe {
        ops_drive(letter, out.as_mut_ptr());
        out.assume_init()
    }
}
fn public_device(reference: Reference) -> api::StorageDeviceRef {
    api::StorageDeviceRef {
        slot: reference.slot,
        reserved: 0,
        generation: reference.generation,
    }
}
fn private_device(reference: api::StorageDeviceRef) -> Result<Reference> {
    if reference.reserved != 0 || reference.generation == 0 || reference.slot >= 16 {
        return Err(api::storage_error_invalid);
    }
    let found = identity(reference.slot as usize).ok_or(api::storage_error_stale)?;
    if found.reference.generation != reference.generation {
        return Err(api::storage_error_stale);
    }
    Ok(found.reference)
}
fn all(id: &Identity) -> Region {
    Region {
        device: id.reference,
        first: 0,
        count: id.sectors,
    }
}
unsafe fn id(r: *mut Record) -> Identity {
    unsafe { ptr::addr_of!((*r).identity).cast::<Identity>().read() }
}
unsafe fn part(r: *mut Record, index: usize) -> Partition {
    unsafe {
        ptr::addr_of!((*r).partitions)
            .cast::<Partition>()
            .add(index)
            .read()
    }
}
fn text(label: Label) -> &'static [u8] {
    unsafe { slice::from_raw_parts(label.data, label.len) }
}
unsafe fn incomplete(r: *mut Record) -> bool {
    let reason = text(unsafe { (*r).layout.reason });
    reason == b"TooManyPartitions" || reason == b"UnsupportedEntryLayout"
}
unsafe fn unsupported(r: *mut Record) -> bool {
    let id = unsafe { id(r) };
    id.sector_bytes != 512 || id.sectors == 0 || id.sectors > u64::MAX / 512
}
unsafe extern "C" fn read_sector(raw: *mut c_void, lba: u64, out: *mut u8) -> bool {
    unsafe { block_io::r4os_rust_block_read(raw.cast::<usize>().read(), lba, 1, out, 512, false) }
}
unsafe fn scan_record(r: *mut Record, identity: Identity, own: Option<Region>) -> Result<()> {
    unsafe {
        let reference = identity.reference;
        let sector_bytes = identity.sector_bytes;
        let scratch = ptr::addr_of_mut!((*state()).scratch).cast::<Record>();
        let mut slot = identity.reference.slot as usize;
        let input = Reader {
            ctx: ptr::addr_of_mut!(slot).cast(),
            read: read_sector,
            sector_bytes: identity.sector_bytes,
            sectors: identity.sectors,
        };
        table::r4os_rust_partition_scan(&input, ptr::addr_of_mut!((*scratch).layout));
        scalar(0, Input::default())?;
        (*r).ready = true;
        (*r).identity.write(identity);
        (*r).generation = generation();
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*scratch).partitions).cast::<Partition>(),
            ptr::addr_of_mut!((*r).partitions).cast::<Partition>(),
            128,
        );
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*scratch).layout),
            ptr::addr_of_mut!((*r).layout),
            1,
        );
        (*r).layout.partitions = ptr::addr_of_mut!((*r).partitions).cast();
        for i in 0..(*r).layout.count {
            let p = part(r, i);
            let region = Region {
                device: reference,
                first: p.first_lba,
                count: p.sector_count,
            };
            if claimed(region) && own.is_none_or(|owner| !owner.contains(region)) {
                continue;
            }
            (*r).hints[i] = api::storage_filesystem_unknown;
            if !(*r).layout.valid || sector_bytes != 512 {
                continue;
            }
            if p.type_guid == table::BIOS {
                (*r).hints[i] = api::storage_filesystem_none;
                continue;
            }
            let mut sector = [0; 512];
            if !block_io::r4os_rust_block_read(
                slot,
                p.first_lba,
                1,
                sector.as_mut_ptr(),
                512,
                false,
            ) {
                (*r).errors[i] = api::storage_error_io;
                continue;
            }
            if &sector[3..11] == b"NTFS    " {
                (*r).hints[i] = api::storage_filesystem_ntfs
            }
            if &sector[82..90] == b"FAT32   " {
                (*r).hints[i] = api::storage_filesystem_fat32
            }
        }
        Ok(())
    }
}
fn ensure_records() -> Result<()> {
    for i in 0..16 {
        let r = record(i);
        let Some(identity) = identity(i) else {
            unsafe { (*r).ready = false };
            continue;
        };
        if unsafe { (*r).ready && id(r).reference == identity.reference } {
            continue;
        }
        let _use = raw_use(all(&identity))?;
        unsafe {
            reset_record(r);
            scan_record(r, identity, None)?
        }
    }
    Ok(())
}
unsafe fn whole_target(r: *mut Record) -> api::StorageTarget {
    let id = unsafe { id(r) };
    api::StorageTarget {
        device: public_device(id.reference),
        layout_generation: unsafe { (*r).generation },
        sector_count: id.sectors,
        kind: api::storage_target_device,
        ..Default::default()
    }
}
unsafe fn partition_target(r: *mut Record, p: Partition) -> api::StorageTarget {
    api::StorageTarget {
        device: public_device(unsafe { id(r) }.reference),
        layout_generation: unsafe { (*r).generation },
        first_lba: p.first_lba,
        sector_count: p.sector_count,
        partition_number: p.number,
        kind: api::storage_target_partition,
        partition_guid: p.unique_guid,
        ..Default::default()
    }
}
unsafe fn target_for_region(r: *mut Record, region: Region) -> api::StorageTarget {
    unsafe {
        for i in 0..(*r).layout.count {
            let p = part(r, i);
            if p.first_lba == region.first && p.sector_count >= region.count {
                return partition_target(r, p);
            }
        }
        whole_target(r)
    }
}
fn validate(target: api::StorageTarget, mutation: bool) -> Result<Region> {
    if target.version != 1 || target.size as usize != mem::size_of::<api::StorageTarget>() {
        return Err(api::storage_error_invalid);
    }
    let reference = private_device(target.device)?;
    let r = record(reference.slot as usize);
    unsafe {
        if !(*r).ready
            || id(r).reference != reference
            || (*r).generation != target.layout_generation
        {
            return Err(api::storage_error_stale);
        }
        if unsupported(r) {
            return Err(api::storage_error_unsupported);
        }
        if mutation && incomplete(r) {
            return Err(api::storage_error_incomplete);
        }
        let id = id(r);
        if mutation && !id.writable {
            return Err(api::storage_error_protected);
        }
        if target.kind == api::storage_target_device {
            if target.first_lba != 0
                || target.sector_count != id.sectors
                || target.partition_number != 0
                || target.partition_guid != [0; 16]
            {
                return Err(api::storage_error_invalid);
            }
            return Ok(all(&id));
        }
        if target.kind != api::storage_target_partition {
            return Err(api::storage_error_invalid);
        }
        if !(*r).layout.valid {
            return Err(api::storage_error_incomplete);
        }
        for i in 0..(*r).layout.count {
            let p = part(r, i);
            if p.number != target.partition_number {
                continue;
            }
            if p.first_lba != target.first_lba
                || p.sector_count != target.sector_count
                || p.unique_guid != target.partition_guid
            {
                return Err(api::storage_error_stale);
            }
            return Ok(Region {
                device: reference,
                first: p.first_lba,
                count: p.sector_count,
            });
        }
    }
    Err(api::storage_error_stale)
}
fn copy_text(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    let len = (out.len() - 1).min(value.len());
    out[..len].copy_from_slice(&value[..len]);
}
fn name(value: &[u8]) -> &[u8] {
    &value[..value.iter().position(|b| *b == 0).unwrap_or(value.len())]
}
fn reserve_recovery(index: usize) -> bool {
    unsafe {
        if (*state()).recovery_cache_device.is_some() {
            return false;
        }
        let runtime = drive(b'C');
        if !runtime.present {
            return false;
        }
        let cache = drive(b'R');
        if !cache.present || runtime.role != 3 || cache.index != index || cache.kind != 2 {
            return false;
        }
        let Some(identity) = identity(index) else {
            return false;
        };
        (*state()).recovery_cache_device = Some(identity.reference);
        true
    }
}
unsafe fn inventory(out: *mut api::StorageInventory) -> Result<i32> {
    unsafe {
        if (*out).version != 1 || (*out).size as usize != mem::size_of::<api::StorageInventory>() {
            return Err(api::storage_error_invalid);
        }
        let _gate = Gate::enter()?;
        ensure_records()?;
        if generation() == u64::MAX {
            return Err(api::storage_error_capacity);
        }
        let mut flags = 0;
        for i in 0..16 {
            let r = record(i);
            if (*r).ready && incomplete(r) {
                flags |= api::storage_inventory_partial
            }
        }
        out.write(api::StorageInventory {
            generation: generation(),
            device_slots: block_registry::r4os_rust_block_registry_query(1, 0) as u32,
            volume_slots: 27,
            flags,
            ..Default::default()
        });
        Ok(0)
    }
}
unsafe fn device_info(
    generation_arg: u64,
    slot: u32,
    out: *mut api::StorageDeviceInfo,
) -> Result<i32> {
    unsafe {
        if (*out).version != 1
            || (*out).size as usize != mem::size_of::<api::StorageDeviceInfo>()
            || slot >= 16
        {
            return Err(api::storage_error_invalid);
        }
        let _gate = Gate::enter()?;
        if generation_arg != generation() {
            return Err(api::storage_error_stale);
        }
        let Some(identity) = identity(slot as usize) else {
            return Ok(api::storage_result_absent);
        };
        let r = record(slot as usize);
        if !(*r).ready || id(r).reference != identity.reference {
            return Err(api::storage_error_stale);
        }
        let mut flags = if identity.writable {
            api::storage_device_writable
        } else {
            0
        };
        if (*r).layout.valid {
            flags |= api::storage_device_table_valid
        }
        if (*r).layout.kind == 2 {
            flags |= api::storage_device_gpt
        }
        if (*r).layout.kind == 1 {
            flags |= api::storage_device_mbr
        }
        if identity.bus == 5 {
            flags |= api::storage_device_ram
        }
        if claimed(all(&identity)) {
            flags |= api::storage_device_claimed
        }
        if (*r).last_error != 0 {
            flags |= api::storage_device_failed
        }
        if unsupported(r) {
            flags |= api::storage_device_unsupported
        }
        if incomplete(r) {
            flags |= api::storage_device_partial
        }
        let mut value = api::StorageDeviceInfo {
            reference: public_device(identity.reference),
            layout_generation: (*r).generation,
            sector_count: identity.sectors,
            disk_guid: (*r).layout.disk_guid,
            first_usable: (*r).layout.first_usable,
            last_usable: (*r).layout.last_usable,
            bus: u32::from(identity.bus),
            flags,
            sector_bytes: identity.sector_bytes,
            partition_slots: (*r).layout.count as u32,
            last_error: (*r).last_error,
            model: identity.model,
            name: identity.name,
            driver: identity.driver,
            ..Default::default()
        };
        copy_text(&mut value.reason, text((*r).layout.reason));
        out.write(value);
        Ok(api::storage_result_present)
    }
}
unsafe fn partition_info(
    generation_arg: u64,
    device: *const api::StorageDeviceRef,
    slot: u32,
    out: *mut api::StoragePartitionInfo,
) -> Result<i32> {
    unsafe {
        if (*out).version != 1
            || (*out).size as usize != mem::size_of::<api::StoragePartitionInfo>()
        {
            return Err(api::storage_error_invalid);
        }
        let _gate = Gate::enter()?;
        if generation_arg != generation() {
            return Err(api::storage_error_stale);
        }
        let reference = private_device(device.read())?;
        let r = record(reference.slot as usize);
        if !(*r).ready || id(r).reference != reference {
            return Err(api::storage_error_stale);
        }
        if slot as usize >= (*r).layout.count {
            return Ok(api::storage_result_absent);
        }
        let p = part(r, slot as usize);
        let region = Region {
            device: reference,
            first: p.first_lba,
            count: p.sector_count,
        };
        let mut flags = if claimed(region) {
            api::storage_partition_claimed
        } else {
            0
        };
        for i in 0..27 {
            let Some(reference) = mount_reference(i) else {
                continue;
            };
            let Ok(mounted) = mount_snapshot(reference) else {
                continue;
            };
            if mounted.region.overlaps(region) {
                flags |= api::storage_partition_mounted
            }
        }
        if (*r).errors[slot as usize] != 0 {
            flags |= api::storage_partition_failed
        }
        out.write(api::StoragePartitionInfo {
            target: partition_target(r, p),
            type_guid: p.type_guid,
            attributes: p.attributes,
            filesystem: (*r).hints[slot as usize],
            flags,
            last_error: (*r).errors[slot as usize],
            mbr_type: u32::from(p.mbr_type),
            name: p.name,
            ..Default::default()
        });
        Ok(api::storage_result_present)
    }
}
unsafe fn volume_info(
    generation_arg: u64,
    slot: u32,
    out: *mut api::StorageVolumeInfo,
) -> Result<i32> {
    unsafe {
        if (*out).version != 1
            || (*out).size as usize != mem::size_of::<api::StorageVolumeInfo>()
            || slot >= 27
        {
            return Err(api::storage_error_invalid);
        }
        let _gate = Gate::enter()?;
        if generation_arg != generation() {
            return Err(api::storage_error_stale);
        }
        let Some(reference) = mount_reference(slot) else {
            return Ok(api::storage_result_absent);
        };
        let mounted = mount_snapshot(reference)?;
        let r = record(mounted.region.device.slot as usize);
        if !(*r).ready {
            return Err(api::storage_error_stale);
        }
        let mut metadata = MaybeUninit::uninit();
        if !ops_volume(slot, metadata.as_mut_ptr()) {
            return Err(api::storage_error_stale);
        }
        let metadata = metadata.assume_init();
        out.write(api::StorageVolumeInfo {
            reference: api::StorageVolumeRef {
                slot,
                generation: reference.generation,
                ..Default::default()
            },
            target: target_for_region(r, mounted.region),
            letter: if slot == 26 {
                0
            } else {
                u32::from(b'A') + slot
            },
            filesystem: metadata.filesystem,
            role: metadata.role,
            flags: if mounted.runtime_required {
                api::storage_volume_required
            } else {
                0
            } | if claimed(mounted.region) {
                api::storage_volume_claimed
            } else {
                0
            },
            ..Default::default()
        });
        Ok(api::storage_result_present)
    }
}

unsafe fn saved(op: *mut Operation, index: usize) -> SavedMount {
    unsafe {
        ptr::addr_of!((*op).saved)
            .cast::<SavedMount>()
            .add(index)
            .read()
    }
}
unsafe fn remember_mounts(op: *mut Operation) -> Result<()> {
    unsafe {
        let region = (*op).region;
        let r = record(region.device.slot as usize);
        for slot in 0..27 {
            let Some(reference) = mount_reference(slot) else {
                continue;
            };
            let mounted = mount_snapshot(reference)?;
            if !mounted.region.overlaps(region) {
                continue;
            }
            if !region.contains(mounted.region) {
                return Err(api::storage_error_invalid);
            }
            if mounted.runtime_required || slot == 26 {
                return Err(api::storage_error_protected);
            }
            let d = drive(b'A' + slot as u8);
            if !d.present {
                return Err(api::storage_error_stale);
            }
            let mut found = None;
            for i in 0..(*r).layout.count {
                let p = part(r, i);
                if p.first_lba == mounted.region.first && p.sector_count >= mounted.region.count {
                    found = Some(p);
                    break;
                }
            }
            let p = found.ok_or(api::storage_error_incomplete)?;
            let value = SavedMount {
                slot,
                reference,
                region: mounted.region,
                partition: p,
                disk_guid: (*r).layout.disk_guid,
                mbr_disk_id: (*r).layout.mbr_disk_id,
                _kind: d.kind,
                role: d.role,
                _bytes: d.bytes,
                _name: d.name,
            };
            ptr::addr_of_mut!((*op).saved)
                .cast::<SavedMount>()
                .add((*op).saved_count)
                .write(value);
            (*op).saved_count += 1;
        }
        Ok(())
    }
}
fn forget(region: Region) {
    unsafe {
        ops_forget(0, region.device.slot as usize, region.first, region.count);
        ops_forget(1, region.device.slot as usize, region.first, region.count);
    }
}
fn cache(region: Region) -> bool {
    unsafe { ops_cache(region.device.slot as usize, region.first, region.count) }
}
struct Preparing {
    op: *mut Operation,
    accepted: bool,
}
impl Drop for Preparing {
    fn drop(&mut self) {
        if !self.accepted {
            unsafe {
                let id = (*self.op).id;
                let owner = (*self.op).owner;
                let _ = scalar(
                    17,
                    Input {
                        id,
                        owner,
                        ..Input::default()
                    },
                );
                let _ = scalar(
                    18,
                    Input {
                        id,
                        owner,
                        ..Input::default()
                    },
                );
                (*self.op).id = 0;
            }
        }
    }
}
struct Releasing {
    op: *mut Operation,
}
impl Drop for Releasing {
    fn drop(&mut self) {
        unsafe {
            let _ = scalar(
                18,
                Input {
                    id: (*self.op).id,
                    owner: (*self.op).owner,
                    ..Input::default()
                },
            );
            (*self.op).id = 0;
        }
    }
}
struct Ending {
    op: *mut Operation,
}
impl Drop for Ending {
    fn drop(&mut self) {
        unsafe {
            let id = (*self.op).id;
            let owner = (*self.op).owner;
            let _ = scalar(
                17,
                Input {
                    id,
                    owner,
                    ..Input::default()
                },
            );
            let _ = scalar(
                18,
                Input {
                    id,
                    owner,
                    ..Input::default()
                },
            );
            (*self.op).id = 0;
        }
    }
}
unsafe fn prepare(target: api::StorageTarget) -> Result<*mut Operation> {
    unsafe {
        let region = validate(target, true)?;
        let owner = owner()?;
        let mut found = ptr::null_mut();
        for i in 0..16 {
            let candidate = operation(i);
            if (*candidate).id == 0 {
                found = candidate;
                break;
            }
        }
        if found.is_null() {
            return Err(api::storage_error_capacity);
        }
        let op = found;
        (*op).id = 0;
        (*op).owner = owner;
        (*op).region = region;
        (*op).saved_count = 0;
        (*op).cleanup = false;
        remember_mounts(op)?;
        let id = scalar(
            13,
            Input {
                region,
                owner,
                ..Input::default()
            },
        )?;
        (*op).id = id;
        let mut preparation = Preparing {
            op,
            accepted: false,
        };
        if !cache(region) {
            (*record(region.device.slot as usize)).last_error = api::storage_error_io;
            return Err(api::storage_error_io);
        }
        for detached in 0..(*op).saved_count {
            let value = saved(op, detached);
            if !ops_unmount(value.reference) {
                for i in 0..detached {
                    let _ = restore(saved(op, i));
                }
                return Err(api::storage_error_busy);
            }
        }
        forget(region);
        scalar(
            15,
            Input {
                id,
                owner,
                ..Input::default()
            },
        )?;
        preparation.accepted = true;
        Ok(op)
    }
}
fn find_operation(id: u64, owner: Owner) -> Result<*mut Operation> {
    for i in 0..16 {
        let op = operation(i);
        unsafe {
            if (*op).id == id && id != 0 {
                if (*op).owner != owner {
                    return Err(api::storage_error_owner);
                }
                return Ok(op);
            }
        }
    }
    Err(api::storage_error_stale)
}
struct Publishing {
    record: *mut Record,
    index: usize,
    letter: u8,
    role: u8,
    name: Label,
    reference: MaybeUninit<Reference>,
}
unsafe extern "C" fn publish_volume(context: *mut c_void, volume: *mut c_void) -> i32 {
    unsafe {
        let p = context.cast::<Publishing>();
        let r = (*p).record;
        let part = part(r, (*p).index);
        let id = id(r);
        let request = DriveRequest {
            letter: (*p).letter,
            kind: match (*r).hints[(*p).index] {
                api::storage_filesystem_fat32 => 2,
                api::storage_filesystem_ntfs => 3,
                _ => unreachable!(),
            },
            role: (*p).role,
            bytes: (part.sector_count * 512) as usize,
            index: id.reference.slot as usize,
            name: (*p).name,
            volume,
        };
        if !crate::storage::normal_boot::drive_dispatch(1, &request) {
            return api::storage_error_capacity;
        }
        if !crate::storage::normal_boot::drive_dispatch(2, &request) {
            crate::storage::normal_boot::drive_dispatch(3, &request);
            return api::storage_error_capacity;
        }
        (*r).errors[(*p).index] = 0;
        assert!(
            ops_mount_reference((*p).letter, (*p).reference.as_mut_ptr()),
            "Published mount lost exact access reference"
        );
        0
    }
}
unsafe fn publish_mount(r: *mut Record, index: usize, letter: u8, role: u8) -> Result<Reference> {
    unsafe {
        let d = drive(letter);
        if d.present || d.has_vfs {
            return Err(api::storage_error_busy);
        }
        let part = part(r, index);
        if part.first_lba > u64::from(u32::MAX) {
            return Err(api::storage_error_unsupported);
        }
        let filesystem = (*r).hints[index];
        if filesystem != api::storage_filesystem_fat32 && filesystem != api::storage_filesystem_ntfs
        {
            return Err(api::storage_error_unsupported);
        }
        let id = id(r);
        let input = Probe {
            index: id.reference.slot as usize,
            lba: part.first_lba as u32,
            sectors: part.sector_count,
            filesystem,
        };
        let mut context = Publishing {
            record: r,
            index,
            letter,
            role,
            // drive.mountBlockRole retains this exact slice after return.
            // Keep it on the resident record, never on this continuation stack.
            name: Label {
                data: ptr::addr_of!((*ptr::addr_of!((*r).identity).cast::<Identity>()).name).cast(),
                len: name(&id.name).len(),
            },
            reference: MaybeUninit::uninit(),
        };
        let code = ops_probe(&input, ptr::addr_of_mut!(context).cast(), publish_volume);
        if code != 0 {
            return Err(code);
        }
        Ok(context.reference.assume_init())
    }
}
unsafe fn restore(saved: SavedMount) -> Result<()> {
    unsafe {
        let r = record(saved.region.device.slot as usize);
        if !(*r).layout.valid {
            return Err(api::storage_error_remount);
        }
        if saved.disk_guid != (*r).layout.disk_guid || saved.mbr_disk_id != (*r).layout.mbr_disk_id
        {
            return Ok(());
        }
        for i in 0..(*r).layout.count {
            let p = part(r, i);
            if p.number != saved.partition.number
                || p.first_lba != saved.partition.first_lba
                || p.sector_count != saved.partition.sector_count
                || p.unique_guid != saved.partition.unique_guid
                || p.type_guid != saved.partition.type_guid
                || p.mbr_type != saved.partition.mbr_type
            {
                continue;
            }
            if let Err(error) = publish_mount(r, i, b'A' + saved.slot as u8, saved.role) {
                (*r).errors[i] = error;
                return Err(error);
            }
            return Ok(());
        }
        Ok(())
    }
}
unsafe fn finish(op: *mut Operation, keep_unmounted: bool) -> Result<()> {
    unsafe {
        scalar(
            17,
            Input {
                id: (*op).id,
                owner: (*op).owner,
                ..Input::default()
            },
        )?;
        let _release = Releasing { op };
        let region = (*op).region;
        let r = record(region.device.slot as usize);
        if !cache(region) {
            (*r).last_error = api::storage_error_io;
            return Err(api::storage_error_io);
        }
        let Some(identity) = identity(region.device.slot as usize) else {
            (*r).last_error = api::storage_error_io;
            return Err(api::storage_error_io);
        };
        if let Err(error) = scan_record(r, identity, Some(region)) {
            (*r).last_error = error;
            return Err(api::storage_error_remount);
        }
        let mut failed = false;
        if !keep_unmounted {
            for i in 0..(*op).saved_count {
                if restore(saved(op, i)).is_err() {
                    failed = true
                }
            }
        }
        (*r).last_error = if failed {
            api::storage_error_remount
        } else {
            0
        };
        if failed {
            return Err(api::storage_error_remount);
        }
        Ok(())
    }
}
unsafe fn claim_begin(target: *const api::StorageTarget, out: *mut u64) -> Result<i32> {
    unsafe {
        out.write(0);
        let _gate = Gate::enter()?;
        if !(*state()).cleanup_started {
            if !ops_create_cleanup(cleanup_main) {
                return Err(api::storage_error_capacity);
            }
            (*state()).cleanup_started = true
        }
        let op = prepare(target.read())?;
        out.write((*op).id);
        Ok(0)
    }
}
fn claim_end(id: u64, flags: u32) -> Result<i32> {
    if flags & !api::storage_claim_end_keep_unmounted != 0 {
        return Err(api::storage_error_invalid);
    }
    let _gate = Gate::enter()?;
    let owner = owner()?;
    let op = find_operation(id, owner)?;
    unsafe { finish(op, flags != 0)? };
    Ok(0)
}
fn raw_region(region: Region, relative: u64, sectors: u32, len: u32) -> Result<Region> {
    if sectors == 0
        || sectors > api::storage_raw_max_sectors
        || len < sectors * 512
        || relative >= region.count
        || u64::from(sectors) > region.count - relative
    {
        return Err(api::storage_error_invalid);
    }
    Ok(Region {
        device: region.device,
        first: region.first + relative,
        count: u64::from(sectors),
    })
}
unsafe fn read(
    target: *const api::StorageTarget,
    relative: u64,
    sectors: u32,
    out: *mut u8,
    len: u32,
) -> Result<i32> {
    let _unwind = Unwind::enter()?;
    let gate = Gate::enter()?;
    let scope = validate(unsafe { target.read() }, false)?;
    let region = raw_region(scope, relative, sectors, len)?;
    let _use = raw_use(region)?;
    drop(gate);
    Ok(
        if unsafe {
            block_io::r4os_rust_block_read(
                region.device.slot as usize,
                region.first,
                sectors as u16,
                out,
                (sectors * 512) as usize,
                false,
            )
        } {
            0
        } else {
            api::storage_error_io
        },
    )
}
unsafe fn claim_raw(
    id: u64,
    relative: u64,
    sectors: u32,
    data: *mut u8,
    len: u32,
    write: bool,
) -> Result<i32> {
    let _unwind = Unwind::enter()?;
    let owner = owner()?;
    let claim = claim_snapshot(id, owner)?;
    let region = raw_region(claim.region, relative, sectors, len)?;
    let _use = claim_use(id, region)?;
    let ok = if write {
        unsafe {
            let mut result = block_io::Transfer::default();
            block_io::r4os_rust_block_write(
                region.device.slot as usize,
                region.first,
                sectors as u16,
                data,
                (sectors * 512) as usize,
                false,
                &mut result,
            );
            result.error == 0 && u32::from(result.sectors_completed) == sectors
        }
    } else {
        unsafe {
            block_io::r4os_rust_block_read(
                region.device.slot as usize,
                region.first,
                sectors as u16,
                data,
                (sectors * 512) as usize,
                false,
            )
        }
    };
    Ok(if ok { 0 } else { api::storage_error_io })
}
fn claim_flush(id: u64) -> Result<i32> {
    let _unwind = Unwind::enter()?;
    let owner = owner()?;
    let claim = claim_snapshot(id, owner)?;
    let _use = claim_use(id, claim.region)?;
    Ok(
        if block_io::r4os_rust_block_flush(claim.region.device.slot as usize) {
            0
        } else {
            api::storage_error_io
        },
    )
}
fn rescan(device: *const api::StorageDeviceRef) -> Result<i32> {
    let _gate = Gate::enter()?;
    let reference = private_device(unsafe { device.read() })?;
    let r = record(reference.slot as usize);
    unsafe {
        if !(*r).ready {
            return Err(api::storage_error_stale);
        }
        let target = whole_target(r);
        let op = prepare(target)?;
        finish(op, false)?;
    }
    Ok(0)
}
/// Discovery of one newly registered removable USB incarnation. Reuse the
/// existing runtime gate, exact device lease, decoder and mount publisher;
/// never reset the boot inventory or rescan unrelated devices.
pub(crate) fn attach_removable(reference: Reference) -> Result<usize> {
    let _gate = Gate::enter()?;
    let identity = identity(reference.slot as usize).ok_or(api::storage_error_stale)?;
    if identity.reference != reference || identity.bus != 4 {
        return Err(api::storage_error_stale);
    }
    let _use = raw_use(all(&identity))?;
    let r = record(reference.slot as usize);
    unsafe {
        if !(*r).ready || id(r).reference != reference {
            reset_record(r);
            scan_record(r, identity, None)?;
        }
        if !(*r).layout.valid || incomplete(r) || unsupported(r) {
            return Err(api::storage_error_unsupported);
        }
        let mut mounted = 0;
        for i in 0..(*r).layout.count {
            if !matches!(
                (*r).hints[i],
                api::storage_filesystem_fat32 | api::storage_filesystem_ntfs
            ) {
                continue;
            }
            let partition = part(r, i);
            let region = Region {
                device: reference,
                first: partition.first_lba,
                count: partition.sector_count,
            };
            if (0..27)
                .filter_map(mount_reference)
                .filter_map(|v| mount_snapshot(v).ok())
                .any(|v| v.region.overlaps(region))
            {
                continue;
            }
            let letter = (b'D'..=b'Z')
                .find(|v| *v != b'R' && !drive(*v).present)
                .ok_or(api::storage_error_capacity)?;
            publish_mount(r, i, letter, 0)?;
            mounted += 1;
        }
        Ok(mounted)
    }
}
unsafe fn mount(
    target: *const api::StorageTarget,
    letter_arg: u32,
    out: *mut api::StorageVolumeRef,
) -> Result<i32> {
    unsafe {
        out.write(api::StorageVolumeRef::default());
        let _gate = Gate::enter()?;
        let target = target.read();
        let region = validate(target, true)?;
        if target.kind != api::storage_target_partition {
            return Err(api::storage_error_unsupported);
        }
        let mut letter = letter_arg;
        if letter == 0 {
            letter = u32::from(b'D');
            while letter <= u32::from(b'Z')
                && (letter == u32::from(b'R') || drive(letter as u8).present)
            {
                letter += 1
            }
        }
        if letter < u32::from(b'D') || letter > u32::from(b'Z') {
            return Err(api::storage_error_protected);
        }
        if letter == u32::from(b'R') {
            let reserved = (*state())
                .recovery_cache_device
                .ok_or(api::storage_error_protected)?;
            let runtime = drive(b'C');
            if !runtime.present || reserved != region.device || runtime.role != 3 {
                return Err(api::storage_error_protected);
            }
        }
        if drive(letter as u8).present {
            return Err(api::storage_error_busy);
        }
        for i in 0..27 {
            let Some(reference) = mount_reference(i) else {
                continue;
            };
            let Ok(value) = mount_snapshot(reference) else {
                continue;
            };
            if value.region.overlaps(region) {
                return Err(api::storage_error_busy);
            }
        }
        let op = prepare(target)?;
        let _ending = Ending { op };
        let r = record(region.device.slot as usize);
        for i in 0..(*r).layout.count {
            let p = part(r, i);
            if p.number != target.partition_number {
                continue;
            }
            if letter == u32::from(b'R') && (*r).hints[i] != api::storage_filesystem_fat32 {
                return Err(api::storage_error_protected);
            }
            let reference = match publish_mount(r, i, letter as u8, 0) {
                Ok(reference) => reference,
                Err(error) => {
                    (*r).errors[i] = error;
                    (*r).last_error = error;
                    return Err(error);
                }
            };
            out.write(api::StorageVolumeRef {
                slot: reference.slot,
                generation: reference.generation,
                ..Default::default()
            });
            return Ok(0);
        }
        Err(api::storage_error_stale)
    }
}
unsafe fn unmount(volume: *const api::StorageVolumeRef) -> Result<i32> {
    if unsafe { (*volume).reserved } != 0 {
        return Err(api::storage_error_invalid);
    }
    let _gate = Gate::enter()?;
    ensure_records()?;
    let volume = unsafe { volume.read() };
    let value = mount_snapshot(Reference {
        slot: volume.slot,
        generation: volume.generation,
    })?;
    let r = record(value.region.device.slot as usize);
    unsafe {
        if !(*r).ready || id(r).reference != value.region.device {
            return Err(api::storage_error_stale);
        }
        let target = target_for_region(r, value.region);
        let op = prepare(target)?;
        finish(op, true)?;
    }
    Ok(0)
}
fn use_end(id: u64) -> Result<i32> {
    let owner = owner()?;
    scalar(
        12,
        Input {
            id,
            owner,
            ..Input::default()
        },
    )?;
    Ok(0)
}
pub(crate) fn release_owner(a: Args) -> bool {
    let input = Input {
        owner: Owner {
            program: a.program,
            program_generation: a.program_generation,
            task: a.task,
            task_generation: a.task_generation,
        },
        ..Default::default()
    };
    let _ = scalar(22, input);
    if scalar(23, input).expect("Owner claims scalar") == 0 {
        return true;
    }
    let Some(_gate) = Gate::try_enter() else {
        return false;
    };
    let mut complete = true;
    for i in 0..16 {
        let op = operation(i);
        unsafe {
            if (*op).id == 0
                || (*op).owner.program != a.program
                || (*op).owner.program_generation != a.program_generation
            {
                continue;
            }
            if a.task != 0
                && ((*op).owner.task != a.task || (*op).owner.task_generation != a.task_generation)
            {
                continue;
            }
            (*op).cleanup = true;
            complete = false;
        }
    }
    complete
}
unsafe extern "C" fn cleanup_main() -> ! {
    loop {
        if let Some(_gate) = Gate::try_enter() {
            for i in 0..16 {
                let op = operation(i);
                unsafe {
                    if (*op).id != 0 && (*op).cleanup {
                        let _ = finish(op, false);
                        break;
                    }
                }
            }
        }
        unsafe {
            scheduler::r4os_rust_scheduler_task(4, ptr::null_mut(), 100, 0, b"sleep".as_ptr(), 5);
        }
    }
}
/// # Safety
/// Private C facade supplies initialized Args and the exact typed output for
/// the selected operation. Original public R4SYS wrappers retain caller buffer
/// validation, instance identity and lifetimes; no borrowed output escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_operations(
    operation: u8,
    input: *const Args,
    out: *mut c_void,
) -> i32 {
    unsafe {
        let a = input.read();
        let result = match operation {
            0 => return i32::from(reserve_recovery(a.id as usize)),
            1 => inventory(out.cast()),
            2 => device_info(a.generation, a.slot, out.cast()),
            3 => partition_info(a.generation, a.device, a.slot, out.cast()),
            4 => volume_info(a.generation, a.slot, out.cast()),
            5 => claim_begin(a.target, out.cast()),
            6 => claim_end(a.id, a.flags),
            7 => read(a.target, a.relative, a.sectors, a.data, a.len),
            8 | 9 => claim_raw(a.id, a.relative, a.sectors, a.data, a.len, operation == 9),
            10 => claim_flush(a.id),
            11 => rescan(a.device),
            12 => mount(a.target, a.slot, out.cast()),
            13 => unmount(out.cast::<api::StorageVolumeRef>()),
            14 => use_end(a.id),
            15 => return i32::from(release_owner(a)),
            _ => unreachable!(),
        };
        match result {
            Ok(code) | Err(code) => code,
        }
    }
}
const _: () = {
    assert!(mem::size_of::<Args>() == 96);
    assert!(mem::size_of::<DriveMetadata>() == 56);
    assert!(mem::size_of::<VolumeMetadata>() == 8);
    assert!(mem::size_of::<Probe>() == 32);
};

unsafe fn ops_cache(index: usize, first: u64, count: u64) -> bool {
    unsafe { crate::fs::page_cache::r4os_rust_pagecache_flush_range(index, first, count) }
}

unsafe fn ops_drive(letter: u8, out: *mut DriveMetadata) {
    unsafe {
        use crate::fs::{drive, vfs, vfs_types::Volume};
        let d = drive::r4os_rust_fs_drive_lookup(1, letter as usize);
        let mut v = MaybeUninit::<Volume>::uninit();
        let mut value = DriveMetadata {
            present: !d.is_null(),
            has_vfs: vfs::r4os_rust_vfs_mount(
                4,
                letter,
                ptr::null(),
                ptr::null(),
                v.as_mut_ptr().cast(),
            ),
            kind: 0,
            role: 0,
            bytes: 0,
            index: usize::MAX,
            name: [0; 32],
        };
        if !d.is_null() {
            value.kind = (*d).kind;
            value.role = (*d).role;
            value.bytes = (*d).bytes;
            value.index = if (*d).has_block {
                (*d).block_index
            } else {
                usize::MAX
            };
            let n = (*d).name.len.min(31);
            ptr::copy_nonoverlapping((*d).name.data, value.name.as_mut_ptr(), n);
        }
        out.write(value);
    }
}

unsafe fn ops_volume(slot: u32, out: *mut VolumeMetadata) -> bool {
    unsafe {
        use crate::fs::{drive, vfs, vfs_types::Volume};
        let mut v = MaybeUninit::<Volume>::uninit();
        let letter = if slot == 26 {
            0
        } else {
            u8::try_from(u32::from(b'A') + slot).unwrap()
        };
        if !vfs::r4os_rust_vfs_mount(
            if slot == 26 { 3 } else { 4 },
            letter,
            ptr::null(),
            ptr::null(),
            v.as_mut_ptr().cast(),
        ) {
            return false;
        }
        let v = v.assume_init();
        let d = if slot == 26 {
            ptr::null_mut()
        } else {
            drive::r4os_rust_fs_drive_lookup(1, letter as usize)
        };
        out.write(VolumeMetadata {
            filesystem: if v.kind == 0 {
                api::storage_filesystem_fat32
            } else {
                api::storage_filesystem_ntfs
            },
            role: if d.is_null() { 0 } else { u32::from((*d).role) },
        });
        true
    }
}

unsafe fn ops_mount_reference(letter: u8, out: *mut Reference) -> bool {
    unsafe {
        let mut v = MaybeUninit::<crate::fs::vfs_types::Volume>::uninit();
        if !crate::fs::vfs::r4os_rust_vfs_mount(
            4,
            letter,
            ptr::null(),
            ptr::null(),
            v.as_mut_ptr().cast(),
        ) {
            return false;
        }
        let v = v.assume_init();
        if !v.has_ref {
            return false;
        }
        out.write(v.reference);
        true
    }
}

unsafe fn ops_unmount(reference: Reference) -> bool {
    unsafe { crate::fs::vfs::r4os_rust_vfs_mount(5, 0, ptr::null(), &reference, ptr::null_mut()) }
}

unsafe fn ops_forget(filesystem: u8, index: usize, first: u64, count: u64) {
    assert!(filesystem <= 1);
    unsafe { crate::fs::vfs_backends::forget(filesystem == 1, index, first, count) }
}

unsafe fn ops_probe(
    input: *const Probe,
    context: *mut c_void,
    consume: unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32,
) -> i32 {
    unsafe {
        let a = &*input;
        let ntfs = match a.filesystem {
            api::storage_filesystem_fat32 => false,
            api::storage_filesystem_ntfs => true,
            _ => return api::storage_error_unsupported,
        };
        let Some(v) = crate::fs::vfs_backends::probe(ntfs, a.index, a.lba, a.sectors, false) else {
            return api::storage_error_remount;
        };
        let mut v = Some(v);
        consume(context, ptr::from_mut(&mut v).cast())
    }
}

unsafe fn ops_create_cleanup(entry: unsafe extern "C" fn() -> !) -> bool {
    let _ = entry;
    let r = crate::sched::task::task_api::CreateRequest {
        name: b"storage-cleanup".as_ptr(),
        name_len: 15,
        state: 1,
        role: 3,
        needs_fpu: true,
        smp_eligible: false,
        smp_r4x: false,
        owner_kind: 0,
        owner_context: ptr::null_mut(),
        entry: Some(cleanup_entry),
    };
    let mut failure = 0;
    !unsafe { crate::sched::task::task_api::r4os_rust_task_create(&r, true, &mut failure) }
        .is_null()
}

unsafe extern "C" fn cleanup_entry() {
    unsafe { cleanup_main() }
}
