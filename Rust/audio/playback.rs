//! Bounded PCM attribution, independent of hardware registers or task state.
//! A device cursor is meaningful only in its exact backend/output/reset epoch.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Epoch {
    pub backend: u64,
    pub output: u64,
    pub reset: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Position {
    pub epoch: Epoch,
    pub dma: u64,
    pub consumed: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Dma,
    Consumed,
    Unconfirmed,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SourceKey {
    pub slot: usize,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Contribution {
    pub key: SourceKey,
    pub first: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Span<const SOURCES: usize> {
    pub epoch: Epoch,
    pub first: u64,
    pub frames: u64,
    pub observed: u64,
    pub observed_dma: u64,
    pub tracking_valid: bool,
    pub sources: [Contribution; SOURCES],
    pub count: usize,
}

impl<const SOURCES: usize> Span<SOURCES> {
    pub const EMPTY: Self = Self {
        epoch: Epoch {
            backend: 0,
            output: 0,
            reset: 0,
        },
        first: 0,
        frames: 0,
        observed: 0,
        observed_dma: 0,
        tracking_valid: true,
        sources: [Contribution {
            key: SourceKey {
                slot: 0,
                generation: 0,
            },
            first: 0,
        }; SOURCES],
        count: 0,
    };
}

pub struct Ledger<const SOURCES: usize, const SPANS: usize> {
    spans: [Span<SOURCES>; SPANS],
    head: usize,
    len: usize,
}

impl<const SOURCES: usize, const SPANS: usize> Ledger<SOURCES, SPANS> {
    pub const EMPTY: Self = Self {
        spans: [Span::EMPTY; SPANS],
        head: 0,
        len: 0,
    };

    pub fn full(&self) -> bool {
        self.len == SPANS
    }
    pub fn empty(&self) -> bool {
        self.len == 0
    }

    pub fn push(&mut self, span: Span<SOURCES>) -> bool {
        if self.full()
            || SPANS == 0
            || span.frames == 0
            || span.count == 0
            || span.count > SOURCES
            || span.tracking_valid
                && (span.epoch.backend == 0 || span.epoch.output == 0 || span.epoch.reset == 0)
            || span.first.checked_add(span.frames).is_none()
            || span.sources[..span.count]
                .iter()
                .any(|c| c.key.generation == 0 || c.first.checked_add(span.frames).is_none())
        {
            return false;
        }
        self.spans[(self.head + self.len) % SPANS] = Span {
            observed: 0,
            observed_dma: 0,
            ..span
        };
        self.len += 1;
        true
    }

    /// `known` reports actual device consumption; `false` retires an
    /// unconfirmed tail after reset, never disguising it as played PCM.
    /// The owner checks each exact source generation before applying updates.
    pub fn observe(
        &mut self,
        current: Position,
        previous: Option<Position>,
        mut update: impl FnMut(SourceKey, u64, Stage),
    ) {
        while self.len != 0 {
            let span = &mut self.spans[self.head];
            let position = if !span.tracking_valid {
                None
            } else if span.epoch == current.epoch {
                Some(current)
            } else {
                previous.filter(|p| p.epoch == span.epoch)
            };
            let same_epoch = span.tracking_valid && span.epoch == current.epoch;
            let through_dma =
                position.map_or(0, |p| p.dma.saturating_sub(span.first).min(span.frames));
            if through_dma > span.observed_dma {
                for c in &span.sources[..span.count] {
                    update(c.key, c.first + through_dma, Stage::Dma);
                }
                span.observed_dma = through_dma;
            }
            let through = position.map_or(0, |p| {
                p.consumed.saturating_sub(span.first).min(span.frames)
            });
            if through > span.observed {
                for c in &span.sources[..span.count] {
                    update(c.key, c.first + through, Stage::Consumed);
                }
                span.observed = through;
            }
            if same_epoch && span.observed != span.frames {
                break;
            }
            if span.observed != span.frames {
                for c in &span.sources[..span.count] {
                    update(c.key, c.first + span.frames, Stage::Unconfirmed);
                }
            }
            self.spans[self.head] = Span::EMPTY;
            self.head = (self.head + 1) % SPANS;
            self.len -= 1;
        }
    }
}

#[derive(Clone, Copy)]
struct Stamp {
    end: u64,
    accepted_ns: u64,
}
impl Stamp {
    const EMPTY: Self = Self {
        end: 0,
        accepted_ns: 0,
    };
}

#[derive(Clone, Copy)]
pub struct Source<const STAMPS: usize> {
    pub generation: u64,
    pub accepted: u64,
    pub submitted: u64,
    pub dma: u64,
    pub consumed: u64,
    pub unconfirmed: u64,
    pub retired: u64,
    pub expected_due_ns: u64,
    pub continuous: bool,
    pub starvations: u64,
    starvation_latched: bool,
    dma_retired: u64,
    stamps: [Stamp; STAMPS],
    head: usize,
    len: usize,
}

impl<const STAMPS: usize> Source<STAMPS> {
    pub const fn new(generation: u64) -> Self {
        let mut value = Self::EMPTY;
        value.generation = generation;
        value
    }

    pub const EMPTY: Self = Self {
        generation: 0,
        accepted: 0,
        submitted: 0,
        dma: 0,
        consumed: 0,
        unconfirmed: 0,
        retired: 0,
        expected_due_ns: 0,
        continuous: false,
        starvations: 0,
        starvation_latched: false,
        dma_retired: 0,
        stamps: [Stamp::EMPTY; STAMPS],
        head: 0,
        len: 0,
    };

    pub fn can_accept(&self, now: u64) -> bool {
        STAMPS != 0
            && (self.len < STAMPS
                || self.stamps[(self.head + self.len - 1) % STAMPS].accepted_ns == now)
    }

    pub fn accept(&mut self, frames: u64, now: u64) -> bool {
        if frames == 0 || self.generation == 0 || !self.can_accept(now) {
            return false;
        }
        let Some(end) = self.accepted.checked_add(frames) else {
            return false;
        };
        if self.len != 0 && self.stamps[(self.head + self.len - 1) % STAMPS].accepted_ns == now {
            self.stamps[(self.head + self.len - 1) % STAMPS].end = end;
        } else {
            self.stamps[(self.head + self.len) % STAMPS] = Stamp {
                end,
                accepted_ns: now,
            };
            self.len += 1;
        }
        self.accepted = end;
        self.starvation_latched = false;
        true
    }

    pub fn submit(&mut self, frames: u64) -> Option<u64> {
        let end = self.submitted.checked_add(frames)?;
        if frames == 0 || end > self.accepted {
            return None;
        }
        let first = self.submitted;
        self.submitted = end;
        Some(first)
    }

    pub fn retire(&mut self, generation: u64, through: u64, known: bool) -> bool {
        if generation == 0
            || self.generation != generation
            || through < self.retired
            || through > self.submitted
            || known && through > self.dma_retired
        {
            return false;
        }
        let frames = through - self.retired;
        if known {
            self.consumed += frames;
        } else {
            self.unconfirmed += frames;
        }
        self.retired = through;
        self.dma_retired = self.dma_retired.max(through);
        while self.len != 0 && self.stamps[self.head].end <= through {
            self.stamps[self.head] = Stamp::EMPTY;
            self.head = (self.head + 1) % STAMPS;
            self.len -= 1;
        }
        true
    }

    pub fn observe_dma(&mut self, generation: u64, through: u64) -> bool {
        if generation == 0
            || generation != self.generation
            || through < self.dma_retired
            || through > self.submitted
        {
            return false;
        }
        self.dma += through - self.dma_retired;
        self.dma_retired = through;
        true
    }

    pub fn apply(&mut self, key: SourceKey, through: u64, stage: Stage) -> bool {
        match stage {
            Stage::Dma => self.observe_dma(key.generation, through),
            Stage::Consumed => self.retire(key.generation, through, true),
            Stage::Unconfirmed => self.retire(key.generation, through, false),
        }
    }

    pub fn queue_age_ns(&self, now: u64) -> u64 {
        if self.len == 0 {
            0
        } else {
            now.saturating_sub(self.stamps[self.head].accepted_ns)
        }
    }

    pub fn pending_deadline(&self) -> Option<u64> {
        (self.continuous && !self.starvation_latched && self.expected_due_ns != 0)
            .then_some(self.expected_due_ns)
    }

    pub fn expectation(&mut self, continuous: bool, due_ns: u64) -> bool {
        if continuous && due_ns == 0 {
            return false;
        }
        self.continuous = continuous;
        self.expected_due_ns = if continuous { due_ns } else { 0 };
        if !continuous {
            self.starvation_latched = false;
        }
        true
    }

    pub fn check_source(&mut self, now: u64) {
        let starved = self.continuous
            && self.expected_due_ns != 0
            && now > self.expected_due_ns
            && self.accepted == self.retired;
        if starved && !self.starvation_latched {
            self.starvations = self.starvations.saturating_add(1);
            self.starvation_latched = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(slot: usize, generation: u64) -> Contribution {
        Contribution {
            key: SourceKey { slot, generation },
            first: 0,
        }
    }
    #[test]
    fn mixed_partial_consumption_reset_and_reused_stream() {
        let epoch = Epoch {
            backend: 7,
            output: 3,
            reset: 11,
        };
        let mut ledger = Ledger::<2, 4>::EMPTY;
        let mut sources = [Source::<4>::EMPTY, Source::<4>::EMPTY];
        for (i, s) in sources.iter_mut().enumerate() {
            s.generation = 10 + i as u64;
            assert!(s.accept(480, 100));
            assert_eq!(s.submit(480), Some(0));
        }
        assert!(ledger.push(Span {
            epoch,
            first: 96,
            frames: 480,
            sources: [key(0, 10), key(1, 11)],
            count: 2,
            ..Span::EMPTY
        }));
        ledger.observe(
            Position {
                epoch,
                dma: 96 + 480,
                consumed: 96 + 120,
            },
            None,
            |key: SourceKey, through, stage| {
                sources[key.slot].apply(key, through, stage);
            },
        );
        assert_eq!((sources[0].consumed, sources[1].consumed), (120, 120));
        assert_eq!((sources[0].dma, sources[1].dma), (480, 480));
        sources[1] = Source::EMPTY;
        sources[1].generation = 99;
        let next = Epoch { reset: 12, ..epoch };
        ledger.observe(
            Position {
                epoch: next,
                dma: 0,
                consumed: 0,
            },
            Some(Position {
                epoch,
                dma: 96 + 480,
                consumed: 96 + 240,
            }),
            |key, through, stage| {
                sources[key.slot].apply(key, through, stage);
            },
        );
        assert!(ledger.empty());
        assert_eq!(
            (
                sources[0].consumed,
                sources[0].unconfirmed,
                sources[0].retired
            ),
            (240, 240, 480)
        );
        assert_eq!(
            (
                sources[1].consumed,
                sources[1].unconfirmed,
                sources[1].retired
            ),
            (0, 0, 0)
        );
    }

    #[test]
    fn queue_age_and_source_deadline_never_advance_from_repeated_reads() {
        let mut s = Source::<2>::EMPTY;
        s.generation = 1;
        assert!(s.accept(17, 100));
        assert!(s.accept(31, 200));
        assert!(!s.accept(1, 300));
        assert_eq!(s.submit(48), Some(0));
        assert!(s.observe_dma(1, 48));
        assert_eq!(s.queue_age_ns(350), 250);
        assert!(s.retire(1, 17, true));
        assert_eq!(s.queue_age_ns(350), 150);
        assert!(s.expectation(true, 400));
        s.check_source(500);
        assert_eq!(s.starvations, 0); // PCM still queued.
        assert!(s.retire(1, 48, true));
        s.check_source(500);
        assert_eq!(s.starvations, 1);
        for _ in 0..100 {
            s.check_source(1000);
        }
        assert_eq!(s.starvations, 1);
        assert!(s.accept(1, 1100));
        assert_eq!(s.submit(1), Some(48));
        assert!(s.observe_dma(1, 49));
        assert!(s.retire(1, 49, true));
        assert!(s.expectation(false, 0));
        s.check_source(2000);
        assert_eq!(s.starvations, 1);
        assert_eq!(s.queue_age_ns(2000), 0);
    }

    #[test]
    fn unavailable_tail_keeps_fifo_and_dma_does_not_count_the_skipped_gap() {
        let epoch = Epoch {
            backend: 7,
            output: 3,
            reset: 11,
        };
        let mut source = Source::<8>::EMPTY;
        source.generation = 1;
        assert!(source.accept(1440, 100));
        assert_eq!(source.submit(1440), Some(0));
        let mut ledger = Ledger::<1, 4>::EMPTY;
        assert!(ledger.push(Span {
            epoch,
            first: 1000,
            frames: 480,
            sources: [key(0, 1)],
            count: 1,
            ..Span::EMPTY
        }));
        assert!(ledger.push(Span {
            frames: 480,
            sources: [Contribution {
                first: 480,
                ..key(0, 1)
            }],
            count: 1,
            tracking_valid: false,
            ..Span::EMPTY
        }));
        ledger.observe(
            Position {
                epoch,
                dma: 1240,
                consumed: 1120,
            },
            None,
            |k, n, stage| {
                assert!(source.apply(k, n, stage));
            },
        );
        assert_eq!(
            (source.dma, source.consumed, source.unconfirmed),
            (240, 120, 0)
        );
        assert!(!ledger.empty());
        ledger.observe(
            Position {
                epoch,
                dma: 1480,
                consumed: 1480,
            },
            None,
            |k, n, stage| {
                assert!(source.apply(k, n, stage));
            },
        );
        assert_eq!(
            (source.dma, source.consumed, source.unconfirmed),
            (480, 480, 480)
        );
        assert!(ledger.empty());
        let next = Epoch { reset: 12, ..epoch };
        assert!(ledger.push(Span {
            epoch: next,
            frames: 480,
            sources: [Contribution {
                first: 960,
                ..key(0, 1)
            }],
            count: 1,
            ..Span::EMPTY
        }));
        ledger.observe(
            Position {
                epoch: next,
                dma: 480,
                consumed: 240,
            },
            None,
            |k, n, stage| {
                assert!(source.apply(k, n, stage));
            },
        );
        assert_eq!(
            (
                source.dma,
                source.consumed,
                source.unconfirmed,
                source.retired
            ),
            (960, 720, 480, 1200)
        );
        assert_eq!(source.accepted - source.retired, 240);
    }
}
