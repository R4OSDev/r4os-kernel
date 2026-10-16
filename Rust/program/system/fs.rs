//! Typed calls into the already native VFS/request/storage owners. Original
//! request guards remain caller-local, with their exact completion/unwind token.
pub(super) use crate::fs::vfs_types::{Args, Bytes, Entry, Result as Output, Volume, op};
use crate::{
    fs::{request, vfs},
    storage::access_state::Reference,
};
use core::{mem::MaybeUninit, ptr};
pub(super) unsafe fn run(operation: u8, args: Args, out: *mut Entry) -> Output {
    unsafe { vfs::r4os_rust_vfs_run(operation, &args, out) }
}
pub(super) fn bytes(value: &[u8]) -> Bytes {
    Bytes {
        data: value.as_ptr(),
        len: value.len(),
    }
}
pub(super) unsafe fn volume(letter: u8) -> Option<Volume> {
    unsafe {
        let mut out = MaybeUninit::<Volume>::uninit();
        if vfs::r4os_rust_vfs_mount(
            if letter == 0 { 3 } else { 4 },
            letter,
            ptr::null(),
            ptr::null(),
            out.as_mut_ptr().cast(),
        ) {
            Some(out.assume_init())
        } else {
            None
        }
    }
}
pub(super) unsafe fn scalar(volume: Volume, operation: u8) -> u64 {
    unsafe { vfs::r4os_rust_vfs_volume(operation, &volume) }
}
pub(super) unsafe fn resolve(volume: Volume, path: &[u8], out: &mut Entry) -> i32 {
    unsafe {
        run(
            op::RESOLVE_ENTRY,
            Args {
                volume,
                name: bytes(path),
                ..Default::default()
            },
            out,
        )
        .status
    }
}
pub(super) unsafe fn parent(volume: Volume, path: &[u8], out: &mut u64) -> i32 {
    unsafe {
        let result = run(
            op::RESOLVE_PATH,
            Args {
                volume,
                name: bytes(path),
                ..Default::default()
            },
            ptr::null_mut(),
        );
        if result.status == 0 {
            *out = result.value;
        }
        result.status
    }
}
pub(super) unsafe fn lookup(
    volume: Volume,
    parent: u64,
    name: &[u8],
    recovery: bool,
    out: &mut Entry,
) -> i32 {
    unsafe {
        run(
            if recovery {
                op::RECOVERY_LOOKUP
            } else {
                op::LOOKUP
            },
            Args {
                volume,
                parent,
                name: bytes(name),
                ..Default::default()
            },
            out,
        )
        .status
    }
}
pub(super) unsafe fn optional(volume: Volume, path: &[u8], out: &mut Option<Entry>) -> i32 {
    unsafe {
        let mut entry = Entry::EMPTY;
        let status = resolve(volume, path, &mut entry);
        *out = if status == 0 { Some(entry) } else { None };
        status
    }
}
pub(super) unsafe fn read(
    volume: Volume,
    entry: &Entry,
    offset: u64,
    out: *mut u8,
    len: usize,
    range: bool,
) -> Option<usize> {
    unsafe {
        let result = run(
            if range { op::READ_RANGE } else { op::READ },
            Args {
                volume,
                entry,
                offset,
                buffer: out,
                buffer_len: len,
                ..Default::default()
            },
            ptr::null_mut(),
        );
        if result.present {
            Some(result.value as usize)
        } else {
            None
        }
    }
}
pub(super) unsafe fn write(
    volume: Volume,
    parent: u64,
    name: &[u8],
    data: &[u8],
    append: bool,
) -> bool {
    unsafe {
        run(
            if append { op::APPEND } else { op::WRITE },
            Args {
                volume,
                parent,
                name: bytes(name),
                data: bytes(data),
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
pub(super) unsafe fn delete(volume: Volume, parent: u64, name: &[u8]) -> bool {
    unsafe {
        run(
            op::DELETE,
            Args {
                volume,
                parent,
                name: bytes(name),
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
pub(super) unsafe fn flush(volume: Volume) -> bool {
    unsafe {
        run(
            op::FLUSH,
            Args {
                volume,
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .present
    }
}
pub(super) unsafe fn notify(volume: Volume, parent: u64) {
    unsafe { vfs::r4os_rust_vfs_notify(&volume, parent) }
}
pub(super) unsafe fn identity(volume: Volume, left: &Entry, right: &Entry, operation: u8) -> bool {
    unsafe {
        vfs::r4os_rust_vfs_identity(
            operation,
            &Args {
                volume,
                entry: left,
                peer: right,
                ..Default::default()
            },
        )
    }
}
pub(super) unsafe fn replace(
    volume: Volume,
    parent: u64,
    target: &[u8],
    staged: &[u8],
    backup: &[u8],
    create_only: bool,
    consume: bool,
) -> i32 {
    unsafe {
        run(
            if create_only {
                op::REPLACE_CREATE
            } else {
                op::REPLACE
            },
            Args {
                volume,
                parent,
                name: bytes(target),
                second: bytes(staged),
                third: bytes(backup),
                flag: consume,
                ..Default::default()
            },
            ptr::null_mut(),
        )
        .status
    }
}
pub(super) struct Request {
    pub guard: request::Guard,
    pub ok: bool,
}
impl Request {
    pub unsafe fn begin(
        kind: u32,
        first: u8,
        second: u8,
        selection: u8,
        immediate: bool,
        volumes: &[Volume],
    ) -> Option<Self> {
        unsafe {
            assert!(volumes.len() <= 2);
            let mut refs = [request::BoundMount {
                reference: Reference::default(),
                present: false,
            }; 2];
            for (output, volume) in refs.iter_mut().zip(volumes) {
                output.reference = volume.reference;
                output.present = volume.has_ref;
            }
            let mut guard = MaybeUninit::uninit();
            if request::r4os_rust_fs_request_begin(
                kind,
                first,
                second,
                selection,
                immediate,
                refs.as_ptr(),
                volumes.len(),
                !volumes.is_empty(),
                guard.as_mut_ptr(),
            ) {
                Some(Self {
                    guard: guard.assume_init(),
                    ok: false,
                })
            } else {
                None
            }
        }
    }
    /// One existing ordered request covers the destination and the durable
    /// claim catalog on C:. No additional request opens under a target gate.
    pub unsafe fn claims(kind: u32, letter: u8, volume: Volume) -> Option<Self> {
        unsafe {
            let Some(system) = crate::fs::vfs::mounted_volume(b'C') else {
                return Self::volume(kind, letter, volume);
            };
            // The ordinary C: path already retains this exact mounted reference.
            let same = vfs::r4os_rust_vfs_identity(
                0,
                &Args {
                    volume,
                    other: system,
                    ..Default::default()
                },
            );
            if letter == b'C' && same {
                return Self::volume(kind, letter, volume);
            }
            Self::begin(kind, letter, b'C', 1, false, &[volume, system])
        }
    }
    pub unsafe fn volume(kind: u32, letter: u8, volume: Volume) -> Option<Self> {
        unsafe { Self::begin(kind, letter, 0, 0, false, &[volume]) }
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        unsafe { request::r4os_rust_fs_request_finish(&mut self.guard, self.ok) };
    }
}
