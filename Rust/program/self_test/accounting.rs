//! Explicit PROGRAMSTORAGE oracle. Recount physical nodes and every immutable
//! base independently of the production caches; never called by normal draws.
use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Counts {
    pub bytes: u64,
    pub commands: u64,
    resources: u64,
    rasters: u64,
}
impl Counts {
    fn add(self, other: Self) -> Option<Self> {
        Some(Self {
            bytes: self.bytes.checked_add(other.bytes)?,
            commands: self.commands.checked_add(other.commands)?,
            resources: self.resources.checked_add(other.resources)?,
            rasters: self.rasters.checked_add(other.rasters)?,
        })
    }
}
unsafe fn chain(frame: *const t::Frame) -> Option<Counts> {
    unsafe {
        let mut nodes = [ptr::null(); c::gui_frame_max_delta_chain as usize];
        let mut count = 0;
        let mut cursor = frame;
        while !cursor.is_null() {
            if count == nodes.len() {
                return None;
            }
            nodes[count] = cursor;
            count += 1;
            cursor = (*cursor).base_frame;
        }
        let mut base = Counts::default();
        for &frame in nodes[..count].iter().rev() {
            let mut local = Counts {
                bytes: (*frame).header.requested_bytes.into(),
                ..Counts::default()
            };
            let mut command = (*frame).command_payload;
            while !command.is_null() {
                local.bytes = local
                    .bytes
                    .checked_add((*command).header.requested_bytes.into())?;
                local.commands = local
                    .commands
                    .checked_add((*command).command_count.into())?;
                command = (*command).next;
            }
            let mut resource = (*frame).resource_payload;
            while !resource.is_null() {
                local.bytes = local
                    .bytes
                    .checked_add((*resource).header.requested_bytes.into())?;
                local.resources = local.resources.checked_add((*resource).byte_count.into())?;
                resource = (*resource).next;
            }
            // Raster semantics are separately checked by the original command
            // owner cases. Here the old chain-count oracle sums local words.
            local.rasters = (*frame).raster_words;
            if local.bytes != (*frame).local_bytes
                || local.commands != (*frame).command_count
                || local.resources != (*frame).resource_len
                || base.bytes != (*frame).base_bytes
                || base.commands != (*frame).base_commands
                || base.resources != (*frame).base_resources
                || base.rasters != (*frame).base_raster_words
            {
                return None;
            }
            base = base.add(local)?;
            if life::frame_bytes(frame) != base.bytes
                || life::chain_count(frame, 0) != base.commands
                || life::chain_count(frame, 1) != base.resources
                || life::chain_count(frame, 2) != base.rasters
            {
                return None;
            }
        }
        Some(base)
    }
}
pub(super) unsafe fn checked(gui: *const t::Gui) -> Option<Counts> {
    unsafe {
        let mut value = chain((*gui).committed_frame)?.add(chain((*gui).building_frame)?)?;
        let mut retired = (*gui).retired_frames;
        while !retired.is_null() {
            value = value.add(chain(retired)?)?;
            retired = (*retired).retired_next;
        }
        if value.bytes != life::owned(gui, false)
            || value.commands != life::owned(gui, true)
            || (*gui).frame_peak_bytes < value.bytes
            || (*gui).frame_peak_commands < value.commands
        {
            return None;
        }
        Some(value)
    }
}
pub(super) unsafe fn peak(gui: *const t::Gui) -> [u64; 2] {
    unsafe { [(*gui).frame_peak_bytes, (*gui).frame_peak_commands] }
}
pub(super) unsafe fn after_append(gui: *const t::Gui, before: [u64; 2]) -> bool {
    unsafe {
        checked(gui).is_some_and(|value| {
            peak(gui) == [before[0].max(value.bytes), before[1].max(value.commands)]
        })
    }
}
pub(super) unsafe fn at_peak(gui: *const t::Gui) -> bool {
    unsafe { checked(gui).is_some_and(|value| peak(gui) == [value.bytes, value.commands]) }
}
