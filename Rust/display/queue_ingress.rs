//! Per-fence IRQ mailbox and coalesced backend wakes. The existing IRQ/runtime
//! owner serializes these records, independently of BO/task locks and callbacks.
use super::{
    error::Error,
    queue_state::{Binding, CANCELLED, COMPLETE, FAILED, Fence},
};
use core::{mem::size_of, slice};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Ack {
    pub fence: Fence,
    pub result: u32,
    pub instant: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct WakeEntry {
    pub owner: u32,
    pub binding: Binding,
    pub pending: bool,
}
pub struct Wakeups<'a> {
    pub entries: &'a mut [WakeEntry],
}
impl Wakeups<'_> {
    pub fn bind(&mut self, slot: usize, owner: u32, binding: Binding) {
        self.entries[slot] = WakeEntry {
            owner,
            binding,
            pending: false,
        }
    }
    pub fn close(&mut self, slot: usize) {
        self.entries[slot] = WakeEntry::default()
    }
    pub fn request(&mut self, owner: u32, binding: Binding) -> Result<(), Error> {
        if owner == 0 || binding.adapter == 0 {
            return Err(Error::Invalid);
        }
        for entry in self.entries.iter_mut() {
            if entry.owner == 0 || entry.binding.adapter != binding.adapter {
                continue;
            }
            if entry.owner != owner {
                return Err(Error::WrongOwner);
            }
            if entry.binding != binding {
                return Err(Error::Stale);
            }
            entry.pending = true;
            return Ok(());
        }
        Err(Error::Stale)
    }
    pub fn take(&mut self, slot: usize) -> bool {
        let pending = self.entries[slot].pending;
        self.entries[slot].pending = false;
        pending
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Entry {
    pub fence: Fence,
    pub owner: u32,
    pub active: bool,
    pub pending: bool,
    pub result: u32,
    pub instant: u64,
}
pub struct Ingress<'a> {
    pub entries: &'a mut [Entry],
}
impl Ingress<'_> {
    pub fn arm(&mut self, owner: u32, fence: Fence) -> Result<(), Error> {
        if owner == 0
            || fence.slot == 0
            || fence.slot as usize > self.entries.len()
            || fence.binding.adapter == 0
        {
            return Err(Error::Invalid);
        }
        let entry = &mut self.entries[fence.slot as usize - 1];
        if entry.active || entry.pending {
            return Err(Error::Busy);
        }
        *entry = Entry {
            fence,
            owner,
            active: true,
            ..Entry::default()
        };
        Ok(())
    }
    pub fn acknowledge(
        &mut self,
        owner: u32,
        fence: Fence,
        result: u32,
        quiesced: bool,
        instant: u64,
    ) -> Result<(), Error> {
        if fence.slot == 0
            || fence.slot as usize > self.entries.len()
            || !matches!(result, COMPLETE | FAILED | CANCELLED)
        {
            return Err(Error::Invalid);
        }
        let entry = &mut self.entries[fence.slot as usize - 1];
        if entry.fence != fence {
            return Err(Error::Stale);
        }
        if owner == 0 || entry.owner != owner {
            return Err(Error::WrongOwner);
        }
        if !entry.active {
            return Err(Error::AlreadyCompleted);
        }
        if !quiesced {
            return Err(Error::Busy);
        }
        entry.active = false;
        entry.pending = true;
        entry.result = result;
        entry.instant = instant;
        Ok(())
    }
    pub fn take(&mut self, slot: usize) -> Option<Ack> {
        let entry = &mut self.entries[slot];
        if !entry.pending {
            return None;
        }
        entry.pending = false;
        Some(Ack {
            fence: entry.fence,
            result: entry.result,
            instant: entry.instant,
        })
    }
    pub fn quiesce(&mut self, fence: Fence) {
        let entry = &mut self.entries[fence.slot as usize - 1];
        if entry.fence == fence {
            entry.active = false;
            entry.pending = false
        }
    }
}
#[repr(C)]
pub struct Args {
    pub owner: u32,
    pub binding: Binding,
    pub fence: Fence,
    pub result: u32,
    pub quiesced: bool,
    pub instant: u64,
}
#[repr(C)]
pub struct OptionalAck {
    pub has: bool,
    pub value: Ack,
}
/// # Safety
/// Exact initialized resident row array of the operation's fixed C type under
/// the IRQ/runtime owner. Inputs readable; optional result output is exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_queue_ingress(
    op: u32,
    rows: *mut core::ffi::c_void,
    len: usize,
    slot: usize,
    args: *const Args,
    out: *mut core::ffi::c_void,
) -> i32 {
    let args = unsafe { &*args };
    let result = if op <= 3 {
        let mut state = Wakeups {
            entries: if len == 0 {
                &mut []
            } else {
                unsafe { slice::from_raw_parts_mut(rows.cast::<WakeEntry>(), len) }
            },
        };
        match op {
            0 => {
                state.bind(slot, args.owner, args.binding);
                Ok(())
            }
            1 => {
                state.close(slot);
                Ok(())
            }
            2 => state.request(args.owner, args.binding),
            3 => {
                unsafe { out.cast::<bool>().write(state.take(slot)) };
                Ok(())
            }
            _ => unreachable!(),
        }
    } else {
        let mut state = Ingress {
            entries: if len == 0 {
                &mut []
            } else {
                unsafe { slice::from_raw_parts_mut(rows.cast::<Entry>(), len) }
            },
        };
        match op {
            4 => state.arm(args.owner, args.fence),
            5 => state.acknowledge(
                args.owner,
                args.fence,
                args.result,
                args.quiesced,
                args.instant,
            ),
            6 => {
                let ack = state.take(slot);
                unsafe {
                    out.cast::<OptionalAck>().write(OptionalAck {
                        has: ack.is_some(),
                        value: ack.unwrap_or_default(),
                    })
                };
                Ok(())
            }
            7 => {
                state.quiesce(args.fence);
                Ok(())
            }
            _ => panic!("graphics IRQ ingress operation"),
        }
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
const _: () = {
    use core::mem::{align_of, offset_of};
    assert!(
        size_of::<Ack>() == 64
            && size_of::<WakeEntry>() == 40
            && size_of::<Entry>() == 72
            && align_of::<Entry>() == 8
    );
    assert!(
        offset_of!(WakeEntry, binding) == 8
            && offset_of!(WakeEntry, pending) == 32
            && offset_of!(Entry, result) == 56
            && offset_of!(Entry, instant) == 64
    );
    assert!(
        size_of::<Args>() == 96
            && offset_of!(Args, binding) == 8
            && offset_of!(Args, fence) == 32
            && offset_of!(Args, instant) == 88
            && size_of::<OptionalAck>() == 72
    );
};
