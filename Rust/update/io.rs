//! Typed transport to the existing native VFS owner. Callers retain their
//! original namespace admission; these helpers do not open another request.
use crate::fs::{
    vfs,
    vfs_types::{Args, Bytes, Entry, Result as Receipt, Volume, op},
};
use core::{mem::MaybeUninit, ptr};
#[derive(Clone, Copy, Debug)]
pub(super) struct Fault;
/// # Safety
/// Caller retains every selected input/output span for its synchronous operation.
pub(super) unsafe fn run(op: u8, args: Args, out: *mut Entry) -> Receipt {
    unsafe { vfs::r4os_rust_vfs_run(op, &args, out) }
}
pub(super) fn resolve(v: Volume, path: Bytes) -> Result<Option<u64>, Fault> {
    unsafe {
        let r = run(
            op::RESOLVE_PATH,
            Args {
                volume: v,
                name: path,
                ..Args::default()
            },
            ptr::null_mut(),
        );
        match r.status {
            0 => Ok(Some(r.value)),
            1 => Ok(None),
            2 => Err(Fault),
            _ => panic!("Invalid VFS path receipt"),
        }
    }
}
pub(super) fn lookup(
    v: Volume,
    parent: u64,
    name: Bytes,
    recovery: bool,
) -> Result<Option<Entry>, Fault> {
    unsafe {
        let mut e = MaybeUninit::uninit();
        let r = run(
            if recovery {
                op::RECOVERY_LOOKUP
            } else {
                op::LOOKUP
            },
            Args {
                volume: v,
                parent,
                name,
                ..Args::default()
            },
            e.as_mut_ptr(),
        );
        match r.status {
            0 => Ok(Some(e.assume_init())),
            1 => Ok(None),
            2 => Err(Fault),
            _ => panic!("Invalid VFS lookup receipt"),
        }
    }
}
pub(super) fn resolve_entry(v: Volume, path: Bytes) -> Result<Option<Entry>, Fault> {
    unsafe {
        let mut e = MaybeUninit::uninit();
        let r = run(
            op::RESOLVE_ENTRY,
            Args {
                volume: v,
                name: path,
                ..Args::default()
            },
            e.as_mut_ptr(),
        );
        match r.status {
            0 => Ok(Some(e.assume_init())),
            1 => Ok(None),
            2 => Err(Fault),
            _ => panic!("Invalid VFS entry receipt"),
        }
    }
}
pub(super) fn flush(v: Volume) -> bool {
    unsafe {
        run(
            op::FLUSH,
            Args {
                volume: v,
                ..Args::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
pub(super) fn mkdir(v: Volume, parent: u64, name: Bytes) -> bool {
    unsafe {
        run(
            op::MKDIR,
            Args {
                volume: v,
                parent,
                name,
                ..Args::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
pub(super) fn write(v: Volume, parent: u64, name: Bytes, data: Bytes) -> bool {
    unsafe {
        run(
            op::WRITE,
            Args {
                volume: v,
                parent,
                name,
                data,
                ..Args::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
pub(super) fn delete(v: Volume, parent: u64, name: Bytes) -> bool {
    unsafe {
        run(
            op::DELETE,
            Args {
                volume: v,
                parent,
                name,
                ..Args::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
pub(super) fn replace(
    v: Volume,
    parent: u64,
    target: Bytes,
    stage: Bytes,
    backup: Bytes,
    create: bool,
) -> i32 {
    unsafe {
        run(
            if create {
                op::REPLACE_CREATE
            } else {
                op::REPLACE
            },
            Args {
                volume: v,
                parent,
                name: target,
                second: stage,
                third: backup,
                flag: !create,
                ..Args::default()
            },
            ptr::null_mut(),
        )
        .status
    }
}
pub(super) fn delete_identity(
    v: Volume,
    parent: u64,
    name: Bytes,
    e: &Entry,
    recovery: bool,
) -> i32 {
    unsafe {
        run(
            if recovery {
                op::RECOVERY_DELETE
            } else {
                op::DELETE_MATCH
            },
            Args {
                volume: v,
                parent,
                name,
                entry: e,
                ..Args::default()
            },
            ptr::null_mut(),
        )
        .status
    }
}
pub(super) fn same_identity(v: Volume, a: &Entry, b: &Entry) -> bool {
    unsafe {
        vfs::r4os_rust_vfs_identity(
            3,
            &Args {
                volume: v,
                entry: a,
                peer: b,
                ..Args::default()
            },
        )
    }
}
/// # Safety
/// Exact output buffer stays writable through synchronous range I/O.
pub(super) unsafe fn read_range(
    v: Volume,
    e: &Entry,
    offset: u64,
    buffer: *mut u8,
    len: usize,
) -> Option<usize> {
    unsafe {
        let r = run(
            op::READ_RANGE,
            Args {
                volume: v,
                entry: e,
                offset,
                buffer,
                buffer_len: len,
                ..Args::default()
            },
            ptr::null_mut(),
        );
        if r.present {
            Some(usize::try_from(r.value).expect("Original VFS read length"))
        } else {
            None
        }
    }
}

pub(super) fn short_name(name: &[u8]) -> bool {
    unsafe {
        run(
            op::SHORT_NAME,
            Args {
                name: super::claim::span(name),
                ..Args::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
