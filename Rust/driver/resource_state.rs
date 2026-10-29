//! Exact existing owner/epoch resource handles, under DriverApi's wait-spanning
//! owner admission. These C records own no file lease, path or backing buffer.
use crate::{contract, driver::owner_table::Rows};
use core::{mem, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Binding {
    pub epoch: u64,
    pub slot: usize,
    pub module_generation: u32,
    pub closing: bool,
}
impl Binding {
    const EMPTY: Self = Self {
        epoch: 0,
        slot: 0,
        module_generation: 0,
        closing: false,
    };
}
#[repr(C)]
pub struct State {
    pub next_epoch: u64,
    pub bindings: Rows<Binding>,
}
const _: () = {
    assert!(mem::size_of::<Binding>() == 24);
    assert!(mem::offset_of!(Binding, module_generation) == 16);
    assert!(mem::size_of::<State>() == 16);
    assert!(mem::offset_of!(State, bindings) == 8);
};
unsafe fn current(state: *const State, owner: u32) -> Result<*const Binding, u8> {
    if owner == 0 {
        return Err(1);
    }
    let binding = unsafe { (*state).bindings.get(owner as usize - 1).cast_const() };
    if binding.is_null() || unsafe { (*binding).epoch == 0 || (*binding).closing } {
        return Err(3);
    }
    Ok(binding)
}

/// # Safety
/// Exclusive initialized exact C state under the existing caller owner or
/// isolated fixture lifetime. Mutations are bounded and retain no borrow.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_resource_state_mutate(
    state: *mut State,
    operation: u8,
    owner: u32,
    slot: usize,
    generation: u32,
) -> u8 {
    match operation {
        0 => {
            if owner == 0 || generation == 0 {
                return 1;
            }
            let binding = unsafe { (*state).bindings.get(owner as usize - 1) };
            if binding.is_null() {
                return 4;
            }
            if unsafe { (*binding).epoch != 0 } {
                return 2;
            }
            let epoch = unsafe { (*state).next_epoch };
            if epoch > 0x00ff_ffff_ffff_ffff {
                return 4;
            }
            unsafe {
                binding.write(Binding {
                    epoch,
                    slot,
                    module_generation: generation,
                    closing: false,
                });
                (*state).next_epoch += 1;
            }
        }
        1 | 2 => {
            if owner != 0 {
                let binding = unsafe { (*state).bindings.get(owner as usize - 1) };
                if binding.is_null() {
                    return 0;
                }
                if operation == 1 {
                    unsafe { (*binding).closing = true };
                } else {
                    unsafe { binding.write(Binding::EMPTY) };
                }
            }
        }
        _ => panic!("Invalid resource state mutation"),
    }
    0
}

/// # Safety
/// Initialized aligned state under the caller's owner or fixture lifetime;
/// output is exclusive C pointer storage. Returned raw binding is caller-owned
/// stable storage with the same guard/lifetime as before, never a Rust borrow.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_resource_current(
    state: *const State,
    owner: u32,
    output: *mut *const Binding,
) -> u8 {
    match unsafe { current(state, owner) } {
        Ok(binding) => {
            unsafe { output.write(binding) };
            0
        }
        Err(code) => code,
    }
}

/// # Safety
/// State is readable under its exact owner or isolated fixture lifetime and
/// output is exclusive C scalar storage. Queries acquire no resource lease.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_resource_handle(
    state: *const State,
    operation: u8,
    owner: u32,
    value: u64,
    output: *mut u64,
) -> u8 {
    let result = (|| match operation {
        0 => {
            if value >= 64 {
                return Err(4);
            }
            let binding = unsafe { current(state, owner) }?;
            Ok((unsafe { (*binding).epoch } << 8) | (value + 1))
        }
        1 => {
            let binding = unsafe { current(state, owner) }?;
            let index = value & 255;
            if value >> 8 != unsafe { (*binding).epoch } || index == 0 || index > 64 {
                return Err(3);
            }
            Ok(index - 1)
        }
        _ => panic!("Invalid resource handle query"),
    })();
    match result {
        Ok(value) => {
            unsafe { output.write(value) };
            0
        }
        Err(code) => code,
    }
}

/// # Safety
/// Output permits reading its version/size header and writing its declared
/// prefix. Input is a complete aligned canonical Contract table, disjoint.
/// Optional trailing fields and bytes beyond the accepted prefix stay intact.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_resource_publish(
    output: *mut contract::DriverResourceApi,
    input: *const contract::DriverResourceApi,
) -> bool {
    if output.is_null()
        || !output
            .addr()
            .is_multiple_of(mem::align_of::<contract::DriverResourceApi>())
    {
        return false;
    }
    let (version, capacity) = unsafe {
        (
            ptr::addr_of!((*output).version).read(),
            ptr::addr_of!((*output).size).read(),
        )
    };
    if version != 1 || capacity < 32 {
        return false;
    }
    let bytes = (capacity & !7).min(mem::size_of::<contract::DriverResourceApi>() as u32);
    let mut value = unsafe { input.read() };
    value.size = bytes;
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(value).cast::<u8>(),
            output.cast::<u8>(),
            bytes as usize,
        )
    };
    true
}
