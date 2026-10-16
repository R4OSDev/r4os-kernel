//! Registry transaction, commit/readback, uncertain-retry and deferred-write
//! orchestration. The transaction gate pins stable buffers, never a Rust
//! reference or ranked registry mutex, across filesystem work.
use super::super::{registry_format as f, registry_format_types::Bytes};
use super::{registry_build as build, registry_state as s};
use crate::contract as c;
use core::ptr;
unsafe fn file(operation: u8, path: &[u8], data: *mut u8, len: usize) -> i32 {
    unsafe {
        match operation {
            0 => super::io::r4os_rust_sys_file_read(path.as_ptr(), data, len as u32),
            1 => super::io::r4os_rust_sys_file_write(path.as_ptr(), data, len as u32),
            2 => super::namespace::r4os_rust_sys_file_delete(path.as_ptr()),
            3 => super::namespace::r4os_rust_sys_dir_create(path.as_ptr()),
            _ => unreachable!(),
        }
    }
}
#[derive(Clone, Copy)]
pub struct Commit {
    pub result: i32,
    pub uncertain: bool,
}
impl Commit {
    fn io(uncertain: bool) -> Self {
        Self {
            result: c::registry_api_result_io,
            uncertain,
        }
    }
}
pub(super) unsafe fn commit(index: u8, prepared: bool) -> Commit {
    unsafe {
        let (kind, bytes) = {
            let Some(_guard) = s::StateGuard::acquire() else {
                return Commit::io(false);
            };
            if index >= 2 || !(*s::slot(index)).valid {
                return Commit::io(false);
            }
            let slot = s::slot(index);
            (
                (*slot).kind,
                (*slot).view.expect("Valid registry candidate").bytes,
            )
        };
        let mut sys = [0u8; 1024];
        let mut directory = [0u8; 1024];
        super::path::write_z(&mut sys, b"C:\\R4OS");
        super::path::write_z(&mut directory, b"C:\\R4OS\\REGISTRY");
        file(3, &sys, ptr::null_mut(), 0);
        file(3, &directory, ptr::null_mut(), 0);
        if kind != 1 {
            return Commit {
                result: c::registry_api_result_bad_path,
                uncertain: false,
            };
        }
        let mut tmp = [0u8; 1024];
        let mut hive = [0u8; 1024];
        let mut bak = [0u8; 1024];
        super::path::write_z(&mut tmp, &s::TMP_PATH[..s::TMP_PATH.len() - 1]);
        super::path::write_z(&mut hive, s::HIVE_PATH);
        super::path::write_z(&mut bak, &s::BAK_PATH[..s::BAK_PATH.len() - 1]);
        let verify = ptr::addr_of_mut!((*s::state()).verify).cast::<u8>();
        if !prepared {
            if file(2, &tmp, ptr::null_mut(), 0) < 0 || file(2, &bak, ptr::null_mut(), 0) < 0 {
                return Commit::io(false);
            }
            let written = file(1, &tmp, bytes.data.cast_mut(), bytes.len);
            if written < 0 || written as usize != bytes.len {
                return Commit::io(false);
            }
            let read = file(0, &tmp, verify, s::HIVE_BYTES);
            if read < 0
                || read as usize != bytes.len
                || core::slice::from_raw_parts(verify, read as usize) != bytes.slice()
            {
                return Commit::io(false);
            }
        }
        let mut replaced = super::atomic::r4os_rust_sys_file_replace(
            hive.as_ptr(),
            tmp.as_ptr(),
            bak.as_ptr(),
            1,
            false,
        );
        if replaced == -7 {
            s::note(2);
            replaced = super::atomic::r4os_rust_sys_file_replace(
                hive.as_ptr(),
                tmp.as_ptr(),
                bak.as_ptr(),
                1,
                false,
            );
        }
        if replaced != 0 {
            return Commit::io(replaced == -7);
        }
        let read = file(0, &hive, verify, s::HIVE_BYTES);
        if read < 0
            || read as usize != bytes.len
            || core::slice::from_raw_parts(verify, read as usize) != bytes.slice()
        {
            return Commit::io(true);
        }
        s::note(1);
        Commit {
            result: c::registry_api_result_ok,
            uncertain: false,
        }
    }
}
pub(super) unsafe fn flush_under_transaction() -> i32 {
    unsafe {
        let active = {
            let Some(_guard) = s::StateGuard::acquire() else {
                return c::registry_api_result_io;
            };
            let active = (*s::state()).active;
            if active == s::NONE || !(*s::slot(active)).valid || !(*s::slot(active)).dirty {
                return c::registry_api_result_ok;
            }
            active
        };
        let committed = commit(active, false);
        if committed.result != c::registry_api_result_ok {
            s::failure(active, committed.uncertain);
            return committed.result;
        }
        let Some(_guard) = s::StateGuard::acquire() else {
            return c::registry_api_result_io;
        };
        (*s::slot(active)).dirty = false;
        (*s::state()).pending = s::NONE;
        c::registry_api_result_ok
    }
}
pub(super) unsafe fn resume() -> i32 {
    unsafe {
        let pending = {
            let Some(_guard) = s::StateGuard::acquire() else {
                return c::registry_api_result_io;
            };
            (*s::state()).pending
        };
        if pending == s::NONE {
            return c::registry_api_result_ok;
        }
        let committed = commit(pending, true);
        if committed.result != c::registry_api_result_ok {
            s::failure(pending, true);
            return committed.result;
        }
        let Some(_guard) = s::StateGuard::acquire() else {
            return c::registry_api_result_io;
        };
        if (*s::state()).active == pending {
            (*s::slot(pending)).dirty = false;
            (*s::state()).pending = s::NONE;
        } else {
            s::publish_locked(pending, false);
        }
        c::registry_api_result_ok
    }
}
pub(super) unsafe fn ensure(kind: u16) -> i32 {
    unsafe {
        let index = {
            let Some(_guard) = s::StateGuard::acquire() else {
                return c::registry_api_result_io;
            };
            if s::cached(kind).is_some() {
                return c::registry_api_result_ok;
            }
            let index = s::inactive();
            (*s::slot(index)).invalidate(true);
            index
        };
        if kind != 1 {
            return c::registry_api_result_bad_path;
        }
        let mut path = [0u8; 1024];
        super::path::write_z(&mut path, s::HIVE_PATH);
        let output = ptr::addr_of_mut!((*s::slot(index)).bytes).cast::<u8>();
        let read = file(0, &path, output, s::HIVE_BYTES);
        s::note(0);
        if read == -5 {
            return c::registry_api_result_buffer_too_small;
        }
        if read == -3 {
            return c::registry_api_result_hive_not_found;
        }
        if read < 0 {
            return c::registry_api_result_io;
        }
        let view = match f::parse(Bytes {
            data: output,
            len: read as usize,
        }) {
            Ok(view) => view,
            Err(_) => return c::registry_api_result_hive_corrupt,
        };
        let Some(_guard) = s::StateGuard::acquire() else {
            return c::registry_api_result_io;
        };
        let slot = s::slot(index);
        (*slot).valid = true;
        (*slot).dirty = false;
        (*slot).kind = kind;
        (*slot).len = view.bytes.len;
        (*slot).view = Some(view);
        (*s::state()).performance.validation_passes =
            (*s::state()).performance.validation_passes.wrapping_add(1);
        s::publish_locked(index, false);
        c::registry_api_result_ok
    }
}
pub(super) unsafe fn prepare(kind: u16) -> i32 {
    unsafe {
        let ready = {
            let Some(_guard) = s::StateGuard::acquire() else {
                return c::registry_api_result_io;
            };
            let ready = s::cached(kind).is_some() && (*s::state()).pending == s::NONE;
            if ready {
                (*s::state()).performance.cache_hits =
                    (*s::state()).performance.cache_hits.wrapping_add(1);
            }
            ready
        };
        if ready {
            return c::registry_api_result_ok;
        }
        let _transaction = s::Transaction::acquire();
        let resumed = resume();
        if resumed != c::registry_api_result_ok {
            return resumed;
        }
        ensure(kind)
    }
}
/// # Safety
/// Existing shutdown or singleton registry transaction caller. No registry
/// reference/lock remains live while the original file owner may park.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_registry_flush() -> i32 {
    unsafe {
        let _transaction = s::Transaction::acquire();
        let resumed = resume();
        if resumed != c::registry_api_result_ok {
            return resumed;
        }
        flush_under_transaction()
    }
}
pub(super) unsafe fn mutate(
    delete: bool,
    kind: u16,
    path: &[u8],
    name: &[u8],
    value_type: u16,
    data: &[u8],
) -> i32 {
    unsafe {
        if kind != 1 {
            return c::registry_api_result_unsupported;
        }
        let _transaction = s::Transaction::acquire();
        let resumed = resume();
        if resumed != c::registry_api_result_ok {
            return resumed;
        }
        let root = b"SYSTEM\\Shell\\RecentDocuments";
        let deferred = path.len() >= root.len()
            && path[..root.len()].eq_ignore_ascii_case(root)
            && (path.len() == root.len() || path[root.len()] == b'\\');
        if !deferred {
            let result = flush_under_transaction();
            if result != c::registry_api_result_ok {
                return result;
            }
        }
        let loaded = ensure(kind);
        if loaded != c::registry_api_result_ok && loaded != c::registry_api_result_hive_not_found {
            return loaded;
        }
        if delete && loaded == c::registry_api_result_hive_not_found {
            return loaded;
        }
        let candidate = build::mutation(delete, kind, path, name, value_type, data);
        if candidate.result != c::registry_api_result_ok {
            return candidate.result;
        }
        if deferred {
            return s::publish(candidate.slot, true);
        }
        let committed = commit(candidate.slot, false);
        if committed.result == c::registry_api_result_ok {
            return s::publish(candidate.slot, false);
        }
        s::failure(candidate.slot, committed.uncertain);
        committed.result
    }
}
