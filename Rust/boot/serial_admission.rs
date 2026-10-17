//! One boot-long COM1 debug claim. Admission is closed before waiting for
//! writers; a failed/abandoned close retains the claim until reboot.
use crate::build_config::config;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering::SeqCst};
const COLD: u8 = 0;
const ACTIVE: u8 = 1;
const CLOSING: u8 = 2;
const RELEASED: u8 = 3;
const FAILED: u8 = 4;
const STARTING: u8 = 5;
static PHASE: AtomicU8 = AtomicU8::new(COLD);
static WRITERS: AtomicUsize = AtomicUsize::new(0);
pub(super) fn begin_init() -> bool {
    config::ENABLE_COM1_DEBUG
        && PHASE
            .compare_exchange(COLD, STARTING, SeqCst, SeqCst)
            .is_ok()
}
pub(super) fn initialized() {
    assert_eq!(PHASE.load(SeqCst), STARTING);
    PHASE.store(ACTIVE, SeqCst);
}
pub(super) fn active() -> bool {
    PHASE.load(SeqCst) == ACTIVE
}
/// Only construct while holding the existing serial owner. Drop before that
/// owner: caller kill/preemption cannot strand an admitted writer count.
pub(super) struct Writer;
pub(super) fn admit() -> Option<Writer> {
    if !active() {
        return None;
    }
    WRITERS
        .try_update(SeqCst, SeqCst, |n| n.checked_add(1))
        .ok()?;
    if !active() {
        WRITERS.fetch_sub(1, SeqCst);
        return None;
    }
    Some(Writer)
}
impl Drop for Writer {
    fn drop(&mut self) {
        assert!(WRITERS.fetch_sub(1, SeqCst) != 0);
    }
}
pub(super) fn begin_close() -> bool {
    PHASE
        .compare_exchange(ACTIVE, CLOSING, SeqCst, SeqCst)
        .is_ok()
}
pub(super) fn writers() -> usize {
    WRITERS.load(SeqCst)
}
pub(super) fn finish(released: bool) {
    assert_eq!(PHASE.load(SeqCst), CLOSING);
    if released {
        assert_eq!(writers(), 0);
    }
    PHASE.store(if released { RELEASED } else { FAILED }, SeqCst);
}
/// Used by the later serial-port owner; no normal claimant may infer release
/// from an empty buffer, closed route or failed UART acknowledgement.
pub(crate) fn claims_com1() -> bool {
    config::ENABLE_COM1_DEBUG && PHASE.load(SeqCst) != RELEASED
}
