//! Bounded copied presentation facts. The enclosing display state owner owns
//! admission; this model never waits, calls a driver or touches BO storage.
use super::{backend_state as backend, equal::same, error::Error};
use crate::contract as a;
use core::mem::{MaybeUninit, size_of};
pub const CAPACITY: usize = a::gfx_output_max_assignments as usize;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Slot<T: Copy> {
    pub has: bool,
    pub value: MaybeUninit<T>,
}
impl<T: Copy> Slot<T> {
    pub const EMPTY: Self = Self {
        has: false,
        value: MaybeUninit::uninit(),
    };
    pub fn get(&self) -> Option<T> {
        if self.has {
            Some(unsafe { self.value.assume_init() })
        } else {
            None
        }
    }
    pub fn set(&mut self, value: T) {
        self.value.write(value);
        self.has = true
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Owner {
    pub driver: usize,
    pub driver_generation: u64,
    pub display_generation: u64,
    pub backend: a::GfxBackendBinding,
    pub heads: [Slot<a::DisplayPresentationStats>; CAPACITY],
    pub infos: [Slot<a::DisplayPresentationInfo>; CAPACITY],
    pub history: [Slot<a::DisplayPresentationStats>; 32],
    pub history_cursor: usize,
}
impl Default for Owner {
    fn default() -> Self {
        Self {
            driver: 0,
            driver_generation: 0,
            display_generation: 0,
            backend: a::GfxBackendBinding::default(),
            heads: [Slot::EMPTY; CAPACITY],
            infos: [Slot::EMPTY; CAPACITY],
            history: [Slot::EMPTY; 32],
            history_cursor: 0,
        }
    }
}
impl Owner {
    /// # Safety
    /// Exact resident unpublished metadata, or its exclusive display owner.
    /// Hidden payloads stay inside MaybeUninit until their presence is published.
    pub unsafe fn init_in_place(out: *mut Self) {
        use core::ptr;
        unsafe {
            ptr::addr_of_mut!((*out).driver).write(0);
            ptr::addr_of_mut!((*out).driver_generation).write(0);
            ptr::addr_of_mut!((*out).display_generation).write(0);
            ptr::addr_of_mut!((*out).backend).write(a::GfxBackendBinding::default());
            let heads = ptr::addr_of_mut!((*out).heads).cast::<Slot<a::DisplayPresentationStats>>();
            let infos = ptr::addr_of_mut!((*out).infos).cast::<Slot<a::DisplayPresentationInfo>>();
            let history =
                ptr::addr_of_mut!((*out).history).cast::<Slot<a::DisplayPresentationStats>>();
            for i in 0..CAPACITY {
                heads.add(i).write(Slot::EMPTY);
                infos.add(i).write(Slot::EMPTY);
            }
            for i in 0..32 {
                history.add(i).write(Slot::EMPTY);
            }
            ptr::addr_of_mut!((*out).history_cursor).write(0);
        }
    }
    pub fn bind(
        &mut self,
        driver: usize,
        driver_generation: u64,
        binding: a::GfxBackendBinding,
        generation: u64,
    ) -> Result<(), Error> {
        if driver == 0
            || driver_generation == 0
            || generation == 0
            || binding.version != 1
            || (binding.size as usize) < size_of::<a::GfxBackendBinding>()
            || binding.adapter_id == 0
            || binding.device_generation == 0
            || binding.reset_generation == 0
            || binding.milestone != a::gfx_queue_milestone_device_execution
        {
            return Err(Error::Invalid);
        }
        self.driver = driver;
        self.driver_generation = driver_generation;
        self.display_generation = generation;
        self.backend = binding;
        // Hidden optional payloads are uninitialized C storage, not borrowed
        // Contract values. Clearing presence avoids a large temporary stack copy.
        for slot in &mut self.heads {
            slot.has = false
        }
        for slot in &mut self.infos {
            slot.has = false
        }
        for slot in &mut self.history {
            slot.has = false
        }
        self.history_cursor = 0;
        Ok(())
    }
    fn current(&self, state: backend::Snapshot) -> bool {
        self.driver != 0
            && state.owner == self.driver
            && state.generation == self.display_generation
            && state.adapter_id == self.backend.adapter_id
            && matches!(
                state.state,
                backend::SOFTWARE_NATIVE | backend::NATIVE | backend::RECOVERING
            )
    }
    pub fn publish_info(
        &mut self,
        driver: usize,
        generation: u64,
        supplied: a::DisplayPresentationInfo,
        state: backend::Snapshot,
    ) -> Result<(), Error> {
        if supplied.version != 1
            || (supplied.size as usize) < size_of::<a::DisplayPresentationInfo>()
        {
            return Err(Error::Invalid);
        }
        let mut input = supplied;
        input.size = size_of::<a::DisplayPresentationInfo>() as u32;
        if driver != self.driver
            || generation != self.driver_generation
            || !self.current(state)
            || input.display_generation != self.display_generation
            || !same(&input.backend, &self.backend)
        {
            return Err(Error::Stale);
        }
        let native = a::display_presentation_info_native;
        let sync = a::display_presentation_info_synchronized;
        let visible = a::display_presentation_info_visibility;
        if input.sequence == 0
            || input.head_id >= 8
            || input.width == 0
            || input.height == 0
            || (input.format != a::gfx_buffer_format_xrgb8888
                && input.format != a::gfx_buffer_format_xrgb2101010)
            || input.flags & !511 != 0
            || input.flags & native == 0
            || input.buffer_count < 2
            || input.buffer_count > 3
            || input.plane_count == 0
            || input.plane_count > 8
            || input.policies & !7 != 0
            || input.policies & 1 == 0
            || input.reserved0 != 0
            || input.path > 3
            || input.interval_ns > 1_000_000_000
            || (input.observed_ns == 0) != (input.observed_sequence == 0)
            || (input.flags & sync != 0 && input.flags & visible == 0)
            || (input.flags
                & (a::display_presentation_info_direct | a::display_presentation_info_overlay)
                != 0
                && input.flags & visible == 0)
            || (input.path == 2 && input.flags & a::display_presentation_info_direct == 0)
            || (input.path == 3 && input.flags & a::display_presentation_info_overlay == 0)
        {
            return Err(Error::Invalid);
        }
        let mut free = None;
        for (index, slot) in self.infos.iter_mut().enumerate() {
            let Some(previous) = slot.get() else {
                if free.is_none() {
                    free = Some(index)
                }
                continue;
            };
            if previous.head_id != input.head_id {
                continue;
            }
            if same(&previous, &input) {
                return Ok(());
            }
            if input.sequence <= previous.sequence
                || input.observed_sequence < previous.observed_sequence
                || input.observed_ns < previous.observed_ns
                || (previous.flags & a::display_presentation_info_lost != 0
                    && input.flags & a::display_presentation_info_lost == 0)
            {
                return Err(Error::Stale);
            }
            slot.set(input);
            return Ok(());
        }
        self.infos[free.ok_or(Error::Capacity)?].set(input);
        Ok(())
    }
    // Original receipt/validation branch scopes retained during the port.
    #[allow(clippy::collapsible_if)]
    pub fn read_info(
        &self,
        head: u32,
        state: backend::Snapshot,
    ) -> Result<a::DisplayPresentationInfo, Error> {
        if !self.current(state) {
            return Err(Error::Unsupported);
        }
        for slot in &self.infos {
            if let Some(mut value) = slot.get() {
                if value.head_id == head {
                    if state.state == backend::RECOVERING {
                        value.flags |= a::display_presentation_info_lost
                    }
                    return Ok(value);
                }
            }
        }
        Err(Error::Unsupported)
    }
    pub fn publish(
        &mut self,
        driver: usize,
        generation: u64,
        supplied: a::DisplayPresentationStats,
        state: backend::Snapshot,
    ) -> Result<(), Error> {
        if supplied.version != 1
            || (supplied.size as usize) < size_of::<a::DisplayPresentationStats>()
        {
            return Err(Error::Invalid);
        }
        let mut input = supplied;
        input.size = size_of::<a::DisplayPresentationStats>() as u32;
        if driver != self.driver
            || generation != self.driver_generation
            || !self.current(state)
            || input.display_generation != self.display_generation
            || !same(&input.backend, &self.backend)
        {
            return Err(Error::Stale);
        }
        if input.version != 1
            || (input.size as usize) < size_of::<a::DisplayPresentationStats>()
            || input.sequence == 0
            || input.flags & a::display_presentation_flag_available == 0
            || input.flags & !15 != 0
            || input.pending & !15 != 0
            || input.buffer_count == 0
            || input.rendered_count > input.acquired_count
            || input.visible_count > input.submitted_count
            || input.released_count > input.visible_count
            || input.visible_count - input.released_count > 1
            || input.visible_sequence > input.submitted_count
            || (input.source_timeline == 0) != (input.source_point == 0)
        {
            return Err(Error::Invalid);
        }
        let polled = input.flags & a::display_presentation_flag_polled != 0;
        let direct = input.flags & a::display_presentation_flag_direct != 0;
        if polled
            && (direct
                || input.render_point != 0
                || input.window_point != 0
                || input.gpu_timestamp != 0
                || input.irq_sequence != 0
                || input.irq_observed_ns != 0)
        {
            return Err(Error::Invalid);
        }
        if input.visible_count == 0 {
            if input.visible_sequence != 0
                || input.source_timeline != 0
                || input.source_point != 0
                || input.render_point != 0
                || input.window_point != 0
                || input.submitted_ns != 0
                || input.visible_ns != 0
                || input.gpu_timestamp != 0
                || input.irq_sequence != 0
                || input.irq_observed_ns != 0
                || input.released_ns != 0
            {
                return Err(Error::Invalid);
            }
        } else {
            if input.visible_sequence == 0
                || input.submitted_ns == 0
                || input.visible_ns < input.submitted_ns
                || (input.released_ns != 0 && input.released_ns < input.visible_ns)
                || (input.released_ns == 0) != (input.released_count < input.visible_count)
            {
                return Err(Error::Invalid);
            }
            if polled {
                if input.source_timeline == 0 {
                    return Err(Error::Invalid);
                }
            } else if (input.render_point == 0 && !direct)
                || (direct && input.source_timeline == 0)
                || input.window_point == 0
                || input.irq_sequence == 0
                || input.irq_observed_ns < input.submitted_ns
                || input.irq_observed_ns > input.visible_ns
            {
                return Err(Error::Invalid);
            }
        }
        let mut free = None;
        for (index, slot) in self.heads.iter_mut().enumerate() {
            let Some(previous) = slot.get() else {
                if free.is_none() {
                    free = Some(index)
                }
                continue;
            };
            if previous.head_id != input.head_id {
                continue;
            }
            if input.sequence == previous.sequence && same(&input, &previous) {
                return Ok(());
            }
            if input.sequence <= previous.sequence
                || input.acquired_count < previous.acquired_count
                || input.rendered_count < previous.rendered_count
                || input.submitted_count < previous.submitted_count
                || input.visible_count < previous.visible_count
                || input.released_count < previous.released_count
                || input.rejected_count < previous.rejected_count
                || input.visible_sequence < previous.visible_sequence
                || input.visible_ns < previous.visible_ns
                || input.irq_sequence < previous.irq_sequence
                || (previous.flags & a::display_presentation_flag_lost != 0
                    && input.flags & a::display_presentation_flag_lost == 0)
            {
                return Err(Error::Stale);
            }
            slot.set(input);
            self.remember(input);
            return Ok(());
        }
        self.heads[free.ok_or(Error::Capacity)?].set(input);
        self.remember(input);
        Ok(())
    }
    // Original receipt/validation branch scopes retained during the port.
    #[allow(clippy::collapsible_if)]
    fn remember(&mut self, value: a::DisplayPresentationStats) {
        if value.visible_ns == 0 || value.source_timeline == 0 {
            return;
        }
        for slot in &mut self.history {
            if let Some(prior) = slot.get() {
                if prior.head_id == value.head_id
                    && prior.source_timeline == value.source_timeline
                    && prior.source_point == value.source_point
                {
                    slot.set(value);
                    return;
                }
            }
        }
        self.history[self.history_cursor].set(value);
        self.history_cursor = (self.history_cursor + 1) % self.history.len();
    }
    // Original receipt/validation branch scopes retained during the port.
    #[allow(clippy::collapsible_if)]
    pub fn feedback(
        &self,
        head: u32,
        source: a::GfxFence,
        state: backend::Snapshot,
    ) -> Result<a::DisplayPresentationStats, Error> {
        if source.timeline == 0 || source.point == 0 {
            return Err(Error::Invalid);
        }
        if !self.current(state)
            || source.adapter_id != self.backend.adapter_id
            || source.device_generation != self.backend.device_generation
            || source.reset_generation != self.backend.reset_generation
        {
            return Err(Error::Stale);
        }
        for slot in &self.history {
            if let Some(mut value) = slot.get() {
                if value.head_id == head
                    && value.source_timeline == source.timeline
                    && value.source_point == source.point
                {
                    if state.state == backend::RECOVERING {
                        value.flags |= a::display_presentation_flag_lost
                    }
                    return Ok(value);
                }
            }
        }
        Err(Error::Unsupported)
    }
    // Original receipt/validation branch scopes retained during the port.
    #[allow(clippy::collapsible_if)]
    pub fn read(
        &self,
        head: u32,
        state: backend::Snapshot,
    ) -> Result<a::DisplayPresentationStats, Error> {
        if !self.current(state) {
            return Err(Error::Unsupported);
        }
        for slot in &self.heads {
            if let Some(mut value) = slot.get() {
                if value.head_id == head {
                    if state.state == backend::RECOVERING {
                        value.flags |= a::display_presentation_flag_lost
                    }
                    return Ok(value);
                }
            }
        }
        Err(Error::Unsupported)
    }
}
const _: () = {
    use core::mem::{align_of, offset_of};
    assert!(
        size_of::<Slot<a::DisplayPresentationStats>>() == 216
            && size_of::<Slot<a::DisplayPresentationInfo>>() == 128
    );
    assert!(
        size_of::<Owner>() == 9728
            && align_of::<Owner>() == 8
            && offset_of!(Owner, heads) == 56
            && offset_of!(Owner, infos) == 1784
            && offset_of!(Owner, history) == 2808
            && offset_of!(Owner, history_cursor) == 9720
    );
};

/// # Safety
/// Exact initialized fixed C metadata under the display owner; inputs readable and output exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_stats_bind(
    owner: *mut Owner,
    driver: usize,
    driver_generation: u64,
    binding: *const a::GfxBackendBinding,
    generation: u64,
) -> i32 {
    match unsafe { (&mut *owner).bind(driver, driver_generation, binding.read(), generation) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized fixed C metadata under the display owner; inputs readable and output exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_stats_publish_info(
    owner: *mut Owner,
    driver: usize,
    generation: u64,
    input: *const a::DisplayPresentationInfo,
    state: *const backend::Snapshot,
) -> i32 {
    match unsafe { (&mut *owner).publish_info(driver, generation, input.read(), state.read()) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized fixed C metadata under the display owner; inputs readable and output exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_stats_read_info(
    owner: *const Owner,
    head: u32,
    state: *const backend::Snapshot,
    out: *mut a::DisplayPresentationInfo,
) -> i32 {
    match unsafe { (&*owner).read_info(head, state.read()) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized fixed C metadata under the display owner; inputs readable and output exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_stats_publish(
    owner: *mut Owner,
    driver: usize,
    generation: u64,
    input: *const a::DisplayPresentationStats,
    state: *const backend::Snapshot,
) -> i32 {
    match unsafe { (&mut *owner).publish(driver, generation, input.read(), state.read()) } {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized fixed C metadata under the display owner; inputs readable and output exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_stats_read(
    owner: *const Owner,
    head: u32,
    state: *const backend::Snapshot,
    out: *mut a::DisplayPresentationStats,
) -> i32 {
    match unsafe { (&*owner).read(head, state.read()) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized fixed C metadata under the display owner; inputs readable and output exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_stats_feedback(
    owner: *const Owner,
    head: u32,
    source: *const a::GfxFence,
    state: *const backend::Snapshot,
    out: *mut a::DisplayPresentationStats,
) -> i32 {
    match unsafe { (&*owner).feedback(head, source.read(), state.read()) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}
