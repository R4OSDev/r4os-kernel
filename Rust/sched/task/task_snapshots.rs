//! Bounded native registry diagnostics. Snapshot records are fixed C values;
//! opaque handles and retained spans never expose native TaskCore layout.
use super::{
    task_registry as registry,
    task_types::{State, Task},
};
use crate::arch::runtime;
use core::ptr::{self, null_mut};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Summary {
    pub max_tasks: u32,
    pub total: u32,
    pub ready: u32,
    pub running: u32,
    pub blocked: u32,
    pub dead: u32,
    pub workers: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InventoryRow {
    pub task_id: u32,
    pub state: u8,
    pub generation: u64,
    pub created_tick: u64,
    pub last_run_tick: u64,
    pub wake_tick: u64,
    pub runtime_ticks: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InventoryPage {
    pub epoch: u64,
    pub total: u32,
    pub returned: u32,
    pub has_more: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct QueueSnapshot {
    pub registry: usize,
    pub ready: usize,
    pub timed: usize,
    pub reap: usize,
    pub valid: bool,
}
pub(crate) unsafe fn summary() -> Summary {
    let flags = runtime::save_runtime();
    let mut result = Summary::default();
    unsafe {
        result.total = (*registry::raw()).count.min(u32::MAX as usize) as u32;
        let mut cursor = (*registry::raw()).head;
        while !cursor.is_null() {
            let c = registry::core(cursor);
            if (*c).entry.is_some() && !matches!((*c).state, State::Unused | State::Dead) {
                result.workers += 1;
            }
            match (*c).state {
                State::Ready => result.ready += 1,
                State::Running => result.running += 1,
                State::Blocked => result.blocked += 1,
                State::Dead => result.dead += 1,
                State::Unused => {}
            }
            cursor = (*c).registry_next;
        }
    }
    runtime::restore_runtime(flags);
    result
}
unsafe fn row(task: *mut Task) -> InventoryRow {
    unsafe {
        let c = registry::core(task);
        InventoryRow {
            task_id: (*c).id,
            state: (*c).state as u8,
            generation: (*c).generation,
            created_tick: (*c).created_tick,
            last_run_tick: (*c).last_scheduled_tick,
            wake_tick: (*c).wake_tick,
            runtime_ticks: (*c).run_ticks,
        }
    }
}
fn sift_up(rows: &mut [InventoryRow], mut index: usize) {
    while index != 0 {
        let parent = (index - 1) / 2;
        if rows[parent].generation >= rows[index].generation {
            break;
        }
        rows.swap(parent, index);
        index = parent;
    }
}
fn sift_down(rows: &mut [InventoryRow], mut index: usize) {
    loop {
        let left = index * 2 + 1;
        if left >= rows.len() {
            return;
        }
        let right = left + 1;
        let mut largest = left;
        if right < rows.len() && rows[right].generation > rows[left].generation {
            largest = right;
        }
        if rows[index].generation >= rows[largest].generation {
            return;
        }
        rows.swap(index, largest);
        index = largest;
    }
}
pub(super) unsafe fn inventory_page(after: u64, rows: &mut [InventoryRow]) -> InventoryPage {
    if rows.is_empty() {
        return InventoryPage::default();
    }
    let flags = runtime::save_runtime();
    let epoch = unsafe { (*registry::raw()).epoch };
    let mut total = 0u32;
    let mut eligible = 0u32;
    let mut selected = 0usize;
    unsafe {
        let mut cursor = (*registry::raw()).head;
        while !cursor.is_null() {
            let c = registry::core(cursor);
            let candidate = cursor;
            cursor = (*c).registry_next;
            if (*c).state == State::Unused {
                continue;
            }
            total = total.saturating_add(1);
            if (*c).generation <= after {
                continue;
            }
            eligible = eligible.saturating_add(1);
            let value = row(candidate);
            if selected < rows.len() {
                rows[selected] = value;
                sift_up(&mut rows[..selected + 1], selected);
                selected += 1;
            } else if value.generation < rows[0].generation {
                rows[0] = value;
                sift_down(&mut rows[..selected], 0);
            }
        }
    }
    runtime::restore_runtime(flags);
    // Same insertion sort outside the short runtime admission as the old
    // owner. No heap, iterator collection, or second inventory truth.
    for index in 1..selected {
        let value = rows[index];
        let mut insertion = index;
        while insertion != 0 && rows[insertion - 1].generation > value.generation {
            rows[insertion] = rows[insertion - 1];
            insertion -= 1;
        }
        rows[insertion] = value;
    }
    InventoryPage {
        epoch,
        total,
        returned: selected as u32,
        has_more: eligible > selected as u32,
    }
}
pub(super) unsafe fn pin_ordinal(wanted: usize) -> *mut Task {
    let flags = runtime::save_runtime();
    let mut ordinal = 0usize;
    let mut cursor = unsafe { (*registry::raw()).head };
    let mut result = null_mut();
    unsafe {
        while !cursor.is_null() {
            let c = registry::core(cursor);
            if ordinal == wanted {
                if (*c).state != State::Unused
                    && !(*c).release_in_progress
                    && (*c).pin_count != u32::MAX
                {
                    (*c).pin_count += 1;
                    result = cursor;
                }
                break;
            }
            ordinal += 1;
            cursor = (*c).registry_next;
        }
    }
    runtime::restore_runtime(flags);
    result
}
pub(super) unsafe fn ordinal(task: *const Task) -> Option<usize> {
    let flags = runtime::save_runtime();
    let mut index = 0usize;
    let mut cursor = unsafe { (*registry::raw()).head };
    let mut result = None;
    unsafe {
        while !cursor.is_null() {
            if ptr::eq(cursor, task) {
                result = Some(index);
                break;
            }
            index += 1;
            cursor = (*registry::core(cursor)).registry_next;
        }
    }
    runtime::restore_runtime(flags);
    result
}
pub(super) unsafe fn queues() -> QueueSnapshot {
    let flags = runtime::save_runtime();
    let result = unsafe { queues_held() };
    runtime::restore_runtime(flags);
    result
}
unsafe fn queues_held() -> QueueSnapshot {
    unsafe {
        let r = registry::raw();
        let mut out = QueueSnapshot {
            valid: true,
            ..QueueSnapshot::default()
        };
        let mut expected_ready = 0usize;
        let mut expected_timed = 0usize;
        let mut expected_reap = 0usize;
        let mut expected = [[0usize; registry::ROLE_COUNT]; registry::MAX_CPUS];
        let mut previous: *mut Task = null_mut();
        let mut cursor = (*r).head;
        while !cursor.is_null() {
            let c = registry::core(cursor);
            out.registry += 1;
            if out.registry > (*r).count {
                out.valid = false;
                break;
            }
            if (*c).registry_prev != previous {
                out.valid = false;
            }
            let ready = (*c).state == State::Ready && (*c).running_cpu == 0xff;
            let timed = (*c).state == State::Blocked && (*c).wake_tick != 0;
            let reap = (*c).retire_pending && !(*c).release_in_progress;
            if (*c).ready_linked != ready
                || (*c).timeout_linked != timed
                || (*c).reap_linked != reap
            {
                out.valid = false;
            }
            if ready {
                expected_ready += 1;
                expected[(*c).home_cpu as usize][registry::dispatch_rank(cursor) as usize] += 1;
            }
            if timed {
                expected_timed += 1;
            }
            if reap {
                expected_reap += 1;
            }
            previous = cursor;
            cursor = (*c).registry_next;
        }
        if out.registry != (*r).count || previous != (*r).tail {
            out.valid = false;
        }
        for (cpu, expected_cpu) in expected.iter().enumerate() {
            let q = ptr::addr_of!((*r).ready[cpu]);
            previous = null_mut();
            cursor = (*q).head;
            let mut observed = [0usize; registry::ROLE_COUNT];
            let mut first = [null_mut::<Task>(); registry::ROLE_COUNT];
            let mut count = 0;
            while !cursor.is_null() {
                let c = registry::core(cursor);
                out.ready += 1;
                count += 1;
                if count > (*q).count {
                    out.valid = false;
                    break;
                }
                if !(*c).ready_linked
                    || (*c).state != State::Ready
                    || (*c).home_cpu as usize != cpu
                    || (*c).ready_prev != previous
                {
                    out.valid = false;
                }
                let rank = registry::dispatch_rank(cursor) as usize;
                observed[rank] += 1;
                if first[rank].is_null() {
                    first[rank] = cursor;
                }
                if (*c).ready_rank as usize != rank {
                    out.valid = false;
                }
                if !previous.is_null() && (*registry::core(previous)).ready_order > (*c).ready_order
                {
                    out.valid = false;
                }
                previous = cursor;
                cursor = (*c).ready_next;
            }
            for rank in 0..registry::ROLE_COUNT {
                if observed[rank] != (*q).dispatch_counts[rank]
                    || observed[rank] != expected_cpu[rank]
                {
                    out.valid = false;
                }
                if (*q).ranks[rank].first != first[rank].cast()
                    || (*q).ranks[rank].count != observed[rank]
                {
                    out.valid = false;
                }
            }
            if count != (*q).count || previous != (*q).tail {
                out.valid = false;
            }
        }
        let ready_total = (*r).ready.iter().map(|q| q.count).sum::<usize>();
        if out.ready != ready_total || out.ready != expected_ready {
            out.valid = false;
        }
        previous = null_mut();
        cursor = (*r).timeout_head;
        while !cursor.is_null() {
            let c = registry::core(cursor);
            out.timed += 1;
            if out.timed > (*r).timeout_count {
                out.valid = false;
                break;
            }
            if !(*c).timeout_linked
                || (*c).state != State::Blocked
                || (*c).wake_tick == 0
                || (*c).timeout_prev != previous
            {
                out.valid = false;
            }
            if !previous.is_null() {
                let before = registry::core(previous);
                if (*before).wake_tick > (*c).wake_tick
                    || ((*before).wake_tick == (*c).wake_tick
                        && (*before).timeout_sequence >= (*c).timeout_sequence)
                {
                    out.valid = false;
                }
            }
            previous = cursor;
            cursor = (*c).timeout_next;
        }
        if out.timed != (*r).timeout_count
            || out.timed != expected_timed
            || previous != (*r).timeout_tail
        {
            out.valid = false;
        }
        if (*r).timeout_index.first != (*r).timeout_head.cast()
            || (*r).timeout_index.count != (*r).timeout_count
        {
            out.valid = false;
        }
        previous = null_mut();
        cursor = (*r).reap_head;
        while !cursor.is_null() {
            let c = registry::core(cursor);
            out.reap += 1;
            if out.reap > (*r).reap_count {
                out.valid = false;
                break;
            }
            if !(*c).reap_linked
                || !(*c).retire_pending
                || (*c).release_in_progress
                || (*c).reap_prev != previous
            {
                out.valid = false;
            }
            previous = cursor;
            cursor = (*c).reap_next;
        }
        if out.reap != (*r).reap_count || out.reap != expected_reap || previous != (*r).reap_tail {
            out.valid = false;
        }
        out
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DeadmanTask {
    pub id: u32,
    pub held_lock_count: u32,
    pub unwind_guard_count: u32,
    pub blocked_for_ticks: u64,
    pub wait_object: u64,
    pub suspicious_mutex_holder: bool,
    pub name_len: u8,
    pub reason_len: u8,
    pub name: [u8; 20],
    pub reason: [u8; 20],
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Deadman {
    pub captured_tick: u64,
    pub wedged_mutex_holder: bool,
    pub blocked_total: u32,
    pub omitted: u32,
    pub count: usize,
    pub tasks: [DeadmanTask; 6],
}
unsafe fn blocked(task: *mut Task, now: u64) -> Option<u64> {
    unsafe {
        let c = registry::core(task);
        if (*c).state == State::Blocked {
            Some(now.saturating_sub((*c).blocked_since_tick))
        } else {
            None
        }
    }
}
unsafe fn copy_text(target: &mut [u8; 20], source: super::task_types::Bytes) -> u8 {
    let len = source.len.min(20);
    if len != 0 {
        unsafe { ptr::copy_nonoverlapping(source.pointer, target.as_mut_ptr(), len) }
    }
    len as u8
}
unsafe fn append(out: &mut Deadman, task: *mut Task, ticks: u64, suspicious: bool) {
    if out.count >= out.tasks.len() {
        return;
    }
    unsafe {
        let c = registry::core(task);
        let entry = &mut out.tasks[out.count];
        *entry = DeadmanTask {
            id: (*c).id,
            held_lock_count: (*c).held_lock_count,
            unwind_guard_count: (*c).unwind_guard_count,
            blocked_for_ticks: ticks,
            wait_object: (*c).wait_object,
            suspicious_mutex_holder: suspicious,
            ..DeadmanTask::default()
        };
        entry.name_len = copy_text(&mut entry.name, (*c).name);
        let reason = if (*c).wait_reason.len != 0 {
            (*c).wait_reason
        } else {
            super::task_types::Bytes {
                pointer: b"unknown".as_ptr(),
                len: 7,
            }
        };
        entry.reason_len = copy_text(&mut entry.reason, reason);
        out.count += 1;
    }
}
pub(crate) unsafe fn deadman(now: u64, threshold: u64) -> Deadman {
    let flags = runtime::save_runtime();
    let mut out = Deadman {
        captured_tick: now,
        ..Deadman::default()
    };
    unsafe {
        let mut cursor = (*registry::raw()).head;
        while !cursor.is_null() {
            let task = cursor;
            let c = registry::core(task);
            cursor = (*c).registry_next;
            let Some(ticks) = blocked(task, now) else {
                continue;
            };
            out.blocked_total = out.blocked_total.saturating_add(1);
            if (*c).held_lock_count == 0 || ticks <= threshold {
                continue;
            }
            out.wedged_mutex_holder = true;
            append(&mut out, task, ticks, true);
        }
        cursor = (*registry::raw()).head;
        while !cursor.is_null() {
            if out.count >= out.tasks.len() {
                break;
            }
            let task = cursor;
            let c = registry::core(task);
            cursor = (*c).registry_next;
            let Some(ticks) = blocked(task, now) else {
                continue;
            };
            if (*c).held_lock_count != 0 && ticks > threshold {
                continue;
            }
            append(&mut out, task, ticks, false);
        }
        out.omitted = out.blocked_total.saturating_sub(out.count as u32);
    }
    runtime::restore_runtime(flags);
    out
}
const _: () = {
    assert!(core::mem::size_of::<Summary>() == 28);
    assert!(core::mem::size_of::<InventoryRow>() == 48);
    assert!(core::mem::size_of::<InventoryPage>() == 24);
    assert!(core::mem::size_of::<QueueSnapshot>() == 40);
    assert!(core::mem::size_of::<DeadmanTask>() == 80);
    assert!(core::mem::size_of::<Deadman>() == 512);
};

/// Copy one original ordinal performance row and both resident labels under
/// the existing runtime owner. No native reference reaches a caller write.
pub(crate) unsafe fn performance(
    index: u32,
) -> Option<crate::contract::ProgramTaskPerformanceInfo> {
    unsafe {
        let task = pin_ordinal(index as usize);
        if task.is_null() {
            return None;
        }
        let result = {
            let _owner = runtime::RuntimeGuard::acquire();
            let c = registry::core(task);
            let now = crate::platform::timer::tick_count();
            let elapsed = |tick: u64| {
                if tick != 0 && now >= tick {
                    now - tick
                } else {
                    0
                }
            };
            let blocked_ticks = if (*c).state == State::Blocked {
                elapsed((*c).blocked_since_tick)
            } else {
                0
            };
            let ticks_since_yield = elapsed((*c).last_yield_tick);
            let ticks_since_scheduled = elapsed((*c).last_scheduled_tick);
            let mut out = crate::contract::ProgramTaskPerformanceInfo {
                index,
                id: (*c).id,
                state: (*c).state as u32,
                flags: u32::from((*c).entry.is_some()),
                wake_tick: (*c).wake_tick,
                blocked_since_tick: (*c).blocked_since_tick,
                blocked_ticks,
                created_tick: (*c).created_tick,
                last_scheduled_tick: (*c).last_scheduled_tick,
                last_yield_tick: (*c).last_yield_tick,
                ticks_since_yield,
                run_ticks: (*c).run_ticks,
                switches_in: (*c).switches_in,
                stack_base: (*c).stack_base,
                stack_top: (*c).stack_top,
                stack_bytes: (*c).stack_top.saturating_sub((*c).stack_base),
                blocked_object: (*c).wait_object,
                wait_result: (*c).wait_result as u32,
                preempt_disable_depth: (*c).preempt_disable_depth,
                preempt_disable_max_depth: (*c).preempt_disable_max_depth,
                ticks_since_scheduled,
                preemption_probe_hits: (*c).preemption_probe_hits,
                preemption_deferred_ticks: (*c).preemption_deferred_ticks,
                long_run_warnings: (*c).long_run_warnings,
                starvation_warnings: (*c).starvation_warnings,
                ready_since_tick: (*c).ready_since_tick,
                last_ready_latency_ticks: (*c).last_ready_latency_ticks,
                max_ready_latency_ticks: (*c).max_ready_latency_ticks,
                last_wait_ticks: (*c).last_wait_ticks,
                max_wait_ticks: (*c).max_wait_ticks,
                max_run_without_switch_ticks: (*c).max_run_without_switch_ticks,
                max_preemption_deferred_ticks: (*c).max_preemption_deferred_ticks,
                ..Default::default()
            };
            for (destination, text) in [
                (&mut out.name[..], (*c).name),
                (&mut out.wait_reason[..], (*c).wait_reason),
            ] {
                let len = text.len.min(destination.len() - 1);
                if len != 0 {
                    destination[..len]
                        .copy_from_slice(core::slice::from_raw_parts(text.pointer, len));
                }
            }
            out
        };
        let _ = super::task_lifecycle::unpin(task);
        Some(result)
    }
}
