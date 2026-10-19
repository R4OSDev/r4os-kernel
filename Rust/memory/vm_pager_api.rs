//! Typed native VM/pager value interface. Decisions and mutable state
//! remain at the sole reservation, pager and backing owners.
use super::{
    owner::Guard, reclaim, virt_policy, vm_page_state as ps, vm_pager as pager,
    vm_pager_diagnostics as diag, vm_pager_types::*, vm_ranges, vm_transactions,
};
fn fixed(mut request: vm_ranges::Reserve) -> Result<u32, u32> {
    let _held = Guard::acquire(5);
    if !vm_transactions::r4os_rust_vm_initialized() {
        return Err(1);
    }
    let mut len = 0;
    let status = unsafe { virt_policy::r4os_rust_vm_normalize(request.len, false, &mut len) };
    if status != 0 {
        return Err(status);
    }
    if request.base & (PAGE - 1) != 0 {
        return Err(BAD_ALIGNMENT);
    }
    request.len = len;
    let mut id = 0;
    let status = unsafe { vm_ranges::r4os_rust_vm_reserve_at(&request, &mut id) };
    if status != 0 { Err(status) } else { Ok(id) }
}
fn first_owned(owner: u8, owner_id: u64, kind: Option<u8>) -> Option<u32> {
    for slot in 0..vm_ranges::RANGES {
        let range = vm_ranges::read(slot);
        if !range.active() || range.owner != owner || range.owner_id != owner_id {
            continue;
        }
        if kind.is_some_and(|wanted| range.kind != wanted) {
            continue;
        }
        return Some(range.id);
    }
    None
}
fn release_owner(owner: u8, owner_id: u64, kind: Option<u8>) -> Result<u64, u32> {
    let mut released = 0;
    loop {
        let candidate = {
            let _held = Guard::acquire(5);
            if !vm_transactions::r4os_rust_vm_initialized() {
                return Err(1);
            }
            first_owned(owner, owner_id, kind)
        };
        let Some(id) = candidate else { break };
        match vm_transactions::r4os_rust_vm_release(id) {
            0 => released += 1,
            NOT_FOUND => continue,
            error => return Err(error),
        }
    }
    Ok(released)
}
/// # Safety
/// Singleton BSP initializes all VM storage before admitting consumers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_pager_init() -> bool {
    unsafe { pager::init() }
}
/// # Safety
/// Request/output are disjoint C records. Borrowed name outlives its range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_reserve_fixed(
    input: *const vm_ranges::Reserve,
    output: *mut u32,
) -> u32 {
    match fixed(unsafe { input.read() }) {
        Ok(id) => {
            unsafe { output.write(id) };
            0
        }
        Err(code) => code,
    }
}
/// # Safety
/// Caller permits existing bounded/waiting VM work; output is exclusive C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_release_owner(
    owner: u8,
    owner_id: u64,
    kind: u8,
    kind_present: bool,
    output: *mut u64,
) -> u32 {
    match release_owner(owner, owner_id, kind_present.then_some(kind)) {
        Ok(value) => {
            unsafe { output.write(value) };
            0
        }
        Err(code) => code,
    }
}
/// # Safety
/// Exact interrupted task frame/SIMD state remains resident on its own stack
/// through a waiting fault. Caller supplies the original bound owner identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_demand_fault(
    address: u64,
    error: u64,
    owner: u8,
    owner_id: u64,
) -> bool {
    demand_fault(address, error, owner, owner_id, u64::MAX)
}
/// Exact program execution supplies its immutable resident limit. Admission
/// includes pending page-ins before the VM owner is released for file I/O.
pub(crate) fn demand_fault(
    address: u64,
    error: u64,
    owner: u8,
    owner_id: u64,
    resident_limit: u64,
) -> bool {
    pager::handle_demand_fault(address, error, owner, owner_id, resident_limit)
}
/// # Safety
/// Input/output are disjoint C values valid until this bounded call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_page_state_probe(
    input: *const PageStateInput,
    output: *mut PageStateResult,
) {
    let input = unsafe { input.read() };
    unsafe { output.write(pager::page_state_probe(input)) };
}
/// # Safety
/// Exact admitted I/O result supplies immutable binding; output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_apply_page_io(
    input: *const PageStateInput,
    page_out: bool,
    output: *mut PageStateResult,
) {
    let input = unsafe { input.read() };
    unsafe { output.write(pager::apply_page_io(input, page_out)) };
}
/// # Safety
/// output receives one passive private C snapshot under existing VM admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_page_state_summary(output: *mut PageStateSummary) {
    let _held = Guard::acquire(5);
    let summary = ps::with(|s| {
        let mut out = s.summary;
        out.span_count = s.active;
        out
    });
    unsafe { output.write(summary) };
}
/// # Safety
/// Input retains the exact caller's admitted failed transfer coordinates.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_policy_failure(input: *const PageStateInput, page_out: bool) {
    pager::record_policy_failure(unsafe { input.read() }, page_out);
}
/// # Safety
/// Pointer denotes the exact resident source mapping; output owns a writeback
/// value. The caller must explicitly end an admitted value after real I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_begin_writeback(
    id: u32,
    offset: u64,
    pages: u64,
    pointer: *const u8,
    output: *mut PageWriteback,
) -> u32 {
    match pager::begin_writeback(id, offset, pages, pointer) {
        Ok(value) => {
            unsafe { output.write(value) };
            0
        }
        Err(code) => code,
    }
}
/// # Safety
/// Caller exclusively owns its exact admitted writeback value and unwind token.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_end_writeback(input: *mut PageWriteback) {
    let mut value = unsafe { input.read() };
    pager::end_writeback(&mut value);
    unsafe { input.write(value) };
}
/// # Safety
/// Exact live writeback and successful transfer binding belong to this caller;
/// output is disjoint. Completion never substitutes for explicit endWriteback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_complete_writeback(
    input: *mut PageWriteback,
    binding: *const PageStateInput,
    output: *mut u64,
) -> u32 {
    let mut value = unsafe { input.read() };
    let result = pager::complete_writeback(&mut value, unsafe { binding.read() });
    unsafe { input.write(value) };
    match result {
        Ok(value) => {
            unsafe { output.write(value) };
            0
        }
        Err(code) => code,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_commit_covers(id: u32, offset: u64, pages: u64) -> bool {
    pager::commit_covers(id, offset, pages)
}
/// # Safety
/// Existing reclaim caller permits bounded waiting work and supplies its exact
/// optional VM owner (Some(0) excludes all nonzero R4X owners). Output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_reclaim_evict(
    reason: u32,
    frames: u32,
    owner: u64,
    owner_present: bool,
    output: *mut reclaim::SourceResult,
) {
    let result = pager::reclaim_evict(reason, frames, owner_present.then_some(owner));
    unsafe { output.write(result) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_evictable_bytes(dirty: bool) -> u64 {
    pager::evictable_bytes(dirty)
}
/// # Safety
/// output is one disjoint writable private C statistics record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_hot_stats(output: *mut HotPathStats) {
    unsafe { output.write(diag::hot_stats()) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_metadata_invariant() -> bool {
    diag::invariant()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vm_dump_stats() {
    diag::dump()
}
