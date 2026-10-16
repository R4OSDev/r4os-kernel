//! Original batch validation and all-or-nothing candidate/commit publication.
use super::super::registry_format as f;
use super::{path, registry as reg, registry_build as build, registry_state as s};
use crate::contract as c;
use core::mem::size_of;
pub fn slice(blob: &[u8], offset: u32, len: u32) -> Option<&[u8]> {
    let offset = offset as usize;
    let len = len as usize;
    if offset > blob.len() || len > blob.len() - offset {
        None
    } else {
        Some(&blob[offset..offset + len])
    }
}
fn fail(out: &mut c::RegistryBatchResult, index: usize, result: i32) -> Option<i32> {
    out.failed_index = index as u32;
    out.status = c::registry_batch_status_validation_failed;
    Some(result)
}
fn validate(
    operations: &[c::RegistryBatchOperation],
    blob: &[u8],
    out: &mut c::RegistryBatchResult,
) -> Option<i32> {
    for (index, operation) in operations.iter().enumerate() {
        if operation.reserved0 != 0
            || !matches!(
                operation.operation,
                c::registry_batch_operation_set | c::registry_batch_operation_delete
            )
        {
            return fail(out, index, c::registry_api_result_invalid);
        }
        let Some(key) = slice(blob, operation.key_path_offset, operation.key_path_len) else {
            return fail(out, index, c::registry_api_result_invalid);
        };
        let Some(name) = slice(blob, operation.value_name_offset, operation.value_name_len) else {
            return fail(out, index, c::registry_api_result_invalid);
        };
        if key.is_empty() || key.len() > c::registry_path_max_bytes as usize {
            return fail(out, index, c::registry_api_result_bad_path);
        }
        let Some(root) = f::parse_root(key) else {
            return fail(out, index, c::registry_api_result_bad_path);
        };
        if root.kind != 1 {
            return fail(out, index, c::registry_api_result_unsupported);
        }
        if !f::valid_name(name, true) {
            return fail(out, index, c::registry_api_result_invalid);
        }
        if operation.operation == c::registry_batch_operation_delete {
            if operation.value_type != 0 || operation.data_len != 0 {
                return fail(out, index, c::registry_api_result_invalid);
            }
        } else {
            if !f::type_valid(operation.value_type) {
                return fail(out, index, c::registry_api_result_invalid);
            }
            let Some(data) = slice(blob, operation.data_offset, operation.data_len) else {
                return fail(out, index, c::registry_api_result_invalid);
            };
            if f::validate_payload(operation.value_type, data).is_err() {
                return fail(out, index, c::registry_api_result_invalid);
            }
        }
        for prior in &operations[..index] {
            if path::equal_path(
                slice(blob, prior.key_path_offset, prior.key_path_len)
                    .expect("Validated prior batch path"),
                key,
            ) && slice(blob, prior.value_name_offset, prior.value_name_len)
                .expect("Validated prior batch name")
                .eq_ignore_ascii_case(name)
            {
                return fail(out, index, c::registry_api_result_invalid);
            }
        }
    }
    None
}
/// # Safety
/// Original nullable validation followed by readable bounded operations/blob
/// and exclusive public result storage lasting through transaction waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_batch(
    operations: *const c::RegistryBatchOperation,
    count: u32,
    blob: *const u8,
    len: u32,
    out: *mut c::RegistryBatchResult,
) -> i32 {
    unsafe {
        if operations.is_null() || blob.is_null() || out.is_null() {
            return c::registry_api_result_invalid;
        }
        if (*out).version != u32::from(c::registry_batch_version)
            || (*out).size < size_of::<c::RegistryBatchResult>() as u32
        {
            return c::registry_api_result_invalid;
        }
        out.write(c::RegistryBatchResult {
            version: u32::from(c::registry_batch_version),
            size: size_of::<c::RegistryBatchResult>() as u32,
            operation_count: count,
            failed_index: c::registry_batch_failed_index_none,
            status: c::registry_batch_status_invalid,
            ..Default::default()
        });
        if count == 0 || count > c::registry_batch_operation_max || len > c::registry_batch_blob_max
        {
            return c::registry_api_result_invalid;
        }
        let operations = core::slice::from_raw_parts(operations, count as usize);
        let blob = core::slice::from_raw_parts(blob, len as usize);
        if let Some(error) = validate(operations, blob, &mut *out) {
            return error;
        }
        let _transaction = s::Transaction::acquire();
        let resumed = reg::resume();
        if resumed != c::registry_api_result_ok {
            (*out).status = c::registry_batch_status_commit_failed;
            return resumed;
        }
        let flushed = reg::flush_under_transaction();
        if flushed != c::registry_api_result_ok {
            (*out).status = c::registry_batch_status_commit_failed;
            return flushed;
        }
        let loaded = reg::ensure(1);
        if loaded != c::registry_api_result_ok && loaded != c::registry_api_result_hive_not_found {
            (*out).status = c::registry_batch_status_commit_failed;
            return loaded;
        }
        let candidate = build::batch(operations, blob, &mut *out);
        if candidate.result != c::registry_api_result_ok {
            (*out).status = c::registry_batch_status_validation_failed;
            (*out).generation_after = (*out).generation_before;
            return candidate.result;
        }
        let committed = reg::commit(candidate.slot, false);
        if committed.result != c::registry_api_result_ok {
            s::failure(candidate.slot, committed.uncertain);
            (*out).status = c::registry_batch_status_commit_failed;
            (*out).generation_after = (*out).generation_before;
            return committed.result;
        }
        let published = s::publish(candidate.slot, false);
        if published != c::registry_api_result_ok {
            (*out).status = c::registry_batch_status_commit_failed;
            (*out).generation_after = (*out).generation_before;
            return published;
        }
        (*out).status = c::registry_batch_status_committed;
        (*out).generation_after = candidate.generation;
        c::registry_api_result_ok
    }
}
