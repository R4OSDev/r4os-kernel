//! Loader snapshots and reader-private detail totals. No I/O or runtime owner.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub active_buffers: u64,
    pub reserved_bytes: u64,
    pub committed_bytes: u64,
    pub peak_reserved_bytes: u64,
    pub peak_committed_bytes: u64,
    pub full_reads: u64,
    pub range_reads: u64,
    pub range_read_bytes: u64,
    pub metadata_reader_initializations: u64,
    pub metadata_logical_reads: u64,
    pub metadata_logical_bytes: u64,
    pub metadata_window_hits: u64,
    pub metadata_window_fills: u64,
    pub metadata_window_fill_bytes: u64,
    pub metadata_direct_reads: u64,
    pub metadata_direct_bytes: u64,
    pub reserve_failures: u64,
    pub commit_failures: u64,
    pub read_failures: u64,
    pub short_reads: u64,
    pub release_failures: u64,
    pub pressure_reclaim_attempts: u64,
    pub pressure_reclaimed_frames: u64,
    pub pressure_failures: u64,
}
impl Stats {
    pub(crate) const EMPTY: Self = Self {
        active_buffers: 0,
        reserved_bytes: 0,
        committed_bytes: 0,
        peak_reserved_bytes: 0,
        peak_committed_bytes: 0,
        full_reads: 0,
        range_reads: 0,
        range_read_bytes: 0,
        metadata_reader_initializations: 0,
        metadata_logical_reads: 0,
        metadata_logical_bytes: 0,
        metadata_window_hits: 0,
        metadata_window_fills: 0,
        metadata_window_fill_bytes: 0,
        metadata_direct_reads: 0,
        metadata_direct_bytes: 0,
        reserve_failures: 0,
        commit_failures: 0,
        read_failures: 0,
        short_reads: 0,
        release_failures: 0,
        pressure_reclaim_attempts: 0,
        pressure_reclaimed_frames: 0,
        pressure_failures: 0,
    };
}

#[derive(Clone, Copy, Default)]
pub(crate) struct MetadataTotals {
    pub logical_reads: u64,
    pub logical_bytes: u64,
    pub window_hits: u64,
    pub window_fills: u64,
    pub window_fill_bytes: u64,
    pub direct_reads: u64,
    pub direct_bytes: u64,
}
impl MetadataTotals {
    pub(crate) const EMPTY: Self = Self {
        logical_reads: 0,
        logical_bytes: 0,
        window_hits: 0,
        window_fills: 0,
        window_fill_bytes: 0,
        direct_reads: 0,
        direct_bytes: 0,
    };
    pub(crate) fn publish(self, out: &mut Stats) {
        out.metadata_logical_reads = out
            .metadata_logical_reads
            .saturating_add(self.logical_reads);
        out.metadata_logical_bytes = out
            .metadata_logical_bytes
            .saturating_add(self.logical_bytes);
        out.metadata_window_hits = out.metadata_window_hits.saturating_add(self.window_hits);
        out.metadata_window_fills = out.metadata_window_fills.saturating_add(self.window_fills);
        out.metadata_window_fill_bytes = out
            .metadata_window_fill_bytes
            .saturating_add(self.window_fill_bytes);
        out.metadata_direct_reads = out.metadata_direct_reads.saturating_add(self.direct_reads);
        out.metadata_direct_bytes = out.metadata_direct_bytes.saturating_add(self.direct_bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_reader_totals_publish_once_without_touching_operational_state() {
        let mut global = Stats::EMPTY;
        global.read_failures = 5;
        global.range_reads = 2;
        global.metadata_reader_initializations = 1;
        let local = MetadataTotals {
            logical_reads: 11,
            logical_bytes: 179,
            window_hits: 8,
            window_fills: 2,
            window_fill_bytes: 5000,
            direct_reads: 1,
            direct_bytes: 8192,
        };
        assert_eq!(global.metadata_logical_reads, 0);
        local.publish(&mut global);
        assert_eq!(
            [
                global.metadata_logical_reads,
                global.metadata_logical_bytes,
                global.metadata_window_hits,
                global.metadata_window_fills,
                global.metadata_window_fill_bytes,
                global.metadata_direct_reads,
                global.metadata_direct_bytes
            ],
            [11, 179, 8, 2, 5000, 1, 8192]
        );
        assert_eq!(
            [
                global.read_failures,
                global.range_reads,
                global.metadata_reader_initializations
            ],
            [5, 2, 1]
        );
    }
    #[test]
    fn completed_reader_merges_saturate_all_detail_fields() {
        let mut global = Stats::EMPTY;
        let high = MetadataTotals {
            logical_reads: u64::MAX - 1,
            logical_bytes: u64::MAX - 1,
            window_hits: u64::MAX - 1,
            window_fills: u64::MAX - 1,
            window_fill_bytes: u64::MAX - 1,
            direct_reads: u64::MAX - 1,
            direct_bytes: u64::MAX - 1,
        };
        high.publish(&mut global);
        high.publish(&mut global);
        assert_eq!(
            [
                global.metadata_logical_reads,
                global.metadata_logical_bytes,
                global.metadata_window_hits,
                global.metadata_window_fills,
                global.metadata_window_fill_bytes,
                global.metadata_direct_reads,
                global.metadata_direct_bytes
            ],
            [u64::MAX; 7]
        );
        assert_eq!(global.read_failures, 0);
    }
}
