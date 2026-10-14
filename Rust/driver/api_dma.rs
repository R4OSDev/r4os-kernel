//! Original R4D DMA allocation/pin/map owner, serialized by the existing
//! execution admission. A caller descriptor never overrides a retained extent.
use crate::{
    contract as a,
    driver::{api_admission as admission, dma_segments as segments, irq_router, utilities},
    memory::layout,
    memory::{paging, phys},
};
use core::{cell::UnsafeCell, mem::size_of, ptr};
const CAP: usize = 128;
const PAGE: u64 = 4096;
const MAX_BYTES: u32 = 16 * 1024 * 1024;
#[derive(Clone, Copy)]
struct Allocation {
    used: bool,
    owner: u32,
    physical: u64,
    bytes: u32,
}
impl Allocation {
    const EMPTY: Self = Self {
        used: false,
        owner: 0,
        physical: 0,
        bytes: 0,
    };
}
#[derive(Clone, Copy)]
struct Pin {
    used: bool,
    generation: u64,
    owner: u32,
    address: u64,
    bytes: u32,
    pages: u32,
    maps: u32,
}
impl Pin {
    const EMPTY: Self = Self {
        used: false,
        generation: 0,
        owner: 0,
        address: 0,
        bytes: 0,
        pages: 0,
        maps: 0,
    };
}
struct Mapping {
    used: bool,
    generation: u64,
    owner: u32,
    pin_slot: usize,
    pin_handle: u64,
    direction: u32,
    flags: u32,
    original: u64,
    bytes: u32,
    bounce_physical: u64,
    bounce_address: u64,
    bounce_bytes: u32,
    device_owned: bool,
    segments: segments::SegmentList,
}
impl Mapping {
    const EMPTY: Self = Self {
        used: false,
        generation: 0,
        owner: 0,
        pin_slot: 0,
        pin_handle: 0,
        direction: 0,
        flags: 0,
        original: 0,
        bytes: 0,
        bounce_physical: 0,
        bounce_address: 0,
        bounce_bytes: 0,
        device_owned: false,
        segments: segments::SegmentList {
            count: 0,
            segments: [segments::Segment {
                phys_addr: 0,
                bytes: 0,
                reserved: 0,
            }; 64],
        },
    };
}
struct State {
    allocations: [Allocation; CAP],
    pins: [Pin; CAP],
    pin_generations: [u64; CAP],
    mappings: [Mapping; CAP],
    mapping_generations: [u64; CAP],
}
struct Storage(UnsafeCell<State>);
// SAFETY: original legacy DMA admission remains bound to the lifecycle/Work
// execution owner. Dedicated tasks/storage/IRQ gain no new permissions.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    allocations: [Allocation::EMPTY; CAP],
    pins: [Pin::EMPTY; CAP],
    pin_generations: [0; CAP],
    mappings: [const { Mapping::EMPTY }; CAP],
    mapping_generations: [0; CAP],
}));
fn raw() -> *mut State {
    STATE.0.get()
}
fn active() -> u32 {
    admission::r4os_rust_driver_api_runtime_owner(false)
}
fn irq() -> bool {
    irq_router::r4os_rust_irq_router_local(false) != 0
}
fn frames(bytes: u32) -> Option<u64> {
    if bytes == 0 {
        None
    } else {
        Some(u64::from(bytes).div_ceil(PAGE))
    }
}
fn allocate_frames(count: u64, maximum: u64) -> Option<u64> {
    let mut out = 0;
    if unsafe { phys::r4os_rust_phys_contiguous(count, maximum, false, &mut out) } {
        Some(out)
    } else {
        None
    }
}
fn take_allocation(id: u32, physical: u64) -> Option<Allocation> {
    if id == 0 {
        return None;
    }
    unsafe {
        for i in 0..CAP {
            let a = (*raw()).allocations[i];
            if a.used && a.owner == id && a.physical == physical {
                (*raw()).allocations[i] = Allocation::EMPTY;
                return Some(a);
            }
        }
    }
    None
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_dma_allocate_legacy(bytes: u32, alignment: u32) -> u64 {
    let mut out = a::DmaBuffer::default();
    if unsafe { r4os_rust_driver_dma_allocate(bytes, alignment, u64::MAX, &mut out) } != 0 {
        return 0;
    }
    out.phys_addr
}
/// # Safety
/// Resident DMA descriptors are aligned C records under the original driver
/// admission. Backing and mappings remain alive through actual device completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_dma_allocate(
    bytes: u32,
    alignment: u32,
    maximum: u64,
    out: *mut a::DmaBuffer,
) -> i32 {
    unsafe { ptr::write(out, a::DmaBuffer::default()) };
    let id = active();
    if id == 0 || irq() {
        return -6;
    }
    let alignment = if alignment == 0 {
        PAGE as u32
    } else {
        alignment
    };
    if bytes == 0 {
        return -1;
    }
    if u64::from(alignment) > PAGE {
        return -2;
    }
    let Some(count) = frames(bytes) else {
        return -3;
    };
    let Some(physical) = allocate_frames(count, maximum) else {
        return -4;
    };
    let address = phys::r4os_rust_phys_to_virt(physical);
    let total = count * PAGE;
    unsafe { ptr::write_bytes(address as *mut u8, 0, total as usize) };
    let mut slot = None;
    for i in 0..CAP {
        if !unsafe { (*raw()).allocations[i].used } {
            slot = Some(i);
            break;
        }
    }
    let Some(slot) = slot else {
        phys::r4os_rust_phys_free_contiguous(physical, count);
        return -5;
    };
    let stored = u32::try_from(total).expect("Original DMA rounded extent exceeds u32");
    unsafe {
        (*raw()).allocations[slot] = Allocation {
            used: true,
            owner: id,
            physical,
            bytes: stored,
        };
        ptr::write(
            out,
            a::DmaBuffer {
                phys_addr: physical,
                virt_addr: address,
                bytes: stored,
                alignment,
                flags: 0,
                reserved: 0,
            },
        )
    };
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_dma_free_physical(physical: u64) {
    if physical == 0 {
        return;
    }
    let Some(a) = take_allocation(active(), physical) else {
        return;
    };
    let Some(count) = frames(a.bytes) else { return };
    phys::r4os_rust_phys_free_contiguous(a.physical, count)
}
/// # Safety
/// Resident DMA descriptors are aligned C records under the original driver
/// admission. Backing and mappings remain alive through actual device completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_dma_free(out: *mut a::DmaBuffer) {
    let value = unsafe { ptr::read(out) };
    if value.phys_addr == 0 || value.bytes == 0 {
        unsafe { ptr::write(out, a::DmaBuffer::default()) };
        return;
    }
    if irq() {
        return;
    }
    let Some(a) = take_allocation(active(), value.phys_addr) else {
        return;
    };
    if let Some(count) = frames(a.bytes) {
        phys::r4os_rust_phys_free_contiguous(a.physical, count)
    }
    unsafe { ptr::write(out, a::DmaBuffer::default()) };
}
fn physical_page(address: u64) -> Option<u64> {
    if address & (PAGE - 1) != 0 {
        return None;
    }
    let direct = phys::r4os_rust_phys_to_virt(0);
    let mut stats = phys::Stats::default();
    unsafe { phys::r4os_rust_phys_stats(&mut stats) };
    let bytes = stats.total_frames.checked_mul(PAGE)?;
    let end = direct.checked_add(bytes)?;
    if address >= direct && address < end {
        return Some(address - direct);
    }
    if (layout::MMIO_BASE..layout::FRAMEBUFFER_END).contains(&address) {
        return None;
    }
    let mut physical = 0;
    if unsafe { paging::r4os_rust_paging_read(address, 0, false, &mut physical) } {
        Some(physical)
    } else {
        None
    }
}
/// # Safety
/// Resident DMA descriptors are aligned C records under the original driver
/// admission. Backing and mappings remain alive through actual device completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_dma_pin(
    address: u64,
    bytes: u32,
    flags: u32,
    out: *mut a::DmaPinnedBuffer,
) -> i32 {
    unsafe { ptr::write(out, a::DmaPinnedBuffer::default()) };
    let id = active();
    if id == 0 || irq() {
        return -1;
    }
    if address == 0 || bytes == 0 || bytes > MAX_BYTES || flags != 0 {
        return -2;
    }
    let Some(end) = address.checked_add(u64::from(bytes)) else {
        return -2;
    };
    if end > u64::MAX - (PAGE - 1) {
        return -2;
    }
    let first = address & !(PAGE - 1);
    let last = (end + PAGE - 1) & !(PAGE - 1);
    if last <= first {
        return -2;
    }
    let count = (last - first) / PAGE;
    if count == 0 || count > u64::from(u32::MAX) {
        return -2;
    }
    for i in 0..count {
        if physical_page(first + i * PAGE).is_none() {
            return -3;
        }
    }
    let mut slot = None;
    for i in 0..CAP {
        if !unsafe { (*raw()).pins[i].used } {
            slot = Some(i);
            break;
        }
    }
    let Some(i) = slot else { return -5 };
    let generation = segments::r4os_rust_dma_handle(0, unsafe { (*raw()).pin_generations[i] }, 0);
    unsafe { (*raw()).pin_generations[i] = generation };
    let handle = segments::r4os_rust_dma_handle(1, i as u64, generation);
    if handle == 0 {
        return -5;
    }
    unsafe {
        (*raw()).pins[i] = Pin {
            used: true,
            generation,
            owner: id,
            address,
            bytes,
            pages: count as u32,
            maps: 0,
        };
        ptr::write(
            out,
            a::DmaPinnedBuffer {
                handle,
                virt_addr: address,
                bytes,
                page_count: count as u32,
                flags,
                ..a::DmaPinnedBuffer::default()
            },
        )
    };
    0
}
fn mapping_slot(handle: u64, id: u32) -> Option<usize> {
    if id == 0 {
        return None;
    }
    let i = segments::r4os_rust_dma_handle(2, handle, CAP as u64);
    if i == u64::MAX {
        return None;
    }
    let i = i as usize;
    if unsafe {
        (*raw()).mappings[i].used
            && (*raw()).mappings[i].owner == id
            && (*raw()).mappings[i].generation == segments::r4os_rust_dma_handle(3, handle, 0)
    } {
        Some(i)
    } else {
        None
    }
}
fn constraints(c: a::DmaConstraints) -> Option<segments::Constraints> {
    if c.boundary != 0 && !c.boundary.is_power_of_two() || !c.alignment.is_power_of_two() {
        return None;
    }
    Some(segments::Constraints {
        dma_mask: c.dma_mask,
        boundary: c.boundary,
        max_segment_bytes: c.max_segment_bytes,
        alignment: c.alignment,
        max_segment_count: c.max_segments,
    })
}
unsafe fn build(pin: Pin, c: segments::Constraints, out: *mut segments::SegmentList) -> u8 {
    let Some(end) = pin.address.checked_add(u64::from(pin.bytes)) else {
        return 5;
    };
    if end > u64::MAX - (PAGE - 1) {
        return 5;
    }
    let first = pin.address & !(PAGE - 1);
    let last = (end + PAGE - 1) & !(PAGE - 1);
    let count = (last - first) / PAGE;
    if count != u64::from(pin.pages) {
        return 1;
    }
    let mut remaining = u64::from(pin.bytes);
    for i in 0..count {
        let Some(frame) = physical_page(first + i * PAGE) else {
            return 1;
        };
        let offset = if i == 0 { pin.address - first } else { 0 };
        let take = remaining.min(PAGE - offset) as u32;
        let code = unsafe { segments::r4os_rust_dma_range(out, frame + offset, take, &c, true) };
        if code != 0 {
            return code;
        }
        remaining -= u64::from(take)
    }
    if remaining != 0 { 1 } else { 0 }
}
unsafe fn synchronize(i: usize, cpu: bool, offset: u32, bytes: u32) -> bool {
    let r = unsafe { ptr::addr_of_mut!((*raw()).mappings[i]) };
    if !unsafe { (*r).used } {
        return false;
    }
    let success = unsafe {
        utilities::r4os_rust_driver_dma_synchronize(
            (*r).original as *mut u8,
            (*r).bytes as usize,
            (*r).bounce_address as *mut u8,
            if (*r).bounce_address == 0 {
                0
            } else {
                (*r).bytes as usize
            },
            (*r).direction,
            cpu,
            offset,
            bytes,
        )
    };
    if success && offset == 0 && bytes == unsafe { (*r).bytes } {
        unsafe { (*r).device_owned = !cpu }
    }
    success
}
fn release_mapping(i: usize, copy_back: bool) {
    if i >= CAP || !unsafe { (*raw()).mappings[i].used } {
        return;
    }
    let bytes = unsafe { (*raw()).mappings[i].bytes };
    if copy_back {
        unsafe { synchronize(i, true, 0, bytes) };
    }
    let physical = unsafe { (*raw()).mappings[i].bounce_physical };
    let bounce = unsafe { (*raw()).mappings[i].bounce_bytes };
    if physical != 0
        && let Some(count) = frames(bounce)
    {
        phys::r4os_rust_phys_free_contiguous(physical, count)
    }
    let p = unsafe { (*raw()).mappings[i].pin_slot };
    if p < CAP && unsafe { (*raw()).pins[p].used && (*raw()).pins[p].maps != 0 } {
        unsafe { (*raw()).pins[p].maps -= 1 }
    }
    unsafe { ptr::write(ptr::addr_of_mut!((*raw()).mappings[i]), Mapping::EMPTY) };
}
fn descriptor(i: usize, handle: u64) -> a::DmaMapping {
    let r = unsafe { ptr::addr_of!((*raw()).mappings[i]) };
    let mut out = a::DmaMapping::default();
    unsafe {
        out.handle = handle;
        out.pin_handle = (*r).pin_handle;
        out.requested_bytes = (*r).bytes;
        out.mapped_bytes = (*r).bytes;
        out.direction = (*r).direction;
        out.flags = (*r).flags;
        out.segment_count = (*r).segments.count;
        for n in 0..64 {
            out.segments[n] = a::DmaSegment {
                phys_addr: (*r).segments.segments[n].phys_addr,
                bytes: (*r).segments.segments[n].bytes,
                reserved: (*r).segments.segments[n].reserved,
            };
        }
    }
    out
}
/// # Safety
/// Resident DMA descriptors are aligned C records under the original driver
/// admission. Backing and mappings remain alive through actual device completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_dma_map(
    pin: *const a::DmaPinnedBuffer,
    input: *const a::DmaConstraints,
    direction: u32,
    out: *mut a::DmaMapping,
) -> i32 {
    unsafe { ptr::write(out, a::DmaMapping::default()) };
    let id = active();
    if id == 0 || irq() {
        return -1;
    }
    let p = unsafe { ptr::read(pin) };
    let c = unsafe { ptr::read(input) };
    let mode = c.flags & 3;
    if p.version != 1
        || p.size < size_of::<a::DmaPinnedBuffer>() as u32
        || p.handle == 0
        || c.version != 1
        || c.size < size_of::<a::DmaConstraints>() as u32
        || c.reserved != 0
        || c.max_segments == 0
        || c.max_segments > 64
        || c.alignment == 0
        || u64::from(c.alignment) > PAGE
        || mode == 0
        || mode == 3
        || c.flags & !7 != 0
        || direction > 2
    {
        return -2;
    }
    let i = segments::r4os_rust_dma_handle(2, p.handle, CAP as u64);
    if i == u64::MAX {
        return -3;
    }
    let i = i as usize;
    let pinned = unsafe { (*raw()).pins[i] };
    if !pinned.used
        || pinned.owner != id
        || pinned.generation != segments::r4os_rust_dma_handle(3, p.handle, 0)
        || pinned.address != p.virt_addr
        || pinned.bytes != p.bytes
    {
        return -3;
    }
    if pinned.maps == u32::MAX {
        return -4;
    }
    let mut slot = None;
    for n in 0..CAP {
        if !unsafe { (*raw()).mappings[n].used } {
            slot = Some(n);
            break;
        }
    }
    let Some(slot) = slot else { return -5 };
    let Some(normalized) = constraints(c) else {
        return -2;
    };
    let mut list = Mapping::EMPTY.segments;
    let result = unsafe { build(pinned, normalized, &mut list) };
    match result {
        0 => {}
        1 | 5 => return -2,
        2..=4 => {
            if c.flags & 4 == 0 {
                return -6;
            }
            list = Mapping::EMPTY.segments
        }
        _ => unreachable!(),
    }
    let mut bounce_physical = 0;
    let mut bounce_address = 0;
    let mut bounce_bytes = 0;
    let mut mapping_flags = u32::from(c.flags);
    if list.count == 0 {
        let Some(count) = frames(pinned.bytes) else {
            return -2;
        };
        let Some(physical) = allocate_frames(count, c.dma_mask) else {
            return -7;
        };
        bounce_physical = physical;
        let total = count * PAGE;
        if total > u64::from(u32::MAX) {
            phys::r4os_rust_phys_free_contiguous(physical, count);
            return -7;
        }
        bounce_bytes = total as u32;
        bounce_address = phys::r4os_rust_phys_to_virt(physical);
        if unsafe {
            segments::r4os_rust_dma_range(&mut list, physical, pinned.bytes, &normalized, false)
        } != 0
        {
            phys::r4os_rust_phys_free_contiguous(physical, count);
            return -7;
        }
        unsafe { ptr::write_bytes(bounce_address as *mut u8, 0, bounce_bytes as usize) };
        mapping_flags |= 1 << 16;
    }
    let generation =
        segments::r4os_rust_dma_handle(0, unsafe { (*raw()).mapping_generations[slot] }, 0);
    unsafe { (*raw()).mapping_generations[slot] = generation };
    let handle = segments::r4os_rust_dma_handle(1, slot as u64, generation);
    if handle == 0 {
        if bounce_physical != 0 {
            phys::r4os_rust_phys_free_contiguous(bounce_physical, frames(bounce_bytes).unwrap_or(0))
        }
        return -5;
    }
    unsafe {
        ptr::write(
            ptr::addr_of_mut!((*raw()).mappings[slot]),
            Mapping {
                used: true,
                generation,
                owner: id,
                pin_slot: i,
                pin_handle: p.handle,
                direction,
                flags: mapping_flags,
                original: pinned.address,
                bytes: pinned.bytes,
                bounce_physical,
                bounce_address,
                bounce_bytes,
                device_owned: false,
                segments: list,
            },
        );
        (*raw()).pins[i].maps += 1
    };
    if !unsafe { synchronize(slot, false, 0, pinned.bytes) } {
        release_mapping(slot, false);
        return -8;
    }
    unsafe { ptr::write(out, descriptor(slot, handle)) };
    0
}
/// # Safety
/// Resident DMA descriptors are aligned C records under the original driver
/// admission. Backing and mappings remain alive through actual device completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_dma_sync(
    mapping: *const a::DmaMapping,
    cpu: bool,
    range: bool,
    offset: u32,
    bytes: u32,
) -> i32 {
    if !range {
        if irq() {
            return -1;
        }
        let Some(i) = mapping_slot(unsafe { (*mapping).handle }, active()) else {
            return -2;
        };
        return if unsafe { synchronize(i, cpu, 0, (*raw()).mappings[i].bytes) } {
            0
        } else {
            -3
        };
    }
    let id = active();
    if id == 0 || irq() || admission::r4os_rust_driver_api_storage_owner() != 0 {
        return -1;
    }
    if unsafe {
        (*mapping).version != 1
            || (*mapping).size < size_of::<a::DmaMapping>() as u32
            || (*mapping).handle == 0
    } {
        return -2;
    }
    let Some(i) = mapping_slot(unsafe { (*mapping).handle }, id) else {
        return -3;
    };
    if unsafe { synchronize(i, cpu, offset, bytes) } {
        0
    } else {
        -2
    }
}
/// # Safety
/// Resident DMA descriptors are aligned C records under the original driver
/// admission. Backing and mappings remain alive through actual device completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_dma_unmap(out: *mut a::DmaMapping) -> i32 {
    if irq() {
        return -1;
    }
    let Some(i) = mapping_slot(unsafe { (*out).handle }, active()) else {
        return -2;
    };
    release_mapping(i, true);
    unsafe { ptr::write(out, a::DmaMapping::default()) };
    0
}
/// # Safety
/// Resident DMA descriptors are aligned C records under the original driver
/// admission. Backing and mappings remain alive through actual device completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_dma_unpin(out: *mut a::DmaPinnedBuffer) -> i32 {
    if irq() {
        return -1;
    }
    let id = active();
    let handle = unsafe { (*out).handle };
    let i = segments::r4os_rust_dma_handle(2, handle, CAP as u64);
    if i == u64::MAX {
        return -2;
    }
    let i = i as usize;
    let p = unsafe { (*raw()).pins[i] };
    if !p.used || p.owner != id || p.generation != segments::r4os_rust_dma_handle(3, handle, 0) {
        return -2;
    }
    if p.maps != 0 {
        return -3;
    }
    unsafe {
        (*raw()).pins[i] = Pin::EMPTY;
        ptr::write(out, a::DmaPinnedBuffer::default())
    };
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_dma_cleanup(id: u32) -> u32 {
    let mut removed = 0;
    for i in 0..CAP {
        if unsafe { (*raw()).mappings[i].used && (*raw()).mappings[i].owner == id } {
            release_mapping(i, true);
            removed += 1
        }
    }
    for i in 0..CAP {
        if unsafe {
            (*raw()).pins[i].used && (*raw()).pins[i].owner == id && (*raw()).pins[i].maps == 0
        } {
            unsafe { (*raw()).pins[i] = Pin::EMPTY };
            removed += 1
        }
    }
    for i in 0..CAP {
        let a = unsafe { (*raw()).allocations[i] };
        if !a.used || a.owner != id {
            continue;
        }
        if let Some(count) = frames(a.bytes) {
            phys::r4os_rust_phys_free_contiguous(a.physical, count)
        }
        unsafe { (*raw()).allocations[i] = Allocation::EMPTY };
        removed += 1
    }
    removed
}
