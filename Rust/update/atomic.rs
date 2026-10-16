//! Checked SYSUPD ownership transitions. Lookups, fingerprints, exact alias
//! identity, mutation and verification retain the caller's one request gate.
use super::owner::Exclusive;
use super::{claim::bytes, io, journal};
use crate::fs::{
    request,
    vfs_types::{Bytes, Entry, Volume},
};
use core::cell::UnsafeCell;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Spec {
    pub long_stage: bool,
    pub target_existed: bool,
    pub old_known: bool,
    pub new_size: u64,
    pub new_checksum: u32,
    pub old_size: u64,
    pub old_checksum: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CleanupSpec {
    pub new_size: u64,
    pub new_checksum: u32,
    pub target_existed: bool,
    pub old_known: bool,
    pub old_size: u64,
    pub old_checksum: u32,
    pub previous_known: bool,
    pub previous_size: u64,
    pub previous_checksum: u32,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Content {
    Missing,
    New,
    Old,
    Other,
}
#[derive(Clone, Copy)]
struct Observed {
    entry: Option<Entry>,
    content: Content,
}
#[repr(u8)]
#[derive(Clone, Copy)]
enum Failure {
    Conflict = 1,
    Io = 2,
    NotAtomic = 3,
}
type Outcome = Result<(), Failure>;
struct Buffer(UnsafeCell<[u8; 65536]>);
// SAFETY: ATOMIC_ADMISSION serializes the resident checksum scratch across
// different drive requests. No scratch reference survives a cooperative yield.
unsafe impl Sync for Buffer {}
static ATOMIC_ADMISSION: Exclusive = Exclusive::new(b"sysupd-atomic-scratch");
static CHECKSUM: Buffer = Buffer(UnsafeCell::new([0; 65536]));
fn progress(phase: u32, done: u64, total: u64) {
    unsafe { request::r4os_rust_fs_request_atomic_progress(phase, done, total) }
}
fn checksum_entry(v: Volume, e: &Entry) -> Option<u32> {
    let mut state = journal::SEED;
    let mut offset = 0u64;
    let total = e.size.div_ceil(65536);
    let mut completed = 0;
    progress(2, 0, total);
    while offset < e.size {
        let want = (e.size - offset).min(65536) as usize;
        let buffer = CHECKSUM.0.get().cast::<u8>();
        let got = unsafe { io::read_range(v, e, offset, buffer, want) }?;
        if got != want {
            return None;
        }
        state =
            journal::checksum_update(state, unsafe { core::slice::from_raw_parts(buffer, want) });
        offset += want as u64;
        completed += 1;
        progress(2, completed, total);
        crate::sched::scheduler::yield_task();
    }
    Some(state)
}
fn observe(v: Volume, parent: u64, name: Bytes, s: Spec) -> Result<Observed, Failure> {
    progress(1, 0, 0);
    let e = io::lookup(v, parent, name, true).map_err(|_| Failure::Io)?;
    let Some(entry) = e else {
        return Ok(Observed {
            entry: None,
            content: Content::Missing,
        });
    };
    if entry.is_dir() || entry.size > usize::MAX as u64 {
        return Ok(Observed {
            entry: Some(entry),
            content: Content::Other,
        });
    }
    let sum = checksum_entry(v, &entry).ok_or(Failure::Io)?;
    let content = if entry.size == s.new_size && sum == s.new_checksum {
        Content::New
    } else if s.old_known && entry.size == s.old_size && sum == s.old_checksum {
        Content::Old
    } else {
        Content::Other
    };
    Ok(Observed {
        entry: Some(entry),
        content,
    })
}
fn mutate(v: Volume, parent: u64, t: Bytes, s: Bytes, b: Bytes) -> Outcome {
    progress(3, 0, 0);
    match io::replace(v, parent, t, s, b, false) {
        0 => Ok(()),
        -6 => Err(Failure::Io),
        -7 => Err(Failure::NotAtomic),
        _ => Err(Failure::Conflict),
    }
}
fn delete(v: Volume, parent: u64, name: Bytes, entry: &Entry) -> Outcome {
    progress(4, 0, 0);
    match io::delete_identity(v, parent, name, entry, false) {
        0 | 1 => {
            if io::flush(v) {
                Ok(())
            } else {
                Err(Failure::Io)
            }
        }
        2 => Err(Failure::Conflict),
        3 => Err(Failure::Io),
        _ => panic!("Invalid checked delete receipt"),
    }
}
fn existing_allowed(v: Volume, t: Observed, s: Observed, b: Observed, spec: Spec) -> bool {
    use Content::*;
    if !matches!(s.content, Missing | New) {
        return false;
    }
    if spec.old_known {
        if !matches!(b.content, Missing | Old) {
            return false;
        }
        return match t.content {
            Old => match b.entry {
                Some(be) => t.entry.is_some_and(|te| io::same_identity(v, &te, &be)),
                None => true,
            },
            Missing => b.content == Old && s.content == New,
            New => {
                b.content == Old
                    && match s.entry {
                        Some(se) => t.entry.is_some_and(|te| io::same_identity(v, &te, &se)),
                        None => true,
                    }
            }
            Other => false,
        };
    }
    if t.content != Other {
        return false;
    }
    match b.entry {
        Some(be) => b.content == Other && t.entry.is_some_and(|te| io::same_identity(v, &te, &be)),
        None => true,
    }
}
fn create_allowed(v: Volume, t: Observed, s: Observed, b: Observed) -> bool {
    use Content::*;
    b.content == Missing
        && matches!(t.content, Missing | New)
        && matches!(s.content, Missing | New)
        && match (t.entry, s.entry) {
            (Some(te), Some(se)) => io::same_identity(v, &te, &se),
            _ => true,
        }
}
#[derive(Clone, Copy)]
struct Context {
    v: Volume,
    parent: u64,
    t: Bytes,
    s: Bytes,
    b: Bytes,
    spec: Spec,
}
impl Context {
    fn observe(&self, name: Bytes) -> Result<Observed, Failure> {
        observe(self.v, self.parent, name, self.spec)
    }
    fn verify_existing(&self, rollback: bool) -> Outcome {
        use Content::*;
        let mut t = self.observe(self.t)?;
        let mut s = self.observe(self.s)?;
        let b = self.observe(self.b)?;
        if !rollback {
            if t.content != New || s.content != Missing || b.content != Old {
                return Err(Failure::Conflict);
            }
        } else {
            if t.content != Old || b.content != Missing {
                return Err(Failure::Conflict);
            }
            if let Some(entry) = s.entry {
                if s.content != New {
                    return Err(Failure::Conflict);
                }
                delete(self.v, self.parent, self.s, &entry)?
            }
            t = self.observe(self.t)?;
            s = self.observe(self.s)?;
            if t.content != Old || s.content != Missing {
                return Err(Failure::Conflict);
            }
        }
        if io::flush(self.v) {
            Ok(())
        } else {
            Err(Failure::Io)
        }
    }
    fn existing_forward(&self, t: Observed, s: Observed, b: Observed) -> Outcome {
        use Content::*;
        if !(t.content == New && s.content == Missing && b.content == Old) {
            mutate(self.v, self.parent, self.t, self.s, self.b)?
        }
        self.verify_existing(false)
    }
    fn existing_rollback(&self, t: Observed, s: Observed, b: Observed) -> Outcome {
        use Content::*;
        if t.content == Old && b.content == Missing {
            return self.verify_existing(true);
        }
        self.existing_forward(t, s, b)?;
        mutate(self.v, self.parent, self.t, self.b, self.s)?;
        self.verify_existing(true)
    }
    fn legacy_rollback(&self, s: Observed, b: Observed) -> Outcome {
        use Content::*;
        if b.entry.is_some() {
            if s.content != New {
                return Err(Failure::Conflict);
            }
            mutate(self.v, self.parent, self.t, self.s, self.b)?;
            mutate(self.v, self.parent, self.t, self.b, self.s)?
        }
        let mut t = self.observe(self.t)?;
        let mut s = self.observe(self.s)?;
        let b = self.observe(self.b)?;
        if t.content != Other || b.content != Missing {
            return Err(Failure::Conflict);
        }
        if let Some(entry) = s.entry {
            if s.content != New {
                return Err(Failure::Conflict);
            }
            delete(self.v, self.parent, self.s, &entry)?
        }
        t = self.observe(self.t)?;
        s = self.observe(self.s)?;
        if t.content == Other && s.content == Missing && io::flush(self.v) {
            Ok(())
        } else {
            Err(Failure::Conflict)
        }
    }
    fn create_forward(&self, t: Observed, s: Observed) -> Outcome {
        use Content::*;
        if !(t.content == New && s.content == Missing) {
            mutate(self.v, self.parent, self.t, self.s, self.b)?
        }
        let t = self.observe(self.t)?;
        let s = self.observe(self.s)?;
        let b = self.observe(self.b)?;
        if t.content == New && s.content == Missing && b.content == Missing && io::flush(self.v) {
            Ok(())
        } else {
            Err(Failure::Conflict)
        }
    }
    fn create_rollback(&self, t: Observed, s: Observed) -> Outcome {
        use Content::*;
        if t.entry.is_some() && s.entry.is_some() {
            mutate(self.v, self.parent, self.t, self.s, self.b)?
        }
        let mut t = self.observe(self.t)?;
        let mut s = self.observe(self.s)?;
        let b = self.observe(self.b)?;
        if b.content != Missing {
            return Err(Failure::Conflict);
        }
        if let Some(entry) = t.entry {
            if t.content != New {
                return Err(Failure::Conflict);
            }
            delete(self.v, self.parent, self.t, &entry)?
        }
        if let Some(entry) = s.entry {
            if s.content != New {
                return Err(Failure::Conflict);
            }
            delete(self.v, self.parent, self.s, &entry)?
        }
        t = self.observe(self.t)?;
        s = self.observe(self.s)?;
        if t.content == Missing && s.content == Missing && io::flush(self.v) {
            Ok(())
        } else {
            Err(Failure::Conflict)
        }
    }
}
fn transition_inner(c: Context, rollback: bool) -> Outcome {
    unsafe {
        let (t, s, b) = (bytes(c.t), bytes(c.s), bytes(c.b));
        let spec = c.spec;
        if t.is_empty()
            || s.is_empty()
            || b.is_empty()
            || (!io::short_name(s) && !spec.long_stage)
            || !io::short_name(b)
        {
            return Err(Failure::Conflict);
        }
        if spec.long_stage && (rollback || c.v.kind != 0) {
            return Err(Failure::NotAtomic);
        }
        if t.eq_ignore_ascii_case(s)
            || t.eq_ignore_ascii_case(b)
            || s.eq_ignore_ascii_case(b)
            || (!spec.target_existed && spec.old_known)
        {
            return Err(Failure::Conflict);
        }
        let target = c.observe(c.t)?;
        let stage = c.observe(c.s)?;
        let backup = c.observe(c.b)?;
        if spec.target_existed {
            if !existing_allowed(c.v, target, stage, backup, spec) {
                return Err(Failure::Conflict);
            }
            if !spec.old_known {
                if target.content != Content::Other {
                    return Err(Failure::Conflict);
                }
                return c.legacy_rollback(stage, backup);
            }
            if rollback {
                c.existing_rollback(target, stage, backup)
            } else {
                c.existing_forward(target, stage, backup)
            }
        } else {
            if !create_allowed(c.v, target, stage, backup) {
                return Err(Failure::Conflict);
            }
            if rollback {
                c.create_rollback(target, stage)
            } else {
                c.create_forward(target, stage)
            }
        }
    }
}
pub fn transition(
    v: Volume,
    parent: u64,
    t: Bytes,
    s: Bytes,
    b: Bytes,
    direction: u8,
    spec: Spec,
) -> u8 {
    let Some(_scratch) = ATOMIC_ADMISSION.acquire() else {
        return Failure::Io as u8;
    };
    match transition_inner(
        Context {
            v,
            parent,
            t,
            s,
            b,
            spec,
        },
        direction == 1,
    ) {
        Ok(()) => 0,
        Err(e) => e as u8,
    }
}
fn cleanup_inner(
    v: Volume,
    parent: u64,
    t: Bytes,
    s: Bytes,
    b: Bytes,
    previous: Bytes,
    spec: CleanupSpec,
) -> Outcome {
    unsafe {
        let (tn, sn, bn, pn) = (bytes(t), bytes(s), bytes(b), bytes(previous));
        if tn.is_empty()
            || sn.is_empty()
            || bn.is_empty()
            || !io::short_name(sn)
            || !io::short_name(bn)
            || (!pn.is_empty() && !io::short_name(pn))
        {
            return Err(Failure::NotAtomic);
        }
        let probe = Spec {
            long_stage: false,
            target_existed: spec.target_existed,
            old_known: spec.old_known,
            new_size: spec.new_size,
            new_checksum: spec.new_checksum,
            old_size: spec.old_size,
            old_checksum: spec.old_checksum,
        };
        let target = observe(v, parent, t, probe)?;
        if target.content != Content::New {
            return Err(Failure::Conflict);
        }
        let stage = observe(v, parent, s, probe)?;
        if stage.entry.is_some() {
            return Err(Failure::Conflict);
        }
        let backup = observe(v, parent, b, probe)?;
        if spec.target_existed {
            if backup.entry.is_none() || (spec.old_known && backup.content != Content::Old) {
                return Err(Failure::Conflict);
            }
        } else if backup.entry.is_some() {
            return Err(Failure::Conflict);
        }
        if pn.is_empty()
            || pn.eq_ignore_ascii_case(bn)
            || !spec.target_existed
            || !spec.previous_known
        {
            return Ok(());
        }
        let probe = Spec {
            long_stage: false,
            target_existed: true,
            old_known: false,
            new_size: spec.previous_size,
            new_checksum: spec.previous_checksum,
            old_size: 0,
            old_checksum: 0,
        };
        let previous_entry = observe(v, parent, previous, probe)?;
        let Some(entry) = previous_entry.entry else {
            return Ok(());
        };
        if previous_entry.content != Content::New {
            return Ok(());
        }
        delete(v, parent, previous, &entry)
    }
}
pub fn cleanup(
    v: Volume,
    parent: u64,
    t: Bytes,
    s: Bytes,
    b: Bytes,
    previous: Bytes,
    spec: CleanupSpec,
) -> u8 {
    let Some(_scratch) = ATOMIC_ADMISSION.acquire() else {
        return Failure::Io as u8;
    };
    match cleanup_inner(v, parent, t, s, b, previous, spec) {
        Ok(()) => 0,
        Err(e) => e as u8,
    }
}
/// # Safety
/// Copied neutral volume/spec and bounded names retain the caller's exact gate.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_update_transition(
    v: *const Volume,
    parent: u64,
    t: Bytes,
    s: Bytes,
    b: Bytes,
    direction: u8,
    spec: *const Spec,
) -> u8 {
    unsafe { transition(v.read(), parent, t, s, b, direction, spec.read()) }
}
/// # Safety
/// Copied neutral volume/spec and all four names retain one checked request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_update_cleanup(
    v: *const Volume,
    parent: u64,
    t: Bytes,
    s: Bytes,
    b: Bytes,
    previous: Bytes,
    spec: *const CleanupSpec,
) -> u8 {
    unsafe { cleanup(v.read(), parent, t, s, b, previous, spec.read()) }
}

const _: () = {
    assert!(core::mem::size_of::<Spec>() == 40);
    assert!(core::mem::size_of::<CleanupSpec>() == 48);
    assert!(core::mem::offset_of!(Spec, new_size) == 8);
    assert!(core::mem::offset_of!(CleanupSpec, previous_size) == 32);
};
