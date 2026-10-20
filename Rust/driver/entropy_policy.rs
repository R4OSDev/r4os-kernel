//! Generic completion validation only; device queues remain in the R4D owner.
use crate::contract as c;

pub(crate) fn completed(r: &c::EntropyDeviceReceipt, bytes: u32, previous: u64) -> bool {
    let required = c::entropy_receipt_flag_started
        | c::entropy_receipt_flag_used
        | c::entropy_receipt_flag_length_verified
        | c::entropy_receipt_flag_within_budget;
    r.version == 1
        && r.size == 128
        && r.status == c::entropy_device_status_completed
        && r.flags & required == required
        && r.flags & (c::entropy_receipt_flag_cancelled | c::entropy_receipt_flag_retained) == 0
        && r.requested_bytes == bytes
        && r.completed_bytes == bytes
        && r.request_generation > previous
        && r.dma_submitted != 0
        && r.dma_submitted == r.dma_completed
        && r.dma_completed <= u64::from(bytes)
        && r.short_completions < r.dma_completed
        && r.last_used_length != 0
        && r.last_used_length <= bytes
        && r.available_index_before <= 65535
        && r.available_index_after <= 65535
        && r.used_index_before <= 65535
        && r.used_index_after <= 65535
        && (r.available_index_after as u16).wrapping_sub(r.available_index_before as u16)
            == r.dma_submitted as u16
        && (r.used_index_after as u16).wrapping_sub(r.used_index_before as u16)
            == r.dma_completed as u16
        && r.end_ns >= r.begin_ns
        && r.end_ns - r.begin_ns <= c::entropy_max_window_ns
        && matches!(r.clock_source, 1 | 2)
        && r.clock_generation != 0
        && r.device_status & 15 == 15
        && r.device_status & 192 == 0
        && r.pci_command & 6 == 6
        && r.reserved0 == [0; 2]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn receipt() -> c::EntropyDeviceReceipt {
        c::EntropyDeviceReceipt {
            flags: 15,
            requested_bytes: 17,
            completed_bytes: 17,
            last_used_length: 8,
            dma_submitted: 2,
            dma_completed: 2,
            short_completions: 1,
            begin_ns: 10,
            end_ns: 20,
            request_generation: 1,
            available_index_before: 65535,
            available_index_after: 1,
            used_index_before: 65535,
            used_index_after: 1,
            device_status: 15,
            pci_command: 6,
            clock_generation: 1,
            clock_source: 2,
            ..Default::default()
        }
    }
    #[test]
    fn accepts_real_length_contract_and_index_wrap() {
        assert!(completed(&receipt(), 17, 0));
        assert!(!completed(&receipt(), 17, 1));
    }
    #[test]
    fn refuses_unfinished_retained_and_bad_provenance() {
        let good = receipt();
        for op in 0..10 {
            let mut r = good;
            match op {
                0 => r.completed_bytes -= 1,
                1 => r.dma_completed -= 1,
                2 => r.flags |= c::entropy_receipt_flag_retained,
                3 => r.last_used_length = 0,
                4 => r.used_index_after += 1,
                5 => r.status = c::entropy_device_status_timeout,
                6 => r.end_ns += c::entropy_max_window_ns,
                7 => r.device_status |= 64,
                8 => r.pci_command = 2,
                _ => r.clock_generation = 0,
            }
            assert!(!completed(&r, 17, 0), "case {op}");
        }
    }
}
