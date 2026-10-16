//! Stateless access to the existing console/incident owners. Formatting and
//! incident ownership stay in the native controller; no log buffer is added.
use super::xhci_types::IncidentToken;
use crate::boot::foundation;

pub(super) fn begin() -> IncidentToken {
    IncidentToken {
        generation: unsafe { usb_xhci_incident_begin() },
    }
}
pub(super) fn resolve(token: IncidentToken) {
    {
        crate::display::diag_screen::resolve(token.generation);
    }
}
pub(super) fn write(text: &[u8]) {
    unsafe {
        crate::display::diag_screen::r4os_rust_diag_write(text.as_ptr(), text.len());
    }
}
pub(super) fn dec(value: impl Into<u64>) {
    unsafe {
        usb_xhci_diag_dec(value.into());
    }
}
pub(super) fn hex(value: impl Into<u64>) {
    unsafe {
        usb_xhci_diag_hex(value.into());
    }
}
pub(super) fn end() {
    {
        crate::display::diag_screen::end_line();
    }
}
pub(super) fn console_hex(value: impl Into<u64>, width: u8) {
    {
        crate::boot::log_route::hex(value.into(), width);
    }
}
pub(super) fn console_dec(value: impl Into<u64>) {
    foundation::dec(value.into());
}

unsafe fn usb_xhci_diag_dec(value: u64) {
    crate::display::diag_screen::number(value, false);
}

unsafe fn usb_xhci_diag_hex(value: u64) {
    crate::display::diag_screen::number(value, true);
}

unsafe fn usb_xhci_incident_begin() -> u64 {
    crate::display::diag_screen::begin_resolvable()
}
