//! Private retiring ranges remain unavailable until their exact completion.
//! Tokens carry values only; no mutable heap borrow crosses a handoff.
use super::*;

pub(super) const RETIRING: u64 = 2;

pub(super) struct Retired {
    offset: usize,
    size: usize,
    ticket: u64,
    requested: Option<usize>,
}

pub(super) fn can_unlock(guard: &HeapGuard, size: usize) -> bool {
    size >= PAGE && guard.can_drop()
}

/// The caller owns the exact live block or a newly split realloc tail under4.
/// Used boundary tags keep neighbours, bins, growth and tail trim away.
pub(super) fn prepare(
    state: &mut Control,
    offset: usize,
    size: usize,
    requested: Option<usize>,
) -> Option<Retired> {
    let ticket = state.next_release_id.checked_add(1)?;
    state.next_release_id = ticket;
    let header = size as u64 | 1 | RETIRING;
    write(offset, header);
    write(offset + 8, ticket);
    write(offset + size - FOOTER, header);
    Some(Retired {
        offset,
        size,
        ticket,
        requested,
    })
}

pub(super) fn finish(retired: Retired, guard: &mut HeapGuard) {
    // Entire kernel-heap blocks are resident. This phase only fills memory;
    // it cannot perform VM/I/O work or gain permission to wait.
    guard.suspend();
    unsafe {
        core::ptr::write_bytes(payload(retired.offset), 0xdd, retired.size - OVERHEAD);
    }
    guard.resume();
    control(|state| {
        let header = retired.size as u64 | 1 | RETIRING;
        assert!(
            retired
                .offset
                .checked_add(retired.size)
                .is_some_and(|end| end <= state.heap_top)
                && read(retired.offset) == header
                && read(retired.offset + 8) == retired.ticket
                && read(retired.offset + retired.size - FOOTER) == header,
            "heap retiring block identity"
        );
        state.poison_bytes = state
            .poison_bytes
            .wrapping_add((retired.size - OVERHEAD) as u64);
        if let Some(requested) = retired.requested {
            state.active_blocks -= 1;
            state.used_bytes -= requested;
        }
        state.publish_free(retired.offset, retired.size);
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Tail {
    range: u32,
    start: usize,
    bytes: usize,
    pressure: bool,
}
impl Tail {
    fn snapshot(state: &Control) -> Self {
        Self {
            range: state.range_id,
            start: state.pending_uncommit_start,
            bytes: state.pending_uncommit_bytes,
            pressure: state.pending_uncommit_pressure,
        }
    }
}

pub(super) fn finish_tail(guard: &mut HeapGuard) -> bool {
    let tail = control(|state| Tail::snapshot(state));
    if tail.bytes == 0 {
        return true;
    }
    // Preserve the original context's permission before returning owner4.
    // Outer owners, disabled IF and non-waitable tasks keep the old locked path.
    let drop_owner = guard.can_wait();
    assert_eq!(GROWTH.load(Ordering::Acquire), 0, "heap tail serialization");
    GROWTH.store(1, Ordering::Release);
    control(|state| state.uncommit_calls = state.uncommit_calls.wrapping_add(1));
    if drop_owner {
        guard.suspend();
    }
    let completed = unsafe { heap_uncommit(tail.range, tail.start as u64, tail.bytes as u64) };
    if drop_owner {
        guard.resume();
    }
    control(|state| {
        assert!(Tail::snapshot(state) == tail, "heap retiring tail identity");
        assert_eq!(GROWTH.load(Ordering::Acquire), 1, "heap tail owner");
        if completed {
            state.uncommitted_pages_total = state
                .uncommitted_pages_total
                .wrapping_add((tail.bytes / PAGE) as u64);
            if tail.pressure {
                state.pressure_releases = state.pressure_releases.wrapping_add(1);
            }
            state.pending_uncommit_start = 0;
            state.pending_uncommit_bytes = 0;
            state.pending_uncommit_pressure = false;
        } else {
            // A partial VM completion retains the full original retry anchor.
            state.uncommit_failures = state.uncommit_failures.wrapping_add(1);
        }
    });
    GROWTH.store(0, Ordering::Release);
    guard.wake_growth |= guard.can_drop() && guard.unwind.context_present;
    completed
}
