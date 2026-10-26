//! Original VFS mount/identity/dispatch/copy owner. Exact mounted generations
//! stay under the storage owner; backend waits never retain its table borrow.
use super::vfs_types::{self as t, Args, Entry, Result as Output, Volume, op};
use crate::{
    memory::{heap, owner::Guard},
    storage::{
        access_runtime::r4os_rust_storage_access_runtime,
        access_state::{Input, Mount, Reference, Region},
    },
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr};
#[derive(Clone, Copy)]
struct Slot {
    present: bool,
    volume: Volume,
}
const EMPTY: Slot = Slot {
    present: false,
    volume: Volume {
        kind: 0,
        has_ref: false,
        reference: Reference {
            slot: 0,
            generation: 0,
        },
        fat: t::Fat {
            device: 0,
            partition: 0,
            sector_bytes: 0,
            cluster_sectors: 0,
            reserved: 0,
            fats: 0,
            fat_sectors: 0,
            sectors: 0,
            root: 0,
            fsinfo: 0,
            backup: 0,
            write_batch: 0,
        },
        ntfs: t::Ntfs {
            slot: 0,
            cluster_bytes: 0,
            sectors: 0,
        },
    },
};
struct Table(UnsafeCell<[Slot; 27]>);
// SAFETY: Every table access is serialized by the original storage owner;
// boot reset is BSP-only. Copied values retain their original mount identity.
unsafe impl Sync for Table {}
static MOUNTS: Table = Table(UnsafeCell::new([EMPTY; 27]));

fn drive_index(letter: u8) -> Option<usize> {
    let upper = letter.to_ascii_uppercase();
    if upper.is_ascii_uppercase() {
        Some(usize::from(upper - b'A'))
    } else {
        None
    }
}
unsafe fn access(
    op: u8,
    locked: bool,
    input: Input,
    snapshot: *mut core::ffi::c_void,
) -> Option<u64> {
    let mut out = 0;
    if unsafe { r4os_rust_storage_access_runtime(op, locked, &input, &mut out, snapshot) } == 0 {
        Some(out)
    } else {
        None
    }
}
unsafe fn region(v: Volume) -> Option<Region> {
    if v.has_ref {
        let mut out = MaybeUninit::<Mount>::uninit();
        unsafe {
            access(
                8,
                false,
                Input {
                    reference: v.reference,
                    ..Input::default()
                },
                out.as_mut_ptr().cast(),
            )
        }?;
        return Some(unsafe { out.assume_init() }.region);
    }
    let (device, first, count) = if v.kind == 0 {
        (
            v.fat.device,
            u64::from(v.fat.partition),
            u64::from(v.fat.sectors),
        )
    } else {
        let mut out = t::Location::default();
        if !unsafe { crate::fs::vfs_backends::location(&v, &mut out) } {
            return None;
        }
        (out.device, out.first, out.count)
    };
    let slot = u32::try_from(device).expect("device slot");
    let generation = unsafe {
        access(
            20,
            false,
            Input {
                reference: Reference {
                    slot,
                    generation: 0,
                },
                ..Input::default()
            },
            ptr::null_mut(),
        )
    }?;
    Some(Region {
        device: Reference { slot, generation },
        first,
        count,
    })
}
/// # Safety
/// Existing boot/mount/storage-transaction contexts. Input/output resident
/// values are disjoint; unmount follows authenticated I/O drain. No table
/// reference crosses a wait, filesystem callback, or desktop notification.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vfs_mount(
    operation: u8,
    letter: u8,
    input: *const Volume,
    reference: *const Reference,
    output: *mut core::ffi::c_void,
) -> bool {
    match operation {
        0 => {
            unsafe { MOUNTS.0.get().write([EMPTY; 27]) };
            true
        }
        1 | 2 => {
            let index = if operation == 1 {
                26
            } else {
                let Some(index) = drive_index(letter) else {
                    return false;
                };
                index
            };
            let mut value = unsafe { input.read() };
            let Some(region) = (unsafe { region(value) }) else {
                return false;
            };
            let _guard = Guard::acquire(1);
            let slot = unsafe { MOUNTS.0.get().cast::<Slot>().add(index) };
            if unsafe { (*slot).present } {
                return false;
            }
            let Some(generation) = (unsafe {
                access(
                    7,
                    true,
                    Input {
                        reference: Reference {
                            slot: index as u32,
                            generation: 0,
                        },
                        region,
                        value: u64::from(index == 2 || index == 26),
                        ..Input::default()
                    },
                    ptr::null_mut(),
                )
            }) else {
                return false;
            };
            value.reference = Reference {
                slot: index as u32,
                generation,
            };
            value.has_ref = true;
            unsafe {
                slot.write(Slot {
                    present: true,
                    volume: value,
                })
            };
            true
        }
        3 | 4 => {
            let index = if operation == 3 {
                26
            } else {
                let Some(index) = drive_index(letter) else {
                    return false;
                };
                index
            };
            let _guard = Guard::acquire(1);
            let slot = unsafe { MOUNTS.0.get().cast::<Slot>().add(index).read() };
            if !slot.present {
                return false;
            }
            unsafe { output.cast::<Volume>().write(slot.volume) };
            true
        }
        5 => {
            let result = {
                let _guard = Guard::acquire(1);
                let reference = unsafe { reference.read() };
                if unsafe {
                    access(
                        9,
                        true,
                        Input {
                            reference,
                            ..Input::default()
                        },
                        ptr::null_mut(),
                    )
                }
                .is_none()
                {
                    false
                } else {
                    assert!(reference.slot <= 26);
                    unsafe {
                        MOUNTS
                            .0
                            .get()
                            .cast::<Slot>()
                            .add(reference.slot as usize)
                            .write(EMPTY)
                    };
                    if reference.slot != 26 {
                        unsafe {
                            super::drive::r4os_rust_fs_drive_control(2, b'A' + reference.slot as u8)
                        };
                    }
                    true
                }
            };
            crate::program::gui_activity::notify(crate::contract::gui_activity_shared_directory);
            result
        }
        6 => {
            let Some(value) = (unsafe { region(input.read()) }) else {
                return false;
            };
            // Region output has its own exact fixed C layout.
            unsafe { output.cast::<Region>().write(value) };
            true
        }
        _ => unreachable!(),
    }
}
/// # Safety
/// Copied initialized C volume. Scalar queries perform no backend I/O;
/// mount-ref extraction in the facade is a representation projection only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vfs_volume(operation: u8, input: *const Volume) -> u64 {
    let v = unsafe { input.read() };
    match operation {
        0 => {
            if v.kind == 0 {
                u64::from(v.fat.sector_bytes) * u64::from(v.fat.cluster_sectors)
            } else {
                u64::from(v.ntfs.cluster_bytes)
            }
        }
        1 => {
            if v.kind == 0 {
                let metadata =
                    u32::from(v.fat.reserved) + u32::from(v.fat.fats) * v.fat.fat_sectors;
                if v.fat.sectors <= metadata || v.fat.cluster_sectors == 0 {
                    0
                } else {
                    u64::from((v.fat.sectors - metadata) / u32::from(v.fat.cluster_sectors))
                }
            } else {
                u64::from(
                    u32::try_from((v.ntfs.sectors * 512) / u64::from(v.ntfs.cluster_bytes))
                        .expect("NTFS cluster count"),
                )
            }
        }
        2 => {
            if v.kind == 0 {
                u64::from(v.fat.root)
            } else {
                5
            }
        }
        3 => 0,
        _ => unreachable!(),
    }
}
fn same_volume(a: Volume, b: Volume) -> bool {
    if a.kind != b.kind {
        return false;
    }
    if a.kind == 0 {
        a.fat.device == b.fat.device && a.fat.partition == b.fat.partition
    } else {
        a.ntfs.slot == b.ntfs.slot
    }
}
fn same_file(v: Volume, a: Entry, b: Entry) -> bool {
    if a.is_dir() || b.is_dir() {
        return false;
    }
    if v.kind == 0 {
        a.size != 0 && a.node != 0 && a.node == b.node && a.size == b.size
    } else {
        a.node == b.node && a.generation == b.generation
    }
}
/// # Safety
/// Exact initialized neutral entries and volume are readable through this
/// bounded query. Backend-specific empty-file copy identity stays at FAT.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vfs_identity(operation: u8, args: *const Args) -> bool {
    let a = unsafe { args.read() };
    if operation == 0 {
        return same_volume(a.volume, a.other);
    }
    let left = unsafe { a.entry.read() };
    let right = unsafe { a.peer.read() };
    match operation {
        1 => same_file(a.volume, left, right),
        2 => {
            if a.volume.kind == 0 {
                unsafe { backend(op::SAME_COPY, a, ptr::null_mut()) }.present
            } else {
                same_file(a.volume, left, right)
            }
        }
        3 => {
            same_file(a.volume, left, right)
                || (a.volume.kind == 0
                    && left.node == 0
                    && right.node == 0
                    && left.generation == 0
                    && right.generation == 0
                    && left.size == 0
                    && right.size == 0
                    && left.attr == right.attr
                    && left.reparse == right.reparse
                    && left.times == right.times)
        }
        _ => unreachable!(),
    }
}
/// # Safety
/// Original caller owns the mounted volume and namespace request. No native
/// reference into a backend or table survives this notification/wake.
unsafe fn notify(v: Volume, parent: u64) {
    if v.has_ref {
        unsafe {
            super::directory_changes::r4os_rust_fs_directory_changes(
                0,
                &v.reference,
                parent,
                ptr::null_mut(),
            )
        };
    }
}
unsafe fn backend(operation: u8, args: Args, output: *mut Entry) -> Output {
    match args.volume.kind {
        0 => unsafe { crate::fs::vfs_backends::fat(operation, &args, output) },
        1 => unsafe { crate::fs::vfs_backends::ntfs(operation, &args, output) },
        _ => unreachable!(),
    }
}

fn append_status(raw: i32, kind: u8) -> i32 {
    if kind == 0 {
        return raw;
    }
    match raw {
        0 => 0,
        2 => 2,
        12 => 3,
        8..=10 => 4,
        13 | 14 => 5,
        1 | 3..=7 | 11 => 1,
        _ => unreachable!(),
    }
}
unsafe fn call(operation: u8, args: Args, output: *mut Entry) -> Output {
    let v = args.volume;
    match operation {
        op::DIAGNOSTIC => {
            if v.kind == 0 {
                Output::value(0)
            } else {
                unsafe { backend(operation, args, output) }
            }
        }
        op::APPEND => {
            if v.kind == 0 {
                unsafe { backend(operation, args, output) }
            } else {
                let size = unsafe { backend(op::CHILD_SIZE, args, ptr::null_mut()) }.value;
                let raw = unsafe {
                    backend(
                        op::APPEND_OFFSET,
                        Args {
                            offset: size,
                            ..args
                        },
                        output,
                    )
                };
                Output::boolean(append_status(raw.status, v.kind) == 0)
            }
        }
        op::APPEND_OFFSET | op::APPEND_DEFERRED => {
            let raw = unsafe { backend(operation, args, output) };
            Output::code(append_status(raw.status, v.kind))
        }
        op::PUBLISH_COPY => {
            let raw = unsafe { backend(operation, args, output) };
            if v.kind == 0 {
                raw
            } else {
                Output::boolean(raw.status == 0)
            }
        }
        op::RECOVERY_DELETE => {
            let raw = unsafe {
                backend(
                    if v.kind == 0 {
                        op::DELETE_MATCH
                    } else {
                        operation
                    },
                    args,
                    output,
                )
            };
            if v.kind == 0 {
                Output::code(match raw.status {
                    0 => 0,
                    1 => 2,
                    2 => 3,
                    3 => 6,
                    _ => unreachable!(),
                })
            } else {
                raw
            }
        }
        op::RENAME => {
            let raw = unsafe { backend(operation, args, output) };
            if v.kind == 0 {
                Output::code(match raw.status {
                    0..=3 => raw.status,
                    4 => 3,
                    5 => 4,
                    _ => unreachable!(),
                })
            } else {
                raw
            }
        }
        op::REPLACE | op::REPLACE_CREATE => {
            let raw = unsafe { backend(operation, args, output) };
            if v.kind == 0 {
                raw
            } else {
                Output::code(-raw.status)
            }
        }
        op::DIR_NEXT => {
            if v.kind == 0 && args.parent > u64::from(u32::MAX) {
                Output::code(3)
            } else {
                unsafe { backend(operation, args, output) }
            }
        }
        op::RECOVERY_LOOKUP => unsafe {
            backend(
                if v.kind == 0 { op::LOOKUP } else { operation },
                args,
                output,
            )
        },
        _ => unsafe { backend(operation, args, output) },
    }
}
unsafe fn run(operation: u8, args: Args, output: *mut Entry) -> Output {
    let result = match operation {
        op::COPY => {
            if args.volume.kind == 0 && args.other.kind == 0 {
                unsafe {
                    backend(
                        if args.flag {
                            op::COPY
                        } else {
                            op::COPY_NO_REPLACE
                        },
                        args,
                        output,
                    )
                }
            } else {
                let chunk = heap::r4os_rust_heap_alloc(32768, 1);
                if chunk.is_null() {
                    Output::boolean(false)
                } else {
                    let mut progress = super::copy_transfer::Progress {
                        bytes: 0,
                        source_size: 0,
                        chunks: 0,
                        max_chunk: 0,
                    };
                    let result = unsafe {
                        copy_buffered(Args {
                            buffer: chunk,
                            buffer_len: 32768,
                            progress: &mut progress,
                            ..args
                        })
                    };
                    let _ = unsafe { heap::r4os_rust_heap_free(chunk, 32768) };
                    Output::boolean(result)
                }
            }
        }
        op::COPY_BUFFERED => Output::boolean(unsafe { copy_buffered(args) }),
        _ => unsafe { call(operation, args, output) },
    };
    match operation {
        op::WRITE
        | op::APPEND
        | op::APPEND_OFFSET
        | op::APPEND_DEFERRED
        | op::PUBLISH_COPY
        | op::MKDIR
        | op::DELETE
        | op::DELETE_MATCH
        | op::RECOVERY_DELETE
        | op::RMDIR
        | op::RENAME
        | op::REPLACE
        | op::REPLACE_CREATE => unsafe { notify(args.volume, args.parent) },
        op::COPY => unsafe { notify(args.other, args.parent) },
        _ => {}
    }
    result
}
// Input args and source entry are private resident value snapshots; mutable
// Progress is accessed only through raw fields. No reference into it spans
// namespace/backend I/O, flush or cleanup, including same-task reentrancy.
unsafe fn copy_buffered(args: Args) -> bool {
    unsafe {
        let source = args.entry.read();
        let progress = args.progress;
        progress.write(super::copy_transfer::Progress {
            bytes: 0,
            source_size: source.size,
            chunks: 0,
            max_chunk: 0,
        });
        if source.is_dir() || args.buffer_len == 0 || source.size > u64::from(u32::MAX) {
            return false;
        }
        let destination = Args {
            volume: args.other,
            ..args
        };
        let mut existing = Entry::EMPTY;
        match run(op::LOOKUP, destination, &mut existing).status {
            0 => {
                if !args.flag || existing.is_dir() || existing.read_only() {
                    return false;
                }
                if same_volume(args.volume, args.other) {
                    let identical = if args.volume.kind == 0 {
                        backend(
                            op::SAME_COPY,
                            Args {
                                entry: &source,
                                peer: &existing,
                                ..args
                            },
                            ptr::null_mut(),
                        )
                        .present
                    } else {
                        same_file(args.volume, source, existing)
                    };
                    if identical {
                        return false;
                    }
                }
                if !run(op::DELETE, destination, ptr::null_mut()).present {
                    return false;
                }
            }
            1 => {}
            2 => return false,
            _ => unreachable!(),
        }
        let completed = copy_payload(args, source, destination);
        if !completed {
            let _ = run(op::DELETE, destination, ptr::null_mut());
        }
        completed
    }
}
unsafe fn copy_payload(args: Args, source: Entry, destination: Args) -> bool {
    unsafe {
        if !run(
            op::WRITE,
            Args {
                data: t::Bytes {
                    data: args.buffer,
                    len: 0,
                },
                ..destination
            },
            ptr::null_mut(),
        )
        .present
        {
            return false;
        }
        while (*args.progress).bytes < source.size {
            let offset = (*args.progress).bytes;
            let want = usize::try_from((source.size - offset).min(args.buffer_len as u64))
                .expect("copy chunk");
            let got = run(
                op::READ_RANGE,
                Args {
                    entry: &source,
                    offset,
                    buffer_len: want,
                    ..args
                },
                ptr::null_mut(),
            );
            if !got.present || got.value != want as u64 {
                return false;
            }
            if run(
                op::APPEND_DEFERRED,
                Args {
                    offset,
                    data: t::Bytes {
                        data: args.buffer,
                        len: want,
                    },
                    ..destination
                },
                ptr::null_mut(),
            )
            .status
                != 0
            {
                return false;
            }
            (*args.progress).bytes += want as u64;
            (*args.progress).chunks += 1;
            (*args.progress).max_chunk = (*args.progress)
                .max_chunk
                .max(u32::try_from(want).expect("copy chunk"));
        }
        run(op::FLUSH, destination, ptr::null_mut()).present
    }
}
/// # Safety
/// Arguments/entries are initialized exact resident C records for the
/// synchronous call. Caller holds the original FS request/mutation lifetime
/// and owns all buffers/cursors/progress through real I/O completion. Only
/// original successful lookup operations write entry output; byte buffers
/// retain their original backend partial/error contract. No borrowed table
/// or backend state crosses a callback or wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vfs_run(
    operation: u8,
    args: *const Args,
    output: *mut Entry,
) -> Output {
    unsafe { run(operation, args.read(), output) }
}
/// Native Entry attribute policy; no disk access or mutable state.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_vfs_attribute(operation: u8, attr: u8) -> bool {
    attr & match operation {
        0 => 0x10,
        1 => 1,
        2 => 6,
        _ => unreachable!(),
    } != 0
}

/// # Safety
/// Exact copied mounted C volume; caller retains original namespace request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vfs_notify(v: *const Volume, parent: u64) {
    unsafe { notify(v.read(), parent) }
}

// Typed native consumers share the exact existing dispatch and mount owner.
pub(crate) fn mounted_volume(letter: u8) -> Option<Volume> {
    let mut out = MaybeUninit::<Volume>::uninit();
    if unsafe { r4os_rust_vfs_mount(4, letter, ptr::null(), ptr::null(), out.as_mut_ptr().cast()) }
    {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
/// Copy the exact current mounted instance under the same RAM owner as its
/// generation validation. No table borrow or mount lease survives this call.
pub(crate) fn mounted_volume_by_reference(reference: Reference) -> Option<Volume> {
    let _guard = Guard::acquire(1);
    unsafe {
        let mut snapshot = MaybeUninit::<Mount>::uninit();
        access(
            8,
            true,
            Input {
                reference,
                ..Input::default()
            },
            snapshot.as_mut_ptr().cast(),
        )?;
        for index in 0..27 {
            let slot = (*MOUNTS.0.get())[index];
            if slot.present
                && slot.volume.has_ref
                && slot.volume.reference.slot == reference.slot
                && slot.volume.reference.generation == reference.generation
            {
                return Some(slot.volume);
            }
        }
    }
    None
}
pub(crate) fn resolve_path(volume: Volume, path: &[u8]) -> Option<u64> {
    let result = unsafe {
        run(
            op::RESOLVE_PATH,
            Args {
                volume,
                name: t::Bytes {
                    data: path.as_ptr(),
                    len: path.len(),
                },
                ..Args::default()
            },
            ptr::null_mut(),
        )
    };
    if result.status == 0 {
        Some(result.value)
    } else {
        None
    }
}
#[expect(
    clippy::large_enum_variant,
    reason = "Original fixed VFS entry remains stack owned; no heap allocation or additional entry lifetime"
)]
pub(crate) enum DirectoryStatus {
    Found(Entry),
    NotFound,
    Io,
}
pub(crate) fn directory_entry(
    volume: Volume,
    parent: u64,
    index: usize,
    name: &mut [u8],
) -> DirectoryStatus {
    let mut entry = MaybeUninit::<Entry>::uninit();
    let result = unsafe {
        run(
            op::DIR_ENTRY,
            Args {
                volume,
                parent,
                index,
                buffer: name.as_mut_ptr(),
                buffer_len: name.len(),
                ..Args::default()
            },
            entry.as_mut_ptr(),
        )
    };
    match result.status {
        0 => DirectoryStatus::Found(unsafe { entry.assume_init() }),
        1 => DirectoryStatus::NotFound,
        2 => DirectoryStatus::Io,
        _ => panic!("Original VFS directory status"),
    }
}
/// # Safety
/// Caller owns the exact buffer through synchronous backend I/O and retains the
/// original FS request and copied entry/mount generation until completion.
pub(crate) unsafe fn read_file_raw(
    volume: Volume,
    entry: &Entry,
    buffer: *mut u8,
    len: usize,
) -> Option<usize> {
    let result = unsafe {
        run(
            op::READ,
            Args {
                volume,
                entry,
                buffer,
                buffer_len: len,
                ..Args::default()
            },
            ptr::null_mut(),
        )
    };
    if result.present {
        Some(usize::try_from(result.value).expect("Original VFS read length"))
    } else {
        None
    }
}
