//! Pure bounded stream accounting. The resident IPC owner serializes these
//! transitions with copied bytes and exact endpoint references.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ring {
    pub head: usize,
    pub len: usize,
    pub readers: u32,
    pub writers: u32,
}
impl Ring {
    pub const fn new() -> Self {
        Self {
            head: 0,
            len: 0,
            readers: 1,
            writers: 1,
        }
    }
    pub fn write_count(&self, capacity: usize, length: usize, step: usize) -> usize {
        assert!(self.len <= capacity && capacity != 0 && step != 0);
        if self.readers == 0 {
            return 0;
        }
        let space = capacity - self.len;
        if length <= capacity.min(step) && length > space {
            return 0;
        }
        length.min(step).min(space)
    }
    pub fn read_count(&self, length: usize, step: usize) -> usize {
        length.min(step).min(self.len)
    }
    pub fn eof(&self) -> bool {
        self.writers == 0 && self.len == 0
    }
    pub fn written(&mut self, capacity: usize, count: usize) {
        assert!(count <= capacity - self.len && self.readers != 0);
        self.len += count;
    }
    pub fn read(&mut self, capacity: usize, count: usize) {
        assert!(count <= self.len);
        self.head = (self.head + count) % capacity;
        self.len -= count;
    }
    pub fn close(&mut self, reader: bool) {
        let count = if reader {
            &mut self.readers
        } else {
            &mut self.writers
        };
        assert!(*count != 0);
        *count -= 1;
        if self.readers == 0 {
            self.len = 0;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_small_writes_and_large_exact_prefixes() {
        let mut r = Ring::new();
        r.written(8192, 7000);
        assert_eq!(r.write_count(8192, 1200, 4096), 0);
        assert_eq!(r.write_count(8192, 9000, 4096), 1192);
        r.written(8192, 1192);
        assert_eq!(r.write_count(8192, 1, 4096), 0);
        r.read(8192, 4096);
        assert_eq!(r.write_count(8192, 4096, 4096), 4096);
    }
    #[test]
    fn last_writer_and_drain_are_separate() {
        let mut r = Ring::new();
        r.writers += 1;
        r.written(7, 7);
        r.close(false);
        assert!(!r.eof());
        r.close(false);
        assert!(!r.eof());
        assert_eq!(r.read_count(10, 4096), 7);
        r.read(7, 3);
        assert!(!r.eof());
        r.read(7, 4);
        assert!(r.eof());
        assert_eq!(r.head, 0);
    }
    #[test]
    fn duplicate_readers_retain_space_until_last_close() {
        let mut r = Ring::new();
        r.readers += 1;
        r.written(3, 2);
        r.close(true);
        assert_eq!(r.len, 2);
        r.close(true);
        assert_eq!(r.len, 0);
        assert_eq!(r.write_count(3, 1, 4096), 0);
    }
    #[test]
    fn every_small_capacity_wraps_without_overwrite() {
        for capacity in 1..129 {
            let mut r = Ring::new();
            for _ in 0..100 {
                assert_eq!(r.write_count(capacity, capacity, 4096), capacity);
                r.written(capacity, capacity);
                r.read(capacity, capacity - 1);
                assert_eq!(r.write_count(capacity, capacity, 4096), 0);
                r.read(capacity, 1);
                assert_eq!(r.len, 0);
            }
        }
    }
}
