//! Original R4DRAW memory bridge; no rendering or physical placement policy.
use super::{
    buffers,
    layout::{self, Binding, Descriptor, Plane},
    stats_wire,
    types::{Error, Handle, Owner},
};
use crate::contract as a;
use core::{ffi::c_void, mem::size_of};
pub fn status(error: Error) -> i32 {
    match error {
        Error::Invalid | Error::WrongOwner => a::gfx_buffer_error_invalid,
        Error::Overflow | Error::Exhausted => a::gfx_buffer_error_overflow,
        Error::Unsupported => a::gfx_buffer_error_unsupported,
        Error::OutOfMemory => a::gfx_buffer_error_oom,
        Error::Budget => a::gfx_buffer_error_budget,
        Error::Capacity => a::gfx_buffer_error_capacity,
        Error::Stale => a::gfx_buffer_error_stale,
        Error::Busy => a::gfx_buffer_error_busy,
        Error::Closed => a::gfx_buffer_error_closed,
        Error::Unavailable => a::gfx_buffer_error_unavailable,
        Error::WaitTimeout => a::gfx_queue_error_wait_timeout,
        Error::WaitCancelled => a::gfx_queue_error_wait_cancelled,
    }
}
pub fn handle(value: a::GfxBufferHandle) -> Result<Handle, Error> {
    if value.id == 0 || value.reserved0 != 0 || value.generation == 0 {
        return Err(Error::Invalid);
    }
    Ok(Handle {
        id: value.id,
        generation: value.generation,
    })
}
pub fn public_handle(value: Handle) -> a::GfxBufferHandle {
    a::GfxBufferHandle {
        id: value.id,
        generation: value.generation,
        reserved0: 0,
    }
}
/// # Safety
/// Non-null pointer retains at least its initialized eight-byte contract header.
/// A complete T is accessed only after this capacity check succeeds.
pub(crate) unsafe fn valid_output<T>(output: *const T) -> bool {
    !output.is_null()
        && unsafe {
            output.cast::<u32>().read() == 1
                && output.cast::<u32>().add(1).read() >= size_of::<T>() as u32
        }
}
/// # Safety
/// Same header contract as valid_output; this API additionally checks alignment.
pub(crate) unsafe fn valid_aligned<T>(value: *const T) -> bool {
    value.is_aligned() && unsafe { valid_output(value) }
}
/// # Safety
/// Caller holds the original exact BO metadata owner throughout the query.
pub(crate) unsafe fn reference_locked(
    reference: Handle,
    owner: Owner,
) -> Result<a::GfxBufferReference, Error> {
    let table = unsafe { buffers::read() };
    Ok(a::GfxBufferReference {
        buffer: public_handle(table.buffer_for(reference, owner)?),
        reference: public_handle(reference),
        flags: if table.read_only(reference, owner)? {
            a::gfx_buffer_reference_immutable
        } else {
            0
        } | if table.mapping_only(reference, owner)? {
            a::gfx_buffer_reference_mapping_only
        } else {
            0
        },
        ..a::GfxBufferReference::default()
    })
}
pub fn descriptor(v: a::GfxBufferDescriptor) -> Result<Descriptor, Error> {
    if v.version != 1 || v.size < size_of::<a::GfxBufferDescriptor>() as u32 || v.reserved0 != 0 {
        return Err(Error::Invalid);
    }
    if !matches!(
        v.format,
        layout::BYTES
            | layout::XRGB8888
            | layout::ARGB8888
            | layout::XRGB2101010
            | layout::ARGB2101010
            | layout::ABGR16161616F
            | layout::R8
            | layout::NV12
            | layout::P010
    ) || v.location > 1
    {
        return Err(Error::Unsupported);
    }
    let mut planes = [Plane::default(); 4];
    for (i, p) in planes.iter_mut().enumerate() {
        *p = Plane {
            offset: v.plane_offsets[i],
            pitch: v.plane_pitches[i],
        };
    }
    Ok(Descriptor {
        bytes: v.byte_length,
        alignment: v.alignment,
        modifier: v.modifier,
        width: v.width,
        height: v.height,
        format: v.format,
        plane_count: v.plane_count,
        planes,
        usage: v.usage,
        location: v.location,
        binding: Binding {
            adapter: v.adapter_id,
            driver_owner: v.driver_owner,
            device_generation: v.device_generation,
        },
    })
}
pub fn public_descriptor(v: Descriptor) -> a::GfxBufferDescriptor {
    let mut result = a::GfxBufferDescriptor {
        byte_length: v.bytes,
        alignment: v.alignment,
        modifier: v.modifier,
        width: v.width,
        height: v.height,
        format: v.format,
        plane_count: v.plane_count,
        usage: v.usage,
        location: v.location,
        adapter_id: v.binding.adapter,
        driver_owner: v.binding.driver_owner,
        device_generation: v.binding.device_generation,
        ..a::GfxBufferDescriptor::default()
    };
    for (i, p) in v.planes.iter().enumerate() {
        result.plane_offsets[i] = p.offset;
        result.plane_pitches[i] = p.pitch;
    }
    result
}
pub struct Locked;
impl Locked {
    pub fn acquire() -> Self {
        buffers::lock();
        Self
    }
}
impl Drop for Locked {
    fn drop(&mut self) {
        buffers::unlock();
    }
}
pub(crate) unsafe fn create(
    owner: Owner,
    input: *const a::GfxBufferDescriptor,
    output: *mut a::GfxBufferReference,
) -> Result<(), Error> {
    if input.is_null() || !unsafe { valid_output(output) } {
        return Err(Error::Invalid);
    }
    let _call = buffers::Call::acquire()?;
    let d = descriptor(unsafe { input.read() })?;
    let c = buffers::create(owner, d)?;
    unsafe {
        output.write(a::GfxBufferReference {
            buffer: public_handle(c.buffer),
            reference: public_handle(c.reference),
            ..a::GfxBufferReference::default()
        });
    }
    Ok(())
}
pub(crate) unsafe fn describe(
    owner: Owner,
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxBufferDescriptor,
) -> Result<(), Error> {
    if input.is_null() || !unsafe { valid_output(output) } {
        return Err(Error::Invalid);
    }
    let h = handle(unsafe { input.read() })?;
    let value = {
        let _locked = Locked::acquire();
        public_descriptor(unsafe { buffers::read().describe(h, owner) }?)
    };
    unsafe {
        output.write(value);
    }
    Ok(())
}
pub(crate) unsafe fn import(
    owner: Owner,
    input: *const a::GfxBufferHandle,
    output: *mut a::GfxBufferReference,
) -> Result<(), Error> {
    if input.is_null() || !unsafe { valid_output(output) } {
        return Err(Error::Invalid);
    }
    let _call = buffers::Call::acquire()?;
    let h = handle(unsafe { input.read() })?;
    buffers::lock_prepared(buffers::Need {
        references: 1,
        ..buffers::Need::default()
    })?;
    let value = {
        let _locked = Locked;
        let reference = unsafe { buffers::table().share(h, owner) }?;
        unsafe { reference_locked(reference, owner) }.expect("Original imported reference")
    };
    unsafe {
        output.write(value);
    }
    Ok(())
}
pub(crate) unsafe fn release(owner: Owner, input: *const a::GfxBufferHandle) -> Result<(), Error> {
    if input.is_null() {
        return Err(Error::Invalid);
    }
    buffers::drop_reference(handle(unsafe { input.read() })?, owner)
}
pub(crate) unsafe fn map(
    owner: Owner,
    input: *const a::GfxBufferHandle,
    access: u32,
    offset: u64,
    bytes: u64,
    output: *mut a::GfxBufferMap,
    persistent: bool,
) -> Result<(), Error> {
    if input.is_null() || !unsafe { valid_output(output) } || access > a::gfx_buffer_map_write {
        return Err(Error::Invalid);
    }
    let _call = buffers::Call::acquire()?;
    let h = handle(unsafe { input.read() })?;
    buffers::lock_prepared(buffers::Need {
        leases: 1,
        ..buffers::Need::default()
    })?;
    let value = {
        let _locked = Locked;
        let mode = if persistent {
            if access == a::gfx_buffer_map_read {
                8
            } else {
                9
            }
        } else if access == a::gfx_buffer_map_read {
            0
        } else {
            1
        };
        unsafe { buffers::map_locked(h, owner, mode, offset, bytes) }?
    };
    unsafe {
        output.write(a::GfxBufferMap {
            lease: public_handle(value.lease),
            cpu_address: value.backing.cpu_address + offset,
            byte_length: bytes,
            cache_policy: value.backing.cache,
            ..a::GfxBufferMap::default()
        });
    }
    Ok(())
}
pub(crate) unsafe fn unmap(owner: Owner, input: *const a::GfxBufferHandle) -> Result<(), Error> {
    if input.is_null() {
        return Err(Error::Invalid);
    }
    let h = handle(unsafe { input.read() })?;
    let _call = buffers::Call::acquire()?;
    {
        let _locked = Locked::acquire();
        unsafe { buffers::unmap_cpu_locked(h, owner) }?;
    }
    buffers::collect();
    Ok(())
}
pub(crate) unsafe fn stats(output: *mut a::GfxBufferStats) -> Result<(), Error> {
    let count = unsafe { stats_wire::r4os_rust_gfx_stats_capacity(output) };
    if count == 0 {
        return Err(Error::Invalid);
    }
    let value = {
        let _locked = Locked::acquire();
        let table = unsafe { buffers::read() };
        let s = table.stats();
        a::GfxBufferStats {
            objects: u32::try_from(s.objects).expect("Original BO object count"),
            references: u32::try_from(s.references).expect("Original BO reference count"),
            leases: u32::try_from(s.leases).expect("Original BO lease count"),
            committed_bytes: s.bytes,
            retained_bytes: s.retained_bytes,
            budget_bytes: table.total_budget_bytes(),
            producer_budget_bytes: buffers::PRODUCER_BUDGET,
            system_bytes: s.system_bytes,
            device_bytes: s.device_bytes,
            system_backed_bytes: s.system_backed_bytes,
            device_backed_bytes: s.device_backed_bytes,
            system_pinned_bytes: s.system_pinned_bytes,
            device_pinned_bytes: s.device_pinned_bytes,
            scanout_pinned_bytes: s.scanout_pinned_bytes,
            device_mapped_bytes: s.device_mapped_bytes,
            allocating_bytes: s.allocating_bytes,
            destroying_bytes: s.destroying_bytes,
            ..a::GfxBufferStats::default()
        }
    };
    unsafe {
        stats_wire::r4os_rust_gfx_stats_write(output, count, &value);
    }
    Ok(())
}
/// # Safety
/// Args selects exact resident input/output contract records. Caller retains its
/// admitted owner throughout blocking allocation; no metadata borrow survives it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_buffer_api(
    op: u32,
    args: *const super::buffer_owner::Args,
    out: *mut c_void,
) -> i32 {
    let a = unsafe { args.read() };
    let result = unsafe {
        match op {
            0 => create(a.owner, a.input.cast(), out.cast()),
            1 => describe(a.owner, a.input.cast(), out.cast()),
            2 => import(a.owner, a.input.cast(), out.cast()),
            3 => release(a.owner, a.input.cast()),
            4 | 5 => map(
                a.owner,
                a.input.cast(),
                a.access,
                a.offset,
                a.bytes,
                out.cast(),
                op == 5,
            ),
            6 => unmap(a.owner, a.input.cast()),
            7 => stats(out.cast()),
            _ => panic!("Invalid BO API operation"),
        }
    };
    match result {
        Ok(()) => a::gfx_buffer_result_ok,
        Err(e) => status(e),
    }
}
/// # Safety
/// Input and output are complete initialized/readable and writable resident
/// fixed C records. They are disjoint from mutable metadata; query op requires
/// the exact original metadata owner. Helpers return private error transport.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_buffer_projection(
    op: u32,
    args: *const super::buffer_owner::Args,
    out: *mut c_void,
) -> i32 {
    let a = unsafe { args.read() };
    let result = (|| -> Result<(), Error> {
        unsafe {
            match op {
                0 => out
                    .cast::<Handle>()
                    .write(handle(a.input.cast::<a::GfxBufferHandle>().read())?),
                1 => out
                    .cast::<Descriptor>()
                    .write(descriptor(a.input.cast::<a::GfxBufferDescriptor>().read())?),
                2 => out
                    .cast::<a::GfxBufferDescriptor>()
                    .write(public_descriptor(a.input.cast::<Descriptor>().read())),
                3 => out
                    .cast::<a::GfxBufferReference>()
                    .write(reference_locked(a.handle, a.owner)?),
                _ => panic!("Invalid BO projection operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gfx_buffer_status(code: i32) -> i32 {
    match code {
        1 | 10 => a::gfx_buffer_error_invalid,
        2 | 4 => a::gfx_buffer_error_overflow,
        3 => a::gfx_buffer_error_unsupported,
        5 => a::gfx_buffer_error_budget,
        6 => a::gfx_buffer_error_capacity,
        7 => a::gfx_buffer_error_stale,
        8 => a::gfx_buffer_error_busy,
        9 => a::gfx_buffer_error_closed,
        11 => a::gfx_buffer_error_oom,
        _ => panic!("Invalid private BO error"),
    }
}

/// # Safety
/// At least eight initialized header bytes at a non-null pointer, as required
/// by the original generic caller. Full record storage is not borrowed here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_buffer_valid_header(
    value: *const c_void,
    expected: usize,
) -> bool {
    !value.is_null()
        && unsafe {
            value.cast::<u32>().read() == 1
                && value.cast::<u32>().add(1).read() as usize >= expected
        }
}
