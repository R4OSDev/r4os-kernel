//! Negative source hints only. A partial donor prefix or a clock-dependent
//! rejection must remain searchable; no candidate, task pointer or wake credit
//! is owned by this cache. The scheduler supplies one Runtime-owned key.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    pub registry: u64,
    pub ready: u64,
    pub online: u64,
    pub cpu_epoch: u64,
    pub current: usize,
    pub target_load: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Cache {
    key: Option<Key>,
    empty: u64,
}

impl Cache {
    pub const EMPTY: Self = Self {
        key: None,
        empty: 0,
    };

    pub fn begin(&mut self, key: Key) {
        if self.key != Some(key) {
            self.key = Some(key);
            self.empty = 0;
        }
    }

    pub fn skip(&self, source: u32) -> bool {
        self.empty & (1u64 << source) != 0
    }

    pub fn rejected(&mut self, source: u32, complete: bool, clock_used: bool, stable: bool) {
        if complete && !clock_used && stable {
            self.empty |= 1u64 << source;
        }
    }

    pub fn all_empty(&self, sources: u64) -> bool {
        self.empty & sources == sources
    }
}

/// Consecutive visits through a donor's shared bounded cursor. Every target
/// contributes to the same traversal. No early partial prefix can prove that
/// the whole donor excludes a CPU. Queue/policy/membership changes reset it.
pub(crate) struct Summary {
    key: Option<(u64, u64, u64)>,
    visited: usize,
    possible: u64,
    complete: Option<u64>,
}

impl Summary {
    pub const EMPTY: Self = Self {
        key: None,
        visited: 0,
        possible: 0,
        complete: None,
    };

    pub fn begin(&mut self, registry: u64, ready: u64, online: u64) {
        let key = (registry, ready, online);
        if self.key != Some(key) {
            self.key = Some(key);
            self.visited = 0;
            self.possible = 0;
            self.complete = None;
        }
    }

    pub fn visit(&mut self, possible: u64, count: usize) {
        self.possible |= possible;
        self.visited += 1;
        if self.visited >= count {
            self.complete = Some(self.possible);
            self.visited = 0;
            self.possible = 0;
        }
    }

    pub fn excludes(&self, target: u32) -> bool {
        self.complete
            .is_some_and(|mask| mask & (1u64 << target) == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: Key = Key {
        registry: 11,
        ready: 29,
        online: 15,
        cpu_epoch: 3,
        current: 0x1000,
        target_load: 0,
    };

    #[test]
    fn transient_saved_stack_or_fpu_handoff_is_never_cached() {
        let mut cache = Cache::EMPTY;
        cache.begin(KEY);
        cache.rejected(1, true, false, false);
        assert!(!cache.skip(1));
        assert!(!cache.all_empty(2));
        cache.rejected(1, true, false, true);
        assert!(cache.skip(1));
    }

    #[test]
    fn unchanged_empty_sources_skip_but_new_ready_publication_retries() {
        let mut cache = Cache::EMPTY;
        cache.begin(KEY);
        cache.rejected(0, true, false, true);
        cache.rejected(3, true, false, true);
        for _ in 0..8 {
            cache.begin(KEY);
            assert!(cache.skip(0) && cache.skip(3));
        }
        cache.begin(Key { ready: 30, ..KEY });
        assert!(!cache.skip(0) && !cache.skip(3));
    }

    #[test]
    fn each_ownership_membership_and_target_boundary_invalidates_hints() {
        for changed in [
            Key {
                registry: 12,
                ..KEY
            },
            Key { online: 7, ..KEY },
            Key {
                cpu_epoch: 4,
                ..KEY
            },
            Key {
                current: 0x2000,
                ..KEY
            },
            Key {
                target_load: 1,
                ..KEY
            },
        ] {
            let mut cache = Cache::EMPTY;
            cache.begin(KEY);
            cache.rejected(0, true, false, true);
            cache.begin(changed);
            assert!(!cache.skip(0));
        }
    }

    #[test]
    fn partial_prefix_cannot_hide_a_ninth_candidate() {
        let mut cache = Cache::EMPTY;
        cache.begin(KEY);
        for _ in 0..8 {
            cache.rejected(0, false, false, true);
            assert!(!cache.skip(0));
        }
    }

    #[test]
    fn cooldown_rejection_retries_without_a_ready_revision_change() {
        let mut cache = Cache::EMPTY;
        cache.begin(KEY);
        cache.rejected(0, true, true, true);
        cache.begin(KEY);
        assert!(!cache.skip(0));
    }

    #[test]
    fn independent_sources_and_last_cpu_bit_remain_exact() {
        let mut cache = Cache::EMPTY;
        cache.begin(KEY);
        cache.rejected(63, true, false, true);
        assert!(cache.skip(63));
        assert!(!cache.skip(0));
    }

    #[test]
    fn shared_bounded_visits_cannot_hide_a_ninth_cpu_allowed_candidate() {
        let mut summary = Summary::EMPTY;
        summary.begin(1, 2, 15);
        for _ in 0..8 {
            summary.visit(0, 9);
        }
        assert!(!summary.excludes(1) && !summary.excludes(2));
        summary.visit(1 << 2, 9);
        assert!(summary.excludes(1));
        assert!(!summary.excludes(2));
        summary.begin(1, 3, 15);
        assert!(!summary.excludes(1));
    }

    #[test]
    fn many_fixed_tasks_converge_without_a_longer_pull_or_per_target_scan() {
        let mut summary = Summary::EMPTY;
        summary.begin(1, 2, 15);
        // Eight separate bounded visits, potentially from different targets.
        for _ in 0..8 {
            for _ in 0..8 {
                summary.visit(0, 64);
            }
        }
        assert!(summary.excludes(0) && summary.excludes(63));
        summary.begin(2, 2, 15);
        assert!(!summary.excludes(0));
    }
}
