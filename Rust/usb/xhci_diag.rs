//! Stateless access to the existing console/incident owners. Formatting and
//! incident ownership stay in the native controller; no log buffer is added.
use super::xhci_types::IncidentToken;
use crate::boot::foundation;
unsafe extern "C" {
    fn r4os_zig_usb_xhci_incident_begin() -> u64;
    fn r4os_zig_usb_xhci_incident_resolve(generation: u64) -> bool;
    fn r4os_zig_usb_xhci_diag_write(text: *const u8, len: usize);
    fn r4os_zig_usb_xhci_diag_dec(value: u64);
    fn r4os_zig_usb_xhci_diag_hex(value: u64);
    fn r4os_zig_usb_xhci_diag_end();
    fn r4os_zig_kernel_hex(value: u64, width: u8);
}
pub(super) fn begin() -> IncidentToken {
    IncidentToken {
        generation: unsafe { r4os_zig_usb_xhci_incident_begin() },
    }
}
pub(super) fn resolve(token: IncidentToken) {
    unsafe {
        r4os_zig_usb_xhci_incident_resolve(token.generation);
    }
}
pub(super) fn write(text: &[u8]) {
    unsafe {
        r4os_zig_usb_xhci_diag_write(text.as_ptr(), text.len());
    }
}
pub(super) fn dec(value: impl Into<u64>) {
    unsafe {
        r4os_zig_usb_xhci_diag_dec(value.into());
    }
}
pub(super) fn hex(value: impl Into<u64>) {
    unsafe {
        r4os_zig_usb_xhci_diag_hex(value.into());
    }
}
pub(super) fn end() {
    unsafe {
        r4os_zig_usb_xhci_diag_end();
    }
}
pub(super) fn console_hex(value: impl Into<u64>, width: u8) {
    unsafe {
        r4os_zig_kernel_hex(value.into(), width);
    }
}
pub(super) fn console_dec(value: impl Into<u64>) {
    foundation::dec(value.into());
}
