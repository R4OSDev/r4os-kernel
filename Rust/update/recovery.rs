//! Terminal journal replay. No Rust metadata reference survives a backend
//! operation or persist: persist may advance the resident journal generation.
use super::claim::span;
use super::journal::{self, Journal, Payload};
use crate::fs::vfs_types::Bytes;
pub trait ReplayIo {
    fn path_state(&mut self, path: Bytes, size: u64, sum: u32) -> u32;
    fn presence(&mut self, path: Bytes) -> u32;
    /// # Safety
    /// Exact resident payload remains owned for the checked backend operation.
    unsafe fn mutate(&mut self, op: u32, entry: *const Payload) -> u32;
    /// # Safety
    /// Exact resident journal remains owned and may be advanced by this call.
    unsafe fn persist(&mut self, j: *mut Journal) -> bool;
}
macro_rules! get {
    ($p:ident.$f:ident) => {
        unsafe { (*$p).$f }
    };
}
macro_rules! path {
    ($p:ident,$m:ident) => {
        unsafe { span((*$p).$m()) }
    };
}
fn checked_state(state: u32, wanted: u32) -> u32 {
    if state == wanted {
        0
    } else if state == 3 {
        3
    } else {
        2
    }
}
fn mutation(state: u32) -> u32 {
    match state {
        0 => 0,
        1 => 2,
        2 => 3,
        _ => panic!("Invalid recovery mutation receipt"),
    }
}
unsafe fn recount(j: *mut Journal) {
    unsafe {
        (*j).committed_count = 0;
        (*j).rollback_count = 0;
        for i in 0..(*j).payload_count as usize {
            (*j).committed_count += u32::from((*j).payloads[i].committed);
            (*j).rollback_count += u32::from((*j).payloads[i].rolled_back)
        }
    }
}
/// # Safety
/// The caller retains the exact journal and I/O admission throughout replay.
pub unsafe fn check_last_good(io: &mut impl ReplayIo, j: *const Journal) -> u32 {
    if !unsafe { journal::valid(&*j) } || !matches!(get!(j.phase), 5..=7) {
        return 1;
    }
    let mut changed = false;
    for i in 0..get!(j.payload_count) as usize {
        let e = unsafe { core::ptr::addr_of!((*j).payloads[i]) };
        if !get!(e.committed) || get!(e.rolled_back) {
            return 1;
        }
        let status = checked_state(
            io.path_state(path!(e, target), get!(e.size), get!(e.checksum)),
            0,
        );
        if status != 0 {
            return status;
        }
        if !get!(e.replace_required) {
            continue;
        }
        changed = true;
        if get!(e.target_existed) && !get!(e.old_known) {
            return 1;
        }
        let status = checked_state(io.presence(path!(e, stage)), 2);
        if status != 0 {
            return status;
        }
        let status = if get!(e.target_existed) {
            checked_state(
                io.path_state(path!(e, backup), get!(e.old_size), get!(e.old_checksum)),
                0,
            )
        } else {
            checked_state(io.presence(path!(e, backup)), 2)
        };
        if status != 0 {
            return status;
        }
    }
    if changed { 0 } else { 1 }
}
/// # Safety
/// Journal and payloads are resident, exclusively admitted and not relocated.
pub unsafe fn rollback(io: &mut impl ReplayIo, j: *mut Journal) -> u32 {
    if !unsafe { journal::valid(&*j) } || matches!(get!(j.phase), 5 | 7) {
        return 1;
    }
    if get!(j.phase) == 9 {
        return 0;
    }
    unsafe {
        (*j).phase = 8;
        (*j).rollback_markers_present = true;
        recount(j);
        if !io.persist(j) {
            return 3;
        }
    }
    let mut cursor = get!(j.payload_count) as usize;
    while cursor > 0 {
        cursor -= 1;
        let e = unsafe { core::ptr::addr_of_mut!((*j).payloads[cursor]) };
        if get!(e.replace_required) {
            let status = mutation(unsafe { io.mutate(0, e) });
            if status != 0 {
                return status;
            }
        }
        if get!(e.rolled_back) {
            continue;
        }
        unsafe {
            (*e).committed = false;
            (*e).rolled_back = true;
            recount(j);
            if !io.persist(j) {
                return 3;
            }
        }
    }
    unsafe {
        (*j).phase = 9;
        recount(j);
        if io.persist(j) { 0 } else { 3 }
    }
}
/// # Safety
/// The complete staged v5 batch stays bound to this exact resident journal.
pub unsafe fn forward(io: &mut impl ReplayIo, j: *mut Journal) -> u32 {
    if !unsafe { journal::valid(&*j) } || !get!(j.batch) {
        return 1;
    }
    if get!(j.phase) == 6 {
        return 0;
    }
    if !matches!(get!(j.phase), 1 | 2) {
        return 1;
    }
    unsafe {
        (*j).phase = 2;
        recount(j);
        if !io.persist(j) {
            return 3;
        }
    }
    for i in 0..get!(j.payload_count) as usize {
        let e = unsafe { core::ptr::addr_of_mut!((*j).payloads[i]) };
        if !get!(e.committed) {
            let status = mutation(unsafe { io.mutate(1, e) });
            if status != 0 {
                return status;
            }
            unsafe {
                (*e).committed = true;
                (*e).rolled_back = false;
                recount(j);
                if !io.persist(j) {
                    return 3;
                }
            }
        }
    }
    for i in 0..get!(j.payload_count) as usize {
        let e = unsafe { core::ptr::addr_of!((*j).payloads[i]) };
        let status = checked_state(
            io.path_state(path!(e, target), get!(e.size), get!(e.checksum)),
            0,
        );
        if status != 0 {
            return status;
        }
    }
    unsafe {
        (*j).phase = 6;
        recount(j);
        if io.persist(j) { 0 } else { 3 }
    }
}
/// # Safety
/// The caller retains the exact package and checked per-payload cleanup seam.
pub unsafe fn cleanup(io: &mut impl ReplayIo, j: *mut Journal) -> u32 {
    if !unsafe { journal::valid(&*j) } {
        return 1;
    }
    if get!(j.phase) == 7 {
        return 0;
    }
    if get!(j.phase) != 5 {
        return 1;
    }
    // Revalidate the complete package before rotating even one older backup.
    for i in 0..get!(j.payload_count) as usize {
        let e = unsafe { core::ptr::addr_of!((*j).payloads[i]) };
        let status = checked_state(
            io.path_state(path!(e, target), get!(e.size), get!(e.checksum)),
            0,
        );
        if status != 0 {
            return status;
        }
        if !get!(e.replace_required) {
            continue;
        }
        let status = checked_state(io.presence(path!(e, stage)), 2);
        if status != 0 {
            return status;
        }
        let status = if get!(e.target_existed) {
            if get!(e.old_known) {
                checked_state(
                    io.path_state(path!(e, backup), get!(e.old_size), get!(e.old_checksum)),
                    0,
                )
            } else {
                checked_state(io.presence(path!(e, backup)), 0)
            }
        } else {
            checked_state(io.presence(path!(e, backup)), 2)
        };
        if status != 0 {
            return status;
        }
    }
    // Each tuple is checked and mutated under one actual filesystem gate.
    // Cleanup never deletes the already consumed stage by name.
    for i in 0..get!(j.payload_count) as usize {
        let e = unsafe { core::ptr::addr_of!((*j).payloads[i]) };
        if get!(e.replace_required) {
            let status = mutation(unsafe { io.mutate(2, e) });
            if status != 0 {
                return status;
            }
        }
    }
    unsafe {
        (*j).phase = 7;
        (*j).rollback_markers_present = true;
        if io.persist(j) { 0 } else { 3 }
    }
}
#[repr(C)]
pub struct Io {
    pub context: *mut core::ffi::c_void,
    pub call: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        u32,
        *mut Journal,
        *const Payload,
        Bytes,
        u64,
        u32,
    ) -> u32,
}
impl ReplayIo for Io {
    fn path_state(&mut self, p: Bytes, size: u64, sum: u32) -> u32 {
        unsafe {
            (self.call)(
                self.context,
                0,
                core::ptr::null_mut(),
                core::ptr::null(),
                p,
                size,
                sum,
            )
        }
    }
    fn presence(&mut self, p: Bytes) -> u32 {
        unsafe {
            (self.call)(
                self.context,
                1,
                core::ptr::null_mut(),
                core::ptr::null(),
                p,
                0,
                0,
            )
        }
    }
    unsafe fn mutate(&mut self, op: u32, e: *const Payload) -> u32 {
        unsafe {
            (self.call)(
                self.context,
                op + 2,
                core::ptr::null_mut(),
                e,
                Bytes::default(),
                0,
                0,
            )
        }
    }
    unsafe fn persist(&mut self, j: *mut Journal) -> bool {
        unsafe {
            (self.call)(
                self.context,
                5,
                j,
                core::ptr::null(),
                Bytes::default(),
                0,
                0,
            ) != 0
        }
    }
}
/// # Safety
/// The fixed-layout callback retains its real typed context and exact records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_recovery_run(op: u32, io: *mut Io, j: *mut Journal) -> u32 {
    unsafe {
        match op {
            0 => check_last_good(&mut *io, j),
            1 => rollback(&mut *io, j),
            2 => forward(&mut *io, j),
            3 => cleanup(&mut *io, j),
            _ => panic!("Invalid recovery replay operation"),
        }
    }
}
