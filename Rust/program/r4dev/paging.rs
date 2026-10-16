//! Copied page-table and task observations; owner admissions end before
//! writing the canonical caller result.
use crate::{
    contract as c,
    memory::{blocks, paging, root},
    sched::task::task_snapshots,
};
/// # Safety
/// Exclusive aligned canonical caller output for this passive observation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_paging_summary(out: *mut c::PagingSummary) -> i32 {
    unsafe {
        let p = super::snapshot(paging::r4os_rust_paging_stats);
        let pt = super::snapshot(root::r4os_rust_root_stats);
        let blocks = super::snapshot(blocks::r4os_rust_blocks_summary);
        let active_count = blocks.active_blocks.min(u64::from(u32::MAX)) as u32;
        let (
            mut page_table_blocks,
            mut kernel_page_table_blocks,
            mut bootloader_page_table_blocks,
            mut page_table_bytes,
        ) = (0u64, 0u64, 0u64, 0u64);
        for index in 0..active_count {
            let Some(block) = blocks::diagnostic_active(index, false) else {
                continue;
            };
            let block = block.info;
            if block.kind != c::memory_kind_page_table {
                continue;
            }
            page_table_blocks += 1;
            page_table_bytes = page_table_bytes.wrapping_add(block.phys_len);
            if block.owner == c::memory_owner_kernel {
                kernel_page_table_blocks += 1
            }
            if block.owner == c::memory_owner_bootloader {
                bootloader_page_table_blocks += 1
            }
        }
        let mut flags = 0;
        if p.initialized {
            flags |= c::paging_flag_initialized
        }
        if p.active_root_confirmed {
            flags |= c::paging_flag_active_root_matches_hardware
        }
        if p.r4os_root_active {
            flags |= c::paging_flag_r4os_root_active
        }
        if pt.cr3_switch_done {
            flags |= c::paging_flag_cr3_switch_done
        }
        if pt.limine_quarantined_frames != 0 {
            flags |= c::paging_flag_limine_quarantine
        }
        out.write(c::PagingSummary {
            flags,
            root_owner: p.root_owner,
            reserved0: [0; 3],
            active_root_phys: p.active_root_phys,
            hardware_cr3: p.hardware_cr3,
            old_cr3: pt.old_cr3,
            new_cr3: pt.new_cr3,
            page_table_blocks,
            kernel_page_table_blocks,
            bootloader_page_table_blocks,
            page_table_bytes,
            limine_old_table_frames: pt.limine_old_table_frames,
            limine_active_table_frames: pt.limine_active_table_frames,
            limine_referenced_frames: pt.limine_referenced_frames,
            limine_quarantined_frames: pt.limine_quarantined_frames,
            limine_released_frames: pt.limine_released_frames,
            limine_retained_frames: pt.limine_retained_frames,
            root_mismatches: p.root_mismatches,
            map_pages: p.map_pages,
            unmap_pages: p.unmap_pages,
            invlpg_flushes: p.invlpg_flushes,
        });
        1
    }
}
/// # Safety
/// Exclusive canonical caller output. Original ordinal/pin rules are kept by
/// the task owner; no Core or resident label is borrowed after its admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_task(
    index: u32,
    out: *mut c::ProgramTaskPerformanceInfo,
) -> i32 {
    unsafe {
        match task_snapshots::performance(index) {
            Some(value) => {
                out.write(value);
                1
            }
            None => {
                out.write(Default::default());
                0
            }
        }
    }
}
