//! Unchanged public R4SYS registry reads, typed mutations and snapshot pages.
//! Contract owns every public record; validated hive views stay with their
//! exact cache generation while the registry owner is held.
use super::super::{registry_format as f, registry_format_types as t};
use super::{path, registry as reg, registry_state as s};
use crate::contract as c;
use core::{mem::size_of, ptr};
macro_rules! some {
    ($value:expr,$error:expr) => {
        match $value {
            Some(value) => value,
            None => return $error,
        }
    };
}
fn kind(path: &[u8]) -> Result<u16, i32> {
    let parsed = f::parse_root(path).ok_or(c::registry_api_result_bad_path)?;
    if parsed.kind != 1 {
        return Err(c::registry_api_result_unsupported);
    }
    Ok(parsed.kind)
}
unsafe fn ready(kind: u16) -> Result<(s::StateGuard, t::View), i32> {
    unsafe {
        let prepared = reg::prepare(kind);
        if prepared != c::registry_api_result_ok {
            return Err(prepared);
        }
        let guard = s::StateGuard::acquire().ok_or(c::registry_api_result_io)?;
        let view = s::cached(kind).ok_or(c::registry_api_result_hive_not_found)?;
        s::note_read();
        Ok((guard, view))
    }
}
unsafe fn value_info(out: *mut c::RegistryValueInfo, hive: t::View, value: t::ValueRecord) {
    unsafe {
        let mut info = c::RegistryValueInfo {
            value_type: value.value_type,
            data_len: value.data_len,
            ..Default::default()
        };
        path::fixed_z(&mut info.name, f::value_name(hive, value));
        out.write(info);
    }
}
unsafe fn name_out(name: &[u8], out: *mut u8, capacity: u32) -> i32 {
    unsafe {
        if capacity == 0 || capacity as usize <= name.len() {
            return c::registry_api_result_buffer_too_small;
        }
        if !name.is_empty() {
            ptr::copy_nonoverlapping(name.as_ptr(), out, name.len());
        }
        out.add(name.len()).write(0);
        name.len() as i32
    }
}
/// # Safety
/// Original readable NUL-terminated path and exclusive public output, including
/// their demand-page lifetimes through preparation and resident cache lookup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_key_info(
    input: *const u8,
    out: *mut c::RegistryKeyInfo,
) -> i32 {
    unsafe {
        out.write(Default::default());
        let mut buffer = path::Buffer::<256>::new();
        let path = some!(
            path::copy_z(input, &mut buffer),
            c::registry_api_result_bad_path
        );
        let kind = match kind(path) {
            Ok(kind) => kind,
            Err(error) => return error,
        };
        let (_guard, hive) = match ready(kind) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let index = some!(
            f::find_key(hive, path),
            c::registry_api_result_key_not_found
        );
        let key = f::key_at(hive, index);
        (*out).child_count = key.child_count;
        (*out).value_count = key.value_count;
        (*out).name = [0; c::registry_name_max];
        if index != 0 {
            path::fixed_z(&mut (*out).name, f::key_name(hive, key));
        }
        c::registry_api_result_ok
    }
}
/// # Safety
/// Original path, bounded exclusive name buffer and cache input lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_enum_key(
    input: *const u8,
    index: u32,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe {
        let mut buffer = path::Buffer::<256>::new();
        let path = some!(
            path::copy_z(input, &mut buffer),
            c::registry_api_result_bad_path
        );
        let kind = match kind(path) {
            Ok(kind) => kind,
            Err(error) => return error,
        };
        let (_guard, hive) = match ready(kind) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let owner = some!(
            f::find_key(hive, path),
            c::registry_api_result_key_not_found
        );
        let key = f::key_at(hive, owner);
        if index >= key.child_count {
            return c::registry_api_result_key_not_found;
        }
        name_out(
            f::key_name(hive, f::key_at(hive, key.first_child_index + index)),
            out,
            capacity,
        )
    }
}
/// # Safety
/// Original readable path and exclusive fixed public value-info storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_enum_value(
    input: *const u8,
    index: u32,
    out: *mut c::RegistryValueInfo,
) -> i32 {
    unsafe {
        out.write(Default::default());
        let mut buffer = path::Buffer::<256>::new();
        let path = some!(
            path::copy_z(input, &mut buffer),
            c::registry_api_result_bad_path
        );
        let kind = match kind(path) {
            Ok(kind) => kind,
            Err(error) => return error,
        };
        let (_guard, hive) = match ready(kind) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let owner = some!(
            f::find_key(hive, path),
            c::registry_api_result_key_not_found
        );
        let key = f::key_at(hive, owner);
        if index >= key.value_count {
            return c::registry_api_result_value_not_found;
        }
        value_info(out, hive, f::value_at(hive, key.first_value_index + index));
        c::registry_api_result_ok
    }
}
/// # Safety
/// Original immutable path/name and exclusive info/data buffers last through
/// preparation and lookup; the data result never outlives its cache owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_get(
    input: *const u8,
    name: *const u8,
    info: *mut c::RegistryValueInfo,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe {
        info.write(Default::default());
        let mut buffer = path::Buffer::<256>::new();
        let mut name_buffer = path::Buffer::<{ c::registry_name_max }>::new();
        let path = some!(
            path::copy_z(input, &mut buffer),
            c::registry_api_result_bad_path
        );
        let name = some!(
            path::copy_z(name, &mut name_buffer),
            c::registry_api_result_invalid
        );
        let kind = match kind(path) {
            Ok(kind) => kind,
            Err(error) => return error,
        };
        let (_guard, hive) = match ready(kind) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let owner = some!(
            f::find_key(hive, path),
            c::registry_api_result_key_not_found
        );
        let value = some!(
            f::find_value(hive, owner, name),
            c::registry_api_result_value_not_found
        );
        let value = f::value_at(hive, value);
        value_info(info, hive, value);
        let data = f::value_data(hive, value);
        if (capacity as usize) < data.len() {
            return c::registry_api_result_buffer_too_small;
        }
        if !data.is_empty() {
            ptr::copy_nonoverlapping(data.as_ptr(), out, data.len());
        }
        data.len() as i32
    }
}
/// # Safety
/// Readable original path/name and bounded immutable payload, retained until
/// the exact transaction has committed or published its original deferred state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_set(
    input: *const u8,
    name: *const u8,
    value_type: u16,
    data: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        let mut buffer = path::Buffer::<256>::new();
        let mut name_buffer = path::Buffer::<{ c::registry_name_max }>::new();
        let path = some!(
            path::copy_z(input, &mut buffer),
            c::registry_api_result_bad_path
        );
        let name = some!(
            path::copy_z(name, &mut name_buffer),
            c::registry_api_result_invalid
        );
        let kind = match kind(path) {
            Ok(kind) => kind,
            Err(error) => return error,
        };
        if !f::type_valid(value_type) {
            return c::registry_api_result_invalid;
        }
        reg::mutate(
            false,
            kind,
            path,
            name,
            value_type,
            t::Bytes {
                data,
                len: len as usize,
            }
            .slice(),
        )
    }
}
/// # Safety
/// Original readable NUL-terminated path/name, retained across transaction waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_delete(input: *const u8, name: *const u8) -> i32 {
    unsafe {
        let mut buffer = path::Buffer::<256>::new();
        let mut name_buffer = path::Buffer::<{ c::registry_name_max }>::new();
        let path = some!(
            path::copy_z(input, &mut buffer),
            c::registry_api_result_bad_path
        );
        let name = some!(
            path::copy_z(name, &mut name_buffer),
            c::registry_api_result_invalid
        );
        let kind = match kind(path) {
            Ok(kind) => kind,
            Err(error) => return error,
        };
        reg::mutate(true, kind, path, name, 5, &[])
    }
}
fn shape(cursor: &c::RegistrySnapshotCursor) -> bool {
    cursor.version == u32::from(c::registry_snapshot_version)
        && cursor.size >= size_of::<c::RegistrySnapshotCursor>() as u32
}
fn valid(cursor: &c::RegistrySnapshotCursor) -> bool {
    shape(cursor)
        && cursor.generation != 0
        && cursor.flags & c::registry_snapshot_cursor_flag_initialized != 0
        && matches!(
            cursor.kind,
            c::registry_snapshot_kind_keys | c::registry_snapshot_kind_values
        )
}
/// # Safety
/// Nullable inputs retain the public validation contract; nonnull path and
/// exclusive cursor are readable/writable for this whole bounded operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_snapshot_begin(
    input: *const u8,
    selected: u32,
    cursor: *mut c::RegistrySnapshotCursor,
) -> i32 {
    unsafe {
        if input.is_null()
            || cursor.is_null()
            || !matches!(
                selected,
                c::registry_snapshot_kind_keys | c::registry_snapshot_kind_values
            )
        {
            return c::registry_api_result_invalid;
        }
        let restarts = if shape(&*cursor) {
            (*cursor).restarts
        } else {
            0
        };
        let mut buffer = path::Buffer::<256>::new();
        let path = some!(
            path::copy_z(input, &mut buffer),
            c::registry_api_result_bad_path
        );
        let kind = match kind(path) {
            Ok(kind) => kind,
            Err(error) => return error,
        };
        let (_guard, hive) = match ready(kind) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let index = some!(
            f::find_key(hive, path),
            c::registry_api_result_key_not_found
        );
        let key = f::key_at(hive, index);
        cursor.write(c::RegistrySnapshotCursor {
            version: u32::from(c::registry_snapshot_version),
            size: size_of::<c::RegistrySnapshotCursor>() as u32,
            generation: hive.header.generation,
            key_index: index,
            kind: selected,
            next_index: 0,
            total: if selected == c::registry_snapshot_kind_keys {
                key.child_count
            } else {
                key.value_count
            },
            flags: c::registry_snapshot_cursor_flag_initialized,
            restarts,
            ..Default::default()
        });
        c::registry_api_result_ok
    }
}
/// # Safety
/// Original nullable validation and disjoint caller cursor/entry/data/page
/// buffers. A validated cache generation is retained only by the ranked owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_snapshot_page(
    cursor: *mut c::RegistrySnapshotCursor,
    entries: *mut c::RegistrySnapshotEntry,
    entry_capacity: u32,
    data: *mut u8,
    data_capacity: u32,
    page: *mut c::RegistrySnapshotPageInfo,
) -> i32 {
    unsafe {
        if cursor.is_null() || entries.is_null() || data.is_null() || page.is_null() {
            return c::registry_api_result_invalid;
        }
        page.write(c::RegistrySnapshotPageInfo {
            version: u32::from(c::registry_snapshot_version),
            size: size_of::<c::RegistrySnapshotPageInfo>() as u32,
            generation: (*cursor).generation,
            total: (*cursor).total,
            next_index: (*cursor).next_index,
            kind: (*cursor).kind,
            status: c::registry_snapshot_status_invalid,
            ..Default::default()
        });
        if !valid(&*cursor)
            || entry_capacity == 0
            || entry_capacity > c::registry_snapshot_page_max
            || data_capacity > c::registry_snapshot_data_max
        {
            return c::registry_api_result_invalid;
        }
        let (_guard, hive) = match ready(1) {
            Ok(value) => value,
            Err(error) => return error,
        };
        if hive.header.generation != (*cursor).generation {
            (*cursor).restarts = (*cursor).restarts.saturating_add(1);
            (*page).status = c::registry_snapshot_status_restart;
            return c::registry_api_result_ok;
        }
        if (*cursor).key_index >= hive.header.key_count || (*cursor).next_index > (*cursor).total {
            (*page).status = c::registry_snapshot_status_invalid;
            return c::registry_api_result_invalid;
        }
        let key = f::key_at(hive, (*cursor).key_index);
        let total = if (*cursor).kind == c::registry_snapshot_kind_keys {
            key.child_count
        } else {
            key.value_count
        };
        if total != (*cursor).total {
            (*cursor).restarts = (*cursor).restarts.saturating_add(1);
            (*page).status = c::registry_snapshot_status_restart;
            return c::registry_api_result_ok;
        }
        let returned = ((*cursor).total - (*cursor).next_index).min(entry_capacity);
        let mut used = 0;
        for relative in 0..returned {
            let entry = entries.add(relative as usize);
            entry.write(Default::default());
            let index = (*cursor).next_index + relative;
            if (*cursor).kind == c::registry_snapshot_kind_keys {
                if key.child_count != 0 && key.first_child_index == t::INVALID {
                    return c::registry_api_result_hive_corrupt;
                }
                let child = f::key_at(hive, key.first_child_index + index);
                (*entry).kind = c::registry_snapshot_kind_keys as u16;
                path::fixed_z(&mut (*entry).name, f::key_name(hive, child));
            } else {
                if key.value_count != 0 && key.first_value_index == t::INVALID {
                    return c::registry_api_result_hive_corrupt;
                }
                let value = f::value_at(hive, key.first_value_index + index);
                let bytes = f::value_data(hive, value);
                (*entry).kind = c::registry_snapshot_kind_values as u16;
                (*entry).value_type = value.value_type;
                (*entry).data_len = value.data_len;
                path::fixed_z(&mut (*entry).name, f::value_name(hive, value));
                if bytes.len() <= data_capacity as usize - used {
                    (*entry).flags = c::registry_snapshot_entry_flag_data_present;
                    (*entry).data_offset = used as u32;
                    if !bytes.is_empty() {
                        ptr::copy_nonoverlapping(bytes.as_ptr(), data.add(used), bytes.len());
                    }
                    used += bytes.len();
                } else {
                    (*entry).flags = c::registry_snapshot_entry_flag_data_omitted;
                }
            }
        }
        (*cursor).next_index += returned;
        page.write(c::RegistrySnapshotPageInfo {
            version: u32::from(c::registry_snapshot_version),
            size: size_of::<c::RegistrySnapshotPageInfo>() as u32,
            generation: (*cursor).generation,
            total: (*cursor).total,
            returned,
            next_index: (*cursor).next_index,
            data_bytes: used as u32,
            kind: (*cursor).kind,
            status: if (*cursor).next_index < (*cursor).total {
                c::registry_snapshot_status_more
            } else {
                c::registry_snapshot_status_complete
            },
            ..Default::default()
        });
        c::registry_api_result_ok
    }
}
