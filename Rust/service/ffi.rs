//! Private fixed-layout transports between native service consumers. Every
//! mutable registry/endpoint owner and algorithm is native; these store none.
use super::{
    endpoint,
    registry::{self as reg, Identity},
    registry_api::{self as api, Change, Key},
    registry_metrics,
    registry_types::*,
};
use crate::{contract as c, fs::vfs_types::Bytes, sched::task_context};

/// # Safety
/// Singleton boot/serialized owner fixtures, before any live service task.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_init() {
    unsafe { reg::reset() };
}
/// # Safety
/// Readable caller-owned byte spans remain valid for this bounded call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_register(
    name: Bytes,
    path: Bytes,
    args: Bytes,
    mode: u8,
    description: Bytes,
) -> i32 {
    unsafe { api::register(name, path, args, mode, description) }
}
/// # Safety
/// Original name/text lifetimes include lifecycle contention waits. No borrowed
/// registry record or held registry owner crosses the endpoint wait boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_name(
    operation: u8,
    name: Bytes,
    state: u8,
    id: u32,
    code: i32,
    ticks: u64,
    text: Bytes,
) -> i32 {
    unsafe {
        let key = Key::Name(name);
        match operation {
            0 => api::change(key, Change::Unregister),
            1 => api::change(key, Change::State(state, id, code, text)),
            2 => {
                let mut reservation = StartReservation::EMPTY;
                if api::change(key, Change::Begin(&mut reservation)) == c::service_api_result_ok {
                    0
                } else {
                    -1
                }
            }
            3 => api::running(name, id, ticks),
            4 => api::change(key, Change::Stopping),
            5 => api::change(key, Change::Finish(false, code, text)),
            6 => api::change(key, Change::Finish(true, code, text)),
            7 => api::restart_count(name),
            8 => api::change(key, Change::Mode(state)),
            9 => {
                api::cancel_start(name, ticks);
                0
            }
            _ => panic!("Invalid private service name operation"),
        }
    }
}
/// # Safety
/// Target is an immutable caller snapshot. Text retains its original duration
/// through possible endpoint lifecycle contention and revalidation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_target(
    operation: u8,
    target: *const ApiIndexTarget,
    code: i32,
    text: Bytes,
) -> i32 {
    unsafe {
        let key = Key::Target(target.read());
        match operation {
            0 => api::change(key, Change::Stopping),
            1 => api::change(key, Change::Finish(false, code, text)),
            2 => api::change(key, Change::Finish(true, code, text)),
            3 => i32::from(api::note_instance(&*target)),
            _ => panic!("Invalid private service target operation"),
        }
    }
}
/// # Safety
/// Reservation/output and text retain their original caller lifetime. Exact
/// slot/start-generation validation precedes completion or failure mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_start(
    operation: u8,
    name: Bytes,
    reservation: *mut StartReservation,
    id: u32,
    ticks: u64,
    code: i32,
    text: Bytes,
) -> i32 {
    unsafe {
        match operation {
            0 => api::change(Key::Name(name), Change::Begin(reservation)),
            1 => i32::from(api::complete_start(reservation, id, ticks)),
            2 => {
                api::fail_start(reservation, code, text);
                0
            }
            _ => panic!("Invalid private service start operation"),
        }
    }
}
/// # Safety
/// Selected output is exclusive resident C storage. Optional input target/name
/// is valid for its selected operation and is never retained after return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_get(
    operation: u8,
    index: usize,
    name: Bytes,
    target: *const ApiIndexTarget,
    out: *mut Entry,
    out_target: *mut ApiIndexTarget,
) -> bool {
    unsafe {
        if operation >= 3 {
            assert!(operation <= 4);
            if let Some(value) = api::target(index as u32, operation == 3) {
                out_target.write(value);
                true
            } else {
                false
            }
        } else {
            let value = match operation {
                0 => api::entry_at(index),
                1 => api::entry_by_name(name),
                2 => api::entry_for_target(&*target),
                _ => unreachable!(),
            };
            if let Some(value) = value {
                out.write(value);
                true
            } else {
                false
            }
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_services_count() -> usize {
    api::count()
}
/// # Safety
/// Selected target/name are readable and output is exclusive resident snapshot
/// storage. No output reference is retained through endpoint lock acquisition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_info(
    operation: u8,
    index: u32,
    name: Bytes,
    target: *const ApiIndexTarget,
    out: *mut c::ServiceInfo,
    now: u64,
) -> i32 {
    unsafe { api::info(operation, index, name, target, out, now) }
}
/// # Safety
/// Same bounded snapshot contract as service_info, including the original
/// public detail/path bounds and exact registry/endpoint generation checks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_detail(
    operation: u8,
    index: u32,
    name: Bytes,
    target: *const ApiIndexTarget,
    out: *mut c::ServiceDetail,
    now: u64,
) -> i32 {
    unsafe { api::detail(operation, index, name, target, out, now) }
}
/// # Safety
/// Name/output retain their original call lifetime and output is resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_endpoint_register(
    name: Bytes,
    id: u32,
    flags: u32,
    out: *mut c::ServiceInfo,
    now: u64,
) -> i32 {
    unsafe { api::register_endpoint(name, id, flags, out, now) }
}
/// # Safety
/// Handle/request identity follows the original owner contract; waits keep no
/// record borrow, and exact endpoint generation is checked after resumption.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_endpoint_control(
    operation: u8,
    handle: u32,
    id: u32,
    timeout: u64,
) -> i32 {
    unsafe {
        match operation {
            0 => api::close(handle),
            1 => api::unregister_endpoint(handle),
            2 => endpoint::poll(handle),
            3 => endpoint::wait_endpoint(handle, timeout),
            4 => endpoint::wait_response(handle, id, timeout),
            5 => endpoint::cancel(handle, id),
            _ => panic!("Invalid private service endpoint operation"),
        }
    }
}
/// # Safety
/// Caller keeps payload alive through admission, and optional unwind output
/// receives the exact task-owned publication pin that its caller must release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_request(
    handle: u32,
    client: u32,
    op: u16,
    data: Bytes,
    timeout: u64,
    wait: bool,
    unwind: *mut task_context::Unwind,
) -> i32 {
    unsafe {
        if wait {
            endpoint::submit_wait(handle, client, op, data, timeout, unwind)
        } else {
            endpoint::submit(handle, client, op, data)
        }
    }
}
/// # Safety
/// Header/buffer are caller-owned valid writable outputs. The original small
/// buffer semantics and metadata-first publication/consumption are retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_recv(
    handle: u32,
    id: u32,
    request: bool,
    header: *mut c::ServiceMessageHeader,
    out: *mut u8,
    capacity: usize,
) -> i32 {
    unsafe {
        if request {
            endpoint::recv(handle, header, out, capacity)
        } else {
            endpoint::take_response(handle, id, header, out, capacity)
        }
    }
}
/// # Safety
/// Caller lends readable payload for the exact delivered request through this
/// bounded endpoint operation; no payload pointer is retained afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_reply(
    handle: u32,
    id: u32,
    status: i32,
    data: Bytes,
) -> i32 {
    unsafe { endpoint::reply(handle, id, status, data) }
}
/// # Safety
/// Output is exclusive resident snapshot storage, disjoint from owners.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_performance(out: *mut PerformanceSummary) {
    unsafe { out.write(registry_metrics::snapshot()) };
}
/// # Safety
/// Selected records are exclusive caller-owned metadata fixtures, not live
/// global slots. This reuses the exact productive algorithms, no second model.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_record(
    operation: u8,
    ep: *mut Endpoint,
    slot: *mut RequestSlot,
    counter: *mut u32,
    out: *mut QueueCounts,
    identity: *const Identity,
) -> u32 {
    unsafe {
        match operation {
            0 => {
                endpoint::reset_slot(slot);
                0
            }
            1 | 2 => {
                endpoint::reset_metadata(ep, operation == 2);
                0
            }
            3 => endpoint::allocate_request(ep, counter),
            4 => {
                out.write(endpoint::counts(ep));
                0
            }
            5 => {
                endpoint::advance(ep);
                0
            }
            6 => u32::from(reg::identity_matches(ep, identity.read())),
            _ => panic!("Invalid private service metadata operation"),
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_services_label(mode: bool, value: u8) -> Bytes {
    let text: &[u8] = if mode {
        match value {
            0 => b"manual",
            1 => b"auto",
            2 => b"disabled",
            _ => panic!("Invalid private service mode"),
        }
    } else {
        match value {
            0 => b"empty",
            1 => b"stopped",
            2 => b"starting",
            3 => b"running",
            4 => b"stopping",
            5 => b"failed",
            6 => b"disabled",
            _ => panic!("Invalid private service state"),
        }
    };
    Bytes {
        data: text.as_ptr(),
        len: text.len(),
    }
}
/// # Safety
/// Readable input bytes remain caller-owned for this pure decision.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_services_parse(value: Bytes) -> u8 {
    unsafe {
        let value = reg::bytes(value);
        if value.eq_ignore_ascii_case(b"manual") {
            0
        } else if value.eq_ignore_ascii_case(b"auto") {
            1
        } else if value.eq_ignore_ascii_case(b"disabled") || value.eq_ignore_ascii_case(b"disable")
        {
            2
        } else {
            255
        }
    }
}
