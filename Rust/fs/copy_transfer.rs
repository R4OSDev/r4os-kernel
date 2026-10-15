//! Original bounded staged-copy lifecycle. Backend leaves remain with their
//! current FS owner; no Rust borrow spans any callback, wait or publication.
use core::{ffi::c_void, ptr};

const MAX_CHUNK_BYTES: usize = 32768;

#[repr(C)]
pub struct Progress {
    pub bytes: u64,
    pub source_size: u64,
    pub chunks: u32,
    pub max_chunk: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Io {
    pub context: *mut c_void,
    pub cancelled: unsafe extern "C" fn(*mut c_void) -> bool,
    pub prepare: unsafe extern "C" fn(*mut c_void) -> bool,
    pub allow_readers: unsafe extern "C" fn(*mut c_void),
    pub read: unsafe extern "C" fn(*mut c_void, u32, *mut u8, usize, *mut usize) -> bool,
    pub append: unsafe extern "C" fn(*mut c_void, u64, *const u8, usize) -> bool,
    pub progress: unsafe extern "C" fn(*mut c_void),
    pub pause: unsafe extern "C" fn(*mut c_void),
    pub flush: unsafe extern "C" fn(*mut c_void) -> bool,
    pub publish: unsafe extern "C" fn(*mut c_void) -> bool,
    pub abort: unsafe extern "C" fn(*mut c_void),
}

struct StageGuard {
    io: Io,
    armed: bool,
}
impl Drop for StageGuard {
    fn drop(&mut self) {
        if self.armed {
            // SAFETY: The exact caller context remains resident until the
            // synchronous transfer returns, including a waiting abort leaf.
            unsafe { (self.io.abort)(self.io.context) };
        }
    }
}

/// # Safety
/// The I/O record, context, bounded scratch and progress remain aligned and
/// resident through this call and every waiting callback. The existing caller
/// retains mount/mutation/unwind ownership. Callback results are the original
/// same-instance leaves. No other task mutates progress; callbacks may observe
/// it, so all progress access is raw and ends before the next callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_copy_transfer(
    io: *const Io,
    buffer: *mut u8,
    buffer_len: usize,
    size: u64,
    progress: *mut Progress,
) -> bool {
    unsafe {
        ptr::write(
            progress,
            Progress {
                bytes: 0,
                source_size: size,
                chunks: 0,
                max_chunk: 0,
            },
        );
    }
    let io = unsafe { ptr::read(io) };
    if buffer_len == 0 || size > 0xffff_ffff || unsafe { (io.cancelled)(io.context) } {
        return false;
    }
    let chunk_len = buffer_len.min(MAX_CHUNK_BYTES);
    let mut stage = StageGuard { io, armed: true };
    if !unsafe { (io.prepare)(io.context) } {
        return false;
    }
    unsafe { (io.allow_readers)(io.context) };
    while unsafe { (*progress).bytes } < size {
        if unsafe { (io.cancelled)(io.context) } {
            return false;
        }
        let offset = unsafe { (*progress).bytes };
        let want = (size - offset).min(chunk_len as u64) as usize;
        let mut got = 0;
        if !unsafe { (io.read)(io.context, offset as u32, buffer, want, &mut got) } || got != want {
            return false;
        }
        // Re-read the observable progress after a backend callback, exactly
        // as the original owner did before append and its checked increments.
        if !unsafe { (io.append)(io.context, (*progress).bytes, buffer, want) } {
            return false;
        }
        unsafe {
            (*progress).bytes += want as u64;
            (*progress).chunks += 1;
            (*progress).max_chunk = (*progress).max_chunk.max(want as u32);
            (io.progress)(io.context);
            (io.pause)(io.context);
        }
    }
    if !unsafe { (io.flush)(io.context) } || unsafe { (io.cancelled)(io.context) } {
        return false;
    }
    // Even a failed publication may already have transferred the payload.
    // Crossing this exact boundary permanently suppresses stage abortion.
    stage.armed = false;
    unsafe { (io.publish)(io.context) }
}
