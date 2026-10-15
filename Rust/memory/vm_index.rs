//! VM ID/slot/address indices, with their original fixed capacities,
//! probing, tombstone rebuilding and operational counters.
use core::cell::UnsafeCell;
const RANGES: usize = 1024;
const IDS: usize = RANGES * 2;
const NONE: usize = usize::MAX;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Record {
    pub id: u32,
    pub slot_used: bool,
    pub status: u8,
    pub window: u8,
    pub base: u64,
    pub len: u64,
}
impl Record {
    fn active(self) -> bool {
        self.slot_used && self.status != 5
    }
}
fn record(slot: usize) -> Record {
    let range = super::vm_ranges::read(slot);
    Record {
        id: range.id,
        slot_used: range.slot_used,
        status: range.status,
        window: range.window,
        base: range.base,
        len: range.len,
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stats {
    pub range_index_capacity: u32,
    pub range_index_entries: u32,
    pub range_index_tombstones: u32,
    pub range_index_lookups: u64,
    pub range_index_hits: u64,
    pub range_index_misses: u64,
    pub range_index_probe_total: u64,
    pub range_index_probe_max: u32,
    pub range_index_probe_last: u32,
    pub range_index_rebuilds: u64,
    pub range_index_insert_failures: u64,
    pub range_free_slot_lookups: u64,
    pub range_free_slot_probe_total: u64,
    pub range_free_slot_probe_max: u32,
    pub range_free_slot_probe_last: u32,
    pub range_address_entries: u32,
    pub range_address_lookups: u64,
    pub range_address_probe_total: u64,
    pub range_address_probe_max: u32,
    pub range_address_probe_last: u32,
}
impl Stats {
    const EMPTY: Self = Self {
        range_index_capacity: IDS as u32,
        range_index_entries: 0,
        range_index_tombstones: 0,
        range_index_lookups: 0,
        range_index_hits: 0,
        range_index_misses: 0,
        range_index_probe_total: 0,
        range_index_probe_max: 0,
        range_index_probe_last: 0,
        range_index_rebuilds: 0,
        range_index_insert_failures: 0,
        range_free_slot_lookups: 0,
        range_free_slot_probe_total: 0,
        range_free_slot_probe_max: 0,
        range_free_slot_probe_last: 0,
        range_address_entries: 0,
        range_address_lookups: 0,
        range_address_probe_total: 0,
        range_address_probe_max: 0,
        range_address_probe_last: 0,
    };
}
#[derive(Clone, Copy)]
struct IdEntry {
    state: u8,
    id: u32,
    slot: usize,
}
impl IdEntry {
    const EMPTY: Self = Self {
        state: 0,
        id: 0,
        slot: 0,
    };
}
struct State {
    ids: [IdEntry; IDS],
    order: [u16; RANGES],
    positions: [u16; RANGES],
    id_entries: usize,
    tombstones: usize,
    addresses: usize,
    free_hint: usize,
    next_id: u32,
    stats: Stats,
}
impl State {
    const EMPTY: Self = Self {
        ids: [IdEntry::EMPTY; IDS],
        order: [0; RANGES],
        positions: [u16::MAX; RANGES],
        id_entries: 0,
        tombstones: 0,
        addresses: 0,
        free_hint: 0,
        next_id: 1,
        stats: Stats::EMPTY,
    };
    fn reset(&mut self) {
        self.ids.fill(IdEntry::EMPTY);
        self.order.fill(0);
        self.positions.fill(u16::MAX);
        self.id_entries = 0;
        self.tombstones = 0;
        self.addresses = 0;
        self.free_hint = 0;
        self.next_id = 1;
        self.stats = Stats::EMPTY;
    }
    fn free_slot(&mut self) -> usize {
        self.stats.range_free_slot_lookups = self.stats.range_free_slot_lookups.wrapping_add(1);
        for probes in 0..RANGES {
            let slot = (self.free_hint + probes) % RANGES;
            let range = record(slot);
            if !range.slot_used || range.status == 5 {
                let count = (probes + 1) as u32;
                self.stats.range_free_slot_probe_last = count;
                self.stats.range_free_slot_probe_total = self
                    .stats
                    .range_free_slot_probe_total
                    .wrapping_add(u64::from(count));
                self.stats.range_free_slot_probe_max =
                    self.stats.range_free_slot_probe_max.max(count);
                self.free_hint = (slot + 1) % RANGES;
                return slot;
            }
        }
        self.stats.range_free_slot_probe_last = RANGES as u32;
        self.stats.range_free_slot_probe_total = self
            .stats
            .range_free_slot_probe_total
            .wrapping_add(RANGES as u64);
        self.stats.range_free_slot_probe_max =
            self.stats.range_free_slot_probe_max.max(RANGES as u32);
        NONE
    }
    fn lower_bound(&self, base: u64) -> usize {
        let mut low = 0;
        let mut high = self.addresses;
        while low < high {
            let mid = low + (high - low) / 2;
            if record(usize::from(self.order[mid])).base < base {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low
    }
    fn insert_address(&mut self, slot: usize) {
        let pos = self.lower_bound(record(slot).base);
        for index in (pos + 1..=self.addresses).rev() {
            self.order[index] = self.order[index - 1];
            self.positions[usize::from(self.order[index])] = index as u16;
        }
        self.order[pos] = slot as u16;
        self.positions[slot] = pos as u16;
        self.addresses += 1;
    }
    fn remove_address(&mut self, slot: usize) {
        let pos = usize::from(self.positions[slot]);
        if pos == u16::MAX as usize || pos >= self.addresses || usize::from(self.order[pos]) != slot
        {
            return;
        }
        for index in pos..self.addresses - 1 {
            self.order[index] = self.order[index + 1];
            self.positions[usize::from(self.order[index])] = index as u16;
        }
        self.addresses -= 1;
        self.positions[slot] = u16::MAX;
    }
    fn by_address(&mut self, address: u64) -> usize {
        self.stats.range_address_lookups = self.stats.range_address_lookups.wrapping_add(1);
        let mut low = 0;
        let mut high = self.addresses;
        let mut probes = 0u32;
        while low < high {
            probes += 1;
            let mid = low + (high - low) / 2;
            if record(usize::from(self.order[mid])).base <= address {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        self.stats.range_address_probe_last = probes;
        self.stats.range_address_probe_total = self
            .stats
            .range_address_probe_total
            .wrapping_add(u64::from(probes));
        self.stats.range_address_probe_max = self.stats.range_address_probe_max.max(probes);
        if low == 0 {
            return NONE;
        }
        let slot = usize::from(self.order[low - 1]);
        let range = record(slot);
        if range
            .base
            .checked_add(range.len)
            .is_some_and(|end| address < end)
        {
            slot
        } else {
            NONE
        }
    }
    fn by_id(&mut self, id: u32, raw: bool) -> usize {
        if !raw {
            self.stats.range_index_lookups = self.stats.range_index_lookups.wrapping_add(1);
        }
        let mut position = start(id);
        let mut result = NONE;
        let mut probes = 0;
        while probes < IDS {
            probes += 1;
            let entry = self.ids[position];
            if entry.state == 0 {
                break;
            }
            if entry.state == 1 && entry.id == id {
                if raw {
                    result = entry.slot;
                } else if entry.slot < RANGES {
                    let range = record(entry.slot);
                    if range.id == id && range.active() {
                        result = entry.slot;
                    }
                }
                break;
            }
            position = (position + 1) % IDS;
        }
        if !raw {
            self.stats.range_index_probe_last = probes as u32;
            self.stats.range_index_probe_total = self
                .stats
                .range_index_probe_total
                .wrapping_add(probes as u64);
            self.stats.range_index_probe_max = self.stats.range_index_probe_max.max(probes as u32);
            if result == NONE {
                self.stats.range_index_misses = self.stats.range_index_misses.wrapping_add(1);
            } else {
                self.stats.range_index_hits = self.stats.range_index_hits.wrapping_add(1);
            }
        }
        result
    }
    fn insert_id_raw(&mut self, id: u32, slot: usize) -> bool {
        let mut first = None;
        let mut position = start(id);
        for _ in 0..IDS {
            let entry = self.ids[position];
            if entry.state == 0 {
                let target = first.unwrap_or(position);
                if self.ids[target].state == 2 && self.tombstones != 0 {
                    self.tombstones -= 1;
                }
                self.ids[target] = IdEntry { state: 1, id, slot };
                self.id_entries += 1;
                return true;
            }
            if entry.state == 2 && first.is_none() {
                first = Some(position);
            } else if entry.state == 1 && entry.id == id {
                self.ids[position].slot = slot;
                return true;
            }
            position = (position + 1) % IDS;
        }
        if let Some(target) = first {
            self.ids[target] = IdEntry { state: 1, id, slot };
            self.id_entries += 1;
            if self.tombstones != 0 {
                self.tombstones -= 1;
            }
            true
        } else {
            false
        }
    }
    fn insert_id(&mut self, id: u32, slot: usize) -> bool {
        if self.insert_id_raw(id, slot) {
            return true;
        }
        self.rebuild();
        if self.insert_id_raw(id, slot) {
            return true;
        }
        self.stats.range_index_insert_failures =
            self.stats.range_index_insert_failures.wrapping_add(1);
        false
    }
    fn remove_id(&mut self, id: u32) {
        let mut position = start(id);
        for _ in 0..IDS {
            let entry = self.ids[position];
            if entry.state == 0 {
                return;
            }
            if entry.state == 1 && entry.id == id {
                self.ids[position] = IdEntry {
                    state: 2,
                    ..IdEntry::EMPTY
                };
                if self.id_entries != 0 {
                    self.id_entries -= 1;
                }
                self.tombstones += 1;
                return;
            }
            position = (position + 1) % IDS;
        }
    }
    fn rebuild(&mut self) {
        self.stats.range_index_rebuilds = self.stats.range_index_rebuilds.wrapping_add(1);
        self.ids.fill(IdEntry::EMPTY);
        self.id_entries = 0;
        self.tombstones = 0;
        for slot in 0..RANGES {
            let range = record(slot);
            if !range.active() {
                continue;
            }
            if !self.insert_id_raw(range.id, slot) {
                self.stats.range_index_insert_failures =
                    self.stats.range_index_insert_failures.wrapping_add(1);
                return;
            }
        }
    }
}
fn start(id: u32) -> usize {
    (u64::from(id).wrapping_mul(11400714819323198485) % IDS as u64) as usize
}
struct Storage(UnsafeCell<State>);
// SAFETY: mutation/read admission is the existing VM owner. The singleton
// BSP initializes first. Fixed range readers cannot reenter an index operation.
unsafe impl Sync for Storage {}
static INDEX: Storage = Storage(UnsafeCell::new(State::EMPTY));
fn state<T>(operation: impl FnOnce(&mut State) -> T) -> T {
    operation(unsafe { &mut *INDEX.0.get() })
}
pub(super) fn neighbors(position: usize) -> (Option<usize>, Option<usize>) {
    state(|state| {
        (
            position
                .checked_sub(1)
                .map(|index| usize::from(state.order[index])),
            (position < state.addresses).then(|| usize::from(state.order[position])),
        )
    })
}
pub(super) fn position(slot: usize) -> Option<usize> {
    state(|state| (state.positions[slot] != u16::MAX).then(|| usize::from(state.positions[slot])))
}
/// # Safety
/// Singleton BSP initialization; outputs are live C pointer slots. The caller
/// reads these resident arrays only while it retains the VM owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_index_init(
    order: *mut *const u16,
    positions: *mut *const u16,
) {
    state(|state| state.reset());
    // Derive persistent read views from raw resident storage, never from an
    // exclusive reference retained by a later mutation.
    unsafe {
        order.write(core::ptr::addr_of!((*INDEX.0.get()).order).cast());
        positions.write(core::ptr::addr_of!((*INDEX.0.get()).positions).cast());
    }
}
/// # Safety
/// Caller retains VM owner and supplies a valid operation/index/identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_index_query(operation: u8, argument: u64) -> usize {
    state(|state| match operation {
        0 => state.free_slot(),
        1 => state.by_id(argument as u32, false),
        2 => state.by_id(argument as u32, true),
        3 => state.lower_bound(argument),
        4 => state.by_address(argument),
        5 => state.addresses,
        6 => state.id_entries,
        7 => state.tombstones,
        8 => {
            if state.next_id == 0 {
                return 0;
            }
            let id = state.next_id;
            // The old VM ID cursor used checked addition, not wraparound.
            state.next_id += 1;
            id as usize
        }
        _ => NONE,
    })
}
/// # Safety
/// Caller retains VM owner; slot is below 1024 and id belongs to that slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_index_change(operation: u8, id: u32, slot: usize) -> bool {
    state(|state| match operation {
        0 => state.insert_id(id, slot),
        1 => {
            state.remove_id(id);
            true
        }
        2 => {
            state.insert_address(slot);
            true
        }
        3 => {
            state.remove_address(slot);
            true
        }
        4 => {
            state.free_hint = slot;
            true
        }
        _ => false,
    })
}
/// # Safety
/// Caller retains VM owner; output is exclusive live Stats C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_vm_index_stats(output: *mut Stats) {
    let stats = state(|state| {
        let mut stats = state.stats;
        stats.range_index_entries = state.id_entries as u32;
        stats.range_index_tombstones = state.tombstones as u32;
        stats.range_address_entries = state.addresses as u32;
        stats
    });
    unsafe {
        output.write(stats);
    }
}
const _: () = {
    assert!(core::mem::size_of::<Record>() == 24);
    assert!(core::mem::size_of::<Stats>() == 128);
};
