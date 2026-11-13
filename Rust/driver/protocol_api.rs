//! Original PPI1 table and exact task-bound execution scopes. Module owners
//! serialize stateful callbacks; nested calls restore their previous identity.
use crate::{
    arch::runtime::RuntimeGuard,
    sched::{
        scheduler,
        task::{task_registry, task_types::Task},
        task_context::Admission,
    },
};
use crate::{contract, driver::protocol_registry, memory::heap};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem,
    sync::atomic::{AtomicUsize, Ordering},
};
const NO_SLOT: usize = usize::MAX;
// Before task creation only the exclusive bootstrap may execute PPI code.
static BOOT_SLOT: AtomicUsize = AtomicUsize::new(NO_SLOT);

pub(crate) struct Execution {
    task: *mut Task,
    previous: usize,
    _unwind: Admission,
}
impl Execution {
    /// The caller retains the exact module's sleepable call owner and code.
    pub(crate) fn enter(slot: usize) -> Option<Self> {
        let task = scheduler::current().cast::<Task>();
        let unwind = Admission::acquire(!task.is_null())?;
        let _runtime = RuntimeGuard::acquire();
        assert_eq!(
            scheduler::current().cast::<Task>(),
            task,
            "PPI admission task"
        );
        let previous = if task.is_null() {
            BOOT_SLOT.swap(slot, Ordering::Relaxed)
        } else {
            unsafe {
                let core = task_registry::core(task);
                let previous = (*core).protocol_slot;
                (*core).protocol_slot = slot;
                previous
            }
        };
        Some(Self {
            task,
            previous,
            _unwind: unwind,
        })
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        let _runtime = RuntimeGuard::acquire();
        assert_eq!(
            scheduler::current().cast::<Task>(),
            self.task,
            "PPI task scope"
        );
        if self.task.is_null() {
            BOOT_SLOT.store(self.previous, Ordering::Relaxed);
        } else {
            unsafe { (*task_registry::core(self.task)).protocol_slot = self.previous };
        }
    }
}
fn current_slot() -> usize {
    let _runtime = RuntimeGuard::acquire();
    let task = scheduler::current().cast::<Task>();
    if task.is_null() {
        BOOT_SLOT.load(Ordering::Relaxed)
    } else {
        unsafe { (*task_registry::core(task)).protocol_slot }
    }
}
struct Table(UnsafeCell<contract::ProtocolApi>);
// The table is installed before R4P init and retained through module calls.
unsafe impl Sync for Table {}
const TEMPLATE: contract::ProtocolApi = contract::ProtocolApi {
    magic: contract::protocol_magic,
    version: contract::protocol_api_version,
    size: mem::size_of::<contract::ProtocolApi>() as u32,
    reserved: 0,
    log_info,
    log_warn,
    log_error,
    register_role,
    set_status,
    dependency_status,
    alloc,
    free,
    file_read,
    cpu_acceleration_info: Some(crate::program::r4dev::cpu_capabilities::acceleration_info),
    cpu_acceleration_work: Some(crate::program::r4dev::cpu_capabilities::acceleration_work),
    entropy_source_info: Some(crate::driver::entropy::info),
    entropy_fill: Some(crate::driver::entropy::fill),
};
static TABLE: Table = Table(UnsafeCell::new(TEMPLATE));
static TABLE2: contract::ProtocolApi2 = contract::ProtocolApi2 {
    magic: contract::protocol_api2_magic,
    version: contract::protocol_api2_version,
    size: mem::size_of::<contract::ProtocolApi2>() as u32,
    reserved: 0,
    log_info,
    log_warn,
    log_error,
    register_role,
    set_status,
    dependency_status,
    alloc,
    free,
    file_read,
    cpu_acceleration_info: Some(crate::program::r4dev::cpu_capabilities::acceleration_info),
    cpu_acceleration_work: Some(crate::program::r4dev::cpu_capabilities::acceleration_work),
    entropy_source_info: Some(crate::driver::entropy::info),
    entropy_fill: Some(crate::driver::entropy::fill),
    protocol_dispatch: Some(protocol_dispatch),
    link_io: Some(link_io),
    process_status: Some(crate::program::info::r4os_rust_program_info_handle),
};

pub(crate) fn table2() -> *const contract::ProtocolApi2 {
    core::ptr::addr_of!(TABLE2)
}

unsafe extern "C" fn protocol_dispatch(
    role: *const u8,
    len: u32,
    op: u32,
    input: *const contract::ProtocolBuffer,
    output: *mut contract::ProtocolBuffer,
) -> i32 {
    let slot = current_slot();
    if slot == NO_SLOT
        || role.is_null()
        || len == 0
        || len > 64
        || input.is_null()
        || output.is_null()
    {
        return contract::network_error_invalid;
    }
    let bytes = unsafe { core::slice::from_raw_parts(role, len as usize) };
    if !crate::loader::r4p::declares_dependency(slot, bytes) {
        return contract::network_error_unsupported_policy;
    }
    unsafe {
        crate::loader::r4p::r4os_rust_r4p_dispatch(
            crate::fs::vfs_types::Bytes {
                data: role,
                len: len as usize,
            },
            op,
            input,
            output,
        )
    }
}

unsafe extern "C" fn link_io(
    input: *const contract::NetworkLinkRequest,
    output: *mut contract::NetworkLinkInfo,
) -> i32 {
    let slot = current_slot();
    if slot == NO_SLOT {
        return contract::network_error_backend;
    }
    let Some(provider) = crate::loader::r4p::provider_identity(slot) else {
        return contract::network_error_provider_changed;
    };
    unsafe { crate::net::protocol_link::call(provider, input, output) }
}

unsafe fn length(text: *const u8) -> usize {
    let mut len = 0;
    while len < 512 && unsafe { text.add(len).read() != 0 } {
        len += 1;
    }
    len
}
fn slot_id() -> u32 {
    let slot = current_slot();
    if slot == NO_SLOT {
        u32::MAX
    } else {
        u32::try_from(slot).expect("Protocol slot exceeds u32")
    }
}
unsafe extern "C" fn log_info(text: *const u8) {
    unsafe { crate::boot::log_event::protocol(0, slot_id(), text) };
}
unsafe extern "C" fn log_warn(text: *const u8) {
    unsafe { crate::boot::log_event::protocol(1, slot_id(), text) };
}
unsafe extern "C" fn log_error(text: *const u8) {
    unsafe { crate::boot::log_event::protocol(2, slot_id(), text) };
}
unsafe extern "C" fn register_role(role: *const u8, _category: u32, _flags: u32) -> i32 {
    let slot = current_slot();
    if slot == NO_SLOT {
        return -6;
    }
    unsafe {
        let len = length(role);
        if protocol_registry::r4os_rust_protocol_registry_query(3, slot, role, len) == 0 {
            let note = b"registered role does not match R4P header";
            protocol_registry::r4os_rust_protocol_registry_mutate(
                slot,
                1,
                0,
                -2,
                note.as_ptr(),
                note.len(),
            );
            return -2;
        }
    }
    0
}
unsafe extern "C" fn set_status(state: u32, note: *const u8) -> i32 {
    let slot = current_slot();
    if slot == NO_SLOT {
        return -6;
    }
    unsafe {
        protocol_registry::r4os_rust_protocol_registry_mutate(slot, 4, state, 0, note, length(note))
    };
    0
}
unsafe extern "C" fn dependency_status(role: *const u8) -> i32 {
    unsafe { protocol_registry::r4os_rust_protocol_registry_query(2, 0, role, length(role)) }
}
unsafe extern "C" fn alloc(bytes: u32, alignment: u32) -> *mut c_void {
    heap::r4os_rust_heap_alloc(bytes as usize, alignment as usize).cast()
}
unsafe extern "C" fn free(pointer: *mut c_void, bytes: u32) {
    if pointer.is_null() || bytes == 0 {
        return;
    }
    unsafe { heap::r4os_rust_heap_free(pointer.cast(), bytes as usize) };
}
unsafe extern "C" fn file_read(path: *const u8, out: *mut u8, len: u32) -> i32 {
    unsafe { protocol_file_read(path, out, len) }
}
/// # Safety
/// R4P receives the canonical Contract table for its existing guarded module
/// lifetime. Trusted consumers preserve the established table contents.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_protocol_table() -> *mut contract::ProtocolApi {
    TABLE.0.get()
}
const _: () = {
    assert!(mem::size_of::<contract::ProtocolApi>() == 120);
    assert!(mem::offset_of!(contract::ProtocolApi, entropy_source_info) == 104);
    assert!(mem::offset_of!(contract::ProtocolApi, entropy_fill) == 112);
    assert!(mem::offset_of!(contract::ProtocolApi, file_read) == 80);
    assert!(mem::offset_of!(contract::ProtocolApi, cpu_acceleration_info) == 88);
    assert!(mem::offset_of!(contract::ProtocolApi, cpu_acceleration_work) == 96);
    assert!(contract::protocol_api_version == 1);
};

unsafe fn protocol_file_read(path: *const u8, out: *mut u8, len: u32) -> i32 {
    unsafe { crate::program::system::io::r4os_rust_sys_file_read(path.cast(), out.cast(), len) }
}
