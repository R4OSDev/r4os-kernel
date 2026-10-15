//! Canonical short storage metadata owner; callbacks/waits never span its guard.
use crate::storage::access_state::{
    Claim, Device, Error, Input, MAX_DEVICES, MAX_MOUNTS, Mount, Owner, Reference, State,
};
use crate::{
    memory::owner::Guard,
    sched::{scheduler, task},
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
type Resolver = unsafe extern "C" fn(*mut Owner) -> bool;
struct Canonical(UnsafeCell<State>);
// Every runtime access is serialized by the original storage owner. Init and
// resolver installation retain the original exclusive BSP boot boundary.
unsafe impl Sync for Canonical {}
static STATE: Canonical = Canonical(UnsafeCell::new(State::EMPTY));
struct Binding(UnsafeCell<Option<Resolver>>);
unsafe impl Sync for Binding {}
static RESOLVER: Binding = Binding(UnsafeCell::new(None));
/// # Safety
/// Original boot-only reset: no live use, mount, claim or callback survives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_access_init() {
    // Initialize in resident storage, never materialize the 24-KB State on the
    // shared kernel stack. Zero is valid for all C scalars and inactive kinds.
    unsafe {
        ptr::write_bytes(STATE.0.get(), 0, 1);
        (*STATE.0.get()).topology_generation = 1;
        RESOLVER.0.get().write(None)
    };
}
/// # Safety
/// Same BSP setup boundary; callback is immutable kernel code and writes one
/// exact task/program Owner only on success. No borrowed owner storage escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_access_resolver(resolver: Resolver) {
    unsafe { RESOLVER.0.get().write(Some(resolver)) };
}
/// # Safety
/// Current task stays resident throughout this synchronous non-waiting call;
/// output is exclusive aligned storage. Resolver runs before storage admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_access_current(output: *mut Owner) -> bool {
    if let Some(resolve) = unsafe { RESOLVER.0.get().read() }
        && unsafe { resolve(output) }
    {
        return true;
    }
    let running = scheduler::current();
    if running.is_null() {
        return false;
    }
    let (id, generation) = unsafe { task::immutable_identity(running) };
    unsafe {
        output.write(Owner {
            task: id,
            task_generation: generation,
            ..Owner::default()
        })
    };
    true
}
fn matches(a: Owner, b: Owner) -> bool {
    a.program == b.program
        && a.program_generation == b.program_generation
        && (b.task == 0 || (a.task == b.task && a.task_generation == b.task_generation))
}
/// # Safety
/// Input and outputs are initialized/disjoint resident C records. `locked`
/// means the block/mount caller already owns the exact storage lock. Otherwise
/// this entry acquires it. No callback, I/O, allocation or wait occurs inside.
/// Snapshot output for op8/14 holds one exclusive Mount/Claim respectively;
/// scalar output is written only on success. No registry pointer is returned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_storage_access_runtime(
    op: u8,
    locked: bool,
    input: *const Input,
    output: *mut u64,
    snapshot: *mut c_void,
) -> i32 {
    let a = unsafe { input.read() };
    let _guard = if locked {
        None
    } else {
        Some(Guard::acquire(1))
    };
    let s = unsafe { &mut *STATE.0.get() };
    let result = match op {
        19 => Ok(s.topology_generation),
        20 => {
            let i = a.reference.slot as usize;
            if i >= MAX_DEVICES || !s.devices[i].active {
                Err(Error::Stale)
            } else {
                Ok(s.devices[i].generation)
            }
        }
        21 => {
            let i = a.reference.slot as usize;
            if i >= MAX_MOUNTS || !s.mounts[i].active {
                Err(Error::Stale)
            } else {
                Ok(s.mounts[i].generation)
            }
        }
        22 => {
            for u in &mut s.uses {
                if u.id != 0 && u.kind == 1 && matches(u.owner, a.owner) {
                    u.id = 0;
                }
            }
            Ok(0)
        }
        23 => Ok(u64::from(
            s.claims
                .iter()
                .any(|c| c.id != 0 && matches(c.owner, a.owner)),
        )),
        24 => Ok(u64::from(s.region_claimed(a.region))),
        _ => s.apply(op, a),
    };
    match result {
        Ok(value) => {
            match op {
                8 => unsafe { snapshot.cast::<Mount>().write(s.mounts[value as usize]) },
                14 => unsafe { snapshot.cast::<Claim>().write(s.claims[value as usize]) },
                5 => unsafe { snapshot.cast::<Device>().write(s.devices[value as usize]) },
                _ => {}
            }
            unsafe { output.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}
const _: () = {
    assert!(core::mem::size_of::<Reference>() == 16);
};
