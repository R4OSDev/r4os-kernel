//! Original module file streaming and bounded two-window metadata owner.
use crate::{
    arch::runtime::RuntimeGuard,
    fs::{drive, request, vfs, vfs_types as t},
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicU16, Ordering},
};
pub const WINDOW_SIZE: usize = 4096;
pub const WINDOW_COUNT: usize = 2;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub active_buffers: u64,
    pub reserved_bytes: u64,
    pub committed_bytes: u64,
    pub peak_reserved_bytes: u64,
    pub peak_committed_bytes: u64,
    pub full_reads: u64,
    pub range_reads: u64,
    pub range_read_bytes: u64,
    pub metadata_reader_initializations: u64,
    pub metadata_logical_reads: u64,
    pub metadata_logical_bytes: u64,
    pub metadata_window_hits: u64,
    pub metadata_window_fills: u64,
    pub metadata_window_fill_bytes: u64,
    pub metadata_direct_reads: u64,
    pub metadata_direct_bytes: u64,
    pub reserve_failures: u64,
    pub commit_failures: u64,
    pub read_failures: u64,
    pub short_reads: u64,
    pub release_failures: u64,
    pub pressure_reclaim_attempts: u64,
    pub pressure_reclaimed_frames: u64,
    pub pressure_failures: u64,
}
impl Stats {
    const EMPTY: Self = Self {
        active_buffers: 0,
        reserved_bytes: 0,
        committed_bytes: 0,
        peak_reserved_bytes: 0,
        peak_committed_bytes: 0,
        full_reads: 0,
        range_reads: 0,
        range_read_bytes: 0,
        metadata_reader_initializations: 0,
        metadata_logical_reads: 0,
        metadata_logical_bytes: 0,
        metadata_window_hits: 0,
        metadata_window_fills: 0,
        metadata_window_fill_bytes: 0,
        metadata_direct_reads: 0,
        metadata_direct_bytes: 0,
        reserve_failures: 0,
        commit_failures: 0,
        read_failures: 0,
        short_reads: 0,
        release_failures: 0,
        pressure_reclaim_attempts: 0,
        pressure_reclaimed_frames: 0,
        pressure_failures: 0,
    };
}
struct Counters(UnsafeCell<Stats>);
// SAFETY: Only bounded updates/snapshots under the existing runtime owner.
unsafe impl Sync for Counters {}
static STATS: Counters = Counters(UnsafeCell::new(Stats::EMPTY));
static EXECUTION_DRIVE: AtomicU16 = AtomicU16::new(256);
fn update(f: impl FnOnce(&mut Stats)) {
    let _owner = RuntimeGuard::acquire();
    unsafe { f(&mut *STATS.0.get()) };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Source {
    pub volume: t::Volume,
    pub entry: t::Entry,
    pub drive: u8,
}
#[repr(C)]
pub struct Window {
    pub valid: bool,
    pub offset: usize,
    pub len: usize,
    pub stamp: u64,
    pub bytes: [MaybeUninit<u8>; WINDOW_SIZE],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Output {
    pub len: usize,
    pub present: bool,
}
impl Output {
    const NONE: Self = Self {
        len: 0,
        present: false,
    };
    const fn some(len: usize) -> Self {
        Self { len, present: true }
    }
}
#[repr(C)]
pub struct Io {
    pub context: *mut c_void,
    pub read: unsafe extern "C" fn(*mut c_void, usize, *mut u8, usize) -> Output,
    pub name: t::Bytes,
    pub verbose: bool,
}
unsafe extern "C" {
    fn r4os_zig_module_file_log(
        kind: u8,
        name: *const u8,
        name_len: usize,
        verbose: bool,
        offset: usize,
        wanted: usize,
        got: usize,
    );
}
fn report(kind: u8, name: t::Bytes, verbose: bool, offset: usize, wanted: usize, got: usize) {
    if verbose {
        unsafe { r4os_zig_module_file_log(kind, name.data, name.len, true, offset, wanted, got) };
    }
}
fn short_read(name: t::Bytes, verbose: bool, offset: usize, wanted: usize, got: usize) {
    update(|s| s.short_reads = s.short_reads.saturating_add(1));
    report(2, name, verbose, offset, wanted, got);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_module_file_restrict(letter: u8) {
    EXECUTION_DRIVE.store(u16::from(letter), Ordering::Relaxed);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_module_file_allowed(letter: u8) -> bool {
    let required = EXECUTION_DRIVE.load(Ordering::Relaxed);
    if required == 256 {
        return true;
    }
    let mounted = unsafe { drive::r4os_rust_fs_drive_lookup(1, usize::from(letter)) };
    if mounted.is_null() {
        return false;
    }
    u16::from(letter) == required && unsafe { (*mounted).role == 3 }
}
/// # Safety
/// Exclusive initialized output; coherent bounded counter snapshot only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_file_stats(out: *mut Stats) {
    let _owner = RuntimeGuard::acquire();
    unsafe { out.write(*STATS.0.get()) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_module_file_note_init() {
    update(|s| {
        s.metadata_reader_initializations = s.metadata_reader_initializations.saturating_add(1)
    });
}
/// # Safety
/// Source/name and output buffer remain resident through real filesystem I/O.
/// Source is ignored for an empty read; output does not alias source metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_file_range(
    source: *const Source,
    offset: usize,
    out: *mut u8,
    len: usize,
    name: t::Bytes,
    verbose: bool,
) -> Output {
    if len == 0 {
        return Output::some(0);
    }
    let source = unsafe { &*source };
    let bound = request::BoundMount {
        reference: source.volume.reference,
        present: source.volume.has_ref,
    };
    let mut guard = MaybeUninit::<request::Guard>::uninit();
    if !unsafe {
        request::r4os_rust_fs_request_begin(
            19,
            source.drive,
            0,
            0,
            false,
            &bound,
            1,
            true,
            guard.as_mut_ptr(),
        )
    } {
        update(|s| s.read_failures += 1);
        report(0, name, verbose, 0, 0, 0);
        return Output::NONE;
    }
    let args = t::Args {
        volume: source.volume,
        entry: &source.entry,
        offset: offset as u64,
        buffer: out,
        buffer_len: len,
        ..t::Args::default()
    };
    let result = unsafe { vfs::r4os_rust_vfs_run(t::op::READ_RANGE, &args, ptr::null_mut()) };
    unsafe { request::r4os_rust_fs_request_finish(guard.as_mut_ptr(), result.present) };
    if !result.present {
        update(|s| s.read_failures += 1);
        report(1, name, verbose, 0, 0, 0);
        return Output::NONE;
    }
    let got = usize::try_from(result.value).expect("module file read size");
    update(|s| {
        s.range_reads += 1;
        s.range_read_bytes = s.range_read_bytes.saturating_add(got as u64);
    });
    Output::some(got)
}
/// # Safety
/// Same source/buffer lifetime as range(); no alias with source metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_file_exact(
    source: *const Source,
    offset: usize,
    out: *mut u8,
    len: usize,
    name: t::Bytes,
    verbose: bool,
) -> bool {
    let result = unsafe { r4os_rust_module_file_range(source, offset, out, len, name, verbose) };
    if !result.present {
        return false;
    }
    if result.len != len {
        short_read(name, verbose, offset, len, result.len);
        return false;
    }
    true
}
/// # Safety
/// Two initialized metadata windows, private to the caller. Payload bytes may
/// remain uninitialized until a successful fill; no payload is read here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_file_find(
    windows: *const Window,
    offset: usize,
    len: usize,
) -> usize {
    for i in 0..WINDOW_COUNT {
        let w = unsafe { &*windows.add(i) };
        if w.valid && offset >= w.offset {
            let local = offset - w.offset;
            if local <= w.len && len <= w.len - local {
                return i;
            }
        }
    }
    usize::MAX
}
/// # Safety
/// Two initialized window metadata records, not concurrently mutated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_file_replace(windows: *const Window) -> usize {
    for i in 0..WINDOW_COUNT {
        if !unsafe { (*windows.add(i)).valid } {
            return i;
        }
    }
    let mut oldest = 0;
    for i in 1..WINDOW_COUNT {
        if unsafe { (*windows.add(i)).stamp < (*windows.add(oldest)).stamp } {
            oldest = i;
        }
    }
    oldest
}
unsafe fn touch(windows: *mut Window, stamp: *mut u64, index: usize) {
    let mut next = unsafe { stamp.read() }.wrapping_add(1);
    if next == 0 {
        for i in 0..WINDOW_COUNT {
            unsafe { (*windows.add(i)).stamp = 0 };
        }
        next = 1;
    }
    unsafe {
        stamp.write(next);
        (*windows.add(index)).stamp = next;
    }
}
/// # Safety
/// Exclusive caller metadata; bytes are deliberately not initialized/reset.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_file_reset(windows: *mut Window, stamp: *mut u64) {
    r4os_rust_module_file_note_init();
    unsafe {
        stamp.write(0);
    }
    for i in 0..WINDOW_COUNT {
        unsafe {
            let w = windows.add(i);
            (*w).valid = false;
            (*w).offset = 0;
            (*w).len = 0;
            (*w).stamp = 0;
        }
    }
}
/// # Safety
/// Two caller-owned resident windows/stamp; no exclusive Rust window borrow
/// spans read callbacks or waits. Callback initializes exactly its reported
/// prefix. Output is disjoint from windows and readable caller metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_file_cached(
    windows: *mut Window,
    stamp: *mut u64,
    file_size: usize,
    offset: usize,
    out: *mut u8,
    len: usize,
    io: *const Io,
) -> bool {
    let io = unsafe { &*io };
    update(|s| {
        s.metadata_logical_reads = s.metadata_logical_reads.saturating_add(1);
        s.metadata_logical_bytes = s.metadata_logical_bytes.saturating_add(len as u64);
    });
    if len == 0 {
        return offset <= file_size;
    }
    if offset > file_size || len > file_size - offset {
        update(|s| s.read_failures = s.read_failures.saturating_add(1));
        report(3, io.name, io.verbose, offset, len, file_size);
        return false;
    }
    if len > WINDOW_SIZE {
        update(|s| {
            s.metadata_direct_reads = s.metadata_direct_reads.saturating_add(1);
            s.metadata_direct_bytes = s.metadata_direct_bytes.saturating_add(len as u64);
        });
        let result = unsafe { (io.read)(io.context, offset, out, len) };
        if !result.present {
            return false;
        }
        if result.len != len {
            short_read(io.name, io.verbose, offset, len, result.len);
            return false;
        }
        return true;
    }
    let index = unsafe { r4os_rust_module_file_find(windows, offset, len) };
    if index != usize::MAX {
        let local = offset - unsafe { (*windows.add(index)).offset };
        assert!(local <= WINDOW_SIZE && len <= WINDOW_SIZE - local);
        unsafe {
            ptr::copy_nonoverlapping(
                (*windows.add(index)).bytes.as_ptr().cast::<u8>().add(local),
                out,
                len,
            );
        }
        update(|s| s.metadata_window_hits = s.metadata_window_hits.saturating_add(1));
        unsafe { touch(windows, stamp, index) };
        return true;
    }
    let index = unsafe { r4os_rust_module_file_replace(windows) };
    let window = unsafe { windows.add(index) };
    let wanted = WINDOW_SIZE.min(file_size - offset);
    unsafe {
        (*window).valid = false;
    }
    let result = unsafe {
        (io.read)(
            io.context,
            offset,
            ptr::addr_of_mut!((*window).bytes).cast::<u8>(),
            wanted,
        )
    };
    if !result.present {
        return false;
    }
    update(|s| {
        s.metadata_window_fills = s.metadata_window_fills.saturating_add(1);
        s.metadata_window_fill_bytes = s
            .metadata_window_fill_bytes
            .saturating_add(result.len as u64);
    });
    if result.len < len {
        short_read(io.name, io.verbose, offset, len, result.len);
        return false;
    }
    unsafe {
        (*window).offset = offset;
        (*window).len = result.len;
        (*window).valid = true;
        touch(windows, stamp, index);
        ptr::copy_nonoverlapping(ptr::addr_of!((*window).bytes).cast::<u8>(), out, len);
    }
    true
}
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 192);
    assert!(core::mem::size_of::<Source>() == 936);
    assert!(core::mem::offset_of!(Source, entry) == 88);
    assert!(core::mem::offset_of!(Source, drive) == 928);
    assert!(core::mem::size_of::<Window>() == 4128);
    assert!(core::mem::offset_of!(Window, bytes) == 32);
    assert!(core::mem::size_of::<Io>() == 40);
    assert!(core::mem::size_of::<Output>() == 16);
};
