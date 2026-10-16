//! Original common BO/reference/lease state. This is metadata only: allocation,
//! callbacks, VM/TLB and physical retirement occur outside the enclosing owner.
use super::{
    layout::{self, Binding, Descriptor, Plane, Range},
    types::{self, Error, Handle, Owner},
};
use core::{ffi::c_void, mem::size_of, ptr, slice};
pub const EMPTY: u32 = 0;
pub const ALLOCATING: u32 = 1;
pub const LIVE: u32 = 2;
pub const RELEASING: u32 = 3;
pub const DESTROYING: u32 = 4;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Backing {
    pub cookie: u64,
    pub cpu_address: u64,
    pub bytes: u64,
    pub cache: u32,
    pub has_driver: bool,
    pub driver: Owner,
}
impl Backing {
    fn equal(self, other: Self) -> bool {
        self.cookie == other.cookie
            && self.cpu_address == other.cpu_address
            && self.bytes == other.bytes
            && self.cache == other.cache
            && self.has_driver == other.has_driver
            && (!self.has_driver || self.driver == other.driver)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Create {
    pub buffer: Handle,
    pub reference: Handle,
    pub bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Use {
    pub lease: Handle,
    pub buffer: Handle,
    pub access: u32,
    pub backing: Backing,
    pub range: Range,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Release {
    pub buffer: Handle,
    pub backing: Backing,
    pub attempt: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OwnedCreate {
    pub create: Create,
    pub cookie: u64,
    pub driver: Owner,
    pub binding: Binding,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OwnedRelease {
    pub release: Release,
    pub driver: Owner,
    pub binding: Binding,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BudgetSnapshot {
    pub limit: u64,
    pub charged: u64,
    pub closing: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub objects: usize,
    pub references: usize,
    pub leases: usize,
    pub bytes: u64,
    pub retained_bytes: u64,
    pub system_bytes: u64,
    pub device_bytes: u64,
    pub system_backed_bytes: u64,
    pub device_backed_bytes: u64,
    pub system_pinned_bytes: u64,
    pub device_pinned_bytes: u64,
    pub scanout_pinned_bytes: u64,
    pub device_mapped_bytes: u64,
    pub allocating_bytes: u64,
    pub destroying_bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Object {
    pub phase: u32,
    pub handle: Handle,
    pub producer: Owner,
    pub producer_open: bool,
    pub descriptor: Descriptor,
    pub allocation_bytes: u64,
    pub has_backing: bool,
    pub backing: Backing,
    pub references: u32,
    pub leases: u32,
    pub scanout_leases: u32,
    pub mapping_leases: u32,
    pub release_attempt: u64,
    pub owned_cookie: u64,
    pub owned_reference: Handle,
    pub device_lost: bool,
    pub device_quiesced: bool,
}
impl Default for Object {
    fn default() -> Self {
        Self {
            phase: EMPTY,
            handle: Handle::EMPTY,
            producer: Owner::EMPTY,
            producer_open: true,
            descriptor: Descriptor::EMPTY,
            allocation_bytes: 0,
            has_backing: false,
            backing: Backing::default(),
            references: 0,
            leases: 0,
            scanout_leases: 0,
            mapping_leases: 0,
            release_attempt: 0,
            owned_cookie: 0,
            owned_reference: Handle::EMPTY,
            device_lost: false,
            device_quiesced: false,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Reference {
    pub handle: Handle,
    pub buffer: Handle,
    pub owner: Owner,
    pub read_only: bool,
    pub mapping_only: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Lease {
    pub handle: Handle,
    pub buffer: Handle,
    pub owner: Owner,
    pub access: u32,
    pub range: Range,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct DeviceBudget {
    pub present: bool,
    pub driver: Owner,
    pub binding: Binding,
    pub bytes: u64,
    pub closing: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawView {
    pub objects: *mut Object,
    pub objects_len: usize,
    pub references: *mut Reference,
    pub references_len: usize,
    pub leases: *mut Lease,
    pub leases_len: usize,
    pub serial: *mut u64,
    pub committed_bytes: *mut u64,
    pub budget_bytes: u64,
    pub producer_budget_bytes: u64,
    pub device_budgets: *mut DeviceBudget,
    pub device_budgets_len: usize,
    pub dynamic_mask: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Args {
    pub owner: Owner,
    pub other: Owner,
    pub handle: Handle,
    pub binding: Binding,
    pub offset: u64,
    pub bytes: u64,
    pub value: u64,
    pub flag: bool,
    pub access: u32,
    pub pool: u32,
    pub input: *const c_void,
    pub other_input: *const c_void,
    pub buffer: *mut c_void,
    pub len: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OptionalRelease {
    pub present: bool,
    pub value: Release,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OptionalOwnedRelease {
    pub present: bool,
    pub value: OwnedRelease,
}
pub struct Table<'a> {
    pub objects: &'a mut [Object],
    pub references: &'a mut [Reference],
    pub leases: &'a mut [Lease],
    pub serial: &'a mut u64,
    pub committed_bytes: &'a mut u64,
    pub budget_bytes: u64,
    pub producer_budget_bytes: u64,
    pub device_budgets: &'a mut [DeviceBudget],
    pub dynamic_mask: u32,
}
unsafe fn initialized<'a, T>(p: *mut T, len: usize) -> &'a mut [T] {
    if len == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(p, len) }
    }
}
impl RawView {
    /// # Safety
    /// The caller exclusively owns disjoint initialized resident metadata arrays
    /// and scalar fields under the common BO owner until this view is dropped.
    /// No callback, allocation, wait or metadata replacement spans a Rust borrow.
    pub(crate) unsafe fn borrow<'a>(self) -> Table<'a> {
        unsafe {
            Table {
                objects: initialized(self.objects, self.objects_len),
                references: initialized(self.references, self.references_len),
                leases: initialized(self.leases, self.leases_len),
                serial: &mut *self.serial,
                committed_bytes: &mut *self.committed_bytes,
                budget_bytes: self.budget_bytes,
                producer_budget_bytes: self.producer_budget_bytes,
                device_budgets: initialized(self.device_budgets, self.device_budgets_len),
                dynamic_mask: self.dynamic_mask,
            }
        }
    }
}
impl Table<'_> {
    pub fn read_view(&self) -> ReadView<'_> {
        ReadView {
            objects: &*self.objects,
            references: &*self.references,
            leases: &*self.leases,
            committed_bytes: *self.committed_bytes,
            budget_bytes: self.budget_bytes,
            producer_budget_bytes: self.producer_budget_bytes,
            device_budgets: &*self.device_budgets,
            dynamic_mask: self.dynamic_mask,
        }
    }

    fn next_handle(&mut self, slot: usize) -> Result<Handle, Error> {
        if *self.serial == u64::MAX || slot >= u32::MAX as usize {
            return Err(Error::Exhausted);
        }
        *self.serial += 1;
        Ok(Handle {
            id: slot as u32 + 1,
            generation: *self.serial,
        })
    }

    pub fn set_device_budget(
        &mut self,
        driver: Owner,
        binding: Binding,
        bytes: u64,
    ) -> Result<(), Error> {
        if !driver.valid()
            || driver.kind != 2
            || driver.id > u64::from(u32::MAX)
            || !binding.valid()
            || binding.portable()
            || u64::from(binding.driver_owner) != driver.id
            || !bytes.is_multiple_of(4096)
        {
            return Err(Error::Invalid);
        }
        let mut selected = None;
        let mut empty = None;
        let mut combined = self.budget_bytes;
        for (i, budget) in self.device_budgets.iter().enumerate() {
            if !budget.present {
                if empty.is_none() {
                    empty = Some(i)
                }
                continue;
            }
            if budget.driver == driver && budget.binding == binding {
                if budget.closing {
                    return Err(Error::Closed);
                }
                selected = Some(i);
                continue;
            }
            if budget.binding.adapter == binding.adapter {
                return Err(Error::Busy);
            }
            combined = combined.checked_add(budget.bytes).ok_or(Error::Overflow)?;
        }
        let _ = combined.checked_add(bytes).ok_or(Error::Overflow)?;
        let i = selected.or(empty).ok_or(Error::Capacity)?;
        self.device_budgets[i] = DeviceBudget {
            present: true,
            driver,
            binding,
            bytes,
            closing: false,
        };
        Ok(())
    }

    fn collect_closed_budgets(&mut self) {
        for i in 0..self.device_budgets.len() {
            let b = self.device_budgets[i];
            if b.present && b.closing && self.read_view().charged_device(b) == 0 {
                self.device_budgets[i] = DeviceBudget::default();
            }
        }
    }
    pub fn begin(&mut self, producer: Owner, descriptor: Descriptor) -> Result<Create, Error> {
        let validated = layout::validate(descriptor, false)?;
        self.begin_validated(producer, descriptor, validated)
    }
    fn begin_validated(
        &mut self,
        producer: Owner,
        descriptor: Descriptor,
        validated: layout::Layout,
    ) -> Result<Create, Error> {
        if !producer.valid() {
            return Err(Error::Invalid);
        }
        let selected = self.read_view().budget_index(producer, descriptor);
        let (mut shared, mut owner_bytes) = (0u64, 0u64);
        for o in self.objects.iter() {
            if o.phase == EMPTY
                || self
                    .read_view()
                    .budget_index(o.producer, o.descriptor)
                    .is_some()
            {
                continue;
            }
            shared += o.allocation_bytes;
            if o.producer == producer {
                owner_bytes += o.allocation_bytes
            }
        }
        if let Some(i) = selected {
            let b = self.device_budgets[i];
            if b.closing {
                return Err(Error::Closed);
            }
            let charged = self.read_view().charged_device(b);
            if charged > b.bytes || validated.allocation_bytes > b.bytes - charged {
                return Err(Error::Budget);
            }
        } else {
            if shared > self.budget_bytes || validated.allocation_bytes > self.budget_bytes - shared
            {
                return Err(Error::Budget);
            }
            if owner_bytes > self.producer_budget_bytes
                || validated.allocation_bytes > self.producer_budget_bytes - owner_bytes
            {
                return Err(Error::Budget);
            }
        }
        let _ = self
            .committed_bytes
            .checked_add(validated.allocation_bytes)
            .ok_or(Error::Overflow)?;
        let slot = self
            .objects
            .iter()
            .position(|o| o.phase == EMPTY)
            .ok_or(Error::Capacity)?;
        let ref_slot = self
            .references
            .iter()
            .position(|r| r.handle.id == 0)
            .ok_or(Error::Capacity)?;
        if *self.serial > u64::MAX - 2 {
            return Err(Error::Exhausted);
        }
        let handle = self.next_handle(slot)?;
        let reference = self.next_handle(ref_slot)?;
        self.objects[slot] = Object {
            phase: ALLOCATING,
            handle,
            producer,
            descriptor,
            allocation_bytes: validated.allocation_bytes,
            references: 1,
            ..Object::default()
        };
        self.references[ref_slot] = Reference {
            handle: reference,
            buffer: handle,
            owner: producer,
            ..Reference::default()
        };
        *self.committed_bytes += validated.allocation_bytes;
        Ok(Create {
            buffer: handle,
            reference,
            bytes: validated.allocation_bytes,
        })
    }
    pub fn begin_owned(
        &mut self,
        driver: Owner,
        descriptor: Descriptor,
        cookie: u64,
    ) -> Result<OwnedCreate, Error> {
        if !driver.valid() || driver.kind != 2 || driver.id > u64::from(u32::MAX) || cookie == 0 {
            return Err(Error::Invalid);
        }
        if descriptor.location != 1
            || descriptor.binding.portable()
            || u64::from(descriptor.binding.driver_owner) != driver.id
            || descriptor.usage & 3 != 0
        {
            return Err(Error::Unsupported);
        }
        for o in self.objects.iter() {
            if o.phase != EMPTY
                && o.producer == driver
                && o.owned_cookie == cookie
                && o.descriptor.binding == descriptor.binding
            {
                return Err(Error::Busy);
            }
        }
        let validated = layout::validate(descriptor, true)?;
        let ticket = self.begin_validated(driver, descriptor, validated)?;
        let i = self.read_view().find_object(ticket.buffer)?;
        self.objects[i].owned_cookie = cookie;
        self.objects[i].owned_reference = ticket.reference;
        Ok(OwnedCreate {
            create: ticket,
            cookie,
            driver,
            binding: descriptor.binding,
        })
    }

    pub fn commit_owned(&mut self, ticket: OwnedCreate, driver: Owner) -> Result<(), Error> {
        let i = self.read_view().owned_creation(ticket, driver)?;
        if !self.objects[i].producer_open || self.objects[i].references == 0 {
            return Err(Error::Closed);
        }
        self.publish(
            ticket.create,
            Backing {
                cookie: ticket.cookie,
                bytes: ticket.create.bytes,
                has_driver: true,
                driver,
                ..Backing::default()
            },
        )
    }
    pub fn abort_owned(
        &mut self,
        ticket: OwnedCreate,
        driver: Owner,
        quiesced: bool,
    ) -> Result<(), Error> {
        let _ = self.read_view().owned_creation(ticket, driver)?;
        if !quiesced {
            return Err(Error::Busy);
        }
        self.abort(ticket.create)
    }
    pub fn publish(&mut self, ticket: Create, backing: Backing) -> Result<(), Error> {
        let i = self.read_view().find_object(ticket.buffer)?;
        let o = &mut self.objects[i];
        if o.phase != ALLOCATING || o.has_backing {
            return Err(Error::Stale);
        }
        if backing.cookie == 0
            || backing.bytes < o.allocation_bytes
            || (backing.cpu_address == 0) != (backing.cache == 0)
            || (backing.cpu_address != 0
                && (!backing.cpu_address.is_multiple_of(o.descriptor.alignment)
                    || backing.cpu_address > u64::MAX - backing.bytes))
        {
            return Err(Error::Invalid);
        }
        if backing.has_driver {
            let driver = backing.driver;
            if driver.kind != 2
                || !driver.valid()
                || u64::from(o.descriptor.binding.driver_owner) != driver.id
            {
                return Err(Error::Invalid);
            }
        } else if o.descriptor.location == 1 {
            return Err(Error::Invalid);
        }
        o.backing = backing;
        o.has_backing = true;
        o.phase = if o.references == 0 && o.leases == 0 {
            RELEASING
        } else {
            LIVE
        };
        Ok(())
    }
    pub fn abort(&mut self, ticket: Create) -> Result<(), Error> {
        let i = self.read_view().find_object(ticket.buffer)?;
        if self.objects[i].phase != ALLOCATING || self.objects[i].has_backing {
            return Err(Error::Stale);
        }
        for reference in self.references.iter_mut() {
            if reference.buffer == ticket.buffer {
                *reference = Reference::default();
            }
        }
        *self.committed_bytes -= self.objects[i].allocation_bytes;
        self.objects[i] = Object::default();
        self.collect_closed_budgets();
        Ok(())
    }
    pub fn import_mode(
        &mut self,
        buffer: Handle,
        consumer: Owner,
        read_only: bool,
    ) -> Result<Handle, Error> {
        if !consumer.valid() {
            return Err(Error::Invalid);
        }
        let i = self.read_view().find_object(buffer)?;
        let o = self.objects[i];
        if o.phase != LIVE || !o.producer_open {
            return Err(Error::Closed);
        }
        if read_only {
            for l in self.leases.iter() {
                if l.handle.id != 0 && l.buffer == buffer && types::writes(l.access) {
                    return Err(Error::Busy);
                }
            }
        }
        let slot = self
            .references
            .iter()
            .position(|r| r.handle.id == 0)
            .ok_or(Error::Capacity)?;
        let reference = self.next_handle(slot)?;
        self.references[slot] = Reference {
            handle: reference,
            buffer,
            owner: consumer,
            read_only,
            mapping_only: false,
        };
        self.objects[i].references += 1;
        Ok(reference)
    }

    pub fn share(&mut self, handle: Handle, consumer: Owner) -> Result<Handle, Error> {
        if handle.id == 0 || handle.id as usize > self.references.len() || handle.generation == 0 {
            return Err(Error::Stale);
        }
        let r = self.references[handle.id as usize - 1];
        if r.handle != handle {
            return Err(Error::Stale);
        }
        if r.mapping_only {
            return Err(Error::Unsupported);
        }
        self.import_mode(r.buffer, consumer, r.read_only)
    }

    pub fn drop_reference(&mut self, handle: Handle, owner: Owner) -> Result<(), Error> {
        let r = self.read_view().find_reference(handle, owner)?;
        let i = self.read_view().find_object(self.references[r].buffer)?;
        self.references[r] = Reference::default();
        self.objects[i].references -= 1;
        Self::maybe_release(&mut self.objects[i]);
        Ok(())
    }
    pub fn use_resource(
        &mut self,
        reference: Handle,
        owner: Owner,
        access: u32,
        offset: u64,
        bytes: u64,
    ) -> Result<Use, Error> {
        let r = self.references[self.read_view().find_reference(reference, owner)?];
        if r.mapping_only && access != 5 {
            return Err(Error::Unsupported);
        }
        if r.read_only && types::writes(access) {
            return Err(Error::Unsupported);
        }
        let i = self.read_view().referenced_object(reference, owner)?;
        let o = self.objects[i];
        if o.phase != LIVE || o.device_lost {
            return Err(Error::Closed);
        }
        if types::writes(access) {
            for item in self.references.iter() {
                if item.handle.id != 0 && item.buffer == o.handle && item.read_only {
                    return Err(Error::Busy);
                }
            }
        }
        // Only a separately mapping-only reference can extend to allocated pages.
        // Ordinary CPU, execution, queue and residency uses retain logical length.
        let limit = if r.mapping_only {
            o.allocation_bytes
        } else {
            o.descriptor.bytes
        };
        if !layout::span_fits(limit, offset, bytes) {
            return Err(Error::Invalid);
        }
        if !o.has_backing {
            return Err(Error::Busy);
        }
        let backing = o.backing;
        let required = match access {
            0 | 8 => 1,
            1 => 2,
            9 => 3,
            2 | 6 => 4,
            3 | 7 => 8 | 16,
            4 => 32,
            5 => 0,
            _ => unreachable!(),
        };
        if required != 0 && o.descriptor.usage & required == 0 {
            return Err(Error::Unsupported);
        }
        if types::persistent_cpu(access)
            && (o.descriptor.location != 0
                || backing.cache != 1
                || o.descriptor.usage & required != required)
        {
            return Err(Error::Unsupported);
        }
        if types::is_cpu(access) && backing.cpu_address == 0 {
            return Err(Error::Unsupported);
        }
        if access == 0 && backing.cache == 2 {
            return Err(Error::Unsupported);
        }
        for l in self.leases.iter() {
            if l.handle.id != 0 && l.buffer == o.handle && types::conflicts(l.access, access) {
                return Err(Error::Busy);
            }
        }
        let slot = self
            .leases
            .iter()
            .position(|l| l.handle.id == 0)
            .ok_or(Error::Capacity)?;
        let token = self.next_handle(slot)?;
        let range = Range { offset, bytes };
        self.leases[slot] = Lease {
            handle: token,
            buffer: o.handle,
            owner,
            access,
            range,
        };
        self.objects[i].leases += 1;
        if access == 4 {
            self.objects[i].scanout_leases += 1
        }
        if access == 5 {
            self.objects[i].mapping_leases += 1
        }
        Ok(Use {
            lease: token,
            buffer: o.handle,
            access,
            backing,
            range,
        })
    }
    pub fn reserve_queued(
        &mut self,
        reference: Handle,
        producer: Owner,
        queue_owner: Owner,
        write: bool,
        offset: u64,
        bytes: u64,
    ) -> Result<Use, Error> {
        if queue_owner.kind != 0 || !queue_owner.valid() {
            return Err(Error::Invalid);
        }
        let result = self.use_resource(
            reference,
            producer,
            if write { 7 } else { 6 },
            offset,
            bytes,
        )?;
        let l = self.read_view().find_lease(result.lease, producer)?;
        self.leases[l].owner = queue_owner;
        Ok(result)
    }
    pub fn retain_queued(
        &mut self,
        handle: Handle,
        queue_owner: Owner,
        driver: Owner,
    ) -> Result<Handle, Error> {
        if queue_owner.kind != 0 || !queue_owner.valid() || driver.kind != 2 || !driver.valid() {
            return Err(Error::Invalid);
        }
        let l = self.leases[self.read_view().find_lease(handle, queue_owner)?];
        if !types::queued(l.access) {
            return Err(Error::Unsupported);
        }
        let i = self.read_view().find_object(l.buffer)?;
        let o = self.objects[i];
        if o.phase != LIVE || o.device_lost || !o.has_backing {
            return Err(Error::Closed);
        }
        let slot = self
            .references
            .iter()
            .position(|r| r.handle.id == 0)
            .ok_or(Error::Capacity)?;
        let reference = self.next_handle(slot)?;
        self.references[slot] = Reference {
            handle: reference,
            buffer: o.handle,
            owner: driver,
            mapping_only: true,
            read_only: false,
        };
        self.objects[i].references += 1;
        Ok(reference)
    }
    pub fn retain_scanout(
        &mut self,
        handle: Handle,
        queue_owner: Owner,
        driver: Owner,
    ) -> Result<Handle, Error> {
        let l = self.leases[self.read_view().find_lease(handle, queue_owner)?];
        let i = self.read_view().find_object(l.buffer)?;
        let o = self.objects[i];
        if l.access != 6 || o.descriptor.location != 1 || o.descriptor.usage & 32 == 0 {
            return Err(Error::Unsupported);
        }
        let reference = self.retain_queued(handle, queue_owner, driver)?;
        let r = self
            .read_view()
            .find_reference(reference, driver)
            .expect("Original retained scanout reference");
        self.references[r].mapping_only = false;
        self.references[r].read_only = true;
        Ok(reference)
    }
    pub fn end_use(
        &mut self,
        handle: Handle,
        owner: Owner,
        device_quiesced: bool,
    ) -> Result<(), Error> {
        let li = self.read_view().find_lease(handle, owner)?;
        let l = self.leases[li];
        if !types::is_cpu(l.access) && !device_quiesced {
            return Err(Error::Busy);
        }
        let i = self.read_view().find_object(l.buffer)?;
        if l.access == 4 {
            self.objects[i].scanout_leases -= 1
        }
        if l.access == 5 {
            self.objects[i].mapping_leases -= 1
        }
        self.leases[li] = Lease::default();
        self.objects[i].leases -= 1;
        Self::maybe_release(&mut self.objects[i]);
        Ok(())
    }

    pub fn lose_device(
        &mut self,
        driver: Owner,
        binding: Binding,
        quiesced: bool,
    ) -> Result<(), Error> {
        if !driver.valid() || driver.kind != 2 || !binding.valid() || binding.portable() {
            return Err(Error::Invalid);
        }
        if u64::from(binding.driver_owner) != driver.id {
            return Err(Error::WrongOwner);
        }
        for b in self.device_budgets.iter_mut() {
            if b.present && b.driver == driver && b.binding == binding {
                b.closing = true;
            }
        }
        for o in self.objects.iter_mut() {
            if o.phase == EMPTY
                || o.descriptor.location != 1
                || o.producer != driver
                || o.descriptor.binding != binding
            {
                continue;
            }
            o.device_lost = true;
            o.device_quiesced |= quiesced;
            o.producer_open = false;
            Self::maybe_release(o);
        }
        self.collect_closed_budgets();
        Ok(())
    }
    pub fn stopped_owner(&mut self, owner: Owner) {
        for b in self.device_budgets.iter_mut() {
            if b.present && b.driver == owner {
                b.closing = true;
            }
        }
        for o in self.objects.iter_mut() {
            if o.phase != EMPTY && o.producer == owner {
                o.producer_open = false;
            }
        }
        for ri in 0..self.references.len() {
            let r = self.references[ri];
            if r.handle.id == 0 || r.owner != owner {
                continue;
            }
            let i = self
                .read_view()
                .find_object(r.buffer)
                .expect("Original stopped reference owner");
            self.references[ri] = Reference::default();
            self.objects[i].references -= 1;
        }
        for li in 0..self.leases.len() {
            let l = self.leases[li];
            if l.handle.id == 0 || l.owner != owner || !types::is_cpu(l.access) {
                continue;
            }
            let i = self
                .read_view()
                .find_object(l.buffer)
                .expect("Original stopped CPU lease owner");
            self.leases[li] = Lease::default();
            self.objects[i].leases -= 1;
        }
        for o in self.objects.iter_mut() {
            if o.phase != EMPTY {
                Self::maybe_release(o);
            }
        }
        self.collect_closed_budgets();
    }

    pub fn pending_release(&mut self, system_only: bool) -> Option<Release> {
        self.pending_filtered(None, system_only)
    }
    pub fn take_owned_release(
        &mut self,
        driver: Owner,
        binding: Binding,
    ) -> Result<Option<OwnedRelease>, Error> {
        if !driver.valid()
            || driver.kind != 2
            || !binding.valid()
            || binding.portable()
            || u64::from(binding.driver_owner) != driver.id
        {
            return Err(Error::Invalid);
        }
        Ok(self
            .pending_filtered(Some((driver, binding)), false)
            .map(|release| OwnedRelease {
                release,
                driver,
                binding,
            }))
    }
    fn pending_filtered(
        &mut self,
        owned: Option<(Owner, Binding)>,
        system_only: bool,
    ) -> Option<Release> {
        for o in self.objects.iter_mut() {
            if o.phase != RELEASING || !o.has_backing {
                continue;
            }
            let backing = o.backing;
            if system_only && backing.has_driver {
                continue;
            }
            if let Some((driver, binding)) = owned
                && (!backing.has_driver
                    || o.owned_cookie == 0
                    || backing.driver != driver
                    || o.descriptor.binding != binding)
            {
                continue;
            }
            if o.release_attempt == u64::MAX {
                continue;
            }
            o.release_attempt += 1;
            o.phase = DESTROYING;
            return Some(Release {
                buffer: o.handle,
                backing,
                attempt: o.release_attempt,
            });
        }
        None
    }
    pub fn finish_owned_release(
        &mut self,
        ticket: OwnedRelease,
        driver: Owner,
        quiesced: bool,
    ) -> Result<(), Error> {
        if ticket.driver != driver {
            return Err(Error::WrongOwner);
        }
        let o = self.objects[self.read_view().find_object(ticket.release.buffer)?];
        if !o.has_backing {
            return Err(Error::Stale);
        }
        let backing = o.backing;
        if !backing.has_driver || backing.driver != driver {
            return Err(Error::WrongOwner);
        }
        if o.owned_cookie == 0
            || o.descriptor.binding != ticket.binding
            || !backing.equal(ticket.release.backing)
            || o.release_attempt != ticket.release.attempt
            || o.phase != DESTROYING
        {
            return Err(Error::Stale);
        }
        if !quiesced {
            return Err(Error::Busy);
        }
        self.finish_release(ticket.release, true)
    }
    pub fn finish_release(&mut self, ticket: Release, released: bool) -> Result<(), Error> {
        let i = self.read_view().find_object(ticket.buffer)?;
        let o = self.objects[i];
        if o.release_attempt != ticket.attempt {
            return Err(Error::Stale);
        }
        if o.phase != DESTROYING
            || o.leases != 0
            || (o.references != 0 && !(o.device_lost && o.device_quiesced))
        {
            return Err(Error::Busy);
        }
        if !o.has_backing {
            return Err(Error::Stale);
        }
        if o.backing.cookie != ticket.backing.cookie {
            return Err(Error::Stale);
        }
        if !released {
            self.objects[i].phase = RELEASING;
            return Err(Error::Busy);
        }
        *self.committed_bytes -= o.allocation_bytes;
        if o.references == 0 {
            self.objects[i] = Object::default();
        } else {
            let o = &mut self.objects[i];
            o.phase = LIVE;
            o.has_backing = false;
            o.backing = Backing::default();
            o.owned_cookie = 0;
            o.owned_reference = Handle::EMPTY;
            o.allocation_bytes = 0;
        }
        self.collect_closed_budgets();
        Ok(())
    }

    fn maybe_release(o: &mut Object) {
        if o.phase != LIVE || o.leases != 0 {
            return;
        }
        if o.device_lost && o.device_quiesced && !o.has_backing && o.owned_cookie == 0 {
            if o.references == 0 {
                *o = Object::default();
            }
            return;
        }
        if o.references == 0 || (o.device_lost && o.device_quiesced) {
            o.phase = RELEASING;
        }
    }
}

pub struct ReadView<'a> {
    pub objects: &'a [Object],
    pub references: &'a [Reference],
    pub leases: &'a [Lease],
    pub committed_bytes: u64,
    pub budget_bytes: u64,
    pub producer_budget_bytes: u64,
    pub device_budgets: &'a [DeviceBudget],
    pub dynamic_mask: u32,
}
unsafe fn initialized_read<'a, T>(p: *const T, len: usize) -> &'a [T] {
    if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p, len) }
    }
}
impl RawView {
    /// # Safety
    /// Caller retains initialized resident metadata under its original owner.
    /// Read-only legacy views may point to constant storage: no mutable reference
    /// is formed for a query, even when it makes no actual write.
    pub(crate) unsafe fn borrow_read<'a>(self) -> ReadView<'a> {
        unsafe {
            ReadView {
                objects: initialized_read(self.objects, self.objects_len),
                references: initialized_read(self.references, self.references_len),
                leases: initialized_read(self.leases, self.leases_len),
                committed_bytes: self.committed_bytes.read(),
                budget_bytes: self.budget_bytes,
                producer_budget_bytes: self.producer_budget_bytes,
                device_budgets: initialized_read(self.device_budgets, self.device_budgets_len),
                dynamic_mask: self.dynamic_mask,
            }
        }
    }
}
impl ReadView<'_> {
    fn find_object(&self, handle: Handle) -> Result<usize, Error> {
        if handle.id == 0 || handle.id as usize > self.objects.len() || handle.generation == 0 {
            return Err(Error::Stale);
        }
        let i = handle.id as usize - 1;
        let item = &self.objects[i];
        if item.phase == EMPTY || item.handle != handle {
            return Err(Error::Stale);
        }
        Ok(i)
    }
    fn find_reference(&self, handle: Handle, owner: Owner) -> Result<usize, Error> {
        if handle.id == 0 || handle.id as usize > self.references.len() || handle.generation == 0 {
            return Err(Error::Stale);
        }
        let i = handle.id as usize - 1;
        let item = &self.references[i];
        if item.handle != handle {
            return Err(Error::Stale);
        }
        if item.owner != owner {
            return Err(Error::WrongOwner);
        }
        Ok(i)
    }
    fn referenced_object(&self, handle: Handle, owner: Owner) -> Result<usize, Error> {
        self.find_object(self.references[self.find_reference(handle, owner)?].buffer)
    }
    fn find_lease(&self, handle: Handle, owner: Owner) -> Result<usize, Error> {
        if handle.id == 0 || handle.id as usize > self.leases.len() || handle.generation == 0 {
            return Err(Error::Stale);
        }
        let i = handle.id as usize - 1;
        let item = &self.leases[i];
        if item.handle != handle {
            return Err(Error::Stale);
        }
        if item.owner != owner {
            return Err(Error::WrongOwner);
        }
        Ok(i)
    }
    pub fn free_slots(&self, pool: u32, needed: usize) -> usize {
        fn count(ids: impl Iterator<Item = u32>, needed: usize) -> usize {
            let mut count = 0;
            for id in ids {
                if id == 0 {
                    count += 1;
                    if count >= needed {
                        break;
                    }
                }
            }
            count
        }
        match pool {
            0 => count(self.objects.iter().map(|o| o.handle.id), needed),
            1 => count(self.references.iter().map(|r| r.handle.id), needed),
            2 => count(self.leases.iter().map(|l| l.handle.id), needed),
            _ => unreachable!(),
        }
    }
    /// # Safety
    /// Prepared storage is distinct, aligned, large enough and has an initialized
    /// new tail. Its prefix is still uninitialized: copy raw records before ever
    /// forming a full Rust slice of the replacement. Caller publishes the slice
    /// header under the same uninterrupted metadata owner after this succeeds.
    pub(crate) unsafe fn replace_storage(
        &self,
        pool: u32,
        prepared: *mut c_void,
        len: usize,
    ) -> Result<(), Error> {
        if self.dynamic_mask & (1 << pool) == 0 {
            return Err(Error::Unsupported);
        }
        let (old, old_len, element) = match pool {
            0 => (
                self.objects.as_ptr().cast::<u8>(),
                self.objects.len(),
                size_of::<Object>(),
            ),
            1 => (
                self.references.as_ptr().cast::<u8>(),
                self.references.len(),
                size_of::<Reference>(),
            ),
            2 => (
                self.leases.as_ptr().cast::<u8>(),
                self.leases.len(),
                size_of::<Lease>(),
            ),
            _ => unreachable!(),
        };
        if len <= old_len || len > u32::MAX as usize {
            return Err(Error::Invalid);
        }
        let start = prepared.addr();
        let end = start
            .checked_add(len * element)
            .expect("Original prepared metadata extent");
        let previous = old.addr();
        if old_len != 0
            && start
                < previous
                    .checked_add(old_len * element)
                    .expect("Original old metadata extent")
            && previous < end
        {
            return Err(Error::Invalid);
        }
        unsafe { ptr::copy_nonoverlapping(old, prepared.cast::<u8>(), old_len * element) }
        Ok(())
    }
    fn belongs(object: &Object, budget: DeviceBudget) -> bool {
        object.phase != EMPTY
            && object.descriptor.location == 1
            && object.producer == budget.driver
            && object.descriptor.binding == budget.binding
    }
    fn budget_index(&self, producer: Owner, descriptor: Descriptor) -> Option<usize> {
        if descriptor.location != 1 || producer.kind != 2 {
            return None;
        }
        self.device_budgets
            .iter()
            .position(|b| b.present && b.driver == producer && b.binding == descriptor.binding)
    }
    fn charged_device(&self, budget: DeviceBudget) -> u64 {
        let mut bytes = 0;
        for object in self.objects.iter() {
            if Self::belongs(object, budget) {
                bytes += object.allocation_bytes
            }
        }
        bytes
    }
    pub fn device_budget(&self, driver: Owner, binding: Binding) -> Result<BudgetSnapshot, Error> {
        for &b in self.device_budgets.iter() {
            if b.present && b.driver == driver && b.binding == binding {
                return Ok(BudgetSnapshot {
                    limit: b.bytes,
                    charged: self.charged_device(b),
                    closing: b.closing,
                });
            }
        }
        Err(Error::Stale)
    }
    pub fn query_device_budget(
        &self,
        adapter: u32,
        generation: u64,
    ) -> Result<BudgetSnapshot, Error> {
        if adapter == 0 || generation == 0 {
            return Err(Error::Invalid);
        }
        for &b in self.device_budgets.iter() {
            if b.present
                && b.binding.adapter == adapter
                && b.binding.device_generation == generation
            {
                return Ok(BudgetSnapshot {
                    limit: b.bytes,
                    charged: self.charged_device(b),
                    closing: b.closing,
                });
            }
        }
        Err(Error::Stale)
    }
    pub fn shared_charged_bytes(&self) -> u64 {
        let mut bytes = self.committed_bytes;
        for &b in self.device_budgets.iter() {
            if b.present {
                bytes -= self.charged_device(b)
            }
        }
        bytes
    }
    pub fn total_budget_bytes(&self) -> u64 {
        let mut bytes = self.budget_bytes;
        for b in self.device_budgets.iter() {
            if b.present {
                bytes += b.bytes
            }
        }
        bytes
    }
    fn owned_creation(&self, ticket: OwnedCreate, driver: Owner) -> Result<usize, Error> {
        if ticket.driver != driver {
            return Err(Error::WrongOwner);
        }
        let i = self.find_object(ticket.create.buffer)?;
        let o = &self.objects[i];
        if o.producer != driver {
            return Err(Error::WrongOwner);
        }
        if o.phase != ALLOCATING
            || o.owned_cookie == 0
            || o.owned_cookie != ticket.cookie
            || o.owned_reference != ticket.create.reference
            || o.allocation_bytes != ticket.create.bytes
            || o.descriptor.binding != ticket.binding
        {
            return Err(Error::Stale);
        }
        Ok(i)
    }
    pub fn describe(&self, reference: Handle, owner: Owner) -> Result<Descriptor, Error> {
        let i = self.referenced_object(reference, owner)?;
        if self.objects[i].device_lost {
            return Err(Error::Closed);
        }
        Ok(self.objects[i].descriptor)
    }
    pub fn read_only(&self, handle: Handle, owner: Owner) -> Result<bool, Error> {
        Ok(self.references[self.find_reference(handle, owner)?].read_only)
    }
    pub fn mapping_only(&self, handle: Handle, owner: Owner) -> Result<bool, Error> {
        Ok(self.references[self.find_reference(handle, owner)?].mapping_only)
    }
    pub fn use_info(&self, handle: Handle, owner: Owner) -> Result<Use, Error> {
        let l = self.leases[self.find_lease(handle, owner)?];
        let o = self.objects[self.find_object(l.buffer)?];
        if !o.has_backing {
            return Err(Error::Busy);
        }
        Ok(Use {
            lease: handle,
            buffer: o.handle,
            access: l.access,
            backing: o.backing,
            range: l.range,
        })
    }
    pub fn buffer_for(&self, handle: Handle, owner: Owner) -> Result<Handle, Error> {
        Ok(self.references[self.find_reference(handle, owner)?].buffer)
    }
    pub fn pending_release_for_owner(&self, producer: Owner) -> bool {
        for o in self.objects.iter() {
            if matches!(o.phase, RELEASING | DESTROYING) && o.producer == producer {
                return true;
            }
            if o.phase == ALLOCATING && o.owned_cookie != 0 && o.producer == producer {
                return true;
            }
        }
        false
    }
    pub fn retains_driver(&self, owner: Owner) -> bool {
        for o in self.objects.iter() {
            if o.phase != EMPTY && o.owned_cookie != 0 && o.producer == owner {
                return true;
            }
        }
        for o in self.objects.iter() {
            if o.has_backing && o.backing.has_driver && o.backing.driver == owner {
                return true;
            }
        }
        for l in self.leases.iter() {
            if l.handle.id != 0 && l.owner == owner && !types::is_cpu(l.access) {
                return true;
            }
        }
        false
    }
    pub fn stats(&self) -> Stats {
        let mut result = Stats {
            bytes: self.committed_bytes,
            ..Stats::default()
        };
        for o in self.objects.iter() {
            if o.phase == EMPTY {
                continue;
            }
            result.objects += 1;
            result.references += o.references as usize;
            result.leases += o.leases as usize;
            if !o.producer_open || matches!(o.phase, RELEASING | DESTROYING) {
                result.retained_bytes += o.allocation_bytes;
            }
            let bytes = o.allocation_bytes;
            let device = o.descriptor.location == 1;
            if device {
                result.device_bytes += bytes
            } else {
                result.system_bytes += bytes
            }
            if o.phase == ALLOCATING {
                result.allocating_bytes += bytes
            }
            if matches!(o.phase, RELEASING | DESTROYING) {
                result.destroying_bytes += bytes
            }
            if o.has_backing {
                if device {
                    result.device_backed_bytes += bytes
                } else {
                    result.system_backed_bytes += bytes
                }
                if o.leases != 0 {
                    if device {
                        result.device_pinned_bytes += bytes
                    } else {
                        result.system_pinned_bytes += bytes
                    }
                }
            }
            if o.scanout_leases != 0 {
                result.scanout_pinned_bytes += bytes
            }
            if o.mapping_leases != 0 {
                result.device_mapped_bytes += bytes
            }
        }
        assert!(result.system_bytes + result.device_bytes == result.bytes);
        result
    }
}

pub(crate) unsafe fn dispatch(
    view: RawView,
    op: u32,
    args: Args,
    out: *mut c_void,
) -> Result<(), Error> {
    if matches!(
        op,
        0 | 1 | 3 | 4 | 5 | 6 | 15 | 17 | 18 | 25 | 26 | 29 | 35 | 36
    ) {
        let t = unsafe { view.borrow_read() };
        unsafe {
            match op {
                0 => out.cast::<usize>().write(t.free_slots(args.pool, args.len)),
                1 => t.replace_storage(args.pool, args.buffer, args.len)?,
                3 => out
                    .cast::<BudgetSnapshot>()
                    .write(t.device_budget(args.owner, args.binding)?),
                4 => out
                    .cast::<BudgetSnapshot>()
                    .write(t.query_device_budget(args.access, args.value)?),
                5 => out.cast::<u64>().write(t.shared_charged_bytes()),
                6 => out.cast::<u64>().write(t.total_budget_bytes()),
                15 => out
                    .cast::<Descriptor>()
                    .write(t.describe(args.handle, args.owner)?),
                17 => out
                    .cast::<bool>()
                    .write(t.read_only(args.handle, args.owner)?),
                18 => out
                    .cast::<bool>()
                    .write(t.mapping_only(args.handle, args.owner)?),
                25 => out
                    .cast::<Use>()
                    .write(t.use_info(args.handle, args.owner)?),
                26 => out
                    .cast::<Handle>()
                    .write(t.buffer_for(args.handle, args.owner)?),
                29 => out
                    .cast::<bool>()
                    .write(t.pending_release_for_owner(args.owner)),
                35 => out.cast::<bool>().write(t.retains_driver(args.owner)),
                36 => out.cast::<Stats>().write(t.stats()),
                _ => unreachable!(),
            }
        }
        return Ok(());
    }
    let mut t = unsafe { view.borrow() };
    unsafe {
        match op {
            2 => t.set_device_budget(args.owner, args.binding, args.bytes)?,
            7 => out
                .cast::<Create>()
                .write(t.begin(args.owner, args.input.cast::<Descriptor>().read())?),
            8 => out.cast::<OwnedCreate>().write(t.begin_owned(
                args.owner,
                args.input.cast::<Descriptor>().read(),
                args.value,
            )?),
            9 => t.commit_owned(args.input.cast::<OwnedCreate>().read(), args.owner)?,
            10 => t.abort_owned(
                args.input.cast::<OwnedCreate>().read(),
                args.owner,
                args.flag,
            )?,
            11 => t.publish(
                args.input.cast::<Create>().read(),
                args.other_input.cast::<Backing>().read(),
            )?,
            12 => t.abort(args.input.cast::<Create>().read())?,
            13 => out
                .cast::<Handle>()
                .write(t.import_mode(args.handle, args.owner, false)?),
            14 => out
                .cast::<Handle>()
                .write(t.import_mode(args.handle, args.owner, args.flag)?),
            16 => out
                .cast::<Handle>()
                .write(t.share(args.handle, args.owner)?),
            19 => t.drop_reference(args.handle, args.owner)?,
            20 => out.cast::<Use>().write(t.use_resource(
                args.handle,
                args.owner,
                args.access,
                args.offset,
                args.bytes,
            )?),
            21 => out.cast::<Use>().write(t.reserve_queued(
                args.handle,
                args.owner,
                args.other,
                args.flag,
                args.offset,
                args.bytes,
            )?),
            22 => {
                out.cast::<Handle>()
                    .write(t.retain_queued(args.handle, args.owner, args.other)?)
            }
            23 => {
                out.cast::<Handle>()
                    .write(t.retain_scanout(args.handle, args.owner, args.other)?)
            }
            24 => t.end_use(args.handle, args.owner, args.flag)?,
            27 => t.lose_device(args.owner, args.binding, args.flag)?,
            28 => t.stopped_owner(args.owner),
            30 | 31 => {
                let value = t.pending_release(op == 31);
                out.cast::<OptionalRelease>().write(OptionalRelease {
                    present: value.is_some(),
                    value: value.unwrap_or_default(),
                });
            }
            32 => {
                let value = t.take_owned_release(args.owner, args.binding)?;
                out.cast::<OptionalOwnedRelease>()
                    .write(OptionalOwnedRelease {
                        present: value.is_some(),
                        value: value.unwrap_or_default(),
                    });
            }
            33 => t.finish_owned_release(
                args.input.cast::<OwnedRelease>().read(),
                args.owner,
                args.flag,
            )?,
            34 => t.finish_release(args.input.cast::<Release>().read(), args.flag)?,
            _ => panic!("Invalid private BO operation"),
        }
    }
    Ok(())
}

/// # Safety
/// The caller holds the original exclusive no-sleep BO metadata owner over the
/// disjoint initialized arrays/scalars in view. Op selects the documented exact
/// resident input/output record shapes; outputs and copied inputs are disjoint
/// from mutable metadata. Replacement's raw uninitialized prefix is only written
/// before its slice is published. No borrow, callback or wait escapes the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_bo_run(
    view: *const RawView,
    op: u32,
    args: *const Args,
    out: *mut c_void,
) -> i32 {
    let view = unsafe { view.read() };
    let args = unsafe { args.read() };
    match unsafe { dispatch(view, op, args, out) } {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}

const _: () = {
    assert!(core::mem::size_of::<Owner>() == 24 && core::mem::align_of::<Owner>() == 8);
    assert!(core::mem::offset_of!(Owner, kind) == 0);
    assert!(core::mem::offset_of!(Owner, id) == 8);
    assert!(core::mem::offset_of!(Owner, generation) == 16);
    assert!(core::mem::size_of::<Handle>() == 16 && core::mem::align_of::<Handle>() == 8);
    assert!(core::mem::offset_of!(Handle, id) == 0);
    assert!(core::mem::offset_of!(Handle, generation) == 8);
    assert!(core::mem::size_of::<Binding>() == 16 && core::mem::align_of::<Binding>() == 8);
    assert!(core::mem::offset_of!(Binding, adapter) == 0);
    assert!(core::mem::offset_of!(Binding, driver_owner) == 4);
    assert!(core::mem::offset_of!(Binding, device_generation) == 8);
    assert!(core::mem::size_of::<Plane>() == 16 && core::mem::align_of::<Plane>() == 8);
    assert!(core::mem::offset_of!(Plane, offset) == 0);
    assert!(core::mem::offset_of!(Plane, pitch) == 8);
    assert!(core::mem::size_of::<Range>() == 16 && core::mem::align_of::<Range>() == 8);
    assert!(core::mem::offset_of!(Range, offset) == 0);
    assert!(core::mem::offset_of!(Range, bytes) == 8);
    assert!(core::mem::size_of::<Descriptor>() == 128 && core::mem::align_of::<Descriptor>() == 8);
    assert!(core::mem::offset_of!(Descriptor, bytes) == 0);
    assert!(core::mem::offset_of!(Descriptor, alignment) == 8);
    assert!(core::mem::offset_of!(Descriptor, modifier) == 16);
    assert!(core::mem::offset_of!(Descriptor, width) == 24);
    assert!(core::mem::offset_of!(Descriptor, height) == 28);
    assert!(core::mem::offset_of!(Descriptor, format) == 32);
    assert!(core::mem::offset_of!(Descriptor, plane_count) == 36);
    assert!(core::mem::offset_of!(Descriptor, planes) == 40);
    assert!(core::mem::offset_of!(Descriptor, usage) == 104);
    assert!(core::mem::offset_of!(Descriptor, location) == 108);
    assert!(core::mem::offset_of!(Descriptor, binding) == 112);
    assert!(core::mem::size_of::<Backing>() == 56 && core::mem::align_of::<Backing>() == 8);
    assert!(core::mem::offset_of!(Backing, cookie) == 0);
    assert!(core::mem::offset_of!(Backing, cpu_address) == 8);
    assert!(core::mem::offset_of!(Backing, bytes) == 16);
    assert!(core::mem::offset_of!(Backing, cache) == 24);
    assert!(core::mem::offset_of!(Backing, has_driver) == 28);
    assert!(core::mem::offset_of!(Backing, driver) == 32);
    assert!(core::mem::size_of::<Create>() == 40 && core::mem::align_of::<Create>() == 8);
    assert!(core::mem::offset_of!(Create, buffer) == 0);
    assert!(core::mem::offset_of!(Create, reference) == 16);
    assert!(core::mem::offset_of!(Create, bytes) == 32);
    assert!(core::mem::size_of::<Use>() == 112 && core::mem::align_of::<Use>() == 8);
    assert!(core::mem::offset_of!(Use, lease) == 0);
    assert!(core::mem::offset_of!(Use, buffer) == 16);
    assert!(core::mem::offset_of!(Use, access) == 32);
    assert!(core::mem::offset_of!(Use, backing) == 40);
    assert!(core::mem::offset_of!(Use, range) == 96);
    assert!(core::mem::size_of::<Release>() == 80 && core::mem::align_of::<Release>() == 8);
    assert!(core::mem::offset_of!(Release, buffer) == 0);
    assert!(core::mem::offset_of!(Release, backing) == 16);
    assert!(core::mem::offset_of!(Release, attempt) == 72);
    assert!(core::mem::size_of::<OwnedCreate>() == 88 && core::mem::align_of::<OwnedCreate>() == 8);
    assert!(core::mem::offset_of!(OwnedCreate, create) == 0);
    assert!(core::mem::offset_of!(OwnedCreate, cookie) == 40);
    assert!(core::mem::offset_of!(OwnedCreate, driver) == 48);
    assert!(core::mem::offset_of!(OwnedCreate, binding) == 72);
    assert!(
        core::mem::size_of::<OwnedRelease>() == 120 && core::mem::align_of::<OwnedRelease>() == 8
    );
    assert!(core::mem::offset_of!(OwnedRelease, release) == 0);
    assert!(core::mem::offset_of!(OwnedRelease, driver) == 80);
    assert!(core::mem::offset_of!(OwnedRelease, binding) == 104);
    assert!(
        core::mem::size_of::<OptionalRelease>() == 88
            && core::mem::align_of::<OptionalRelease>() == 8
    );
    assert!(core::mem::offset_of!(OptionalRelease, present) == 0);
    assert!(core::mem::offset_of!(OptionalRelease, value) == 8);
    assert!(
        core::mem::size_of::<OptionalOwnedRelease>() == 128
            && core::mem::align_of::<OptionalOwnedRelease>() == 8
    );
    assert!(core::mem::offset_of!(OptionalOwnedRelease, present) == 0);
    assert!(core::mem::offset_of!(OptionalOwnedRelease, value) == 8);
    assert!(
        core::mem::size_of::<BudgetSnapshot>() == 24
            && core::mem::align_of::<BudgetSnapshot>() == 8
    );
    assert!(core::mem::offset_of!(BudgetSnapshot, limit) == 0);
    assert!(core::mem::offset_of!(BudgetSnapshot, charged) == 8);
    assert!(core::mem::offset_of!(BudgetSnapshot, closing) == 16);
    assert!(core::mem::size_of::<Stats>() == 120 && core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, objects) == 0);
    assert!(core::mem::offset_of!(Stats, references) == 8);
    assert!(core::mem::offset_of!(Stats, leases) == 16);
    assert!(core::mem::offset_of!(Stats, bytes) == 24);
    assert!(core::mem::offset_of!(Stats, retained_bytes) == 32);
    assert!(core::mem::offset_of!(Stats, system_bytes) == 40);
    assert!(core::mem::offset_of!(Stats, device_bytes) == 48);
    assert!(core::mem::offset_of!(Stats, system_backed_bytes) == 56);
    assert!(core::mem::offset_of!(Stats, device_backed_bytes) == 64);
    assert!(core::mem::offset_of!(Stats, system_pinned_bytes) == 72);
    assert!(core::mem::offset_of!(Stats, device_pinned_bytes) == 80);
    assert!(core::mem::offset_of!(Stats, scanout_pinned_bytes) == 88);
    assert!(core::mem::offset_of!(Stats, device_mapped_bytes) == 96);
    assert!(core::mem::offset_of!(Stats, allocating_bytes) == 104);
    assert!(core::mem::offset_of!(Stats, destroying_bytes) == 112);
    assert!(core::mem::size_of::<Object>() == 312 && core::mem::align_of::<Object>() == 8);
    assert!(core::mem::offset_of!(Object, phase) == 0);
    assert!(core::mem::offset_of!(Object, handle) == 8);
    assert!(core::mem::offset_of!(Object, producer) == 24);
    assert!(core::mem::offset_of!(Object, producer_open) == 48);
    assert!(core::mem::offset_of!(Object, descriptor) == 56);
    assert!(core::mem::offset_of!(Object, allocation_bytes) == 184);
    assert!(core::mem::offset_of!(Object, has_backing) == 192);
    assert!(core::mem::offset_of!(Object, backing) == 200);
    assert!(core::mem::offset_of!(Object, references) == 256);
    assert!(core::mem::offset_of!(Object, leases) == 260);
    assert!(core::mem::offset_of!(Object, scanout_leases) == 264);
    assert!(core::mem::offset_of!(Object, mapping_leases) == 268);
    assert!(core::mem::offset_of!(Object, release_attempt) == 272);
    assert!(core::mem::offset_of!(Object, owned_cookie) == 280);
    assert!(core::mem::offset_of!(Object, owned_reference) == 288);
    assert!(core::mem::offset_of!(Object, device_lost) == 304);
    assert!(core::mem::offset_of!(Object, device_quiesced) == 305);
    assert!(core::mem::size_of::<Reference>() == 64 && core::mem::align_of::<Reference>() == 8);
    assert!(core::mem::offset_of!(Reference, handle) == 0);
    assert!(core::mem::offset_of!(Reference, buffer) == 16);
    assert!(core::mem::offset_of!(Reference, owner) == 32);
    assert!(core::mem::offset_of!(Reference, read_only) == 56);
    assert!(core::mem::offset_of!(Reference, mapping_only) == 57);
    assert!(core::mem::size_of::<Lease>() == 80 && core::mem::align_of::<Lease>() == 8);
    assert!(core::mem::offset_of!(Lease, handle) == 0);
    assert!(core::mem::offset_of!(Lease, buffer) == 16);
    assert!(core::mem::offset_of!(Lease, owner) == 32);
    assert!(core::mem::offset_of!(Lease, access) == 56);
    assert!(core::mem::offset_of!(Lease, range) == 64);
    assert!(
        core::mem::size_of::<DeviceBudget>() == 64 && core::mem::align_of::<DeviceBudget>() == 8
    );
    assert!(core::mem::offset_of!(DeviceBudget, present) == 0);
    assert!(core::mem::offset_of!(DeviceBudget, driver) == 8);
    assert!(core::mem::offset_of!(DeviceBudget, binding) == 32);
    assert!(core::mem::offset_of!(DeviceBudget, bytes) == 48);
    assert!(core::mem::offset_of!(DeviceBudget, closing) == 56);
    assert!(core::mem::size_of::<RawView>() == 104 && core::mem::align_of::<RawView>() == 8);
    assert!(core::mem::offset_of!(RawView, objects) == 0);
    assert!(core::mem::offset_of!(RawView, objects_len) == 8);
    assert!(core::mem::offset_of!(RawView, references) == 16);
    assert!(core::mem::offset_of!(RawView, references_len) == 24);
    assert!(core::mem::offset_of!(RawView, leases) == 32);
    assert!(core::mem::offset_of!(RawView, leases_len) == 40);
    assert!(core::mem::offset_of!(RawView, serial) == 48);
    assert!(core::mem::offset_of!(RawView, committed_bytes) == 56);
    assert!(core::mem::offset_of!(RawView, budget_bytes) == 64);
    assert!(core::mem::offset_of!(RawView, producer_budget_bytes) == 72);
    assert!(core::mem::offset_of!(RawView, device_budgets) == 80);
    assert!(core::mem::offset_of!(RawView, device_budgets_len) == 88);
    assert!(core::mem::offset_of!(RawView, dynamic_mask) == 96);
    assert!(core::mem::size_of::<Args>() == 152 && core::mem::align_of::<Args>() == 8);
    assert!(core::mem::offset_of!(Args, owner) == 0);
    assert!(core::mem::offset_of!(Args, other) == 24);
    assert!(core::mem::offset_of!(Args, handle) == 48);
    assert!(core::mem::offset_of!(Args, binding) == 64);
    assert!(core::mem::offset_of!(Args, offset) == 80);
    assert!(core::mem::offset_of!(Args, bytes) == 88);
    assert!(core::mem::offset_of!(Args, value) == 96);
    assert!(core::mem::offset_of!(Args, flag) == 104);
    assert!(core::mem::offset_of!(Args, access) == 108);
    assert!(core::mem::offset_of!(Args, pool) == 112);
    assert!(core::mem::offset_of!(Args, input) == 120);
    assert!(core::mem::offset_of!(Args, other_input) == 128);
    assert!(core::mem::offset_of!(Args, buffer) == 136);
    assert!(core::mem::offset_of!(Args, len) == 144);
};
