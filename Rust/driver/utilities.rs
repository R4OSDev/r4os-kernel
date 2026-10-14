//! Existing driver policies, transferred without changing resource admission.
use core::ptr;

#[repr(C)]
pub struct MsixGeometry {
    pub bir: u8,
    pub offset: u32,
    pub mapping_bytes: u32,
}

/// # Safety
/// The output is writable and aligned; failure does not modify it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_msix_geometry(
    descriptor: u32,
    max_mapping_bytes: u64,
    output: *mut MsixGeometry,
) -> bool {
    let bir = (descriptor & 7) as u8;
    let offset = descriptor & 0xffff_fff8;
    let required = u64::from(offset) + 16;
    if bir >= 6 || required > max_mapping_bytes || required > u64::from(u32::MAX) {
        return false;
    }
    unsafe {
        output.write(MsixGeometry {
            bir,
            offset,
            mapping_bytes: required as u32,
        });
    }
    true
}

#[repr(C)]
pub struct WorkTiming {
    pub queue_ticks: u64,
    pub run_ticks: u64,
    pub lateness_ticks: u64,
    pub deadline_missed: bool,
    pub budget_overrun: bool,
}

const _: () = {
    assert!(core::mem::size_of::<MsixGeometry>() == 12);
    assert!(core::mem::offset_of!(MsixGeometry, offset) == 4);
    assert!(core::mem::offset_of!(MsixGeometry, mapping_bytes) == 8);
    assert!(core::mem::size_of::<WorkTiming>() == 32);
    assert!(core::mem::offset_of!(WorkTiming, deadline_missed) == 24);
    assert!(core::mem::offset_of!(WorkTiming, budget_overrun) == 25);
};

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_deadline_valid(
    version: u32,
    size: u32,
    required_size: u32,
    work_class: u32,
    serial_key: u64,
    deadline_tick: u64,
    budget_ticks: u64,
) -> bool {
    version == 1
        && size >= required_size
        && work_class == 1
        && serial_key != 0
        && deadline_tick != 0
        && budget_ticks != 0
        && budget_ticks <= 4
}

/// # Safety
/// Exclusive aligned output; timing reads have no resource ownership effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_deadline_analyze(
    submitted: u64,
    started: u64,
    completed: u64,
    deadline: u64,
    budget: u64,
    output: *mut WorkTiming,
) {
    let run_ticks = completed.saturating_sub(started);
    unsafe {
        output.write(WorkTiming {
            queue_ticks: started.saturating_sub(submitted),
            run_ticks,
            lateness_ticks: started.saturating_sub(deadline),
            deadline_missed: started > deadline,
            budget_overrun: run_ticks > budget,
        });
    }
}

/// # Safety
/// Caller holds the existing exact live mapping/owner admission. Original and
/// optional bounce are resident writable non-overlapping ranges of the supplied
/// lengths. A null bounce denotes the existing direct-mapping path. No lifetime
/// or concurrent CPU/device access permission is granted here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_dma_synchronize(
    original: *mut u8,
    original_len: usize,
    bounce: *mut u8,
    bounce_len: usize,
    direction: u32,
    cpu_phase: bool,
    offset: u32,
    bytes: u32,
) -> bool {
    assert!(direction <= 2, "Invalid DMA direction");
    let offset = offset as usize;
    let bytes = bytes as usize;
    if bytes == 0 || offset > original_len || bytes > original_len - offset {
        return false;
    }
    if !bounce.is_null() && bounce_len != original_len {
        return false;
    }
    unsafe {
        if cpu_phase {
            core::arch::asm!("mfence", options(nostack, preserves_flags));
        }
        if !bounce.is_null() {
            if !cpu_phase && direction != 2 {
                ptr::copy_nonoverlapping(original.add(offset), bounce.add(offset), bytes);
            }
            if cpu_phase && direction != 1 {
                ptr::copy_nonoverlapping(bounce.add(offset), original.add(offset), bytes);
            }
        }
        if !cpu_phase {
            core::arch::asm!("mfence", options(nostack, preserves_flags));
        }
    }
    true
}
