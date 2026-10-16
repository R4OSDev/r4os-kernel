//! Sole additional-output runtime. BO admission precedes the short catalog
//! owner; framebuffer work, physical receipts and callbacks stay outside both.
use super::{
    equal::same,
    error::Error,
    native_additional_mode,
    native_types::AdditionalBinding as ModeBinding,
    output_runtime_state::{self as model, Store},
    output_target, outputs,
    ownership::{self, Resident},
    queue_state as queue,
};
use crate::{
    contract as a,
    gfx::{buffers, types::Owner},
    service::desktop_events,
};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit};
static STORE: Resident<MaybeUninit<Store>> = Resident(UnsafeCell::new(MaybeUninit::uninit()));
static READY: Resident<bool> = Resident(UnsafeCell::new(false));
static ADMISSION: Resident<crate::sched::sync_types::UnwindGuard> = Resident(UnsafeCell::new(
    ownership::execution(b"additional-output-admission"),
));
// Only called while the common BO metadata owner is held. Initialization is
// resident and fieldwise; no 79-KB catalog is materialized on a task stack.
unsafe fn store() -> *mut Store {
    if !unsafe { READY.0.get().read() } {
        unsafe {
            Store::init_in_place(STORE.0.get().cast());
            READY.0.get().write(true);
        }
    }
    STORE.0.get().cast()
}
struct BoLock;
impl BoLock {
    fn acquire() -> Self {
        buffers::lock();
        Self
    }
}
impl Drop for BoLock {
    fn drop(&mut self) {
        buffers::unlock()
    }
}
fn admission() -> Result<ownership::RecursiveExecution, Error> {
    unsafe { ownership::RecursiveExecution::acquire(ADMISSION.0.get()) }
}
fn signal() {
    desktop_events::r4os_rust_desktop_signal()
}
fn port(target: a::GfxOutputTarget) -> a::GfxOutputId {
    a::GfxOutputId {
        adapter_id: target.adapter_id,
        connector_id: target.connector_id,
        device_generation: target.device_generation,
        connection_generation: target.connection_generation,
    }
}
fn binding(value: a::GfxBackendBinding) -> queue::Binding {
    queue::Binding {
        adapter: value.adapter_id,
        device_generation: value.device_generation,
        reset_generation: value.reset_generation,
    }
}
fn busy(target: a::GfxOutputTarget) -> bool {
    unsafe { super::queue::output_busy_locked(target) }
}
fn query(fence: queue::Fence) -> Result<queue::Status, Error> {
    unsafe { super::queue::query_locked(fence) }
}
pub fn register(
    driver: Owner,
    request: a::GfxAdditionalOutput,
) -> Result<a::GfxOutputTarget, Error> {
    let _admission = admission()?;
    let _bo = BoLock::acquire();
    unsafe {
        super::queue::require_output_targets_locked(
            driver,
            binding(request.backend),
            request.job_size,
        )
    }?;
    let catalog = unsafe { &mut *store() };
    for entry in &catalog.entries {
        if entry.driver.id != 0
            && entry.target.adapter_id == request.backend.adapter_id
            && (entry.target.head_id == request.head_id
                || entry.target.connector_id == request.output.connector_id)
            && busy(entry.target)
        {
            return Err(Error::Busy);
        }
    }
    let _owner = ownership::enter_state();
    outputs::validate_additional_locked(u32::try_from(driver.id).expect("driver id"), request)?;
    let target = catalog.register(driver, request)?;
    let _ = outputs::pause_additional_locked(
        u32::try_from(driver.id).expect("driver id"),
        request.output,
        true,
    )?;
    Ok(target)
}
pub fn transition(
    driver: Owner,
    target: a::GfxOutputTarget,
    operation: u32,
    quiesced: u32,
) -> Result<(), Error> {
    if operation > 2 || quiesced != u32::from(operation == 2) {
        return Err(Error::Invalid);
    }
    let _admission = admission()?;
    if operation == 2 {
        {
            let _bo = BoLock::acquire();
            let catalog = unsafe { &mut *store() };
            let index = catalog.find(target)?;
            let entry = &mut catalog.entries[index];
            if entry.driver != driver {
                return Err(Error::WrongOwner);
            }
            if busy(target) {
                return Err(Error::Busy);
            }
            entry.removing = true;
        }
        if let Err(error) = native_additional_mode::retire(driver, target) {
            let _bo = BoLock::acquire();
            let catalog = unsafe { &mut *store() };
            if let Ok(index) = catalog.find(target) {
                let entry = &mut catalog.entries[index];
                if entry.driver == driver {
                    entry.removing = false;
                }
            }
            return Err(error);
        }
    }
    let changed = (|| -> Result<bool, Error> {
        let _bo = BoLock::acquire();
        let catalog = unsafe { &mut *store() };
        let index = catalog.find(target)?;
        let entry = &mut catalog.entries[index];
        if entry.driver != driver {
            return Err(Error::WrongOwner);
        }
        if operation == 0 && (entry.removing || entry.mode_blocked || entry.mode_lost) {
            return Err(Error::Busy);
        }
        if operation == 2 && busy(target) {
            return Err(Error::Busy);
        }
        let _owner = ownership::enter_state();
        let selected = outputs::current_identity_locked(
            u32::try_from(driver.id).expect("driver id"),
            port(target),
        )
        .unwrap_or(port(target));
        if operation == 0 && !same(&selected, &port(target)) {
            return Err(Error::Stale);
        }
        let paused_changed = if operation == 0 {
            outputs::validate_additional_activation_locked(
                u32::try_from(driver.id).expect("driver id"),
                selected,
                target.head_id,
                entry.width,
                entry.height,
            )?;
            outputs::pause_additional_locked(
                u32::try_from(driver.id).expect("driver id"),
                selected,
                false,
            )?
        } else {
            match outputs::pause_additional_locked(
                u32::try_from(driver.id).expect("driver id"),
                selected,
                true,
            ) {
                Ok(value) => value,
                Err(Error::Stale) => false,
                Err(error) => return Err(error),
            }
        };
        let changed = match outputs::set_native_active_locked(
            u32::try_from(driver.id).expect("driver id"),
            selected,
            target.head_id,
            operation == 0,
        ) {
            Ok(value) => value,
            Err(error) => {
                if operation == 0 || error != Error::Stale {
                    return Err(error);
                }
                outputs::forget_native_port_locked(
                    u32::try_from(driver.id).expect("driver id"),
                    selected,
                );
                entry.active = false;
                if operation == 2 {
                    unsafe { model::Entry::init_in_place(entry) };
                }
                return Ok(true);
            }
        };
        entry.active = operation == 0;
        if operation == 2 {
            unsafe { model::Entry::init_in_place(entry) };
        }
        Ok(changed || paused_changed)
    })()?;
    if operation == 1 {
        let _ = super::mode_work::withdrawing_output(
            u32::try_from(driver.id).expect("driver id"),
            port(target),
        );
    }
    if changed {
        signal()
    }
    Ok(())
}
pub fn target_at(adapter: u32, head: u32) -> Result<Option<a::GfxOutputTarget>, Error> {
    let _bo = BoLock::acquire();
    let catalog = unsafe { &*store() };
    let Some(index) = catalog.at(adapter, head) else {
        return Ok(None);
    };
    let entry = &catalog.entries[index];
    let current = outputs::active_target(
        u32::try_from(entry.driver.id).expect("driver id"),
        port(entry.target),
        entry.target.display_generation,
    )?;
    if !output_target::same(&current, &entry.target) {
        return Err(Error::Stale);
    }
    Ok(Some(current))
}
pub fn contains(target: a::GfxOutputTarget) -> bool {
    let _bo = BoLock::acquire();
    unsafe { &*store() }.find(target).is_ok()
}
/// Fixed private image metadata. The caller keeps copied request/dependency
/// records resident for their original exact unwind lifetime.
#[repr(C)]
pub struct Image {
    pub request: *const super::queue_resources::Request,
    pub deadline: u64,
    pub frame_key: u64,
    pub dependencies: *const queue::Fence,
    pub dependencies_len: usize,
}
/// # Safety
/// Exact copied request and admitted immutable dependency array remain alive
/// across prepared BO growth and through this bounded queue submission.
// Original receipt/validation branch scopes retained during the port.
#[allow(clippy::collapsible_if)]
pub unsafe fn submit_image(
    caller: Owner,
    timeline: u64,
    input: &Image,
) -> Result<queue::Status, Error> {
    let request = unsafe { &*input.request };
    let dependencies = if input.dependencies_len == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(input.dependencies, input.dependencies_len) }
    };
    let submission = super::queue_resources::Submission {
        deadline_ns: input.deadline,
        frame_key: input.frame_key,
        dependencies,
    };
    let result = {
        buffers::lock_prepared(buffers::Need {
            objects: 0,
            references: 0,
            leases: 2,
        })?;
        let _bo = BoLock;
        let catalog = unsafe { &mut *store() };
        let index = catalog
            .find(request.display_target)
            .map_err(|_| Error::Stale)?;
        let entry = &mut catalog.entries[index];
        if !entry.active || entry.mode_lost || entry.removing {
            return Err(Error::Unavailable);
        }
        if entry.mode_blocked {
            return Err(Error::Busy);
        }
        if entry.has_pending {
            if let Ok(status) = query(entry.pending_data) {
                if status.phase != queue::TERMINAL
                    || (!entry.direct && (status.device_active || status.resources_held))
                {
                    return Err(Error::Busy);
                }
            }
            entry.has_pending = false;
        }
        let descriptor = unsafe { buffers::read() }.describe(request.source, caller)?;
        let presentation = entry.info()?;
        if (request.operation != 5 && request.operation != 7)
            || descriptor.width != entry.width
            || descriptor.height != entry.height
            || descriptor.format != presentation.format
            || descriptor.plane_count != 1
            || descriptor.planes[0].offset != 0
        {
            return Err(Error::Unsupported);
        }
        let native_binding = binding(entry.binding);
        let accepted = unsafe {
            super::queue::submit_output_locked(
                caller,
                timeline,
                &submission,
                request,
                native_binding,
            )
        }?;
        entry.has_pending = true;
        entry.pending_data = accepted.fence;
        entry.direct = request.operation == 7;
        accepted
    };
    super::queue::wake();
    Ok(result)
}
pub fn info(target: a::GfxOutputTarget) -> Result<a::DisplayPresentationInfo, Error> {
    let _bo = BoLock::acquire();
    let catalog = unsafe { &*store() };
    let index = catalog.find(target)?;
    let entry = &catalog.entries[index];
    if entry.active {
        let current = outputs::active_target(
            u32::try_from(entry.driver.id).expect("driver id"),
            port(target),
            target.display_generation,
        )?;
        if !output_target::same(&current, &target) {
            return Err(Error::Stale);
        }
    }
    entry.info()
}
pub fn feedback(
    target: a::GfxOutputTarget,
    source: a::GfxFence,
) -> Result<a::DisplayPresentationStats, Error> {
    let _bo = BoLock::acquire();
    let catalog = unsafe { &*store() };
    let index = catalog.find(target)?;
    let entry = &catalog.entries[index];
    let mut value = entry
        .statistics
        .feedback(target.head_id, source, entry.snapshot())?;
    if entry.mode_lost {
        value.flags |= a::display_presentation_flag_lost;
    }
    Ok(value)
}
pub fn mode_binding(assignment: a::GfxScanoutState) -> Result<ModeBinding, Error> {
    let _bo = BoLock::acquire();
    for entry in &unsafe { &*store() }.entries {
        if entry.driver.id == 0 || !same(&port(entry.target), &assignment.output) {
            continue;
        }
        if entry.removing || entry.mode_lost || entry.target.head_id != assignment.head_id {
            return Err(Error::Stale);
        }
        return Ok(ModeBinding {
            driver: entry.driver,
            backend: entry.binding,
            target: entry.target,
            width: entry.width,
            height: entry.height,
            active: entry.active,
        });
    }
    Err(Error::Stale)
}
pub fn begin_mode(expected: ModeBinding) -> Result<(), Error> {
    let _bo = BoLock::acquire();
    let catalog = unsafe { &mut *store() };
    let index = catalog.find(expected.target)?;
    let entry = &mut catalog.entries[index];
    if entry.driver != expected.driver || !same(&entry.binding, &expected.backend) || entry.removing
    {
        return Err(Error::Stale);
    }
    if busy(entry.target) {
        return Err(Error::Busy);
    }
    entry.has_pending = false;
    entry.mode_blocked = true;
    Ok(())
}
pub fn finish_mode(
    expected: ModeBinding,
    width: u32,
    height: u32,
    active: bool,
    lost: bool,
) -> Result<(), Error> {
    {
        let _bo = BoLock::acquire();
        let catalog = unsafe { &mut *store() };
        let index = catalog.find(expected.target)?;
        let entry = &mut catalog.entries[index];
        if entry.driver != expected.driver
            || !same(&entry.binding, &expected.backend)
            || entry.removing
            || width == 0
            || height == 0
            || width > 65536
            || height > 65536
        {
            return Err(Error::Stale);
        }
        let _owner = ownership::enter_state();
        let _ = outputs::pause_additional_locked(
            u32::try_from(entry.driver.id).expect("driver id"),
            port(entry.target),
            !active || lost,
        )?;
        let _ = outputs::set_native_active_locked(
            u32::try_from(entry.driver.id).expect("driver id"),
            port(entry.target),
            entry.target.head_id,
            active && !lost,
        )?;
        entry.width = width;
        entry.height = height;
        entry.active = active && !lost;
        entry.mode_lost = lost;
        entry.mode_blocked = lost;
    }
    signal();
    Ok(())
}
pub fn publish_info(driver: Owner, value: a::DisplayPresentationInfo) -> Result<bool, Error> {
    let _bo = BoLock::acquire();
    let catalog = unsafe { &mut *store() };
    for entry in &mut catalog.entries {
        if entry.driver == driver && entry.target.display_generation == value.display_generation {
            entry.publish_info(driver, value)?;
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn publish_stats(driver: Owner, value: a::DisplayPresentationStats) -> Result<bool, Error> {
    let _bo = BoLock::acquire();
    let catalog = unsafe { &mut *store() };
    for entry in &mut catalog.entries {
        if entry.driver == driver && entry.target.display_generation == value.display_generation {
            entry.publish_stats(driver, value)?;
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn stopped_driver(driver: u32) {
    let _bo = BoLock::acquire();
    unsafe { &mut *store() }.stop(driver);
}
pub fn retire_after_reset(driver: Owner, backend: a::GfxBackendBinding) -> Result<(), Error> {
    let _admission = admission()?;
    for index in 0..model::CAPACITY {
        let target = {
            let _bo = BoLock::acquire();
            let entry = &unsafe { &*store() }.entries[index];
            if entry.driver != driver || !same(&entry.binding, &backend) {
                continue;
            }
            entry.target
        };
        transition(driver, target, 2, 1)?;
    }
    Ok(())
}
#[repr(C)]
pub struct Args {
    pub owner: Owner,
    pub target: a::GfxOutputTarget,
    pub backend: a::GfxBackendBinding,
    pub binding: ModeBinding,
    pub timeline: u64,
    pub value: u32,
    pub other: u32,
    pub width: u32,
    pub height: u32,
    pub flag: bool,
    pub lost: bool,
    pub input: *const c_void,
}
#[repr(C)]
pub struct Optional<T> {
    pub has: bool,
    pub value: MaybeUninit<T>,
}
/// # Safety
/// Exact initialized fixed arguments and operation-specific private/contract
/// buffers. Queue input transport retains the original task's request lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_additional_runtime(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { &*args };
    let result = match op {
        0 => register(args.owner, unsafe {
            args.input.cast::<a::GfxAdditionalOutput>().read()
        })
        .map(|v| unsafe { out.cast::<a::GfxOutputTarget>().write(v) }),
        1 => transition(args.owner, args.target, args.value, args.other),
        2 => target_at(args.value, args.other).map(|v| unsafe {
            out.cast::<Optional<a::GfxOutputTarget>>().write(Optional {
                has: v.is_some(),
                value: v.map_or(MaybeUninit::uninit(), MaybeUninit::new),
            })
        }),
        3 => {
            unsafe { out.cast::<bool>().write(contains(args.target)) };
            Ok(())
        }
        4 => unsafe { submit_image(args.owner, args.timeline, &*args.input.cast::<Image>()) }
            .map(|v| unsafe { out.cast::<queue::Status>().write(v) }),
        5 => {
            info(args.target).map(|v| unsafe { out.cast::<a::DisplayPresentationInfo>().write(v) })
        }
        6 => feedback(args.target, unsafe {
            args.input.cast::<a::GfxFence>().read()
        })
        .map(|v| unsafe { out.cast::<a::DisplayPresentationStats>().write(v) }),
        7 => mode_binding(unsafe { args.input.cast::<a::GfxScanoutState>().read() })
            .map(|v| unsafe { out.cast::<ModeBinding>().write(v) }),
        8 => begin_mode(args.binding),
        9 => finish_mode(args.binding, args.width, args.height, args.flag, args.lost),
        10 => publish_info(args.owner, unsafe {
            args.input.cast::<a::DisplayPresentationInfo>().read()
        })
        .map(|v| unsafe { out.cast::<bool>().write(v) }),
        11 => publish_stats(args.owner, unsafe {
            args.input.cast::<a::DisplayPresentationStats>().read()
        })
        .map(|v| unsafe { out.cast::<bool>().write(v) }),
        12 => {
            stopped_driver(u32::try_from(args.owner.id).expect("driver id"));
            Ok(())
        }
        13 => retire_after_reset(args.owner, args.backend),
        _ => panic!("additional output runtime operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
