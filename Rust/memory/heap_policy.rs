//! Unchanged RAM-aware growth and tail hysteresis.
pub const MIN_GROWTH_PAGES: usize = 16;
pub const MAX_GROWTH_PAGES: usize = 256;
pub const MIN_RETAINED_PAGES: usize = 16;
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_growth(required: usize, hint: usize, remaining: usize) -> usize {
    if required == 0 || remaining == 0 {
        return 0;
    }
    required
        .max(hint.clamp(MIN_GROWTH_PAGES, MAX_GROWTH_PAGES))
        .min(remaining)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_next_hint(committed: usize) -> usize {
    if committed == 0 {
        MIN_GROWTH_PAGES
    } else {
        committed
            .saturating_mul(2)
            .clamp(MIN_GROWTH_PAGES, MAX_GROWTH_PAGES)
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_retained(hint: usize, cap: usize, pressure: bool) -> usize {
    if pressure {
        return 0;
    }
    hint.max(MIN_RETAINED_PAGES)
        .min((cap / 1024).clamp(MIN_RETAINED_PAGES, MAX_GROWTH_PAGES))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_heap_release_tail(
    free: usize,
    retained: usize,
    pressure: bool,
) -> bool {
    if free == 0 {
        false
    } else if pressure {
        true
    } else {
        free > retained.saturating_mul(2).max(MIN_RETAINED_PAGES * 2)
    }
}
