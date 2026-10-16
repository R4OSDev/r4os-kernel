//! Sole queued image/resource lifetime model. The caller holds the common
//! BO owner; submission copies metadata and reserves backing without work,
//! allocation, callbacks, pixel access or waits in this scope.
#![allow(clippy::too_many_arguments)]
use super::{equal::same, error::Error, queue_state as queue};
use crate::{
    contract as a,
    gfx::{
        buffer_owner::{self as bo, Use},
        layout::{self, Descriptor},
        types::{Handle, Owner},
    },
};
use core::{
    ffi::c_void,
    mem::{MaybeUninit, size_of},
    ptr, slice,
};
pub const OWNER: Owner = Owner {
    kind: 0,
    id: 2,
    generation: 1,
};
pub const DISPLAY_OWNER: Owner = Owner {
    kind: 0,
    id: 3,
    generation: 1,
};
pub const COPY: u32 = 0;
pub const BARRIER: u32 = 1;
pub const UPLOAD: u32 = 2;
pub const COPY_ROWS: u32 = 3;
pub const RENDER: u32 = 4;
pub const PRESENT: u32 = 5;
pub const RENDER_LIST: u32 = 6;
pub const DIRECT_PRESENT: u32 = 7;
pub const RENDER_GRID_LIST: u32 = 8;
pub const RENDER_COLOR_LIST: u32 = 9;
pub const NATIVE: u32 = 10;
pub const RENDER_COLOR_GRID_LIST: u32 = 11;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UseSlot {
    pub has: bool,
    pub value: MaybeUninit<Use>,
}
impl UseSlot {
    pub const EMPTY: Self = Self {
        has: false,
        value: MaybeUninit::uninit(),
    };
    pub fn get(&self) -> Option<Use> {
        if self.has {
            Some(unsafe { self.value.assume_init() })
        } else {
            None
        }
    }
    pub fn set(&mut self, value: Use) {
        self.value.write(value);
        self.has = true;
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Request {
    pub operation: u32,
    pub source: Handle,
    pub target: Handle,
    pub source_offset: u64,
    pub target_offset: u64,
    pub bytes: u64,
    pub row_count: u32,
    pub source_pitch: u64,
    pub target_pitch: u64,
    pub render: a::GfxRenderCommand,
    pub render_list: *const a::GfxRenderList,
    pub grid_list: *const a::GfxRenderGridList,
    pub color_list: *const a::GfxRenderColorList,
    pub color_grid_list: *const a::GfxRenderColorGridList,
    pub has_memory: bool,
    pub memory: layout::Binding,
    pub display_target: a::GfxOutputTarget,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            operation: COPY,
            source: Handle::EMPTY,
            target: Handle::EMPTY,
            source_offset: 0,
            target_offset: 0,
            bytes: 0,
            row_count: 0,
            source_pitch: 0,
            target_pitch: 0,
            render: a::GfxRenderCommand::default(),
            render_list: ptr::null(),
            grid_list: ptr::null(),
            color_list: ptr::null(),
            color_grid_list: ptr::null(),
            has_memory: false,
            memory: layout::Binding::default(),
            display_target: a::GfxOutputTarget::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Entry {
    pub fence: queue::Fence,
    pub operation: u32,
    pub uses: [UseSlot; 2],
    pub native_uses: *mut UseSlot,
    pub native_uses_len: usize,
    pub bytes: u64,
    pub copied: u64,
    pub source_offset: u64,
    pub target_offset: u64,
    pub row_bytes: u64,
    pub row_count: u32,
    pub source_pitch: u64,
    pub target_pitch: u64,
    pub render: a::GfxRenderCommand,
    pub deadline_ns: u64,
    pub display_target: a::GfxOutputTarget,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            fence: queue::Fence::default(),
            operation: BARRIER,
            uses: [UseSlot::EMPTY; 2],
            native_uses: ptr::null_mut(),
            native_uses_len: 0,
            bytes: 0,
            copied: 0,
            source_offset: 0,
            target_offset: 0,
            row_bytes: 0,
            row_count: 0,
            source_pitch: 0,
            target_pitch: 0,
            render: a::GfxRenderCommand::default(),
            deadline_ns: 0,
            display_target: a::GfxOutputTarget::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawView {
    pub entries: *mut Entry,
    pub len: usize,
    pub lists: *mut a::GfxRenderList,
    pub grids: *mut [a::GfxSampleGrid; a::gfx_render_list_capacity as usize],
    pub colors: *mut a::GfxRenderColorProgram,
}
pub struct Table<'a> {
    pub entries: &'a mut [Entry],
    pub lists: &'a mut [a::GfxRenderList],
    pub grids: &'a mut [[a::GfxSampleGrid; a::gfx_render_list_capacity as usize]],
    pub colors: &'a mut [a::GfxRenderColorProgram],
    pub seen: &'a mut [bool],
    pub pending: &'a mut [MaybeUninit<u16>],
}
impl RawView {
    /// # Safety
    /// Exact disjoint initialized resident metadata and scratch arrays, admitted
    /// by the shared BO owner. No callback, wait or replacement until dropped.
    pub unsafe fn borrow<'a>(
        self,
        seen: &'a mut [bool],
        pending: &'a mut [MaybeUninit<u16>],
    ) -> Table<'a> {
        unsafe {
            Table {
                entries: slice::from_raw_parts_mut(self.entries, self.len),
                lists: slice::from_raw_parts_mut(self.lists, self.len),
                grids: slice::from_raw_parts_mut(self.grids, self.len),
                colors: slice::from_raw_parts_mut(self.colors, self.len),
                seen,
                pending,
            }
        }
    }
}
pub struct Submission<'a> {
    pub deadline_ns: u64,
    pub frame_key: u64,
    pub dependencies: &'a [queue::Fence],
}
pub fn row_span(bytes: u64, rows: u32, pitch: u64) -> Result<u64, Error> {
    if bytes == 0 || rows == 0 || pitch < bytes {
        return Err(Error::Invalid);
    }
    u64::from(rows - 1)
        .checked_mul(pitch)
        .and_then(|n| n.checked_add(bytes))
        .ok_or(Error::Overflow)
}
// Retain the original checked arithmetic and overflow boundary.
#[allow(clippy::manual_div_ceil)]
fn plane_rows(
    desc: Descriptor,
    offset: u64,
    bytes: u64,
    rows: u32,
    pitch: u64,
) -> Result<(), Error> {
    if desc.format == layout::BYTES {
        return Ok(());
    }
    let multi = desc.format == layout::NV12 || desc.format == layout::P010;
    let sample = match desc.format {
        layout::XRGB8888 | layout::ARGB8888 | layout::XRGB2101010 | layout::ARGB2101010 => 4,
        layout::ABGR16161616F => 8,
        layout::P010 => 2,
        _ => 1,
    };
    for (index, plane) in desc.planes[..desc.plane_count as usize].iter().enumerate() {
        if offset < plane.offset || pitch != plane.pitch {
            continue;
        }
        let relative = offset - plane.offset;
        let x = relative % pitch;
        let y = relative / pitch;
        let columns = if multi && index == 1 {
            (u64::from(desc.width) + 1) / 2 * 2
        } else {
            u64::from(desc.width)
        };
        let height = if multi && index == 1 {
            (u64::from(desc.height) + 1) / 2
        } else {
            u64::from(desc.height)
        };
        if y < height
            && u64::from(rows) <= height - y
            && x < columns * sample
            && bytes <= columns * sample - x
        {
            return Ok(());
        }
    }
    Err(Error::Invalid)
}
/// # Safety
/// Both exact retained CPU-visible queue leases outlive this unlocked bounded
/// copy. Input extents were admitted and source/target objects do not alias.
pub unsafe fn copy_chunk(entry: &Entry, budget: u64, max_spans: u32) -> u64 {
    let Some(source) = entry.uses[0].get() else {
        return 0;
    };
    let Some(target) = entry.uses[1].get() else {
        return 0;
    };
    let remaining = budget.min(entry.bytes - entry.copied);
    let mut done = 0;
    let mut spans = 0;
    while done < remaining && spans < max_spans {
        let cursor = entry.copied + done;
        let rows = entry.operation == COPY_ROWS;
        let contiguous = !rows
            || (entry.source_pitch == entry.row_bytes && entry.target_pitch == entry.row_bytes);
        let y = if rows { cursor / entry.row_bytes } else { 0 };
        let x = if rows {
            cursor % entry.row_bytes
        } else {
            cursor
        };
        let count = if contiguous {
            remaining - done
        } else {
            (remaining - done).min(entry.row_bytes - x)
        };
        let src = source.backing.cpu_address + entry.source_offset + y * entry.source_pitch + x;
        let dst = target.backing.cpu_address + entry.target_offset + y * entry.target_pitch + x;
        unsafe {
            ptr::copy_nonoverlapping(
                ptr::with_exposed_provenance::<u8>(src as usize),
                ptr::with_exposed_provenance_mut::<u8>(dst as usize),
                count as usize,
            )
        };
        done += count;
        spans += 1;
    }
    done
}
fn validate_color_list<T>(
    version: u32,
    bytes: u32,
    reserved: u32,
    count: u32,
    program: &a::GfxRenderColorProgram,
    render: a::GfxRenderCommand,
) -> Result<(), Error> {
    if version != 1
        || bytes != size_of::<T>() as u32
        || reserved != 0
        || count == 0
        || count > a::gfx_render_list_capacity
        || program.version != 1
        || program.size != size_of::<a::GfxRenderColorProgram>() as u32
        || program.reserved0 != 0
        || render.kind != a::gfx_render_kind_sample
        || render.filter != a::gfx_render_filter_nearest
        || render.transfer != a::gfx_render_transfer_color
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn validate_grids(
    grids: &[a::GfxSampleGrid; a::gfx_render_list_capacity as usize],
    count: u32,
    commands: &[a::GfxRenderCommand; a::gfx_render_list_capacity as usize],
    transfer: u32,
) -> Result<(), Error> {
    for (index, value) in grids.iter().enumerate() {
        if index >= count as usize || value.enabled == 0 {
            if !same(value, &a::GfxSampleGrid::default()) {
                return Err(Error::Invalid);
            }
        } else if value.enabled != 1
            || value.reserved != 0
            || value.rotation > 3
            || value.scale < 60
            || value.scale > 960
            || value.pixel_width == 0
            || value.pixel_height == 0
            || value.viewport_width == 0
            || value.viewport_height == 0
            || value.guest_width == 0
            || value.guest_height == 0
            || commands[index].kind != a::gfx_render_kind_sample
            || commands[index].filter != a::gfx_render_filter_nearest
            || commands[index].transfer != transfer
        {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
fn validate_memory(
    request: &Request,
    config: queue::Config,
    descriptor: Descriptor,
) -> Result<(), Error> {
    if !descriptor.binding.portable()
        && (!request.has_memory
            || request.memory.adapter != config.binding.adapter
            || descriptor.binding != request.memory)
    {
        return Err(Error::Stale);
    }
    Ok(())
}
fn rollback(buffers: &mut bo::Table<'_>, entry: &Entry) {
    for held in entry.uses.iter().rev().filter_map(UseSlot::get) {
        buffers
            .end_use(held.lease, OWNER, true)
            .expect("exact resource rollback");
    }
}
impl Table<'_> {
    pub fn ordered(
        &mut self,
        state: &queue::Table<'_>,
        timeline: u64,
        dependencies: &[queue::Fence],
        object: Handle,
        write: bool,
    ) -> Result<(), Error> {
        for prior in self.entries.iter() {
            if prior.fence.slot == 0 {
                continue;
            }
            let status = state.query(prior.fence)?;
            if status.phase == queue::TERMINAL && !status.device_active {
                continue;
            }
            for (j, slot) in prior.uses.iter().enumerate() {
                if let Some(held) = slot.get() {
                    if held.buffer != object || (!write && j == 0) {
                        continue;
                    }
                    if !state.orders(
                        timeline,
                        dependencies,
                        prior.fence,
                        self.seen,
                        self.pending,
                    )? {
                        return Err(Error::Busy);
                    }
                }
            }
            let uses = if prior.native_uses_len == 0 {
                &[][..]
            } else {
                unsafe { slice::from_raw_parts(prior.native_uses, prior.native_uses_len) }
            };
            for slot in uses {
                if let Some(held) = slot.get() {
                    if held.buffer != object || (!write && held.access == 6) {
                        continue;
                    }
                    if !state.orders(
                        timeline,
                        dependencies,
                        prior.fence,
                        self.seen,
                        self.pending,
                    )? {
                        return Err(Error::Busy);
                    }
                }
            }
        }
        Ok(())
    }
    fn install(&mut self, entry: Entry) -> queue::Fence {
        let fence = entry.fence;
        let index = fence.slot as usize - 1;
        assert_eq!(self.entries[index].fence.slot, 0);
        self.entries[index] = entry;
        fence
    }
    fn render_submit(
        &mut self,
        state: &mut queue::Table<'_>,
        buffers: &mut bo::Table<'_>,
        timeline: u64,
        producer: Owner,
        submission: &Submission<'_>,
        request: &Request,
        now: u64,
    ) -> Result<queue::Fence, Error> {
        let config = state.configuration(timeline, producer)?;
        if config.binding.adapter == 0 {
            return Err(Error::Unsupported);
        }
        let draw = request.render;
        if request.bytes != 0
            || request.source_offset != 0
            || request.target_offset != 0
            || draw.reserved0 != 0
            || draw.kind > a::gfx_render_kind_sample
            || draw.filter > a::gfx_render_filter_bilinear
            || draw.blend > a::gfx_render_blend_over
            || draw.transfer
                > if request.operation == RENDER_COLOR_LIST
                    || request.operation == RENDER_COLOR_GRID_LIST
                {
                    a::gfx_render_transfer_color
                } else {
                    a::gfx_render_transfer_srgb_encode
                }
            || draw.opacity > 255
        {
            return Err(Error::Invalid);
        }
        let sampled = draw.kind == a::gfx_render_kind_sample;
        if !sampled
            && (request.source.id != 0
                || request.source.generation != 0
                || draw.filter != 0
                || draw.transfer != 0
                || !same(&draw.source_rect, &a::GfxRenderRect::default()))
        {
            return Err(Error::Invalid);
        }
        if sampled && draw.color != 0 {
            return Err(Error::Invalid);
        }
        if matches!(
            request.operation,
            RENDER_LIST | RENDER_GRID_LIST | RENDER_COLOR_LIST | RENDER_COLOR_GRID_LIST
        ) {
            let list = unsafe { request.render_list.as_ref() }.ok_or(Error::Invalid)?;
            if list.version != 1
                || list.size != size_of::<a::GfxRenderList>() as u32
                || list.count == 0
                || list.count > a::gfx_render_list_capacity
                || list.reserved0 != 0
                || !same(&list.commands[0], &draw)
            {
                return Err(Error::Invalid);
            }
            for (i, item) in list.commands.iter().enumerate() {
                if i >= list.count as usize {
                    if !same(item, &a::GfxRenderCommand::default()) {
                        return Err(Error::Invalid);
                    }
                    continue;
                }
                if item.kind != draw.kind
                    || item.filter != draw.filter
                    || item.blend != draw.blend
                    || item.transfer != draw.transfer
                    || item.opacity > 255
                    || item.reserved0 != 0
                    || (sampled && item.color != 0)
                    || (!sampled && !same(&item.source_rect, &a::GfxRenderRect::default()))
                {
                    return Err(Error::Invalid);
                }
            }
        } else if !request.render_list.is_null() {
            return Err(Error::Invalid);
        }
        let mut entry = Entry {
            operation: request.operation,
            render: draw,
            deadline_ns: submission.deadline_ns,
            ..Entry::default()
        };
        let references = [request.source, request.target];
        let mut objects = [Handle::EMPTY; 2];
        let mut extents = [0; 2];
        for i in (if sampled { 0 } else { 1 })..2 {
            objects[i] = buffers.read_view().buffer_for(references[i], producer)?;
            let descriptor = buffers.read_view().describe(references[i], producer)?;
            validate_memory(request, config, descriptor)?;
            if i == 1 && descriptor.usage & 16 == 0 {
                return Err(Error::Unsupported);
            }
            extents[i] = descriptor.bytes;
            self.ordered(state, timeline, submission.dependencies, objects[i], i == 1)?;
        }
        if sampled && objects[0] == objects[1] {
            return Err(Error::Unsupported);
        }
        let admitted = (|| -> Result<(), Error> {
            if sampled {
                entry.uses[0].set(buffers.reserve_queued(
                    request.source,
                    producer,
                    OWNER,
                    false,
                    0,
                    extents[0],
                )?);
            }
            entry.uses[1].set(buffers.reserve_queued(
                request.target,
                producer,
                OWNER,
                true,
                0,
                extents[1],
            )?);
            entry.fence = state.submit(
                timeline,
                producer,
                submission.deadline_ns,
                submission.frame_key,
                submission.dependencies,
                now,
            )?;
            Ok(())
        })();
        if let Err(error) = admitted {
            rollback(buffers, &entry);
            return Err(error);
        }
        let fence = self.install(entry);
        let index = fence.slot as usize - 1;
        if let Some(list) = unsafe { request.render_list.as_ref() } {
            self.lists[index] = *list;
        }
        if let Some(list) = unsafe { request.grid_list.as_ref() } {
            self.grids[index] = list.grids;
        }
        if let Some(list) = unsafe { request.color_list.as_ref() } {
            self.colors[index] = list.program;
        }
        if let Some(list) = unsafe { request.color_grid_list.as_ref() } {
            self.grids[index] = list.grids;
            self.colors[index] = list.program;
        }
        Ok(fence)
    }
    pub fn submit(
        &mut self,
        state: &mut queue::Table<'_>,
        buffers: &mut bo::Table<'_>,
        timeline: u64,
        producer: Owner,
        submission: &Submission<'_>,
        request: &Request,
        now: u64,
    ) -> Result<queue::Fence, Error> {
        assert!(
            request.operation <= RENDER_COLOR_GRID_LIST,
            "resource operation enum"
        );
        let config = state.configuration(timeline, producer)?;
        if request.operation == RENDER_COLOR_GRID_LIST {
            if request.row_count != 0
                || request.source_pitch != 0
                || request.target_pitch != 0
                || !request.color_list.is_null()
                || !request.grid_list.is_null()
                || !request.render_list.is_null()
            {
                return Err(Error::Invalid);
            }
            let combined = unsafe { request.color_grid_list.as_ref() }.ok_or(Error::Invalid)?;
            validate_color_list::<a::GfxRenderColorGridList>(
                combined.version,
                combined.size,
                combined.reserved0,
                combined.count,
                &combined.program,
                request.render,
            )?;
            validate_grids(
                &combined.grids,
                combined.count,
                &combined.commands,
                a::gfx_render_transfer_color,
            )?;
            let list = a::GfxRenderList {
                count: combined.count,
                commands: combined.commands,
                ..a::GfxRenderList::default()
            };
            let copied = Request {
                render_list: &list,
                ..*request
            };
            return self
                .render_submit(state, buffers, timeline, producer, submission, &copied, now);
        }
        if !request.color_grid_list.is_null() {
            return Err(Error::Invalid);
        }
        if request.operation == RENDER_COLOR_LIST {
            if request.row_count != 0
                || request.source_pitch != 0
                || request.target_pitch != 0
                || !request.grid_list.is_null()
                || !request.render_list.is_null()
            {
                return Err(Error::Invalid);
            }
            let color = unsafe { request.color_list.as_ref() }.ok_or(Error::Invalid)?;
            validate_color_list::<a::GfxRenderColorList>(
                color.version,
                color.size,
                color.reserved0,
                color.count,
                &color.program,
                request.render,
            )?;
            let list = a::GfxRenderList {
                count: color.count,
                commands: color.commands,
                ..a::GfxRenderList::default()
            };
            let copied = Request {
                render_list: &list,
                ..*request
            };
            return self
                .render_submit(state, buffers, timeline, producer, submission, &copied, now);
        }
        if !request.color_list.is_null() {
            return Err(Error::Invalid);
        }
        if request.operation == RENDER_GRID_LIST {
            if request.row_count != 0 || request.source_pitch != 0 || request.target_pitch != 0 {
                return Err(Error::Invalid);
            }
            let grid = unsafe { request.grid_list.as_ref() }.ok_or(Error::Invalid)?;
            if !request.render_list.is_null()
                || grid.version != 1
                || grid.size != size_of::<a::GfxRenderGridList>() as u32
                || grid.count == 0
                || grid.count > a::gfx_render_list_capacity
                || grid.reserved0 != 0
            {
                return Err(Error::Invalid);
            }
            validate_grids(
                &grid.grids,
                grid.count,
                &grid.commands,
                a::gfx_render_transfer_identity,
            )?;
            let list = a::GfxRenderList {
                count: grid.count,
                commands: grid.commands,
                ..a::GfxRenderList::default()
            };
            let copied = Request {
                render_list: &list,
                ..*request
            };
            return self
                .render_submit(state, buffers, timeline, producer, submission, &copied, now);
        }
        if !request.grid_list.is_null()
            || (request.operation != RENDER_LIST && !request.render_list.is_null())
        {
            return Err(Error::Invalid);
        }
        if request.operation == PRESENT || request.operation == DIRECT_PRESENT {
            return self
                .present_submit(state, buffers, timeline, producer, submission, request, now);
        }
        let rows = request.operation == COPY_ROWS;
        if !rows
            && (request.row_count != 0 || request.source_pitch != 0 || request.target_pitch != 0)
        {
            return Err(Error::Invalid);
        }
        if request.operation == RENDER || request.operation == RENDER_LIST {
            return self
                .render_submit(state, buffers, timeline, producer, submission, request, now);
        }
        if !same(&request.render, &a::GfxRenderCommand::default()) {
            return Err(Error::Invalid);
        }
        let source_span = if rows {
            row_span(request.bytes, request.row_count, request.source_pitch)?
        } else {
            request.bytes
        };
        let target_span = if rows {
            row_span(request.bytes, request.row_count, request.target_pitch)?
        } else {
            request.bytes
        };
        let mut entry = Entry {
            operation: request.operation,
            deadline_ns: submission.deadline_ns,
            bytes: if rows {
                request
                    .bytes
                    .checked_mul(u64::from(request.row_count))
                    .ok_or(Error::Overflow)?
            } else {
                request.bytes
            },
            source_offset: request.source_offset,
            target_offset: request.target_offset,
            row_bytes: if rows { request.bytes } else { 0 },
            row_count: request.row_count,
            source_pitch: request.source_pitch,
            target_pitch: request.target_pitch,
            ..Entry::default()
        };
        if matches!(request.operation, COPY | UPLOAD | COPY_ROWS) {
            if request.bytes == 0 {
                return Err(Error::Invalid);
            }
            let source = buffers.read_view().buffer_for(request.source, producer)?;
            let upload = request.operation == UPLOAD;
            if upload
                && (config.binding.adapter == 0
                    || request.target.id != 0
                    || request.target.generation != 0
                    || request.target_offset != 0)
            {
                return Err(Error::Unsupported);
            }
            let target = if upload {
                Handle::EMPTY
            } else {
                buffers.read_view().buffer_for(request.target, producer)?
            };
            if source == target {
                return Err(Error::Unsupported);
            }
            let references = [request.source, request.target];
            let objects = [source, target];
            let mut offsets = [request.source_offset, request.target_offset];
            let mut extents = [source_span, target_span];
            for i in 0..if upload { 1 } else { 2 } {
                let descriptor = buffers.read_view().describe(references[i], producer)?;
                validate_memory(request, config, descriptor)?;
                if !layout::span_fits(descriptor.bytes, offsets[i], extents[i]) {
                    return Err(Error::Invalid);
                }
                if rows {
                    plane_rows(
                        descriptor,
                        offsets[i],
                        request.bytes,
                        request.row_count,
                        if i == 0 {
                            request.source_pitch
                        } else {
                            request.target_pitch
                        },
                    )?;
                }
                if descriptor.modifier != 0 {
                    if !rows || config.binding.adapter == 0 {
                        return Err(Error::Unsupported);
                    }
                    offsets[i] = 0;
                    extents[i] = descriptor.bytes;
                }
                self.ordered(state, timeline, submission.dependencies, objects[i], i == 1)?;
            }
            let result = (|| -> Result<(), Error> {
                entry.uses[0].set(buffers.reserve_queued(
                    request.source,
                    producer,
                    OWNER,
                    false,
                    offsets[0],
                    extents[0],
                )?);
                if !upload {
                    entry.uses[1].set(buffers.reserve_queued(
                        request.target,
                        producer,
                        OWNER,
                        true,
                        offsets[1],
                        extents[1],
                    )?);
                }
                if config.binding.adapter == 0 {
                    for slot in &entry.uses {
                        let held = slot.get().expect("software copy endpoints");
                        if held.backing.cpu_address == 0 || held.backing.cache != 1 {
                            return Err(Error::Unsupported);
                        }
                    }
                }
                entry.fence = state.submit(
                    timeline,
                    producer,
                    submission.deadline_ns,
                    submission.frame_key,
                    submission.dependencies,
                    now,
                )?;
                Ok(())
            })();
            if let Err(error) = result {
                rollback(buffers, &entry);
                return Err(error);
            }
            return Ok(self.install(entry));
        }
        if request.source.id != 0
            || request.source.generation != 0
            || request.target.id != 0
            || request.target.generation != 0
            || request.bytes != 0
            || request.source_offset != 0
            || request.target_offset != 0
        {
            return Err(Error::Invalid);
        }
        entry.fence = state.submit(
            timeline,
            producer,
            submission.deadline_ns,
            submission.frame_key,
            submission.dependencies,
            now,
        )?;
        Ok(self.install(entry))
    }
    fn present_submit(
        &mut self,
        state: &mut queue::Table<'_>,
        buffers: &mut bo::Table<'_>,
        timeline: u64,
        producer: Owner,
        submission: &Submission<'_>,
        request: &Request,
        now: u64,
    ) -> Result<queue::Fence, Error> {
        let config = state.configuration(timeline, producer)?;
        if config.binding.adapter == 0
            || request.source_offset != 0
            || request.target_offset != 0
            || request.target.id != 0
            || request.target.generation != 0
            || request.target_pitch != 0
            || !same(&request.render, &a::GfxRenderCommand::default())
        {
            return Err(Error::Invalid);
        }
        let descriptor = buffers.read_view().describe(request.source, producer)?;
        if request.operation == DIRECT_PRESENT
            && (descriptor.location != 1
                || descriptor.modifier != 0
                || descriptor.usage & 32 == 0
                || descriptor.planes[0].pitch & 63 != 0)
        {
            return Err(Error::Unsupported);
        }
        if (descriptor.format != layout::XRGB8888 && descriptor.format != layout::XRGB2101010)
            || descriptor.plane_count != 1
            || descriptor.planes[0].offset != 0
            || descriptor.usage & 4 == 0
            || request.bytes != u64::from(descriptor.width) * 4
            || request.row_count != descriptor.height
            || request.source_pitch != descriptor.planes[0].pitch
            || row_span(request.bytes, request.row_count, request.source_pitch)? > descriptor.bytes
        {
            return Err(Error::Unsupported);
        }
        validate_memory(request, config, descriptor)?;
        let object = buffers.read_view().buffer_for(request.source, producer)?;
        self.ordered(state, timeline, submission.dependencies, object, false)?;
        let held =
            buffers.reserve_queued(request.source, producer, OWNER, false, 0, descriptor.bytes)?;
        let fence = match state.submit(
            timeline,
            producer,
            submission.deadline_ns,
            submission.frame_key,
            submission.dependencies,
            now,
        ) {
            Ok(value) => value,
            Err(error) => {
                buffers
                    .end_use(held.lease, OWNER, true)
                    .expect("exact present rollback");
                return Err(error);
            }
        };
        let mut entry = Entry {
            fence,
            operation: request.operation,
            bytes: request
                .bytes
                .checked_mul(u64::from(request.row_count))
                .expect("admitted present geometry"),
            row_bytes: request.bytes,
            row_count: request.row_count,
            source_pitch: request.source_pitch,
            deadline_ns: submission.deadline_ns,
            display_target: request.display_target,
            ..Entry::default()
        };
        entry.uses[0].set(held);
        Ok(self.install(entry))
    }
    pub fn render_list(
        &self,
        state: &queue::Table<'_>,
        fence: queue::Fence,
    ) -> Result<a::GfxRenderList, Error> {
        if !state.query(fence)?.device_active {
            return Err(Error::Invalid);
        }
        let index = fence.slot as usize - 1;
        let entry = &self.entries[index];
        if entry.fence != fence || entry.operation != RENDER_LIST {
            return Err(Error::Invalid);
        }
        Ok(self.lists[index])
    }
    pub fn render_grid_list(
        &self,
        state: &queue::Table<'_>,
        fence: queue::Fence,
    ) -> Result<a::GfxRenderGridList, Error> {
        if !state.query(fence)?.device_active {
            return Err(Error::Invalid);
        }
        let index = fence.slot as usize - 1;
        let entry = &self.entries[index];
        if entry.fence != fence
            || (entry.operation != RENDER_GRID_LIST && entry.operation != RENDER_COLOR_GRID_LIST)
        {
            return Err(Error::Invalid);
        }
        let list = &self.lists[index];
        Ok(a::GfxRenderGridList {
            count: list.count,
            commands: list.commands,
            grids: self.grids[index],
            ..a::GfxRenderGridList::default()
        })
    }
    pub fn render_color_list(
        &self,
        state: &queue::Table<'_>,
        fence: queue::Fence,
    ) -> Result<a::GfxRenderColorList, Error> {
        if !state.query(fence)?.device_active {
            return Err(Error::Invalid);
        }
        let index = fence.slot as usize - 1;
        let entry = &self.entries[index];
        if entry.fence != fence
            || (entry.operation != RENDER_COLOR_LIST && entry.operation != RENDER_COLOR_GRID_LIST)
        {
            return Err(Error::Invalid);
        }
        let list = &self.lists[index];
        Ok(a::GfxRenderColorList {
            count: list.count,
            commands: list.commands,
            program: self.colors[index],
            ..a::GfxRenderColorList::default()
        })
    }
    pub fn retain(
        &self,
        state: &queue::Table<'_>,
        buffers: &mut bo::Table<'_>,
        fence: queue::Fence,
        which: u32,
        driver: Owner,
    ) -> Result<Handle, Error> {
        if which > 1 {
            return Err(Error::Invalid);
        }
        let status = state.query(fence)?;
        if !status.device_active {
            return Err(if status.phase == queue::QUEUED {
                Error::Busy
            } else {
                Error::AlreadyCompleted
            });
        }
        let entry = &self.entries[fence.slot as usize - 1];
        if entry.fence != fence {
            return Err(Error::Stale);
        }
        let held = entry.uses[which as usize].get().ok_or(Error::Invalid)?;
        Ok(buffers.retain_queued(held.lease, OWNER, driver)?)
    }
    pub fn retain_scanout(
        &self,
        state: &queue::Table<'_>,
        buffers: &mut bo::Table<'_>,
        fence: queue::Fence,
        driver: Owner,
    ) -> Result<Handle, Error> {
        if !state.query(fence)?.device_active {
            return Err(Error::Invalid);
        }
        let entry = &self.entries[fence.slot as usize - 1];
        if entry.fence != fence || entry.operation != DIRECT_PRESENT {
            return Err(Error::Invalid);
        }
        let held = entry.uses[0].get().ok_or(Error::Invalid)?;
        Ok(buffers.retain_scanout(held.lease, OWNER, driver)?)
    }
    pub fn release(
        &mut self,
        buffers: &mut bo::Table<'_>,
        ticket: queue::Release,
    ) -> Result<(), Error> {
        if ticket.fence.slot == 0 || ticket.fence.slot as usize > self.entries.len() {
            return Err(Error::Invalid);
        }
        let entry = &mut self.entries[ticket.fence.slot as usize - 1];
        if entry.fence != ticket.fence {
            return Err(Error::Stale);
        }
        for slot in &mut entry.uses {
            if let Some(held) = slot.get() {
                buffers.end_use(held.lease, OWNER, true)?;
                slot.has = false;
            }
        }
        if entry.native_uses_len != 0 {
            for slot in
                unsafe { slice::from_raw_parts_mut(entry.native_uses, entry.native_uses_len) }
            {
                if let Some(held) = slot.get() {
                    buffers.end_use(held.lease, OWNER, true)?;
                    slot.has = false;
                }
            }
        }
        *entry = Entry::default();
        Ok(())
    }
}
#[repr(C)]
pub struct Args {
    pub owner: Owner,
    pub timeline: u64,
    pub now: u64,
    pub deadline: u64,
    pub frame_key: u64,
    pub fence: queue::Fence,
    pub release: queue::Release,
    pub object: Handle,
    pub which: u32,
    pub write: bool,
    pub dependencies: *const queue::Fence,
    pub dependencies_len: usize,
    pub request: *const Request,
    pub seen: *mut bool,
    pub pending: *mut MaybeUninit<u16>,
}
/// # Safety
/// Exact disjoint C views are admitted by the same BO owner. Null BO view
/// selects its sole native resident table; null queue is used only by release.
/// Scratch arrays cover all queue jobs and selected outputs have full capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_queue_resources(
    op: u32,
    view: *const RawView,
    queue_view: *const queue::RawView,
    bo_view: *const bo::RawView,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { &*args };
    let mut state = if queue_view.is_null() {
        None
    } else {
        Some(unsafe { queue_view.read().borrow() })
    };
    let count = state.as_ref().map_or(0, |q| q.jobs.len());
    let seen = if count == 0 {
        &mut [][..]
    } else {
        unsafe { slice::from_raw_parts_mut(args.seen, count) }
    };
    let pending = if count == 0 {
        &mut [][..]
    } else {
        unsafe { slice::from_raw_parts_mut(args.pending, count) }
    };
    let mut resources = unsafe { view.read().borrow(seen, pending) };
    let mut buffers = if op == 0 || (2..=4).contains(&op) {
        None
    } else if bo_view.is_null() {
        Some(unsafe { crate::gfx::buffers::table() })
    } else {
        Some(unsafe { bo_view.read().borrow() })
    };
    let dependencies = if args.dependencies_len == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(args.dependencies, args.dependencies_len) }
    };
    let submission = Submission {
        deadline_ns: args.deadline,
        frame_key: args.frame_key,
        dependencies,
    };
    let result = match op {
        0 => resources.ordered(
            state.as_ref().expect("queue state"),
            args.timeline,
            dependencies,
            args.object,
            args.write,
        ),
        1 => resources
            .submit(
                state.as_mut().expect("queue state"),
                buffers.as_mut().expect("BO state"),
                args.timeline,
                args.owner,
                &submission,
                unsafe { &*args.request },
                args.now,
            )
            .map(|v| unsafe { out.cast::<queue::Fence>().write(v) }),
        2 => resources
            .render_list(state.as_ref().expect("queue state"), args.fence)
            .map(|v| unsafe { out.cast::<a::GfxRenderList>().write(v) }),
        3 => resources
            .render_grid_list(state.as_ref().expect("queue state"), args.fence)
            .map(|v| unsafe { out.cast::<a::GfxRenderGridList>().write(v) }),
        4 => resources
            .render_color_list(state.as_ref().expect("queue state"), args.fence)
            .map(|v| unsafe { out.cast::<a::GfxRenderColorList>().write(v) }),
        5 => resources
            .retain(
                state.as_ref().expect("queue state"),
                buffers.as_mut().expect("BO state"),
                args.fence,
                args.which,
                args.owner,
            )
            .map(|v| unsafe { out.cast::<Handle>().write(v) }),
        6 => resources
            .retain_scanout(
                state.as_ref().expect("queue state"),
                buffers.as_mut().expect("BO state"),
                args.fence,
                args.owner,
            )
            .map(|v| unsafe { out.cast::<Handle>().write(v) }),
        7 => resources.release(buffers.as_mut().expect("BO state"), args.release),
        _ => panic!("queue resource operation"),
    };
    match result {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
/// # Safety
/// Exclusive scalar output, with the same original row-span inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_row_span(
    bytes: u64,
    rows: u32,
    pitch: u64,
    out: *mut u64,
) -> i32 {
    match row_span(bytes, rows, pitch) {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}
/// # Safety
/// Exact copied entry with retained disjoint admitted queue leases through the
/// bounded unlocked CPU copy, as for copy_chunk().
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_copy_chunk(
    entry: *const Entry,
    budget: u64,
    spans: u32,
) -> u64 {
    unsafe { copy_chunk(&*entry, budget, spans) }
}
const _: () = {
    assert!(size_of::<UseSlot>() == 120);
};

/// # Safety
/// Exact initialized admitted descriptor; row extents match the original model.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_plane_rows(
    descriptor: *const Descriptor,
    offset: u64,
    bytes: u64,
    rows: u32,
    pitch: u64,
) -> i32 {
    match plane_rows(unsafe { descriptor.read() }, offset, bytes, rows, pitch) {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
