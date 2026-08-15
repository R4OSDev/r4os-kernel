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
