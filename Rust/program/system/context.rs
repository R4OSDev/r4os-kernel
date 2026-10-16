//! Original path/stream/module provider handles. Native state retains exact
//! Zig ABI addresses; stateless typed invokers marshal their source records.
use super::super::registry_format_types::Bytes;
use super::{fs, path};
use crate::{
    fs::{drive::Drive, vfs_types::Volume},
    sched::{
        scheduler,
        task::{self, task_api},
    },
};
use core::{
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Target {
    pub drive_ref: *mut Drive,
    pub path: Bytes,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StreamOwner {
    pub kind: u8,
    pub id: u32,
    pub generation: u64,
    pub program_id: u32,
    pub program_generation: u64,
}
static PATH: AtomicUsize = AtomicUsize::new(0);
static STREAM: AtomicUsize = AtomicUsize::new(0);
static MODULE: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" {
    fn r4os_zig_sys_path_resolve(
        address: usize,
        raw: Bytes,
        out: *mut u8,
        target: *mut Target,
    ) -> bool;
    fn r4os_zig_sys_stream_owner(address: usize, out: *mut StreamOwner) -> bool;
    fn r4os_zig_sys_module_running(address: usize, letter: u8, path: Bytes) -> i32;
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_sys_set_provider(operation: u8, address: usize) {
    match operation {
        0 => &PATH,
        1 => &STREAM,
        2 => &MODULE,
        _ => panic!("R4SYS provider"),
    }
    .store(address, Ordering::Release);
}
pub(super) unsafe fn resolve(raw: &[u8], out: &mut path::Buffer<1024>) -> Option<Target> {
    unsafe {
        let address = PATH.load(Ordering::Acquire);
        if address == 0 {
            return None;
        }
        let mut target = MaybeUninit::uninit();
        if r4os_zig_sys_path_resolve(
            address,
            Bytes::from_slice(raw),
            out.as_mut_ptr(),
            target.as_mut_ptr(),
        ) {
            Some(target.assume_init())
        } else {
            None
        }
    }
}
pub(super) unsafe fn owner() -> Option<StreamOwner> {
    unsafe {
        let address = STREAM.load(Ordering::Acquire);
        if address != 0 {
            let mut out = MaybeUninit::uninit();
            if r4os_zig_sys_stream_owner(address, out.as_mut_ptr()) {
                return Some(out.assume_init());
            }
        }
        let current = scheduler::current();
        if current.is_null() {
            return None;
        }
        let (id, generation) = task::immutable_identity(current);
        Some(StreamOwner {
            kind: 1,
            id,
            generation,
            ..Default::default()
        })
    }
}
pub(super) fn has_owner_provider() -> bool {
    STREAM.load(Ordering::Acquire) != 0
}
pub(super) unsafe fn module_running(letter: u8, path: &[u8]) -> i32 {
    unsafe {
        let address = MODULE.load(Ordering::Acquire);
        if address == 0 {
            -2
        } else {
            r4os_zig_sys_module_running(address, letter, Bytes::from_slice(path))
        }
    }
}
pub(super) fn boot_subtree(value: &[u8]) -> bool {
    value.len() >= 5
        && path::separator(value[0])
        && value[1..5].eq_ignore_ascii_case(b"boot")
        && (value.len() == 5 || path::separator(value[5]))
}
pub(super) unsafe fn boot_target(target: Target) -> bool {
    unsafe {
        (*target.drive_ref).role == 1
            && boot_subtree(target.path.slice())
            && fs::volume(0).is_some()
    }
}
pub(super) unsafe fn volume(target: Target) -> Option<Volume> {
    unsafe {
        if (*target.drive_ref).role == 1
            && boot_subtree(target.path.slice())
            && let Some(volume) = fs::volume(0)
        {
            return Some(volume);
        }
        fs::volume((*target.drive_ref).letter)
    }
}
pub(super) unsafe fn alive(id: u32, generation: u64) -> bool {
    unsafe { task_api::r4os_rust_task_control(19, u64::from(id), generation) != 0 }
}
pub(super) struct PathBuffers {
    raw: path::Buffer<1024>,
    resolved: path::Buffer<1024>,
    len: usize,
}
impl PathBuffers {
    pub const fn new() -> Self {
        Self {
            raw: path::Buffer::new(),
            resolved: path::Buffer::new(),
            len: 0,
        }
    }
    pub unsafe fn copy(&mut self, input: *const u8) -> bool {
        if let Some(raw) = unsafe { path::copy_z(input, &mut self.raw) } {
            self.len = raw.len();
            true
        } else {
            false
        }
    }
    pub unsafe fn resolve_copied(&mut self) -> Option<Target> {
        unsafe { resolve(self.raw.initialized(self.len), &mut self.resolved) }
    }
    pub unsafe fn resolve(&mut self, input: *const u8) -> Option<Target> {
        if !unsafe { self.copy(input) } {
            return None;
        }
        unsafe { self.resolve_copied() }
    }
    pub fn raw(&self) -> &[u8] {
        unsafe { self.raw.initialized(self.len) }
    }
}
