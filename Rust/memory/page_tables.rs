//! Private-root construction. Caller-pinned C storage retains the exact frame
//! list; Rust alone mutates it until the existing boot owner adopts the root.
use super::{
    blocks::{self, ReleasePlan},
    phys,
};
use core::mem::MaybeUninit;
const PAGE: u64 = 4096;
const HUGE_BYTES: u64 = 2 * 1024 * 1024;
const MASK: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 2;
const HUGE: u64 = 1 << 7;
const TRACKED: usize = 512;
#[repr(C)]
pub struct Builder {
    pub pml4_phys: u64,
    pub table_frames: [u64; TRACKED],
    pub table_frame_count: usize,
    pub owned: bool,
}
#[repr(C)]
#[derive(Default)]
pub struct RangeStats {
    pub pages_4k: u64,
    pub pages_2m: u64,
    pub skipped: u64,
}
#[repr(C)]
#[derive(Default)]
pub struct Walk {
    pub mapped: bool,
    pub virt: u64,
    pub phys: u64,
    pub flags: u64,
    pub level: u8,
    pub huge: bool,
}
fn aligned(value: u64, alignment: u64) -> bool {
    value & (alignment - 1) == 0
}
fn table(physical: u64) -> Option<*mut u64> {
    if !aligned(physical, PAGE) {
        return None;
    }
    let address = phys::r4os_rust_phys_to_virt(physical);
    if address == 0 {
        None
    } else {
        Some(address as *mut u64)
    }
}
fn index(address: u64, shift: u32) -> usize {
    ((address >> shift) & 511) as usize
}
fn at(table: *mut u64, slot: usize) -> *mut u64 {
    unsafe { table.add(slot) }
}
fn read(entry: *mut u64) -> u64 {
    unsafe { entry.read_volatile() }
}
fn write(entry: *mut u64, value: u64) {
    unsafe {
        entry.write_volatile(value);
    }
}
fn sanitize(flags: u64) -> u64 {
    // Builder inputs use normalized 4-KB attributes: bit7 is PAT here.
    flags & !MASK
}
fn release_private_frame(frame: u64) {
    if frame == 0 {
        return;
    }
    let mut plan = MaybeUninit::<ReleasePlan>::uninit();
    if unsafe { blocks::r4os_rust_blocks_prepare(frame, PAGE, plan.as_mut_ptr()) } != 0 {
        return;
    }
    // These are the builder's private, unpublished frames. Active mappings
    // retain the separate paging/TLB completion path.
    phys::r4os_rust_phys_free(frame);
    unsafe {
        blocks::r4os_rust_blocks_finish(plan.as_mut_ptr(), false, false);
    }
}
impl Builder {
    pub(super) fn reset(&mut self) {
        self.pml4_phys = 0;
        self.table_frames.fill(0);
        self.table_frame_count = 0;
        self.owned = false;
    }
    fn allocate(&mut self) -> Option<u64> {
        if self.table_frame_count >= TRACKED {
            return None;
        }
        let mut frame = 0;
        if !unsafe { phys::r4os_rust_phys_alloc(&mut frame) } {
            return None;
        }
        let request = blocks::Block::physical(frame, PAGE, 3, 0, 0, 2, b"page-table");
        if unsafe { blocks::claim(request) }.is_err() {
            phys::r4os_rust_phys_free(frame);
            return None;
        }
        let Some(table) = table(frame) else {
            release_private_frame(frame);
            return None;
        };
        unsafe {
            core::ptr::write_bytes(table, 0, 512);
        }
        self.table_frames[self.table_frame_count] = frame;
        self.table_frame_count += 1;
        Some(frame)
    }
    pub(super) fn init(&mut self) -> bool {
        self.reset();
        let Some(root) = self.allocate() else {
            return false;
        };
        self.pml4_phys = root;
        self.owned = true;
        true
    }
    pub(super) fn deinit(&mut self) {
        for index in (0..self.table_frame_count).rev() {
            release_private_frame(self.table_frames[index]);
            self.table_frames[index] = 0;
        }
        self.reset();
    }
    fn next(&mut self, table: *mut u64, slot: usize) -> Option<*mut u64> {
        let entry = at(table, slot);
        if read(entry) & PRESENT == 0 {
            let frame = self.allocate()?;
            write(entry, (frame & MASK) | PRESENT | WRITABLE);
        }
        let value = read(entry);
        if value & HUGE != 0 {
            None
        } else {
            self::table(value & MASK)
        }
    }
    fn directory(&mut self, address: u64) -> Option<*mut u64> {
        let pml4 = table(self.pml4_phys)?;
        let pdpt = self.next(pml4, index(address, 39))?;
        self.next(pdpt, index(address, 30))
    }
    fn leaf_table(&mut self, address: u64, split: bool) -> Option<*mut u64> {
        let directory = self.directory(address)?;
        let slot = index(address, 21);
        let entry = at(directory, slot);
        let value = read(entry);
        if split && value & PRESENT != 0 && value & HUGE != 0 {
            let frame = self.allocate()?;
            let child = table(frame)?;
            let base = value & MASK & !(HUGE_BYTES - 1);
            let flags = (value & !MASK & !HUGE) | if value & (1 << 12) != 0 { 1 << 7 } else { 0 };
            for page in 0..512 {
                write(
                    at(child, page),
                    (base + page as u64 * PAGE) | flags | PRESENT,
                );
            }
            write(entry, (frame & MASK) | PRESENT | WRITABLE);
            return Some(child);
        }
        self.next(directory, slot)
    }
    pub(super) fn map_page(
        &mut self,
        address: u64,
        physical: u64,
        flags: u64,
        ensure_flags: bool,
    ) -> bool {
        if self.pml4_phys == 0 || !aligned(address, PAGE) || !aligned(physical, PAGE) {
            return false;
        }
        let Some(table) = self.leaf_table(address, ensure_flags) else {
            return false;
        };
        let entry = at(table, index(address, 12));
        let previous = read(entry);
        if previous & PRESENT != 0 && (!ensure_flags || previous & MASK != physical) {
            return false;
        }
        write(entry, (physical & MASK) | sanitize(flags) | PRESENT);
        true
    }
    pub(super) fn ensure_page(&mut self, address: u64, physical: u64, flags: u64) -> bool {
        if self.pml4_phys == 0 || !aligned(address, PAGE) || !aligned(physical, PAGE) {
            return false;
        }
        let existing = walk(self.pml4_phys, address);
        if existing.mapped {
            existing.phys & !(PAGE - 1) == physical
        } else {
            self.map_page(address, physical, flags, false)
        }
    }
    pub(super) fn map_huge(&mut self, address: u64, physical: u64, flags: u64) -> bool {
        if self.pml4_phys == 0 || !aligned(address, HUGE_BYTES) || !aligned(physical, HUGE_BYTES) {
            return false;
        }
        let Some(directory) = self.directory(address) else {
            return false;
        };
        let entry = at(directory, index(address, 21));
        if read(entry) & PRESENT != 0 {
            return false;
        }
        let attributes =
            (sanitize(flags) & !HUGE) | if flags & (1 << 7) != 0 { 1 << 12 } else { 0 };
        write(entry, (physical & MASK) | attributes | PRESENT | HUGE);
        true
    }
    pub(super) fn map_range(
        &mut self,
        address: u64,
        physical: u64,
        bytes: u64,
        flags: u64,
        auto: bool,
        stats: &mut RangeStats,
    ) -> bool {
        if bytes == 0 || !aligned(address, PAGE) || !aligned(physical, PAGE) {
            return false;
        }
        let Some(end) = bytes.checked_add(PAGE - 1).map(|value| value & !(PAGE - 1)) else {
            return false;
        };
        let mut offset = 0;
        while offset < end {
            let address = address + offset;
            let physical = physical + offset;
            if auto
                && end - offset >= HUGE_BYTES
                && aligned(address, HUGE_BYTES)
                && aligned(physical, HUGE_BYTES)
            {
                if !self.map_huge(address, physical, flags) {
                    return false;
                }
                stats.pages_2m += 1;
                offset += HUGE_BYTES;
            } else {
                if !self.map_page(address, physical, flags, false) {
                    return false;
                }
                if auto {
                    stats.pages_4k += 1;
                }
                offset += PAGE;
            }
        }
        true
    }
}
pub(super) fn walk(root: u64, address: u64) -> Walk {
    let absent = || Walk {
        virt: address,
        ..Walk::default()
    };
    if root == 0 {
        return absent();
    }
    let Some(pml4) = table(root) else {
        return absent();
    };
    let entry = read(at(pml4, index(address, 39)));
    if entry & PRESENT == 0 {
        return absent();
    }
    let Some(pdpt) = table(entry & MASK) else {
        return absent();
    };
    let entry = read(at(pdpt, index(address, 30)));
    if entry & PRESENT == 0 {
        return absent();
    }
    if entry & HUGE != 0 {
        return huge_walk(address, entry, 1 << 30, 3);
    }
    let Some(directory) = table(entry & MASK) else {
        return absent();
    };
    let entry = read(at(directory, index(address, 21)));
    if entry & PRESENT == 0 {
        return absent();
    }
    if entry & HUGE != 0 {
        return huge_walk(address, entry, HUGE_BYTES, 2);
    }
    let Some(leaf) = table(entry & MASK) else {
        return absent();
    };
    let entry = read(at(leaf, index(address, 12)));
    if entry & PRESENT == 0 {
        return absent();
    }
    Walk {
        mapped: true,
        virt: address,
        phys: (entry & MASK) | (address & (PAGE - 1)),
        flags: entry & !MASK,
        level: 1,
        huge: false,
    }
}
fn huge_walk(address: u64, entry: u64, bytes: u64, level: u8) -> Walk {
    Walk {
        mapped: true,
        virt: address,
        phys: (entry & MASK & !(bytes - 1)) | (address & (bytes - 1)),
        flags: (entry & !MASK & !HUGE) | if entry & (1 << 12) != 0 { 1 << 7 } else { 0 },
        level,
        huge: true,
    }
}
/// # Safety
/// builder is exclusive pinned C storage for a private root. Operation
/// 0 resets/initializes, 1 releases tracked private frames.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_builder_control(builder: *mut Builder, operation: u8) -> bool {
    let builder = unsafe { &mut *builder };
    match operation {
        0 => builder.init(),
        1 => {
            builder.deinit();
            true
        }
        _ => false,
    }
}
/// # Safety
/// builder exclusively owns its private root; the address/physical range is
/// retained for the operation. No active-root TLB completion is implied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_builder_map(
    builder: *mut Builder,
    address: u64,
    physical: u64,
    flags: u64,
    operation: u8,
) -> bool {
    let builder = unsafe { &mut *builder };
    match operation {
        0 => builder.map_page(address, physical, flags, false),
        1 => builder.ensure_page(address, physical, flags),
        2 => builder.map_page(address, physical, flags, true),
        3 => builder.map_huge(address, physical, flags),
        _ => false,
    }
}
/// # Safety
/// builder is exclusive private-root storage; stats is exclusive live output
/// for auto mapping. The plain path never dereferences stats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_builder_range(
    builder: *mut Builder,
    address: u64,
    physical: u64,
    bytes: u64,
    flags: u64,
    auto: bool,
    stats: *mut RangeStats,
) -> bool {
    let builder = unsafe { &mut *builder };
    if auto {
        builder.map_range(address, physical, bytes, flags, true, unsafe {
            &mut *stats
        })
    } else {
        builder.map_range(
            address,
            physical,
            bytes,
            flags,
            false,
            &mut RangeStats::default(),
        )
    }
}
/// # Safety
/// The caller retains the complete root and its traversed table frames, and
/// supplies exclusive live C output. This read does not admit a mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_builder_walk(root: u64, address: u64, output: *mut Walk) {
    unsafe {
        output.write(walk(root, address));
    }
}
const _: () = {
    assert!(core::mem::size_of::<Builder>() == 4120);
    assert!(core::mem::offset_of!(Builder, table_frame_count) == 4104);
    assert!(core::mem::size_of::<RangeStats>() == 24);
    assert!(core::mem::size_of::<Walk>() == 40);
};
