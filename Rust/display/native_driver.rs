//! Sole privileged display bridge. The R4D owns physical device commands;
//! common display execution and the reentrant bridge guard retain exact BO,
//! mode, reset and restored-console identities until their real receipts.
#![allow(clippy::too_many_arguments)]
use super::{
    backend_state as backend,
    core_types::{self as core_display, Backend, BootSnapshot, Rect, Target, Text},
    cursor_work,
    equal::same,
    error::Error,
    framebuffer::{self, Framebuffer},
    manager as display, mode_work, native_additional_mode,
    native_types::ModeBinding,
    output_runtime, outputs,
    ownership::{self, Resident},
    queue,
    queue_resources::{self as resource, DISPLAY_OWNER as OWNER},
    queue_state as model,
};
use crate::{
    contract as a,
    driver::{api_admission, irq_router},
    gfx::{
        buffer_api, buffers, layout,
        types::{Handle, Owner},
    },
    memory::paging,
    platform::{monotonic, pci_inventory as pci, timer},
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    fmt::{self, Write},
    mem::size_of,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
type Callback = unsafe extern "C" fn(u64, u64, *const a::GfxNativeBootInfo) -> i32;
// The old inactive bridge had an undefined framebuffer. Keep initialized raw
// storage in Rust; it is never used before a real saved boot frame is bound.
const EMPTY_FRAME: Framebuffer = Framebuffer {
    address: ptr::null_mut(),
    width: 0,
    height: 0,
    pitch: 0,
    bpp: 0,
    memory_model: 0,
    red_mask_size: 0,
    red_mask_shift: 0,
    green_mask_size: 0,
    green_mask_shift: 0,
    blue_mask_size: 0,
    blue_mask_shift: 0,
    unused: [0; 5],
    edid_size: 0,
    edid: ptr::null(),
};
#[derive(Clone, Copy)]
struct Bridge {
    driver_owner: Owner,
    registration: a::GfxNativeRegistration,
    reference: Handle,
    cpu_lease: Handle,
    cpu_refresh_required: bool,
    cpu_retry_reported: bool,
    timeline: u64,
    pending: Option<model::Fence>,
    image_pending: Option<model::Fence>,
    image_direct: bool,
    bytes: u64,
    frame: Framebuffer,
    output_format: u32,
    encoded_output: bool,
    held_generation: u64,
    generation: u64,
    ready: bool,
    cancelled: bool,
    hardware_restored: bool,
    pixels_restored: bool,
    queue_stopped: bool,
    modes_enabled: bool,
    driver_reference: Handle,
}
impl Bridge {
    const EMPTY: Self = Self {
        driver_owner: Owner {
            kind: 2,
            id: 0,
            generation: 0,
        },
        registration: a::GfxNativeRegistration {
            version: 1,
            size: 128,
            backend: a::GfxBackendBinding {
                version: 1,
                size: 32,
                adapter_id: 0,
                milestone: 0,
                device_generation: 0,
                reset_generation: 0,
            },
            output: a::GfxOutputId {
                adapter_id: 0,
                connector_id: 0,
                device_generation: 0,
                connection_generation: 0,
            },
            reference: a::GfxBufferHandle {
                id: 0,
                reserved0: 0,
                generation: 0,
            },
            name: [0; 24],
            context: 0,
            commit_callback: 0,
            restore_callback: 0,
        },
        reference: Handle::EMPTY,
        cpu_lease: Handle::EMPTY,
        cpu_refresh_required: false,
        cpu_retry_reported: false,
        timeline: 0,
        pending: None,
        image_pending: None,
        image_direct: false,
        bytes: 0,
        frame: EMPTY_FRAME,
        output_format: layout::XRGB8888,
        encoded_output: false,
        held_generation: 0,
        generation: 0,
        ready: false,
        cancelled: false,
        hardware_restored: false,
        pixels_restored: false,
        queue_stopped: false,
        modes_enabled: false,
        driver_reference: Handle::EMPTY,
    };
}
#[derive(Clone, Copy)]
struct Surface {
    reference: Handle,
    driver_reference: Handle,
    read_lease: Handle,
    bytes: u64,
    frame: Framebuffer,
    output_format: u32,
    encoded_output: bool,
}
#[derive(Clone, Copy)]
struct Replacement {
    old: Surface,
    new: Surface,
    using_new: bool,
}
#[derive(Clone, Copy)]
struct ResetRecord {
    driver: Owner,
    backend: a::GfxBackendBinding,
    original_generation: u64,
    generation: u64,
    retired: bool,
    terminal_released: bool,
    resumed_backend: Option<a::GfxBackendBinding>,
    reported_step: ResetStep,
    reported_result: i32,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ResetStep {
    None,
    Queue,
    Mode,
    Cursor,
    Output,
    Display,
    Bridge,
    Retired,
}
impl ResetStep {
    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Queue => "queue",
            Self::Mode => "mode",
            Self::Cursor => "cursor",
            Self::Output => "output",
            Self::Display => "display",
            Self::Bridge => "bridge",
            Self::Retired => "retired",
        }
    }
}
#[derive(Clone, Copy)]
struct ConsoleReceipt {
    driver: Owner,
    adapter: u32,
    reset_generation: u64,
    restored_generation: u64,
}
static BRIDGE: Resident<Bridge> = Resident(UnsafeCell::new(Bridge::EMPTY));
static REPLACEMENT: Resident<Option<Replacement>> = Resident(UnsafeCell::new(None));
static RESET: Resident<Option<ResetRecord>> = Resident(UnsafeCell::new(None));
static CONSOLE: Resident<Option<ConsoleReceipt>> = Resident(UnsafeCell::new(None));
static EXECUTION: Resident<crate::sched::sync_types::UnwindGuard> = Resident(UnsafeCell::new(
    ownership::execution(b"native-display-driver"),
));
static RETAINED: AtomicU32 = AtomicU32::new(0);
fn execution() -> Result<ownership::RecursiveExecution, Error> {
    unsafe { ownership::RecursiveExecution::acquire(EXECUTION.0.get()) }
}
struct BoLock;
impl BoLock {
    fn acquire() -> Self {
        buffers::lock();
        Self
    }
    fn prepared(references: usize, leases: usize) -> Result<Self, Error> {
        buffers::lock_prepared(buffers::Need {
            objects: 0,
            references,
            leases,
        })?;
        Ok(Self)
    }
}
impl Drop for BoLock {
    fn drop(&mut self) {
        buffers::unlock()
    }
}
struct DisplayExecution;
impl DisplayExecution {
    fn acquire() -> Result<Self, Error> {
        if display::begin_output_commit() {
            Ok(Self)
        } else {
            Err(Error::Busy)
        }
    }
}
impl Drop for DisplayExecution {
    fn drop(&mut self) {
        display::end_output_commit()
    }
}
pub fn binding(value: a::GfxBackendBinding) -> model::Binding {
    model::Binding {
        adapter: value.adapter_id,
        device_generation: value.device_generation,
        reset_generation: value.reset_generation,
    }
}
pub fn retained(id: u32) -> bool {
    id != 0 && RETAINED.load(Ordering::Acquire) == id
}
// Read only under common DisplayExecution; replacement changes also require it.
pub fn mode_pending() -> bool {
    unsafe {
        match *REPLACEMENT.0.get() {
            None => false,
            Some(change) => {
                !change.using_new
                    || !mode_work::presentation_ready(
                        (*BRIDGE.0.get()).driver_owner,
                        (*BRIDGE.0.get()).registration.backend,
                    )
            }
        }
    }
}
pub fn code(error: Error) -> i32 {
    match error {
        Error::Busy => a::gfx_output_error_busy,
        Error::Stale | Error::WrongOwner => a::gfx_output_error_stale,
        Error::Disabled | Error::Unsupported => a::gfx_output_error_unsupported,
        Error::Unavailable => a::gfx_output_error_unavailable,
        Error::OutOfMemory | Error::Capacity => a::gfx_output_error_capacity,
        _ => a::gfx_output_error_invalid,
    }
}
// Log transport remains with the existing kernel log sink. This thunk has no
// state and preserves raw serial plus explicit boot-ring publication exactly.

struct Message<const N: usize> {
    bytes: [u8; N],
    len: usize,
}
impl<const N: usize> Message<N> {
    fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
    fn emit(&self) {
        unsafe { native_display_log(self.bytes.as_ptr(), self.len) }
    }
    fn text(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).expect("ASCII diagnostic")
    }
}
impl<const N: usize> Write for Message<N> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if text.len() > N - self.len {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + text.len()].copy_from_slice(text.as_bytes());
        self.len += text.len();
        Ok(())
    }
}
fn state_name(value: u32) -> &'static str {
    match value {
        0 => "unavailable",
        1 => "bootfb",
        2 => "preparing",
        3 => "native",
        4 => "software_native",
        5 => "recovering",
        _ => unreachable!(),
    }
}
fn reason_name(value: u32) -> &'static str {
    match value {
        0 => "none",
        1 => "no_native_backend",
        2 => "policy_disabled",
        3 => "backend_rejected",
        4 => "prepare_failed",
        5 => "commit_failed",
        6 => "device_lost",
        7 => "restore_failed",
        _ => unreachable!(),
    }
}
unsafe fn reset_result(reset: *mut ResetRecord, step: ResetStep, result: i32) -> i32 {
    unsafe {
        if (*reset).reported_step != step || (*reset).reported_result != result {
            (*reset).reported_step = step;
            (*reset).reported_result = result;
            let mut message = Message::<192>::new();
            writeln!(
                &mut message,
                "[GFX-RESET] owner={} generation={} retire={} result={}",
                (*reset).driver.id,
                (*reset).generation,
                step.name(),
                result
            )
            .expect("original reset bound");
            message.emit();
            let current = display::backend_state();
            let b = BRIDGE.0.get();
            let mut detail = Message::<320>::new();
            writeln!(&mut detail,"[GFX-RESET] display={}/{} owner={} generation={} adapter={} bridge-generation={} timeline={} reference={}/{} cpu-lease={} pending={}/{} replacement={}",
                state_name(current.state),reason_name(current.reason),current.owner,current.generation,current.adapter_id,
                (*b).generation,(*b).timeline,(*b).reference.id,(*b).driver_reference.id,(*b).cpu_lease.id,
                u8::from((*b).pending.is_some()),u8::from((*b).image_pending.is_some()),u8::from((*REPLACEMENT.0.get()).is_some()))
                .expect("original reset detail bound");
            detail.emit();
        }
    }
    result
}
fn restore_after_reset(id: u32, generation: u64) -> Result<(), Error> {
    unsafe {
        if (*RESET.0.get()).is_some_and(|r| !r.retired) {
            return Err(Error::Busy);
        }
        display::restore_boot_backend(id as usize, generation)?;
        *CONSOLE.0.get() = None;
        if let Some(reset) = *RESET.0.get() {
            let restored = display::backend_state();
            if reset.driver.id == u64::from(id)
                && reset.retired
                && !reset.terminal_released
                && reset.resumed_backend.is_none()
                && restored.state == backend::BOOTFB
                && restored.owner == 0
                && restored.generation > reset.generation
                && display::restored_boot_owner(
                    id as usize,
                    restored.generation,
                    reset.backend.adapter_id,
                )
                .unwrap_or(false)
            {
                *CONSOLE.0.get() = Some(ConsoleReceipt {
                    driver: reset.driver,
                    adapter: reset.backend.adapter_id,
                    reset_generation: reset.generation,
                    restored_generation: restored.generation,
                });
            }
        }
        *RESET.0.get() = None;
    }
    Ok(())
}
fn owns_restored_console(identity: Owner, generation: u64, adapter: u32) -> bool {
    unsafe {
        (*CONSOLE.0.get()).is_some_and(|receipt| {
            receipt.driver == identity
                && receipt.adapter == adapter
                && receipt.restored_generation == generation
                && generation > receipt.reset_generation
        })
    }
}
pub fn boot_description(generation: u64, saved: &BootSnapshot) -> a::GfxNativeBootInfo {
    let state = display::backend_state();
    let mut physical = 0;
    if !unsafe {
        paging::r4os_rust_paging_contiguous(
            saved.mapping.virt_base,
            saved.mapping.byte_len,
            &mut physical,
        )
    } {
        physical = 0
    }
    a::GfxNativeBootInfo {
        generation,
        physical_address: physical,
        byte_length: saved.mapping.byte_len,
        width: saved.mode.width,
        height: saved.mode.height,
        pitch: saved.mode.pitch,
        format: if framebuffer::is_native_xrgb32(&saved.framebuffer) {
            layout::XRGB8888
        } else {
            0
        },
        policy: state.policy,
        state: state.state,
        ..Default::default()
    }
}
/// # Safety
/// R4D retains the initialized output header and writable admitted prefix.
pub unsafe fn boot_info(out: *mut a::GfxNativeBootInfo) -> i32 {
    if irq_router::in_dispatch() || !unsafe { buffer_api::valid_output(out) } {
        return a::gfx_output_error_invalid;
    }
    let Some(saved) = display::boot_snapshot() else {
        return a::gfx_output_error_unavailable;
    };
    unsafe {
        out.write(boot_description(
            display::backend_state().generation,
            &saved,
        ))
    };
    a::gfx_output_ok
}
fn state_result(id: u32, outcome: u32) -> a::GfxNativeState {
    let state = display::backend_state();
    let generation = unsafe {
        if retained(id) && !(*BRIDGE.0.get()).ready {
            (*BRIDGE.0.get()).generation
        } else if state.pending_owner == id as usize {
            state.pending_generation
        } else {
            state.generation
        }
    };
    a::GfxNativeState {
        generation,
        state: state.state,
        outcome,
        retained: u32::from(display::retains_driver_owner(id as usize) || retained(id)),
        ..Default::default()
    }
}
pub fn valid_adapter(adapter: u32) -> bool {
    for index in 0..pci::count() {
        let Some(item) = pci::device_at(index) else {
            return false;
        };
        if item.class_code == 3
            && adapter
                == (0x0100_0000
                    | (u32::from(item.bus) << 8)
                    | (u32::from(item.device) << 3)
                    | u32::from(item.function))
        {
            return true;
        }
    }
    false
}
/// # Safety
/// Exact admitted R4D keeps request/output resident through preparation.
pub unsafe fn prepare_request(
    identity: Owner,
    input: *const a::GfxNativeRegistration,
    held_generation: u64,
    reset_generation: u64,
    out: *mut a::GfxNativeState,
) -> i32 {
    if input.is_null()
        || irq_router::in_dispatch()
        || unsafe {
            (*input).version != 1
                || ((*input).size as usize) < size_of::<a::GfxNativeRegistration>()
                || !buffer_api::valid_output(out)
        }
    {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    unsafe {
        if reset_generation != 0 {
            let Some(reset) = *RESET.0.get() else {
                return a::gfx_output_error_stale;
            };
            if reset.driver != identity
                || reset.generation != reset_generation
                || (*input).backend.adapter_id != reset.backend.adapter_id
                || (*input).backend.device_generation <= reset.backend.device_generation
            {
                return a::gfx_output_error_stale;
            }
            if !reset.retired {
                return a::gfx_output_error_busy;
            }
            if reset.terminal_released
                || reset
                    .resumed_backend
                    .is_some_and(|resumed| !same(&resumed, &(*input).backend))
            {
                return a::gfx_output_error_stale;
            }
        } else if (*RESET.0.get()).is_some() {
            return a::gfx_output_error_busy;
        }
        let result = match prepare_impl(identity, input.read(), held_generation, reset_generation) {
            Ok(result) => result,
            Err(error) => {
                if (*BRIDGE.0.get()).driver_owner == identity && !(*BRIDGE.0.get()).ready {
                    out.write(state_result(
                        driver_id(identity),
                        a::gfx_output_outcome_lost,
                    ));
                }
                return code(error);
            }
        };
        if reset_generation != 0 {
            *RESET.0.get() = None
        }
        out.write(result);
    }
    a::gfx_output_ok
}
fn driver_id(identity: Owner) -> u32 {
    u32::try_from(identity.id).expect("original R4D id range")
}
fn prepare_impl(
    identity: Owner,
    request: a::GfxNativeRegistration,
    held_generation: u64,
    reset_generation: u64,
) -> Result<a::GfxNativeState, Error> {
    unsafe {
        let b = BRIDGE.0.get();
        if identity.kind != 2 || !identity.valid() || (*b).driver_owner.id != 0 {
            return Err(Error::Busy);
        }
        if request.version != 1
            || (request.size as usize) < size_of::<a::GfxNativeRegistration>()
            || request.reference.reserved0 != 0
            || request.commit_callback < 0xffff800000000000
            || request.restore_callback < 0xffff800000000000
            || !valid_adapter(request.backend.adapter_id)
            || request.backend.milestone != 1
        {
            return Err(Error::Invalid);
        }
        let end = request
            .name
            .iter()
            .position(|&c| c == 0)
            .ok_or(Error::Invalid)?;
        if end == 0 || request.name[end..].iter().any(|&c| c != 0) {
            return Err(Error::Invalid);
        }
        let saved = display::boot_snapshot().ok_or(Error::Unavailable)?;
        if !framebuffer::is_native_xrgb32(&saved.framebuffer) {
            return Err(Error::Unsupported);
        }
        outputs::validate_native(
            driver_id(identity),
            request.backend,
            request.output,
            saved.mode.width,
            saved.mode.height,
            held_generation != 0,
        )?;
        let caller_reference = buffer_api::handle(request.reference)?;
        let (reference, address, descriptor) = {
            let _bo = BoLock::prepared(1, 1)?;
            let d = buffers::read().describe(caller_reference, identity)?;
            if d.format != layout::XRGB8888
                || d.location != 0
                || !d.binding.portable()
                || d.modifier != 0
                || d.plane_count != 1
                || d.planes[0].offset != 0
                || d.width != saved.mode.width
                || d.height != saved.mode.height
                || d.planes[0].pitch > u64::from(u32::MAX)
                || d.planes[0].pitch != u64::from(d.width) * 4
                || d.bytes != d.planes[0].pitch * u64::from(d.height)
                || d.usage & 6 != 6
            {
                return Err(Error::Unsupported);
            }
            let reference = buffers::table().share(caller_reference, OWNER)?;
            let mapped = match buffers::map_locked(reference, OWNER, 1, 0, d.bytes) {
                Ok(mapped) => mapped,
                Err(error) => {
                    buffers::table()
                        .drop_reference(reference, OWNER)
                        .expect("exact new reference");
                    return Err(error.into());
                }
            };
            if let Err(error) = buffers::unmap_cpu_locked(mapped.lease, OWNER) {
                buffers::table()
                    .drop_reference(reference, OWNER)
                    .expect("exact new reference");
                return Err(error.into());
            }
            (reference, mapped.backing.cpu_address, d)
        };
        b.write(Bridge {
            driver_owner: identity,
            registration: request,
            reference,
            bytes: descriptor.bytes,
            frame: saved.framebuffer,
            held_generation,
            ..Bridge::EMPTY
        });
        RETAINED.store(driver_id(identity), Ordering::Release);
        let prepared = (|| -> Result<a::GfxNativeState, Error> {
            (*b).frame.address = address as *mut u8;
            (*b).frame.pitch = descriptor.planes[0].pitch;
            (*b).frame.edid = ptr::null();
            (*b).frame.edid_size = 0;
            let mut mode = saved.mode;
            mode.pitch = u32::try_from((*b).frame.pitch).expect("validated pitch");
            let candidate = Backend {
                owner: identity.id as usize,
                adapter_id: request.backend.adapter_id,
                target: Target {
                    name: Text {
                        pointer: ptr::addr_of!((*b).registration.name).cast(),
                        len: end,
                    },
                    kind: display::NATIVE,
                    flags: 255,
                    mode,
                    mapping: core_display::Mapping {
                        kind: 2,
                        cache_policy: 0,
                        virt_base: address,
                        byte_len: (*b).bytes,
                        volatile_cpu_writes: false,
                    },
                    framebuffer: ptr::addr_of_mut!((*b).frame),
                },
                context: b.addr(),
                commit: core_display::Commit(commit),
                restore: core_display::Restore(restore),
                begin_cpu: Some(core_display::BeginCpu(begin_cpu)),
                end_cpu: Some(core_display::EndCpu(end_cpu)),
            };
            (*b).generation = if reset_generation != 0 {
                display::prepare_recovered_native(candidate, held_generation, reset_generation)?
            } else if held_generation != 0 {
                display::prepare_held_native(candidate, held_generation)?
            } else {
                display::prepare_native(candidate)?
            };
            (*b).timeline = queue::open(
                OWNER,
                model::Config {
                    binding: binding(request.backend),
                    milestone: 1,
                    capacity: 1,
                    ..Default::default()
                },
            )?;
            queue::bind_display_queue(
                driver_id(identity),
                binding(request.backend),
                (*b).timeline,
            )?;
            display::bind_presentation_stats(
                identity.id as usize,
                identity.generation,
                request.backend,
                (*b).generation,
            )?;
            (*b).ready = true;
            Ok(state_result(
                driver_id(identity),
                a::gfx_output_outcome_validated,
            ))
        })();
        if prepared.is_err() {
            cancel_preparation()
        }
        prepared
    }
}
fn cancel_preparation() {
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).generation != 0
            && display::abort_native((*b).driver_owner.id as usize, (*b).generation).is_err()
        {
            return;
        }
        (*b).cancelled = true;
        let _ = discard();
    }
}
/// # Safety
/// Output follows the initialized native-state header contract.
pub unsafe fn transition(
    id: u32,
    generation: u64,
    operation: u32,
    out: *mut a::GfxNativeState,
) -> i32 {
    if id == 0
        || irq_router::in_dispatch()
        || !unsafe { buffer_api::valid_output(out) }
        || operation > 2
    {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    unsafe {
        let b = BRIDGE.0.get();
        if (*REPLACEMENT.0.get()).is_some() {
            return a::gfx_output_error_busy;
        }
        if (*b).driver_owner.id != u64::from(id)
            && !(operation == 2
                && (*b).driver_owner.id == 0
                && display::retains_driver_owner(id as usize))
        {
            return a::gfx_output_error_stale;
        }
        let result = (|| -> Result<u32, Error> {
            Ok(match operation {
                0 => {
                    if !(*b).ready || (*b).cancelled {
                        return Err(Error::Busy);
                    }
                    let result = display::commit_native(id as usize, generation)?;
                    if result == 1 {
                        (*b).cancelled = true;
                        (*b).ready = false;
                        if !discard() {
                            a::gfx_output_outcome_lost
                        } else {
                            a::gfx_output_outcome_old_preserved
                        }
                    } else if result == 0 {
                        a::gfx_output_outcome_applied
                    } else {
                        a::gfx_output_outcome_lost
                    }
                }
                1 => {
                    if generation != (*b).generation {
                        return Err(Error::Stale);
                    }
                    if !(*b).cancelled {
                        display::abort_native(id as usize, generation)?
                    }
                    (*b).cancelled = true;
                    (*b).ready = false;
                    if discard() {
                        a::gfx_output_outcome_old_preserved
                    } else {
                        a::gfx_output_outcome_lost
                    }
                }
                2 => match restore_after_reset(id, generation) {
                    Ok(()) => a::gfx_output_outcome_applied,
                    Err(Error::RestoreFailed) => a::gfx_output_outcome_lost,
                    Err(e) => return Err(e),
                },
                _ => unreachable!(),
            })
        })();
        match result {
            Ok(outcome) => out.write(state_result(id, outcome)),
            Err(error) => return code(error),
        }
    }
    a::gfx_output_ok
}
unsafe extern "C" fn commit(_: usize, generation: u64, saved: *const BootSnapshot) -> u32 {
    let Ok(_execution) = execution() else {
        return 1;
    };
    unsafe {
        let saved = saved.read();
        let b = BRIDGE.0.get();
        if outputs::validate_native(
            driver_id((*b).driver_owner),
            (*b).registration.backend,
            (*b).registration.output,
            saved.mode.width,
            saved.mode.height,
            (*b).held_generation != 0,
        )
        .is_err()
            || !begin_cpu(0)
        {
            return 1;
        }
        let copied = if (*b).held_generation != 0 {
            copy_boot_snapshot(
                (*b).driver_owner,
                (*b).held_generation,
                &saved,
                ptr::addr_of!((*b).frame),
                false,
            )
        } else {
            for y in 0..u64::from(saved.mode.height) {
                for x in 0..u64::from(saved.mode.width) {
                    let source = saved
                        .framebuffer
                        .address
                        .add((y * saved.framebuffer.pitch + x * 4) as usize)
                        .cast::<u32>();
                    let target = (*b)
                        .frame
                        .address
                        .add((y * (*b).frame.pitch + x * 4) as usize)
                        .cast::<u32>();
                    target.write(source.read_volatile());
                }
            }
            true
        };
        let released = end_cpu(0, false, ptr::null());
        if !copied || released != 0 {
            return 1;
        }
        if !api_admission::r4os_rust_driver_api_enter(
            driver_id((*b).driver_owner),
            u64::from(timer::frequency().max(1)),
        ) {
            return 1;
        }
        let callback: Callback = core::mem::transmute((*b).registration.commit_callback as usize);
        let boot = boot_description(generation, &saved);
        let result = callback((*b).registration.context, generation, &boot);
        if result == 1 {
            outputs::native_active(driver_id((*b).driver_owner), (*b).registration.output, true)
        }
        let _ = api_admission::r4os_rust_driver_api_leave();
        match result {
            1 => 0,
            2 => 1,
            _ => 2,
        }
    }
}
unsafe fn copy_boot_snapshot(
    identity: Owner,
    generation: u64,
    saved: *const BootSnapshot,
    target: *const Framebuffer,
    padding: bool,
) -> bool {
    let identity = super::boot_driver::Owner {
        kind: identity.kind,
        id: identity.id,
        generation: identity.generation,
    };
    unsafe {
        super::boot_driver::r4os_rust_boot_driver_copy(
            &identity,
            generation,
            saved.cast(),
            target,
            padding,
        )
    }
}
unsafe extern "C" fn restore(_: usize, generation: u64, saved: *const BootSnapshot) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    unsafe {
        let b = BRIDGE.0.get();
        if (*REPLACEMENT.0.get()).is_some() {
            return false;
        }
        if !(*b).hardware_restored {
            if !api_admission::r4os_rust_driver_api_enter(
                driver_id((*b).driver_owner),
                u64::from(timer::frequency().max(1)),
            ) {
                return false;
            }
            let callback: Callback =
                core::mem::transmute((*b).registration.restore_callback as usize);
            let boot = boot_description(generation, &saved.read());
            let restored = callback((*b).registration.context, generation, &boot) == 1;
            let _ = api_admission::r4os_rust_driver_api_leave();
            if !restored {
                return false;
            }
            (*b).hardware_restored = true;
        }
        if (*b).held_generation != 0 && !(*b).pixels_restored {
            if !copy_boot_snapshot(
                (*b).driver_owner,
                (*b).held_generation,
                saved,
                ptr::addr_of!((*saved).framebuffer),
                true,
            ) {
                return false;
            }
            (*b).pixels_restored = true;
        }
        if !(*b).queue_stopped {
            outputs::native_active(
                driver_id((*b).driver_owner),
                (*b).registration.output,
                false,
            );
            if let Err(e) = queue::unregister_native(
                driver_id((*b).driver_owner),
                binding((*b).registration.backend),
                true,
            ) && !matches!(e, Error::Busy | Error::Stale)
            {
                return false;
            }
            (*b).queue_stopped = true;
        }
        discard()
    }
}
unsafe extern "C" fn begin_cpu(_: usize) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).encoded_output
            || (*b).cpu_lease.id != 0
            || (*b).reference.id == 0
            || (*b).pending.is_some()
            || !image_idle()
        {
            return false;
        }
        let Ok(_bo) = BoLock::prepared(0, 1) else {
            return false;
        };
        let Ok(use_) = buffers::map_locked((*b).reference, OWNER, 1, 0, (*b).bytes) else {
            return false;
        };
        if use_.backing.cpu_address != (*b).frame.address.addr() as u64 {
            let _ = buffers::unmap_cpu_locked(use_.lease, OWNER);
            return false;
        }
        (*b).cpu_lease = use_.lease;
        true
    }
}
fn image_idle() -> bool {
    unsafe {
        let b = BRIDGE.0.get();
        let Some(fence) = (*b).image_pending else {
            return true;
        };
        let status = match queue::query(fence) {
            Ok(s) => s,
            Err(e) => {
                if e != Error::Stale {
                    return false;
                }
                (*b).image_pending = None;
                return true;
            }
        };
        if status.phase != model::TERMINAL
            || (!(*b).image_direct && (status.device_active || status.resources_held))
        {
            return false;
        }
        (*b).image_pending = None;
        true
    }
}
pub fn output_target(adapter: u32, head: u32) -> Result<a::GfxOutputTarget, Error> {
    if let Some(target) = output_runtime::target_at(adapter, head)? {
        return Ok(target);
    }
    let _display = DisplayExecution::acquire()?;
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        let state = display::backend_state();
        let target = if adapter == 0 {
            outputs::active_target(
                0,
                a::GfxOutputId {
                    connector_id: 1,
                    device_generation: 1,
                    ..Default::default()
                },
                state.generation,
            )?
        } else {
            if !(*b).ready
                || (*b).cancelled
                || (*b).hardware_restored
                || state.state != backend::SOFTWARE_NATIVE
            {
                return Err(Error::Unsupported);
            }
            queue::validate_output_binding(
                driver_id((*b).driver_owner),
                (*b).registration.backend,
            )?;
            outputs::active_target(
                driver_id((*b).driver_owner),
                (*b).registration.output,
                (*b).generation,
            )?
        };
        if target.adapter_id != adapter || target.head_id != head {
            return Err(Error::Unsupported);
        }
        Ok(target)
    }
}
/// # Safety
/// Caller keeps all request pointer payloads resident through exact admission.
pub unsafe fn submit_image(
    caller: Owner,
    timeline: u64,
    submission: &resource::Submission<'_>,
    request: &resource::Request,
) -> Result<model::Status, Error> {
    if output_runtime::contains(request.display_target) {
        return unsafe {
            output_runtime::submit_image(
                caller,
                timeline,
                &output_runtime::Image {
                    request,
                    deadline: submission.deadline_ns,
                    frame_key: submission.frame_key,
                    dependencies: submission.dependencies.as_ptr(),
                    dependencies_len: submission.dependencies.len(),
                },
            )
        };
    }
    let _display = DisplayExecution::acquire()?;
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if !(*b).ready
            || (*b).cancelled
            || (*b).hardware_restored
            || display::backend_state().state != backend::SOFTWARE_NATIVE
        {
            return Err(Error::Unsupported);
        }
        if mode_pending() || (*b).cpu_lease.id != 0 || (*b).pending.is_some() || !image_idle() {
            return Err(Error::Busy);
        }
        if outputs::native_paused(driver_id((*b).driver_owner), (*b).registration.output) {
            return Err(Error::Unavailable);
        }
        if request.display_target.connector_id != 0 {
            let current = outputs::active_target(
                driver_id((*b).driver_owner),
                (*b).registration.output,
                (*b).generation,
            )?;
            if !super::output_target::same(&current, &request.display_target) {
                return Err(Error::Stale);
            }
        }
        queue::validate_output_binding(driver_id((*b).driver_owner), (*b).registration.backend)?;
        let descriptor = {
            let _bo = BoLock::acquire();
            buffers::read().describe(request.source, caller)?
        };
        if !matches!(
            request.operation,
            resource::PRESENT | resource::DIRECT_PRESENT
        ) || u64::from(descriptor.width) != (*b).frame.width
            || u64::from(descriptor.height) != (*b).frame.height
            || descriptor.format != (*b).output_format
            || !matches!(descriptor.location, 0 | 1)
            || descriptor.plane_count != 1
            || descriptor.planes[0].offset != 0
        {
            return Err(Error::Unsupported);
        }
        let accepted = queue::submit_display_image(
            caller,
            timeline,
            submission,
            request,
            binding((*b).registration.backend),
        )?;
        (*b).image_pending = Some(accepted.fence);
        (*b).image_direct = request.operation == resource::DIRECT_PRESENT;
        Ok(accepted)
    }
}
fn cpu_failure(stage: &str, reason: &str) -> u32 {
    let mut message = Message::<320>::new();
    if unsafe { super::ownership::raw_guard(EXECUTION.0.get(), 0, 0) } {
        unsafe {
            let b = BRIDGE.0.get();
            writeln!(&mut message,"[GFX-CPU] stage={} reason={} owner={} generation={} timeline={} lease={} pending={} image-pending={}",
                stage,reason,(*b).driver_owner.id,(*b).generation,(*b).timeline,(*b).cpu_lease.id,
                u8::from((*b).pending.is_some()),u8::from((*b).image_pending.is_some())).expect("original CPU log bound");
        }
    } else {
        writeln!(
            &mut message,
            "[GFX-CPU] stage={} reason={} bridge-snapshot=not-owned",
            stage, reason
        )
        .expect("original CPU log bound");
    }
    message.emit();
    2
}
fn cpu_error(stage: &str, error: Error) -> u32 {
    let mut reason = Message::<32>::new();
    write!(&mut reason, "{error:?}").expect("error name bound");
    cpu_failure(stage, reason.text())
}
unsafe extern "C" fn end_cpu(_: usize, changed: bool, damage: *const Rect) -> u32 {
    let Ok(_execution) = execution() else {
        return cpu_failure("finish-entry", "Busy");
    };
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).cpu_lease.id == 0 {
            return cpu_failure("finish-lease", "Missing");
        }
        {
            let bo = BoLock::acquire();
            if let Err(e) = buffers::unmap_cpu_locked((*b).cpu_lease, OWNER) {
                drop(bo);
                return cpu_error("finish-unmap", e.into());
            }
            (*b).cpu_lease = Handle::EMPTY;
        }
        if !changed {
            return 0;
        }
        let full = Rect {
            w: u32::try_from((*b).frame.width).expect("frame width"),
            h: u32::try_from((*b).frame.height).expect("frame height"),
            ..Default::default()
        };
        let requested = if damage.is_null() {
            full
        } else {
            damage.read()
        };
        if requested.w == 0
            || requested.h == 0
            || u64::from(requested.x) >= (*b).frame.width
            || u64::from(requested.y) >= (*b).frame.height
            || u64::from(requested.w) > (*b).frame.width - u64::from(requested.x)
            || u64::from(requested.h) > (*b).frame.height - u64::from(requested.y)
        {
            return cpu_failure("finish-damage", "Invalid");
        }
        let rect = if (*b).cpu_refresh_required {
            full
        } else {
            requested
        };
        (*b).cpu_refresh_required = true;
        if outputs::native_paused(driver_id((*b).driver_owner), (*b).registration.output) {
            return 0;
        }
        let offset = u64::from(rect.y) * (*b).frame.pitch + u64::from(rect.x) * 4;
        let span = u64::from(rect.h - 1) * (*b).frame.pitch + u64::from(rect.w) * 4;
        let Some(now) = monotonic::now_nanoseconds() else {
            return cpu_failure("finish-clock", "Unavailable");
        };
        let accepted = match queue::submit(
            OWNER,
            (*b).timeline,
            &resource::Submission {
                deadline_ns: now.saturating_add(3_000_000_000),
                frame_key: 0,
                dependencies: &[],
            },
            &resource::Request {
                operation: resource::UPLOAD,
                source: (*b).reference,
                source_offset: offset,
                bytes: span,
                ..Default::default()
            },
        ) {
            Ok(value) => value,
            Err(error) => {
                if matches!(error, Error::Busy | Error::Capacity) {
                    if !(*b).cpu_retry_reported {
                        (*b).cpu_retry_reported = true;
                        cpu_error("retry-submit", error);
                    }
                    return 1;
                }
                return cpu_error("finish-submit", error);
            }
        };
        (*b).pending = Some(accepted.fence);
        let complete =
            match queue::wait(accepted.fence, u64::from(timer::frequency().max(1)) * 4, 1) {
                Ok(status) => status,
                Err(e) => return cpu_error("finish-wait", e),
            };
        if let Err(e) = queue::drop_reference(OWNER, accepted.fence) {
            return cpu_error("finish-drop", e);
        }
        (*b).pending = None;
        let succeeded = complete.result == model::COMPLETE
            && !complete.device_active
            && !complete.resources_held;
        if succeeded {
            (*b).cpu_refresh_required = false;
            (*b).cpu_retry_reported = false
        }
        if succeeded
            || (complete.result == model::CANCELLED
                && !complete.device_active
                && !complete.resources_held)
        {
            return 0;
        }
        let mut reason = Message::<192>::new();
        let result = match complete.result {
            0 => "pending",
            1 => "complete",
            2 => "cancelled",
            3 => "dropped",
            4 => "device_lost",
            5 => "timeout",
            6 => "failed",
            7 => "dependency_failed",
            _ => unreachable!(),
        };
        write!(
            &mut reason,
            "result={} device-active={} resources-held={} fence={}:{}",
            result,
            u8::from(complete.device_active),
            u8::from(complete.resources_held),
            accepted.fence.timeline,
            accepted.fence.point
        )
        .expect("original result log bound");
        cpu_failure("finish-result", reason.text())
    }
}
fn discard() -> bool {
    unsafe {
        let b = BRIDGE.0.get();
        if (*REPLACEMENT.0.get()).is_some() || !image_idle() {
            return false;
        }
        if (*b).cpu_lease.id != 0 {
            let _bo = BoLock::acquire();
            if buffers::unmap_cpu_locked((*b).cpu_lease, OWNER).is_err() {
                return false;
            }
            (*b).cpu_lease = Handle::EMPTY;
        }
        if let Some(fence) = (*b).pending {
            if queue::drop_reference(OWNER, fence).is_err() {
                return false;
            }
            (*b).pending = None;
        }
        if (*b).timeline != 0 {
            queue::unbind_display_queue(
                driver_id((*b).driver_owner),
                binding((*b).registration.backend),
                (*b).timeline,
            );
            if let Err(e) = queue::close(OWNER, (*b).timeline)
                && e != Error::Stale
            {
                return false;
            }
            (*b).timeline = 0;
        }
        if (*b).reference.id != 0 {
            if buffers::drop_reference((*b).reference, OWNER).is_err() {
                return false;
            }
            (*b).reference = Handle::EMPTY;
        }
        if (*b).driver_reference.id != 0 {
            if buffers::drop_reference((*b).driver_reference, (*b).driver_owner).is_err() {
                return false;
            }
            (*b).driver_reference = Handle::EMPTY;
        }
        b.write(Bridge::EMPTY);
        RETAINED.store(0, Ordering::Release);
        true
    }
}
pub fn cursor_binding() -> Result<cursor_work::Binding, Error> {
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if !(*b).ready
            || (*b).cancelled
            || (*b).hardware_restored
            || display::backend_state().state != backend::SOFTWARE_NATIVE
        {
            return Err(Error::Unsupported);
        }
        if mode_pending() || (*b).cpu_lease.id != 0 || (*b).pending.is_some() || !image_idle() {
            return Err(Error::Busy);
        }
        queue::validate_output_binding(driver_id((*b).driver_owner), (*b).registration.backend)?;
        Ok(cursor_work::Binding {
            driver: (*b).driver_owner,
            backend: (*b).registration.backend,
            generation: (*b).generation,
            head: outputs::cursor_head(driver_id((*b).driver_owner), (*b).registration.output)?,
            timeline: 0,
            point: 0,
        })
    }
}
pub fn enable_modes(identity: Owner, backend: a::GfxBackendBinding) -> Result<(), Error> {
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if !(*b).ready
            || (*b).driver_owner != identity
            || !same(&(*b).registration.backend, &backend)
            || (*b).cancelled
            || (*b).hardware_restored
            || display::backend_state().state != super::backend_state::SOFTWARE_NATIVE
        {
            return Err(Error::Stale);
        }
        queue::validate_output_binding(driver_id(identity), backend)?;
        (*b).modes_enabled = true;
        Ok(())
    }
}
pub fn modes_enabled(id: u32, backend: a::GfxBackendBinding) -> bool {
    let Ok(_execution) = execution() else {
        return false;
    };
    unsafe {
        let b = BRIDGE.0.get();
        (*b).ready
            && (*b).modes_enabled
            && (*b).driver_owner.id == u64::from(id)
            && same(&(*b).registration.backend, &backend)
            && !(*b).cancelled
            && !(*b).hardware_restored
    }
}
pub fn mode_binding(assignment: a::GfxScanoutState) -> Result<ModeBinding, Error> {
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if !(*b).ready
            || !(*b).modes_enabled
            || (*b).cancelled
            || (*b).hardware_restored
            || display::backend_state().state != backend::SOFTWARE_NATIVE
        {
            return Err(Error::Unsupported);
        }
        if assignment.output.adapter_id != (*b).registration.backend.adapter_id
            || assignment.output.device_generation != (*b).registration.backend.device_generation
        {
            return Err(Error::Stale);
        }
        queue::validate_output_binding(driver_id((*b).driver_owner), (*b).registration.backend)?;
        if assignment.output.connector_id != (*b).registration.output.connector_id {
            let additional = output_runtime::mode_binding(assignment)?;
            if additional.driver != (*b).driver_owner
                || !same(&additional.backend, &(*b).registration.backend)
            {
                return Err(Error::Stale);
            }
            return Ok(ModeBinding {
                driver: additional.driver,
                backend: additional.backend,
                generation: additional.target.display_generation,
                has_additional: true,
                additional,
            });
        }
        Ok(ModeBinding {
            driver: (*b).driver_owner,
            backend: (*b).registration.backend,
            generation: (*b).generation,
            ..Default::default()
        })
    }
}
pub fn prepare_mode_encoding(
    caller: Owner,
    assignment: a::GfxScanoutState,
    mode: a::GfxOutputMode,
    encoded_format: Option<u32>,
) -> Result<a::GfxBufferReference, Error> {
    if encoded_format
        .is_some_and(|format| !matches!(format, layout::XRGB8888 | layout::XRGB2101010))
    {
        return Err(Error::Unsupported);
    }
    let _execution = execution()?;
    let selected = mode_binding(assignment)?;
    if selected.has_additional {
        return native_additional_mode::prepare(caller, assignment, mode, selected.additional);
    }
    unsafe {
        let b = BRIDGE.0.get();
        if (*REPLACEMENT.0.get()).is_some()
            || (*b).cpu_lease.id != 0
            || (*b).pending.is_some()
            || !image_idle()
        {
            return Err(Error::Busy);
        }
        if assignment.source_x != 0
            || assignment.source_y != 0
            || assignment.destination_x != 0
            || assignment.destination_y != 0
            || assignment.source_width != mode.width
            || assignment.source_height != mode.height
            || assignment.destination_width != mode.width
            || assignment.destination_height != mode.height
            || assignment.rotation != 0
            || assignment.color != 0
            || assignment.bits_per_color != 8
        {
            return Err(Error::Unsupported);
        }
        let source = buffer_api::handle(assignment.buffer)?;
        let _bo = BoLock::prepared(2, 1)?;
        let d = buffers::read().describe(source, caller)?;
        let usage = 2 | 4 | 32;
        if d.format != layout::XRGB8888
            || d.location != 0
            || !d.binding.portable()
            || d.modifier != 0
            || d.plane_count != 1
            || d.planes[0].offset != 0
            || d.usage & usage != usage
            || d.width != mode.width
            || d.height != mode.height
            || d.planes[0].pitch > u64::from(u32::MAX)
            || d.bytes != d.planes[0].pitch * u64::from(d.height)
        {
            return Err(Error::Unsupported);
        }
        if buffers::read().buffer_for(source, caller)?
            == buffers::read().buffer_for((*b).reference, OWNER)?
        {
            return Err(Error::Invalid);
        }
        let reference = buffers::table().share(source, OWNER)?;
        let mut driver_reference = Handle::EMPTY;
        let mut read_lease = Handle::EMPTY;
        let prepared = (|| -> Result<a::GfxBufferReference, Error> {
            driver_reference = buffers::table().share(source, (*b).driver_owner)?;
            let mapped = buffers::map_locked(reference, OWNER, 1, 0, d.bytes)?;
            buffers::unmap_cpu_locked(mapped.lease, OWNER)?;
            read_lease = buffers::table()
                .use_resource(reference, OWNER, 2, 0, d.bytes)?
                .lease;
            let wire = buffer_api::reference_locked(driver_reference, (*b).driver_owner)?;
            let mut frame = (*b).frame;
            frame.address = mapped.backing.cpu_address as *mut u8;
            frame.width = u64::from(d.width);
            frame.height = u64::from(d.height);
            frame.pitch = d.planes[0].pitch;
            frame.edid = ptr::null();
            frame.edid_size = 0;
            *REPLACEMENT.0.get() = Some(Replacement {
                old: Surface {
                    reference: (*b).reference,
                    driver_reference: (*b).driver_reference,
                    read_lease: Handle::EMPTY,
                    bytes: (*b).bytes,
                    frame: (*b).frame,
                    output_format: (*b).output_format,
                    encoded_output: (*b).encoded_output,
                },
                new: Surface {
                    reference,
                    driver_reference,
                    read_lease,
                    bytes: d.bytes,
                    frame,
                    output_format: encoded_format.unwrap_or(layout::XRGB8888),
                    encoded_output: encoded_format.is_some(),
                },
                using_new: false,
            });
            Ok(wire)
        })();
        if prepared.is_err() {
            if read_lease.id != 0 {
                buffers::table()
                    .end_use(read_lease, OWNER, true)
                    .expect("new mode read lease")
            }
            if driver_reference.id != 0 {
                buffers::table()
                    .drop_reference(driver_reference, (*b).driver_owner)
                    .expect("new driver reference")
            }
            buffers::table()
                .drop_reference(reference, OWNER)
                .expect("new display reference");
        }
        prepared
    }
}
unsafe fn change() -> Result<*mut Replacement, Error> {
    unsafe {
        (*REPLACEMENT.0.get())
            .as_mut()
            .map(ptr::from_mut)
            .ok_or(Error::Stale)
    }
}
pub fn start_mode(operation: u32, expected: ModeBinding) -> Result<(), Error> {
    let _execution = execution()?;
    if expected.has_additional {
        return native_additional_mode::start(operation, expected.additional);
    }
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).driver_owner != expected.driver
            || (*b).generation != expected.generation
            || !same(&(*b).registration.backend, &expected.backend)
            || (*b).cancelled
            || (*b).hardware_restored
        {
            return Err(Error::Stale);
        }
        let change = change()?;
        if (*b).cpu_lease.id != 0 || (*b).pending.is_some() || !image_idle() {
            return Err(Error::Busy);
        }
        queue::validate_output_binding(driver_id(expected.driver), expected.backend)?;
        let surface = if operation == a::gfx_mode_operation_apply {
            ptr::addr_of_mut!((*change).old)
        } else if operation == a::gfx_mode_operation_rollback {
            ptr::addr_of_mut!((*change).new)
        } else {
            return Ok(());
        };
        if (*surface).read_lease.id == 0 {
            let _bo = BoLock::prepared(0, 1)?;
            (*surface).read_lease = buffers::table()
                .use_resource((*surface).reference, OWNER, 2, 0, (*surface).bytes)?
                .lease;
        }
    }
    Ok(())
}
unsafe fn release_read(surface: *mut Surface) -> Result<(), Error> {
    unsafe {
        if (*surface).read_lease.id == 0 {
            return Ok(());
        }
        let _bo = BoLock::acquire();
        buffers::table().end_use((*surface).read_lease, OWNER, true)?;
        (*surface).read_lease = Handle::EMPTY;
    }
    Ok(())
}
unsafe fn release_surface(surface: *mut Surface) -> Result<(), Error> {
    unsafe {
        release_read(surface)?;
        if (*surface).driver_reference.id != 0 {
            buffers::drop_reference((*surface).driver_reference, (*BRIDGE.0.get()).driver_owner)?;
            (*surface).driver_reference = Handle::EMPTY;
        }
        if (*surface).reference.id != 0 {
            buffers::drop_reference((*surface).reference, OWNER)?;
            (*surface).reference = Handle::EMPTY
        }
    }
    Ok(())
}
unsafe fn select_surface(surface: *const Surface) -> Result<(), Error> {
    unsafe {
        let b = BRIDGE.0.get();
        let before = (*b).frame;
        (*b).frame = (*surface).frame;
        if let Err(e) = display::replace_native_frame_locked(
            (*b).driver_owner.id as usize,
            (*b).generation,
            ptr::addr_of_mut!((*b).frame),
            (*surface).bytes,
        ) {
            (*b).frame = before;
            return Err(e);
        }
        (*b).reference = (*surface).reference;
        (*b).driver_reference = (*surface).driver_reference;
        (*b).bytes = (*surface).bytes;
        (*b).output_format = (*surface).output_format;
        (*b).encoded_output = (*surface).encoded_output;
        outputs::native_active(driver_id((*b).driver_owner), (*b).registration.output, true);
    }
    Ok(())
}
pub fn settle_mode(operation: u32, outcome: u32) -> Result<(), Error> {
    let _execution = execution()?;
    unsafe {
        let change = change()?;
        let b = BRIDGE.0.get();
        if outcome == a::gfx_output_outcome_lost {
            display::lose_native_mode_locked((*b).driver_owner.id as usize, (*b).generation)?;
            outputs::native_active(
                driver_id((*b).driver_owner),
                (*b).registration.output,
                false,
            );
            return Ok(());
        }
        if outcome == a::gfx_output_outcome_applied {
            if operation == a::gfx_mode_operation_apply {
                release_read(ptr::addr_of_mut!((*change).new))?;
                select_surface(ptr::addr_of!((*change).new))?;
                (*change).using_new = true;
                return Ok(());
            }
            if operation != a::gfx_mode_operation_confirm || !(*change).using_new {
                return Err(Error::Invalid);
            }
            select_surface(ptr::addr_of!((*change).new))?;
            release_surface(ptr::addr_of_mut!((*change).old))?;
        } else if outcome == a::gfx_output_outcome_old_preserved {
            release_read(ptr::addr_of_mut!((*change).old))?;
            select_surface(ptr::addr_of!((*change).old))?;
            release_surface(ptr::addr_of_mut!((*change).new))?;
        } else {
            return Err(Error::Invalid);
        }
        *REPLACEMENT.0.get() = None;
    }
    Ok(())
}
pub fn settle_mode_for(operation: u32, outcome: u32, expected: ModeBinding) -> Result<(), Error> {
    if expected.has_additional {
        native_additional_mode::settle(operation, outcome)
    } else {
        settle_mode(operation, outcome)
    }
}
pub fn abort_prepared_mode(expected: ModeBinding) {
    let _ = settle_mode_for(
        a::gfx_mode_operation_apply,
        a::gfx_output_outcome_old_preserved,
        expected,
    );
}
pub fn retire_mode_after_reset(
    identity: Owner,
    backend: a::GfxBackendBinding,
) -> Result<(), Error> {
    let _execution = execution()?;
    unsafe {
        let b = BRIDGE.0.get();
        if (*b).driver_owner.id != 0 {
            if (*b).driver_owner != identity || !same(&(*b).registration.backend, &backend) {
                return Err(Error::Stale);
            }
            if (*b).cpu_lease.id != 0 {
                let _bo = BoLock::acquire();
                buffers::unmap_cpu_locked((*b).cpu_lease, OWNER)?;
                (*b).cpu_lease = Handle::EMPTY;
            }
            if let Some(fence) = (*b).pending {
                require_quiesced_fence(fence)?;
                if let Err(e) = queue::drop_reference(OWNER, fence)
                    && e != Error::Stale
                {
                    return Err(e);
                }
                (*b).pending = None;
            }
            if let Some(fence) = (*b).image_pending {
                require_quiesced_fence(fence)?;
                (*b).image_pending = None
            }
            (*b).modes_enabled = false;
            (*b).cancelled = true;
            if (*REPLACEMENT.0.get()).is_some() {
                let change = change()?;
                (*b).reference = Handle::EMPTY;
                (*b).driver_reference = Handle::EMPTY;
                release_surface(ptr::addr_of_mut!((*change).old))?;
                release_surface(ptr::addr_of_mut!((*change).new))?;
                *REPLACEMENT.0.get() = None;
            }
        }
    }
    native_additional_mode::retire_after_reset(identity, backend)
}
fn require_quiesced_fence(fence: model::Fence) -> Result<(), Error> {
    let status = match queue::query(fence) {
        Ok(s) => s,
        Err(Error::Stale) => return Ok(()),
        Err(e) => return Err(e),
    };
    if status.phase != model::TERMINAL || status.device_active || status.resources_held {
        Err(Error::Busy)
    } else {
        Ok(())
    }
}
fn reset_pointer() -> Result<*mut ResetRecord, Error> {
    unsafe {
        (*RESET.0.get())
            .as_mut()
            .map(ptr::from_mut)
            .ok_or(Error::Stale)
    }
}
/// # Safety
/// R4D keeps both full fixed records live through the admitted reset operation.
// Original receipt/validation branch scopes retained during the port.
#[allow(clippy::collapsible_if)]
pub unsafe fn device_reset(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    generation: u64,
    quiesced: u32,
    out: *mut a::GfxNativeState,
) -> i32 {
    if input.is_null()
        || irq_router::in_dispatch()
        || !unsafe { buffer_api::valid_output(out) }
        || quiesced > 1
        || unsafe {
            (*input).version != 1
                || ((*input).size as usize) < size_of::<a::GfxBackendBinding>()
                || generation == 0
                || !identity.valid()
                || identity.kind != 2
                || !valid_adapter((*input).adapter_id)
                || (*input).milestone != a::gfx_queue_milestone_device_execution
                || ((*input).device_generation == 0) != ((*input).reset_generation == 0)
        }
    {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    let result = (|| -> Result<a::GfxNativeState, i32> {
        unsafe {
            let input = input.read();
            let b = BRIDGE.0.get();
            let mut rebuilt_headless = false;
            let restored_console = quiesced == 0
                && input.device_generation == 0
                && input.reset_generation == 0
                && display::restored_boot_owner(identity.id as usize, generation, input.adapter_id)
                    .map_err(code)?;
            if restored_console
                && (!owns_restored_console(identity, generation, input.adapter_id)
                    || (*b).driver_owner.id != 0)
            {
                return Err(a::gfx_output_error_stale);
            }
            if let Some(reset) = *RESET.0.get()
                && let Some(resumed) = reset.resumed_backend
            {
                if quiesced != 0
                    || generation != reset.generation
                    || reset.driver != identity
                    || !same(&resumed, &input)
                    || reset.terminal_released
                    || !reset.retired
                {
                    return Err(a::gfx_output_error_stale);
                }
                rebuilt_headless = true;
            }
            if (*RESET.0.get()).is_none() || rebuilt_headless || restored_console {
                if quiesced != 0 {
                    return Err(a::gfx_output_error_stale);
                }
                let current = display::backend_state();
                if restored_console {
                    // Common begin_reset authenticates the exact restored epoch.
                } else if current.state == backend::PREPARING {
                    if current.pending_owner != identity.id as usize
                        || current.pending_generation != generation
                        || current.pending_adapter_id != input.adapter_id
                    {
                        return Err(a::gfx_output_error_stale);
                    }
                } else if current.owner != identity.id as usize
                    || current.generation != generation
                    || current.adapter_id != input.adapter_id
                {
                    return Err(a::gfx_output_error_stale);
                }
                if (*b).driver_owner.id != 0
                    && ((*b).driver_owner != identity
                        || !same(&(*b).registration.backend, &input)
                        || ((*b).generation != generation
                            && !(current.state == backend::UNAVAILABLE
                                && current.reason == 7
                                && (*b).generation < generation)))
                {
                    return Err(a::gfx_output_error_stale);
                }
                mode_work::stopped_driver(driver_id(identity));
                cursor_work::stopped_driver(driver_id(identity));
                output_runtime::stopped_driver(driver_id(identity));
                let next =
                    display::begin_device_reset(identity.id as usize, generation, input.adapter_id)
                        .map_err(code)?;
                (*b).cancelled = true;
                *RESET.0.get() = Some(ResetRecord {
                    driver: identity,
                    backend: input,
                    original_generation: generation,
                    generation: next,
                    retired: false,
                    terminal_released: false,
                    resumed_backend: None,
                    reported_step: ResetStep::None,
                    reported_result: 0,
                });
                *CONSOLE.0.get() = None;
            }
            let reset = reset_pointer().map_err(code)?;
            if (*reset).terminal_released
                || (*reset).driver != identity
                || !same(&(*reset).backend, &input)
                || generation
                    != if quiesced == 0 {
                        (*reset).original_generation
                    } else {
                        (*reset).generation
                    }
            {
                return Err(a::gfx_output_error_stale);
            }
            if quiesced == 1 && !(*reset).retired {
                // Keep the original public error mapping and changed-checkpoint
                // logging. A completed prefix stays published across Busy retries.
                let retired = (|| -> Result<(), (ResetStep, Error)> {
                    if input.device_generation != 0 {
                        if let Err(e) =
                            queue::unregister_native(driver_id(identity), binding(input), true)
                            && e != Error::Stale
                        {
                            return Err((ResetStep::Queue, e));
                        }
                    }
                    mode_work::retire_after_reset(identity, input, (*reset).generation)
                        .map_err(|e| (ResetStep::Mode, e))?;
                    cursor_work::retire_after_reset(identity, input)
                        .map_err(|e| (ResetStep::Cursor, e))?;
                    output_runtime::retire_after_reset(identity, input)
                        .map_err(|e| (ResetStep::Output, e))?;
                    display::retire_device_reset(identity.id as usize, (*reset).generation)
                        .map_err(|e| (ResetStep::Display, e))?;
                    if !discard() {
                        return Err((ResetStep::Bridge, Error::Busy));
                    }
                    Ok(())
                })();
                if let Err((step, error)) = retired {
                    // Cursor has its own wider public capacity/stale projection.
                    let public = if step == ResetStep::Cursor {
                        cursor_work::code(error)
                    } else {
                        code(error)
                    };
                    return Err(reset_result(reset, step, public));
                }
                (*reset).retired = true;
                reset_result(reset, ResetStep::Retired, a::gfx_output_ok);
            }
            Ok(a::GfxNativeState {
                generation: (*reset).generation,
                state: display::backend_state().state,
                outcome: a::gfx_output_outcome_lost,
                retained: 1,
                ..Default::default()
            })
        }
    })();
    match result {
        Ok(value) => {
            unsafe { out.write(value) };
            a::gfx_output_ok
        }
        Err(public) => public,
    }
}

/// # Safety
/// Exact initialized backend/output remain resident for this R4D operation.
pub unsafe fn resume_headless(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    generation: u64,
    out: *mut a::GfxNativeState,
) -> i32 {
    if !unsafe { valid_transition_records(identity, input, generation, out) } {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    let result = (|| -> Result<a::GfxNativeState, Error> {
        unsafe {
            let input = input.read();
            let reset = reset_pointer()?;
            if (*reset).driver != identity
                || (*reset).generation != generation
                || (*reset).terminal_released
                || input.adapter_id != (*reset).backend.adapter_id
                || input.device_generation <= (*reset).backend.device_generation
            {
                return Err(Error::Stale);
            }
            if !(*reset).retired {
                return Err(Error::Busy);
            }
            let current = display::backend_state();
            if current.state != backend::RECOVERING
                || current.owner != identity.id as usize
                || current.generation != generation
                || current.adapter_id != input.adapter_id
                || (*BRIDGE.0.get()).driver_owner.id != 0
            {
                return Err(Error::Stale);
            }
            if let Some(resumed) = (*reset).resumed_backend {
                if !same(&resumed, &input) {
                    return Err(Error::Stale);
                }
            } else {
                queue::validate_output_binding(driver_id(identity), input)?;
                if input.milestone != a::gfx_queue_milestone_device_execution {
                    return Err(Error::Stale);
                }
                (*reset).resumed_backend = Some(input);
            }
            Ok(a::GfxNativeState {
                generation,
                state: a::display_state_recovering,
                outcome: a::gfx_output_outcome_lost,
                retained: 1,
                ..Default::default()
            })
        }
    })();
    match result {
        Ok(value) => {
            unsafe { out.write(value) };
            a::gfx_output_ok
        }
        Err(e) => code(e),
    }
}
unsafe fn valid_transition_records(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    generation: u64,
    out: *const a::GfxNativeState,
) -> bool {
    !input.is_null()
        && input
            .addr()
            .is_multiple_of(core::mem::align_of::<a::GfxBackendBinding>())
        && !irq_router::in_dispatch()
        && unsafe {
            buffer_api::valid_output(out)
                && (*input).version == 1
                && (*input).size as usize >= size_of::<a::GfxBackendBinding>()
        }
        && generation != 0
        && identity.valid()
        && identity.kind == 2
}
/// # Safety
/// R4D retains exact backend and initialized state output through release.
pub unsafe fn release_terminal(
    identity: Owner,
    input: *const a::GfxBackendBinding,
    generation: u64,
    out: *mut a::GfxNativeState,
) -> i32 {
    if !unsafe { valid_transition_records(identity, input, generation, out) } {
        return a::gfx_output_error_invalid;
    }
    let Ok(_execution) = execution() else {
        return a::gfx_output_error_busy;
    };
    let result = (|| -> Result<a::GfxNativeState, Error> {
        unsafe {
            let reset = reset_pointer()?;
            if (*reset).driver != identity
                || !same(&(*reset).backend, &input.read())
                || generation != (*reset).generation
            {
                return Err(Error::Stale);
            }
            if !(*reset).retired || (*reset).resumed_backend.is_some() {
                return Err(Error::Busy);
            }
            if !(*reset).terminal_released {
                display::release_terminal(identity.id as usize, generation, (*input).adapter_id)?;
                (*reset).terminal_released = true
            }
            Ok(a::GfxNativeState {
                generation,
                state: display::backend_state().state,
                outcome: a::gfx_output_outcome_lost,
                retained: 0,
                ..Default::default()
            })
        }
    })();
    match result {
        Ok(value) => {
            unsafe { out.write(value) };
            a::gfx_output_ok
        }
        Err(e) => code(e),
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Args {
    pub owner: Owner,
    pub backend: a::GfxBackendBinding,
    pub generation: u64,
    pub held: u64,
    pub reset: u64,
    pub value: u32,
    pub other: u32,
    pub input: *const c_void,
    pub second: *const c_void,
}
#[repr(C)]
pub struct Prepare {
    pub assignment: a::GfxScanoutState,
    pub mode: a::GfxOutputMode,
    pub has_format: bool,
    pub format: u32,
}
/// # Safety
/// Existing Kernel facades retain exact fixed records and pointer payloads.
/// Public R4D operations validate their original headers before full access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_native_driver(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { args.read() };
    let public = unsafe {
        match op {
            0 => Some(boot_info(out.cast())),
            5..=7 => Some(if op == 6 && args.held == 0 || op == 7 && args.reset == 0 {
                a::gfx_output_error_stale
            } else {
                prepare_request(
                    args.owner,
                    args.input.cast(),
                    args.held,
                    args.reset,
                    out.cast(),
                )
            }),
            8 => Some(transition(
                args.value,
                args.generation,
                args.other,
                out.cast(),
            )),
            21 => Some(device_reset(
                args.owner,
                args.input.cast(),
                args.generation,
                args.value,
                out.cast(),
            )),
            22 => Some(resume_headless(
                args.owner,
                args.input.cast(),
                args.generation,
                out.cast(),
            )),
            23 => Some(release_terminal(
                args.owner,
                args.input.cast(),
                args.generation,
                out.cast(),
            )),
            _ => None,
        }
    };
    if let Some(public) = public {
        return public;
    }
    let result = (|| -> Result<(), Error> {
        unsafe {
            match op {
                1 => out.cast::<bool>().write(retained(args.value)),
                2 => out.cast::<bool>().write(mode_pending()),
                3 => out.cast::<bool>().write(valid_adapter(args.value)),
                4 => out.cast::<a::GfxNativeBootInfo>().write(boot_description(
                    args.generation,
                    &args.input.cast::<BootSnapshot>().read(),
                )),
                9 => out
                    .cast::<a::GfxOutputTarget>()
                    .write(output_target(args.value, args.other)?),
                10 => {
                    let image = args.input.cast::<output_runtime::Image>().read();
                    let dependencies = if image.dependencies_len == 0 {
                        &[]
                    } else {
                        core::slice::from_raw_parts(image.dependencies, image.dependencies_len)
                    };
                    let status = submit_image(
                        args.owner,
                        args.generation,
                        &resource::Submission {
                            deadline_ns: image.deadline,
                            frame_key: image.frame_key,
                            dependencies,
                        },
                        &*image.request,
                    )?;
                    out.cast::<model::Status>().write(status);
                }
                11 => enable_modes(args.owner, args.backend)?,
                12 => out
                    .cast::<bool>()
                    .write(modes_enabled(args.value, args.backend)),
                13 => out.cast::<ModeBinding>().write(mode_binding(
                    args.input.cast::<a::GfxScanoutState>().read(),
                )?),
                14 => {
                    let input = &*args.input.cast::<Prepare>();
                    out.cast::<a::GfxBufferReference>()
                        .write(prepare_mode_encoding(
                            args.owner,
                            input.assignment,
                            input.mode,
                            if input.has_format {
                                Some(input.format)
                            } else {
                                None
                            },
                        )?);
                }
                15 => start_mode(args.value, args.input.cast::<ModeBinding>().read())?,
                16 => settle_mode(args.value, args.other)?,
                17 => settle_mode_for(
                    args.value,
                    args.other,
                    args.input.cast::<ModeBinding>().read(),
                )?,
                18 => abort_prepared_mode(args.input.cast::<ModeBinding>().read()),
                19 => retire_mode_after_reset(args.owner, args.backend)?,
                20 => out.cast::<cursor_work::Binding>().write(cursor_binding()?),
                _ => panic!("native display operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

// Private projections for the existing console-restoration oracle only. No
// fixture backup or state inspection API is compiled into the product image.

const _: () = {
    assert!(size_of::<Args>() == 104);
    assert!(core::mem::offset_of!(Args, backend) == 24);
    assert!(core::mem::offset_of!(Args, generation) == 56);
    assert!(core::mem::offset_of!(Args, held) == 64);
    assert!(core::mem::offset_of!(Args, reset) == 72);
    assert!(core::mem::offset_of!(Args, value) == 80);
    assert!(core::mem::offset_of!(Args, other) == 84);
    assert!(core::mem::offset_of!(Args, input) == 88);
    assert!(core::mem::offset_of!(Args, second) == 96);
    assert!(size_of::<cursor_work::Binding>() == 88);
};

unsafe fn native_display_log(bytes: *const u8, len: usize) {
    unsafe {
        crate::boot::log_route::r4os_rust_log_write(1, bytes, len);
        crate::boot::log::r4os_rust_bootlog_write(bytes, len)
    };
}
