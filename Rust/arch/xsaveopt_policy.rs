// Pure optional standard-save qualification and exact kernel image identity.
#[derive(Clone, Copy, Default)]
pub struct Tag {
    pub id: u32,
    pub generation: u64,
    pub pointer: usize,
    pub valid: bool,
}
impl Tag {
    pub fn matches(self, id: u32, generation: u64, pointer: usize) -> bool {
        self.valid
            && id != 0
            && generation != 0
            && pointer != 0
            && self.id == id
            && self.generation == generation
            && self.pointer == pointer
    }
}
pub fn expected(count: u32) -> u64 {
    if count >= 64 {
        u64::MAX
    } else {
        (1u64 << count.max(1)) - 1
    }
}
pub fn eligible(count: u32, known: u64, supported: u64, enabled: bool, standard: bool) -> bool {
    if count > 64 {
        return false;
    }
    let mask = expected(count);
    enabled && standard && known & mask == mask && supported & mask == mask
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_unknown_or_unsupported_cpu() {
        assert!(eligible(4, 15, 15, true, true));
        for (k, s) in [(7, 7), (15, 7), (0, 0), (15, 0)] {
            assert!(!eligible(4, k, s, true, true));
        }
        assert!(!eligible(4, 15, 15, false, true));
        assert!(!eligible(4, 15, 15, true, false));
    }
    #[test]
    fn full_capacity_and_single() {
        assert_eq!(expected(64), u64::MAX);
        assert!(eligible(64, u64::MAX, u64::MAX, true, true));
        assert!(!eligible(64, u64::MAX, u64::MAX >> 1, true, true));
        assert!(eligible(1, 1, 1, true, true));
        assert_eq!(expected(0), 1);
    }
    #[test]
    fn exact_task_generation_and_buffer() {
        let t = Tag {
            id: 3,
            generation: 9,
            pointer: 64,
            valid: true,
        };
        assert!(t.matches(3, 9, 64));
        for (id, g, p) in [
            (4, 9, 64),
            (3, 10, 64),
            (3, 9, 128),
            (0, 9, 64),
            (3, 0, 64),
            (3, 9, 0),
        ] {
            assert!(!t.matches(id, g, p));
        }
        assert!(!Tag { valid: false, ..t }.matches(3, 9, 64));
    }
}
