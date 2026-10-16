//! Package-wide recovery before loader/config/font/R4D caches. Journal buffers
//! and all three 181728-byte records remain resident, never task-stack values.
use super::{
    atomic,
    claim::{bytes, span},
    io,
    journal::{self, JOURNAL_MAX, Journal, Payload},
    recovery::{self, ReplayIo},
};
use crate::{
    boot::foundation,
    fs::{
        vfs,
        vfs_types::{Bytes, Volume},
    },
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr};
const DIRECTORY: &[u8] = b"/R4OS/UPDATE/STAGED";
const NAMES: [&[u8]; 2] = [b"SYSUPD0.JRN", b"SYSUPD1.JRN"];
const PATHS: [&[u8]; 2] = [
    b"/R4OS/UPDATE/STAGED/SYSUPD0.JRN",
    b"/R4OS/UPDATE/STAGED/SYSUPD1.JRN",
];
struct State {
    active: Journal,
    parse: Journal,
    verify: Journal,
    buffers: [[u8; JOURNAL_MAX]; 2],
    write: [u8; JOURNAL_MAX],
    checksum: [u8; 65536],
}
struct Storage(UnsafeCell<State>);
// SAFETY: original linear BSP recovery before runtime owns every field. Raw
// field addresses keep metadata references out of VFS waits and persistence.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    active: Journal::EMPTY,
    parse: Journal::EMPTY,
    verify: Journal::EMPTY,
    buffers: [[0; JOURNAL_MAX]; 2],
    write: [0; JOURNAL_MAX],
    checksum: [0; 65536],
}));
impl State {
    fn active() -> *mut Journal {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).active) }
    }
    fn parse() -> *mut Journal {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).parse) }
    }
    fn verify() -> *mut Journal {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).verify) }
    }
    fn buffers() -> *mut [[u8; JOURNAL_MAX]; 2] {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).buffers) }
    }
    fn write() -> *mut [u8; JOURNAL_MAX] {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).write) }
    }
    fn checksum() -> *mut [u8; 65536] {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).checksum) }
    }
}
macro_rules! field {
    ($f:ident) => {
        State::$f()
    };
}
fn buffer(slot: usize) -> *mut u8 {
    unsafe { field!(buffers).cast::<[u8; JOURNAL_MAX]>().add(slot).cast() }
}
fn boot_volume() -> Option<Volume> {
    let mut out = MaybeUninit::<Volume>::uninit();
    if unsafe { vfs::r4os_rust_vfs_mount(3, 0, ptr::null(), ptr::null(), out.as_mut_ptr().cast()) }
    {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
fn drive(path: &[u8]) -> u8 {
    if path.len() >= 3 && path[1] == b':' && path[2] == b'\\' {
        path[0].to_ascii_uppercase()
    } else {
        0
    }
}
fn strip(path: &[u8]) -> &[u8] {
    if path.len() >= 3 && path[1] == b':' && path[2] == b'\\' {
        &path[2..]
    } else {
        path
    }
}
fn payload_volume(path: &[u8]) -> Option<Volume> {
    if path.len() >= 3 && path[1] == b':' && path[2] == b'\\' {
        return vfs::mounted_volume(path[0]);
    }
    if path.len() >= 5
        && path[0] == b'\\'
        && path[1..5].eq_ignore_ascii_case(b"boot")
        && (path.len() == 5 || path[5] == b'\\')
        && let Some(v) = boot_volume()
    {
        return Some(v);
    }
    vfs::mounted_volume(b'C')
}
fn split(path: &[u8]) -> Option<(Bytes, Bytes)> {
    let path = strip(path);
    let i = path.iter().rposition(|b| *b == b'\\')? + 1;
    if i >= path.len() {
        return None;
    }
    Some((
        span(if i == 1 { b"/" } else { &path[..i - 1] }),
        span(&path[i..]),
    ))
}
fn checksum_entry(v: Volume, e: &crate::fs::vfs_types::Entry) -> Option<u32> {
    if e.is_dir() || e.size > usize::MAX as u64 {
        return None;
    }
    let mut state = journal::SEED;
    let mut done = 0;
    while done < e.size {
        let want = (e.size - done).min(65536) as usize;
        let output = field!(checksum).cast::<u8>();
        if unsafe { io::read_range(v, e, done, output, want) }? != want {
            return None;
        }
        state =
            journal::checksum_update(state, unsafe { core::slice::from_raw_parts(output, want) });
        done += want as u64
    }
    Some(state)
}
fn read_newest(v: Volume, out: *mut Journal) -> u8 {
    unsafe {
        journal::reset(out);
        let mut lengths = [0; 2];
        let mut present = [false; 2];
        let mut valid = [false; 2];
        let mut generations = [0; 2];
        for slot in 0..2 {
            let e = match io::resolve_entry(v, span(PATHS[slot])) {
                Ok(Some(e)) => e,
                Ok(None) => continue,
                Err(_) => return 2,
            };
            present[slot] = true;
            if e.is_dir() || e.size == 0 || e.size > JOURNAL_MAX as u64 {
                continue;
            }
            let expected = e.size as usize;
            let Some(got) = vfs::read_file_raw(v, &e, buffer(slot), expected) else {
                return 2;
            };
            if got != expected {
                return 2;
            }
            lengths[slot] = expected;
            if !journal::parse(
                core::slice::from_raw_parts(buffer(slot), expected),
                slot as u8,
                field!(parse),
            ) {
                continue;
            }
            valid[slot] = true;
            generations[slot] = (*field!(parse)).journal_generation;
        }
        let selected = match journal::select(present, valid, generations) {
            0 => 0,
            1 => 1,
            2 => return 1,
            3 => return 2,
            _ => unreachable!(),
        };
        if journal::parse(
            core::slice::from_raw_parts(buffer(selected), lengths[selected]),
            selected as u8,
            out,
        ) {
            0
        } else {
            2
        }
    }
}
struct Restore {
    journal: *mut Journal,
    slot: u8,
    generation: u64,
    valid: bool,
    durable: bool,
}
impl Drop for Restore {
    fn drop(&mut self) {
        if !self.durable {
            unsafe {
                (*self.journal).slot = self.slot;
                (*self.journal).journal_generation = self.generation;
                (*self.journal).valid = self.valid
            }
        }
    }
}
unsafe fn persist(v: Volume, dir: u64, j: *mut Journal) -> bool {
    unsafe {
        if (*j).journal_generation == u64::MAX {
            return false;
        }
        let mut restore = Restore {
            journal: j,
            slot: (*j).slot,
            generation: (*j).journal_generation,
            valid: (*j).valid,
            durable: false,
        };
        (*j).slot = 1 - (*j).slot;
        (*j).journal_generation += 1;
        (*j).valid = true;
        let Some(len) = journal::serialize(&*j, &mut *field!(write)) else {
            return false;
        };
        let encoded = Bytes {
            data: field!(write).cast(),
            len,
        };
        let slot = (*j).slot as usize;
        // An error receipt may still have written the inactive slot. Only exact
        // readback plus a valid journal authenticates its durable completion.
        let _ = io::write(v, dir, span(NAMES[slot]), encoded);
        let e = match io::resolve_entry(v, span(PATHS[slot])) {
            Ok(Some(e)) => e,
            _ => return false,
        };
        if e.is_dir() || e.size != len as u64 {
            return false;
        }
        let Some(got) = vfs::read_file_raw(v, &e, buffer(slot), len) else {
            return false;
        };
        if got != len || bytes(encoded) != core::slice::from_raw_parts(buffer(slot), len) {
            return false;
        }
        if !journal::parse(
            core::slice::from_raw_parts(buffer(slot), len),
            slot as u8,
            field!(verify),
        ) {
            return false;
        }
        let verify = field!(verify);
        restore.durable = (*verify).transaction_generation == (*j).transaction_generation
            && (*verify).journal_generation == (*j).journal_generation
            && (*verify).phase == (*j).phase;
        restore.durable
    }
}
struct BootIo {
    volume: Volume,
    directory: u64,
}
fn mutation_status(status: u8) -> u32 {
    match status {
        0 => 0,
        2 => 2,
        1 | 3 => 1,
        _ => panic!("Invalid checked update result"),
    }
}
impl ReplayIo for BootIo {
    fn path_state(&mut self, path: Bytes, size: u64, sum: u32) -> u32 {
        unsafe {
            let path = bytes(path);
            let Some(v) = payload_volume(path) else {
                return 3;
            };
            let e = match io::resolve_entry(v, span(strip(path))) {
                Ok(Some(e)) => e,
                Ok(None) => return 1,
                Err(_) => return 3,
            };
            if e.is_dir() || e.size != size {
                return 2;
            }
            match checksum_entry(v, &e) {
                Some(actual) => {
                    if actual == sum {
                        0
                    } else {
                        2
                    }
                }
                None => 3,
            }
        }
    }
    fn presence(&mut self, path: Bytes) -> u32 {
        unsafe {
            let path = bytes(path);
            let Some(v) = payload_volume(path) else {
                return 3;
            };
            match io::resolve_entry(v, span(strip(path))) {
                Ok(Some(e)) => {
                    if e.is_dir() {
                        1
                    } else {
                        0
                    }
                }
                Ok(None) => 2,
                Err(_) => 3,
            }
        }
    }
    unsafe fn mutate(&mut self, op: u32, e: *const Payload) -> u32 {
        unsafe {
            let target = span((*e).target());
            let stage = span((*e).stage());
            let backup = span((*e).backup());
            let previous = span((*e).previous());
            if op == 1 && !(*e).replace_required {
                return match self.path_state(target, (*e).size, (*e).checksum) {
                    0 => 0,
                    3 => 2,
                    _ => 1,
                };
            }
            let (t, s, b) = (bytes(target), bytes(stage), bytes(backup));
            // Preserve ordering: transition rejects a different drive before its
            // first volume probe; cleanup retains its original parent check.
            if op != 2 && (drive(t) != drive(s) || drive(t) != drive(b)) {
                return 1;
            }
            let Some(v) = payload_volume(t) else { return 2 };
            let Some((tp, tn)) = split(t) else { return 1 };
            let Some((sp, sn)) = split(s) else { return 1 };
            let Some((bp, bn)) = split(b) else { return 1 };
            if !journal::same(bytes(tp), bytes(sp)) || !journal::same(bytes(tp), bytes(bp)) {
                return 1;
            }
            let mut previous_name = span(b"");
            if op == 2 && previous.len != 0 && !journal::same(bytes(previous), b) {
                let Some((pp, pn)) = split(bytes(previous)) else {
                    return 1;
                };
                if !journal::same(bytes(tp), bytes(pp)) {
                    return 1;
                }
                previous_name = pn
            }
            let parent = match io::resolve(v, tp) {
                Ok(Some(parent)) => parent,
                Ok(None) => return if op == 2 { 2 } else { 1 },
                Err(_) => return 2,
            };
            let result = if op == 2 {
                atomic::cleanup(
                    v,
                    parent,
                    tn,
                    sn,
                    bn,
                    previous_name,
                    atomic::CleanupSpec {
                        new_size: (*e).size,
                        new_checksum: (*e).checksum,
                        target_existed: (*e).target_existed,
                        old_known: (*e).old_known,
                        old_size: (*e).old_size,
                        old_checksum: (*e).old_checksum,
                        previous_known: (*e).previous_backup_known,
                        previous_size: (*e).previous_backup_size,
                        previous_checksum: (*e).previous_backup_checksum,
                    },
                )
            } else {
                atomic::transition(
                    v,
                    parent,
                    tn,
                    sn,
                    bn,
                    if op == 0 { 1 } else { 0 },
                    atomic::Spec {
                        long_stage: false,
                        target_existed: (*e).target_existed,
                        old_known: (*e).old_known,
                        new_size: (*e).size,
                        new_checksum: (*e).checksum,
                        old_size: (*e).old_size,
                        old_checksum: (*e).old_checksum,
                    },
                )
            };
            mutation_status(result)
        }
    }
    unsafe fn persist(&mut self, j: *mut Journal) -> bool {
        unsafe { persist(self.volume, self.directory, j) }
    }
}
pub fn recover_before_runtime() -> bool {
    unsafe {
        let Some(v) = vfs::mounted_volume(b'C') else {
            return true;
        };
        let dir = match io::resolve(v, span(DIRECTORY)) {
            Ok(Some(d)) => d,
            Ok(None) => return true,
            Err(_) => return false,
        };
        let j = field!(active);
        match read_newest(v, j) {
            0 => {}
            1 => return true,
            2 => return false,
            _ => unreachable!(),
        }
        if journal::terminal((*j).phase) {
            return true;
        }
        let mut io = BootIo {
            volume: v,
            directory: dir,
        };
        let status = if (*j).batch {
            match (*j).phase {
                6 | 7 | 9 => 0,
                1 | 2 => {
                    let forward = recovery::forward(&mut io, j);
                    if forward == 2 {
                        recovery::rollback(&mut io, j)
                    } else {
                        forward
                    }
                }
                5 => recovery::cleanup(&mut io, j),
                0 | 3 | 4 | 8 => recovery::rollback(&mut io, j),
                _ => panic!("Invalid boot journal phase"),
            }
        } else {
            match (*j).phase {
                5 => recovery::cleanup(&mut io, j),
                0..=4 | 6 | 8 => recovery::rollback(&mut io, j),
                7 | 9 => 0,
                _ => panic!("Invalid boot journal phase"),
            }
        };
        if status != 0 {
            foundation::puts(b"  SYSUPD recovery [FAILED: ");
            foundation::puts(match status {
                1 => b"journal-invalid",
                2 => b"state-conflict",
                3 => b"storage-io",
                _ => unreachable!(),
            });
            foundation::puts(b"]\r\n");
            return false;
        }
        foundation::puts(b"  SYSUPD recovery [OK]\r\n");
        true
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_update_boot_recover() -> bool {
    recover_before_runtime()
}
