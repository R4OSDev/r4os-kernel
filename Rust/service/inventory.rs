//! Caller-owned, revision-bound paging. Only membership/configuration shares
//! one revision; endpoint/process observations keep their actual owners.
use super::{registry as reg, registry_api as api, registry_types::*};
use crate::{contract as c, memory::scratch::Scratch, platform::timer, sched::task_context};
use core::{mem, ptr};

const ROWS: usize = c::service_inventory_page_rows;
struct Batch {
    targets: [ApiIndexTarget; ROWS],
    rows: [c::ServiceInventoryRow; ROWS],
}

fn revision() -> Option<u64> {
    let _guard = reg::lock_registry(2)?;
    Some(unsafe { (*reg::raw()).inventory_revision })
}

fn changed(expected: u64) -> i32 {
    if revision().is_some_and(|current| current != expected) {
        c::service_api_result_restart
    } else {
        c::service_api_result_busy
    }
}

/// # Safety
/// Caller lends disjoint writable cursor/row storage for the entire call.
/// No caller pointer or partially copied page is retained after return.
pub unsafe extern "C" fn page(
    cursor: *mut c::ServiceInventoryCursor,
    rows: *mut c::ServiceInventoryRow,
    capacity: u32,
) -> i32 {
    unsafe {
        if cursor.is_null() || rows.is_null() || capacity == 0 {
            return c::service_api_result_invalid;
        }
        let Some(_call) = task_context::Admission::acquire(false) else {
            return c::service_api_result_busy;
        };
        let mut next = cursor.read();
        if next.reserved != 0
            || next.flags & !(c::service_inventory_dynamic | c::service_inventory_end) != 0
            || (next.revision == 0 && (next.after_id != 0 || next.total != 0 || next.flags != 0))
        {
            return c::service_api_result_invalid;
        }
        let Ok(after) = usize::try_from(next.after_id) else {
            return c::service_api_result_invalid;
        };
        let Some(batch) = Scratch::<Batch>::new() else {
            return c::service_api_result_memory;
        };
        let batch = batch.as_mut_ptr();
        ptr::write_bytes(batch, 0, 1);
        let count = {
            let Some(_guard) = reg::lock_registry(2) else {
                return c::service_api_result_busy;
            };
            let r = reg::raw();
            let current = (*r).inventory_revision;
            if next.revision != 0 && next.revision != current {
                return c::service_api_result_restart;
            }
            if next.revision == 0 {
                next.revision = current;
                next.total = (*r).entries.count as u64;
            } else if next.total != (*r).entries.count as u64
                || (after != 0 && !reg::entry_exists(after))
            {
                return c::service_api_result_invalid;
            }
            let mut last = after;
            let mut count = 0;
            while count < (capacity as usize).min(ROWS) {
                let node = (*r).entries.after(last);
                if node.is_null() {
                    break;
                }
                last = (*node).index.key as usize;
                (*batch).targets[count] = reg::target_for_slot(last, NAMED_TARGET_INDEX);
                count += 1;
            }
            count
        };
        for i in 0..count {
            let target = ptr::addr_of_mut!((*batch).targets[i]);
            if !crate::program::service_api::refresh_inventory_target(&mut *target) {
                return changed(next.revision);
            }
            let row = ptr::addr_of_mut!((*batch).rows[i]);
            let endpoint = {
                let Some(_guard) = reg::lock_registry(2) else {
                    return c::service_api_result_busy;
                };
                if (*reg::raw()).inventory_revision != next.revision {
                    return c::service_api_result_restart;
                }
                let Some(slot) = reg::target_slot(&*target, true) else {
                    return c::service_api_result_busy;
                };
                let e = &*reg::entry(slot);
                (*row).registration_id = slot as u64;
                (*row).start_generation = e.start_generation;
                (*row).process = e.binding.process;
                api::fill_entry(slot, &mut (*row).info, timer::tick_count());
                (&mut (*row).path)[..e.path_len].copy_from_slice(&e.path[..e.path_len]);
                (&mut (*row).args)[..e.args_len].copy_from_slice(&e.args[..e.args_len]);
                (&mut (*row).description)[..e.description_len]
                    .copy_from_slice(&e.description[..e.description_len]);
                reg::endpoint_for_slot(slot).map(|id| reg::identity(reg::endpoint(id)))
            };
            api::append_endpoint(endpoint, ptr::addr_of_mut!((*row).info));
            next.after_id = (*row).registration_id;
        }
        {
            let Some(_guard) = reg::lock_registry(2) else {
                return c::service_api_result_busy;
            };
            let r = reg::raw();
            if (*r).inventory_revision != next.revision {
                return c::service_api_result_restart;
            }
            next.flags = c::service_inventory_dynamic;
            if (*r).entries.after(next.after_id as usize).is_null() {
                next.flags |= c::service_inventory_end;
            }
        }
        // No registry/endpoint owner spans potentially pageable caller copies.
        ptr::copy_nonoverlapping((*batch).rows.as_ptr(), rows, count);
        cursor.write(next);
        count as i32
    }
}

/// Bind before any process refresh/close/start. Full registration identity
/// refuses a same-name reinstall even when both old/new rows are unbound.
pub(crate) fn control_target(request: &c::ServiceControlRequest) -> Result<ApiIndexTarget, i32> {
    let Some(_guard) = reg::lock_registry(2) else {
        return Err(c::service_api_result_busy);
    };
    unsafe {
        let Ok(slot) = usize::try_from(request.registration_id) else {
            return Err(c::service_api_result_invalid);
        };
        if slot == 0 {
            return Err(c::service_api_result_invalid);
        }
        if !reg::entry_exists(slot) {
            return Err(c::service_api_result_not_found);
        }
        let target = reg::target_for_slot(slot, NAMED_TARGET_INDEX);
        let expected = request.process;
        let actual = target.binding.process;
        if expected.reserved != 0 {
            return Err(c::service_api_result_invalid);
        }
        if target.start_generation != request.start_generation
            || actual.instance_id != expected.instance_id
            || actual.generation != expected.generation
            || actual.reserved != expected.reserved
        {
            return Err(c::service_api_result_busy);
        }
        Ok(target)
    }
}

const _: () = {
    assert!(mem::size_of::<c::ServiceInventoryCursor>() == 32);
    assert!(mem::size_of::<c::ServiceInventoryRow>() == 1448);
    assert!(mem::size_of::<c::ServiceControlRequest>() == 48);
};
