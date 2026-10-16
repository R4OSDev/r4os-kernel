//! Original bounded mutation candidate assembly. Only the registry state mutex
//! admits these pure scratch operations; no I/O or owner wait occurs here.
use super::super::{
    registry_format as f, registry_format_builder as builder, registry_format_types as t,
};
use super::{path, registry_batch as batch, registry_state as s};
use crate::contract as c;
use core::ptr;
#[derive(Clone, Copy)]
pub struct Candidate {
    pub result: i32,
    pub slot: u8,
    pub generation: u64,
}
impl Candidate {
    fn fail(result: i32) -> Self {
        Self {
            result,
            slot: s::NONE,
            generation: 0,
        }
    }
}
struct List {
    count: usize,
    path_len: usize,
}
impl List {
    fn new() -> Self {
        Self {
            count: 0,
            path_len: 0,
        }
    }
    unsafe fn append(
        &mut self,
        key_path: &[u8],
        name: &[u8],
        value_type: u16,
        data: &[u8],
    ) -> bool {
        unsafe {
            if self.count >= s::VALUES {
                return false;
            }
            (*s::state()).values[self.count] = t::BuildValue {
                key_path: t::Bytes::from_slice(key_path),
                name: t::Bytes::from_slice(name),
                value_type,
                data: t::Bytes::from_slice(data),
            };
            self.count += 1;
            true
        }
    }
    unsafe fn text<'a>(&mut self, text: &[u8]) -> Option<&'a [u8]> {
        unsafe {
            if self.path_len + text.len() > s::PATH_POOL {
                return None;
            }
            let start = self.path_len;
            ptr::copy_nonoverlapping(
                text.as_ptr(),
                ptr::addr_of_mut!((*s::state()).paths)
                    .cast::<u8>()
                    .add(start),
                text.len(),
            );
            self.path_len += text.len();
            Some(core::slice::from_raw_parts(
                ptr::addr_of!((*s::state()).paths).cast::<u8>().add(start),
                text.len(),
            ))
        }
    }
    unsafe fn byte(&mut self, byte: u8) -> bool {
        unsafe {
            if self.path_len >= s::PATH_POOL {
                return false;
            }
            (*s::state()).paths[self.path_len] = byte;
            self.path_len += 1;
            true
        }
    }
}
unsafe fn key_path<'a>(hive: t::View, index: u32, list: &mut List) -> Option<&'a [u8]> {
    unsafe {
        let start = list.path_len;
        list.text(f::root_name(hive.header.hive_kind))?;
        let mut parts = [core::mem::MaybeUninit::<u32>::uninit(); 32];
        let mut count = 0;
        let mut current = index;
        while current != 0 && count < parts.len() {
            parts[count].write(current);
            count += 1;
            current = f::key_at(hive, current).parent_index;
            if current >= hive.header.key_count {
                return None;
            }
        }
        if current != 0 {
            return None;
        }
        while count > 0 {
            count -= 1;
            if !list.byte(b'\\') {
                return None;
            }
            list.text(f::key_name(
                hive,
                f::key_at(hive, parts[count].assume_init()),
            ))?;
        }
        Some(core::slice::from_raw_parts(
            ptr::addr_of!((*s::state()).paths).cast::<u8>().add(start),
            list.path_len - start,
        ))
    }
}
unsafe fn collect(
    hive: t::View,
    list: &mut List,
    skip: Option<u32>,
    name: &[u8],
    operations: &[c::RegistryBatchOperation],
    blob: &[u8],
) -> i32 {
    unsafe {
        for index in 0..hive.header.key_count {
            let key = f::key_at(hive, index);
            if key.value_count == 0 {
                continue;
            }
            let Some(path) = key_path(hive, index, list) else {
                return c::registry_api_result_buffer_too_small;
            };
            for offset in 0..key.value_count {
                let value = f::value_at(hive, key.first_value_index + offset);
                let value_name = f::value_name(hive, value);
                if skip == Some(index) && value_name.eq_ignore_ascii_case(name) {
                    continue;
                }
                if operations.iter().any(|operation| {
                    path::equal_path(
                        path,
                        batch::slice(blob, operation.key_path_offset, operation.key_path_len)
                            .expect("Validated registry batch path"),
                    ) && value_name.eq_ignore_ascii_case(
                        batch::slice(blob, operation.value_name_offset, operation.value_name_len)
                            .expect("Validated registry batch name"),
                    )
                }) {
                    continue;
                }
                if !list.append(
                    path,
                    value_name,
                    value.value_type,
                    f::value_data(hive, value),
                ) {
                    return c::registry_api_result_buffer_too_small;
                }
            }
        }
        c::registry_api_result_ok
    }
}
pub(super) fn error(error: t::Error) -> i32 {
    match error {
        t::Error::InvalidPath | t::Error::RootMismatch => c::registry_api_result_bad_path,
        t::Error::BadName
        | t::Error::BadData
        | t::Error::BadValue
        | t::Error::DuplicateKey
        | t::Error::DuplicateValue => c::registry_api_result_invalid,
        t::Error::OutOfMemory | t::Error::TooManyEntries => c::registry_api_result_buffer_too_small,
        _ => c::registry_api_result_hive_corrupt,
    }
}
unsafe fn finish(
    index: u8,
    kind: u16,
    generation: u64,
    list: &List,
    report_generation: bool,
) -> Candidate {
    unsafe {
        let state = s::state();
        let slot = s::slot(index);
        let result = builder::build(t::BuildInput {
            output: ptr::addr_of_mut!((*slot).bytes).cast(),
            output_capacity: s::HIVE_BYTES,
            keys: ptr::addr_of_mut!((*state).keys).cast(),
            key_capacity: s::KEYS,
            read_key: None,
            write_key: None,
            values: ptr::addr_of!((*state).values).cast(),
            values_len: list.count,
            read_value: None,
            owners: ptr::addr_of_mut!((*state).owners).cast(),
            owners_capacity: s::VALUES,
            flat: ptr::addr_of_mut!((*state).flat).cast(),
            flat_capacity: s::KEYS,
            grouped: ptr::addr_of_mut!((*state).order).cast(),
            grouped_capacity: s::VALUES,
            kind,
            generation,
        });
        let view = match result {
            Ok(view) => view,
            Err(err) => return Candidate::fail(error(err)),
        };
        (*slot).valid = true;
        (*slot).kind = kind;
        (*slot).len = view.bytes.len;
        (*slot).view = Some(view);
        (*state).performance.validation_passes =
            (*state).performance.validation_passes.wrapping_add(1);
        Candidate {
            result: c::registry_api_result_ok,
            slot: index,
            generation: if report_generation { generation } else { 0 },
        }
    }
}
pub(super) unsafe fn mutation(
    delete: bool,
    kind: u16,
    path: &[u8],
    name: &[u8],
    value_type: u16,
    data: &[u8],
) -> Candidate {
    unsafe {
        let Some(_guard) = s::StateGuard::acquire() else {
            return Candidate::fail(c::registry_api_result_io);
        };
        let current = s::cached(kind);
        let index = s::inactive();
        (*s::slot(index)).invalidate(true);
        let mut list = List::new();
        if let Some(hive) = current {
            let target = f::find_key(hive, path);
            if delete {
                let Some(target) = target else {
                    return Candidate::fail(c::registry_api_result_key_not_found);
                };
                if f::find_value(hive, target, name).is_none() {
                    return Candidate::fail(c::registry_api_result_value_not_found);
                }
            }
            let result = collect(hive, &mut list, target, name, &[], &[]);
            if result != c::registry_api_result_ok {
                return Candidate::fail(result);
            }
        }
        if !delete && !list.append(path, name, value_type, data) {
            return Candidate::fail(c::registry_api_result_buffer_too_small);
        }
        finish(
            index,
            kind,
            current.map_or(1, |hive| s::next_generation(hive.header.generation)),
            &list,
            false,
        )
    }
}
pub(super) unsafe fn batch(
    operations: &[c::RegistryBatchOperation],
    blob: &[u8],
    result: &mut c::RegistryBatchResult,
) -> Candidate {
    unsafe {
        let Some(_guard) = s::StateGuard::acquire() else {
            return Candidate::fail(c::registry_api_result_io);
        };
        let current = s::cached(1);
        result.generation_before = current.map_or(0, |hive| hive.header.generation);
        result.generation_after = result.generation_before;
        for (index, operation) in operations.iter().enumerate() {
            if operation.operation != c::registry_batch_operation_delete {
                continue;
            }
            let error = match current {
                None => Some(c::registry_api_result_hive_not_found),
                Some(hive) => {
                    let path =
                        batch::slice(blob, operation.key_path_offset, operation.key_path_len)
                            .expect("Validated registry batch path");
                    let name =
                        batch::slice(blob, operation.value_name_offset, operation.value_name_len)
                            .expect("Validated registry batch name");
                    match f::find_key(hive, path) {
                        None => Some(c::registry_api_result_key_not_found),
                        Some(key) => {
                            if f::find_value(hive, key, name).is_none() {
                                Some(c::registry_api_result_value_not_found)
                            } else {
                                None
                            }
                        }
                    }
                }
            };
            if let Some(error) = error {
                result.failed_index = index as u32;
                return Candidate::fail(error);
            }
        }
        let index = s::inactive();
        (*s::slot(index)).invalidate(true);
        let mut list = List::new();
        if let Some(hive) = current {
            let code = collect(hive, &mut list, None, &[], operations, blob);
            if code != c::registry_api_result_ok {
                return Candidate::fail(code);
            }
        }
        for operation in operations {
            if operation.operation != c::registry_batch_operation_set {
                continue;
            }
            if !list.append(
                batch::slice(blob, operation.key_path_offset, operation.key_path_len)
                    .expect("Validated batch path"),
                batch::slice(blob, operation.value_name_offset, operation.value_name_len)
                    .expect("Validated batch name"),
                operation.value_type,
                batch::slice(blob, operation.data_offset, operation.data_len)
                    .expect("Validated batch payload"),
            ) {
                return Candidate::fail(c::registry_api_result_buffer_too_small);
            }
        }
        finish(
            index,
            1,
            current.map_or(1, |hive| s::next_generation(hive.header.generation)),
            &list,
            true,
        )
    }
}
