//! Sole network/IPC boot coordinator. Peers perform work at their existing
//! owners; every status publication is a small copy outside peer calls.
use super::{config, tcp};
use crate::{
    arch::runtime::RuntimeGuard,
    boot::{fatal, foundation, log},
    loader::boot as loader,
    sched::sync_types::Text,
};
use core::{cell::UnsafeCell, ffi::c_void, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub phase: u8,
    pub contract_ready: bool,
    pub core_ready: bool,
    pub boot_config_applied: bool,
    pub ipc_ready: bool,
    pub net_ipc_services_ready: bool,
    pub adapter_count: usize,
    pub config_invalid_options: u64,
    pub config_source: Text,
    pub failed: bool,
    pub reason: Text,
}
impl Status {
    pub const EMPTY: Self = Self {
        phase: 0,
        contract_ready: false,
        core_ready: false,
        boot_config_applied: false,
        ipc_ready: false,
        net_ipc_services_ready: false,
        adapter_count: 0,
        config_invalid_options: 0,
        config_source: Text::literal(b"none"),
        failed: false,
        reason: Text::literal(b"not initialized"),
    };
}
struct Storage(UnsafeCell<Status>);
// SAFETY: original linear BSP bootstrap is the only writer; short runtime
// admission also protects concurrent passive copies. No peer borrow escapes.
unsafe impl Sync for Storage {}
static CURRENT: Storage = Storage(UnsafeCell::new(Status::EMPTY));
fn status() -> Status {
    let _guard = RuntimeGuard::acquire();
    unsafe { CURRENT.0.get().read() }
}
fn update(f: impl FnOnce(&mut Status)) {
    let _guard = RuntimeGuard::acquire();
    f(unsafe { &mut *CURRENT.0.get() });
}
fn bytes(value: Text) -> &'static [u8] {
    if value.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value.pointer, value.len) }
    }
}
fn yes_no(value: bool) -> &'static [u8] {
    if value { b"yes" } else { b"no" }
}
fn phase_name(phase: u8) -> &'static [u8] {
    match phase {
        0 => b"not-initialized",
        1 => b"contract-ready",
        2 => b"core-ready",
        3 => b"boot-config-applied",
        4 => b"retired",
        5 => b"ipc-ready",
        6 => b"net-ipc-services-ready",
        7 => b"failed",
        _ => panic!("Network boot phase"),
    }
}
fn initialized(value: Status) -> bool {
    value.net_ipc_services_ready && !value.failed
}
fn bootlog(value: &[u8]) {
    unsafe { log::r4os_rust_bootlog_write(value.as_ptr(), value.len()) };
}
fn bootdec(value: usize) {
    log::r4os_rust_bootlog_dec(value as u64);
}
fn line(value: &[u8]) {
    crate::boot::log_route::puts(value);
}
fn mark_contract(reason: &'static [u8]) {
    update(|s| {
        *s = Status {
            phase: 1,
            contract_ready: true,
            reason: Text::literal(reason),
            ..Status::EMPTY
        }
    });
}
fn fail(message: &'static [u8], reason: &'static [u8]) -> bool {
    update(|s| {
        s.phase = 7;
        s.failed = true;
        s.reason = Text::literal(reason);
    });
    bootlog(b"[NETBOOT][CRASH] reason=");
    bootlog(reason);
    bootlog(b"\r\n");
    unsafe { fatal::r4os_rust_fatal_record(15, message.as_ptr(), message.len()) };
    false
}
fn prepare_contract() -> bool {
    let s = status();
    if s.failed {
        return false;
    }
    if s.contract_ready {
        return true;
    }
    mark_contract(b"contract ready; waiting for productive init");
    bootlog(b"[NETBOOT] contract [OK]\r\n");
    line(b"  Network boot [contract]\r\n");
    true
}
fn ready_reason(s: Status) -> &'static [u8] {
    if s.config_invalid_options != 0 {
        b"network/ipc ready with invalid net config options"
    } else if s.adapter_count == 0 {
        b"network/ipc ready without adapter"
    } else {
        b"network/ipc ready"
    }
}
unsafe fn init() -> bool {
    let s = status();
    if s.failed {
        return false;
    }
    if initialized(s) {
        return true;
    }
    if !loader::r4os_rust_loader_boot_ready() {
        return fail(
            b"Network boot before loader config",
            b"loader config missing",
        );
    }
    let config = loader::r4os_rust_loader_boot_config();
    if config.is_null() {
        return fail(b"Network boot config missing", b"loader config missing");
    }
    unsafe { init_with_config(config) }
}
unsafe fn init_with_config(loaded: *const c_void) -> bool {
    let s = status();
    if s.failed {
        return false;
    }
    if initialized(s) {
        return true;
    }
    if !s.contract_ready {
        mark_contract(b"contract ready");
    }
    unsafe { super::bindings::init() };
    let count = super::adapters::r4os_rust_net_adapter_count();
    update(|s| {
        s.core_ready = true;
        s.adapter_count = count;
        s.phase = 2;
    });
    bootlog(b"[NETBOOT] core [OK] adapters=");
    bootdec(count);
    bootlog(b"\r\n");
    unsafe {
        config::apply_boot_config(loaded);
        super::bindings::configure_injection(loaded)
    };
    let config = config::settings();
    let source = config::source_name();
    update(|s| {
        s.boot_config_applied = true;
        s.config_invalid_options = config.invalid_options;
        s.config_source = source;
        s.phase = 3;
    });
    bootlog(b"[NETBOOT] boot-config [OK] source=");
    bootlog(bytes(source));
    bootlog(b" invalid=");
    log::r4os_rust_bootlog_dec(config.invalid_options);
    bootlog(b"\r\n");
    unsafe { crate::service::ipc::r4os_rust_ipc_control(0, 0) };
    update(|s| {
        s.ipc_ready = true;
        s.phase = 5;
    });
    bootlog(b"[NETBOOT] ipc [OK]\r\n");
    super::ipc_services::r4os_rust_net_ipc_init();
    update(|s| {
        s.net_ipc_services_ready = true;
        s.phase = 6;
        s.reason = Text::literal(ready_reason(*s));
    });
    bootlog(b"[NETBOOT] net-ipc-services [OK]\r\n");
    dump_status_bootlog();
    emit_boot_status_line();
    line(b"  Network boot [OK]\r\n");
    true
}
fn dump_status_bootlog() {
    let s = status();
    bootlog(b"[NETBOOT] status phase=");
    bootlog(phase_name(s.phase));
    for (label, value) in [
        (b" contract=".as_slice(), s.contract_ready),
        (b" core=", s.core_ready),
        (b" config=", s.boot_config_applied),
        (b" ipc=", s.ipc_ready),
        (b" services=", s.net_ipc_services_ready),
    ] {
        bootlog(label);
        bootlog(yes_no(value));
    }
    bootlog(b" adapters=");
    bootdec(s.adapter_count);
    bootlog(b" config_source=");
    bootlog(bytes(s.config_source));
    bootlog(b" invalid=");
    log::r4os_rust_bootlog_dec(s.config_invalid_options);
    bootlog(b" reason=");
    bootlog(bytes(s.reason));
    bootlog(b"\r\n");
}
fn dump_status_log() {
    let s = status();
    foundation::puts(b"Network boot\r\n  phase=");
    foundation::puts(phase_name(s.phase));
    foundation::puts(b" initialized=");
    foundation::puts(yes_no(initialized(s)));
    foundation::puts(b" failed=");
    foundation::puts(yes_no(s.failed));
    foundation::puts(b"\r\n  core=");
    foundation::puts(yes_no(s.core_ready));
    for (label, value) in [
        (b" config=".as_slice(), s.boot_config_applied),
        (b" ipc=", s.ipc_ready),
        (b" services=", s.net_ipc_services_ready),
    ] {
        foundation::puts(label);
        foundation::puts(yes_no(value));
    }
    foundation::puts(b"\r\n  adapters=");
    foundation::dec(s.adapter_count as u64);
    foundation::puts(b" config_source=");
    foundation::puts(bytes(s.config_source));
    foundation::puts(b" invalid=");
    foundation::dec(s.config_invalid_options);
    foundation::puts(b" reason=");
    foundation::puts(bytes(s.reason));
    foundation::puts(b"\r\n");
}
fn emit_boot_status_line() {
    let s = status();
    foundation::puts(b"  NETBOOT cfg=");
    foundation::puts(bytes(s.config_source));
    foundation::puts(b" adapters=");
    foundation::dec(s.adapter_count as u64);
    foundation::puts(b" invalid=");
    foundation::dec(s.config_invalid_options);
    foundation::puts(b" [OK]\r\n");
    foundation::puts(b"  NETBOOT tcp-table conns=");
    foundation::dec(tcp::MAX_CONNECTIONS as u64);
    foundation::puts(b" slot_kb=");
    foundation::dec((core::mem::size_of::<tcp::Connection>() / 1024) as u64);
    foundation::puts(b" total_kb=");
    foundation::dec((tcp::MAX_CONNECTIONS * core::mem::size_of::<tcp::Connection>() / 1024) as u64);
    foundation::puts(b"\r\n");
}
/// # Safety
/// Exact stable boot peer table; callbacks retain each original owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_network_boot_prepare() -> bool {
    prepare_contract()
}
/// # Safety
/// Linear BSP bootstrap and exact stable peer table; resident boot config.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_network_boot_init() -> bool {
    unsafe { init() }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_network_boot_ready() -> bool {
    initialized(status())
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_network_boot_contract() -> bool {
    let s = status();
    s.contract_ready && !s.failed
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_network_boot_dump() {
    dump_status_log();
}
/// # Safety
/// Exact exclusive paired status output; no mutable resident reference escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_network_boot_status(out: *mut Status) {
    unsafe { out.write(status()) };
}
/// # Safety
/// Exact output and valid private phase discriminant.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_network_boot_phase(phase: u8, out: *mut Text) {
    unsafe { out.write(Text::literal(phase_name(phase))) };
}

const _: () = {
    assert!(core::mem::size_of::<Status>() == 64);
    assert!(core::mem::align_of::<Status>() == 8);
    assert!(core::mem::offset_of!(Status, phase) == 0);
    assert!(core::mem::offset_of!(Status, contract_ready) == 1);
    assert!(core::mem::offset_of!(Status, core_ready) == 2);
    assert!(core::mem::offset_of!(Status, boot_config_applied) == 3);
    assert!(core::mem::offset_of!(Status, ipc_ready) == 4);
    assert!(core::mem::offset_of!(Status, net_ipc_services_ready) == 5);
    assert!(core::mem::offset_of!(Status, adapter_count) == 8);
    assert!(core::mem::offset_of!(Status, config_invalid_options) == 16);
    assert!(core::mem::offset_of!(Status, config_source) == 24);
    assert!(core::mem::offset_of!(Status, failed) == 40);
    assert!(core::mem::offset_of!(Status, reason) == 48);
};
