// Linux uses the USB reset-recovery minimum plus margin before enumeration.
// The extra margin matters after firmware-to-kernel xHCI ownership handoff,
// where the port can still report Enabled/U0 although the device must be put
// back into Default state first.
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
// One BOT/xHCI recovery incident owns this single wall-clock budget.  Each
// command still has the shorter per-event timeout, but later recovery steps
// may not start a fresh full window after the incident budget expired.
pub const MSC_RECOVERY_BUDGET_MS: u32 = 8_000;

pub const SCSI_RETRY_DELAY_MS: u32 = 100;
pub const SCSI_INQUIRY_ATTEMPTS: u8 = 3;
pub const SCSI_READY_ATTEMPTS: u8 = 20;
pub const SCSI_CAPACITY_ATTEMPTS: u8 = 5;
pub const SCSI_INQUIRY_BUDGET_MS: u32 = 500;
pub const SCSI_READY_BUDGET_MS: u32 = 2_000;
pub const SCSI_CAPACITY_BUDGET_MS: u32 = 500;

pub fn ticksForMilliseconds(milliseconds: u32, frequency_hz: u32) u64 {
    if (milliseconds == 0 or frequency_hz == 0) return 0;
    const scaled = @as(u64, milliseconds) * @as(u64, frequency_hz);
    return @max(@as(u64, 1), (scaled + 999) / 1000);
}

test "millisecond delays round up to at least one tick" {
    const testing = @import("std").testing;

    try testing.expectEqual(@as(u64, 50), ticksForMilliseconds(PORT_RESET_RECOVERY_MS, 1000));
    try testing.expectEqual(@as(u64, 10), ticksForMilliseconds(SET_ADDRESS_SETTLE_MS, 1000));
    try testing.expectEqual(@as(u64, 2000), ticksForMilliseconds(XHCI_EVENT_TIMEOUT_MS, 1000));
    try testing.expectEqual(@as(u64, 1), ticksForMilliseconds(1, 100));
    try testing.expectEqual(@as(u64, 0), ticksForMilliseconds(0, 1000));
}

test "boot retry windows remain bounded" {
    const testing = @import("std").testing;

    try testing.expect(SCSI_INQUIRY_ATTEMPTS >= 2);
    try testing.expect(SCSI_READY_ATTEMPTS >= 10);
    try testing.expect(SCSI_READY_ATTEMPTS <= 30);
    try testing.expect(SCSI_CAPACITY_ATTEMPTS >= 2);
    try testing.expect(SCSI_RETRY_DELAY_MS <= 250);
    try testing.expect(SCSI_INQUIRY_BUDGET_MS >= retryDelayWindow(SCSI_INQUIRY_ATTEMPTS));
    try testing.expect(SCSI_READY_BUDGET_MS >= retryDelayWindow(SCSI_READY_ATTEMPTS));
    try testing.expect(SCSI_CAPACITY_BUDGET_MS >= retryDelayWindow(SCSI_CAPACITY_ATTEMPTS));
    try testing.expect(XHCI_CONTROLLER_RESET_TIMEOUT_MS <= XHCI_EVENT_TIMEOUT_MS);
    try testing.expect(XHCI_PORT_DEBOUNCE_STABLE_MS < XHCI_PORT_DEBOUNCE_TIMEOUT_MS);
}

fn retryDelayWindow(attempts: u8) u32 {
    if (attempts <= 1) return 0;
    return @as(u32, attempts - 1) * SCSI_RETRY_DELAY_MS;
}
