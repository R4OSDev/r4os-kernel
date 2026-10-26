//! Standalone host checks of the real cache policy/index owner.
//! Run with the pinned rustc --test; this file is not linked into the kernel.
#![allow(dead_code)]
#[path = "page_cache_policy.rs"]
mod policy;
use policy::{Index, MAX_ENTRIES, Queue};

fn index() -> Box<Index> {
    let mut output = Box::<Index>::new_uninit();
    unsafe {
        policy::r4os_rust_fs_cache_index(output.as_mut_ptr(), 0, 0, 0, false);
        output.assume_init()
    }
}
fn op(state: &mut Index, operation: u8, slot: usize, device: usize) -> usize {
    unsafe { policy::r4os_rust_fs_cache_index(state, operation, slot, device, false) }
}
fn attach(state: &mut Index, device: usize, cold: bool) -> usize {
    let slot = op(state, 1, 0, 0);
    assert!(slot < MAX_ENTRIES);
    assert!(state.attach_class(slot, device, cold));
    slot
}

#[test]
fn ram_budget_grows_above_two_mb_and_contracts_at_real_pressure_thresholds() {
    let large = policy::capacity_for_memory(4_194_304, 4_194_304);
    assert_eq!(large.ram_pages, 8192);
    assert_eq!(u64::from(large.active_pages) * 4096, 32 * 1024 * 1024);
    let kvm = policy::capacity_for_memory(480_000, 480_000);
    assert!(kvm.active_pages > 512 && kvm.active_pages <= large.active_pages);
    for (free, expected, pressure) in [(8192, 256, 1), (4096, 128, 2), (1024, 64, 3)] {
        let constrained = policy::capacity_for_memory(480_000, free);
        assert_eq!(constrained.ram_pages, kvm.ram_pages);
        assert_eq!(constrained.active_pages, expected);
        assert_eq!(constrained.pressure_level, pressure);
    }
    let tiny = policy::capacity_for_memory(0, 0);
    assert_eq!(tiny.ram_pages, 64);
    assert_eq!(tiny.active_pages, 64);
    assert_eq!(
        policy::capacity_for_memory(u64::MAX, u64::MAX).ram_pages,
        8192
    );
}

#[test]
fn bulk_fill_window_recycles_cold_heads_without_displacing_reused_metadata() {
    let mut state = index();
    let hot: Vec<_> = (0..32).map(|_| attach(&mut state, 0, false)).collect();
    let cold: Vec<_> = (0..16).map(|_| attach(&mut state, 0, true)).collect();
    // Actual fill pins remove the entire bounded window from victim lists.
    for &slot in &cold {
        assert_eq!(op(&mut state, 6, slot, 0), 1);
    }
    assert!(hot.contains(&op(&mut state, 10, 0, 0)));
    for &slot in &cold {
        assert_eq!(op(&mut state, 7, slot, 0), 1);
    }
    for _ in 0..1024 {
        let victim = op(&mut state, 10, 0, 0);
        assert!(cold.contains(&victim));
        assert_eq!(op(&mut state, 8, victim, 0), 1);
        assert!(state.attach_class(victim, 0, true));
    }
    for slot in hot {
        assert!(state.links[slot].queue == Queue::Clean && !state.links[slot].cold);
    }
    assert_eq!(op(&mut state, 14, 0, 0), 48);
}

#[test]
fn actual_reuse_promotes_cold_and_dirty_or_busy_never_becomes_a_clean_victim() {
    let mut state = index();
    let reused = attach(&mut state, 0, true);
    let cold = attach(&mut state, 0, true);
    op(&mut state, 3, reused, 0);
    assert!(!state.links[reused].cold);
    assert_eq!(op(&mut state, 10, 0, 0), cold);
    assert_eq!(op(&mut state, 4, cold, 0), 1);
    assert!(!state.links[cold].cold && state.links[cold].queue == Queue::Dirty);
    assert_eq!(op(&mut state, 6, reused, 0), 1);
    assert_eq!(op(&mut state, 10, 0, 0), usize::MAX);
    assert_eq!(op(&mut state, 6, cold, 0), 1);
    assert_eq!(op(&mut state, 11, 0, 0), usize::MAX);
    assert_eq!(state.busy_count, 2);
    assert_eq!(state.devices[0].busy_dirty, 1);
    assert_eq!(op(&mut state, 5, cold, 0), 1);
    assert!(state.links[cold].queue == Queue::BusyClean);
    assert_eq!(op(&mut state, 10, 0, 0), usize::MAX);
    assert_eq!(op(&mut state, 7, cold, 0), 1);
    assert_eq!(op(&mut state, 10, 0, 0), cold);
    assert_eq!(op(&mut state, 7, reused, 0), 1);
    assert_eq!(state.busy_count, 0);
}

#[test]
fn all_8192_slots_are_real_and_reclaim_uses_linked_heads_with_pins_preserved() {
    let mut state = index();
    let slots: Vec<_> = (0..MAX_ENTRIES)
        .map(|_| attach(&mut state, 0, false))
        .collect();
    assert_eq!(op(&mut state, 1, 0, 0), usize::MAX);
    assert_eq!(op(&mut state, 14, 0, 0), MAX_ENTRIES);
    let live = slots[MAX_ENTRIES - 1];
    assert_eq!(op(&mut state, 6, live, 0), 1);
    while op(&mut state, 14, 0, 0) > 64 {
        let before = state.clean_device_probes;
        let victim = op(&mut state, 10, 0, usize::MAX);
        assert_ne!(victim, live);
        assert!(state.clean_device_probes - before <= policy::MAX_DEVICES as u64);
        assert_eq!(op(&mut state, 9, victim, 0), 1);
    }
    assert!(state.links[live].queue == Queue::BusyClean);
    assert_eq!(state.busy_count, 1);
    assert_eq!(op(&mut state, 7, live, 0), 1);
    assert_eq!(state.free_count as usize, MAX_ENTRIES - 64);
    assert!(core::mem::size_of::<Index>() < 128 * 1024);
}

#[test]
fn dirty_fifo_and_device_fairness_survive_bulk_admission() {
    let mut state = index();
    let a = attach(&mut state, 0, false);
    let b = attach(&mut state, 1, false);
    assert_eq!(op(&mut state, 4, a, 0), 1);
    assert_eq!(op(&mut state, 4, b, 0), 1);
    assert_eq!(op(&mut state, 13, 0, 0), 0);
    assert_eq!(op(&mut state, 13, 0, 0), 1);
    assert_eq!(op(&mut state, 11, 0, 0), a);
    assert_eq!(op(&mut state, 11, 0, 1), b);
    assert_eq!(op(&mut state, 10, 0, usize::MAX), usize::MAX);
}

#[test]
fn existing_confirmed_fill_bounds_remain_independent_from_larger_residency() {
    assert_eq!(policy::fill_run_page_limit(16, 8, 8, 8192), 1);
    assert_eq!(policy::fill_run_page_limit(16, 32, 8, 8192), 4);
    assert_eq!(policy::fill_run_page_limit(16, 128, 8, 2), 2);
    assert_eq!(policy::fill_run_page_limit(16, 128, 8, 0), 0);
    assert_eq!(policy::MAX_FILL_PAGES, 16);
    assert_eq!(policy::TRIM_PAGES, 16);
}

fn ahead() -> policy::ReadAhead {
    policy::ReadAhead {
        generation: 1,
        pending: false,
        pending_page: 0,
        pending_pages: 0,
        inflight: false,
        inflight_page: 0,
        inflight_pages: 0,
        resident_pages: 0,
        sequential: policy::Sequential {
            have_last: false,
            last_end_page: 0,
            window_pages: 0,
        },
    }
}
#[test]
fn sequential_evidence_adapts_one_two_four_and_random_or_pressure_stops_it() {
    let mut a = ahead();
    assert_eq!(a.sequential.observe(10, 11, 4).pages, 0);
    assert_eq!(a.sequential.observe(11, 12, 4).pages, 1);
    assert_eq!(a.sequential.observe(12, 13, 4).pages, 2);
    for first in 13..100 {
        assert_eq!(a.sequential.observe(first, first + 1, 4).pages, 4);
    }
    let random = a.sequential.observe(500, 501, 4);
    assert!(random.random_reset);
    assert_eq!(random.pages, 0);
    assert_eq!(a.sequential.observe(501, 502, 0).pages, 0);
    for free in [8192, 4096, 1024, 0] {
        assert_eq!(
            policy::capacity_for_memory(500_000, free).read_ahead_pages,
            0
        );
    }
    assert_eq!(
        policy::capacity_for_memory(500_000, 500_000).read_ahead_pages,
        4
    );
}
#[test]
fn cancellation_never_releases_inflight_until_exact_completion_and_refuses_stale_data() {
    let mut a = ahead();
    a.schedule(100, 4);
    let old = a.begin().unwrap();
    assert!(a.cancel_all());
    assert!(a.inflight);
    assert!(a.begin().is_none());
    a.schedule(300, 2);
    assert!(a.begin().is_none());
    assert!(!a.complete(old, true));
    assert!(!a.inflight);
    assert!(a.pending);
    let fresh = a.begin().unwrap();
    assert_eq!(fresh.page, 300);
    assert!(a.complete(fresh, true));
    a.schedule(400, 2);
    let request = a.begin().unwrap();
    assert!(!a.demand(400));
    assert!(a.inflight);
    assert!(a.demand(900));
    assert!(!a.complete(request, true));
    a.resident_pages = 2;
    unsafe {
        policy::r4os_rust_fs_cache_readahead(
            &mut a,
            5,
            0,
            0,
            core::ptr::null(),
            false,
            core::ptr::null_mut(),
        );
    }
    assert_eq!(a.resident_pages, 1);
}
