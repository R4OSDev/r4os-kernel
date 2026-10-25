//! The actual chooser with independent FIFO/rank head fixtures. Reused by
//! the original explicit role check and the standalone pinned Rust host run.
use super::{Fairness, Order};
#[derive(Clone, Copy)]
struct Queue {
    entries: [(u32, u8); 3],
    len: usize,
}
impl Queue {
    const MIXED: Self = Self {
        entries: [(1, 3), (2, 3), (100, 0)],
        len: 3,
    };
    fn head(&self, order: Order) -> Option<u32> {
        self.entries[..self.len]
            .iter()
            .find(|(_, rank)| match order {
                Order::Fifo => true,
                Order::Rank(wanted) => *rank == wanted,
            })
            .map(|(id, _)| *id)
    }
    fn take(&mut self, id: u32) -> Option<(u32, u8)> {
        let index = self.entries[..self.len]
            .iter()
            .position(|(candidate, _)| *candidate == id)?;
        let task = self.entries[index];
        for i in index + 1..self.len {
            self.entries[i - 1] = self.entries[i];
        }
        self.len -= 1;
        Some(task)
    }
    fn step(&mut self, policy: &mut Fairness, urgent: bool, recycle: bool) -> Option<(u32, bool)> {
        let pick = policy.choose(urgent, |order| self.head(order));
        *policy = pick.fairness;
        let id = pick.task?;
        let task = self.take(id)?;
        if recycle {
            self.entries[self.len] = task;
            self.len += 1;
        }
        Some((id, pick.fifo))
    }
}
fn phases() -> bool {
    for prefix in 0..8 {
        let (mut local, mut foreign) = (Fairness::EMPTY, Fairness::EMPTY);
        let (mut queue, mut other_queue) = (Queue::MIXED, Queue::MIXED);
        for _ in 0..prefix {
            if queue.step(&mut local, false, true) != Some((100, false)) {
                return false;
            }
        }
        // Foreign CPU work cannot advance the local phase, including when
        // that CPU itself consumes its own FIFO opportunities.
        for index in 0..24 + prefix * 3 {
            if other_queue
                .step(&mut foreign, index % 3 == 0, true)
                .is_none()
            {
                return false;
            }
        }
        for index in 0..4 + prefix {
            let mut visits = 0;
            let empty = local.choose(index % 2 == 0, |_| {
                visits += 1;
                None::<u32>
            });
            if empty.task.is_some() || visits == 0 || visits > 4 {
                return false;
            }
            local = empty.fairness;
        }
        // Especially prefix=7: arbitrary urgent picks retain the due FIFO
        // debt. They still choose the real highest-rank head themselves.
        for _ in 0..9 + prefix {
            if queue.step(&mut local, true, true) != Some((100, false)) {
                return false;
            }
        }
        for step in prefix + 1..=8 {
            let expected = if step == 8 { (1, true) } else { (100, false) };
            if queue.step(&mut local, false, true) != Some(expected) {
                return false;
            }
        }
    }
    true
}
fn progress() -> bool {
    let mut queue = Queue::MIXED;
    let mut policy = Fairness::EMPTY;
    // Two older Batch tasks remain queued while Input work is replenished
    // after every dispatch. Each exact old task must get its FIFO turn.
    for step in 1..=32 {
        let expected = match step {
            8 | 24 => (1, true),
            16 | 32 => (2, true),
            _ => (100, false),
        };
        if queue.step(&mut policy, false, true) != Some(expected) {
            return false;
        }
    }
    true
}
fn ranks() -> bool {
    let mut queue = Queue {
        entries: [(20, 2), (10, 1), (11, 1)],
        len: 3,
    };
    let mut policy = Fairness::EMPTY;
    for expected in [10, 11, 20] {
        if queue.step(&mut policy, false, false) != Some((expected, false)) {
            return false;
        }
    }
    queue.step(&mut policy, false, false).is_none()
}
#[cold]
pub fn run() -> bool {
    phases() && progress() && ranks()
}
#[cfg(test)]
mod tests {
    #[test]
    fn every_phase_survives_foreign_empty_and_urgent_picks() {
        assert!(super::phases());
    }
    #[test]
    fn old_batch_tasks_progress_with_replenished_input() {
        assert!(super::progress());
    }
    #[test]
    fn best_rank_and_same_rank_fifo_remain() {
        assert!(super::ranks());
    }
}
