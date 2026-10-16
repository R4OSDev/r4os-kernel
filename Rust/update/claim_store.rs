//! Durable create-only publish claims. Writes use the publisher's existing
//! filesystem request; replay runs before any normal runtime consumer.
use super::owner::Exclusive;
use super::{
    claim::{self, Build, Claim, Identity, Summary, bytes, span},
    io,
};
use crate::fs::{
    vfs,
    vfs_types::{Args, Bytes, Entry, Volume, op},
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr};
pub const DIRECTORY: &[u8] = b"/R4OS/UPDATE/CLAIMS";
struct State {
    write: [u8; 4096],
    read: [u8; 4096],
    claim: Claim,
    replay: Claim,
    name: [u8; 16],
    entry_name: [u8; 768],
    next_generation: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: STORE_ADMISSION serializes all resident scratch and generations
// across different drive requests. Boot replay uses the same exclusive owner.
// No reference into State survives a VFS wait; backend leaves open no request.
unsafe impl Sync for Storage {}
static STORE_ADMISSION: Exclusive = Exclusive::new(b"upload-claim-store");
static STATE: Storage = Storage(UnsafeCell::new(State {
    write: [0; 4096],
    read: [0; 4096],
    claim: Claim::EMPTY,
    replay: Claim::EMPTY,
    name: [0; 16],
    entry_name: [0; 768],
    next_generation: 1,
}));
impl State {
    fn write() -> *mut [u8; 4096] {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).write) }
    }
    fn read() -> *mut [u8; 4096] {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).read) }
    }
    fn claim() -> *mut Claim {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).claim) }
    }
    fn replay() -> *mut Claim {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).replay) }
    }
    fn name() -> *mut [u8; 16] {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).name) }
    }
    fn entry_name() -> *mut [u8; 768] {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).entry_name) }
    }
    fn next_generation() -> *mut u64 {
        unsafe { ptr::addr_of_mut!((*STATE.0.get()).next_generation) }
    }
}
macro_rules! field {
    ($f:ident) => {
        State::$f()
    };
}
pub fn volume_token(v: Volume) -> u64 {
    if v.kind == 0 {
        (1u64 << 62) | ((v.fat.device as u64) << 48) | u64::from(v.fat.partition)
    } else {
        (2u64 << 62) | u64::from(v.ntfs.slot)
    }
}
fn name(generation: u64) -> Bytes {
    let out = field!(name).cast::<u8>();
    for i in 0..8 {
        unsafe { *out.add(i) = b"0123456789ABCDEF"[((generation >> ((7 - i) * 4)) & 15) as usize] }
    }
    unsafe { ptr::copy_nonoverlapping(b".CLM".as_ptr(), out.add(8), 4) }
    Bytes { data: out, len: 12 }
}
fn catalog_admitted() -> bool {
    #[cfg(r4os_owner_fixture)]
    {
        true
    }
    #[cfg(not(r4os_owner_fixture))]
    {
        crate::sched::scheduler::current().is_null() || crate::fs::request::owns_drive(b'C')
    }
}
fn directory(v: Volume) -> Option<u64> {
    match io::resolve(v, span(DIRECTORY)) {
        Ok(Some(dir)) => return Some(dir),
        Err(_) => return None,
        Ok(None) => {}
    }
    let parent = match io::resolve(v, span(b"/R4OS/UPDATE")) {
        Ok(Some(p)) => p,
        Err(_) => return None,
        Ok(None) => {
            let root = io::resolve(v, span(b"/R4OS")).ok()??;
            let _ = io::mkdir(v, root, span(b"UPDATE"));
            io::resolve(v, span(b"/R4OS/UPDATE")).ok()??
        }
    };
    let _ = io::mkdir(v, parent, span(b"CLAIMS"));
    io::resolve(v, span(DIRECTORY)).ok()?
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WriteArgs {
    pub volume: Volume,
    pub parent: Bytes,
    pub stage: Bytes,
    pub target: Bytes,
    pub backup: Bytes,
    pub identity: Identity,
    pub protocol: u8,
    pub reuse_generation: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WriteResult {
    pub ok: bool,
    pub generation: u64,
}
const FAILED: WriteResult = WriteResult {
    ok: false,
    generation: 0,
};
pub fn write(a: &WriteArgs) -> WriteResult {
    let Some(_scratch) = STORE_ADMISSION.acquire() else {
        return FAILED;
    };
    let Some(v) = vfs::mounted_volume(b'C') else {
        return FAILED;
    };
    let Some(dir) = directory(v) else {
        return FAILED;
    };
    let generation = if a.reuse_generation != 0 {
        a.reuse_generation
    } else {
        unsafe { *field!(next_generation) }
    };
    let build = Build {
        generation,
        volume: volume_token(a.volume),
        parent: a.parent,
        stage: a.stage,
        target: a.target,
        backup: a.backup,
        identity: a.identity,
        protocol: a.protocol,
    };
    if !unsafe { claim::build(field!(claim), &build) } {
        return FAILED;
    }
    let encoded = unsafe { claim::serialize(&*field!(claim), &mut *field!(write)) };
    let Some(len) = encoded else { return FAILED };
    if !io::write(
        v,
        dir,
        name(generation),
        Bytes {
            data: field!(write).cast(),
            len,
        },
    ) || !io::flush(v)
    {
        return FAILED;
    }
    if a.reuse_generation == 0 {
        unsafe {
            let p = field!(next_generation);
            *p = (*p).wrapping_add(1);
            if *p == 0 {
                *p = 1
            }
        }
    }
    WriteResult {
        ok: true,
        generation,
    }
}
fn retire_in(v: Volume, dir: u64, generation: u64) -> bool {
    let n = name(generation);
    let _ = io::delete(v, dir, n);
    io::flush(v) && matches!(io::lookup(v, dir, n, false), Ok(None))
}
pub fn retire(generation: u64) -> bool {
    if generation == 0 {
        return true;
    }
    if !catalog_admitted() {
        return false;
    }
    let Some(_scratch) = STORE_ADMISSION.acquire() else {
        return false;
    };
    let Some(v) = vfs::mounted_volume(b'C') else {
        return false;
    };
    match io::resolve(v, span(DIRECTORY)) {
        Ok(Some(dir)) => retire_in(v, dir, generation),
        Ok(None) => true,
        Err(_) => false,
    }
}
struct Replay {
    volume: Volume,
    directory: u64,
}
unsafe fn resolve_target(c: *const Claim) -> Option<(Volume, u64)> {
    unsafe {
        let drive = (*c).parent();
        if drive.len() < 3 || drive[1] != b':' {
            return None;
        }
        let letter = drive[0];
        let path = span(&drive[2..]);
        let token = (*c).volume;
        let v = vfs::mounted_volume(letter)?;
        if volume_token(v) != token {
            return None;
        }
        let dir = io::resolve(v, path).ok()??;
        Some((v, dir))
    }
}
fn identity(e: &Entry) -> Identity {
    Identity {
        node: e.node,
        generation: e.generation,
        size: e.size,
    }
}
impl claim::ReplayIo for Replay {
    unsafe fn invoke(&mut self, op: u32, c: *const Claim) -> u32 {
        unsafe {
            if op == 3 {
                return u32::from(retire_in(self.volume, self.directory, (*c).generation));
            }
            let Some((v, parent)) = resolve_target(c) else {
                return if op == 0 { 5 } else { 0 };
            };
            let stage = span((*c).stage());
            let target = span((*c).target());
            let backup = span((*c).backup());
            let expected = (*c).identity;
            match op {
                0 => {
                    let stage_entry = match io::lookup(v, parent, stage, true) {
                        Ok(e) => e,
                        Err(_) => return 6,
                    };
                    let target_entry = match io::lookup(v, parent, target, true) {
                        Ok(e) => e,
                        Err(_) => return 6,
                    };
                    if stage_entry.is_none() && target_entry.is_none() {
                        return 4;
                    }
                    if stage_entry.is_some_and(|e| identity(&e) != expected)
                        || target_entry.is_some_and(|e| identity(&e) != expected)
                    {
                        return 5;
                    }
                    if let (Some(s), Some(t)) = (stage_entry, target_entry) {
                        return if s.node != t.node || s.generation != t.generation {
                            5
                        } else {
                            2
                        };
                    }
                    if target_entry.is_some() {
                        return 3;
                    }
                    match io::lookup(v, parent, stage, false) {
                        Ok(Some(_)) => 0,
                        Err(_) => 1,
                        Ok(None) => 6,
                    }
                }
                1 => u32::from(
                    io::replace(v, parent, target, stage, backup, true) == 0 && io::flush(v),
                ),
                2 => {
                    let mut e = Entry::EMPTY;
                    e.node = expected.node;
                    e.generation = expected.generation;
                    e.size = expected.size;
                    u32::from(
                        matches!(io::delete_identity(v, parent, stage, &e, true), 0..=2)
                            && io::flush(v),
                    )
                }
                _ => panic!("Invalid upload replay I/O operation"),
            }
        }
    }
}
/// # Safety
/// Exact summary is writable under the original linear pre-runtime admission.
pub unsafe fn recover(summary: *mut Summary) -> bool {
    unsafe {
        summary.write(Summary::default());
        let Some(_scratch) = STORE_ADMISSION.acquire() else {
            return false;
        };
        let Some(v) = vfs::mounted_volume(b'C') else {
            return true;
        };
        let dir = match io::resolve(v, span(DIRECTORY)) {
            Ok(Some(d)) => d,
            Ok(None) => return true,
            Err(_) => return false,
        };
        let mut replay = Replay {
            volume: v,
            directory: dir,
        };
        let (mut index, mut visited) = (2, 0);
        while visited < claim::MAX_CLAIMS {
            let mut out = MaybeUninit::<Entry>::uninit();
            let r = io::run(
                op::DIR_ENTRY,
                Args {
                    volume: v,
                    parent: dir,
                    index,
                    buffer: field!(entry_name).cast(),
                    buffer_len: 768,
                    ..Args::default()
                },
                out.as_mut_ptr(),
            );
            match r.status {
                0 => {}
                1 => break,
                2 => return false,
                _ => panic!("Invalid claim directory receipt"),
            };
            let e = out.assume_init();
            visited += 1;
            if e.is_dir() {
                index += 1;
                continue;
            }
            if e.size == 0 || e.size > claim::CLAIM_MAX as u64 {
                (*summary).invalid += 1;
                index += 1;
                continue;
            }
            let Some(got) = vfs::read_file_raw(v, &e, field!(read).cast(), e.size as usize) else {
                (*summary).failed += 1;
                index += 1;
                continue;
            };
            if !claim::parse(
                bytes(Bytes {
                    data: field!(read).cast(),
                    len: got,
                }),
                &mut *field!(replay),
            ) {
                (*summary).invalid += 1;
                index += 1;
                continue;
            }
            let status = claim::replay(&mut replay, field!(replay));
            (*summary).add(status);
            if matches!(status, 4 | 5) {
                index += 1
            }
        }
        true
    }
}
pub fn write_mode(refresh: bool, a: &WriteArgs) -> WriteResult {
    if !catalog_admitted() {
        return FAILED;
    }

    if refresh && a.reuse_generation == 0 {
        return FAILED;
    }
    let mut args = *a;
    if !refresh {
        args.reuse_generation = 0
    }
    write(&args)
}
/// # Safety
/// Exact fixed-layout volume projection is readable for this scalar query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_claim_volume(v: *const Volume) -> u64 {
    volume_token(unsafe { v.read() })
}
/// # Safety
/// Caller retains its existing namespace gate and exact argument/output records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_claim_store_write(
    refresh: bool,
    a: *const WriteArgs,
    out: *mut WriteResult,
) {
    unsafe { out.write(write_mode(refresh, &*a)) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_claim_store_retire(generation: u64) -> bool {
    retire(generation)
}
/// # Safety
/// Caller owns the exact summary under linear pre-runtime admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_claim_store_recover(summary: *mut Summary) -> bool {
    unsafe { recover(summary) }
}

const _: () = {
    assert!(core::mem::size_of::<WriteArgs>() == 192);
    assert!(core::mem::offset_of!(WriteArgs, identity) == 152);
    assert!(core::mem::offset_of!(WriteArgs, reuse_generation) == 184);
    assert!(core::mem::size_of::<WriteResult>() == 16);
};
