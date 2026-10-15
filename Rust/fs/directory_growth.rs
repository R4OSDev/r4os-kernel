//! Original FAT directory-extension ordering and ambiguous-link retention.
use core::{ffi::c_void, ptr};

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Io {
    pub context: *mut c_void,
    pub allocate: unsafe extern "C" fn(*mut c_void, usize, *mut u32) -> bool,
    pub initialize: unsafe extern "C" fn(*mut c_void, u32, usize) -> bool,
    pub flush: unsafe extern "C" fn(*mut c_void) -> bool,
    pub tail_available: unsafe extern "C" fn(*mut c_void) -> bool,
    pub publish: unsafe extern "C" fn(*mut c_void, u32) -> bool,
    pub discard: unsafe extern "C" fn(*mut c_void, u32),
}

#[repr(C)]
pub struct Extension {
    pub first: u32,
    pub count: usize,
}

struct AllocationGuard {
    io: Io,
    first: u32,
    armed: bool,
}
impl Drop for AllocationGuard {
    fn drop(&mut self) {
        if self.armed {
            // SAFETY: The caller's exact FAT volume/context and request lane
            // remain resident and owned through any waiting discard leaf.
            unsafe { (self.io.discard)(self.io.context, self.first) };
        }
    }
}

/// # Safety
/// The aligned callback record, exact FAT context and exclusive output stay
/// resident throughout this synchronous extension and all waiting callbacks.
/// The caller retains its original volume request/mutation ownership. No Rust
/// reference is retained across a leaf; output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_directory_extend(
    io: *const Io,
    cluster_bytes: usize,
    missing_entries: usize,
    remaining_clusters: usize,
    output: *mut Extension,
) -> u8 {
    if !(512..=65536).contains(&cluster_bytes)
        || !cluster_bytes.is_power_of_two()
        || !(1..=21).contains(&missing_entries)
    {
        return 1; // Geometry
    }
    let count = (missing_entries * 32).div_ceil(cluster_bytes);
    if count > remaining_clusters {
        return 2; // Capacity
    }
    let io = unsafe { ptr::read(io) };
    let mut first = 0;
    if !unsafe { (io.allocate)(io.context, count, &mut first) } {
        return 3; // Allocation
    }
    let mut allocation = AllocationGuard {
        io,
        first,
        armed: true,
    };
    if !unsafe { (io.initialize)(io.context, first, count) } {
        return 4; // Initialize
    }
    if !unsafe { (io.flush)(io.context) } {
        return 5; // Flush
    }
    if !unsafe { (io.tail_available)(io.context) } {
        return 6; // Tail
    }
    // The first mirror may become linked even when publish returns false.
    allocation.armed = false;
    if !unsafe { (io.publish)(io.context, first) } {
        return 7; // Publish
    }
    if !unsafe { (io.flush)(io.context) } {
        return 5; // Flush
    }
    unsafe { ptr::write(output, Extension { first, count }) };
    0
}
