// Exact resident upload replay summary is published by the native boot owner.
const store=@import("../fs/upload_claim_store.zig");
extern fn r4os_rust_upload_boot_recover()bool;
extern fn r4os_rust_upload_boot_summary(out:*store.ReplaySummary)void;
pub fn recoverBeforeRuntime()bool{return r4os_rust_upload_boot_recover();}
pub fn lastSummary()store.ReplaySummary{var out:store.ReplaySummary=undefined;r4os_rust_upload_boot_summary(&out);return out;}
