//! Original dirty-bracket retention and terminal allocation cleanup.
use super::{
    ntfs_volume_clusters as c, ntfs_volume_io as io, ntfs_volume_mft as m,
    ntfs_volume_records as r, ntfs_volume_types::*,
};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
static BUDGET: AtomicU64 = AtomicU64::new(0);
pub static CUT_AFTER_CANONICAL: AtomicBool = AtomicBool::new(false);
pub fn set_budget(value: Option<u32>) {
    BUDGET.store(value.map_or(0, |x| (1u64 << 32) | u64::from(x)), Relaxed)
}
pub fn budget() -> Option<u32> {
    let x = BUDGET.load(Relaxed);
    if x & (1u64 << 32) != 0 {
        Some(x as u32)
    } else {
        None
    }
}
pub(super) unsafe fn budgeted_flush(v: Volume) -> bool {
    unsafe {
        if !io::flush(v.io) {
            return false;
        }
        if let Some(n) = budget() {
            if n == 0 {
                return false;
            }
            set_budget(Some(n - 1))
        }
        true
    }
}
#[inline(never)]
pub(super) unsafe fn uncertain(v: Volume) -> WriteStatus {
    unsafe {
        if !v.cache().is_null() {
            super::ntfs_volume_cache::with(v.cache(), |cache| cache.invalidate(Mutation::Recovery))
        }
        let marked = r::ensure_dirty(v);
        (*v.scratch()).repair_required = true;
        if marked {
            WriteStatus::Io
        } else {
            WriteStatus::CleanupFailed
        }
    }
}
#[inline(never)]
pub(super) unsafe fn abort(v: Volume, status: WriteStatus) -> WriteStatus {
    unsafe {
        if status == WriteStatus::CleanupFailed {
            return status;
        }
        if !v.cache().is_null() {
            super::ntfs_volume_cache::with(v.cache(), |cache| cache.invalidate(Mutation::Recovery))
        }
        if !io::flush(v.io) || !r::set_dirty(v, false) {
            let _ = uncertain(v);
            return WriteStatus::CleanupFailed;
        }
        status
    }
}
pub(super) unsafe fn after_commit(v: Volume, status: WriteStatus) -> WriteStatus {
    unsafe {
        if matches!(status, WriteStatus::Io | WriteStatus::CleanupFailed) {
            if !v.cache().is_null() {
                super::ntfs_volume_cache::with(v.cache(), |cache| {
                    cache.invalidate(Mutation::Recovery)
                })
            }
            status
        } else {
            abort(v, status)
        }
    }
}
#[inline(never)]
pub(super) unsafe fn freeing(v: Volume, runs: io::Runs, status: WriteStatus) -> WriteStatus {
    unsafe {
        if status == WriteStatus::CleanupFailed {
            return status;
        }
        if !v.cache().is_null() {
            super::ntfs_volume_cache::with(v.cache(), |cache| cache.invalidate(Mutation::Recovery))
        }
        if runs.len != 0 && !c::free(v, runs) {
            return WriteStatus::CleanupFailed;
        }
        abort(v, status)
    }
}
#[inline(never)]
pub(super) unsafe fn releasing(
    v: Volume,
    number: u64,
    stored: bool,
    runs: io::Runs,
    status: WriteStatus,
) -> WriteStatus {
    unsafe {
        if status == WriteStatus::CleanupFailed {
            return status;
        }
        if !v.cache().is_null() {
            super::ntfs_volume_cache::with(v.cache(), |cache| cache.invalidate(Mutation::Recovery))
        }
        if stored && !m::mark_free(v, number) {
            return WriteStatus::CleanupFailed;
        }
        if !m::release(v, number) {
            return WriteStatus::CleanupFailed;
        }
        freeing(v, runs, status)
    }
}
#[inline(never)]
pub(super) unsafe fn unpublished(
    v: Volume,
    number: u64,
    stored: bool,
    status: WriteStatus,
) -> WriteStatus {
    unsafe {
        if !v.cache().is_null() {
            super::ntfs_volume_cache::with(v.cache(), |cache| cache.invalidate(Mutation::Recovery))
        }
        if stored && !m::mark_free(v, number) {
            return WriteStatus::CleanupFailed;
        }
        if !m::release(v, number) {
            return WriteStatus::CleanupFailed;
        }
        status
    }
}
