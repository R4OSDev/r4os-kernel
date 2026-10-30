//! Exactly one waiting child and its still-running donor. Reuse the admitted
//! live case's event/FPU/unwind worker and real task-retirement receipts.
use super::*;
use crate::sched::task::balance;

unsafe fn ready_count(cpu: u32) -> usize {
    unsafe { (*registry::raw()).ready[cpu as usize].count }
}

// No foreign queue is changed to manufacture an idle CPU. Retry while ordinary
// work occupies the source queue or all other CPUs; every attempt is bounded.
#[derive(Default)]
struct Capacity {
    queued: u64,
    no_idle: u64,
    no_core: u64,
    source_ready: u64,
    idle: u64,
    busy: u64,
    free_core: u64,
    parent: u32,
    ids: [u32; 3],
    roles: [u8; 3],
    names: [[u8; 32]; 3],
    lengths: [usize; 3],
}
impl Capacity {
    fn log(&self) {
        helpers::puts(b"TASKREG08525 single-capacity");
        for (label, value) in [
            (b" queued=".as_slice(), self.queued),
            (b" no_idle=".as_slice(), self.no_idle),
            (b" no_core=".as_slice(), self.no_core),
            (b" source_ready=".as_slice(), self.source_ready),
            (b" idle=".as_slice(), self.idle),
            (b" busy=".as_slice(), self.busy),
            (b" free_core=".as_slice(), self.free_core),
            (b" parent=".as_slice(), u64::from(self.parent)),
        ] {
            helpers::puts(label);
            helpers::dec(value);
        }
        helpers::puts(b"\r\n");
        for i in 0..self.ids.len() {
            if self.ids[i] == 0 {
                continue;
            }
            helpers::puts(b"TASKREG08525 source-ready id=");
            helpers::dec(u64::from(self.ids[i]));
            helpers::puts(b" role=");
            helpers::dec(u64::from(self.roles[i]));
            helpers::puts(b" name=");
            helpers::puts(&self.names[i][..self.lengths[i]]);
            helpers::puts(b"\r\n");
        }
    }
}
fn release(source: u32, capacity: &mut Capacity) -> Result<Option<(u32, u64)>, ()> {
    let _runtime = RuntimeGuard::acquire();
    unsafe {
        let parent = scheduler::current().cast::<Task>();
        if parent.is_null() || percpu::current_index() != source {
            return Err(());
        }
        let parent_core = registry::core(parent);
        if (*parent_core).state != State::Running || (*parent_core).cpu_idle {
            return Err(());
        }
        capacity.source_ready = ready_count(source) as u64;
        if capacity.source_ready != 0 {
            if capacity.queued == 0 {
                capacity.parent = (*parent_core).id;
                let mut task = (*registry::raw()).ready[source as usize].head;
                for i in 0..capacity.ids.len() {
                    if task.is_null() {
                        break;
                    }
                    let core = registry::core(task);
                    capacity.ids[i] = (*core).id;
                    capacity.roles[i] = (*core).role as u8;
                    let name = (*core).name;
                    let len = name.len.min(capacity.names[i].len());
                    capacity.names[i][..len]
                        .copy_from_slice(core::slice::from_raw_parts(name.pointer, len));
                    capacity.lengths[i] = len;
                    task = (*core).ready_next;
                }
            }
            capacity.queued += 1;
            return Ok(None);
        }
        let online = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
        let (mut idle_mask, mut busy) = (0u64, 0u64);
        for cpu in 0..registry::MAX_CPUS as u32 {
            if online & (1 << cpu) == 0 {
                continue;
            }
            let current = scheduler::current_on_cpu(cpu).cast::<Task>();
            if balance::workload(cpu, current) != 0 {
                busy |= 1 << cpu;
            }
            if cpu != source
                && ready_count(cpu) == 0
                && !current.is_null()
                && (*registry::core(current)).cpu_idle
            {
                idle_mask |= 1 << cpu;
            }
        }
        capacity.idle = idle_mask;
        capacity.busy = busy;
        if idle_mask == 0 {
            capacity.no_idle += 1;
            return Ok(None);
        }
        let domains = crate::platform::cpu_topology::domains();
        let free_core = domains.iter().enumerate().fold(0u64, |mask, (cpu, d)| {
            mask | if idle_mask & (1 << cpu) != 0 && d.core != 0 && d.core & busy == 0 {
                1 << cpu
            } else {
                0
            }
        });
        // Where another known core exists, observe its actual idle boundary;
        // do not clear foreign queues to manufacture the SMT preference case.
        let source_core = domains[source as usize].core;
        let other_known_core = domains
            .iter()
            .enumerate()
            .any(|(cpu, d)| online & (1 << cpu) != 0 && d.core != 0 && d.core & source_core == 0);
        capacity.free_core = free_core;
        if source_core != 0 && other_known_core && free_core == 0 {
            capacity.no_core += 1;
            return Ok(None);
        }
        let target = idle_mask.trailing_zeros();
        let idle = scheduler::current_on_cpu(target).cast::<Task>();
        let t = TASKS[0].load(Ordering::Acquire);
        let c = registry::core(t);
        let parent_before = facts(parent);
        let next_order = (*registry::raw()).next_ready_order;
        sync_objects::r4os_rust_sync_event(event(0), 0, 0);
        let before = facts(t);
        if !(*c).cpu_affinity_bound
            || (*c).state != State::Ready
            || !(*c).ready_linked
            || (*c).running_cpu != 0xff
            || u32::from((*c).home_cpu) != source
            || ready_count(source) != 1
            || balance::workload(source, parent) != 2
            || balance::workload(target, idle) != 0
            || !balance::pull(source, target, parent, idle, timer::tick_count, 1).is_null()
            || facts(t) != before
            || facts(parent) != parent_before
        {
            return Err(());
        }
        // Only this owned diagnostic child loses its temporary pin. Exercise
        // the production Ready notification, including pending and exact IPI.
        (*c).cpu_affinity_bound = false;
        crate::sched::task::balance::note_ready_publication(t);
        scheduler::notify_ready(t.cast());
        let target = u32::from((*c).home_cpu);
        let target_core = domains[target as usize].core;
        if idle_mask & (1 << target) == 0
            || (free_core != 0 && target_core != 0 && target_core & busy != 0)
        {
            return Err(());
        }
        if facts(t) != before
            || facts(parent) != parent_before
            || scheduler::current() != parent.cast()
            || scheduler::current_on_cpu(source) != parent.cast()
            || (*parent_core).state != State::Running
            || (*parent_core).running_cpu != source as u8
            || (*c).state != State::Ready
            || (*c).running_cpu != 0xff
            || !(*c).ready_linked
            || !(*c).home_cpu_bound
            || u32::from((*c).home_cpu) != target
            || (*c).last_migration_tick == 0
            || ready_count(source) != 0
            || ready_count(target) != 1
            || (*registry::raw()).next_ready_order != next_order + 1
            || !helpers::queue_valid()
        {
            return Err(());
        }
        Ok(Some((target, free_core)))
    }
}

pub(super) fn run() -> bool {
    if percpu::r4os_rust_percpu_control(15, 0, 0, 0) < 2 {
        helpers::puts(b"TASKREG0859 single-ready unavailable: requires two CPUs\r\n");
        return true;
    }
    if !prepare() {
        return false;
    }
    let source = percpu::current_index();
    let t = tasks::create_on_cpu(b"spectra-single-ready", worker, Role::Batch, source);
    if t.is_null() {
        return false;
    }
    TASKS[0].store(t, Ordering::Release);
    if !tasks::start(t) {
        finish(true);
        return false;
    }
    let end = timer::tick_count().saturating_add(2000);
    while !enrolled(1) {
        if kernel_check::cancelled()
            || FAILURES.load(Ordering::Acquire) != 0
            || timer::tick_count() >= end
        {
            finish(true);
            return false;
        }
        tasks::sleep(1, b"spectra-single-enrollment");
    }
    let mut capacity = Capacity::default();
    let (target, free_core) = loop {
        match release(source, &mut capacity) {
            Ok(Some(cpu)) => break cpu,
            Err(()) => {
                finish(true);
                return helpers::fail(b"single-ready-facts");
            }
            Ok(None) => {}
        }
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            capacity.log();
            return helpers::fail(b"single-ready-idle");
        }
        wait_capacity(b"spectra-single-capacity");
    };
    capacity.log();
    while DONE.load(Ordering::Acquire) != 1 {
        if kernel_check::cancelled() || timer::tick_count() >= end {
            finish(true);
            return false;
        }
        tasks::sleep(1, b"spectra-single-done");
    }
    let valid = FIRST[0].load(Ordering::Acquire) == source
        && LAST[0].load(Ordering::Acquire) == target
        && RUNS[0].load(Ordering::Acquire) == 1
        && FAILURES.load(Ordering::Acquire) == 0;
    let clean = finish(false) && unsafe { helpers::queue_valid() };
    if valid && clean {
        helpers::puts(b"TASKREG0859 single-ready PASS tasks=1 donor=running target=previously-idle pinned=excluded physical-current=preserved fpu=1 unwind=1 ready=preserved own=retired\r\n");
        helpers::puts(b"TASKREG08511 placement PASS target=");
        helpers::dec(u64::from(target));
        helpers::puts(b" free-core-mask=");
        helpers::dec(free_core);
        helpers::puts(b" pinned=preserved\r\n");
    }
    valid && clean
}
