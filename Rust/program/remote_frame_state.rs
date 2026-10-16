//! Original bounded capture-demand/lease registry and nonzero dirty history.
//! Enclosing R4DESK owns storage and admission; no allocation or copies here.
use crate::display::error::Error;
use core::mem::size_of;
pub const HISTORY_CAPACITY: usize = 64;
pub const SNAPSHOT_CAPACITY: usize = 3;
pub const CONSUMER_CAPACITY: usize = 64;
pub const LEASES_PER_CONSUMER: usize = 8;
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Owner {
    pub id: u32,
    pub generation: u64,
}
impl Owner {
    pub fn valid(self) -> bool {
        self.id != 0 && self.generation != 0
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Consumer {
    pub owner: Owner,
    pub count: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Lease {
    pub id: u64,
    pub owner: Owner,
    pub snapshot: usize,
    pub epoch: u64,
    pub started_ns: u64,
}
#[repr(C)]
pub struct CaptureRegistry {
    pub consumers: [Consumer; CONSUMER_CAPACITY],
    pub leases: [Lease; CONSUMER_CAPACITY],
    pub references: [u32; SNAPSHOT_CAPACITY],
    pub count: u32,
    pub next_lease: u64,
    pub acquired: u64,
    pub max_reader_ns: u64,
}
impl CaptureRegistry {
    pub fn acquire(&mut self, owner: Owner) -> Result<u32, Error> {
        if !owner.valid() {
            return Err(Error::Invalid);
        }
        if self.count == 0x7fff_ffff {
            return Err(Error::Capacity);
        }
        let index = self
            .consumers
            .iter()
            .position(|value| value.count != 0 && value.owner == owner)
            .or_else(|| self.consumers.iter().position(|value| value.count == 0))
            .ok_or(Error::Capacity)?;
        self.consumers[index].owner = owner;
        self.consumers[index].count += 1;
        self.count += 1;
        Ok(self.count)
    }
    pub fn release(&mut self, owner: Owner) -> Result<u32, Error> {
        let index = self
            .consumers
            .iter()
            .position(|value| value.count != 0 && value.owner == owner)
            .ok_or(Error::Stale)?;
        let entry = &mut self.consumers[index];
        entry.count -= 1;
        self.count -= 1;
        if entry.count == 0 {
            *entry = Consumer::default();
        }
        Ok(self.count)
    }
    pub fn has_consumer(&self, owner: Owner) -> bool {
        self.consumers
            .iter()
            .any(|value| value.count != 0 && value.owner == owner)
    }
    pub fn begin_lease(
        &mut self,
        owner: Owner,
        snapshot: usize,
        epoch: u64,
        now: u64,
    ) -> Result<Lease, Error> {
        if !self.has_consumer(owner) || snapshot >= SNAPSHOT_CAPACITY || epoch == 0 {
            return Err(Error::Invalid);
        }
        let held = self
            .leases
            .iter()
            .filter(|value| value.id != 0 && value.owner == owner)
            .count();
        if held >= LEASES_PER_CONSUMER {
            return Err(Error::Busy);
        }
        if self.next_lease == 0 {
            return Err(Error::Capacity);
        }
        let entry = self
            .leases
            .iter_mut()
            .find(|value| value.id == 0)
            .ok_or(Error::Capacity)?;
        *entry = Lease {
            id: self.next_lease,
            owner,
            snapshot,
            epoch,
            started_ns: now,
        };
        self.next_lease = self.next_lease.wrapping_add(1);
        self.references[snapshot] += 1;
        self.acquired = self.acquired.saturating_add(1);
        Ok(*entry)
    }
    pub fn end_lease(&mut self, owner: Owner, id: u64, epoch: u64, now: u64) -> Result<(), Error> {
        let entry = self
            .leases
            .iter_mut()
            .find(|value| {
                value.id != 0 && value.id == id && value.epoch == epoch && value.owner == owner
            })
            .ok_or(Error::Stale)?;
        self.references[entry.snapshot] -= 1;
        self.max_reader_ns = self.max_reader_ns.max(now.saturating_sub(entry.started_ns));
        *entry = Lease::default();
        Ok(())
    }
    pub fn stopped(&mut self, owner: Owner, now: u64) {
        for entry in &mut self.consumers {
            if entry.count != 0 && entry.owner == owner {
                self.count -= entry.count;
                *entry = Consumer::default();
            }
        }
        for index in 0..self.leases.len() {
            let entry = self.leases[index];
            if entry.id != 0 && entry.owner == owner {
                self.end_lease(owner, entry.id, entry.epoch, now)
                    .expect("exact capture lease");
            }
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
impl Rect {
    pub fn empty(self) -> bool {
        self.w == 0 || self.h == 0
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Change {
    pub revision: u32,
    pub rect: Rect,
}
#[repr(C)]
pub struct History {
    pub changes: [Change; HISTORY_CAPACITY],
    pub count: usize,
}
impl History {
    pub fn reset(&mut self) {
        self.changes.fill(Change::default());
        self.count = 0;
    }
    pub fn record(&mut self, revision: u32, rect: Rect) {
        if revision == 0 {
            return;
        }
        self.changes[revision as usize % HISTORY_CAPACITY] = Change { revision, rect };
        if self.count < HISTORY_CAPACITY {
            self.count += 1;
        }
    }
    pub fn union_since(
        &self,
        last_revision: u32,
        current_revision: u32,
        width: u32,
        height: u32,
    ) -> Rect {
        let full = Rect {
            w: width,
            h: height,
            ..Rect::default()
        };
        if width == 0 || height == 0 || current_revision == 0 || last_revision == current_revision {
            return Rect::default();
        }
        if last_revision == 0 {
            return full;
        }
        let distance = revision_distance(last_revision, current_revision);
        if distance == 0 || distance > HISTORY_CAPACITY || distance > self.count {
            return full;
        }
        let mut revision = last_revision;
        let mut merged = Rect::default();
        let mut remaining = distance;
        while remaining != 0 {
            revision = if revision == u32::MAX {
                1
            } else {
                revision + 1
            };
            let change = self.changes[revision as usize % HISTORY_CAPACITY];
            if change.revision != revision || change.rect.empty() {
                return full;
            }
            merged = if merged.empty() {
                change.rect
            } else {
                merge(merged, change.rect, width, height)
            };
            remaining -= 1;
        }
        if merged.empty() { full } else { merged }
    }
}
pub fn merge(a: Rect, b: Rect, width: u32, height: u32) -> Rect {
    if a.empty() {
        return b;
    }
    if b.empty() {
        return a;
    }
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (u64::from(a.x) + u64::from(a.w))
        .max(u64::from(b.x) + u64::from(b.w))
        .min(u64::from(width));
    let y1 = (u64::from(a.y) + u64::from(a.h))
        .max(u64::from(b.y) + u64::from(b.h))
        .min(u64::from(height));
    Rect {
        x: x0,
        y: y0,
        w: u32::try_from(x1 - u64::from(x0)).expect("capture rectangle width"),
        h: u32::try_from(y1 - u64::from(y0)).expect("capture rectangle height"),
    }
}
fn revision_distance(older: u32, newer: u32) -> usize {
    if older == 0 || newer == 0 || older == newer {
        return 0;
    }
    if newer > older {
        (newer - older) as usize
    } else {
        ((u32::MAX - older) + newer) as usize
    }
}
#[repr(C)]
pub struct Args {
    pub owner: Owner,
    pub snapshot: usize,
    pub id: u64,
    pub epoch: u64,
    pub now: u64,
}
/// # Safety
/// Exact initialized private registry and exclusive matching result, under its
/// original caller's capture owner. No reference survives this model call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_remote_capture(
    op: u32,
    registry: *mut CaptureRegistry,
    args: *const Args,
    out: *mut core::ffi::c_void,
) -> i32 {
    let args = unsafe { &*args };
    if op == 2 {
        unsafe {
            out.cast::<bool>()
                .write((&*registry).has_consumer(args.owner))
        };
        return 0;
    }
    let registry = unsafe { &mut *registry };
    let result = match op {
        0 => registry
            .acquire(args.owner)
            .map(|v| unsafe { out.cast::<u32>().write(v) }),
        1 => registry
            .release(args.owner)
            .map(|v| unsafe { out.cast::<u32>().write(v) }),
        2 => {
            unsafe { out.cast::<bool>().write(registry.has_consumer(args.owner)) };
            Ok(())
        }
        3 => registry
            .begin_lease(args.owner, args.snapshot, args.epoch, args.now)
            .map(|v| unsafe { out.cast::<Lease>().write(v) }),
        4 => registry.end_lease(args.owner, args.id, args.epoch, args.now),
        5 => {
            registry.stopped(args.owner, args.now);
            Ok(())
        }
        _ => panic!("capture registry operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
#[repr(C)]
pub struct HistoryArgs {
    pub revision: u32,
    pub current: u32,
    pub rect: Rect,
    pub other: Rect,
    pub width: u32,
    pub height: u32,
}
/// # Safety
/// Exact initialized history for stateful operations and matching copied C
/// argument/output. Mutation is admitted by the original frame publisher.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_remote_history(
    op: u32,
    history: *mut History,
    args: *const HistoryArgs,
    out: *mut Rect,
) {
    let args = unsafe { &*args };
    match op {
        0 => unsafe { &mut *history }.reset(),
        1 => unsafe { &mut *history }.record(args.revision, args.rect),
        2 => unsafe {
            out.write((&*history).union_since(args.revision, args.current, args.width, args.height))
        },
        3 => unsafe { out.write(merge(args.rect, args.other, args.width, args.height)) },
        _ => panic!("capture history operation"),
    }
}
const _: () = {
    assert!(
        size_of::<Owner>() == 16
            && size_of::<Lease>() == 48
            && size_of::<CaptureRegistry>() == 4648
            && size_of::<History>() == 1288
    );
};
