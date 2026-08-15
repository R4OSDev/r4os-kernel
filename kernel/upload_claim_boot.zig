// Pre-runtime replay of durable create-only upload publish claims (0.60.22).
//
// SFTP, SCP and FTP hand an upload over from a private stage name to its
// target name.  A reset inside that hand-over used to lose the only recovery
// token, because it lived in RAM.  Since 0.60.22 a durable claim is written
// before the first visibility point; this seam drives every surviving claim
// to a terminal state before normal runtime consumers start, so no service
// ever observes a half-published object.
//
// Ordering: this runs directly after the SYSUPD package recovery and before
// IRQ/driver-policy/service/shell start, i.e. while the system still has the
// filesystem to itself.

const upload_claim_store = @import("../fs/upload_claim_store.zig");
const boot_status = @import("boot_status.zig");
const log = @import("log.zig");

var summary: upload_claim_store.ReplaySummary = .{};

/// Replays all durable upload claims.  Returns false only when the claim
/// store itself could not be read; an individual claim that fails stays on
/// disk for the next boot instead of blocking it.
pub fn recoverBeforeRuntime() bool {
    if (!upload_claim_store.recoverBeforeRuntime(&summary)) {
        log.puts("[UPLOADCLAIM] claim store unreadable\n");
        return false;
    }
    if (summary.total() == 0) return true;

    // Only report when something was actually pending, so a normal boot stays
    // quiet.  The counters are the visible telemetry the contract asks for.
    log.puts("[UPLOADCLAIM] replay published=");
    log.putDec(summary.published);
    log.puts(" rolled-back=");
    log.putDec(summary.rolled_back);
    log.puts(" retired=");
    log.putDec(summary.retired);
    log.puts(" foreign=");
    log.putDec(summary.foreign);
    log.puts(" invalid=");
    log.putDec(summary.invalid);
    log.puts(" failed=");
    log.putDec(summary.failed);
    log.puts("\n");

    if (summary.failed != 0) {
        boot_status.statusLine("Upload claim replay incomplete; retried on next boot");
    }
    return true;
}

pub fn lastSummary() upload_claim_store.ReplaySummary {
    return summary;
}
