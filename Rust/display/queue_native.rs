//! Resident opaque commands and canonical VA/BO execution retention. Device
//! commands, placement and interface policy remain with the registered R4D.
use super::{error::Error, queue_resources as resources, queue_state as queue};
use crate::{
    contract as a,
    gfx::{
        buffers, layout,
        types::{Handle, Owner},
        virtual_runtime as virtuals,
    },
    memory::heap,
};
use core::{
    ffi::c_void,
    mem::{MaybeUninit, align_of, size_of},
    ptr, slice,
};
struct Resource {
    input: a::GfxNativeResource,
    held: Option<virtuals::Execution>,
    object: Handle,
    offset: u64,
    bytes: u64,
    write: bool,
    leader: usize,
}
pub struct Job {
    allocation: *mut u8,
    allocation_len: usize,
    pub info: a::GfxNativeJobInfo,
    resources: *mut Resource,
    pub resource_count: usize,
    pub uses: *mut resources::UseSlot,
    order: *mut usize,
    pub commands: *mut u8,
    pub command_bytes: usize,
}
fn align(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}
impl Job {
    /// # Safety
    /// Original validated resource/command byte ranges remain readable through
    /// their resident copy. The returned exact allocation owns every subarray.
    pub unsafe fn create(input: a::GfxNativeSubmission) -> Result<*mut Self, Error> {
        if input.version != 1
            || input.size != size_of::<a::GfxNativeSubmission>() as u32
            || input.reserved0 != 0
            || input.revision == 0
            || (input.interface_id_lo == 0 && input.interface_id_hi == 0)
            || (input.commands == 0) != (input.command_bytes == 0)
            || (input.resources == 0) != (input.resource_count == 0)
            || !input
                .resources
                .is_multiple_of(align_of::<a::GfxNativeResource>() as u64)
            || input.commands > u64::MAX - u64::from(input.command_bytes)
            || input.resources
                > u64::MAX
                    - u64::from(input.resource_count) * size_of::<a::GfxNativeResource>() as u64
        {
            return Err(Error::Invalid);
        }
        let count = input.resource_count as usize;
        let resource_offset = align(size_of::<Self>(), align_of::<Resource>());
        let use_offset = align(
            resource_offset + count * size_of::<Resource>(),
            align_of::<resources::UseSlot>(),
        );
        let order_offset = align(
            use_offset + count * size_of::<resources::UseSlot>(),
            align_of::<usize>(),
        );
        let command_offset = order_offset + count * size_of::<usize>();
        let allocation_len = command_offset + input.command_bytes as usize;
        let allocation = heap::r4os_rust_heap_alloc(
            allocation_len,
            align_of::<Self>().max(align_of::<Resource>()),
        );
        if allocation.is_null() {
            return Err(Error::OutOfMemory);
        }
        let job = allocation.cast::<Self>();
        let items = unsafe { allocation.add(resource_offset).cast::<Resource>() };
        let uses = unsafe { allocation.add(use_offset).cast::<resources::UseSlot>() };
        let order = unsafe { allocation.add(order_offset).cast::<usize>() };
        let commands = unsafe { allocation.add(command_offset) };
        unsafe {
            job.write(Self {
                allocation,
                allocation_len,
                info: a::GfxNativeJobInfo {
                    interface_id_lo: input.interface_id_lo,
                    interface_id_hi: input.interface_id_hi,
                    revision: input.revision,
                    command_bytes: input.command_bytes,
                    resource_count: input.resource_count,
                    ..a::GfxNativeJobInfo::default()
                },
                resources: items,
                resource_count: count,
                uses,
                order,
                commands,
                command_bytes: input.command_bytes as usize,
            });
        }
        for index in 0..count {
            unsafe {
                uses.add(index).write(resources::UseSlot::EMPTY);
                items.add(index).write(Resource {
                    input: ptr::with_exposed_provenance::<a::GfxNativeResource>(
                        input.resources as usize,
                    )
                    .add(index)
                    .read(),
                    held: None,
                    object: Handle::EMPTY,
                    offset: 0,
                    bytes: 0,
                    write: false,
                    leader: 0,
                });
            }
        }
        if input.command_bytes != 0 {
            unsafe {
                ptr::copy_nonoverlapping(
                    ptr::with_exposed_provenance::<u8>(input.commands as usize),
                    commands,
                    input.command_bytes as usize,
                )
            };
        }
        Ok(job)
    }
    /// # Safety
    /// Exact resident job and all nodes are retained under the common BO owner;
    /// every use slot was prepared before entry. No callback/allocation/wait.
    pub unsafe fn acquire_locked(
        &mut self,
        producer: Owner,
        binding: layout::Binding,
        state: &queue::Table<'_>,
        resources: &mut resources::Table<'_>,
        timeline: u64,
        dependencies: &[queue::Fence],
    ) -> Result<(), Error> {
        let items = unsafe { slice::from_raw_parts_mut(self.resources, self.resource_count) };
        let order = unsafe { slice::from_raw_parts_mut(self.order, self.resource_count) };
        for (index, item) in items.iter_mut().enumerate() {
            let held = unsafe { virtuals::retain_execution_locked(producer, item.input, binding) }?;
            item.held = Some(held);
            item.object = unsafe { buffers::read() }.buffer_for(held.reference, held.driver)?;
            let desc = unsafe { buffers::read() }.describe(held.reference, held.driver)?;
            if !desc.binding.portable() && desc.binding != binding {
                return Err(Error::Stale);
            }
            item.offset = unsafe { (*held.entry).request.byte_offset };
            item.bytes = unsafe { (*held.entry).request.byte_length };
            if !layout::span_fits(desc.bytes, item.offset, item.bytes) {
                return Err(Error::Invalid);
            }
            item.write = item.input.access == 1;
            item.leader = index;
            order[index] = index;
        }
        // Preserve the original std.sort.heap comparisons and equal-key swaps.
        heap_sort(order, items);
        for (position, &index) in order.iter().enumerate() {
            if position == 0 {
                continue;
            }
            let previous = order[position - 1];
            if items[previous].object == items[index].object {
                items[index].leader = items[previous].leader;
                let leader = items[index].leader;
                let end = (items[leader].offset + items[leader].bytes)
                    .max(items[index].offset + items[index].bytes);
                items[leader].offset = items[leader].offset.min(items[index].offset);
                items[leader].bytes = end - items[leader].offset;
                items[leader].write |= items[index].write;
            }
        }
        for (index, item) in items.iter().enumerate() {
            if item.leader != index {
                continue;
            }
            resources.ordered(state, timeline, dependencies, item.object, item.write)?;
            let held = item.held.expect("exact native execution");
            let use_record = unsafe { buffers::table() }.reserve_queued(
                held.reference,
                held.driver,
                resources::OWNER,
                item.write,
                item.offset,
                item.bytes,
            )?;
            unsafe { (*self.uses.add(index)).set(use_record) };
        }
        Ok(())
    }
    /// # Safety
    /// Shared BO owner still retains this exact job after partial admission.
    pub unsafe fn rollback_locked(&mut self) {
        for slot in unsafe { slice::from_raw_parts_mut(self.uses, self.resource_count) } {
            if let Some(held) = slot.get() {
                unsafe { buffers::table() }
                    .end_use(held.lease, resources::OWNER, true)
                    .expect("native exact rollback");
                slot.has = false;
            }
        }
        unsafe { self.release_bindings_locked() }
    }
    /// # Safety
    /// Exact job under the BO owner; every queued resource lease is released.
    pub unsafe fn release_bindings_locked(&mut self) {
        for slot in unsafe { slice::from_raw_parts(self.uses, self.resource_count) } {
            assert!(!slot.has);
        }
        for item in unsafe { slice::from_raw_parts_mut(self.resources, self.resource_count) } {
            if let Some(held) = item.held {
                unsafe { virtuals::release_execution_locked(held) };
                item.held = None;
            }
        }
    }
    /// # Safety
    /// Exact resident job under the shared owner, with current device admission.
    pub unsafe fn binding(&self, index: u32) -> Result<a::GfxNativeBinding, Error> {
        if index as usize >= self.resource_count {
            return Err(Error::Invalid);
        }
        unsafe { (*self.resources.add(index as usize)).held }
            .map(|held| held.binding)
            .ok_or(Error::Stale)
    }
    /// # Safety
    /// Consumes the exact allocation after all BO/VA execution leases ended;
    /// no Rust reference to this header may span the actual heap release.
    pub unsafe fn destroy(job: *mut Self) {
        let value = unsafe { &*job };
        for slot in unsafe { slice::from_raw_parts(value.uses, value.resource_count) } {
            assert!(!slot.has);
        }
        for item in unsafe { slice::from_raw_parts(value.resources, value.resource_count) } {
            assert!(item.held.is_none());
        }
        let allocation = value.allocation;
        let len = value.allocation_len;
        assert_eq!(unsafe { heap::r4os_rust_heap_free(allocation, len) }, 0);
    }
}
fn less(items: &[Resource], left: usize, right: usize) -> bool {
    let lhs = items[left].object;
    let rhs = items[right].object;
    if lhs.id != rhs.id {
        lhs.id < rhs.id
    } else {
        lhs.generation < rhs.generation
    }
}
// Keep the original Zig heap-sort comparison/swap control flow.
#[allow(clippy::while_let_loop)]
fn sift(order: &mut [usize], items: &[Resource], target: usize, end: usize) {
    let mut current = target;
    loop {
        let Some(child) = current.checked_mul(2) else {
            break;
        };
        let mut child = child + 1;
        if child >= end {
            break;
        }
        let next = child + 1;
        if next < end && less(items, order[child], order[next]) {
            child = next;
        }
        if less(items, order[child], order[current]) {
            break;
        }
        order.swap(child, current);
        current = child;
    }
}
fn heap_sort(order: &mut [usize], items: &[Resource]) {
    let mut i = order.len() / 2;
    while i != 0 {
        i -= 1;
        sift(order, items, i, order.len());
    }
    let mut i = order.len();
    while i != 0 {
        i -= 1;
        order.swap(0, i);
        sift(order, items, 0, i);
    }
}
#[repr(C)]
pub struct Projection {
    pub info: a::GfxNativeJobInfo,
    pub resource_count: usize,
    pub uses: *mut resources::UseSlot,
    pub commands: *mut u8,
    pub command_bytes: usize,
}
/// # Safety
/// The selected exact job/input and copied output remain retained throughout.
/// Opaque job handles never expose a native Rust resource/Option layout to Zig.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_queue_native(
    op: u32,
    job: *mut Job,
    input: *const c_void,
    out: *mut c_void,
    index: u32,
) -> i32 {
    let result = match op {
        0 => unsafe { Job::create(input.cast::<a::GfxNativeSubmission>().read()) }
            .map(|job| unsafe { out.cast::<*mut Job>().write(job) }),
        1 => {
            unsafe { (&mut *job).rollback_locked() };
            Ok(())
        }
        2 => {
            unsafe { (&mut *job).release_bindings_locked() };
            Ok(())
        }
        3 => {
            unsafe { Job::destroy(job) };
            Ok(())
        }
        4 => {
            let job = unsafe { &*job };
            unsafe {
                out.cast::<Projection>().write(Projection {
                    info: job.info,
                    resource_count: job.resource_count,
                    uses: job.uses,
                    commands: job.commands,
                    command_bytes: job.command_bytes,
                })
            };
            Ok(())
        }
        5 => unsafe { (&*job).binding(index) }
            .map(|v| unsafe { out.cast::<a::GfxNativeBinding>().write(v) }),
        _ => panic!("native queue job operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
#[repr(C)]
pub struct Acquire {
    pub producer: Owner,
    pub binding: layout::Binding,
    pub queue: queue::RawView,
    pub resources: resources::RawView,
    pub timeline: u64,
    pub dependencies: *const queue::Fence,
    pub dependencies_len: usize,
    pub seen: *mut bool,
    pub pending: *mut MaybeUninit<u16>,
}
/// # Safety
/// All disjoint resident metadata is admitted by the shared BO owner and exact
/// prepared leases remain available. Scratch arrays cover the queue job count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_queue_native_acquire(
    job: *mut Job,
    args: *const Acquire,
) -> i32 {
    let args = unsafe { &*args };
    let state = unsafe { args.queue.borrow() };
    let seen = unsafe { slice::from_raw_parts_mut(args.seen, args.queue.jobs_len) };
    let pending = unsafe { slice::from_raw_parts_mut(args.pending, args.queue.jobs_len) };
    let mut resources = unsafe { args.resources.borrow(seen, pending) };
    let dependencies = if args.dependencies_len == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(args.dependencies, args.dependencies_len) }
    };
    match unsafe {
        (&mut *job).acquire_locked(
            args.producer,
            args.binding,
            &state,
            &mut resources,
            args.timeline,
            dependencies,
        )
    } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
