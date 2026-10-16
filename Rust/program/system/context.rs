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
use core::sync::atomic::{AtomicBool, Ordering};
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
static PATH: AtomicBool = AtomicBool::new(false);
static STREAM: AtomicBool = AtomicBool::new(false);
static MODULE: AtomicBool = AtomicBool::new(false);
pub(crate) fn install_program_providers() {
    PATH.store(true, Ordering::Release);
    STREAM.store(true, Ordering::Release);
    MODULE.store(true, Ordering::Release);
}
pub(super) unsafe fn resolve(raw: &[u8], out: &mut path::Buffer<1024>) -> Option<Target> {
    if !PATH.load(Ordering::Acquire) {
        return None;
    }
    let target = unsafe {
        super::super::program_api::resolve(
            crate::fs::vfs_types::Bytes {
                data: raw.as_ptr(),
                len: raw.len(),
            },
            out.as_mut_ptr(),
        )
    }?;
    Some(Target {
        drive_ref: target.drive,
        path: Bytes {
            data: target.path.data,
            len: target.path.len,
        },
    })
}
pub(super) unsafe fn owner() -> Option<StreamOwner> {
    if STREAM.load(Ordering::Acquire)
        && let Some(owner) = unsafe { super::super::async_io::stream_owner() }
    {
        return Some(StreamOwner {
            kind: 0,
            id: owner.task,
            generation: owner.task_generation,
            program_id: owner.program,
            program_generation: owner.program_generation,
        });
    }
    let current = scheduler::current();
    if current.is_null() {
        return None;
    }
    let (id, generation) = unsafe { task::immutable_identity(current) };
    Some(StreamOwner {
        kind: 1,
        id,
        generation,
        ..Default::default()
    })
}
pub(super) fn has_owner_provider() -> bool {
    STREAM.load(Ordering::Acquire)
}
pub(super) unsafe fn module_running(letter: u8, path: &[u8]) -> i32 {
    if !MODULE.load(Ordering::Acquire) {
        return -2;
    }
    unsafe {
        super::super::launch::r4os_rust_program_launch_running(
            letter,
            crate::fs::vfs_types::Bytes {
                data: path.as_ptr(),
                len: path.len(),
            },
        )
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
