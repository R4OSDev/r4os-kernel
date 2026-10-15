//! Shared immutable raster generations. The BO owner retains allocation,
//! mappings and physical release; this owner retains exact producer/frame/
//! consumer generations under that same BO metadata mutex, with no mirror.
use super::{console::Lease, context, gui_flow, gui_view, payload_types as t, registry, threads};
use crate::{
    contract::{self as c, GuiSharedRasterHandle as Handle, ProgramProcessHandle as Owner},
    sched::{sync_locks, sync_types::Mutex},
};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr,
};
const RESOURCES: usize = 32;
const LEASES: usize = 128;
const FRAMES: usize = 256;
const STAT_SLOTS: usize = 64;
const BUFFERS: usize = 3;
const NO_OWNER: Owner = Owner {
    instance_id: 0,
    reserved: 0,
    generation: 0,
};
const NO_HANDLE: Handle = Handle {
    id: 0,
    generation: 0,
};
const NO_REFERENCE: t::SharedRasterRef = t::SharedRasterRef {
    handle: NO_HANDLE,
    raster_generation: 0,
    data_bytes: 0,
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BackendHandle {
    pub generation: u64,
    pub id: u32,
    pub reserved: u32,
}
const NO_BACKEND: BackendHandle = BackendHandle {
    generation: 0,
    id: 0,
    reserved: 0,
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Created {
    pub buffer: BackendHandle,
    pub reference: BackendHandle,
    pub address: u64,
    pub bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Prepared {
    pub status: i32,
    pub out_of_memory: bool,
    pub reserved: [u8; 3],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GraphicsOwner {
    pub id: u64,
    pub generation: u64,
    pub kind: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Debug {
    pub failure_value: u32,
    pub failure_cursor: u32,
    pub failure_present: bool,
    pub reserved: [u8; 7],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub owner: Owner,
    pub publish_count: u64,
    pub acquire_count: u64,
    pub release_count: u64,
    pub backpressure_count: u64,
    pub published_bytes: u64,
    pub frame_bytes_avoided: u64,
    pub acquired_bytes: u64,
    pub live_bytes: u64,
    pub used: bool,
    pub reserved: [u8; 7],
}
const EMPTY_STATS: Stats = Stats {
    owner: NO_OWNER,
    publish_count: 0,
    acquire_count: 0,
    release_count: 0,
    backpressure_count: 0,
    published_bytes: 0,
    frame_bytes_avoided: 0,
    acquired_bytes: 0,
    live_bytes: 0,
    used: false,
    reserved: [0; 7],
};
#[derive(Clone, Copy)]
struct Buffer {
    memory: *mut u8,
    bytes: usize,
    backing: BackendHandle,
    reference: BackendHandle,
    write_lease: BackendHandle,
    frame_lease: BackendHandle,
    raster_generation: u64,
    write_token: u64,
    frame_refs: u32,
    lease_refs: u32,
}
const EMPTY_BUFFER: Buffer = Buffer {
    memory: ptr::null_mut(),
    bytes: 0,
    backing: NO_BACKEND,
    reference: NO_BACKEND,
    write_lease: NO_BACKEND,
    frame_lease: NO_BACKEND,
    raster_generation: 0,
    write_token: 0,
    frame_refs: 0,
    lease_refs: 0,
};
#[derive(Clone, Copy)]
struct Resource {
    used: bool,
    closing: bool,
    owner: Owner,
    handle: Handle,
    info: c::GuiSharedRasterCreateInfo,
    buffers: [Buffer; BUFFERS],
}
const EMPTY_RESOURCE: Resource = Resource {
    used: false,
    closing: false,
    owner: NO_OWNER,
    handle: NO_HANDLE,
    info: c::GuiSharedRasterCreateInfo {
        version: 1,
        size: 48,
        format: 0,
        width: 0,
        height: 0,
        stride_bytes: 0,
        data_offset: 0,
        flags: 0,
        data_bytes: 0,
        reserved0: 0,
    },
    buffers: [EMPTY_BUFFER; BUFFERS],
};
#[derive(Clone, Copy)]
struct LeaseRecord {
    used: bool,
    consumer: Owner,
    producer: Owner,
    handle: Handle,
    raster_generation: u64,
    lease_token: u64,
    buffer_index: u32,
    backing_lease: BackendHandle,
}
const EMPTY_LEASE: LeaseRecord = LeaseRecord {
    used: false,
    consumer: NO_OWNER,
    producer: NO_OWNER,
    handle: NO_HANDLE,
    raster_generation: 0,
    lease_token: 0,
    buffer_index: 0,
    backing_lease: NO_BACKEND,
};
#[derive(Clone, Copy)]
struct FrameRecord {
    used: bool,
    frame: *mut t::Frame,
    reference: t::SharedRasterRef,
}
const EMPTY_FRAME: FrameRecord = FrameRecord {
    used: false,
    frame: ptr::null_mut(),
    reference: NO_REFERENCE,
};
#[derive(Clone, Copy)]
struct FreeSet {
    memories: [Option<Created>; BUFFERS],
}
const EMPTY_SET: FreeSet = FreeSet {
    memories: [None; BUFFERS],
};
struct State {
    debug: Debug,
    resources: [Resource; RESOURCES],
    leases: [LeaseRecord; LEASES],
    frames: [FrameRecord; FRAMES],
    stats: [Stats; STAT_SLOTS],
    handle_generation: u64,
    generation: u64,
    write_token: u64,
    lease_token: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: all live resource/reference/counter operations use the existing BO
// metadata mutex; diagnostic injection retains its original serialized test
// admission. Only this storage owns these records and monotonic generations.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    debug: Debug {
        failure_value: 0,
        failure_cursor: 0,
        failure_present: false,
        reserved: [0; 7],
    },
    resources: [EMPTY_RESOURCE; RESOURCES],
    leases: [EMPTY_LEASE; LEASES],
    frames: [EMPTY_FRAME; FRAMES],
    stats: [EMPTY_STATS; STAT_SLOTS],
    handle_generation: 0,
    generation: 0,
    write_token: 0,
    lease_token: 0,
}));
fn state() -> *mut State {
    STATE.0.get()
}
unsafe extern "C" {
    // Stateless typed leaves into the separate remaining BO/platform owners.
    fn r4os_zig_program_shared_mutex() -> *mut Mutex;
    fn r4os_zig_program_shared_prepare(references: usize, leases: usize) -> Prepared;
    fn r4os_zig_program_shared_create(
        owner: Owner,
        info: c::GuiSharedRasterCreateInfo,
        xrgb: bool,
        out: *mut Created,
    ) -> bool;
    fn r4os_zig_program_shared_import(
        buffer: BackendHandle,
        owner: Owner,
        raster: bool,
        read_only: bool,
        out: *mut BackendHandle,
    ) -> i32;
    fn r4os_zig_program_shared_drop(
        reference: BackendHandle,
        owner: Owner,
        raster: bool,
        locked: bool,
    ) -> i32;
    fn r4os_zig_program_shared_map(
        reference: BackendHandle,
        write: bool,
        bytes: usize,
        out: *mut BackendHandle,
    ) -> bool;
    fn r4os_zig_program_shared_unmap(lease: BackendHandle) -> bool;
    fn r4os_zig_program_shared_use(lease: BackendHandle, out: *mut BackendHandle) -> bool;
    fn r4os_zig_program_shared_reference(
        reference: BackendHandle,
        owner: Owner,
        out: *mut c::GfxBufferReference,
    ) -> bool;
    fn r4os_zig_program_shared_stopped(op: u8, owner: Owner);
}
pub(super) fn lock() {
    while !unsafe { sync_locks::r4os_rust_sync_mutex(r4os_zig_program_shared_mutex(), 0, 0) } {
        core::hint::spin_loop();
    }
}
fn unlock() {
    unsafe { sync_locks::r4os_rust_sync_mutex(r4os_zig_program_shared_mutex(), 2, 0) };
}
struct Metadata;
impl Metadata {
    fn acquire() -> Self {
        lock();
        Self
    }
    fn prepared(references: usize, leases: usize) -> Result<Self, Prepared> {
        let result = unsafe { r4os_zig_program_shared_prepare(references, leases) };
        if result.status == 0 {
            Ok(Self)
        } else {
            Err(result)
        }
    }
}
impl Drop for Metadata {
    fn drop(&mut self) {
        unlock();
    }
}
unsafe fn next(counter: *mut u64) -> Option<u64> {
    unsafe {
        if *counter == u64::MAX {
            return None;
        }
        *counter += 1;
        Some(*counter)
    }
}
pub(super) fn valid_handle(handle: Handle) -> bool {
    handle.id != 0 && handle.id <= RESOURCES as u64 && handle.generation != 0
}
pub(super) fn equal(a: Handle, b: Handle) -> bool {
    a.id == b.id && a.generation == b.generation
}
unsafe fn resource(handle: Handle) -> *mut Resource {
    unsafe {
        if !valid_handle(handle) {
            return ptr::null_mut();
        }
        let resource = ptr::addr_of_mut!((*state()).resources[(handle.id - 1) as usize]);
        if !(*resource).used || !equal((*resource).handle, handle) {
            ptr::null_mut()
        } else {
            resource
        }
    }
}
unsafe fn stats_locked(owner: Owner, create: bool) -> *mut Stats {
    unsafe {
        for index in 0..STAT_SLOTS {
            let item = ptr::addr_of_mut!((*state()).stats[index]);
            if (*item).used && registry::equal((*item).owner, owner) {
                return item;
            }
        }
        if !create {
            return ptr::null_mut();
        }
        for index in 0..STAT_SLOTS {
            let item = ptr::addr_of_mut!((*state()).stats[index]);
            if (*item).used {
                continue;
            }
            item.write(Stats {
                used: true,
                owner,
                ..EMPTY_STATS
            });
            return item;
        }
        ptr::null_mut()
    }
}
unsafe fn can_free(resource: *const Resource) -> bool {
    unsafe {
        if !(*resource).closing {
            return false;
        }
        for buffer in (*resource).buffers {
            if buffer.write_token != 0 || buffer.frame_refs != 0 || buffer.lease_refs != 0 {
                return false;
            }
        }
        true
    }
}
unsafe fn clear_resource(resource: *mut Resource) -> FreeSet {
    unsafe {
        let mut result = EMPTY_SET;
        for (index, buffer) in (*resource).buffers.into_iter().enumerate() {
            assert!(!buffer.memory.is_null());
            result.memories[index] = Some(Created {
                buffer: buffer.backing,
                reference: buffer.reference,
                address: buffer.memory.addr() as u64,
                bytes: buffer.bytes as u64,
            });
        }
        let stats = stats_locked((*resource).owner, false);
        if !stats.is_null() {
            let allocated = (*resource).info.data_bytes.saturating_mul(BUFFERS as u64);
            (*stats).live_bytes = (*stats).live_bytes.saturating_sub(allocated);
        }
        resource.write(EMPTY_RESOURCE);
        result
    }
}
fn free_memories(set: FreeSet) {
    for allocation in set.memories.into_iter().flatten() {
        assert_eq!(
            unsafe { r4os_zig_program_shared_drop(allocation.reference, NO_OWNER, true, false) },
            0
        );
    }
}
pub(super) fn graphics_owner(owner: Owner) -> GraphicsOwner {
    GraphicsOwner {
        id: u64::from(owner.instance_id),
        generation: owner.generation,
        kind: 1,
        reserved: 0,
    }
}
unsafe fn end_write(buffer: *mut Buffer) {
    unsafe {
        if (*buffer).write_lease.id != 0 {
            assert!(r4os_zig_program_shared_unmap((*buffer).write_lease));
        }
        (*buffer).write_lease = NO_BACKEND;
        (*buffer).write_token = 0;
    }
}
unsafe fn retain_frame(buffer: *mut Buffer) -> bool {
    unsafe {
        if (*buffer).frame_refs == 0 {
            assert!(!(*buffer).memory.is_null());
            let mut lease = MaybeUninit::uninit();
            if !r4os_zig_program_shared_map(
                (*buffer).reference,
                false,
                (*buffer).bytes,
                lease.as_mut_ptr(),
            ) {
                return false;
            }
            (*buffer).frame_lease = lease.assume_init();
        }
        (*buffer).frame_refs += 1;
        true
    }
}
unsafe fn drop_frame(buffer: *mut Buffer) {
    unsafe {
        assert!((*buffer).frame_refs != 0);
        (*buffer).frame_refs -= 1;
        if (*buffer).frame_refs == 0 {
            assert!(r4os_zig_program_shared_unmap((*buffer).frame_lease));
            (*buffer).frame_lease = NO_BACKEND;
        }
    }
}
pub(super) fn valid_info(info: c::GuiSharedRasterCreateInfo) -> bool {
    if info.version != c::gui_shared_raster_create_info_version
        || info.size != c::gui_shared_raster_create_info_size
        || info.width == 0
        || info.height == 0
        || info.stride_bytes == 0
        || info.flags != 0
        || info.reserved0 != 0
        || info.data_bytes == 0
        || info.data_bytes > c::gui_shared_raster_max_bytes
    {
        return false;
    }
    let row = match info.format {
        c::gui_shared_raster_format_xrgb32 => {
            if info.data_offset != 0 {
                return false;
            }
            let Some(row) = u64::from(info.width).checked_mul(4) else {
                return false;
            };
            row
        }
        c::gui_shared_raster_format_indexed8 => {
            if info.data_offset != 1024 {
                return false;
            }
            u64::from(info.width)
        }
        c::gui_shared_raster_format_alpha8 => {
            if info.data_offset != 0 {
                return false;
            }
            u64::from(info.width)
        }
        _ => return false,
    };
    if u64::from(info.stride_bytes) < row
        || (info.format == c::gui_shared_raster_format_xrgb32
            && !info.stride_bytes.is_multiple_of(4))
    {
        return false;
    }
    let Some(rows) = u64::from(info.stride_bytes).checked_mul(u64::from(info.height)) else {
        return false;
    };
    let Some(required) = u64::from(info.data_offset).checked_add(rows) else {
        return false;
    };
    required == info.data_bytes && usize::try_from(required).is_ok()
}
unsafe fn allocate(owner: Owner, info: c::GuiSharedRasterCreateInfo) -> Option<Created> {
    unsafe {
        let debug = ptr::addr_of_mut!((*state()).debug);
        if (*debug).failure_present {
            let cursor = (*debug).failure_cursor;
            (*debug).failure_cursor = cursor.wrapping_add(1);
            if cursor == (*debug).failure_value {
                return None;
            }
        }
        let mut created = MaybeUninit::uninit();
        if !r4os_zig_program_shared_create(
            owner,
            info,
            info.format == c::gui_shared_raster_format_xrgb32,
            created.as_mut_ptr(),
        ) {
            return None;
        }
        let mut created = created.assume_init();
        let Ok(metadata) = Metadata::prepared(1, 0) else {
            r4os_zig_program_shared_drop(created.reference, owner, false, false);
            return None;
        };
        let mut bridge = MaybeUninit::uninit();
        if r4os_zig_program_shared_import(
            created.buffer,
            NO_OWNER,
            true,
            false,
            bridge.as_mut_ptr(),
        ) != 0
        {
            drop(metadata);
            r4os_zig_program_shared_drop(created.reference, owner, false, false);
            return None;
        }
        assert_eq!(
            r4os_zig_program_shared_drop(created.reference, owner, false, true),
            0
        );
        created.reference = bridge.assume_init();
        drop(metadata);
        Some(created)
    }
}
pub(super) unsafe fn create(
    owner: Owner,
    info: c::GuiSharedRasterCreateInfo,
    out: *mut Handle,
) -> i32 {
    unsafe {
        if !registry::valid(owner) || !valid_info(info) {
            return c::gui_frame_error_invalid;
        }
        let Some(_operation) = threads::Operation::enter() else {
            return c::gui_frame_error_state;
        };
        let mut memories = EMPTY_SET;
        for index in 0..BUFFERS {
            let Some(created) = allocate(owner, info) else {
                free_memories(memories);
                return c::gui_frame_error_oom;
            };
            memories.memories[index] = Some(created);
        }
        let metadata = Metadata::acquire();
        let mut selected = None;
        for index in 0..RESOURCES {
            if !(*state()).resources[index].used {
                selected = Some(index);
                break;
            }
        }
        let Some(index) = selected else {
            drop(metadata);
            free_memories(memories);
            return c::gui_frame_error_unavailable;
        };
        let Some(generation) = next(ptr::addr_of_mut!((*state()).handle_generation)) else {
            drop(metadata);
            free_memories(memories);
            return c::gui_frame_error_overflow;
        };
        let stats = stats_locked(owner, true);
        if stats.is_null() {
            drop(metadata);
            free_memories(memories);
            return c::gui_frame_error_unavailable;
        }
        let handle = Handle {
            id: index as u64 + 1,
            generation,
        };
        let resource = ptr::addr_of_mut!((*state()).resources[index]);
        resource.write(Resource {
            used: true,
            owner,
            handle,
            info,
            ..EMPTY_RESOURCE
        });
        for index in 0..BUFFERS {
            let allocation = memories.memories[index].expect("Shared raster allocation");
            let buffer = ptr::addr_of_mut!((*resource).buffers[index]);
            (*buffer).memory = allocation.address as *mut u8;
            (*buffer).bytes =
                usize::try_from(allocation.bytes).expect("Shared raster allocation bytes");
            (*buffer).backing = allocation.buffer;
            (*buffer).reference = allocation.reference;
        }
        (*stats).live_bytes = (*stats)
            .live_bytes
            .saturating_add(info.data_bytes.saturating_mul(BUFFERS as u64));
        out.write(handle);
        drop(metadata);
        c::gui_frame_result_ok
    }
}
pub(super) unsafe fn destroy(owner: Owner, handle: Handle) -> i32 {
    unsafe {
        let Some(_operation) = threads::Operation::enter() else {
            return c::gui_frame_error_state;
        };
        if !registry::valid(owner) || !valid_handle(handle) {
            return c::gui_frame_error_invalid;
        }
        let mut free = None;
        let metadata = Metadata::acquire();
        let resource = resource(handle);
        if resource.is_null() {
            return c::gui_frame_error_stale;
        }
        if !registry::equal((*resource).owner, owner) {
            return c::gui_frame_error_invalid;
        }
        (*resource).closing = true;
        for index in 0..BUFFERS {
            end_write(ptr::addr_of_mut!((*resource).buffers[index]));
        }
        if can_free(resource) {
            free = Some(clear_resource(resource));
        }
        drop(metadata);
        if let Some(set) = free {
            free_memories(set);
        }
        c::gui_frame_result_ok
    }
}
pub(super) unsafe fn map_write(
    owner: Owner,
    handle: Handle,
    out: *mut c::GuiSharedRasterWriteMap,
) -> i32 {
    unsafe {
        let metadata = match Metadata::prepared(0, 1) {
            Ok(metadata) => metadata,
            Err(error) => {
                return if error.out_of_memory {
                    c::gui_frame_error_oom
                } else {
                    c::gui_frame_error_state
                };
            }
        };
        let resource = resource(handle);
        if resource.is_null() {
            return c::gui_frame_error_stale;
        }
        if !registry::equal((*resource).owner, owner) || (*resource).closing {
            return c::gui_frame_error_invalid;
        }
        let Some(token) = next(ptr::addr_of_mut!((*state()).write_token)) else {
            return c::gui_frame_error_overflow;
        };
        let mut selected = None;
        for pass in 0..2 {
            for index in 0..BUFFERS {
                let buffer = ptr::addr_of_mut!((*resource).buffers[index]);
                if (*buffer).write_token != 0
                    || (*buffer).frame_refs != 0
                    || (*buffer).lease_refs != 0
                    || (pass == 0 && (*buffer).raster_generation != 0)
                    || (pass == 1 && (*buffer).raster_generation == 0)
                {
                    continue;
                }
                if (*buffer).memory.is_null() {
                    continue;
                }
                let mut lease = MaybeUninit::uninit();
                if !r4os_zig_program_shared_map(
                    (*buffer).reference,
                    true,
                    (*buffer).bytes,
                    lease.as_mut_ptr(),
                ) {
                    continue;
                }
                (*buffer).write_lease = lease.assume_init();
                selected = Some(index);
                break;
            }
            if selected.is_some() {
                break;
            }
        }
        let Some(index) = selected else {
            let stats = stats_locked(owner, false);
            if !stats.is_null() {
                (*stats).backpressure_count = (*stats).backpressure_count.wrapping_add(1);
            }
            return c::gui_frame_error_state;
        };
        let buffer = ptr::addr_of_mut!((*resource).buffers[index]);
        assert!(!(*buffer).memory.is_null());
        (*buffer).raster_generation = 0;
        (*buffer).write_token = token;
        out.write(c::GuiSharedRasterWriteMap {
            handle,
            data_address: (*buffer).memory.addr() as u64,
            byte_length: (*buffer).bytes as u64,
            write_token: token,
            buffer_index: index as u32,
            ..Default::default()
        });
        drop(metadata);
        c::gui_frame_result_ok
    }
}
pub(super) unsafe fn publish(owner: Owner, map: c::GuiSharedRasterWriteMap, out: *mut u64) -> i32 {
    unsafe {
        if map.version != c::gui_shared_raster_write_map_version
            || map.size != c::gui_shared_raster_write_map_size
            || map.write_token == 0
            || map.buffer_index >= c::gui_shared_raster_buffer_count
            || map.reserved0 != 0
        {
            return c::gui_frame_error_invalid;
        }
        let metadata = Metadata::acquire();
        let resource = resource(map.handle);
        if resource.is_null() {
            return c::gui_frame_error_stale;
        }
        if !registry::equal((*resource).owner, owner) || (*resource).closing {
            return c::gui_frame_error_invalid;
        }
        let buffer = ptr::addr_of_mut!((*resource).buffers[map.buffer_index as usize]);
        if (*buffer).memory.is_null() {
            return c::gui_frame_error_unavailable;
        }
        let address = (*buffer).memory.addr() as u64;
        let bytes = (*buffer).bytes as u64;
        if (*buffer).write_token != map.write_token
            || map.data_address != address
            || map.byte_length != bytes
            || (*buffer).frame_refs != 0
            || (*buffer).lease_refs != 0
        {
            return c::gui_frame_error_stale;
        }
        let Some(generation) = next(ptr::addr_of_mut!((*state()).generation)) else {
            return c::gui_frame_error_overflow;
        };
        end_write(buffer);
        (*buffer).raster_generation = generation;
        let stats = stats_locked(owner, false);
        if !stats.is_null() {
            (*stats).publish_count = (*stats).publish_count.wrapping_add(1);
            (*stats).published_bytes = (*stats).published_bytes.saturating_add(bytes);
        }
        out.write(generation);
        drop(metadata);
        c::gui_frame_result_ok
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Original exact raster pin keeps independent optional format/geometry/length checks"
)]
pub(super) unsafe fn pin_exact(
    owner: Owner,
    frame: *mut t::Frame,
    handle: Handle,
    generation: u64,
    format: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    bytes: Option<u64>,
) -> Option<t::SharedRasterRef> {
    unsafe {
        if generation == 0 {
            return None;
        }
        let Ok(_metadata) = Metadata::prepared(0, 1) else {
            return None;
        };
        for index in 0..FRAMES {
            let record = ptr::addr_of_mut!((*state()).frames[index]);
            if (*record).used
                && (*record).frame == frame
                && equal((*record).reference.handle, handle)
                && (*record).reference.raster_generation == generation
            {
                return Some((*record).reference);
            }
        }
        let mut selected = ptr::null_mut();
        for index in 0..FRAMES {
            let record = ptr::addr_of_mut!((*state()).frames[index]);
            if !(*record).used {
                selected = record;
                break;
            }
        }
        if selected.is_null() {
            return None;
        }
        let resource = resource(handle);
        if resource.is_null() || !registry::equal((*resource).owner, owner) || (*resource).closing {
            return None;
        }
        if format.is_some_and(|value| (*resource).info.format != value)
            || width.is_some_and(|value| (*resource).info.width != value)
            || height.is_some_and(|value| (*resource).info.height != value)
            || bytes.is_some_and(|value| (*resource).info.data_bytes != value)
        {
            return None;
        }
        for index in 0..BUFFERS {
            let buffer = ptr::addr_of_mut!((*resource).buffers[index]);
            if (*buffer).raster_generation != generation
                || (*buffer).write_token != 0
                || (*buffer).frame_refs == u32::MAX
            {
                continue;
            }
            if !retain_frame(buffer) {
                return None;
            }
            let reference = t::SharedRasterRef {
                handle,
                raster_generation: generation,
                data_bytes: (*resource).info.data_bytes,
            };
            selected.write(FrameRecord {
                used: true,
                frame,
                reference,
            });
            return Some(reference);
        }
        None
    }
}
pub(super) unsafe fn pin_frame(
    owner: Owner,
    frame: *mut t::Frame,
    descriptor: c::GuiSharedRasterResource,
) -> Option<t::SharedRasterRef> {
    unsafe {
        pin_exact(
            owner,
            frame,
            descriptor.handle,
            descriptor.raster_generation,
            Some(descriptor.format),
            Some(descriptor.guest_w),
            Some(descriptor.guest_h),
            None,
        )
    }
}
pub(super) unsafe fn copy_frames(
    owner: Owner,
    source: *const t::Frame,
    target: *mut t::Frame,
) -> bool {
    unsafe {
        let metadata = Metadata::acquire();
        let mut added = [0usize; c::gui_shared_raster_max_frame_resources as usize];
        let mut added_count = 0;
        let mut copied = 0u32;
        let initial = (*target).shared_raster_count;
        for source_index in 0..FRAMES {
            let source_record = (*state()).frames[source_index];
            if !source_record.used || source_record.frame.cast_const() != source {
                continue;
            }
            let mut duplicate = false;
            for target_index in 0..FRAMES {
                let record = (*state()).frames[target_index];
                if record.used
                    && record.frame == target
                    && equal(record.reference.handle, source_record.reference.handle)
                    && record.reference.raster_generation
                        == source_record.reference.raster_generation
                {
                    duplicate = true;
                    break;
                }
            }
            if duplicate {
                copied += 1;
                continue;
            }
            let mut free = None;
            for index in 0..FRAMES {
                if !(*state()).frames[index].used {
                    free = Some(index);
                    break;
                }
            }
            let Some(index) = free else {
                break;
            };
            let record = ptr::addr_of_mut!((*state()).frames[index]);
            let resource = resource(source_record.reference.handle);
            if resource.is_null() {
                break;
            }
            if !registry::equal((*resource).owner, owner)
                || (*resource).info.data_bytes != source_record.reference.data_bytes
            {
                break;
            }
            let mut pinned = false;
            for buffer_index in 0..BUFFERS {
                let buffer = ptr::addr_of_mut!((*resource).buffers[buffer_index]);
                if (*buffer).raster_generation != source_record.reference.raster_generation
                    || (*buffer).write_token != 0
                    || (*buffer).frame_refs == 0
                    || (*buffer).frame_refs == u32::MAX
                {
                    continue;
                }
                if !retain_frame(buffer) {
                    break;
                }
                pinned = true;
                break;
            }
            if !pinned {
                break;
            }
            record.write(FrameRecord {
                used: true,
                frame: target,
                reference: source_record.reference,
            });
            added[added_count] = index;
            added_count += 1;
            copied += 1;
        }
        if copied != (*source).shared_raster_count {
            for &index in &added[..added_count] {
                let record = ptr::addr_of_mut!((*state()).frames[index]);
                let resource = resource((*record).reference.handle);
                if !resource.is_null() {
                    for buffer_index in 0..BUFFERS {
                        let buffer = ptr::addr_of_mut!((*resource).buffers[buffer_index]);
                        if (*buffer).raster_generation == (*record).reference.raster_generation
                            && (*buffer).frame_refs != 0
                        {
                            drop_frame(buffer);
                            break;
                        }
                    }
                }
                record.write(EMPTY_FRAME);
            }
            return false;
        }
        drop(metadata);
        (*target).shared_raster_count =
            initial + u32::try_from(added_count).expect("Shared frame reference count");
        true
    }
}
pub(super) unsafe fn move_frames(source: *mut t::Frame, target: *mut t::Frame) -> bool {
    unsafe {
        let metadata = Metadata::acquire();
        let mut moved = 0u32;
        for index in 0..FRAMES {
            let record = (*state()).frames[index];
            if record.used && record.frame == source {
                moved += 1;
            }
        }
        if moved != (*source).shared_raster_count
            || (*target).shared_raster_count + moved > c::gui_shared_raster_max_frame_resources
        {
            return false;
        }
        for index in 0..FRAMES {
            let record = ptr::addr_of_mut!((*state()).frames[index]);
            if (*record).used && (*record).frame == source {
                (*record).frame = target;
            }
        }
        drop(metadata);
        (*target).shared_raster_count += moved;
        (*source).shared_raster_count = 0;
        true
    }
}
pub(super) unsafe fn release_frames(frame: *mut t::Frame) -> bool {
    unsafe {
        let Some(_operation) = threads::Operation::enter() else {
            return false;
        };
        let mut sets =
            [MaybeUninit::<FreeSet>::uninit(); c::gui_shared_raster_max_frame_resources as usize];
        let mut count = 0;
        let mut released = 0u32;
        let metadata = Metadata::acquire();
        for index in 0..FRAMES {
            let record = ptr::addr_of_mut!((*state()).frames[index]);
            if !(*record).used || (*record).frame != frame {
                continue;
            }
            let resource = resource((*record).reference.handle);
            if !resource.is_null() {
                for buffer_index in 0..BUFFERS {
                    let buffer = ptr::addr_of_mut!((*resource).buffers[buffer_index]);
                    if (*buffer).raster_generation != (*record).reference.raster_generation
                        || (*buffer).frame_refs == 0
                    {
                        continue;
                    }
                    drop_frame(buffer);
                    released += 1;
                    break;
                }
                if can_free(resource) {
                    sets[count].write(clear_resource(resource));
                    count += 1;
                }
            }
            record.write(EMPTY_FRAME);
        }
        drop(metadata);
        for set in &sets[..count] {
            free_memories(set.assume_init());
        }
        let expected = (*frame).shared_raster_count;
        (*frame).shared_raster_count = 0;
        released == expected
    }
}
pub(super) unsafe fn has_reference(
    frame: *const t::Frame,
    handle: Handle,
    generation: u64,
) -> bool {
    let _metadata = Metadata::acquire();
    unsafe {
        for index in 0..FRAMES {
            let record = (*state()).frames[index];
            if record.used
                && record.frame.cast_const() == frame
                && equal(record.reference.handle, handle)
                && record.reference.raster_generation == generation
            {
                return true;
            }
        }
        false
    }
}
pub(super) unsafe fn acquire(
    consumer: Owner,
    owner: Owner,
    frame_generation: u64,
    handle: Handle,
    generation: u64,
    out: *mut c::GuiSharedRasterMap,
) -> i32 {
    unsafe {
        if !registry::valid(consumer)
            || !registry::valid(owner)
            || frame_generation == 0
            || !valid_handle(handle)
            || generation == 0
        {
            return c::gui_frame_error_invalid;
        }
        let metadata = match Metadata::prepared(0, 1) {
            Ok(metadata) => metadata,
            Err(error) => {
                return if error.out_of_memory {
                    c::gui_frame_error_oom
                } else {
                    c::gui_frame_error_state
                };
            }
        };
        let resource = resource(handle);
        if resource.is_null() {
            return c::gui_frame_error_stale;
        }
        if !registry::equal((*resource).owner, owner) {
            return c::gui_frame_error_invalid;
        }
        let mut selected = None;
        for index in 0..BUFFERS {
            let buffer = ptr::addr_of_mut!((*resource).buffers[index]);
            if (*buffer).raster_generation == generation
                && (*buffer).write_token == 0
                && (*buffer).frame_refs != 0
                && (*buffer).lease_refs != u32::MAX
            {
                selected = Some(index);
                break;
            }
        }
        let Some(index) = selected else {
            return c::gui_frame_error_stale;
        };
        let mut selected = ptr::null_mut();
        for record_index in 0..LEASES {
            let record = ptr::addr_of_mut!((*state()).leases[record_index]);
            if !(*record).used {
                selected = record;
                break;
            }
        }
        if selected.is_null() {
            return c::gui_frame_error_unavailable;
        }
        let Some(token) = next(ptr::addr_of_mut!((*state()).lease_token)) else {
            return c::gui_frame_error_overflow;
        };
        let buffer = ptr::addr_of_mut!((*resource).buffers[index]);
        if (*buffer).memory.is_null() {
            return c::gui_frame_error_unavailable;
        }
        let mut backing = MaybeUninit::uninit();
        if !r4os_zig_program_shared_map(
            (*buffer).reference,
            false,
            (*buffer).bytes,
            backing.as_mut_ptr(),
        ) {
            return c::gui_frame_error_state;
        }
        (*buffer).lease_refs += 1;
        selected.write(LeaseRecord {
            used: true,
            consumer,
            producer: owner,
            handle,
            raster_generation: generation,
            lease_token: token,
            buffer_index: index as u32,
            backing_lease: backing.assume_init(),
        });
        let stats = stats_locked(owner, false);
        if !stats.is_null() {
            (*stats).acquire_count = (*stats).acquire_count.wrapping_add(1);
            (*stats).acquired_bytes = (*stats)
                .acquired_bytes
                .saturating_add((*buffer).bytes as u64);
        }
        out.write(c::GuiSharedRasterMap {
            frame_owner: owner,
            frame_generation,
            lease: c::GuiSharedRasterLease {
                handle,
                raster_generation: generation,
                lease_token: token,
                ..Default::default()
            },
            data_address: (*buffer).memory.addr() as u64,
            byte_length: (*buffer).bytes as u64,
            format: (*resource).info.format,
            width: (*resource).info.width,
            height: (*resource).info.height,
            stride_bytes: (*resource).info.stride_bytes,
            data_offset: (*resource).info.data_offset,
            ..Default::default()
        });
        drop(metadata);
        c::gui_frame_result_ok
    }
}
pub(super) unsafe fn release(consumer: Owner, lease: c::GuiSharedRasterLease) -> i32 {
    unsafe {
        let Some(_operation) = threads::Operation::enter() else {
            return c::gui_frame_error_state;
        };
        if lease.version != c::gui_shared_raster_lease_version
            || lease.size != c::gui_shared_raster_lease_size
            || !valid_handle(lease.handle)
            || lease.raster_generation == 0
            || lease.lease_token == 0
            || lease.reserved0 != 0
        {
            return c::gui_frame_error_invalid;
        }
        let mut free = None;
        let metadata = Metadata::acquire();
        let mut found = ptr::null_mut();
        for index in 0..LEASES {
            let record = ptr::addr_of_mut!((*state()).leases[index]);
            if (*record).used && (*record).lease_token == lease.lease_token {
                found = record;
                break;
            }
        }
        if found.is_null() {
            return c::gui_frame_error_stale;
        }
        if !registry::equal((*found).consumer, consumer)
            || !equal((*found).handle, lease.handle)
            || (*found).raster_generation != lease.raster_generation
        {
            return c::gui_frame_error_invalid;
        }
        let owner = (*found).producer;
        let resource = resource((*found).handle);
        if resource.is_null() {
            found.write(EMPTY_LEASE);
            return c::gui_frame_error_stale;
        }
        let buffer = ptr::addr_of_mut!((*resource).buffers[(*found).buffer_index as usize]);
        if (*buffer).raster_generation != (*found).raster_generation || (*buffer).lease_refs == 0 {
            found.write(EMPTY_LEASE);
            return c::gui_frame_error_stale;
        }
        (*buffer).lease_refs -= 1;
        assert!(r4os_zig_program_shared_unmap((*found).backing_lease));
        found.write(EMPTY_LEASE);
        let stats = stats_locked(owner, false);
        if !stats.is_null() {
            (*stats).release_count = (*stats).release_count.wrapping_add(1);
        }
        if can_free(resource) {
            free = Some(clear_resource(resource));
        }
        drop(metadata);
        if let Some(free) = free {
            free_memories(free);
        }
        c::gui_frame_result_ok
    }
}
pub(super) unsafe fn release_process(owner: Owner) {
    unsafe {
        let Some(_operation) = threads::Operation::enter() else {
            return;
        };
        let mut sets = [MaybeUninit::<FreeSet>::uninit(); RESOURCES];
        let mut count = 0;
        let metadata = Metadata::acquire();
        for index in 0..LEASES {
            let record = ptr::addr_of_mut!((*state()).leases[index]);
            if !(*record).used || !registry::equal((*record).consumer, owner) {
                continue;
            }
            let resource = resource((*record).handle);
            if !resource.is_null() {
                let buffer =
                    ptr::addr_of_mut!((*resource).buffers[(*record).buffer_index as usize]);
                if (*buffer).raster_generation == (*record).raster_generation
                    && (*buffer).lease_refs != 0
                {
                    (*buffer).lease_refs -= 1;
                }
                let stats = stats_locked((*record).producer, false);
                if !stats.is_null() {
                    (*stats).release_count = (*stats).release_count.wrapping_add(1);
                }
            }
            assert!(r4os_zig_program_shared_unmap((*record).backing_lease));
            record.write(EMPTY_LEASE);
        }
        for index in 0..RESOURCES {
            let resource = ptr::addr_of_mut!((*state()).resources[index]);
            if (*resource).used && registry::equal((*resource).owner, owner) {
                (*resource).closing = true;
                for index in 0..BUFFERS {
                    end_write(ptr::addr_of_mut!((*resource).buffers[index]));
                }
            }
            if (*resource).used && can_free(resource) {
                sets[count].write(clear_resource(resource));
                count += 1;
            }
        }
        for index in 0..STAT_SLOTS {
            let stats = ptr::addr_of_mut!((*state()).stats[index]);
            if (*stats).used && registry::equal((*stats).owner, owner) {
                stats.write(EMPTY_STATS);
            }
        }
        r4os_zig_program_shared_stopped(0, owner);
        drop(metadata);
        for op in 1..=7 {
            r4os_zig_program_shared_stopped(op, owner);
        }
        for set in &sets[..count] {
            free_memories(set.assume_init());
        }
        r4os_zig_program_shared_stopped(8, owner);
    }
}
pub(super) fn snapshot(owner: Owner) -> Stats {
    let _metadata = Metadata::acquire();
    unsafe {
        let stats = stats_locked(owner, false);
        if stats.is_null() {
            EMPTY_STATS
        } else {
            stats.read()
        }
    }
}
pub(super) fn empty() -> bool {
    let _metadata = Metadata::acquire();
    unsafe {
        for index in 0..RESOURCES {
            if (*state()).resources[index].used {
                return false;
            }
        }
        for index in 0..LEASES {
            if (*state()).leases[index].used {
                return false;
            }
        }
        for index in 0..FRAMES {
            if (*state()).frames[index].used {
                return false;
            }
        }
        for index in 0..STAT_SLOTS {
            if (*state()).stats[index].used {
                return false;
            }
        }
        true
    }
}
pub(super) unsafe fn note_commit(owner: Owner, frame: *const t::Frame) {
    let _metadata = Metadata::acquire();
    unsafe {
        let mut avoided = 0u64;
        for index in 0..FRAMES {
            let record = (*state()).frames[index];
            if record.used && record.frame.cast_const() == frame {
                avoided = avoided.saturating_add(record.reference.data_bytes);
            }
        }
        let stats = stats_locked(owner, false);
        if !stats.is_null() {
            (*stats).frame_bytes_avoided = (*stats).frame_bytes_avoided.saturating_add(avoided);
        }
    }
}
unsafe fn export_raster(
    consumer: Owner,
    input: *const c::GuiSharedRasterLease,
    out: *mut c::GfxBufferReference,
) -> i32 {
    unsafe {
        if input.is_null()
            || out.is_null()
            || (*out).version != 1
            || (*out).size < mem::size_of::<c::GfxBufferReference>() as u32
        {
            return c::gfx_buffer_error_invalid;
        }
        let input = input.read();
        if input.version != c::gui_shared_raster_lease_version
            || input.size != c::gui_shared_raster_lease_size
            || input.reserved0 != 0
        {
            return c::gfx_buffer_error_invalid;
        }
        let Some(_operation) = threads::Operation::enter() else {
            return c::gfx_buffer_error_busy;
        };
        let metadata = match Metadata::prepared(1, 0) {
            Ok(metadata) => metadata,
            Err(error) => return error.status,
        };
        let mut value = MaybeUninit::<c::GfxBufferReference>::uninit();
        let mut found = false;
        for index in 0..LEASES {
            let record = (*state()).leases[index];
            if !record.used
                || record.lease_token != input.lease_token
                || !registry::equal(record.consumer, consumer)
                || !equal(record.handle, input.handle)
                || record.raster_generation != input.raster_generation
            {
                continue;
            }
            let mut buffer = MaybeUninit::uninit();
            if !r4os_zig_program_shared_use(record.backing_lease, buffer.as_mut_ptr()) {
                return c::gfx_buffer_error_stale;
            }
            let mut reference = MaybeUninit::uninit();
            let status = r4os_zig_program_shared_import(
                buffer.assume_init(),
                consumer,
                false,
                true,
                reference.as_mut_ptr(),
            );
            if status != 0 {
                return status;
            }
            assert!(r4os_zig_program_shared_reference(
                reference.assume_init(),
                consumer,
                value.as_mut_ptr()
            ));
            found = true;
            break;
        }
        if !found {
            return c::gfx_buffer_error_stale;
        }
        drop(metadata);
        out.write(value.assume_init());
        c::gfx_buffer_result_ok
    }
}
unsafe fn api_create(info: *const c::GuiSharedRasterCreateInfo, out: *mut Handle) -> i32 {
    unsafe {
        if info.is_null() || out.is_null() {
            return c::gui_frame_error_invalid;
        }
        let Some(owner) = context::handle() else {
            return c::gui_frame_error_unavailable;
        };
        create(owner, info.read(), out)
    }
}
unsafe fn api_destroy(handle: *const Handle) -> i32 {
    unsafe {
        if handle.is_null() {
            return c::gui_frame_error_invalid;
        }
        let Some(owner) = context::handle() else {
            return c::gui_frame_error_unavailable;
        };
        destroy(owner, handle.read())
    }
}
unsafe fn api_write(handle: *const Handle, out: *mut c::GuiSharedRasterWriteMap) -> i32 {
    unsafe {
        if handle.is_null()
            || out.is_null()
            || (*out).version < c::gui_shared_raster_write_map_version
            || (*out).size < c::gui_shared_raster_write_map_size
        {
            return c::gui_frame_error_invalid;
        }
        let Some(owner) = context::handle() else {
            return c::gui_frame_error_unavailable;
        };
        map_write(owner, handle.read(), out)
    }
}
unsafe fn api_publish(map: *const c::GuiSharedRasterWriteMap, out: *mut u64) -> i32 {
    unsafe {
        if map.is_null() || out.is_null() {
            return c::gui_frame_error_invalid;
        }
        let Some(owner) = context::handle() else {
            return c::gui_frame_error_unavailable;
        };
        publish(owner, map.read(), out)
    }
}
unsafe fn api_acquire(
    owner: *const Owner,
    frame_generation: u64,
    handle: *const Handle,
    generation: u64,
    out: *mut c::GuiSharedRasterMap,
) -> i32 {
    unsafe {
        if owner.is_null()
            || handle.is_null()
            || out.is_null()
            || frame_generation == 0
            || generation == 0
        {
            return c::gui_frame_error_invalid;
        }
        if (*out).version < c::gui_shared_raster_map_version
            || (*out).size < c::gui_shared_raster_map_size
        {
            return c::gui_frame_error_invalid;
        }
        let Some(consumer) = context::handle() else {
            return c::gui_frame_error_unavailable;
        };
        let owner = owner.read();
        let handle = handle.read();
        if !registry::valid(owner) || !valid_handle(handle) {
            return c::gui_frame_error_invalid;
        }
        let Some(lease) = Lease::handle(owner) else {
            return c::gui_frame_error_stale;
        };
        let instance = lease.0.instance;
        let Some(capture) = gui_flow::capture(instance, Some(frame_generation), false) else {
            return c::gui_frame_error_stale;
        };
        let reader = gui_view::Reader { instance, capture };
        if !has_reference(reader.capture.frame, handle, generation) {
            return c::gui_frame_error_invalid;
        }
        acquire(consumer, owner, frame_generation, handle, generation, out)
    }
}
unsafe fn api_release(lease: *const c::GuiSharedRasterLease) -> i32 {
    unsafe {
        if lease.is_null() {
            return c::gui_frame_error_invalid;
        }
        let Some(owner) = context::handle() else {
            return c::gui_frame_error_unavailable;
        };
        release(owner, lease.read())
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_debug() -> *mut Debug {
    unsafe { ptr::addr_of_mut!((*state()).debug) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_failure(value: u32, present: bool, reset_cursor: bool) {
    unsafe {
        (*state()).debug.failure_value = value;
        (*state()).debug.failure_present = present;
        if reset_cursor {
            (*state()).debug.failure_cursor = 0;
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_lock() {
    lock();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_valid(handle: Handle) -> bool {
    valid_handle(handle)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_equal(a: Handle, b: Handle) -> bool {
    equal(a, b)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_info_valid(info: c::GuiSharedRasterCreateInfo) -> bool {
    valid_info(info)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_graphics_owner(owner: Owner) -> GraphicsOwner {
    graphics_owner(owner)
}
/// # Safety
/// Original private allocation owner and exclusive successful handle output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_create(
    owner: Owner,
    info: c::GuiSharedRasterCreateInfo,
    out: *mut Handle,
) -> i32 {
    unsafe { create(owner, info, out) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_destroy(owner: Owner, handle: Handle) -> i32 {
    unsafe { destroy(owner, handle) }
}
/// # Safety
/// Exclusive canonical write-map output and exact retained producer owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_write(
    owner: Owner,
    handle: Handle,
    out: *mut c::GuiSharedRasterWriteMap,
) -> i32 {
    unsafe { map_write(owner, handle, out) }
}
/// # Safety
/// Exact producer map token and exclusive successful generation output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_publish(
    owner: Owner,
    map: c::GuiSharedRasterWriteMap,
    out: *mut u64,
) -> i32 {
    unsafe { publish(owner, map, out) }
}
/// # Safety
/// Exact retained owner/private frame and exclusive successful reference output.
/// Mask bits distinguish absent optional values from explicit Some0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_pin(
    owner: Owner,
    frame: *mut t::Frame,
    handle: Handle,
    generation: u64,
    format: u32,
    width: u32,
    height: u32,
    bytes: u64,
    mask: u8,
    out: *mut t::SharedRasterRef,
) -> bool {
    unsafe {
        let Some(reference) = pin_exact(
            owner,
            frame,
            handle,
            generation,
            if mask & 1 != 0 { Some(format) } else { None },
            if mask & 2 != 0 { Some(width) } else { None },
            if mask & 4 != 0 { Some(height) } else { None },
            if mask & 8 != 0 { Some(bytes) } else { None },
        ) else {
            return false;
        };
        out.write(reference);
        true
    }
}
/// # Safety
/// Exact retained owner/private frame and exclusive successful reference output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_pin_frame(
    owner: Owner,
    frame: *mut t::Frame,
    descriptor: c::GuiSharedRasterResource,
    out: *mut t::SharedRasterRef,
) -> bool {
    unsafe {
        let Some(reference) = pin_frame(owner, frame, descriptor) else {
            return false;
        };
        out.write(reference);
        true
    }
}
/// # Safety
/// Exact retained source and unpublished target frame under original GUI admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_copy(
    owner: Owner,
    source: *const t::Frame,
    target: *mut t::Frame,
) -> bool {
    unsafe { copy_frames(owner, source, target) }
}
/// # Safety
/// Exact private delta/target with successful node/byte overflow preflight.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_move(
    source: *mut t::Frame,
    target: *mut t::Frame,
) -> bool {
    unsafe { move_frames(source, target) }
}
/// # Safety
/// Exact frame with exclusive immutable-reference release ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_release_frames(frame: *mut t::Frame) -> bool {
    unsafe { release_frames(frame) }
}
/// # Safety
/// Exact retained immutable frame/generation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_has(
    frame: *const t::Frame,
    handle: Handle,
    generation: u64,
) -> bool {
    unsafe { has_reference(frame, handle, generation) }
}
/// # Safety
/// Exact consumer/frame-owner/generations and exclusive canonical map output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_acquire(
    consumer: Owner,
    owner: Owner,
    frame_generation: u64,
    handle: Handle,
    generation: u64,
    out: *mut c::GuiSharedRasterMap,
) -> i32 {
    unsafe { acquire(consumer, owner, frame_generation, handle, generation, out) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_release(
    consumer: Owner,
    lease: c::GuiSharedRasterLease,
) -> i32 {
    unsafe { release(consumer, lease) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_release_process(owner: Owner) {
    unsafe {
        release_process(owner);
    }
}
/// # Safety
/// Exclusive readonly snapshot output of the original compiler-checked88-byte type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_stats(owner: Owner, out: *mut Stats) {
    unsafe {
        out.write(snapshot(owner));
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_shared_empty() -> bool {
    empty()
}
/// # Safety
/// Exact retained immutable frame under the original publication lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_note(owner: Owner, frame: *const t::Frame) {
    unsafe {
        note_commit(owner, frame);
    }
}
/// # Safety
/// Original optional input/output pointers; validation retains their original order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_api_create(
    info: *const c::GuiSharedRasterCreateInfo,
    out: *mut Handle,
) -> i32 {
    unsafe { api_create(info, out) }
}
/// # Safety
/// Readable canonical handle pointer when present.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_api_destroy(handle: *const Handle) -> i32 {
    unsafe { api_destroy(handle) }
}
/// # Safety
/// Original readable handle and exclusive canonical in/out write-map prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_api_write(
    handle: *const Handle,
    out: *mut c::GuiSharedRasterWriteMap,
) -> i32 {
    unsafe { api_write(handle, out) }
}
/// # Safety
/// Original readable canonical map and exclusive generation output when present.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_api_publish(
    map: *const c::GuiSharedRasterWriteMap,
    out: *mut u64,
) -> i32 {
    unsafe { api_publish(map, out) }
}
/// # Safety
/// Original readable owner/handle and exclusive canonical in/out map prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_api_acquire(
    owner: *const Owner,
    frame_generation: u64,
    handle: *const Handle,
    generation: u64,
    out: *mut c::GuiSharedRasterMap,
) -> i32 {
    unsafe { api_acquire(owner, frame_generation, handle, generation, out) }
}
/// # Safety
/// Original readable canonical lease pointer when present.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_api_release(
    lease: *const c::GuiSharedRasterLease,
) -> i32 {
    unsafe { api_release(lease) }
}
/// # Safety
/// Readable canonical lease and exclusive versioned BO reference output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_export(
    consumer: Owner,
    input: *const c::GuiSharedRasterLease,
    out: *mut c::GfxBufferReference,
) -> i32 {
    unsafe { export_raster(consumer, input, out) }
}
/// # Safety
/// Original current consumer context and readable/exclusive canonical pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_shared_api_export(
    input: *const c::GuiSharedRasterLease,
    out: *mut c::GfxBufferReference,
) -> i32 {
    unsafe {
        let Some(consumer) = context::handle() else {
            return c::gfx_buffer_error_unavailable;
        };
        export_raster(consumer, input, out)
    }
}
const _: () = {
    assert!(mem::size_of::<BackendHandle>() == 16);
    assert!(mem::align_of::<BackendHandle>() == 8);
    assert!(mem::offset_of!(BackendHandle, generation) == 0);
    assert!(mem::offset_of!(BackendHandle, id) == 8);
    assert!(mem::offset_of!(BackendHandle, reserved) == 12);
    assert!(mem::size_of::<Created>() == 48);
    assert!(mem::offset_of!(Created, buffer) == 0);
    assert!(mem::offset_of!(Created, reference) == 16);
    assert!(mem::offset_of!(Created, address) == 32);
    assert!(mem::offset_of!(Created, bytes) == 40);
    assert!(mem::size_of::<Prepared>() == 8);
    assert!(mem::offset_of!(Prepared, status) == 0);
    assert!(mem::offset_of!(Prepared, out_of_memory) == 4);
    assert!(mem::offset_of!(Prepared, reserved) == 5);
    assert!(mem::size_of::<GraphicsOwner>() == 24);
    assert!(mem::offset_of!(GraphicsOwner, id) == 0);
    assert!(mem::offset_of!(GraphicsOwner, generation) == 8);
    assert!(mem::offset_of!(GraphicsOwner, kind) == 16);
    assert!(mem::offset_of!(GraphicsOwner, reserved) == 20);
    assert!(mem::size_of::<Debug>() == 16);
    assert!(mem::align_of::<Debug>() == 4);
    assert!(mem::offset_of!(Debug, failure_value) == 0);
    assert!(mem::offset_of!(Debug, failure_cursor) == 4);
    assert!(mem::offset_of!(Debug, failure_present) == 8);
    assert!(mem::offset_of!(Debug, reserved) == 9);
    assert!(mem::size_of::<Stats>() == 88);
    assert!(mem::align_of::<Stats>() == 8);
    assert!(mem::offset_of!(Stats, owner) == 0);
    assert!(mem::offset_of!(Stats, publish_count) == 16);
    assert!(mem::offset_of!(Stats, acquire_count) == 24);
    assert!(mem::offset_of!(Stats, release_count) == 32);
    assert!(mem::offset_of!(Stats, backpressure_count) == 40);
    assert!(mem::offset_of!(Stats, published_bytes) == 48);
    assert!(mem::offset_of!(Stats, frame_bytes_avoided) == 56);
    assert!(mem::offset_of!(Stats, acquired_bytes) == 64);
    assert!(mem::offset_of!(Stats, live_bytes) == 72);
    assert!(mem::offset_of!(Stats, used) == 80);
    assert!(mem::offset_of!(Stats, reserved) == 81);
};
