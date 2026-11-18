//! The caller owns the directory's mutation admission from name selection
//! through publication. Readers may see a completed stage but cannot replace it.
use super::{Args, Entry, Volume, op, run, t};
use core::{
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

pub(crate) struct Target<'a> {
    volume: Volume,
    parent: u64,
    name: &'a [u8],
    pub(crate) stage: [u8; 12],
    backup: [u8; 12],
}

fn bytes(name: &[u8]) -> t::Bytes {
    t::Bytes {
        data: name.as_ptr(),
        len: name.len(),
    }
}

impl<'a> Target<'a> {
    pub(crate) unsafe fn select(volume: Volume, parent: u64, name: &'a [u8]) -> Option<Self> {
        let mut target = Self {
            volume,
            parent,
            name,
            stage: *b"~C000000.TMP",
            backup: *b"~C000000.BAK",
        };
        'names: for _ in 0..128 {
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            for i in 0..6 {
                target.stage[i + 2] =
                    b"0123456789ABCDEF"[((sequence >> ((5 - i) * 4)) & 15) as usize];
            }
            target.backup[..8].copy_from_slice(&target.stage[..8]);
            for sibling in [&target.stage, &target.backup] {
                let args = Args {
                    second: bytes(sibling),
                    ..target.args(name)
                };
                let equal = unsafe { run(op::NAMES, args, ptr::null_mut()) };
                if !equal.present {
                    return None;
                }
                if equal.value != 0 || target.lookup(sibling).ok()?.is_some() {
                    continue 'names;
                }
            }
            return Some(target);
        }
        None
    }

    fn args(&self, name: &[u8]) -> Args {
        Args {
            volume: self.volume,
            parent: self.parent,
            name: bytes(name),
            ..Default::default()
        }
    }

    fn lookup(&self, name: &[u8]) -> Result<Option<Entry>, ()> {
        let mut entry = Entry::EMPTY;
        match unsafe { run(op::RECOVERY_LOOKUP, self.args(name), &mut entry) }.status {
            0 => Ok(Some(entry)),
            1 => Ok(None),
            _ => Err(()),
        }
    }

    fn same_payload(&self, a: &Entry, b: &Entry) -> bool {
        !a.is_dir()
            && !b.is_dir()
            && a.node == b.node
            && a.generation == b.generation
            && a.size == b.size
    }

    fn remove(&self, name: &[u8], expected: &Entry) -> bool {
        let args = Args {
            entry: expected,
            ..self.args(name)
        };
        matches!(
            unsafe { run(op::DELETE_MATCH, args, ptr::null_mut()) }.status,
            0 | 1
        )
    }

    fn create_from(&self, source: &[u8]) -> bool {
        let args = Args {
            second: bytes(self.name),
            ..self.args(source)
        };
        unsafe { run(op::PUBLISH_COPY, args, ptr::null_mut()) }.present
    }

    fn install(&self, replacing: bool) -> bool {
        if !replacing {
            return self.create_from(&self.stage);
        }
        let args = Args {
            second: bytes(&self.stage),
            third: bytes(&self.backup),
            flag: true,
            ..self.args(self.name)
        };
        unsafe { run(op::REPLACE, args, ptr::null_mut()) }.status == 0
    }

    // After the first namespace write, generic stage deletion could free a
    // FAT chain already shared with the target. Reconcile the exact tuple.
    #[inline(never)]
    pub(crate) fn publish(&self, original: Option<&Entry>) -> bool {
        let Ok(Some(staged)) = self.lookup(&self.stage) else {
            return false;
        };
        if self.install(original.is_some()) {
            return original.is_none_or(|old| self.remove(&self.backup, old));
        }
        for attempt in 0..2 {
            let (Ok(target), Ok(stage), Ok(backup)) = (
                self.lookup(self.name),
                self.lookup(&self.stage),
                self.lookup(&self.backup),
            ) else {
                return false;
            };
            if stage
                .as_ref()
                .is_some_and(|s| !self.same_payload(s, &staged))
                || backup
                    .as_ref()
                    .is_some_and(|b| !original.is_some_and(|old| self.same_payload(b, old)))
            {
                return false;
            }
            let new_target = target
                .as_ref()
                .is_some_and(|t| self.same_payload(t, &staged));
            let old_target = target
                .as_ref()
                .is_some_and(|t| original.is_some_and(|old| self.same_payload(t, old)));
            if new_target && stage.is_none() {
                return unsafe { run(op::FLUSH, self.args(self.name), ptr::null_mut()) }.present
                    && original.is_none_or(|old| self.remove(&self.backup, old));
            }
            if target.is_some() && !new_target && !old_target {
                return false;
            }
            if attempt == 0 && stage.is_some() {
                if self.install(original.is_some()) {
                    return original.is_none_or(|old| self.remove(&self.backup, old));
                }
                continue;
            }
            // NTFS can already have moved the old name to the backup while
            // installation of the new record failed. Restore the old name.
            if !new_target && backup.is_some() && !self.create_from(&self.backup) {
                return false;
            }
            if !new_target
                && stage.is_some()
                && (old_target || original.is_none() || backup.is_some())
            {
                self.remove(&self.stage, &staged);
            }
            return false;
        }
        false
    }
}
