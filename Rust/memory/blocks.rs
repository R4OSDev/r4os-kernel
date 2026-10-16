//! Fixed physical/virtual metadata catalogue. AVL, ID and reusable-slot
//! indices retain the original policy; only this owner mutates the table.
use super::owner::{self, Guard, Token};
use crate::boot::info;
use core::cell::UnsafeCell;

pub const MAX_BLOCKS: usize = 8192;
const ID_CAPACITY: usize = MAX_BLOCKS * 2;
const SLOT_WORDS: usize = MAX_BLOCKS / 64;
const NONE: usize = usize::MAX;
const FREE_KIND: u8 = 12;
const SYSTEM: u8 = 7;
const FREE: u8 = 0;
const RESERVED: u8 = 1;
const COMMITTED: u8 = 2;
const MAPPED: u8 = 4;
const RELEASED: u8 = 5;

#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum Error {
    NotInitialized = 1,
    TableFull,
    EmptyRange,
    Overlap,
    NotFound,
    NotFree,
    InvalidBytes,
    InvalidRange,
    Overflow,
}
type Result<T> = core::result::Result<T, Error>;

// Names remain borrowed under the caller's original lifetime contract. The
// private C boundary carries pointer/length, never a Rust or Zig slice.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Block {
    pub slot_used: bool,
    pub id: u32,
    pub kind: u8,
    pub owner: u8,
    pub status: u8,
    pub owner_id: u64,
    pub name_ptr: *const u8,
    pub name_len: usize,
    pub phys_base: u64,
    pub phys_len: u64,
    pub virt_base: u64,
    pub virt_len: u64,
    pub reserved_bytes: u64,
    pub committed_bytes: u64,
}
impl Block {
    const EMPTY: Self = Self {
        slot_used: false,
        id: 0,
        kind: 13,
        owner: SYSTEM,
        status: FREE,
        owner_id: 0,
        name_ptr: b"".as_ptr(),
        name_len: 0,
        phys_base: 0,
        phys_len: 0,
        virt_base: 0,
        virt_len: 0,
        reserved_bytes: 0,
        committed_bytes: 0,
    };
    const REQUEST: Self = Self {
        status: RESERVED,
        ..Self::EMPTY
    };
    pub fn physical(
        base: u64,
        len: u64,
        kind: u8,
        owner: u8,
        owner_id: u64,
        status: u8,
        name: &[u8],
    ) -> Self {
        Self {
            phys_base: base,
            phys_len: len,
            kind,
            owner,
            owner_id,
            status,
            name_ptr: name.as_ptr(),
            name_len: name.len(),
            ..Self::REQUEST
        }
    }
    fn active(self) -> bool {
        self.slot_used && self.status != RELEASED
    }
    fn physical_index(self) -> bool {
        self.active() && self.phys_len != 0
    }
    fn name(&self) -> &[u8] {
        if self.name_len == 0 {
            b""
        } else {
            unsafe { core::slice::from_raw_parts(self.name_ptr, self.name_len) }
        }
    }
    fn canonical_free(self) -> bool {
        self.active()
            && self.kind == FREE_KIND
            && self.owner == SYSTEM
            && self.owner_id == 0
            && self.status == FREE
            && self.virt_len == 0
            && self.name() == b"free"
    }
    fn released(mut self) -> Self {
        self.status = RELEASED;
        self.reserved_bytes = 0;
        self.committed_bytes = 0;
        self
    }
    fn entry(mut self, id: u32) -> Self {
        self.slot_used = true;
        self.id = id;
        self
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Update {
    pub mask: u32,
    pub kind: u8,
    pub owner: u8,
    pub status: u8,
    pub owner_id: u64,
    pub name_ptr: *const u8,
    pub name_len: usize,
    pub reserved_bytes: u64,
    pub committed_bytes: u64,
}
#[repr(C)]
#[derive(Default)]
pub struct Snapshot {
    pub copied: usize,
    pub total: usize,
    pub truncated: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Summary {
    pub total_slots_used: u64,
    pub active_blocks: u64,
    pub released_blocks: u64,
    pub error_blocks: u64,
    pub physical_bytes: u64,
    pub virtual_bytes: u64,
    pub reserved_bytes: u64,
    pub committed_bytes: u64,
    pub free_physical_bytes: u64,
    pub largest_free_phys_base: u64,
    pub largest_free_phys_len: u64,
    pub by_kind: [u64; 14],
    pub by_owner: [u64; 8],
    pub by_status: [u64; 7],
    pub overflow: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HotStats {
    pub physical_index_entries: u32,
    pub physical_lookups: u64,
    pub physical_steps: u64,
    pub physical_step_max: u32,
    pub physical_mutations: u64,
    pub physical_rebuilds: u64,
    pub id_index_entries: u32,
    pub id_index_lookups: u64,
    pub id_index_steps: u64,
    pub id_index_step_max: u32,
    pub free_slot_lookups: u64,
    pub free_slot_word_steps: u64,
    pub free_slot_word_step_max: u32,
    pub claim_transactions: u64,
    pub claim_rollbacks: u64,
}
impl HotStats {
    const EMPTY: Self = Self {
        physical_index_entries: 0,
        physical_lookups: 0,
        physical_steps: 0,
        physical_step_max: 0,
        physical_mutations: 0,
        physical_rebuilds: 0,
        id_index_entries: 0,
        id_index_lookups: 0,
        id_index_steps: 0,
        id_index_step_max: 0,
        free_slot_lookups: 0,
        free_slot_word_steps: 0,
        free_slot_word_step_max: 0,
        claim_transactions: 0,
        claim_rollbacks: 0,
    };
}
#[repr(C)]
pub struct ReleasePlan {
    target_index: usize,
    target_released: Block,
    write_indices: [usize; 3],
    write_entries: [Block; 3],
    write_count: u8,
    merge_primary_index: usize,
    merge_primary_entry: Block,
    merge_secondary_index: usize,
    merge_secondary_released: Block,
    next_id_after: u32,
    lock_token: Token,
    active: bool,
}
impl ReleasePlan {
    fn new(index: usize, entry: Block, next_id: u32) -> Self {
        Self {
            target_index: index,
            target_released: entry.released(),
            write_indices: [0; 3],
            write_entries: [Block::EMPTY; 3],
            write_count: 0,
            merge_primary_index: NONE,
            merge_primary_entry: Block::EMPTY,
            merge_secondary_index: NONE,
            merge_secondary_released: Block::EMPTY,
            next_id_after: next_id,
            lock_token: Token::default(),
            active: false,
        }
    }
}
#[derive(Clone, Copy)]
struct Node {
    in_tree: bool,
    left: Option<u16>,
    right: Option<u16>,
    height: u8,
}
impl Node {
    const EMPTY: Self = Self {
        in_tree: false,
        left: None,
        right: None,
        height: 1,
    };
}
#[derive(Clone, Copy)]
struct IdEntry {
    state: u8,
    id: u32,
    slot: u16,
}
impl IdEntry {
    const EMPTY: Self = Self {
        state: 0,
        id: 0,
        slot: 0,
    };
}
struct Table {
    entries: [Block; MAX_BLOCKS],
    next_id: u32,
    nodes: [Node; MAX_BLOCKS],
    root: Option<u16>,
    physical_count: u32,
    ids: [IdEntry; ID_CAPACITY],
    id_count: u32,
    id_tombstones: u32,
    slots: [u64; SLOT_WORDS],
    next_slot_word: usize,
    hot: HotStats,
    initialized: bool,
}
impl Table {
    const EMPTY: Self = Self {
        entries: [Block::EMPTY; MAX_BLOCKS],
        next_id: 1,
        nodes: [Node::EMPTY; MAX_BLOCKS],
        root: None,
        physical_count: 0,
        ids: [IdEntry::EMPTY; ID_CAPACITY],
        id_count: 0,
        id_tombstones: 0,
        slots: [u64::MAX; SLOT_WORDS],
        next_slot_word: 0,
        hot: HotStats::EMPTY,
        initialized: false,
    };
    fn reset(&mut self) {
        // Reset resident arrays in place, never copy the whole table to a
        // limited kernel stack.
        self.entries.fill(Block::EMPTY);
        self.nodes.fill(Node::EMPTY);
        self.ids.fill(IdEntry::EMPTY);
        self.slots.fill(u64::MAX);
        self.next_id = 1;
        self.root = None;
        self.physical_count = 0;
        self.id_count = 0;
        self.id_tombstones = 0;
        self.next_slot_word = 0;
        self.hot = HotStats::EMPTY;
    }
    fn register(&mut self, request: Block) -> Result<u32> {
        validate(request)?;
        if self.overlaps(request.phys_base, request.phys_len, true, 0)
            || self.overlaps(request.virt_base, request.virt_len, false, 0)
        {
            return Err(Error::Overlap);
        }
        let index = self.free_slot().ok_or(Error::TableFull)?;
        let id = take_id(&mut self.next_id).ok_or(Error::Overflow)?;
        self.replace(index, request.entry(id));
        Ok(id)
    }
    fn claim(&mut self, mut request: Block) -> Result<u32> {
        let base = request.phys_base;
        let len = request.phys_len;
        if len == 0 || base.checked_add(len).is_none() {
            return Err(Error::EmptyRange);
        }
        self.hot.claim_transactions = self.hot.claim_transactions.wrapping_add(1);
        let index = self.containing(base, len, true).ok_or(Error::NotFree)?;
        let free = self.entries[index];
        let end = free
            .phys_base
            .checked_add(free.phys_len)
            .ok_or(Error::Overflow)?;
        let claim_end = base.checked_add(len).ok_or(Error::Overflow)?;
        let before = base - free.phys_base;
        let after = end - claim_end;
        let mut journal = Journal::new(self.next_id);
        journal.remember(self, index)?;
        let result = (|| {
            self.replace(index, free.released());
            if before != 0 {
                let slot = self.free_slot().ok_or(Error::TableFull)?;
                journal.remember(self, slot)?;
                self.register(free_request(free.phys_base, before))?;
            }
            request.status = COMMITTED;
            request.reserved_bytes = len;
            request.committed_bytes = len;
            let slot = match self.merge_slot(request)? {
                Some(slot) => slot,
                None => self.free_slot().ok_or(Error::TableFull)?,
            };
            journal.remember(self, slot)?;
            let id = self.add_or_merge(request)?;
            if after != 0 {
                let slot = self.free_slot().ok_or(Error::TableFull)?;
                journal.remember(self, slot)?;
                self.register(free_request(claim_end, after))?;
            }
            Ok(id)
        })();
        if result.is_err() {
            self.hot.claim_rollbacks = self.hot.claim_rollbacks.wrapping_add(1);
            journal.rollback(self);
        }
        result
    }
    fn prepare_release(&mut self, base: u64, len: u64) -> Result<ReleasePlan> {
        match self.build_release(base, len) {
            Err(Error::TableFull) => {
                self.coalesce();
                self.build_release(base, len)
            }
            result => result,
        }
    }
    fn build_release(&mut self, base: u64, len: u64) -> Result<ReleasePlan> {
        if len == 0 {
            return Err(Error::EmptyRange);
        }
        let end = base.checked_add(len).ok_or(Error::Overflow)?;
        let index = self.containing(base, len, false).ok_or(Error::NotFound)?;
        let block = self.entries[index];
        if block.virt_len != 0 || block.kind == FREE_KIND || block.status == FREE {
            return Err(Error::InvalidRange);
        }
        let block_end = block
            .phys_base
            .checked_add(block.phys_len)
            .ok_or(Error::Overflow)?;
        let before = base - block.phys_base;
        let after = block_end - end;
        let mut plan = ReleasePlan::new(index, block, self.next_id);
        self.plan_free_merge(base, end, block.id, &mut plan)?;
        let mut requests = [Block::REQUEST; 3];
        let mut count = 0;
        if before != 0 {
            requests[count] = part_request(block, block.phys_base, before);
            count += 1;
        }
        if after != 0 {
            requests[count] = part_request(block, end, after);
            count += 1;
        }
        if plan.merge_primary_index == NONE {
            requests[count] = free_request(base, len);
            count += 1;
        }
        let mut slots = [0; 3];
        let mut slot_count = 0;
        if count != 0 {
            slots[0] = index;
            slot_count = 1;
        }
        self.collect_slots(&mut slots, &mut slot_count, count, index);
        if slot_count != count {
            return Err(Error::TableFull);
        }
        let mut next_id = self.next_id;
        for output in 0..count {
            let request = requests[output];
            validate(request)?;
            if self.overlaps(request.phys_base, request.phys_len, true, block.id) {
                return Err(Error::Overlap);
            }
            let id = take_id(&mut next_id).ok_or(Error::Overflow)?;
            plan.write_indices[output] = slots[output];
            plan.write_entries[output] = request.entry(id);
        }
        plan.write_count = count as u8;
        plan.next_id_after = next_id;
        Ok(plan)
    }
    fn plan_free_merge(
        &mut self,
        base: u64,
        end: u64,
        ignore: u32,
        plan: &mut ReleasePlan,
    ) -> Result<()> {
        let mut left = self.predecessor_before(base, ignore);
        if let Some(index) = left {
            let entry = self.entries[index];
            let entry_end = entry
                .phys_base
                .checked_add(entry.phys_len)
                .ok_or(Error::Overflow)?;
            if entry_end != base || !entry.canonical_free() {
                left = None;
            }
        }
        let mut right = self.lower_bound(end);
        if let Some(index) = right
            && self.entries[index].id == ignore
        {
            right = self.successor(index);
        }
        if let Some(index) = right
            && (self.entries[index].phys_base != end || !self.entries[index].canonical_free())
        {
            right = None;
        }
        let Some(primary_index) = left.or(right) else {
            return Ok(());
        };
        let mut primary = self.entries[primary_index];
        let merged_base = left.map_or(base, |index| self.entries[index].phys_base);
        let merged_end = match right {
            Some(index) => self.entries[index]
                .phys_base
                .checked_add(self.entries[index].phys_len)
                .ok_or(Error::Overflow)?,
            None => end,
        };
        primary.phys_base = merged_base;
        primary.phys_len = merged_end.checked_sub(merged_base).ok_or(Error::Overflow)?;
        primary.reserved_bytes = 0;
        primary.committed_bytes = 0;
        plan.merge_primary_index = primary_index;
        plan.merge_primary_entry = primary;
        if let (Some(left), Some(right)) = (left, right)
            && left != right
        {
            plan.merge_secondary_index = right;
            plan.merge_secondary_released = self.entries[right].released();
        }
        Ok(())
    }
    fn finish_release(&mut self, plan: &ReleasePlan, coalesce: bool) {
        self.replace(plan.target_index, plan.target_released);
        if plan.merge_primary_index != NONE {
            self.replace(plan.merge_primary_index, plan.merge_primary_entry);
        }
        if plan.merge_secondary_index != NONE {
            self.replace(plan.merge_secondary_index, plan.merge_secondary_released);
        }
        for index in 0..plan.write_count as usize {
            self.replace(plan.write_indices[index], plan.write_entries[index]);
        }
        self.next_id = plan.next_id_after;
        if coalesce {
            self.coalesce();
        }
    }
    fn retag(&mut self, mut request: Block) -> Result<u32> {
        let base = request.phys_base;
        let len = request.phys_len;
        if len == 0 || base.checked_add(len).is_none() {
            return Err(Error::EmptyRange);
        }
        let index = self.containing(base, len, false).ok_or(Error::NotFound)?;
        let block = self.entries[index];
        let block_end = block
            .phys_base
            .checked_add(block.phys_len)
            .ok_or(Error::Overflow)?;
        let end = base.checked_add(len).ok_or(Error::Overflow)?;
        let before = base - block.phys_base;
        let after = block_end - end;
        let mut journal = Journal::new(self.next_id);
        journal.remember(self, index)?;
        let result = (|| {
            self.replace(index, block.released());
            if before != 0 {
                let slot = self.free_slot().ok_or(Error::TableFull)?;
                journal.remember(self, slot)?;
                self.register(part_request(block, block.phys_base, before))?;
            }
            request.reserved_bytes = if request.status == FREE || request.status == RELEASED {
                0
            } else {
                len
            };
            request.committed_bytes = if request.status == COMMITTED || request.status == MAPPED {
                len
            } else {
                0
            };
            let slot = match self.merge_slot(request)? {
                Some(slot) => slot,
                None => self.free_slot().ok_or(Error::TableFull)?,
            };
            journal.remember(self, slot)?;
            let id = self.add_or_merge(request)?;
            if after != 0 {
                let slot = self.free_slot().ok_or(Error::TableFull)?;
                journal.remember(self, slot)?;
                self.register(part_request(block, end, after))?;
            }
            self.coalesce();
            Ok(id)
        })();
        if result.is_err() {
            journal.rollback(self);
        }
        result
    }
    fn add_or_merge(&mut self, request: Block) -> Result<u32> {
        if let Some(index) = self.merge_slot(request)? {
            let mut block = self.entries[index];
            let block_end = block
                .phys_base
                .checked_add(block.phys_len)
                .ok_or(Error::Overflow)?;
            let end = request
                .phys_base
                .checked_add(request.phys_len)
                .ok_or(Error::Overflow)?;
            let len = block
                .phys_len
                .checked_add(request.phys_len)
                .ok_or(Error::Overflow)?;
            let reserved = block
                .reserved_bytes
                .checked_add(request.reserved_bytes)
                .ok_or(Error::Overflow)?;
            let committed = block
                .committed_bytes
                .checked_add(request.committed_bytes)
                .ok_or(Error::Overflow)?;
            if block_end == request.phys_base {
                block.phys_len = len;
            } else if end == block.phys_base {
                block.phys_base = request.phys_base;
                block.phys_len = len;
            } else {
                panic!("nonadjacent physical merge");
            }
            block.reserved_bytes = reserved;
            block.committed_bytes = committed;
            self.replace(index, block);
            Ok(block.id)
        } else {
            self.register(request)
        }
    }
    fn merge_slot(&mut self, request: Block) -> Result<Option<usize>> {
        let end = request
            .phys_base
            .checked_add(request.phys_len)
            .ok_or(Error::Overflow)?;
        if let Some(left) = self.predecessor_before(request.phys_base, 0) {
            let block = self.entries[left];
            let block_end = block
                .phys_base
                .checked_add(block.phys_len)
                .ok_or(Error::Overflow)?;
            if block_end == request.phys_base && merge_compatible(block, request) {
                return Ok(Some(left));
            }
        }
        if let Some(right) = self.lower_bound(end) {
            let block = self.entries[right];
            if block.phys_base == end && merge_compatible(block, request) {
                return Ok(Some(right));
            }
        }
        Ok(None)
    }
    fn update(&mut self, id: u32, update: Update) -> Result<()> {
        let index = self.by_id(id).ok_or(Error::NotFound)?;
        let mut block = self.entries[index];
        if !block.active() {
            return Err(Error::NotFound);
        }
        if update.mask & 1 != 0 {
            block.kind = update.kind;
        }
        if update.mask & 2 != 0 {
            block.owner = update.owner;
        }
        if update.mask & 4 != 0 {
            block.owner_id = update.owner_id;
        }
        if update.mask & 8 != 0 {
            block.status = update.status;
        }
        if update.mask & 16 != 0 {
            block.name_ptr = update.name_ptr;
            block.name_len = update.name_len;
        }
        if update.mask & 32 != 0 {
            block.reserved_bytes = update.reserved_bytes;
        }
        if update.mask & 64 != 0 {
            block.committed_bytes = update.committed_bytes;
        }
        validate_bytes(block.reserved_bytes, block.committed_bytes)?;
        self.replace(index, block);
        Ok(())
    }
    fn set_committed(&mut self, id: u32, bytes: u64) -> Result<()> {
        self.update(
            id,
            Update {
                mask: 8 | 64,
                status: if bytes == 0 { RESERVED } else { COMMITTED },
                committed_bytes: bytes,
                kind: 0,
                owner: 0,
                owner_id: 0,
                name_ptr: b"".as_ptr(),
                name_len: 0,
                reserved_bytes: 0,
            },
        )
    }
    fn change_committed(&mut self, id: u32, bytes: u64, subtract: bool) -> Result<()> {
        let index = self.by_id(id).ok_or(Error::NotFound)?;
        let current = self.entries[index].committed_bytes;
        let next = if subtract {
            current.checked_sub(bytes).ok_or(Error::InvalidBytes)?
        } else {
            current.checked_add(bytes).ok_or(Error::Overflow)?
        };
        self.set_committed(id, next)
    }
    fn release(&mut self, id: u32) -> Result<()> {
        let index = self.by_id(id).ok_or(Error::NotFound)?;
        self.replace(index, self.entries[index].released());
        Ok(())
    }
    fn containing(&mut self, base: u64, len: u64, free: bool) -> Option<usize> {
        let end = base.checked_add(len)?;
        let index = self.floor(base)?;
        let block = self.entries[index];
        if free {
            if !block.canonical_free() {
                return None;
            }
        } else if !block.active()
            || block.kind == FREE_KIND
            || block.status == FREE
            || block.phys_len == 0
        {
            return None;
        }
        let block_end = block.phys_base.checked_add(block.phys_len)?;
        if end <= block_end { Some(index) } else { None }
    }
    fn prefix(&mut self, base: u64, max_len: u64, free: bool) -> u64 {
        if max_len == 0 {
            return 0;
        }
        let Some(index) = self.floor(base) else {
            return 0;
        };
        let block = self.entries[index];
        if free {
            if !block.canonical_free() || base < block.phys_base {
                return 0;
            }
        } else if !block.active()
            || block.kind == FREE_KIND
            || block.status == FREE
            || block.phys_len == 0
            || base < block.phys_base
        {
            return 0;
        }
        let Some(end) = block.phys_base.checked_add(block.phys_len) else {
            return 0;
        };
        if base >= end {
            0
        } else {
            max_len.min(end - base)
        }
    }
    fn summary(&self) -> Summary {
        let mut result = Summary::default();
        for block in &self.entries {
            if !block.slot_used {
                continue;
            }
            result.total_slots_used += 1;
            if block.status == RELEASED {
                result.released_blocks += 1;
                continue;
            }
            result.active_blocks += 1;
            if block.status == 6 {
                result.error_blocks += 1;
            }
            add_summary(
                &mut result.physical_bytes,
                block.phys_len,
                &mut result.overflow,
            );
            add_summary(
                &mut result.virtual_bytes,
                block.virt_len,
                &mut result.overflow,
            );
            add_summary(
                &mut result.reserved_bytes,
                block.reserved_bytes,
                &mut result.overflow,
            );
            add_summary(
                &mut result.committed_bytes,
                block.committed_bytes,
                &mut result.overflow,
            );
            add_summary(
                &mut result.by_kind[block.kind as usize],
                1,
                &mut result.overflow,
            );
            add_summary(
                &mut result.by_owner[block.owner as usize],
                1,
                &mut result.overflow,
            );
            add_summary(
                &mut result.by_status[block.status as usize],
                1,
                &mut result.overflow,
            );
            if block.kind == FREE_KIND && block.status == FREE && block.phys_len != 0 {
                add_summary(
                    &mut result.free_physical_bytes,
                    block.phys_len,
                    &mut result.overflow,
                );
                if block.phys_len > result.largest_free_phys_len {
                    result.largest_free_phys_base = block.phys_base;
                    result.largest_free_phys_len = block.phys_len;
                }
            }
        }
        result
    }
}

struct Journal {
    indices: [usize; 4],
    entries: [Block; 4],
    count: usize,
    next_id: u32,
}
impl Journal {
    fn new(next_id: u32) -> Self {
        Self {
            indices: [0; 4],
            entries: [Block::EMPTY; 4],
            count: 0,
            next_id,
        }
    }
    fn remember(&mut self, table: &Table, index: usize) -> Result<()> {
        if self.indices[..self.count].contains(&index) {
            return Ok(());
        }
        if self.count == self.indices.len() {
            return Err(Error::TableFull);
        }
        self.indices[self.count] = index;
        self.entries[self.count] = table.entries[index];
        self.count += 1;
        Ok(())
    }
    fn rollback(&self, table: &mut Table) {
        for index in (0..self.count).rev() {
            table.entries[self.indices[index]] = self.entries[index];
        }
        table.next_id = self.next_id;
        table.rebuild();
    }
}
fn validate(block: Block) -> Result<()> {
    if (block.phys_len == 0 && block.virt_len == 0)
        || block.phys_base.checked_add(block.phys_len).is_none()
        || block.virt_base.checked_add(block.virt_len).is_none()
    {
        return Err(Error::EmptyRange);
    }
    validate_bytes(block.reserved_bytes, block.committed_bytes)
}
fn validate_bytes(reserved: u64, committed: u64) -> Result<()> {
    if reserved != 0 && committed > reserved {
        Err(Error::InvalidBytes)
    } else {
        Ok(())
    }
}
fn take_id(next: &mut u32) -> Option<u32> {
    if *next == 0 {
        None
    } else {
        let id = *next;
        *next = next.wrapping_add(1);
        Some(id)
    }
}
fn free_request(base: u64, len: u64) -> Block {
    Block::physical(base, len, FREE_KIND, SYSTEM, 0, FREE, b"free")
}
fn part_request(block: Block, base: u64, len: u64) -> Block {
    Block {
        phys_base: base,
        phys_len: len,
        virt_base: 0,
        virt_len: 0,
        reserved_bytes: block.reserved_bytes.min(len),
        committed_bytes: block.committed_bytes.min(len),
        slot_used: false,
        id: 0,
        ..block
    }
}
fn merge_compatible(block: Block, request: Block) -> bool {
    block.active()
        && block.virt_len == 0
        && block.phys_len != 0
        && block.kind == request.kind
        && block.owner == request.owner
        && block.owner_id == request.owner_id
        && block.status == request.status
        && block.name() == request.name()
}
fn add_summary(target: &mut u64, value: u64, overflow: &mut bool) {
    match target.checked_add(value) {
        Some(next) => *target = next,
        None => {
            *target = u64::MAX;
            *overflow = true;
        }
    }
}

fn id_start(id: u32) -> usize {
    (u64::from(id).wrapping_mul(11400714819323198485) % ID_CAPACITY as u64) as usize
}
fn record(steps: usize, total: &mut u64, max: &mut u32) {
    let value = steps.min(u32::MAX as usize) as u32;
    *total = total.wrapping_add(u64::from(value));
    *max = (*max).max(value);
}
impl Table {
    fn free_slot(&mut self) -> Option<usize> {
        self.hot.free_slot_lookups = self.hot.free_slot_lookups.wrapping_add(1);
        for scanned in 0..SLOT_WORDS {
            let index = (self.next_slot_word + scanned) % SLOT_WORDS;
            let word = self.slots[index];
            if word == 0 {
                continue;
            }
            let slot = index * 64 + word.trailing_zeros() as usize;
            self.next_slot_word = index;
            record(
                scanned + 1,
                &mut self.hot.free_slot_word_steps,
                &mut self.hot.free_slot_word_step_max,
            );
            return Some(slot);
        }
        record(
            SLOT_WORDS,
            &mut self.hot.free_slot_word_steps,
            &mut self.hot.free_slot_word_step_max,
        );
        None
    }
    fn collect_slots(
        &mut self,
        slots: &mut [usize; 3],
        count: &mut usize,
        wanted: usize,
        excluded: usize,
    ) {
        let mut scanned = 0;
        while *count < wanted && scanned < SLOT_WORDS {
            let mut word = self.slots[scanned];
            scanned += 1;
            while word != 0 && *count < wanted {
                let bit = word.trailing_zeros();
                let slot = (scanned - 1) * 64 + bit as usize;
                word &= !(1u64 << bit);
                if slot == excluded {
                    continue;
                }
                slots[*count] = slot;
                *count += 1;
            }
        }
        record(
            scanned,
            &mut self.hot.free_slot_word_steps,
            &mut self.hot.free_slot_word_step_max,
        );
    }
    fn by_id(&mut self, id: u32) -> Option<usize> {
        self.hot.id_index_lookups = self.hot.id_index_lookups.wrapping_add(1);
        let mut position = id_start(id);
        for step in 0..ID_CAPACITY {
            let entry = self.ids[position];
            if entry.state == 0 || (entry.state == 1 && entry.id == id) {
                record(
                    step + 1,
                    &mut self.hot.id_index_steps,
                    &mut self.hot.id_index_step_max,
                );
                if entry.state == 0 {
                    return None;
                }
                let index = usize::from(entry.slot);
                return if index < MAX_BLOCKS
                    && self.entries[index].slot_used
                    && self.entries[index].id == id
                {
                    Some(index)
                } else {
                    None
                };
            }
            position = (position + 1) % ID_CAPACITY;
        }
        record(
            ID_CAPACITY,
            &mut self.hot.id_index_steps,
            &mut self.hot.id_index_step_max,
        );
        None
    }
    fn raw_id(&self, id: u32) -> Option<usize> {
        let mut position = id_start(id);
        for _ in 0..ID_CAPACITY {
            let entry = self.ids[position];
            if entry.state == 0 {
                return None;
            }
            if entry.state == 1 && entry.id == id {
                return Some(usize::from(entry.slot));
            }
            position = (position + 1) % ID_CAPACITY;
        }
        None
    }
    fn insert_id(&mut self, id: u32, slot: u16) {
        let mut position = id_start(id);
        for _ in 0..ID_CAPACITY {
            if self.ids[position].state == 0 {
                self.ids[position] = IdEntry { state: 1, id, slot };
                self.id_count = self.id_count.saturating_add(1);
                return;
            }
            if self.ids[position].state == 1 && self.ids[position].id == id {
                self.ids[position].slot = slot;
                return;
            }
            position = (position + 1) % ID_CAPACITY;
        }
        panic!("full memory-block ID index");
    }
    fn remove_id(&mut self, id: u32) {
        let mut position = id_start(id);
        for _ in 0..ID_CAPACITY {
            let entry = self.ids[position];
            if entry.state == 0 {
                return;
            }
            if entry.state == 1 && entry.id == id {
                self.ids[position] = IdEntry::EMPTY;
                if self.id_count != 0 {
                    self.id_count -= 1;
                }
                let mut rehash = (position + 1) % ID_CAPACITY;
                while self.ids[rehash].state == 1 {
                    let displaced = self.ids[rehash];
                    self.ids[rehash] = IdEntry::EMPTY;
                    if self.id_count != 0 {
                        self.id_count -= 1;
                    }
                    self.insert_id(displaced.id, displaced.slot);
                    rehash = (rehash + 1) % ID_CAPACITY;
                }
                return;
            }
            position = (position + 1) % ID_CAPACITY;
        }
    }
    fn reusable(&mut self, index: usize, reusable: bool) {
        let word = index / 64;
        let mask = 1u64 << (index % 64);
        if reusable {
            self.slots[word] |= mask;
            if word < self.next_slot_word {
                self.next_slot_word = word;
            }
        } else {
            self.slots[word] &= !mask;
        }
    }
    fn replace(&mut self, index: usize, entry: Block) {
        if self.nodes[index].in_tree {
            self.remove_physical(index as u16);
        }
        if self.entries[index].slot_used {
            self.remove_id(self.entries[index].id);
        }
        self.entries[index] = entry;
        self.reusable(index, !entry.slot_used || entry.status == RELEASED);
        if entry.slot_used {
            self.insert_id(entry.id, index as u16);
        }
        if entry.physical_index() {
            self.insert_physical(index as u16);
        }
        self.hot.physical_mutations = self.hot.physical_mutations.wrapping_add(1);
    }
    fn rebuild(&mut self) {
        self.hot.physical_rebuilds = self.hot.physical_rebuilds.wrapping_add(1);
        self.nodes.fill(Node::EMPTY);
        self.ids.fill(IdEntry::EMPTY);
        self.slots.fill(0);
        self.root = None;
        self.physical_count = 0;
        self.id_count = 0;
        self.id_tombstones = 0;
        self.next_slot_word = 0;
        for index in 0..MAX_BLOCKS {
            let entry = self.entries[index];
            self.reusable(index, !entry.slot_used || entry.status == RELEASED);
            if entry.slot_used {
                self.insert_id(entry.id, index as u16);
            }
            if entry.physical_index() {
                self.insert_physical(index as u16);
            }
        }
    }
    fn less(&self, left: u16, right: u16) -> bool {
        let left_base = self.entries[usize::from(left)].phys_base;
        let right_base = self.entries[usize::from(right)].phys_base;
        left_base < right_base || (left_base == right_base && left < right)
    }
    fn height(&self, slot: Option<u16>) -> u8 {
        slot.map_or(0, |index| self.nodes[usize::from(index)].height)
    }
    fn update_height(&mut self, slot: u16) {
        let node = self.nodes[usize::from(slot)];
        self.nodes[usize::from(slot)].height =
            1 + self.height(node.left).max(self.height(node.right));
    }
    fn rotate_left(&mut self, slot: u16) -> u16 {
        let right = self.nodes[usize::from(slot)]
            .right
            .expect("AVL right child");
        let transfer = self.nodes[usize::from(right)].left;
        self.nodes[usize::from(right)].left = Some(slot);
        self.nodes[usize::from(slot)].right = transfer;
        self.update_height(slot);
        self.update_height(right);
        right
    }
    fn rotate_right(&mut self, slot: u16) -> u16 {
        let left = self.nodes[usize::from(slot)].left.expect("AVL left child");
        let transfer = self.nodes[usize::from(left)].right;
        self.nodes[usize::from(left)].right = Some(slot);
        self.nodes[usize::from(slot)].left = transfer;
        self.update_height(slot);
        self.update_height(left);
        left
    }
    fn rebalance(&mut self, slot: u16) -> u16 {
        self.update_height(slot);
        let node = self.nodes[usize::from(slot)];
        let balance = i16::from(self.height(node.left)) - i16::from(self.height(node.right));
        if balance > 1 {
            let left = node.left.expect("AVL left child");
            let child = self.nodes[usize::from(left)];
            if self.height(child.left) < self.height(child.right) {
                self.nodes[usize::from(slot)].left = Some(self.rotate_left(left));
            }
            return self.rotate_right(slot);
        }
        if balance < -1 {
            let right = node.right.expect("AVL right child");
            let child = self.nodes[usize::from(right)];
            if self.height(child.right) < self.height(child.left) {
                self.nodes[usize::from(slot)].right = Some(self.rotate_right(right));
            }
            return self.rotate_left(slot);
        }
        slot
    }
    fn insert_physical(&mut self, slot: u16) {
        self.nodes[usize::from(slot)] = Node {
            in_tree: true,
            ..Node::EMPTY
        };
        self.root = Some(self.insert_at(self.root, slot));
        self.physical_count = self.physical_count.saturating_add(1);
    }
    fn insert_at(&mut self, root: Option<u16>, slot: u16) -> u16 {
        let Some(root) = root else {
            return slot;
        };
        if self.less(slot, root) {
            self.nodes[usize::from(root)].left =
                Some(self.insert_at(self.nodes[usize::from(root)].left, slot));
        } else {
            self.nodes[usize::from(root)].right =
                Some(self.insert_at(self.nodes[usize::from(root)].right, slot));
        }
        self.rebalance(root)
    }
    fn remove_physical(&mut self, slot: u16) {
        if !self.nodes[usize::from(slot)].in_tree {
            return;
        }
        self.root = self.remove_at(self.root, slot);
        self.nodes[usize::from(slot)] = Node::EMPTY;
        if self.physical_count != 0 {
            self.physical_count -= 1;
        }
    }
    fn remove_at(&mut self, root: Option<u16>, slot: u16) -> Option<u16> {
        let root = root?;
        if self.less(slot, root) {
            self.nodes[usize::from(root)].left =
                self.remove_at(self.nodes[usize::from(root)].left, slot);
            return Some(self.rebalance(root));
        }
        if self.less(root, slot) {
            self.nodes[usize::from(root)].right =
                self.remove_at(self.nodes[usize::from(root)].right, slot);
            return Some(self.rebalance(root));
        }
        let node = self.nodes[usize::from(root)];
        let Some(left) = node.left else {
            return node.right;
        };
        let Some(right) = node.right else {
            return node.left;
        };
        let (remainder, minimum) = self.detach_minimum(right);
        self.nodes[usize::from(minimum)].left = Some(left);
        self.nodes[usize::from(minimum)].right = remainder;
        Some(self.rebalance(minimum))
    }
    fn detach_minimum(&mut self, root: u16) -> (Option<u16>, u16) {
        let Some(left) = self.nodes[usize::from(root)].left else {
            return (self.nodes[usize::from(root)].right, root);
        };
        let (remainder, minimum) = self.detach_minimum(left);
        self.nodes[usize::from(root)].left = remainder;
        (Some(self.rebalance(root)), minimum)
    }
    fn floor(&mut self, base: u64) -> Option<usize> {
        self.hot.physical_lookups = self.hot.physical_lookups.wrapping_add(1);
        let mut current = self.root;
        let mut result = None;
        let mut steps = 0;
        while let Some(slot) = current {
            let index = usize::from(slot);
            steps += 1;
            if self.entries[index].phys_base <= base {
                result = Some(index);
                current = self.nodes[index].right;
            } else {
                current = self.nodes[index].left;
            }
        }
        record(
            steps,
            &mut self.hot.physical_steps,
            &mut self.hot.physical_step_max,
        );
        result
    }
    fn lower_bound(&mut self, base: u64) -> Option<usize> {
        self.hot.physical_lookups = self.hot.physical_lookups.wrapping_add(1);
        let mut current = self.root;
        let mut result = None;
        let mut steps = 0;
        while let Some(slot) = current {
            let index = usize::from(slot);
            steps += 1;
            if self.entries[index].phys_base >= base {
                result = Some(index);
                current = self.nodes[index].left;
            } else {
                current = self.nodes[index].right;
            }
        }
        record(
            steps,
            &mut self.hot.physical_steps,
            &mut self.hot.physical_step_max,
        );
        result
    }
    fn predecessor_before(&mut self, base: u64, ignore: u32) -> Option<usize> {
        self.hot.physical_lookups = self.hot.physical_lookups.wrapping_add(1);
        let mut current = self.root;
        let mut result = None;
        let mut steps = 0;
        while let Some(slot) = current {
            let index = usize::from(slot);
            steps += 1;
            if self.entries[index].phys_base < base {
                result = Some(index);
                current = self.nodes[index].right;
            } else {
                current = self.nodes[index].left;
            }
        }
        if let Some(index) = result
            && ignore != 0
            && self.entries[index].id == ignore
        {
            result = self.predecessor(index as u16).map(usize::from);
        }
        record(
            steps,
            &mut self.hot.physical_steps,
            &mut self.hot.physical_step_max,
        );
        result
    }
    fn predecessor(&self, target: u16) -> Option<u16> {
        let mut current = self.root;
        let mut result = None;
        while let Some(slot) = current {
            if self.less(slot, target) {
                result = Some(slot);
                current = self.nodes[usize::from(slot)].right;
            } else {
                current = self.nodes[usize::from(slot)].left;
            }
        }
        result
    }
    fn successor(&mut self, target: usize) -> Option<usize> {
        self.hot.physical_lookups = self.hot.physical_lookups.wrapping_add(1);
        let mut current = self.root;
        let mut result = None;
        let mut steps = 0;
        while let Some(slot) = current {
            let index = usize::from(slot);
            steps += 1;
            if self.less(target as u16, slot) {
                result = Some(index);
                current = self.nodes[index].left;
            } else {
                current = self.nodes[index].right;
            }
        }
        record(
            steps,
            &mut self.hot.physical_steps,
            &mut self.hot.physical_step_max,
        );
        result
    }
    fn minimum(&self, root: Option<u16>) -> Option<u16> {
        let mut current = root?;
        while let Some(left) = self.nodes[usize::from(current)].left {
            current = left;
        }
        Some(current)
    }
    fn overlaps(&mut self, base: u64, len: u64, physical: bool, ignore: u32) -> bool {
        if len == 0 {
            return false;
        }
        let Some(end) = base.checked_add(len) else {
            return true;
        };
        if physical {
            if let Some(index) = self.floor(base) {
                let block = self.entries[index];
                if block.id != ignore {
                    let Some(block_end) = block.phys_base.checked_add(block.phys_len) else {
                        return true;
                    };
                    if base < block_end && block.phys_base < end {
                        return true;
                    }
                }
            }
            let mut next = self.lower_bound(base);
            if let Some(index) = next
                && self.entries[index].id == ignore
            {
                next = self.successor(index);
            }
            if let Some(index) = next {
                let block = self.entries[index];
                let Some(block_end) = block.phys_base.checked_add(block.phys_len) else {
                    return true;
                };
                if base < block_end && block.phys_base < end {
                    return true;
                }
            }
            false
        } else {
            for block in &self.entries {
                if !block.active() || block.id == ignore || block.virt_len == 0 {
                    continue;
                }
                let Some(other_end) = block.virt_base.checked_add(block.virt_len) else {
                    return true;
                };
                if base < other_end && block.virt_base < end {
                    return true;
                }
            }
            false
        }
    }
    fn coalesce(&mut self) {
        let mut current = self.minimum(self.root).map(usize::from);
        while let Some(left_index) = current {
            let Some(right_index) = self.successor(left_index) else {
                break;
            };
            let left = self.entries[left_index];
            let right = self.entries[right_index];
            let merged = (|| {
                let end = left.phys_base.checked_add(left.phys_len)?;
                if end != right.phys_base
                    || !right.active()
                    || !merge_compatible(left, right)
                    || right.virt_len != 0
                    || right.phys_len == 0
                {
                    return None;
                }
                Some(Block {
                    phys_len: left.phys_len.checked_add(right.phys_len)?,
                    reserved_bytes: left.reserved_bytes.checked_add(right.reserved_bytes)?,
                    committed_bytes: left.committed_bytes.checked_add(right.committed_bytes)?,
                    ..left
                })
            })();
            if let Some(block) = merged {
                self.replace(left_index, block);
                self.replace(right_index, right.released());
            } else {
                current = Some(right_index);
            }
        }
    }
    fn validate_tree(&self, root: Option<u16>, depth: u8) -> Validation {
        let Some(slot) = root else {
            return Validation::EMPTY;
        };
        let index = usize::from(slot);
        let node = self.nodes[index];
        if depth > 64 || !node.in_tree || !self.entries[index].physical_index() {
            return Validation::INVALID;
        }
        let left = self.validate_tree(node.left, depth + 1);
        if !left.valid {
            return Validation::INVALID;
        }
        let right = self.validate_tree(node.right, depth + 1);
        if !right.valid {
            return Validation::INVALID;
        }
        if let Some(maximum) = left.maximum {
            let entry = self.entries[usize::from(maximum)];
            if !self.less(maximum, slot)
                || entry
                    .phys_base
                    .checked_add(entry.phys_len)
                    .is_none_or(|end| end > self.entries[index].phys_base)
            {
                return Validation::INVALID;
            }
        }
        if let Some(minimum) = right.minimum {
            let entry = self.entries[index];
            if !self.less(slot, minimum)
                || entry
                    .phys_base
                    .checked_add(entry.phys_len)
                    .is_none_or(|end| end > self.entries[usize::from(minimum)].phys_base)
            {
                return Validation::INVALID;
            }
        }
        let height = 1 + left.height.max(right.height);
        let balance = i16::from(left.height) - i16::from(right.height);
        if node.height != height || !(-1..=1).contains(&balance) {
            return Validation::INVALID;
        }
        Validation {
            valid: true,
            count: left.count.saturating_add(right.count).saturating_add(1),
            height,
            minimum: left.minimum.or(Some(slot)),
            maximum: right.maximum.or(Some(slot)),
        }
    }
    fn invariant(&self) -> bool {
        let physical = self.validate_tree(self.root, 0);
        if !physical.valid || physical.count != self.physical_count {
            return false;
        }
        let mut physical_count: u32 = 0;
        let mut id_count: u32 = 0;
        for index in 0..MAX_BLOCKS {
            let entry = self.entries[index];
            let reusable = self.slots[index / 64] & (1u64 << (index % 64)) != 0;
            if reusable != (!entry.slot_used || entry.status == RELEASED) {
                return false;
            }
            if entry.slot_used {
                id_count = id_count.saturating_add(1);
                if self.raw_id(entry.id) != Some(index) {
                    return false;
                }
            }
            if entry.physical_index() {
                physical_count = physical_count.saturating_add(1);
                if !self.nodes[index].in_tree {
                    return false;
                }
            } else if self.nodes[index].in_tree {
                return false;
            }
        }
        physical_count == self.physical_count && id_count == self.id_count
    }
}
struct Validation {
    valid: bool,
    count: u32,
    height: u8,
    minimum: Option<u16>,
    maximum: Option<u16>,
}
impl Validation {
    const EMPTY: Self = Self {
        valid: true,
        count: 0,
        height: 0,
        minimum: None,
        maximum: None,
    };
    const INVALID: Self = Self {
        valid: false,
        ..Self::EMPTY
    };
}

struct Storage(UnsafeCell<Table>);
// SAFETY: the exact physical-memory owner serializes every table access.
// A release plan carries values and a token; no table borrow spans its calls.
unsafe impl Sync for Storage {}
static TABLE: Storage = Storage(UnsafeCell::new(Table::EMPTY));
fn with_table<T>(operation: impl FnOnce(&mut Table) -> Result<T>) -> Result<T> {
    let _guard = Guard::acquire(7);
    let table = unsafe { &mut *TABLE.0.get() };
    if !table.initialized {
        Err(Error::NotInitialized)
    } else {
        operation(table)
    }
}
/// # Safety
/// The request uses valid enum values and a resident readable name, retained
/// for the claimed block's active lifetime under the catalogue contract.
pub unsafe fn claim(request: Block) -> Result<u32> {
    with_table(|table| table.claim(request))
}
fn code(result: Result<()>) -> u32 {
    result.err().map_or(0, |error| error as u32)
}
unsafe fn output_id(result: Result<u32>, output: *mut u32) -> u32 {
    match result {
        Ok(id) => {
            unsafe {
                output.write(id);
            }
            0
        }
        Err(error) => error as u32,
    }
}
fn boot_request(entry: info::MemoryEntry) -> Block {
    let (kind, owner, status, name): (u8, u8, u8, &[u8]) = match entry.kind {
        0 => (FREE_KIND, SYSTEM, FREE, b"free"),
        1 => (11, SYSTEM, RESERVED, b"reserved"),
        2 => (11, SYSTEM, RESERVED, b"acpi-reclaim"),
        3 => (11, SYSTEM, RESERVED, b"acpi-nvs"),
        4 => (11, SYSTEM, 6, b"bad-memory"),
        5 => (0, 6, RESERVED, b"bootloader"),
        6 => (1, 0, COMMITTED, b"kernel"),
        7 => (10, 5, MAPPED, b"framebuffer"),
        _ => (13, SYSTEM, RESERVED, b"unknown"),
    };
    let mut request = Block::physical(entry.base, entry.length, kind, owner, 0, status, name);
    request.reserved_bytes = if entry.kind == 0 { 0 } else { entry.length };
    request.committed_bytes = if entry.kind == 6 || entry.kind == 7 {
        entry.length
    } else {
        0
    };
    request
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_blocks_init() -> bool {
    let _guard = Guard::acquire(7);
    {
        let table = unsafe { &mut *TABLE.0.get() };
        table.reset();
        table.initialized = true;
    }
    let current = unsafe { &*info::r4os_rust_boot_info() };
    for index in 0..current.memory_count {
        let entry = unsafe { current.memory.add(index).read() };
        if !entry.valid || entry.length == 0 {
            continue;
        }
        // The old init acquired the same owner recursively through register.
        if with_table(|table| table.register(boot_request(entry))).is_err() {
            return false;
        }
    }
    true
}
/// # Safety
/// request/output are live exclusive C records; borrowed name lifetime follows
/// the caller's catalogue contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_blocks_register(request: *const Block, output: *mut u32) -> u32 {
    unsafe { output_id(with_table(|table| table.register(request.read())), output) }
}
/// # Safety
/// The request and output are live C records, with valid enum values and a
/// readable resident name retained for every resulting active block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_blocks_range(
    request: *const Block,
    retag: bool,
    output: *mut u32,
) -> u32 {
    unsafe {
        output_id(
            with_table(|table| {
                if retag {
                    table.retag(request.read())
                } else {
                    table.claim(request.read())
                }
            }),
            output,
        )
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_blocks_prefix(base: u64, max_len: u64, free: bool) -> u64 {
    with_table(|table| Ok(table.prefix(base, max_len, free))).unwrap_or(0)
}
/// # Safety
/// output is exclusive; success retains the exact CPU-bound owner token.
/// The caller must finish or cancel that plan on its acquiring CPU.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_blocks_prepare(
    base: u64,
    len: u64,
    output: *mut ReleasePlan,
) -> u32 {
    let lock = owner::named(7);
    let token = lock.acquire();
    let result = {
        let table = unsafe { &mut *TABLE.0.get() };
        if table.initialized {
            table.prepare_release(base, len)
        } else {
            Err(Error::NotInitialized)
        }
    };
    match result {
        Ok(mut plan) => {
            plan.lock_token = token;
            plan.active = true;
            unsafe {
                output.write(plan);
            }
            0
        }
        Err(error) => {
            lock.release(token);
            error as u32
        }
    }
}
/// # Safety
/// plan is the exact successful preparation, still owned on its original CPU.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_blocks_finish(
    plan: *mut ReleasePlan,
    coalesce: bool,
    cancel: bool,
) {
    let plan = unsafe { &mut *plan };
    if cancel && !plan.active {
        return;
    }
    assert!(plan.active, "inactive physical release plan");
    if !cancel {
        unsafe {
            (&mut *TABLE.0.get()).finish_release(plan, coalesce);
        }
    }
    plan.active = false;
    owner::named(7).release(plan.lock_token);
}
/// # Safety
/// update is live for operation 0, with valid enum values and resident names
/// for the resulting active block; other operations do not dereference it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_blocks_change(
    id: u32,
    operation: u8,
    bytes: u64,
    update: *const Update,
) -> u32 {
    code(with_table(|table| match operation {
        0 => table.update(id, unsafe { update.read() }),
        1 => table.set_committed(id, bytes),
        2 => table.change_committed(id, bytes, false),
        3 => table.change_committed(id, bytes, true),
        4 => table.release(id),
        _ => Err(Error::InvalidRange),
    }))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_blocks_coalesce() {
    let _ = with_table(|table| {
        table.coalesce();
        Ok(())
    });
}
/// # Safety
/// output is an exclusive live C record. operation 0=owner, 1=virtual, 2=active ordinal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_blocks_read(
    operation: u8,
    owner: u8,
    number: u64,
    output: *mut Block,
) -> bool {
    let result = with_table(|table| {
        let mut seen = 0u64;
        for entry in &table.entries {
            if !entry.active() {
                continue;
            }
            let matches = match operation {
                0 => entry.owner == owner && entry.owner_id == number,
                1 => {
                    entry.virt_len != 0
                        && entry
                            .virt_base
                            .checked_add(entry.virt_len)
                            .is_some_and(|end| number >= entry.virt_base && number < end)
                }
                2 => {
                    let matches = seen == number;
                    seen += 1;
                    matches
                }
                _ => false,
            };
            if matches {
                return Ok(Some(*entry));
            }
        }
        Ok(None)
    });
    if let Ok(Some(entry)) = result {
        unsafe {
            output.write(entry);
        }
        true
    } else {
        false
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_blocks_count(owner: u8, owner_id: u64) -> u64 {
    with_table(|table| {
        Ok(table
            .entries
            .iter()
            .filter(|entry| entry.active() && entry.owner == owner && entry.owner_id == owner_id)
            .count() as u64)
    })
    .unwrap_or(0)
}
/// # Safety
/// output is exclusive live storage with the asserted C Summary layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_blocks_summary(output: *mut Summary) {
    unsafe {
        output.write(with_table(|table| Ok(table.summary())).unwrap_or_default());
    }
}
/// # Safety
/// output is exclusive live storage with the asserted C HotStats layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_blocks_stats(output: *mut HotStats) {
    let stats = with_table(|table| {
        let mut stats = table.hot;
        stats.physical_index_entries = table.physical_count;
        stats.id_index_entries = table.id_count;
        Ok(stats)
    })
    .unwrap_or_default();
    unsafe {
        output.write(stats);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_blocks_invariant() -> bool {
    with_table(|table| Ok(table.invariant())).unwrap_or(false)
}

const _: () = {
    assert!(core::mem::size_of::<Block>() == 88);
    assert!(core::mem::offset_of!(Block, owner_id) == 16);
    assert!(core::mem::offset_of!(Block, phys_base) == 40);
    assert!(core::mem::size_of::<Update>() == 48);
    assert!(core::mem::size_of::<Snapshot>() == 24);
    assert!(core::mem::size_of::<Summary>() == 328);
    assert!(core::mem::size_of::<HotStats>() == 120);
    assert!(core::mem::size_of::<ReleasePlan>() == 616);
};

// Host-only cases exercise the real table algorithms without privileged
// lock instructions. This module is absent from the product archive.

/// Pointer-free passive copy under the same physical-memory catalog owner.
pub(crate) struct DiagnosticBlock {
    pub info: Block,
    pub name: [u8; 32],
}
pub(crate) fn diagnostic_active(index: u32, copy_name: bool) -> Option<DiagnosticBlock> {
    with_table(|table| {
        let mut seen = 0u64;
        for entry in &table.entries {
            if !entry.active() {
                continue;
            }
            if seen == u64::from(index) {
                let mut name = [0u8; 32];
                if copy_name && entry.name_len != 0 {
                    let len = entry.name_len.min(name.len() - 1);
                    // Name storage remains valid until catalog release; copy
                    // before releasing that exact owner, never lend it out.
                    unsafe {
                        core::ptr::copy_nonoverlapping(entry.name_ptr, name.as_mut_ptr(), len)
                    };
                }
                let mut info = *entry;
                info.name_ptr = core::ptr::null();
                info.name_len = 0;
                return Ok(Some(DiagnosticBlock { info, name }));
            }
            seen += 1;
        }
        Ok(None)
    })
    .ok()
    .flatten()
}
