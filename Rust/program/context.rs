//! Current execution identity. Exception attribution never waits or pins;
//! registry iteration is only the original cooperative normal-context fallback.
use super::{registry, registry_state, registry_types::ProgramInstance, threads};
use crate::{
    contract::ProgramProcessHandle as Handle,
    sched::{scheduler, task::task_registry},
};
use core::ptr;
#[repr(C)]
pub struct AsyncExecution {
    pub instance: *mut ProgramInstance,
    pub id: u32,
    pub reserved: u32,
    pub generation: u64,
}

pub(super) unsafe fn execution() -> *mut ProgramInstance {
    unsafe {
        let thread = threads::current();
        if !thread.is_null() {
            return (*thread).owner_instance;
        }
        super::async_io::execution().map_or(ptr::null_mut(), |owner| owner.instance)
    }
}
pub(super) unsafe fn current() -> *mut ProgramInstance {
    unsafe {
        let task = scheduler::current();
        if task.is_null() {
            return ptr::null_mut();
        }
        let id = (*task_registry::core(task.cast())).id;
        let instance = execution();
        if !instance.is_null() {
            return instance;
        }
        let mut iterator = registry::Iterator {
            chunk: (*registry_state::state()).head,
            slot_index: 0,
            include_done: true,
        };
        loop {
            let instance = registry::r4os_rust_program_registry_iterator(&mut iterator);
            if instance.is_null() {
                return instance;
            }
            if (*instance).task_id == id {
                return instance;
            }
        }
    }
}
pub(super) unsafe fn handle() -> Option<Handle> {
    unsafe {
        let thread = threads::current();
        let handle = if !thread.is_null() {
            Handle {
                instance_id: (*thread).instance_id,
                reserved: 0,
                generation: (*thread).instance_generation,
            }
        } else {
            let owner = super::async_io::execution()?;
            Handle {
                instance_id: owner.id,
                reserved: 0,
                generation: owner.generation,
            }
        };
        if registry::valid(handle) {
            Some(handle)
        } else {
            None
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_context_current(execution_only: bool) -> *mut ProgramInstance {
    unsafe {
        if execution_only {
            execution()
        } else {
            current()
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_context_memory_owner() -> u64 {
    unsafe {
        let instance = execution();
        if instance.is_null() {
            0
        } else {
            u64::from((*instance).id)
        }
    }
}
/// # Safety
/// Resident exclusive handle output. Absence leaves its bytes unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_context_handle(out: *mut Handle) -> bool {
    unsafe {
        let Some(handle) = handle() else {
            return false;
        };
        out.write(handle);
        true
    }
}
const _: () = {
    assert!(core::mem::size_of::<AsyncExecution>() == 24);
    assert!(core::mem::offset_of!(AsyncExecution, generation) == 16);
};
