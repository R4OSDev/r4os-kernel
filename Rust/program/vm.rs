//! Original R4X VM API admission, profile limits and reserve-only diagnostic.
//! Native VM/block owners provide narrow budgets and complete admitted transactions.
use super::{memory, registry_types::ProgramInstance, stack};
use crate::{
    contract::{self, ProgramVmRegionInfo as Info, ProgramVmReserveProbe as Probe},
    memory::{blocks, vm_query, vm_ranges::Range, vm_transactions},
};
use core::mem::MaybeUninit;
const PAGE: u64 = 4096;
const NO_INSTANCE: i32 = -12;
const INVALID: i32 = -1;
const OWNER: i32 = -3;
const FLAGS: i32 = -11;
const LIMIT: i32 = -13;

// The explicit reserve probe still needs the complete diagnostic snapshot.
fn diagnostic_stats() -> vm_query::Stats {
    let mut out = MaybeUninit::uninit();
    unsafe {
        vm_query::r4os_rust_vm_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
pub(super) unsafe fn profile_limit(
    operation: u8,
    instance: *const ProgramInstance,
    add: u64,
) -> bool {
    let stats = vm_query::admission_application(unsafe { u64::from((*instance).id) });
    let (current, limit) = unsafe {
        match operation {
            0 => (stats.reserved, (*instance).memory_limits.vm_reserve_limit),
            1 => (stats.committed, (*instance).memory_limits.vm_commit_limit),
            2 => (stats.resident, (*instance).memory_limits.resident_limit),
            _ => unreachable!(),
        }
    };
    current.checked_add(add).is_some_and(|next| next <= limit)
}
pub(super) fn system_limit(add: u64) -> bool {
    let (stats, available_frames) = vm_query::admission_system();
    let available = available_frames * PAGE;
    let budget = stats.resident.saturating_add(available);
    stats
        .committed
        .checked_add(add)
        .is_some_and(|next| next <= budget)
}
/// # Safety
/// Original exact instance lifetime for 0..=2; operation3 reads system budget.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_vm_limit(
    operation: u8,
    instance: *const ProgramInstance,
    add: u64,
) -> bool {
    if operation == 3 {
        system_limit(add)
    } else {
        unsafe { profile_limit(operation, instance, add) }
    }
}
fn error(code: u32) -> i32 {
    match code {
        0 => 0,
        1 | 3 | 5 | 8 | 15 => INVALID,
        4 => -2,
        2 => -4,
        6 | 7 => -5,
        9 => -7,
        10 => -8,
        11 => -9,
        12 => -6,
        13 | 14 => -10,
        _ => unreachable!(),
    }
}
fn paging_flags(raw: u64) -> Option<u64> {
    if raw & !3 != 0 {
        return None;
    }
    let flags = if raw == 0 { 1 } else { raw };
    let mut out = 1u64 << 63;
    if flags & 1 != 0 {
        out |= 2;
    }
    if flags & 2 != 0 {
        out &= !(1 << 63);
    }
    Some(out)
}
fn public_flags(flags: u64) -> u64 {
    let mut out = 0;
    if flags & 2 != 0 {
        out |= 1;
    }
    if flags & (1 << 63) == 0 {
        out |= 2;
    }
    out
}
fn owned(instance: u32, region: u32) -> Result<Range, i32> {
    let info = stack::range(region).ok_or(INVALID)?;
    if info.owner != 3 || info.owner_id != u64::from(instance) {
        return Err(OWNER);
    }
    if info.window != 6 || info.kind != 4 {
        return Err(INVALID);
    }
    Ok(info)
}
unsafe fn current_region(region: u32) -> Result<Range, i32> {
    let instance = unsafe { super::context::current() };
    if instance.is_null() {
        return Err(NO_INSTANCE);
    }
    owned(unsafe { (*instance).id }, region)
}
unsafe fn fill(instance: u32, region: u32, out: *mut Info) -> i32 {
    let info = match owned(instance, region) {
        Ok(info) => info,
        Err(code) => return code,
    };
    unsafe {
        out.write(Info {
            id: info.id,
            status: info.status,
            owner: info.owner,
            kind: info.kind,
            window: info.window,
            owner_id: info.owner_id,
            base: info.base,
            len: info.len,
            committed_bytes: info.committed_bytes,
            guard_base: info.guard_base,
            guard_len: info.guard_len,
            flags: public_flags(info.flags),
            resident_bytes: info.resident_bytes,
            peak_resident_bytes: info.peak_resident_bytes,
            fault_count: info.fault_count,
            failed_faults: info.failed_faults,
        })
    };
    0
}
/// # Safety
/// Exclusive resident canonical output and original current-execution lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_vm_reserve(
    size: u64,
    alignment: u64,
    flags: u64,
    out: *mut Info,
) -> i32 {
    unsafe {
        out.write(Info::default());
        let instance = super::context::current();
        if instance.is_null() {
            return NO_INSTANCE;
        }
        let Some(map_flags) = paging_flags(flags) else {
            return FLAGS;
        };
        let alignment = if alignment == 0 { PAGE } else { alignment };
        let Some(len) = memory::page_align(size) else {
            return INVALID;
        };
        // The quota snapshot and publication share the VM owner. No caller
        // output or waiting I/O is copied while this outer admission is held.
        let region = {
            let _vm = crate::memory::owner::Guard::acquire(5);
            if !profile_limit(0, instance, len) {
                return LIMIT;
            }
            match stack::reserve(
                6,
                4,
                (*instance).id,
                size,
                alignment,
                b"r4x-vm-region",
                map_flags,
                true,
            ) {
                Ok(id) => id,
                Err(code) => return error(code),
            }
        };
        let code = fill((*instance).id, region, out);
        if code != 0 {
            vm_transactions::r4os_rust_vm_release(region);
        }
        code
    }
}
/// # Safety
/// Retains the original current-execution owner lifetime and no registry lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_vm_commit(
    region: u32,
    offset: u64,
    len: u64,
    flags: u64,
) -> i32 {
    unsafe {
        if flags != 0 && flags != contract::vm_commit_flag_resident {
            return FLAGS;
        }
        if let Err(code) = current_region(region) {
            return code;
        }
        let Some(commit_len) = memory::page_align(len) else {
            return INVALID;
        };
        let instance = super::context::current();
        if instance.is_null() {
            return NO_INSTANCE;
        }
        // Window6 demand commit is metadata-only; resident commit is already
        // bounded to64 pages and retains this owner for its complete operation.
        let _vm = crate::memory::owner::Guard::acquire(5);
        if !profile_limit(1, instance, commit_len) || !system_limit(commit_len) {
            return LIMIT;
        }
        if flags == contract::vm_commit_flag_resident {
            if commit_len > contract::vm_commit_resident_max_bytes {
                return INVALID;
            }
            if !profile_limit(2, instance, commit_len) {
                return LIMIT;
            }
            error(vm_transactions::r4os_rust_vm_commit_resident(
                region, offset, len,
            ))
        } else {
            error(vm_transactions::r4os_rust_vm_commit(region, offset, len))
        }
    }
}
/// # Safety
/// Original current-execution owner lifetime. operation0=decommit,1=release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_vm_change(
    operation: u8,
    region: u32,
    offset: u64,
    len: u64,
) -> i32 {
    if let Err(code) = unsafe { current_region(region) } {
        return code;
    }
    error(match operation {
        0 => vm_transactions::r4os_rust_vm_uncommit(region, offset, len),
        1 => vm_transactions::r4os_rust_vm_release(region),
        _ => unreachable!(),
    })
}
/// # Safety
/// Exclusive canonical output; current-execution owner stays resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_vm_query(region: u32, out: *mut Info) -> i32 {
    unsafe {
        out.write(Info::default());
        let instance = super::context::current();
        if instance.is_null() {
            return NO_INSTANCE;
        }
        fill((*instance).id, region, out)
    }
}
fn block_summary() -> blocks::Summary {
    let mut out = MaybeUninit::uninit();
    unsafe {
        blocks::r4os_rust_blocks_summary(out.as_mut_ptr());
        out.assume_init()
    }
}
fn before(out: &mut Probe) {
    let blocks = block_summary();
    let virt = diagnostic_stats();
    out.active_before = blocks.active_blocks;
    out.committed_before = blocks.committed_bytes;
    out.largest_free_before = virt.largest_free_virtual_len;
}
fn during(out: &mut Probe) {
    let blocks = block_summary();
    out.active_during = blocks.active_blocks;
    out.committed_during = blocks.committed_bytes;
}
fn after(out: &mut Probe) {
    let blocks = block_summary();
    let virt = diagnostic_stats();
    out.active_after = blocks.active_blocks;
    out.committed_after = blocks.committed_bytes;
    out.largest_free_after = virt.largest_free_virtual_len;
    if out.active_during == 0 {
        out.active_during = out.active_before;
    }
    if out.committed_during == 0 {
        out.committed_during = out.committed_before;
    }
}
fn probe_error(code: u32) -> i32 {
    match code {
        3 | 4 | 5 | 15 => -2,
        2 => -3,
        6 | 7 => -4,
        _ => -8,
    }
}
/// # Safety
/// Exclusive resident canonical output for the complete reserve/release probe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_vm_probe(requested: u64, out: *mut Probe) -> i32 {
    unsafe {
        // Raw output accesses do not retain a Rust reference over VM operations.
        out.write(Probe {
            requested_bytes: requested,
            ..Probe::default()
        });
        before(&mut *out);
        let instance = super::context::current();
        if instance.is_null() {
            (*out).status = -1;
            after(&mut *out);
            return 0;
        }
        let Some(len) = memory::page_align(requested) else {
            (*out).status = -2;
            after(&mut *out);
            return 0;
        };
        if len == 0 {
            (*out).status = -2;
            after(&mut *out);
            return 0;
        }
        let region = match stack::reserve(
            6,
            4,
            (*instance).id,
            len,
            PAGE,
            b"r4x-vm-probe",
            2 | (1 << 63),
            true,
        ) {
            Ok(id) => id,
            Err(code) => {
                (*out).status = probe_error(code);
                after(&mut *out);
                return 0;
            }
        };
        (*out).region_id = region;
        during(&mut *out);
        if let Some(info) = stack::range(region) {
            (*out).base = info.base;
            (*out).len = info.len;
            (*out).reserved_bytes = info.len;
            (*out).committed_bytes = info.committed_bytes;
        } else {
            (*out).status = -5;
        }
        if (*out).status == 0 {
            let mut block = MaybeUninit::<blocks::Block>::uninit();
            if blocks::r4os_rust_blocks_read(1, 0, (*out).base, block.as_mut_ptr()) {
                let block = block.assume_init();
                (*out).reserved_bytes = block.reserved_bytes;
                (*out).committed_bytes = block.committed_bytes;
                (*out).phys_len = block.phys_len;
                (*out).owner_id = block.owner_id;
                (*out).kind = block.kind;
                (*out).owner = block.owner;
                (*out).block_status = block.status;
            } else {
                (*out).status = -6;
            }
        }
        let released = vm_transactions::r4os_rust_vm_release(region) == 0;
        (*out).released = u32::from(released);
        if !released && (*out).status == 0 {
            (*out).status = -7;
        }
        after(&mut *out);
        0
    }
}
