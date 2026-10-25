//! Native early R4D boot-display hold. The original display and BO owners keep
//! their admission/resource operations behind typed projections until .22/.23.
//! This module alone owns the bridge, exact execution guard and capture loops.
use super::framebuffer::{self as fb, Framebuffer};
pub use crate::gfx::types::{Handle, Owner};
use crate::{
    contract as a,
    driver::{api_admission, irq_router},
    platform::timer,
    sched::sync_types::{Text, UnwindGuard, WaitQueue},
};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub frame: Framebuffer,
    pub mapping_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Description {
    pub bytes: u64,
    pub usage: u32,
    pub is_bytes: bool,
    pub is_system: bool,
    pub portable: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Mapping {
    pub lease: Handle,
    pub address: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HoldResult {
    pub generation: u64,
    pub captured: bool,
    pub retained: bool,
}
#[derive(Clone, Copy)]
struct Bridge {
    identity: Owner,
    request: a::GfxBootHoldRequest,
    reference: Handle,
    lease: Handle,
    address: u64,
    bytes: u64,
    held_generation: u64,
    width: u32,
    height: u32,
    pitch: u32,
    captured: bool,
}
impl Bridge {
    fn new(identity: Owner, request: a::GfxBootHoldRequest) -> Self {
        Self {
            identity,
            request,
            reference: Handle::default(),
            lease: Handle::default(),
            address: 0,
            bytes: 0,
            held_generation: 0,
            width: 0,
            height: 0,
            pitch: 0,
            captured: false,
        }
    }
}
struct Storage<T>(UnsafeCell<T>);
// SAFETY: the exact native boot-display execution guard owns bridge access.
// Synchronous display callbacks consume that same guard; adopted callbacks must
// enter it independently. No mutable Rust bridge reference spans another owner.
unsafe impl<T> Sync for Storage<T> {}
static EXECUTION: Storage<UnwindGuard> = Storage(UnsafeCell::new(UnwindGuard {
    owner: 0,
    owner_generation: 0,
    boot_owner: false,
    depth: 0,
    name: Text::literal(b"boot-display-driver"),
    queue: WaitQueue::EMPTY,
    ..UnwindGuard::EMPTY
}));
static BRIDGE: Storage<Option<Bridge>> = Storage(UnsafeCell::new(None));
struct Execution;
impl Execution {
    fn enter() -> Option<Self> {
        unsafe {
            if !super::ownership::raw_guard(EXECUTION.0.get(), 2, 0) {
                return None;
            }
            if ptr::addr_of!((*EXECUTION.0.get()).depth).read() != 1 {
                let _ = super::ownership::raw_guard(EXECUTION.0.get(), 3, 0);
                return None;
            }
            Some(Self)
        }
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        unsafe {
            core::arch::asm!("sfence", options(nostack, preserves_flags));
            let _ = super::ownership::raw_guard(EXECUTION.0.get(), 3, 0);
        }
    }
}

struct BufferLock;
impl BufferLock {
    fn acquire(prepared: bool) -> Option<Self> {
        if prepared {
            crate::gfx::buffers::lock_prepared(crate::gfx::buffers::Need {
                references: 1,
                leases: 1,
                objects: 0,
            })
            .ok()?;
        } else {
            crate::gfx::buffers::lock();
        }
        Some(Self)
    }
}
impl Drop for BufferLock {
    fn drop(&mut self) {
        crate::gfx::buffers::unlock()
    }
}
fn current() -> Option<Bridge> {
    unsafe { BRIDGE.0.get().read() }
}
fn publish(value: Option<Bridge>) {
    unsafe { BRIDGE.0.get().write(value) }
}
unsafe fn valid_output(out: *const a::GfxNativeState) -> bool {
    !out.is_null()
        && unsafe {
            (*out).version == 1 && (*out).size as usize >= core::mem::size_of::<a::GfxNativeState>()
        }
}
unsafe fn output(out: *mut a::GfxNativeState, generation: u64, outcome: u32, retained: bool) {
    unsafe {
        out.write(a::GfxNativeState {
            version: 1,
            size: core::mem::size_of::<a::GfxNativeState>() as u32,
            generation,
            state: native_state(),
            outcome,
            retained: u32::from(retained),
            reserved0: 0,
        })
    }
}
unsafe fn snapshot(saved: *const c_void) -> Snapshot {
    let mut out = MaybeUninit::uninit();
    unsafe {
        native_snapshot(saved, out.as_mut_ptr());
        out.assume_init()
    }
}
/// # Safety
/// Exact caller identity and resident request/output follow the existing R4D
/// admission. Output version/size is initialized before this operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_driver_hold(
    identity: *const Owner,
    input: *const a::GfxBootHoldRequest,
    out: *mut a::GfxNativeState,
) -> i32 {
    if irq_router::in_dispatch()
        || input.is_null()
        || unsafe {
            (*input).version != 1
                || ((*input).size as usize) < core::mem::size_of::<a::GfxBootHoldRequest>()
                || !valid_output(out)
        }
    {
        return a::gfx_output_error_invalid;
    }
    let Some(_execution) = Execution::enter() else {
        return a::gfx_output_error_busy;
    };
    if current().is_some() {
        return a::gfx_output_error_busy;
    }
    let identity = unsafe { identity.read() };
    let request = unsafe { input.read() };
    if identity.kind != 2
        || identity.id == 0
        || identity.generation == 0
        || request.reserved0 != 0
        || request.reference.reserved0 != 0
        || request.generation == 0
        || !unsafe { native_adapter(request.adapter_id) }
        || request.restore_callback < 0xffff800000000000
    {
        return a::gfx_output_error_invalid;
    }
    publish(Some(Bridge::new(identity, request)));
    let mut result = HoldResult::default();
    let code = unsafe { native_hold(&identity, &request, &mut result) };
    if code != 0 {
        publish(None);
        return code;
    }
    if result.retained {
        let mut b = current().expect("Retained exact boot bridge");
        b.held_generation = result.generation;
        publish(Some(b));
    } else {
        publish(None)
    }
    unsafe {
        output(
            out,
            result.generation,
            if result.captured {
                a::gfx_output_outcome_validated
            } else if result.retained {
                a::gfx_output_outcome_lost
            } else {
                a::gfx_output_outcome_old_preserved
            },
            result.retained,
        )
    }
    a::gfx_output_ok
}
/// # Safety
/// Caller retains the exact identity and initialized native-state output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_driver_finish(
    identity: *const Owner,
    generation: u64,
    operation: u32,
    out: *mut a::GfxNativeState,
) -> i32 {
    if irq_router::in_dispatch() || !unsafe { valid_output(out) } {
        return a::gfx_output_error_invalid;
    }
    let Some(_execution) = Execution::enter() else {
        return a::gfx_output_error_busy;
    };
    let Some(current) = current() else {
        return a::gfx_output_error_stale;
    };
    let identity = unsafe { identity.read() };
    if current.identity != identity {
        return a::gfx_output_error_stale;
    }
    if operation == 1 && !current.captured {
        return a::gfx_output_error_invalid;
    }
    let mut result = 0u8;
    let code = unsafe { native_finish(identity.id, generation, operation, &mut result) };
    if code != 0 {
        return code;
    }
    if result == 2 {
        unsafe { output(out, generation, a::gfx_output_outcome_lost, true) }
        return a::gfx_output_ok;
    }
    let released = result == 1;
    if released {
        publish(None)
    }
    unsafe {
        output(
            out,
            generation,
            if released {
                a::gfx_output_outcome_old_preserved
            } else {
                a::gfx_output_outcome_validated
            },
            !released,
        )
    }
    a::gfx_output_ok
}
/// # Safety
/// Request/output and exact owner remain resident through existing preparation;
/// the registration callback ABI and hardware admission remain unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_driver_prepare(
    identity: *const Owner,
    input: *const a::GfxNativeRegistration,
    generation: u64,
    reset: u64,
    is_reset: bool,
    out: *mut a::GfxNativeState,
) -> i32 {
    if irq_router::in_dispatch()
        || input.is_null()
        || unsafe {
            (*input).version != 1
                || ((*input).size as usize) < core::mem::size_of::<a::GfxNativeRegistration>()
                || !valid_output(out)
        }
    {
        return a::gfx_output_error_invalid;
    }
    let Some(_execution) = Execution::enter() else {
        return a::gfx_output_error_busy;
    };
    let identity = unsafe { identity.read() };
    let adapter = unsafe { ptr::addr_of!((*input).backend.adapter_id).read() };
    if !is_reset || generation != 0 {
        let Some(current) = current() else {
            return a::gfx_output_error_stale;
        };
        if current.identity != identity
            || (!is_reset && generation == 0)
            || current.held_generation != generation
            || current.request.adapter_id != adapter
        {
            return a::gfx_output_error_stale;
        }
        if !current.captured || current.lease.id == 0 {
            return a::gfx_output_error_invalid;
        }
    } else if current().is_some() {
        return a::gfx_output_error_stale;
    }
    unsafe { native_prepare(&identity, input, generation, reset, is_reset, out) }
}
/// # Safety
/// Display callback holds the saved boot snapshot; caller owns the exact CPU
/// destination lease or proved recovery of the original scanout mapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_driver_copy(
    identity: *const Owner,
    generation: u64,
    saved: *const c_void,
    target: *const Framebuffer,
    include_padding: bool,
) -> bool {
    let Some(_execution) = Execution::enter() else {
        return false;
    };
    let Some(current) = current() else {
        return false;
    };
    let identity = unsafe { identity.read() };
    let saved = unsafe { snapshot(saved) };
    let target = unsafe { &*target };
    if current.identity != identity
        || generation == 0
        || current.held_generation != generation
        || !current.captured
        || current.lease.id == 0
        || current.width != saved.width
        || current.height != saved.height
        || current.pitch != saved.pitch
        || target.width != u64::from(current.width)
        || target.height != u64::from(current.height)
        || !fb::is_native_xrgb32(target)
        || target.pitch < u64::from(current.width) * 4
        || target.pitch & 3 != 0
        || target.address.addr() & 3 != 0
        || target.pitch > 256 * 1024 * 1024 / u64::from(current.height)
    {
        return false;
    }
    if include_padding
        && (target.pitch != u64::from(current.pitch) || target.address != saved.frame.address)
    {
        return false;
    }
    let words = if include_padding {
        current.pitch / 4
    } else {
        current.width
    };
    let source = current.address as *const u32;
    let destination = target.address.cast::<u32>();
    for y in 0..u64::from(current.height) {
        for x in 0..u64::from(words) {
            unsafe {
                destination
                    .add((y * (target.pitch / 4) + x) as usize)
                    .write_volatile(
                        source
                            .add((y * u64::from(current.pitch / 4) + x) as usize)
                            .read(),
                    );
            }
        }
    }
    true
}
/// # Safety
/// Called synchronously by display hold while the exact bridge guard is held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_driver_capture(saved: *const c_void) -> bool {
    let Some(mut current) = current() else {
        return false;
    };
    let saved = unsafe { snapshot(saved) };
    let bytes = u64::from(saved.pitch) * u64::from(saved.height);
    if bytes == 0
        || bytes > 256 * 1024 * 1024
        || bytes > saved.mapping_bytes
        || bytes & 3 != 0
        || saved.frame.address.addr() & 3 != 0
    {
        return false;
    }
    let handle = current.request.reference;
    if handle.id == 0 || handle.reserved0 != 0 || handle.generation == 0 {
        return false;
    }
    let caller = Handle {
        id: handle.id,
        generation: handle.generation,
    };
    let Some(lock) = BufferLock::acquire(true) else {
        return false;
    };
    let mut descriptor = Description::default();
    if !unsafe { buffer_describe(&caller, &current.identity, &mut descriptor) }
        || !descriptor.is_bytes
        || !descriptor.is_system
        || !descriptor.portable
        || descriptor.bytes != bytes
        || descriptor.usage & 3 != 3
    {
        return false;
    }
    if !unsafe { buffer_share(&caller, &current.identity, &mut current.reference) } {
        return false;
    }
    publish(Some(current));
    let mut mapped = Mapping::default();
    if !unsafe { buffer_map(&current.reference, true, bytes, &mut mapped) } {
        return false;
    }
    current.lease = mapped.lease;
    current.bytes = bytes;
    current.width = saved.width;
    current.height = saved.height;
    current.pitch = saved.pitch;
    current.address = mapped.address;
    publish(Some(current));
    drop(lock);
    let source = saved.frame.address.cast::<u32>();
    let target = current.address as *mut u32;
    for index in 0..bytes / 4 {
        unsafe {
            target
                .add(index as usize)
                .write(source.add(index as usize).read_volatile());
        }
    }
    let Some(_lock) = BufferLock::acquire(false) else {
        unreachable!("Blocking BO metadata lock")
    };
    if !unsafe { buffer_unmap(&current.lease) } {
        return false;
    }
    current.lease = Handle::default();
    publish(Some(current));
    if !unsafe { buffer_map(&current.reference, false, bytes, &mut mapped) } {
        return false;
    }
    current.lease = mapped.lease;
    publish(Some(current));
    if mapped.address != current.address {
        return false;
    }
    current.captured = true;
    publish(Some(current));
    true
}
/// # Safety
/// Exact synchronous holder or adopted owner keeps the bridge/snapshot live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_driver_restore(
    generation: u64,
    saved: *const c_void,
    adopted: bool,
) -> bool {
    let _execution = if adopted {
        let Some(e) = Execution::enter() else {
            return false;
        };
        Some(e)
    } else {
        None
    };
    let Some(current) = current() else {
        return false;
    };
    if !current.captured || current.lease.id == 0 {
        return false;
    }
    let owner = u32::try_from(current.identity.id).expect("Original R4D identity range");
    if !api_admission::r4os_rust_driver_api_enter(owner, u64::from(timer::frequency().max(1))) {
        return false;
    }
    let mut boot = MaybeUninit::uninit();
    unsafe { native_description(saved, generation, boot.as_mut_ptr()) }
    let callback: unsafe extern "C" fn(u64, u64, *const a::GfxNativeBootInfo) -> i32 =
        unsafe { core::mem::transmute(current.request.restore_callback as usize) };
    let restored = unsafe { callback(current.request.context, generation, boot.as_ptr()) } == 1;
    let _ = api_admission::r4os_rust_driver_api_leave();
    if !restored {
        return false;
    }
    let saved = unsafe { snapshot(saved) };
    let source = current.address as *const u32;
    let target = saved.frame.address.cast::<u32>();
    for index in 0..current.bytes / 4 {
        unsafe {
            target
                .add(index as usize)
                .write_volatile(source.add(index as usize).read());
        }
    }
    true
}
/// # Safety
/// Exact synchronous holder or independently admitted adopted owner releases
/// only this bridge's resources. Failed unmap/drop leaves them retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_driver_release(adopted: bool) -> bool {
    let _execution = if adopted {
        let Some(e) = Execution::enter() else {
            return false;
        };
        Some(e)
    } else {
        None
    };
    let Some(mut current) = current() else {
        return true;
    };
    let Some(lock) = BufferLock::acquire(false) else {
        unreachable!("Blocking BO metadata lock")
    };
    if current.lease.id != 0 {
        if !unsafe { buffer_unmap(&current.lease) } {
            return false;
        }
        current.lease = Handle::default();
        publish(Some(current));
    }
    drop(lock);
    if current.reference.id != 0 {
        if !unsafe { buffer_drop(&current.reference) } {
            return false;
        }
        current.reference = Handle::default();
        publish(Some(current));
    }
    publish(None);
    true
}

const _: () = {
    assert!(core::mem::size_of::<Owner>() == 24);
    assert!(core::mem::align_of::<Owner>() == 8);
    assert!(core::mem::offset_of!(Owner, kind) == 0);
    assert!(core::mem::offset_of!(Owner, id) == 8);
    assert!(core::mem::offset_of!(Owner, generation) == 16);
    assert!(core::mem::size_of::<Handle>() == 16);
    assert!(core::mem::align_of::<Handle>() == 8);
    assert!(core::mem::offset_of!(Handle, id) == 0);
    assert!(core::mem::offset_of!(Handle, generation) == 8);
    assert!(core::mem::size_of::<Snapshot>() == 88);
    assert!(core::mem::align_of::<Snapshot>() == 8);
    assert!(core::mem::offset_of!(Snapshot, frame) == 0);
    assert!(core::mem::offset_of!(Snapshot, mapping_bytes) == 64);
    assert!(core::mem::offset_of!(Snapshot, width) == 72);
    assert!(core::mem::offset_of!(Snapshot, height) == 76);
    assert!(core::mem::offset_of!(Snapshot, pitch) == 80);
    assert!(core::mem::size_of::<Description>() == 16);
    assert!(core::mem::align_of::<Description>() == 8);
    assert!(core::mem::offset_of!(Description, bytes) == 0);
    assert!(core::mem::offset_of!(Description, usage) == 8);
    assert!(core::mem::offset_of!(Description, is_bytes) == 12);
    assert!(core::mem::offset_of!(Description, is_system) == 13);
    assert!(core::mem::offset_of!(Description, portable) == 14);
    assert!(core::mem::size_of::<Mapping>() == 24);
    assert!(core::mem::align_of::<Mapping>() == 8);
    assert!(core::mem::offset_of!(Mapping, lease) == 0);
    assert!(core::mem::offset_of!(Mapping, address) == 16);
    assert!(core::mem::size_of::<HoldResult>() == 16);
    assert!(core::mem::align_of::<HoldResult>() == 8);
    assert!(core::mem::offset_of!(HoldResult, generation) == 0);
    assert!(core::mem::offset_of!(HoldResult, captured) == 8);
    assert!(core::mem::offset_of!(HoldResult, retained) == 9);
};

const DISPLAY_BUFFER_OWNER: crate::gfx::types::Owner = crate::gfx::types::Owner {
    kind: 0,
    id: 3,
    generation: 1,
};
fn buffer_handle(value: Handle) -> crate::gfx::types::Handle {
    crate::gfx::types::Handle {
        id: value.id,
        generation: value.generation,
    }
}
fn wire_handle(value: crate::gfx::types::Handle) -> Handle {
    Handle {
        id: value.id,
        generation: value.generation,
    }
}
unsafe fn buffer_describe(
    value: *const Handle,
    identity: *const Owner,
    out: *mut Description,
) -> bool {
    let identity = unsafe { identity.read() };
    let owner = crate::gfx::types::Owner {
        kind: identity.kind,
        id: identity.id,
        generation: identity.generation,
    };
    let Ok(d) =
        (unsafe { crate::gfx::buffers::read().describe(buffer_handle(value.read()), owner) })
    else {
        return false;
    };
    unsafe {
        out.write(Description {
            bytes: d.bytes,
            usage: d.usage,
            is_bytes: d.format == crate::gfx::layout::BYTES,
            is_system: d.location == 0,
            portable: d.binding.portable(),
        });
    }
    true
}
unsafe fn buffer_share(value: *const Handle, _: *const Owner, out: *mut Handle) -> bool {
    let Ok(h) = (unsafe {
        crate::gfx::buffers::table().share(buffer_handle(value.read()), DISPLAY_BUFFER_OWNER)
    }) else {
        return false;
    };
    unsafe {
        out.write(wire_handle(h));
    }
    true
}
unsafe fn buffer_map(value: *const Handle, write: bool, bytes: u64, out: *mut Mapping) -> bool {
    let Ok(value) = (unsafe {
        crate::gfx::buffers::map_locked(
            buffer_handle(value.read()),
            DISPLAY_BUFFER_OWNER,
            if write { 1 } else { 0 },
            0,
            bytes,
        )
    }) else {
        return false;
    };
    unsafe {
        out.write(Mapping {
            lease: wire_handle(value.lease),
            address: value.backing.cpu_address,
        });
    }
    true
}
unsafe fn buffer_unmap(value: *const Handle) -> bool {
    unsafe {
        crate::gfx::buffers::unmap_cpu_locked(buffer_handle(value.read()), DISPLAY_BUFFER_OWNER)
    }
    .is_ok()
}
unsafe fn buffer_drop(value: *const Handle) -> bool {
    crate::gfx::buffers::drop_reference(
        unsafe { buffer_handle(value.read()) },
        DISPLAY_BUFFER_OWNER,
    )
    .is_ok()
}

unsafe fn native_adapter(id: u32) -> bool {
    super::native_driver::valid_adapter(id)
}
unsafe fn native_state() -> u32 {
    super::manager::backend_state().state
}
unsafe extern "C" fn capture_holder(
    _: usize,
    saved: *const super::core_types::BootSnapshot,
) -> bool {
    unsafe { r4os_rust_boot_driver_capture(saved.cast()) }
}
unsafe extern "C" fn restore_holder(
    _: usize,
    generation: u64,
    saved: *const super::core_types::BootSnapshot,
) -> bool {
    unsafe { r4os_rust_boot_driver_restore(generation, saved.cast(), false) }
}
unsafe extern "C" fn restore_adopted(
    _: usize,
    generation: u64,
    saved: *const super::core_types::BootSnapshot,
) -> bool {
    unsafe { r4os_rust_boot_driver_restore(generation, saved.cast(), true) }
}
unsafe extern "C" fn release_holder(_: usize) -> bool {
    unsafe { r4os_rust_boot_driver_release(false) }
}
unsafe extern "C" fn release_adopted(_: usize) -> bool {
    unsafe { r4os_rust_boot_driver_release(true) }
}
unsafe fn native_hold(
    identity: *const Owner,
    request: *const a::GfxBootHoldRequest,
    out: *mut HoldResult,
) -> i32 {
    use super::core_types as c;
    let identity = unsafe { identity.read() };
    let request = unsafe { request.read() };
    let holder = c::Holder {
        owner: identity.id as usize,
        adapter_id: request.adapter_id,
        expected_generation: request.generation,
        context: 0,
        capture: c::Capture(capture_holder),
        restore: c::Restore(restore_holder),
        restore_adopted: Some(c::Restore(restore_adopted)),
        release: c::Release(release_holder),
        release_adopted: Some(c::Release(release_adopted)),
    };
    match super::manager::hold_boot(holder) {
        Ok(result) => {
            unsafe {
                out.write(HoldResult {
                    generation: result.generation,
                    captured: result.captured,
                    retained: result.retained,
                })
            };
            0
        }
        Err(e) => super::native_driver::code(e),
    }
}
unsafe fn native_finish(id: u64, generation: u64, operation: u32, out: *mut u8) -> i32 {
    match super::manager::finish_boot(id as usize, generation, operation) {
        Ok(released) => {
            unsafe { out.write(u8::from(released)) };
            0
        }
        Err(super::error::Error::RestoreFailed) => {
            unsafe { out.write(2) };
            0
        }
        Err(e) => super::native_driver::code(e),
    }
}
unsafe fn native_prepare(
    identity: *const Owner,
    input: *const a::GfxNativeRegistration,
    generation: u64,
    reset: u64,
    is_reset: bool,
    out: *mut a::GfxNativeState,
) -> i32 {
    let id = unsafe { identity.read() };
    let identity = crate::gfx::types::Owner {
        kind: id.kind,
        id: id.id,
        generation: id.generation,
    };
    if is_reset && reset == 0 || !is_reset && generation == 0 {
        return a::gfx_output_error_stale;
    }
    unsafe {
        super::native_driver::prepare_request(
            identity,
            input,
            generation,
            if is_reset { reset } else { 0 },
            out,
        )
    }
}
unsafe fn native_snapshot(saved: *const c_void, out: *mut Snapshot) {
    let saved = unsafe { saved.cast::<super::core_types::BootSnapshot>().read() };
    unsafe {
        out.write(Snapshot {
            frame: saved.framebuffer,
            mapping_bytes: saved.mapping.byte_len,
            width: saved.mode.width,
            height: saved.mode.height,
            pitch: saved.mode.pitch,
        })
    }
}
unsafe fn native_description(
    saved: *const c_void,
    generation: u64,
    out: *mut a::GfxNativeBootInfo,
) {
    let saved = unsafe { saved.cast::<super::core_types::BootSnapshot>().read() };
    unsafe { out.write(super::native_driver::boot_description(generation, &saved)) }
}
