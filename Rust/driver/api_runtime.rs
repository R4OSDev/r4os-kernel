//! Original optional heap/thread/semaphore API prefixes and per-call owner
//! admission. Bound tables remain usable for release after queries close.
use crate::{
    contract as a,
    driver::{api_admission as admission, heap, irq_router, semaphores, threads},
};
use core::{
    mem::{offset_of, size_of},
    ptr,
};
fn runtime() -> u32 {
    admission::r4os_rust_driver_api_runtime_owner(true)
}
fn heap_owner() -> u32 {
    if irq_router::r4os_rust_irq_router_local(false) != 0
        || admission::r4os_rust_driver_api_storage_owner() != 0
    {
        return 0;
    }
    let thread = threads::r4os_rust_driver_thread_current(0, 0) as u32;
    if thread != 0 {
        thread
    } else {
        admission::r4os_rust_driver_api_runtime_owner(false)
    }
}
unsafe fn valid<T>(output: *const T) -> bool {
    let h = output.cast::<u32>();
    unsafe { *h == 1 && *h.add(1) >= size_of::<T>() as u32 }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_heap_owner() -> u32 {
    heap_owner()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_semaphore_context() -> u32 {
    if runtime() == 0 {
        return 0;
    }
    if irq_router::r4os_rust_irq_router_local(false) != 0 {
        return a::driver_semaphore_context_irq;
    }
    if admission::r4os_rust_driver_api_storage_owner() == 0
        && semaphores::r4os_rust_driver_semaphore_can_sleep()
    {
        a::driver_semaphore_context_sleepable
    } else {
        0
    }
}
/// # Safety
/// The caller provides an aligned resident versioned C prefix of its declared
/// size and retains the admitted driver identity through the call and any wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_heap_api(output: *mut a::DriverHeapApi) -> i32 {
    if !unsafe { valid(output) } {
        return a::driver_heap_error_invalid;
    }
    unsafe { ptr::write(output, a::DriverHeapApi::default()) };
    let id = heap_owner();
    if id == 0 {
        return a::driver_heap_error_owner;
    }
    let result = heap::r4os_rust_driver_heap_query(id);
    if result != 0 {
        return result;
    }
    unsafe {
        ptr::write(
            output,
            a::DriverHeapApi {
                allocate: heap_allocate as *const () as usize as u64,
                release: heap_release as *const () as usize as u64,
                stats: heap_stats as *const () as usize as u64,
                ..a::DriverHeapApi::default()
            },
        )
    };
    a::driver_heap_ok
}
unsafe extern "C" fn heap_allocate(
    bytes: u64,
    alignment: u32,
    out: *mut a::DriverHeapAllocation,
) -> i32 {
    unsafe { heap::r4os_rust_driver_heap_allocate(heap_owner(), bytes, alignment, out) }
}
extern "C" fn heap_release(handle: u64) -> i32 {
    heap::r4os_rust_driver_heap_release(heap_owner(), handle)
}
unsafe extern "C" fn heap_stats(out: *mut a::DriverHeapStats) -> i32 {
    unsafe { heap::r4os_rust_driver_heap_stats(heap_owner(), out) }
}
/// # Safety
/// The caller provides an aligned resident versioned C prefix of its declared
/// size and retains the admitted driver identity through the call and any wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_semaphore_api(output: *mut a::DriverSemaphoreApi) -> i32 {
    if !unsafe { valid(output) } {
        return a::driver_semaphore_error_invalid;
    }
    unsafe { ptr::write(output, a::DriverSemaphoreApi::default()) };
    let result = semaphores::r4os_rust_driver_semaphore_query(runtime());
    if result != 0 {
        return result;
    }
    unsafe {
        ptr::write(
            output,
            a::DriverSemaphoreApi {
                create: semaphore_create as *const () as usize as u64,
                acquire: semaphore_acquire as *const () as usize as u64,
                release: semaphore_release as *const () as usize as u64,
                destroy: semaphore_destroy as *const () as usize as u64,
                status: semaphore_status as *const () as usize as u64,
                stats: semaphore_stats as *const () as usize as u64,
                context_flags: r4os_rust_driver_semaphore_context as *const () as usize as u64,
                ..a::DriverSemaphoreApi::default()
            },
        )
    };
    a::driver_semaphore_ok
}
unsafe extern "C" fn semaphore_create(initial: u32, maximum: u32, out: *mut u64) -> i32 {
    unsafe { *out = 0 };
    if r4os_rust_driver_semaphore_context() & a::driver_semaphore_context_sleepable == 0 {
        return a::driver_semaphore_error_context;
    }
    unsafe { semaphores::r4os_rust_driver_semaphore_create(runtime(), initial, maximum, out) }
}
extern "C" fn semaphore_acquire(handle: u64, ticks: u64) -> i32 {
    if ticks != 0
        && r4os_rust_driver_semaphore_context() & a::driver_semaphore_context_sleepable == 0
    {
        return a::driver_semaphore_error_context;
    }
    semaphores::r4os_rust_driver_semaphore_acquire(runtime(), handle, ticks)
}
extern "C" fn semaphore_release(handle: u64) -> i32 {
    semaphores::r4os_rust_driver_semaphore_release(runtime(), handle)
}
extern "C" fn semaphore_destroy(handle: u64) -> i32 {
    if r4os_rust_driver_semaphore_context() & a::driver_semaphore_context_sleepable == 0 {
        return a::driver_semaphore_error_context;
    }
    semaphores::r4os_rust_driver_semaphore_destroy(runtime(), handle)
}
unsafe extern "C" fn semaphore_status(handle: u64, out: *mut a::DriverSemaphoreStatus) -> i32 {
    unsafe { semaphores::r4os_rust_driver_semaphore_status(runtime(), handle, out) }
}
unsafe extern "C" fn semaphore_stats(out: *mut a::DriverSemaphoreStats) -> i32 {
    unsafe { semaphores::r4os_rust_driver_semaphore_stats(runtime(), out) }
}
/// # Safety
/// The caller provides an aligned resident versioned C prefix of its declared
/// size and retains the admitted driver identity through the call and any wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_thread_api(output: *mut a::DriverThreadApi) -> i32 {
    let head = output.cast::<u32>();
    if unsafe { *head != 1 || *head.add(1) < a::driver_thread_api_min_bytes } {
        return a::driver_thread_error_invalid;
    }
    let cap = unsafe { *head.add(1) } as usize;
    let bytes = if cap >= size_of::<a::DriverThreadApi>() {
        size_of::<a::DriverThreadApi>()
    } else if cap >= offset_of!(a::DriverThreadApi, current_request) {
        offset_of!(a::DriverThreadApi, current_request)
    } else {
        a::driver_thread_api_min_bytes as usize
    };
    let mut service = a::DriverThreadApi {
        size: bytes as u32,
        ..a::DriverThreadApi::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(service).cast::<u8>(),
            output.cast::<u8>(),
            bytes,
        )
    };
    let id = heap_owner();
    if id == 0 {
        return a::driver_thread_error_owner;
    }
    let result = threads::r4os_rust_driver_thread_query(id);
    if result != 0 {
        return result;
    }
    service = a::DriverThreadApi {
        size: bytes as u32,
        start: thread_start as *const () as usize as u64,
        stop: thread_stop as *const () as usize as u64,
        join: thread_join as *const () as usize as u64,
        release: thread_release as *const () as usize as u64,
        status: thread_status as *const () as usize as u64,
        current: thread_current as *const () as usize as u64,
        sleep_ticks: thread_sleep as *const () as usize as u64,
        stats: thread_stats as *const () as usize as u64,
        abort_current: thread_abort as *const () as usize as u64,
        current_request: thread_request as *const () as usize as u64,
        ..a::DriverThreadApi::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::addr_of!(service).cast::<u8>(),
            output.cast::<u8>(),
            bytes,
        )
    };
    a::driver_thread_ok
}
unsafe extern "C" fn thread_request(out: *mut a::DriverThreadRequest) -> i32 {
    unsafe { threads::r4os_rust_driver_thread_request(heap_owner(), out) }
}
extern "C" fn thread_abort(result: i32) -> i32 {
    let id = heap_owner();
    if id == 0 {
        return a::driver_thread_error_owner;
    }
    threads::r4os_rust_driver_thread_abort(id, result)
}
unsafe extern "C" fn thread_start(input: *const a::DriverThreadRequest, out: *mut u64) -> i32 {
    let id = heap_owner();
    if heap::r4os_rust_driver_heap_query(id) == a::driver_heap_error_closed
        && (id != admission::r4os_rust_driver_api_owner_scalar(0)
            || admission::r4os_rust_driver_api_owner_scalar(1) == 0)
    {
        unsafe { *out = 0 };
        return a::driver_thread_error_closed;
    }
    unsafe { threads::r4os_rust_driver_thread_start(id, input, out) }
}
extern "C" fn thread_stop(handle: u64) -> i32 {
    threads::r4os_rust_driver_thread_stop(heap_owner(), handle)
}
unsafe extern "C" fn thread_join(handle: u64, ticks: u64, out: *mut i32) -> i32 {
    unsafe { threads::r4os_rust_driver_thread_join(heap_owner(), handle, ticks, out) }
}
extern "C" fn thread_release(handle: u64) -> i32 {
    threads::r4os_rust_driver_thread_release(heap_owner(), handle)
}
unsafe extern "C" fn thread_status(handle: u64, out: *mut a::DriverThreadStatus) -> i32 {
    unsafe { threads::r4os_rust_driver_thread_status(heap_owner(), handle, out) }
}
extern "C" fn thread_current() -> u64 {
    threads::r4os_rust_driver_thread_current(2, heap_owner())
}
extern "C" fn thread_sleep(ticks: u64) -> i32 {
    threads::r4os_rust_driver_thread_sleep(heap_owner(), ticks)
}
unsafe extern "C" fn thread_stats(out: *mut a::DriverThreadStats) -> i32 {
    unsafe { threads::r4os_rust_driver_thread_stats(heap_owner(), out) }
}
