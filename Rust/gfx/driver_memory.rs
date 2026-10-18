//! Original authenticated driver/device backing and MMIO lifetime runtime.
//! Stable resident records retain exact self/state identities across unlocked
//! allocation/page walks. Only Init/Shutdown's existing guard admits MMIO.
use super::{
    buffer_api as api, buffer_owner as bo, buffers, driver_memory_state as model,
    types::{Error, Owner},
};
use crate::{
    arch::runtime,
    boot::info,
    contract as a,
    driver::{mmio_windows as windows, registry::MAX_DRIVERS, treap::Tree},
    memory::{heap, paging},
    platform::cpu,
    sched::scheduler,
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{align_of, size_of},
    ptr,
};
struct Cell<T>(UnsafeCell<T>);
// SAFETY: exact epoch/device metadata uses the shared BO owner (or the
// original pre-task IF exclusion). MMIO has its original driver execution guard.
unsafe impl<T> Sync for Cell<T> {}
const EMPTY_EPOCH: model::Epoch = model::Epoch {
    identity: Owner {
        kind: 2,
        id: 0,
        generation: 0,
    },
    closing: false,
    mmio_busy: false,
    mmio_retained: false,
    mmio_pending: false,
    devices: 0,
};
struct State {
    epochs: [model::Epoch; MAX_DRIVERS],
    devices: Tree,
}
static STATE: Cell<State> = Cell(UnsafeCell::new(State {
    epochs: [EMPTY_EPOCH; MAX_DRIVERS],
    devices: Tree {
        root: ptr::null_mut(),
        xorshift: 0,
    },
}));
unsafe fn state() -> model::View {
    unsafe {
        let s = STATE.0.get();
        model::View {
            epochs: ptr::addr_of_mut!((*s).epochs).cast(),
            capacity: MAX_DRIVERS,
            devices: ptr::addr_of_mut!((*s).devices),
            identity: s.addr(),
        }
    }
}
struct Lifecycle(Option<u64>);
impl Lifecycle {
    fn acquire() -> Self {
        if scheduler::current().is_null() {
            let flags = runtime::save_local();
            assert!(!runtime::enabled());
            Self(Some(flags))
        } else {
            buffers::lock();
            Self(None)
        }
    }
}
impl Drop for Lifecycle {
    fn drop(&mut self) {
        if let Some(flags) = self.0 {
            runtime::restore_local(flags);
        } else {
            buffers::unlock();
        }
    }
}
pub fn bind(id: u32, generation: u64) -> bool {
    let _owner = Lifecycle::acquire();
    unsafe { state().bind(id, generation) }
}
pub fn discard_empty_binding(id: u32, generation: u64) -> bool {
    let _owner = Lifecycle::acquire();
    unsafe {
        state().retire(Owner {
            kind: 2,
            id: u64::from(id),
            generation,
        })
    }
}
pub fn owner(id: u32, admission: bool) -> Result<Owner, Error> {
    let _owner = Lifecycle::acquire();
    unsafe { state().owner(id, admission) }
}
/// # Safety
/// The original shared BO owner remains held through admission/publication.
pub(crate) unsafe fn admit_locked(identity: Owner) -> Result<(), Error> {
    if identity.kind != 2 || identity.id > u64::from(u32::MAX) {
        return Err(Error::WrongOwner);
    }
    let current = unsafe { state().owner(identity.id as u32, true) }?;
    if current != identity {
        return Err(Error::Stale);
    }
    Ok(())
}
pub fn begin_close(id: u32) {
    {
        let _owner = Lifecycle::acquire();
        unsafe {
            let view = state();
            let e = view.find(u64::from(id));
            if !e.is_null() {
                (*e).closing = true;
            }
        }
    }
    if !scheduler::current().is_null() {
        super::allocations::closing_driver(id);
        super::virtual_runtime::closing_driver(id);
        super::telemetry_api::close_driver(id);
    }
}
pub fn retained(id: u32) -> bool {
    let held = {
        let _owner = Lifecycle::acquire();
        let Ok(identity) = (unsafe { state().owner(id, false) }) else {
            return false;
        };
        unsafe { state().retained(identity, false) }
    };
    held || (!scheduler::current().is_null()
        && (buffers::retains_driver(id)
            || super::allocations::retains_driver(id)
            || super::virtual_runtime::retains_driver(id)))
}
pub fn finish_owner(id: u32) {
    let Ok(identity) = owner(id, false) else {
        return;
    };
    if !scheduler::current().is_null() {
        buffers::stopped(identity);
    }
    let _owner = Lifecycle::acquire();
    assert!(unsafe { state().retire(identity) });
}
fn free_device(record: *mut model::Device) {
    assert!(unsafe { heap::r4os_rust_heap_free(record.cast(), size_of::<model::Device>()) } == 0);
}
struct DeviceAllocation {
    record: *mut model::Device,
    published: bool,
}
impl Drop for DeviceAllocation {
    fn drop(&mut self) {
        if !self.published {
            free_device(self.record);
        }
    }
}
pub(crate) unsafe fn acquire(
    identity: Owner,
    reference: *const a::GfxBufferHandle,
    request_ptr: *const a::GfxDeviceRequest,
    output: *mut a::GfxDeviceLease,
) -> Result<(), Error> {
    unsafe {
        if reference.is_null() || request_ptr.is_null() || !api::valid_output(output) {
            return Err(Error::Invalid);
        }
        let request = request_ptr.read();
        let reference = api::handle(reference.read())?;
        if request.version != 1
            || request.size < size_of::<a::GfxDeviceRequest>() as u32
            || request.reserved0 != 0
            || request.adapter_id == 0
            || request.device_generation == 0
            || request.access > 4
            || request.address_space > 1
            || (request.address_space == 0
                && (request.gpu_virtual_address != 0 || request.access == 3))
            || (request.address_space == 1
                && (request.gpu_virtual_address == 0
                    || request.gpu_virtual_address > u64::MAX - request.byte_length))
            || (request.access == 4 && request.address_space != 0)
            || (request.access == 3
                && (request.byte_offset | request.gpu_virtual_address) & 4095 != 0)
        {
            return Err(Error::Invalid);
        }
        let _call = buffers::Call::acquire()?;
        let record =
            heap::r4os_rust_heap_alloc(size_of::<model::Device>(), align_of::<model::Device>())
                .cast::<model::Device>();
        if record.is_null() {
            return Err(Error::OutOfMemory);
        }
        record.write(model::Device::default());
        let mut allocation = DeviceAllocation {
            record,
            published: false,
        };
        buffers::lock_prepared(buffers::Need {
            leases: 1,
            ..buffers::Need::default()
        })?;
        let (value, descriptor) = {
            let _lock = api::Locked;
            let desc = buffers::read().describe(reference, identity)?;
            if !desc.binding.portable()
                && (desc.binding.adapter != request.adapter_id
                    || u64::from(desc.binding.driver_owner) != identity.id
                    || desc.binding.device_generation != request.device_generation)
            {
                return Err(Error::Stale);
            }
            if request.access == 3
                && !model::mapping_range(
                    request.byte_offset,
                    request.byte_length,
                    request.gpu_virtual_address,
                    desc.bytes,
                )
            {
                return Err(Error::Invalid);
            }
            let access = match request.access {
                0 => 2,
                1 => 3,
                2 => 4,
                3 | 4 => 5,
                _ => unreachable!(),
            };
            let value = buffers::map_locked(
                reference,
                identity,
                access,
                request.byte_offset,
                request.byte_length,
            )?;
            let d = a::GfxDeviceLease {
                lease: api::public_handle(value.lease),
                byte_offset: request.byte_offset,
                byte_length: request.byte_length,
                gpu_virtual_address: request.gpu_virtual_address,
                device_generation: request.device_generation,
                adapter_id: request.adapter_id,
                driver_owner: u32::try_from(identity.id).expect("Original driver ID"),
                access: request.access,
                address_space: request.address_space,
                dma_mask: request.dma_mask,
                ..a::GfxDeviceLease::default()
            };
            if let Err(e) = state().reserve(identity, d, record) {
                buffers::table()
                    .end_use(value.lease, identity, true)
                    .expect("Original failed device reservation");
                return Err(e);
            }
            allocation.published = true;
            (value, d)
        };
        if request.address_space == 0 {
            let mut offset = 0;
            while offset < request.byte_length {
                match dma_segment(value, offset, request.dma_mask) {
                    Ok(piece) => offset = piece.next_offset,
                    Err(e) => {
                        {
                            let _lock = api::Locked::acquire();
                            buffers::table()
                                .end_use(value.lease, identity, true)
                                .expect("Original device DMA rollback");
                            state().detach(record).expect("Original device detach");
                            allocation.published = false;
                        }
                        buffers::collect();
                        return Err(e);
                    }
                }
            }
        }
        {
            let _lock = api::Locked::acquire();
            (*record).busy = false;
        }
        output.write(descriptor);
        Ok(())
    }
}
unsafe fn lease_value(input: *const a::GfxDeviceLease) -> Option<a::GfxDeviceLease> {
    if !unsafe { api::valid_output(input) } {
        return None;
    }
    let mut normalized = unsafe { input.read() };
    normalized.size = size_of::<a::GfxDeviceLease>() as u32;
    Some(normalized)
}
pub fn dma_segment(value: bo::Use, offset: u64, mask: u64) -> Result<a::GfxDmaSegment, Error> {
    if offset >= value.range.bytes
        || value.backing.cpu_address == 0
        || value.backing.cache != 1
        || value.backing.has_driver
    {
        return Err(Error::Unsupported);
    }
    let address = value.backing.cpu_address + value.range.offset + offset;
    let inside = address % 4096;
    let mut physical = 0;
    if !unsafe { paging::r4os_rust_paging_read(address - inside, 0, false, &mut physical) } {
        return Err(Error::Stale);
    }
    let bytes = (4096 - inside).min(value.range.bytes - offset);
    let dma = physical.checked_add(inside).ok_or(Error::Overflow)?;
    if dma > mask || bytes - 1 > mask - dma {
        return Err(Error::Unsupported);
    }
    Ok(a::GfxDmaSegment {
        dma_address: dma,
        byte_length: bytes,
        next_offset: offset + bytes,
        ..a::GfxDmaSegment::default()
    })
}
pub(crate) unsafe fn segment(
    identity: Owner,
    input: *const a::GfxDeviceLease,
    offset: u64,
    output: *mut a::GfxDmaSegment,
) -> Result<(), Error> {
    unsafe {
        if !api::valid_output(output) {
            return Err(Error::Invalid);
        }
        let d = lease_value(input).ok_or(Error::Stale)?;
        let value = {
            let _lock = api::Locked::acquire();
            let record = state().matching(identity, d)?;
            if (*record).descriptor.address_space != 0 {
                return Err(Error::Unsupported);
            }
            let value = buffers::read().use_info(
                api::handle((*record).descriptor.lease).expect("Original lease identity"),
                identity,
            )?;
            dma_segment(value, offset, (*record).descriptor.dma_mask)?
        };
        output.write(value);
        Ok(())
    }
}
pub(crate) unsafe fn release(
    identity: Owner,
    input: *const a::GfxDeviceLease,
    quiesced: u32,
) -> Result<(), Error> {
    unsafe {
        let d = lease_value(input).ok_or(Error::Stale)?;
        let _call = buffers::Call::acquire()?;
        let record = {
            let _lock = api::Locked::acquire();
            let record = state().matching(identity, d)?;
            if quiesced != 1 {
                return Err(if quiesced == 0 {
                    Error::Busy
                } else {
                    Error::Invalid
                });
            }
            buffers::visibility();
            buffers::table().end_use(
                api::handle((*record).descriptor.lease).expect("Original lease identity"),
                identity,
                true,
            )?;
            state().detach(record).expect("Original device detach");
            record
        };
        free_device(record);
        buffers::collect();
        Ok(())
    }
}
const EMPTY_WINDOW: windows::Window = windows::Window {
    handle: windows::Handle {
        generation: 0,
        id: 0,
    },
    cpu: 0,
    physical: 0,
    bytes: 0,
    policy: 0,
    borrowed: false,
};
const EMPTY_MMIO: windows::Record = windows::Record {
    owner: windows::Owner {
        id: 0,
        generation: 0,
        kind: 2,
    },
    window: EMPTY_WINDOW,
    mapped: 0,
    used: false,
    retiring: false,
};
static MMIO: Cell<windows::State> = Cell(UnsafeCell::new(windows::State {
    records: [EMPTY_MMIO; 64],
    serial: 0,
    charged: 0,
    budget: 1024 * 1024 * 1024,
}));
const BACKEND: windows::Backend = windows::Backend {
    context: ptr::null_mut(),
    call: backend,
};
fn window_owner(identity: Owner) -> windows::Owner {
    windows::Owner {
        id: identity.id,
        generation: identity.generation,
        kind: u8::try_from(identity.kind).expect("Original owner kind"),
    }
}
unsafe extern "C" fn backend(
    _: *mut c_void,
    op: u8,
    address: u64,
    bytes: u64,
    policy: u8,
    out: *mut u64,
) -> bool {
    unsafe {
        let boot = &*info::r4os_rust_boot_info();
        match op {
            0 => {
                for i in 0..boot.memory_count {
                    let entry = boot.memory.add(i).read();
                    if !entry.valid || address >= entry.end || entry.base >= address + bytes {
                        continue;
                    }
                    if !matches!(entry.kind, 1 | 7) {
                        return false;
                    }
                }
                true
            }
            1 => {
                if boot.flags & 4 == 0 {
                    return false;
                }
                let Some(cpu) = address.checked_add(boot.hhdm) else {
                    return false;
                };
                if cpu.checked_add(bytes).is_none() {
                    return false;
                }
                out.write(cpu);
                true
            }
            2 => {
                let mut selector = 0;
                let mut physical = 0;
                if !paging::r4os_rust_paging_read(address, 1, false, &mut selector) {
                    return false;
                }
                if !paging::r4os_rust_paging_read(address, 2, false, &mut physical)
                    || boot.flags & 4 == 0
                    || address < boot.hhdm
                    || address - boot.hhdm != physical
                {
                    out.write(0);
                    return true;
                }
                let Some(memory_type) = crate::arch::pat::effective_at(selector as u8, physical)
                else {
                    return false;
                };
                out.write(match memory_type {
                    0 => 2,
                    1 => 1,
                    _ => 0,
                });
                true
            }
            3 => {
                let flags = match policy {
                    2 => {
                        if cpu::r4os_rust_cpu_control(0, 0) == 0 {
                            return false;
                        }
                        let Some(bits) =
                            crate::arch::pat::cache_bits(crate::arch::memory_types::UC, false)
                        else {
                            return false;
                        };
                        bits
                    }
                    1 => {
                        if cpu::r4os_rust_cpu_control(2, 0) == 0 {
                            return false;
                        }
                        let Some(bits) =
                            crate::arch::pat::cache_bits(crate::arch::memory_types::WC, false)
                        else {
                            return false;
                        };
                        bits
                    }
                    _ => return false,
                };
                paging::r4os_rust_paging_map(address, bytes, 2 | (1 << 63) | flags, false)
            }
            4 => paging::r4os_rust_paging_unmap(address, false),
            _ => panic!("Invalid graphics MMIO backend operation"),
        }
    }
}
fn window_error(code: u8) -> Result<(), Error> {
    match code {
        0 => Ok(()),
        1 => Err(Error::Invalid),
        2 => Err(Error::Overflow),
        3 => Err(Error::Unsupported),
        4 => Err(Error::Busy),
        5 => Err(Error::Capacity),
        6 => Err(Error::Stale),
        7 => Err(Error::Exhausted),
        8 => Err(Error::OutOfMemory),
        _ => panic!("Invalid MMIO result"),
    }
}
fn begin_mmio(identity: Owner) {
    let _lock = api::Locked::acquire();
    unsafe {
        let e = state().exact(identity).expect("Original exact MMIO epoch");
        assert!(!(*e).mmio_busy);
        (*e).mmio_busy = true;
    }
}
fn end_mmio(identity: Owner) {
    unsafe {
        let owner = window_owner(identity);
        let held = windows::r4os_rust_mmio_query(MMIO.0.get(), &BACKEND, &owner, 1);
        let pending = windows::r4os_rust_mmio_query(MMIO.0.get(), &BACKEND, &owner, 2);
        let _lock = api::Locked::acquire();
        let e = state().exact(identity).expect("Original exact MMIO epoch");
        assert!((*e).mmio_busy);
        (*e).mmio_busy = false;
        (*e).mmio_retained = held;
        (*e).mmio_pending = pending;
    }
}
struct Mmio(Owner);
impl Mmio {
    fn begin(identity: Owner) -> Self {
        begin_mmio(identity);
        Self(identity)
    }
}
impl Drop for Mmio {
    fn drop(&mut self) {
        end_mmio(self.0);
    }
}
pub(crate) unsafe fn map_window(
    identity: Owner,
    input: *const a::GfxMmioRequest,
    output: *mut a::GfxMmioWindow,
) -> Result<(), Error> {
    unsafe {
        if !api::valid_output(input)
            || (*input).resource_flags & !1 != 0
            || !api::valid_output(output)
        {
            return Err(Error::Invalid);
        }
        let input = input.read();
        let policy = match input.cache_policy {
            a::gfx_buffer_cache_uncached => 2,
            a::gfx_buffer_cache_write_combining => 1,
            _ => return Err(Error::Unsupported),
        };
        let _mmio = Mmio::begin(identity);
        let mut result = windows::Window::default();
        window_error(windows::r4os_rust_mmio_create(
            MMIO.0.get(),
            &BACKEND,
            &window_owner(identity),
            &windows::Request {
                resource_base: input.resource_base,
                resource_bytes: input.resource_bytes,
                offset: input.byte_offset,
                bytes: input.byte_length,
                policy,
                prefetchable: input.resource_flags & 1 != 0,
            },
            &mut result,
        ))?;
        output.write(a::GfxMmioWindow {
            handle: a::GfxBufferHandle {
                id: result.handle.id,
                generation: result.handle.generation,
                reserved0: 0,
            },
            cpu_address: result.cpu,
            physical_address: result.physical,
            byte_length: result.bytes,
            cache_policy: input.cache_policy,
            flags: u32::from(result.borrowed),
            ..a::GfxMmioWindow::default()
        });
        Ok(())
    }
}
pub(crate) unsafe fn unmap_window(
    identity: Owner,
    input: *const a::GfxBufferHandle,
    quiesced: u32,
) -> Result<(), Error> {
    unsafe {
        if input.is_null() || quiesced > 1 {
            return Err(Error::Invalid);
        }
        let h = api::handle(input.read())?;
        let _mmio = Mmio::begin(identity);
        window_error(windows::r4os_rust_mmio_release(
            MMIO.0.get(),
            &BACKEND,
            &window_owner(identity),
            &windows::Handle {
                generation: h.generation,
                id: h.id,
            },
            quiesced == 1,
        ))
    }
}
pub fn collect(identity: Owner) -> bool {
    begin_mmio(identity);
    buffers::collect();
    let released = unsafe {
        windows::r4os_rust_mmio_query(MMIO.0.get(), &BACKEND, &window_owner(identity), 0)
    };
    end_mmio(identity);
    released && !buffers::pending_release_for_owner(identity)
}
pub fn collect_buffers(identity: Owner) -> bool {
    buffers::collect();
    let pending = {
        let _lock = api::Locked::acquire();
        unsafe { state().retained(identity, true) }
    };
    !pending && !buffers::pending_release_for_owner(identity)
}
/// # Safety
/// Original caller admission retains Args and operation-specific exact C
/// inputs/output; admitted driver owner/code remains live through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_driver_memory_run(
    op: u32,
    args: *const bo::Args,
    out: *mut c_void,
) -> i32 {
    let a = unsafe { args.read() };
    let result = (|| -> Result<(), Error> {
        unsafe {
            match op {
                0 => out.cast::<bool>().write(bind(a.value as u32, a.offset)),
                1 => out
                    .cast::<bool>()
                    .write(discard_empty_binding(a.value as u32, a.offset)),
                2 => out.cast::<Owner>().write(owner(a.value as u32, a.flag)?),
                3 => admit_locked(a.owner)?,
                4 => begin_close(a.value as u32),
                5 => out.cast::<bool>().write(retained(a.value as u32)),
                6 => finish_owner(a.value as u32),
                7 => acquire(a.owner, a.input.cast(), a.other_input.cast(), out.cast())?,
                8 => segment(a.owner, a.input.cast(), a.offset, out.cast())?,
                9 => release(a.owner, a.input.cast(), a.value as u32)?,
                10 => map_window(a.owner, a.input.cast(), out.cast())?,
                11 => unmap_window(a.owner, a.input.cast(), a.value as u32)?,
                12 => {
                    if !collect(a.owner) {
                        return Err(Error::Busy);
                    }
                }
                13 => {
                    if !collect_buffers(a.owner) {
                        return Err(Error::Busy);
                    }
                }
                14 => out.cast::<a::GfxDmaSegment>().write(dma_segment(
                    a.input.cast::<bo::Use>().read(),
                    a.offset,
                    a.value,
                )?),
                _ => panic!("Invalid driver memory operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            if op < 7 || op == 14 {
                0
            } else {
                a::gfx_buffer_result_ok
            }
        }
        Err(e) => {
            if op < 7 || op == 14 {
                e as i32
            } else {
                api::status(e)
            }
        }
    }
}
