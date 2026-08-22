// Pure retry policy for USB Mass Storage BOT commands.
//
// BOT/xHCI recovery repairs the transport, but it does not replay the SCSI
// command that failed. READ10, a sector-exact WRITE10, and SYNCHRONIZE CACHE
// are safe to repeat once after a fully successful recovery. Command/protocol
// failures and incomplete recoveries must never be retried as transport
// failures.

pub const transport_retry_limit: u8 = 1;

pub fn shouldRetryTransport(
    last_failure_transport: bool,
    last_recovery_ok: bool,
    retries: u8,
) bool {
    return last_failure_transport and
        last_recovery_ok and
        retries < transport_retry_limit;
}

// A valid SCSI sense result is the device's state contract. Unit Attention
// and readiness states that can clear on their own are retried inside the
// caller's wall-clock budget. Permanent protocol/media states stop
// immediately. Without valid sense, retain exactly one compatibility retry
// for devices that transiently fail REQUEST SENSE itself.
pub fn shouldRetryScsiFailure(
    last_failure_transport: bool,
    last_recovery_ok: bool,
    sense_valid: bool,
    sense_key: u8,
    asc: u8,
    ascq: u8,
    retries: u8,
) bool {
    if (last_failure_transport) return last_recovery_ok;
    if (!sense_valid) return retries == 0;
    return switch (sense_key) {
        0x06 => true, // Unit Attention is cleared by the sense exchange.
        0x02 => transientNotReady(asc, ascq),
        else => false,
    };
}

pub fn transientNotReady(asc: u8, ascq: u8) bool {
    if (asc == 0x3A) return false; // Medium not present.
    if (asc != 0x04) return true;
    return switch (ascq) {
        0x02, // Initializing command required; START STOP is not issued here.
        0x03, // Manual intervention required.
        0x12, // Logical unit offline.
        => false,
        else => true,
    };
}

test "successful transport recovery permits the first retry" {
    const testing = @import("std").testing;
    try testing.expect(shouldRetryTransport(true, true, 0));
}

test "transport retry is bounded to exactly one" {
    const testing = @import("std").testing;
    try testing.expect(!shouldRetryTransport(true, true, 1));
    try testing.expect(!shouldRetryTransport(true, true, 2));
}

test "command failure is not a transport retry" {
    const testing = @import("std").testing;
    try testing.expect(!shouldRetryTransport(false, true, 0));
}

test "failed recovery is never retried" {
    const testing = @import("std").testing;
    try testing.expect(!shouldRetryTransport(true, false, 0));
}

test "unclassified failure is never retried" {
    const testing = @import("std").testing;
    try testing.expect(!shouldRetryTransport(false, false, 0));
}

test "transient SCSI readiness and unit attention are retried" {
    const testing = @import("std").testing;
    try testing.expect(shouldRetryScsiFailure(false, false, true, 0x02, 0x04, 0x01, 0));
    try testing.expect(shouldRetryScsiFailure(false, false, true, 0x06, 0x29, 0x00, 0));
}

test "permanent SCSI readiness and protocol errors stop" {
    const testing = @import("std").testing;
    try testing.expect(!shouldRetryScsiFailure(false, false, true, 0x02, 0x3A, 0x00, 0));
    try testing.expect(!shouldRetryScsiFailure(false, false, true, 0x02, 0x04, 0x03, 0));
    try testing.expect(!shouldRetryScsiFailure(false, false, true, 0x05, 0x20, 0x00, 0));
}

test "missing sense keeps one bounded compatibility retry" {
    const testing = @import("std").testing;
    try testing.expect(shouldRetryScsiFailure(false, false, false, 0, 0, 0, 0));
    try testing.expect(!shouldRetryScsiFailure(false, false, false, 0, 0, 0, 1));
}

test "transport retry requires completed recovery" {
    const testing = @import("std").testing;
    try testing.expect(shouldRetryScsiFailure(true, true, false, 0, 0, 0, 3));
    try testing.expect(!shouldRetryScsiFailure(true, false, false, 0, 0, 0, 0));
}
