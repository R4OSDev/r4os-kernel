//! BO retention for additional heads. Driver receipts, exact references and
//! partial successful release prefixes retain their original ownership.
use super::{
    equal::same,
    error::Error,
    mode_color::DISPLAY_OWNER as OWNER,
    native_types::AdditionalBinding as Binding,
    ownership::{self, Resident},
};
use crate::{
    contract as a,
    gfx::{
        buffer_api, buffers, layout,
        types::{Handle, Owner},
    },
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
#[derive(Clone, Copy, Default)]
struct Surface {
    reference: Handle,
    driver_reference: Handle,
    read_lease: Handle,
    bytes: u64,
}
#[derive(Clone, Copy)]
struct Consumer {
    binding: Binding,
    surface: Surface,
}
#[derive(Clone, Copy)]
struct Change {
    index: usize,
    binding: Binding,
    old: Surface,
    new: Surface,
    width: u32,
    height: u32,
    using_new: bool,
}
static RESIDENTS: Resident<[Option<Consumer>; a::gfx_output_max_assignments as usize]> = Resident(
    UnsafeCell::new([None; a::gfx_output_max_assignments as usize]),
);
static CHANGE: Resident<Option<Change>> = Resident(UnsafeCell::new(None));
static EXECUTION: Resident<crate::sched::sync_types::UnwindGuard> = Resident(UnsafeCell::new(
    ownership::execution(b"additional-native-mode"),
));
unsafe extern "C" {}
fn execution() -> Result<ownership::RecursiveExecution, Error> {
    unsafe { ownership::RecursiveExecution::acquire(EXECUTION.0.get()) }
}
fn same_consumer(left: Binding, driver: Owner, target: a::GfxOutputTarget) -> bool {
    left.driver == driver
        && left.target.display_generation == target.display_generation
        && left.target.adapter_id == target.adapter_id
        && left.target.device_generation == target.device_generation
        && left.target.connector_id == target.connector_id
        && left.target.head_id == target.head_id
}
fn same_binding(left: Binding, right: Binding) -> bool {
    left.driver == right.driver
        && same(&left.backend, &right.backend)
        && same(&left.target, &right.target)
        && left.width == right.width
        && left.height == right.height
        && left.active == right.active
}
fn validate_queue(expected: Binding) -> Result<(), Error> {
    super::queue::validate_output_binding(
        u32::try_from(expected.driver.id).expect("driver id"),
        expected.backend,
    )
}
fn begin(expected: Binding) -> Result<(), Error> {
    super::output_runtime::begin_mode(expected)
}
fn finish(
    expected: Binding,
    width: u32,
    height: u32,
    active: bool,
    lost: bool,
) -> Result<(), Error> {
    super::output_runtime::finish_mode(expected, width, height, active, lost)
}
struct BoLock;
impl Drop for BoLock {
    fn drop(&mut self) {
        buffers::unlock()
    }
}
pub fn prepare(
    caller: Owner,
    assignment: a::GfxScanoutState,
    mode: a::GfxOutputMode,
    expected: Binding,
) -> Result<a::GfxBufferReference, Error> {
    let _execution = execution()?;
    if unsafe { (*CHANGE.0.get()).is_some() } {
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
    let current = super::output_runtime::mode_binding(assignment)?;
    if !same_binding(current, expected) {
        return Err(Error::Stale);
    }
    validate_queue(expected)?;
    let mut free = None;
    let mut found = None;
    for (index, slot) in unsafe { &*RESIDENTS.0.get() }.iter().enumerate() {
        if let Some(value) = slot {
            if same_consumer(value.binding, expected.driver, expected.target) {
                found = Some(index);
                break;
            }
        } else if free.is_none() {
            free = Some(index);
        }
    }
    let index = found.or(free).ok_or(Error::Capacity)?;
    let old =
        unsafe { (*RESIDENTS.0.get())[index] }.map_or(Surface::default(), |value| value.surface);
    begin(expected)?;
    let prepared = (|| -> Result<_, Error> {
        buffers::lock_prepared(buffers::Need {
            objects: 0,
            references: 2,
            leases: 1,
        })?;
        let _lock = BoLock;
        let descriptor = unsafe { buffers::read() }.describe(source, caller)?;
        let usage = 2 | 4 | 32;
        if descriptor.format != layout::XRGB8888
            || descriptor.location != 0
            || !descriptor.binding.portable()
            || descriptor.modifier != 0
            || descriptor.plane_count != 1
            || descriptor.planes[0].offset != 0
            || descriptor.usage & usage != usage
            || descriptor.width != mode.width
            || descriptor.height != mode.height
            || descriptor.planes[0].pitch != u64::from(mode.width) * 4
            || descriptor.bytes != descriptor.planes[0].pitch * u64::from(mode.height)
        {
            return Err(Error::Unsupported);
        }
        if old.reference.id != 0
            && unsafe { buffers::read() }.buffer_for(source, caller)?
                == unsafe { buffers::read() }.buffer_for(old.reference, OWNER)?
        {
            return Err(Error::Invalid);
        }
        let reference = unsafe { buffers::table() }.share(source, OWNER)?;
        let result = (|| -> Result<_, Error> {
            let driver_reference = unsafe { buffers::table() }.share(source, expected.driver)?;
            let result = (|| -> Result<_, Error> {
                let read = unsafe { buffers::table() }.use_resource(
                    reference,
                    OWNER,
                    2,
                    0,
                    descriptor.bytes,
                )?;
                let result =
                    unsafe { buffer_api::reference_locked(driver_reference, expected.driver) };
                if result.is_err() {
                    unsafe { buffers::table() }
                        .end_use(read.lease, OWNER, true)
                        .expect("additional mode exact read");
                }
                result
                    .map(|wire| {
                        (
                            Surface {
                                reference,
                                driver_reference,
                                read_lease: read.lease,
                                bytes: descriptor.bytes,
                            },
                            wire,
                        )
                    })
                    .map_err(Error::from)
            })();
            if result.is_err() {
                unsafe { buffers::table() }
                    .drop_reference(driver_reference, expected.driver)
                    .expect("additional mode driver reference");
            }
            result
        })();
        if result.is_err() {
            unsafe { buffers::table() }
                .drop_reference(reference, OWNER)
                .expect("additional mode reference");
        }
        result
    })();
    let (surface, wire) = match prepared {
        Ok(value) => value,
        Err(error) => {
            let _ = finish(
                expected,
                expected.width,
                expected.height,
                expected.active,
                false,
            );
            return Err(error);
        }
    };
    unsafe {
        (*RESIDENTS.0.get())[index] = Some(Consumer {
            binding: expected,
            surface: old,
        });
        CHANGE.0.get().write(Some(Change {
            index,
            binding: expected,
            old,
            new: surface,
            width: mode.width,
            height: mode.height,
            using_new: false,
        }));
    }
    Ok(wire)
}
fn change_pointer() -> Result<*mut Change, Error> {
    unsafe {
        (*CHANGE.0.get())
            .as_mut()
            .map(ptr::from_mut)
            .ok_or(Error::Stale)
    }
}
pub fn start(operation: u32, expected: Binding) -> Result<(), Error> {
    let _execution = execution()?;
    let change = change_pointer()?;
    let value = unsafe { change.read() };
    if !same_binding(value.binding, expected) {
        return Err(Error::Stale);
    }
    validate_queue(expected)?;
    begin(expected)?;
    let surface = match operation {
        a::gfx_mode_operation_apply => unsafe { ptr::addr_of_mut!((*change).old) },
        a::gfx_mode_operation_rollback => unsafe { ptr::addr_of_mut!((*change).new) },
        a::gfx_mode_operation_confirm => return Ok(()),
        _ => return Err(Error::Invalid),
    };
    let value = unsafe { surface.read() };
    if value.reference.id != 0 && value.read_lease.id == 0 {
        buffers::lock_prepared(buffers::Need {
            objects: 0,
            references: 0,
            leases: 1,
        })?;
        let _lock = BoLock;
        let read =
            unsafe { buffers::table() }.use_resource(value.reference, OWNER, 2, 0, value.bytes)?;
        unsafe { ptr::addr_of_mut!((*surface).read_lease).write(read.lease) };
    }
    Ok(())
}
unsafe fn release_read(surface: *mut Surface) -> Result<(), Error> {
    let read = unsafe { ptr::addr_of!((*surface).read_lease).read() };
    if read.id == 0 {
        return Ok(());
    }
    buffers::lock();
    let _lock = BoLock;
    unsafe { buffers::table() }.end_use(read, OWNER, true)?;
    unsafe { ptr::addr_of_mut!((*surface).read_lease).write(Handle::EMPTY) };
    Ok(())
}
unsafe fn release_surface(driver: Owner, surface: *mut Surface) -> Result<(), Error> {
    unsafe { release_read(surface) }?;
    let reference = unsafe { ptr::addr_of!((*surface).driver_reference).read() };
    if reference.id != 0 {
        buffers::drop_reference(reference, driver)?;
        unsafe { ptr::addr_of_mut!((*surface).driver_reference).write(Handle::EMPTY) };
    }
    let reference = unsafe { ptr::addr_of!((*surface).reference).read() };
    if reference.id != 0 {
        buffers::drop_reference(reference, OWNER)?;
        unsafe { ptr::addr_of_mut!((*surface).reference).write(Handle::EMPTY) };
    }
    Ok(())
}
pub fn settle(operation: u32, outcome: u32) -> Result<(), Error> {
    let _execution = execution()?;
    let change = change_pointer()?;
    let value = unsafe { change.read() };
    let expected = value.binding;
    if outcome == a::gfx_output_outcome_lost {
        finish(
            expected,
            if value.using_new {
                value.width
            } else {
                expected.width
            },
            if value.using_new {
                value.height
            } else {
                expected.height
            },
            false,
            true,
        )?;
        return Ok(());
    }
    if outcome == a::gfx_output_outcome_applied {
        if operation == a::gfx_mode_operation_apply {
            unsafe { release_read(ptr::addr_of_mut!((*change).new)) }?;
            finish(expected, value.width, value.height, expected.active, false)?;
            unsafe {
                (*RESIDENTS.0.get())[value.index]
                    .as_mut()
                    .expect("additional mode resident")
                    .surface = (*change).new;
                (*change).using_new = true;
            }
            return Ok(());
        }
        if operation != a::gfx_mode_operation_confirm || !value.using_new {
            return Err(Error::Invalid);
        }
        finish(expected, value.width, value.height, expected.active, false)?;
        unsafe { release_surface(expected.driver, ptr::addr_of_mut!((*change).old)) }?;
        unsafe {
            (*RESIDENTS.0.get())[value.index]
                .as_mut()
                .expect("additional mode resident")
                .surface = (*change).new;
        }
    } else if outcome == a::gfx_output_outcome_old_preserved {
        unsafe { release_read(ptr::addr_of_mut!((*change).old)) }?;
        finish(
            expected,
            expected.width,
            expected.height,
            expected.active,
            false,
        )?;
        unsafe { release_surface(expected.driver, ptr::addr_of_mut!((*change).new)) }?;
        unsafe {
            (*RESIDENTS.0.get())[value.index]
                .as_mut()
                .expect("additional mode resident")
                .surface = (*change).old;
        }
    } else {
        return Err(Error::Invalid);
    }
    unsafe { CHANGE.0.get().write(None) };
    Ok(())
}
pub fn retire(driver: Owner, target: a::GfxOutputTarget) -> Result<(), Error> {
    let _execution = execution()?;
    for index in 0..a::gfx_output_max_assignments as usize {
        let slot = unsafe { (*RESIDENTS.0.get())[index] };
        let Some(value) = slot else {
            continue;
        };
        if !same_consumer(value.binding, driver, target) {
            continue;
        }
        if unsafe { *CHANGE.0.get() }.is_some_and(|pending| pending.index == index) {
            return Err(Error::Busy);
        }
        let surface = unsafe {
            ptr::addr_of_mut!(
                (*RESIDENTS.0.get())[index]
                    .as_mut()
                    .expect("resident")
                    .surface
            )
        };
        unsafe { release_surface(driver, surface) }?;
        unsafe { (*RESIDENTS.0.get())[index] = None };
        return Ok(());
    }
    Ok(())
}
pub fn retire_after_reset(driver: Owner, backend: a::GfxBackendBinding) -> Result<(), Error> {
    let _execution = execution()?;
    if let Ok(change) = change_pointer() {
        let value = unsafe { change.read() };
        if value.binding.driver == driver && same(&value.binding.backend, &backend) {
            unsafe { (*RESIDENTS.0.get())[value.index] = None };
            unsafe { release_surface(driver, ptr::addr_of_mut!((*change).old)) }?;
            unsafe { release_surface(driver, ptr::addr_of_mut!((*change).new)) }?;
            unsafe { CHANGE.0.get().write(None) };
        }
    }
    for index in 0..a::gfx_output_max_assignments as usize {
        let Some(value) = (unsafe { (*RESIDENTS.0.get())[index] }) else {
            continue;
        };
        if value.binding.driver != driver || !same(&value.binding.backend, &backend) {
            continue;
        }
        let surface = unsafe {
            ptr::addr_of_mut!(
                (*RESIDENTS.0.get())[index]
                    .as_mut()
                    .expect("resident")
                    .surface
            )
        };
        unsafe { release_surface(driver, surface) }?;
        unsafe { (*RESIDENTS.0.get())[index] = None };
    }
    Ok(())
}
#[repr(C)]
pub struct Args {
    pub owner: Owner,
    pub expected: Binding,
    pub target: a::GfxOutputTarget,
    pub backend: a::GfxBackendBinding,
    pub assignment: a::GfxScanoutState,
    pub mode: a::GfxOutputMode,
    pub operation: u32,
    pub outcome: u32,
}
/// # Safety
/// Copied fixed private arguments and exact initialized output. Physical idle
/// remains the caller's proved input to terminal retirement, never inferred.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_additional_mode(
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { &*args };
    let result = match op {
        0 => prepare(args.owner, args.assignment, args.mode, args.expected)
            .map(|value| unsafe { out.cast::<a::GfxBufferReference>().write(value) }),
        1 => start(args.operation, args.expected),
        2 => settle(args.operation, args.outcome),
        3 => retire(args.owner, args.target),
        4 => retire_after_reset(args.owner, args.backend),
        _ => panic!("additional mode operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
