//! Original bounded boot/reset/SCSI timing, rounded up in functional ticks.
pub const PORT_RESET_RECOVERY_MS: u32 = 50;
pub const SET_ADDRESS_SETTLE_MS: u32 = 10;
pub const XHCI_EVENT_TIMEOUT_MS: u32 = 2_000;
pub const XHCI_BIOS_HANDOFF_TIMEOUT_MS: u32 = 1_000;
pub const XHCI_CONTROLLER_HALT_TIMEOUT_MS: u32 = 1_000;
pub const XHCI_CONTROLLER_RESET_TIMEOUT_MS: u32 = 1_000;
pub const XHCI_CONTROLLER_READY_TIMEOUT_MS: u32 = 1_000;
pub const XHCI_CONTROLLER_RUN_TIMEOUT_MS: u32 = 1_000;
pub const XHCI_PORT_DEBOUNCE_STABLE_MS: u32 = 100;
pub const XHCI_PORT_DEBOUNCE_TIMEOUT_MS: u32 = 500;
pub const XHCI_PORT_POWER_TIMEOUT_MS: u32 = 100;
pub const XHCI_PORT_RESET_TIMEOUT_MS: u32 = 500;
pub const XHCI_PORT_WARM_RESET_TIMEOUT_MS: u32 = 500;
pub const XHCI_PORT_U0_TIMEOUT_MS: u32 = 500;
pub const MSC_RECOVERY_BUDGET_MS: u32 = 8_000;
pub const SCSI_RETRY_DELAY_MS: u32 = 100;
pub const SCSI_INQUIRY_ATTEMPTS: u8 = 3;
pub const SCSI_READY_ATTEMPTS: u8 = 20;
pub const SCSI_CAPACITY_ATTEMPTS: u8 = 5;
pub const SCSI_INQUIRY_BUDGET_MS: u32 = 500;
pub const SCSI_READY_BUDGET_MS: u32 = 2_000;
pub const SCSI_CAPACITY_BUDGET_MS: u32 = 500;
#[expect(
    clippy::manual_div_ceil,
    reason = "Retain the original checked rounding expression during migration."
)]
pub fn ticks_for_milliseconds(milliseconds: u32, hz: u32) -> u64 {
    if milliseconds == 0 || hz == 0 {
        return 0;
    }
    let scaled = u64::from(milliseconds) * u64::from(hz);
    ((scaled + 999) / 1000).max(1)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_ticks(milliseconds: u32, hz: u32) -> u64 {
    ticks_for_milliseconds(milliseconds, hz)
}
