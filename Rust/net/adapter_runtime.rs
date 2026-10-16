//! Original runtime adapter link/status and terminal registry coordination.
//! A counted backend callback also pins its exact task against hard-kill;
//! registry snapshots and short metadata admission end before R4D execution.
use super::{
    adapters,
    backend_admission::CallbackGuard,
    cleanup::{self, DiagnosticCleanupScope},
    config, dhcp_client, packet_pool, protocol_stats,
};
use crate::sched::sync_types::Text;
type BackendStatus = adapters::BackendStatus;
fn count() -> usize {
    adapters::r4os_rust_net_adapter_count()
}
fn link(index: usize) -> u8 {
    adapters::snapshot(index).map_or(0, |a| a.link)
}
fn lifecycle(link: u8) -> u8 {
    match link {
        2 => 2,
        1 => 3,
        _ => 1,
    }
}
unsafe fn clean(reason: Text, scope: Option<DiagnosticCleanupScope>) {
    if let Some(scope) = scope {
        unsafe {
            cleanup::scoped(reason, scope);
        }
    } else {
        unsafe {
            cleanup::all(reason);
        }
    }
}
pub(super) fn refresh(
    index: usize,
    scope: Option<DiagnosticCleanupScope>,
) -> Option<BackendStatus> {
    let _callback = CallbackGuard::acquire()?;
    let adapter = adapters::snapshot(index)?;
    if let Some(callback) = adapter.ops.status {
        // All canonical fields are integers. Preserve the original zero
        // defaults and the only nonzero default: no routed IRQ line (0xff).
        let mut status = unsafe { core::mem::zeroed::<BackendStatus>() };
        status.irq_line = 255;
        if unsafe { callback(adapter.ops.status_context, index, &mut status) } == 0 {
            let new_link = if status.link_up != 0 { 2 } else { 1 };
            let old_link = link(index);
            if new_link != old_link {
                adapters::r4os_rust_net_adapter_link(index, new_link);
                if new_link == 1 && old_link == 2 {
                    if index == 0 && config::settings().dhcp_enabled {
                        dhcp_client::r4os_rust_dhcp_client_invalidate_lease();
                        unsafe {
                            config::clear_preserving_mode(Text::literal(b"link-down"));
                        }
                        protocol_stats::r4os_rust_net_arp_cache_flush_owned();
                    }
                    unsafe {
                        clean(Text::literal(b"link-down"), scope);
                    }
                }
            } else if adapters::snapshot(index)
                .is_some_and(|a| a.lifecycle == 1 || a.lifecycle == 4)
            {
                adapters::r4os_rust_net_adapter_lifecycle(index, lifecycle(new_link));
            }
            return Some(status);
        }
    }
    adapters::r4os_rust_net_adapter_lifecycle(index, lifecycle(link(index)));
    None
}
// SAFETY: caller owns backend-mutation admission or the exact loan-free
// diagnostic adapter until both packet remapping and registry removal finish.
unsafe fn remove(index: usize) -> bool {
    if index >= count() {
        return false;
    }
    unsafe {
        packet_pool::r4os_rust_net_packet_remove_adapter(index);
    }
    unsafe { adapters::r4os_rust_net_adapter_remove(index) }
}
unsafe fn unregister(index: usize, scope: Option<DiagnosticCleanupScope>) -> bool {
    if index >= count() {
        return false;
    }
    adapters::r4os_rust_net_adapter_lifecycle(index, 5);
    unsafe {
        clean(Text::literal(b"adapter-unregister"), scope);
    }
    unsafe { remove(index) }
}
unsafe fn reset(index: usize, reason: Text, scope: Option<DiagnosticCleanupScope>) -> bool {
    if index >= count() {
        return false;
    }
    adapters::r4os_rust_net_adapter_lifecycle(index, 4);
    unsafe {
        adapters::r4os_rust_net_adapter_reset_note(index, reason);
        clean(Text::literal(b"adapter-reset"), scope);
    }
    if index >= count() {
        return false;
    }
    // Keep the original productive refresh here, even after scoped cleanup.
    if refresh(index, None).is_none() {
        adapters::r4os_rust_net_adapter_lifecycle(index, lifecycle(link(index)));
    }
    true
}
unsafe fn scope(input: *const DiagnosticCleanupScope) -> Option<DiagnosticCleanupScope> {
    if input.is_null() {
        None
    } else {
        Some(unsafe { *input })
    }
}
/// # Safety
/// Optional resident scope, exact writable canonical output through the whole
/// callback/cleanup. The owned result is copied after callback admission ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_runtime_refresh(
    index: usize,
    input: *const DiagnosticCleanupScope,
    out: *mut BackendStatus,
) -> bool {
    let scope = unsafe { scope(input) };
    if let Some(status) = refresh(index, scope) {
        unsafe {
            out.write(status);
        }
        true
    } else {
        false
    }
}
/// # Safety
/// Existing backend-mutation admission for a real driver, or the exact local
/// diagnostic adapter. Optional scope contains only that diagnostic's handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_runtime_unregister(
    index: usize,
    input: *const DiagnosticCleanupScope,
) -> bool {
    unsafe { unregister(index, scope(input)) }
}
/// # Safety
/// Exact local diagnostic adapter, with no outstanding callback/claim loans.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_runtime_discard(index: usize) -> bool {
    if index >= count() {
        return false;
    }
    adapters::r4os_rust_net_adapter_lifecycle(index, 5);
    unsafe { remove(index) }
}
/// # Safety
/// Existing mutation/diagnostic admission retains the exact enumeration entry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_runtime_remove(index: usize) -> bool {
    unsafe { remove(index) }
}
/// # Safety
/// Existing mutation/diagnostic admission, optional resident exact scope and
/// stable original kernel-string reason through cleanup and status refresh.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_adapter_runtime_reset(
    index: usize,
    reason: Text,
    input: *const DiagnosticCleanupScope,
) -> bool {
    unsafe { reset(index, reason, scope(input)) }
}
