//! Additional native output metadata. R4Ds own scanout storage/hardware and
//! the desktop owns topology policy; this catalog never releases a BO.
use super::{
    backend_state as backend, equal::same, error::Error, output_target,
    presentation_stats as stats, queue_state::Fence,
};
use crate::{contract as a, gfx::types::Owner};
use core::{mem::size_of, ptr};
pub const CAPACITY: usize = a::gfx_output_max_assignments as usize;
#[repr(C)]
pub struct Entry {
    pub driver: Owner,
    pub target: a::GfxOutputTarget,
    pub binding: a::GfxBackendBinding,
    pub width: u32,
    pub height: u32,
    pub active: bool,
    pub mode_blocked: bool,
    pub mode_lost: bool,
    pub removing: bool,
    pub has_pending: bool,
    pub pending_data: Fence,
    pub direct: bool,
    pub statistics: stats::Owner,
}
impl Entry {
    /// # Safety
    /// Exact unpublished resident entry, or the enclosing exclusive BO owner.
    pub unsafe fn init_in_place(out: *mut Self) {
        unsafe {
            ptr::addr_of_mut!((*out).driver).write(Owner {
                kind: 2,
                id: 0,
                generation: 0,
            });
            ptr::addr_of_mut!((*out).target).write(a::GfxOutputTarget::default());
            ptr::addr_of_mut!((*out).binding).write(a::GfxBackendBinding::default());
            ptr::addr_of_mut!((*out).width).write(0);
            ptr::addr_of_mut!((*out).height).write(0);
            ptr::addr_of_mut!((*out).active).write(false);
            ptr::addr_of_mut!((*out).mode_blocked).write(false);
            ptr::addr_of_mut!((*out).mode_lost).write(false);
            ptr::addr_of_mut!((*out).removing).write(false);
            ptr::addr_of_mut!((*out).has_pending).write(false);
            ptr::addr_of_mut!((*out).pending_data).write(Fence::default());
            ptr::addr_of_mut!((*out).direct).write(false);
            stats::Owner::init_in_place(ptr::addr_of_mut!((*out).statistics));
        }
    }
    fn reset(&mut self) {
        unsafe { Self::init_in_place(self) }
    }
    pub fn snapshot(&self) -> backend::Snapshot {
        backend::Snapshot {
            state: backend::SOFTWARE_NATIVE,
            owner: self.driver.id as usize,
            generation: self.target.display_generation,
            adapter_id: self.binding.adapter_id,
            ..backend::Snapshot::default()
        }
    }
    pub fn info(&self) -> Result<a::DisplayPresentationInfo, Error> {
        let mut value = self
            .statistics
            .read_info(self.target.head_id, self.snapshot())?;
        value.width = self.width;
        value.height = self.height;
        if !self.active {
            value.flags = (value.flags & !a::display_presentation_info_active)
                | a::display_presentation_info_occluded
        }
        if self.mode_lost {
            value.flags |= a::display_presentation_info_lost
        }
        Ok(value)
    }
    pub fn publish_info(
        &mut self,
        driver: Owner,
        value: a::DisplayPresentationInfo,
    ) -> Result<(), Error> {
        if self.driver != driver
            || value.head_id != self.target.head_id
            || value.width != self.width
            || value.height != self.height
        {
            return Err(Error::Stale);
        }
        let state = self.snapshot();
        self.statistics
            .publish_info(driver.id as usize, driver.generation, value, state)
    }
    pub fn publish_stats(
        &mut self,
        driver: Owner,
        value: a::DisplayPresentationStats,
    ) -> Result<(), Error> {
        if self.driver != driver || value.head_id != self.target.head_id {
            return Err(Error::Stale);
        }
        let state = self.snapshot();
        self.statistics
            .publish(driver.id as usize, driver.generation, value, state)
    }
}
#[repr(C)]
pub struct Store {
    pub entries: [Entry; CAPACITY],
    pub serial: u64,
}
impl Store {
    /// # Safety
    /// Exact resident unobserved storage; no full Store reference precedes init.
    pub unsafe fn init_in_place(out: *mut Self) {
        unsafe {
            let entries = ptr::addr_of_mut!((*out).entries).cast::<Entry>();
            for i in 0..CAPACITY {
                Entry::init_in_place(entries.add(i))
            }
            ptr::addr_of_mut!((*out).serial).write(1u64 << 63);
        }
    }
    pub fn register(
        &mut self,
        driver: Owner,
        request: a::GfxAdditionalOutput,
    ) -> Result<a::GfxOutputTarget, Error> {
        let binding = request.backend;
        if driver.kind != 2
            || !driver.valid()
            || request.version != 1
            || request.size as usize != size_of::<a::GfxAdditionalOutput>()
            || request.flags != 0
            || request.reserved0 != 0
            || (request.job_size != 272
                && request.job_size as usize != size_of::<a::GfxDriverJob>())
            || request.head_id >= a::gfx_output_max_assignments
            || request.width == 0
            || request.width > 65536
            || request.height == 0
            || request.height > 65536
            || request.format != a::gfx_buffer_format_xrgb8888
            || binding.adapter_id == 0
            || request.output.adapter_id != binding.adapter_id
            || request.output.device_generation != binding.device_generation
            || request.output.connector_id == 0
            || request.output.connection_generation == 0
        {
            return Err(Error::Invalid);
        }
        let mut free = None;
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if entry.driver.id == 0 {
                if free.is_none() {
                    free = Some(i)
                }
                continue;
            }
            if entry.target.adapter_id == binding.adapter_id
                && (entry.target.head_id == request.head_id
                    || entry.target.connector_id == request.output.connector_id)
            {
                if entry.driver != driver
                    || entry.active
                    || entry.mode_blocked
                    || entry.removing
                    || !same(&entry.binding, &binding)
                    || entry.target.head_id != request.head_id
                    || entry.target.connector_id != request.output.connector_id
                    || entry.width != request.width
                    || entry.height != request.height
                {
                    return Err(Error::Busy);
                }
                entry.target = output_target::from_output(
                    request.output,
                    request.head_id,
                    entry.target.display_generation,
                );
                return Ok(entry.target);
            }
        }
        let index = free.ok_or(Error::Capacity)?;
        let generation = self.serial.checked_add(1).ok_or(Error::Exhausted)?;
        let entry = &mut self.entries[index];
        let checked = (|| {
            entry
                .statistics
                .bind(driver.id as usize, driver.generation, binding, generation)?;
            entry.statistics.publish_info(
                driver.id as usize,
                driver.generation,
                a::DisplayPresentationInfo {
                    flags: a::display_presentation_info_native
                        | a::display_presentation_info_active,
                    head_id: request.head_id,
                    backend: binding,
                    display_generation: generation,
                    sequence: 1,
                    width: request.width,
                    height: request.height,
                    format: request.format,
                    policies: 7,
                    buffer_count: 2,
                    plane_count: 1,
                    ..a::DisplayPresentationInfo::default()
                },
                backend::Snapshot {
                    state: backend::SOFTWARE_NATIVE,
                    owner: driver.id as usize,
                    generation,
                    adapter_id: binding.adapter_id,
                    ..backend::Snapshot::default()
                },
            )?;
            Ok(())
        })();
        if let Err(error) = checked {
            entry.reset();
            return Err(error);
        }
        entry.driver = driver;
        entry.target = output_target::from_output(request.output, request.head_id, generation);
        entry.binding = binding;
        entry.width = request.width;
        entry.height = request.height;
        self.serial = generation;
        Ok(entry.target)
    }
    pub fn find(&self, target: a::GfxOutputTarget) -> Result<usize, Error> {
        if !output_target::valid(&target) {
            return Err(Error::Invalid);
        }
        self.entries
            .iter()
            .position(|entry| entry.driver.id != 0 && output_target::same(&entry.target, &target))
            .ok_or(Error::Stale)
    }
    pub fn at(&self, adapter: u32, head: u32) -> Option<usize> {
        self.entries.iter().position(|entry| {
            entry.driver.id != 0
                && entry.active
                && entry.target.adapter_id == adapter
                && entry.target.head_id == head
        })
    }
    pub fn stop(&mut self, driver: u32) {
        for entry in &mut self.entries {
            if entry.driver.id == u64::from(driver) {
                entry.reset()
            }
        }
    }
}
const _: () = {
    use core::mem::{align_of, offset_of};
    assert!(
        size_of::<Entry>() == 9904
            && align_of::<Entry>() == 8
            && offset_of!(Entry, pending_data) == 120
            && offset_of!(Entry, direct) == 168
            && offset_of!(Entry, statistics) == 176
    );
    assert!(size_of::<Store>() == 79240 && offset_of!(Store, serial) == 79232);
};

/// # Safety
/// Exact initialized resident entry under BO admission. Read operations never
/// borrow mutably. Inputs are initialized and output holds its exact C record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_additional_entry(
    op: u32,
    entry: *mut Entry,
    driver: *const Owner,
    input: *const core::ffi::c_void,
    out: *mut core::ffi::c_void,
) -> i32 {
    match op {
        0 => {
            unsafe { out.cast::<backend::Snapshot>().write((&*entry).snapshot()) };
            0
        }
        1 => match unsafe { &*entry }.info() {
            Ok(value) => {
                unsafe { out.cast::<a::DisplayPresentationInfo>().write(value) };
                0
            }
            Err(error) => error as i32,
        },
        2 => match unsafe {
            (&mut *entry).publish_info(
                driver.read(),
                input.cast::<a::DisplayPresentationInfo>().read(),
            )
        } {
            Ok(()) => 0,
            Err(error) => error as i32,
        },
        3 => match unsafe {
            (&mut *entry).publish_stats(
                driver.read(),
                input.cast::<a::DisplayPresentationStats>().read(),
            )
        } {
            Ok(()) => 0,
            Err(error) => error as i32,
        },
        _ => panic!("additional output metadata operation"),
    }
}
/// # Safety
/// Exact initialized store under the BO owner; inputs readable and output exclusive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_additional_register(
    store: *mut Store,
    driver: *const Owner,
    input: *const a::GfxAdditionalOutput,
    out: *mut a::GfxOutputTarget,
) -> i32 {
    match unsafe { (&mut *store).register(driver.read(), input.read()) } {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as i32,
    }
}
/// # Safety
/// Exact initialized store/target under BO admission, exclusive output index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_additional_find(
    store: *const Store,
    target: *const a::GfxOutputTarget,
    out: *mut usize,
) -> i32 {
    match unsafe { (&*store).find(target.read()) } {
        Ok(index) => {
            unsafe { out.write(index) };
            0
        }
        Err(error) => error as i32,
    }
}
/// # Safety
/// Exact initialized store under BO admission, exclusive optional output index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_additional_at(
    store: *const Store,
    adapter: u32,
    head: u32,
    out: *mut usize,
) -> bool {
    let Some(index) = unsafe { &*store }.at(adapter, head) else {
        return false;
    };
    unsafe { out.write(index) };
    true
}
/// # Safety
/// Exact initialized store under its exclusive BO owner. This clears metadata
/// only and never establishes a physical queue/image release acknowledgement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_additional_stop(store: *mut Store, driver: u32) {
    unsafe { (&mut *store).stop(driver) }
}
