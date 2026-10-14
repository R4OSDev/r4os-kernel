//! Original terminal driver cleanup, prepared storage tokens and quarantine.
//! Rust owns the ordering; foreign owners acknowledge their exact resources.
use crate::driver::{
    api_admission as admission, api_audio as audio, api_dma as dma, api_msi as msi, api_net as net,
    api_storage as storage, heap, irq_router, resource_state, resources, semaphores, threads, work,
};
use core::{
    mem::{MaybeUninit, offset_of, size_of},
    ptr,
};
#[repr(C)]
pub struct Token {
    pub owner: u32,
    pub storage_plan: storage::Plan,
    pub net_mutation_active: bool,
    pub display_blit_prepared: bool,
    pub audio_prepared: bool,
    pub shutdown_started: bool,
    pub active: bool,
}
impl Token {
    fn empty(owner: u32) -> Self {
        Self {
            owner,
            storage_plan: storage::Plan::default(),
            net_mutation_active: false,
            display_blit_prepared: false,
            audio_prepared: false,
            shutdown_started: false,
            active: false,
        }
    }
}
#[repr(C)]
#[derive(Default)]
pub struct Counts {
    pub irq: u32,
    pub work: u32,
    pub threads: u64,
    pub semaphores: u64,
    pub dma: u32,
    pub msi: u32,
    pub cpu_heap: u64,
    pub cpu_bytes: u64,
    pub audio: u32,
    pub storage: u32,
    pub usb_host: u32,
    pub display_blit: u32,
    pub net: u32,
}
#[repr(C)]
struct BackendCleanup {
    removed: u32,
    failed: bool,
}
unsafe extern "C" {
    fn r4os_zig_driver_cleanup_owner(op: u8, owner: u32) -> u64;
    fn r4os_zig_driver_usb_cleanup(owner: u32, out: *mut BackendCleanup);
    fn r4os_zig_driver_cleanup_log(event: u8, owner: u32, counts: *const Counts);
    fn r4os_zig_net_mutation(op: u8) -> bool;
}
fn bound(owner: u32) -> bool {
    owner != 0
        && admission::r4os_rust_driver_api_owner_scalar(0) == owner
        && admission::r4os_rust_driver_api_owner_scalar(1) != 0
}
fn backend(op: u8, owner: u32) -> u64 {
    unsafe { r4os_zig_driver_cleanup_owner(op, owner) }
}
fn log(event: u8, owner: u32) {
    unsafe { r4os_zig_driver_cleanup_log(event, owner, ptr::null()) }
}
fn close_data(owner: u32) {
    unsafe {
        resource_state::r4os_rust_resource_state_mutate(
            resources::r4os_rust_driver_resource_state(),
            1,
            owner,
            0,
            0,
        )
    };
    heap::r4os_rust_driver_heap_close(owner);
    semaphores::r4os_rust_driver_semaphore_close(owner);
    backend(6, owner);
    backend(7, owner);
}
unsafe fn net_finish(token: *mut Token, quarantine: bool) {
    if !unsafe { (*token).net_mutation_active } {
        return;
    }
    unsafe {
        r4os_zig_net_mutation(if quarantine { 2 } else { 1 });
        (*token).net_mutation_active = false;
    }
}
/// # Safety
/// The output is an exclusive resident private C token. The exact existing
/// driver lifecycle guard owns its state through all prepared backend barriers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_cleanup_prepare(owner: u32, out: *mut Token) -> bool {
    if !bound(owner) {
        return false;
    }
    if backend(0, owner) != 0 || backend(1, owner) != 0 {
        return false;
    }
    let mut token = Token::empty(owner);
    if !unsafe { storage::r4os_rust_driver_storage_prepare(owner, &mut token.storage_plan) } {
        return false;
    }
    if net::r4os_rust_driver_net_has_owner(owner) {
        if !unsafe { r4os_zig_net_mutation(0) } {
            unsafe { storage::r4os_rust_driver_storage_cancel_plan(&mut token.storage_plan) };
            return false;
        }
        token.net_mutation_active = true;
    }
    if backend(4, owner) == 0 {
        unsafe {
            storage::r4os_rust_driver_storage_cancel_plan(&mut token.storage_plan);
            net_finish(&mut token, false)
        };
        return false;
    }
    token.display_blit_prepared = true;
    if !audio::r4os_rust_driver_audio_closing(owner, true) {
        audio::r4os_rust_driver_audio_closing(owner, false);
        backend(5, owner);
        unsafe {
            storage::r4os_rust_driver_storage_cancel_plan(&mut token.storage_plan);
            net_finish(&mut token, false)
        };
        return false;
    }
    token.audio_prepared = true;
    token.active = true;
    unsafe { ptr::write(out, token) };
    true
}
/// # Safety
/// The exact active resident token retains prepared barriers and lifecycle owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_cleanup_cancel(token: *mut Token) -> bool {
    if !unsafe { (*token).active }
        || unsafe { (*token).shutdown_started }
        || !bound(unsafe { (*token).owner })
    {
        return false;
    }
    unsafe {
        storage::r4os_rust_driver_storage_cancel_plan(ptr::addr_of_mut!((*token).storage_plan));
        net_finish(token, false)
    };
    let owner = unsafe { (*token).owner };
    if unsafe { (*token).display_blit_prepared } {
        backend(5, owner);
    }
    if unsafe { (*token).audio_prepared } {
        audio::r4os_rust_driver_audio_closing(owner, false);
    }
    unsafe { (*token).active = false };
    true
}
/// # Safety
/// The retained private token belongs to the currently admitted driver shutdown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_cleanup_begin(token: *mut Token) {
    let owner = unsafe { (*token).owner };
    close_data(owner);
    threads::r4os_rust_driver_thread_close(owner);
    unsafe { (*token).shutdown_started = true };
}
/// # Safety
/// The exact active shutdown token retains every unacknowledged backend resource.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_cleanup_quarantine(token: *mut Token) -> bool {
    if !unsafe { (*token).active }
        || !unsafe { (*token).shutdown_started }
        || !bound(unsafe { (*token).owner })
    {
        return false;
    }
    unsafe {
        storage::r4os_rust_driver_storage_cancel_plan(ptr::addr_of_mut!((*token).storage_plan));
        net_finish(token, true);
        (*token).active = false
    };
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_system_transition(owner: u32, op: u8) -> bool {
    if !bound(owner) {
        return false;
    }
    match op {
        0 => {
            if backend(4, owner) == 0 {
                return false;
            }
            close_data(owner);
            threads::r4os_rust_driver_thread_stop_existing(owner);
            true
        }
        1 => {
            threads::r4os_rust_driver_thread_close(owner);
            true
        }
        2 => {
            if backend(0, owner) != 0
                || backend(1, owner) != 0
                || backend(2, owner) != 0
                || backend(3, owner) != 0
                || !threads::r4os_rust_driver_thread_quiesced(owner)
            {
                return false;
            }
            let mut result = MaybeUninit::uninit();
            unsafe { work::r4os_rust_driver_work_cleanup(owner, result.as_mut_ptr()) };
            if !unsafe { result.assume_init() }.quiesced {
                return false;
            }
            backend(2, owner) == 0 && backend(3, owner) == 0
        }
        _ => unreachable!(),
    }
}
/// # Safety
/// The resident token retains exact prepared backend admissions under the
/// lifecycle owner. Only genuine shutdown/quiescence/unregister/free receipts
/// allow subsequent destructive phases; failure retains closed ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_cleanup_commit(token: *mut Token) -> bool {
    let owner = unsafe { (*token).owner };
    if !unsafe { (*token).active } || !bound(owner) {
        return false;
    }
    if !unsafe { (*token).shutdown_started } {
        unsafe {
            storage::r4os_rust_driver_storage_cancel_plan(ptr::addr_of_mut!((*token).storage_plan));
            net_finish(token, false)
        };
        if unsafe { (*token).display_blit_prepared } {
            backend(5, owner);
        }
        if unsafe { (*token).audio_prepared } {
            audio::r4os_rust_driver_audio_closing(owner, false);
        }
        unsafe { (*token).active = false };
        return false;
    }
    if !threads::r4os_rust_driver_thread_quiesced(owner) {
        unsafe { r4os_rust_driver_cleanup_quarantine(token) };
        log(1, owner);
        return false;
    }
    if backend(2, owner) != 0 || backend(3, owner) != 0 {
        unsafe { r4os_rust_driver_cleanup_quarantine(token) };
        return false;
    }
    let mut count = Counts::default();
    let mut result = MaybeUninit::uninit();
    unsafe {
        storage::r4os_rust_driver_storage_commit(
            ptr::addr_of_mut!((*token).storage_plan),
            result.as_mut_ptr(),
        )
    };
    let result = unsafe { result.assume_init() };
    if result.remaining != 0 {
        unsafe {
            net_finish(token, true);
            (*token).active = false
        };
        return false;
    }
    count.storage = result.removed;
    let mut result = MaybeUninit::uninit();
    unsafe { net::r4os_rust_driver_net_cleanup(owner, result.as_mut_ptr()) };
    let result = unsafe { result.assume_init() };
    if result.failed {
        unsafe {
            net_finish(token, true);
            (*token).active = false
        };
        log(2, owner);
        return false;
    }
    count.net = result.removed;
    unsafe { net_finish(token, false) };
    count.display_blit = backend(10, owner) as u32;
    let mut result = MaybeUninit::uninit();
    unsafe { audio::r4os_rust_driver_audio_cleanup(owner, result.as_mut_ptr()) };
    let result = unsafe { result.assume_init() };
    if result.failed {
        unsafe { (*token).active = false };
        log(3, owner);
        return false;
    }
    count.audio = result.removed;
    let mut result = MaybeUninit::uninit();
    unsafe { r4os_zig_driver_usb_cleanup(owner, result.as_mut_ptr()) };
    let result = unsafe { result.assume_init() };
    if result.failed {
        unsafe { (*token).active = false };
        log(4, owner);
        return false;
    }
    count.usb_host = result.removed;
    let mut result = MaybeUninit::uninit();
    unsafe { msi::r4os_rust_driver_msi_cleanup(owner, result.as_mut_ptr()) };
    let result = unsafe { result.assume_init() };
    if result.failed {
        unsafe { (*token).active = false };
        log(5, owner);
        return false;
    }
    count.msi = result.removed;
    count.irq = irq_router::r4os_rust_irq_router_cleanup(owner);
    let mut result = MaybeUninit::uninit();
    unsafe { work::r4os_rust_driver_work_cleanup(owner, result.as_mut_ptr()) };
    let result = unsafe { result.assume_init() };
    if !result.quiesced {
        unsafe { (*token).active = false };
        log(6, owner);
        return false;
    }
    count.work = result.removed;
    if backend(2, owner) != 0 || backend(3, owner) != 0 {
        unsafe { (*token).active = false };
        log(7, owner);
        return false;
    }
    let mut result = MaybeUninit::uninit();
    unsafe { threads::r4os_rust_driver_thread_cleanup(owner, result.as_mut_ptr()) };
    let result = unsafe { result.assume_init() };
    if !result.quiesced {
        unsafe { (*token).active = false };
        log(8, owner);
        return false;
    }
    count.threads = result.released;
    let mut result = MaybeUninit::uninit();
    unsafe { semaphores::r4os_rust_driver_semaphore_cleanup(owner, result.as_mut_ptr()) };
    let result = unsafe { result.assume_init() };
    if !result.quiesced {
        unsafe { (*token).active = false };
        log(9, owner);
        return false;
    }
    count.semaphores = result.released;
    count.dma = dma::r4os_rust_driver_dma_cleanup(owner);
    let mut result = MaybeUninit::uninit();
    unsafe { heap::r4os_rust_driver_heap_cleanup(owner, result.as_mut_ptr()) };
    let result = unsafe { result.assume_init() };
    if !result.quiesced {
        unsafe { (*token).active = false };
        log(10, owner);
        return false;
    }
    count.cpu_heap = result.released;
    count.cpu_bytes = result.bytes;
    backend(9, owner);
    unsafe {
        resource_state::r4os_rust_resource_state_mutate(
            resources::r4os_rust_driver_resource_state(),
            2,
            owner,
            0,
            0,
        )
    };
    backend(8, owner);
    unsafe { (*token).active = false };
    if count.irq == 0
        && count.work == 0
        && count.threads == 0
        && count.semaphores == 0
        && count.dma == 0
        && count.cpu_heap == 0
        && count.msi == 0
        && count.audio == 0
        && count.storage == 0
        && count.usb_host == 0
        && count.display_blit == 0
        && count.net == 0
    {
        return true;
    }
    unsafe { r4os_zig_driver_cleanup_log(0, owner, &count) };
    true
}
struct Owner;
impl Drop for Owner {
    fn drop(&mut self) {
        admission::r4os_rust_driver_api_leave();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_cleanup(owner: u32) -> bool {
    if owner == 0 {
        return true;
    }
    if !admission::r4os_rust_driver_api_enter(owner, u64::MAX) {
        return false;
    }
    let _owner = Owner;
    let mut token = MaybeUninit::uninit();
    if !unsafe { r4os_rust_driver_cleanup_prepare(owner, token.as_mut_ptr()) } {
        log(11, owner);
        return false;
    }
    // Preserve the existing helper: absence of top-level Shutdown cancels the
    // prepared plan and never creates a destructive cleanup permission.
    unsafe { r4os_rust_driver_cleanup_commit(token.as_mut_ptr()) }
}
const _: () = {
    assert!(size_of::<Token>() == 280 && offset_of!(Token, storage_plan) == 8);
    assert!(size_of::<Counts>() == 72);
};
