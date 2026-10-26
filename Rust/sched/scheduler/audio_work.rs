//! Audio Work publishes copied metadata under its existing owner. Scheduler
//! consumes it under Runtime; neither side borrows the other's mutable state.
#[cfg(test)]
#[path = "audio_work/host_tests.rs"]
mod host_tests;
use core::sync::atomic::{AtomicU64, Ordering, fence};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Identity {
    pub id: u32,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Hint {
    pub worker: Identity,
    pub ticket: u32,
    pub owner: u32,
    pub serial_key: u64,
    pub deadline: u64,
    pub budget: u64,
    pub running: bool,
    pub started: u64,
}
impl Hint {
    pub(crate) const EMPTY: Self = Self {
        worker: Identity {
            id: 0,
            generation: 0,
        },
        ticket: 0,
        owner: 0,
        serial_key: 0,
        deadline: 0,
        budget: 0,
        running: false,
        started: 0,
    };
    fn eligible(self, now: u64) -> bool {
        self.worker.id != 0 && self.worker.generation != 0 && self.ticket != 0
            && self.serial_key != 0 && self.deadline != 0 && self.budget != 0
            && now >= self.deadline.saturating_sub(self.budget)
            // The producer's existing callback budget includes time spent
            // waiting. Starting, blocking or adopting a hint cannot reset it.
            && (!self.running || now.saturating_sub(self.started) < self.budget)
    }
}

pub(crate) struct Mailbox {
    sequence: AtomicU64,
    words: [AtomicU64; 7],
}
pub(crate) enum Update {
    Unchanged,
    Busy,
    Changed(u64, Hint),
}
impl Mailbox {
    pub(crate) const fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            words: [const { AtomicU64::new(0) }; 7],
        }
    }
    /// # Safety
    /// The existing Work owner serializes writers for the complete publication.
    /// No Runtime entry, allocation, callback or wait occurs here, including IRQ.
    pub(crate) unsafe fn publish(&self, hint: Hint) {
        self.sequence.fetch_add(1, Ordering::AcqRel);
        let words = [
            u64::from(hint.worker.id) | (u64::from(hint.ticket) << 32),
            hint.worker.generation,
            u64::from(hint.owner) << 1 | u64::from(hint.running),
            hint.serial_key,
            hint.deadline,
            hint.budget,
            hint.started,
        ];
        for (slot, value) in self.words.iter().zip(words) {
            slot.store(value, Ordering::Relaxed);
        }
        self.sequence.fetch_add(1, Ordering::Release);
    }
    pub(crate) fn read_after(&self, seen: u64) -> Update {
        let sequence = self.sequence.load(Ordering::Acquire);
        if sequence & 1 != 0 {
            return Update::Busy;
        }
        if sequence == seen {
            return Update::Unchanged;
        }
        let words = self.words.each_ref().map(|v| v.load(Ordering::Relaxed));
        fence(Ordering::Acquire);
        if self.sequence.load(Ordering::Relaxed) != sequence {
            return Update::Busy;
        }
        Update::Changed(
            sequence,
            Hint {
                worker: Identity {
                    id: words[0] as u32,
                    generation: words[1],
                },
                ticket: (words[0] >> 32) as u32,
                owner: (words[2] >> 1) as u32,
                running: words[2] & 1 != 0,
                serial_key: words[3],
                deadline: words[4],
                budget: words[5],
                started: words[6],
            },
        )
    }
    pub(crate) fn still_current(&self, sequence: u64) -> bool {
        self.sequence.load(Ordering::Acquire) == sequence
    }
}
pub(crate) static MAILBOX: Mailbox = Mailbox::new();

#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub seen: u64,
    pub hint: Hint,
    until: u64,
    next_grant: u64,
    owed: bool,
}
impl Policy {
    pub(crate) const EMPTY: Self = Self {
        seen: 0,
        hint: Hint::EMPTY,
        until: 0,
        next_grant: 0,
        owed: false,
    };
    pub(crate) fn adopt(&mut self, sequence: u64, hint: Hint) {
        if self.hint.worker != hint.worker {
            *self = Self::EMPTY;
        }
        self.seen = sequence;
        self.hint = hint;
        // Ticket changes, cancellation and an empty queue do not replenish a
        // worker's reservation. Only a genuinely different task identity does.
    }
    pub(crate) fn eligible(self, now: u64) -> bool {
        self.hint.eligible(now) && (now < self.until || now >= self.next_grant)
    }
    pub(crate) fn protected(self, now: u64) -> bool {
        self.hint.eligible(now) && now < self.until
    }
    pub(crate) fn giveback(self, now: u64) -> bool {
        self.owed && (!self.hint.eligible(now) || now >= self.until)
    }
    pub(crate) fn selected(&mut self, audio: bool, now: u64, burst: u64, period: u64) {
        if !audio {
            self.owed = false;
            return;
        }
        if !self.eligible(now) {
            return;
        }
        if now >= self.until {
            self.until = now.saturating_add(burst.min(self.hint.budget));
            self.next_grant = now.saturating_add(period);
        }
        self.owed = true;
    }
}
