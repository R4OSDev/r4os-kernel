//! Sole productive output catalog/publication owner. All metadata admissions
//! use the existing program owner, ending before queue/device work or wakes.
use super::{
    backend_state as backend, equal::same, error::Error, framebuffer::Framebuffer,
    output_state as model, output_target, ownership, queue_state,
};
use crate::{
    contract as a,
    driver::irq_router,
    gfx::{buffer_api, buffers, types::Owner},
    platform::monotonic,
    service::desktop_events,
};
use core::{
    cell::UnsafeCell,
    mem::{MaybeUninit, size_of},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
struct Catalog(UnsafeCell<MaybeUninit<model::Store>>);
struct Cell<T>(UnsafeCell<T>);
// SAFETY: the established program-state owner serializes all catalog/epoch/
// port admission. No Rust metadata reference survives a foreign owner or wait.
unsafe impl Sync for Catalog {}
unsafe impl<T> Sync for Cell<T> {}
static CATALOG: Catalog = Catalog(UnsafeCell::new(MaybeUninit::uninit()));
static READY: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Copy)]
struct NativePort {
    owner: u32,
    identity: a::GfxOutputId,
    head: u32,
}
impl NativePort {
    const EMPTY: Self = Self {
        owner: 0,
        identity: a::GfxOutputId {
            adapter_id: 0,
            connector_id: 0,
            device_generation: 0,
            connection_generation: 0,
        },
        head: 0,
    };
}
struct Metadata {
    owner_epoch: u64,
    epoch_exhausted: bool,
    native_ports: [NativePort; a::gfx_output_max_assignments as usize],
}
static META: Cell<Metadata> = Cell(UnsafeCell::new(Metadata {
    owner_epoch: 1,
    epoch_exhausted: false,
    native_ports: [NativePort::EMPTY; a::gfx_output_max_assignments as usize],
}));
fn catalog() -> *mut model::Store {
    let out = CATALOG.0.get().cast::<model::Store>();
    if !READY.load(Ordering::Acquire) {
        unsafe { model::Store::init_in_place(out) };
        READY.store(true, Ordering::Release)
    }
    out
}
fn signal() {
    desktop_events::r4os_rust_desktop_signal()
}
fn same_port(left: a::GfxOutputId, right: a::GfxOutputId) -> bool {
    left.adapter_id == right.adapter_id
        && left.connector_id == right.connector_id
        && left.device_generation == right.device_generation
}
fn native_port_locked(owner: u32, identity: a::GfxOutputId) -> Option<usize> {
    if owner == 0 {
        return None;
    }
    unsafe { &*META.0.get() }
        .native_ports
        .iter()
        .position(|port| port.owner == owner && same_port(port.identity, identity))
}
fn native_presence(flags: u32, held: bool) -> bool {
    flags & a::gfx_output_flag_connected != 0
        || (held && flags & a::gfx_output_flag_connection_unknown != 0)
}

fn queue_validate(owner: u32, binding: a::GfxBackendBinding) -> Result<(), Error> {
    super::queue::validate_output_binding(owner, binding)
}
fn queue_wake(owner: u32, target: a::GfxOutputTarget) {
    let _ = super::queue::wake_output(owner, target);
}
fn refresh_now() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(0)
}
fn current_refresh_target(target: a::GfxOutputTarget) -> Result<(), Error> {
    let current = super::native_driver::output_target(target.adapter_id, target.head_id)
        .map_err(|_| Error::Stale)?;
    if !same(&current, &target) {
        return Err(Error::Stale);
    }
    Ok(())
}

pub fn init_boot(frame: &Framebuffer) {
    if frame.width == 0 || frame.height == 0 || frame.width > 65536 || frame.height > 65536 {
        return;
    }
    let mut data = [0u8; a::gfx_output_max_edid_bytes as usize];
    let mut bytes = 0;
    if !frame.edid.is_null()
        && frame.edid_size <= data.len() as u64
        && frame.edid_size.is_multiple_of(128)
    {
        bytes = frame.edid_size as usize;
        if bytes != 0 {
            unsafe { ptr::copy_nonoverlapping(frame.edid.cast::<u8>(), data.as_mut_ptr(), bytes) }
        }
    }
    let width = frame.width as u32;
    let height = frame.height as u32;
    let info = a::GfxOutputInfo {
        identity: a::GfxOutputId {
            connector_id: 1,
            device_generation: 1,
            ..a::GfxOutputId::default()
        },
        flags: a::gfx_output_flag_connected
            | a::gfx_output_flag_firmware_snapshot
            | a::gfx_output_flag_active
            | a::gfx_output_flag_fixed_geometry
            | a::gfx_output_flag_connection_unknown,
        connector_kind: a::gfx_output_kind_firmware,
        mode_count: 1,
        preferred_mode_id: 1,
        edid_bytes: bytes as u32,
        possible_heads: 1,
        possible_planes: 1,
        possible_plls: 1,
        limits: a::GfxDisplayLimits {
            head_mask: 1,
            plane_mask: 1,
            pll_mask: 1,
            max_width: width,
            max_height: height,
            ..a::GfxDisplayLimits::default()
        },
        ..a::GfxOutputInfo::default()
    };
    let mode = a::GfxOutputMode {
        mode_id: 1,
        flags: a::gfx_output_mode_geometry_only | a::gfx_output_mode_preferred,
        width,
        height,
        ..a::GfxOutputMode::default()
    };
    let _state = ownership::enter_state();
    let catalog = unsafe { &mut *catalog() };
    if catalog.revision != 0 {
        return;
    }
    let _ = catalog.publish(0, info, &[mode], &data[..bytes], false);
}
pub fn revision() -> a::GfxDisplayRevision {
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.cursor()
}
pub fn info_at(index: u32) -> Option<a::GfxOutputInfo> {
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.info_at(index)
}
pub fn color_at(identity: a::GfxOutputId) -> Result<a::GfxOutputColorState, Error> {
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.color_at(identity)
}
pub fn publish_color(owner: u32, value: a::GfxOutputColorState) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let changed = {
        let _state = ownership::enter_state();
        unsafe { &mut *catalog() }.publish_color(owner, value)?
    };
    if changed {
        signal()
    }
    Ok(())
}
pub fn refresh_at(target: a::GfxOutputTarget) -> Result<a::GfxOutputRefresh, Error> {
    current_refresh_target(target)?;
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.refresh_at(target)
}
pub fn publish_refresh(owner: u32, value: a::GfxOutputRefresh) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    if value.status.phase == a::gfx_refresh_phase_active {
        current_refresh_target(value.target)?
    }
    let changed = {
        let _state = ownership::enter_state();
        registered_head_locked(owner, port(value.target), value.target.head_id)?;
        unsafe { &mut *catalog() }.publish_refresh(owner, value)?
    };
    if changed {
        signal()
    }
    Ok(())
}
pub fn request_refresh(
    actor: Owner,
    input: a::GfxRefreshRequest,
) -> Result<a::GfxRefreshRequest, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    current_refresh_target(input.target)?;
    let now = refresh_now();
    let (owner, value) = {
        let _state = ownership::enter_state();
        let catalog = unsafe { &mut *catalog() };
        let index = catalog.refresh_entry(input.target)?;
        (
            catalog.entries[index].owner,
            catalog.request_refresh(actor, input, now)?,
        )
    };
    queue_wake(owner, input.target);
    Ok(value)
}
pub fn read_refresh(owner: u32, target: a::GfxOutputTarget) -> Result<a::GfxRefreshRequest, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let now = refresh_now();
    let _state = ownership::enter_state();
    unsafe { &mut *catalog() }.read_refresh(owner, target, now)
}
pub fn refresh_stopped(actor: Owner) {
    for index in 0..model::CAPACITY {
        let wake = {
            let _state = ownership::enter_state();
            let entry = unsafe { &mut (*catalog()).entries[index] };
            if entry.refresh.stopped(actor) && entry.refresh.has_value {
                Some((entry.owner, entry.refresh.value.target))
            } else {
                None
            }
        };
        if let Some((owner, target)) = wake {
            queue_wake(owner, target)
        }
    }
}
pub fn power_at(identity: a::GfxOutputId) -> Result<a::GfxOutputPower, Error> {
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.power_at(identity)
}
pub fn publish_power(owner: u32, value: a::GfxOutputPower) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let changed = {
        let _state = ownership::enter_state();
        if native_port_locked(owner, value.identity).is_none() {
            return Err(Error::Stale);
        }
        unsafe { &mut *catalog() }.publish_power(owner, value)?
    };
    if changed {
        signal()
    }
    Ok(())
}
fn wake_power(owner: u32, identity: a::GfxOutputId) {
    queue_wake(
        owner,
        a::GfxOutputTarget {
            adapter_id: identity.adapter_id,
            device_generation: identity.device_generation,
            connector_id: identity.connector_id,
            connection_generation: identity.connection_generation,
            ..a::GfxOutputTarget::default()
        },
    )
}
pub fn request_power(actor: Owner, input: a::GfxPowerRequest) -> Result<a::GfxPowerRequest, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let now = refresh_now();
    let (owner, value) = {
        let _state = ownership::enter_state();
        let catalog = unsafe { &mut *catalog() };
        let index = catalog.power_entry(input.identity)?;
        (
            catalog.entries[index].owner,
            catalog.request_power(actor, input, now)?,
        )
    };
    wake_power(owner, input.identity);
    Ok(value)
}
pub fn read_power(owner: u32, identity: a::GfxOutputId) -> Result<a::GfxPowerRequest, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let now = refresh_now();
    let _state = ownership::enter_state();
    unsafe { &mut *catalog() }.read_power(owner, identity, now)
}
pub fn power_stopped(actor: Owner) {
    for index in 0..model::CAPACITY {
        let wake = {
            let _state = ownership::enter_state();
            let entry = unsafe { &mut (*catalog()).entries[index] };
            if entry.power.stopped(actor) {
                Some((entry.owner, entry.info.identity))
            } else {
                None
            }
        };
        if let Some((owner, identity)) = wake {
            wake_power(owner, identity)
        }
    }
}
pub fn mode_at(identity: a::GfxOutputId, index: u32) -> Result<Option<a::GfxOutputMode>, Error> {
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.mode_at(identity, index)
}
pub fn edid_at(identity: a::GfxOutputId, index: u32) -> Result<Option<a::GfxEdidBlock>, Error> {
    let _state = ownership::enter_state();
    let catalog = unsafe { &*catalog() };
    let entry = &catalog.entries[catalog.find(identity)?];
    if index >= entry.info.edid_bytes / 128 {
        return Ok(None);
    }
    let mut result = a::GfxEdidBlock {
        identity,
        block_index: index,
        ..a::GfxEdidBlock::default()
    };
    result
        .data
        .copy_from_slice(&entry.edid[index as usize * 128..index as usize * 128 + 128]);
    Ok(Some(result))
}
fn port(target: a::GfxOutputTarget) -> a::GfxOutputId {
    a::GfxOutputId {
        adapter_id: target.adapter_id,
        connector_id: target.connector_id,
        device_generation: target.device_generation,
        connection_generation: target.connection_generation,
    }
}

/// # Safety
/// The admitted R4D retains the full immutable resident publication through the
/// synchronous call. Only bounded copied fields/spans are borrowed, and no
/// catalog or input reference is held while entering a foreign queue owner.
pub unsafe fn publish(
    owner: u32,
    input: *const a::GfxOutputPublication,
) -> Result<a::GfxOutputId, Error> {
    if owner == 0 || irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let version = unsafe { ptr::addr_of!((*input).version).read() };
    let size = unsafe { ptr::addr_of!((*input).size).read() };
    let initial = unsafe { ptr::addr_of!((*input).info).read() };
    let binding = unsafe { ptr::addr_of!((*input).backend).read() };
    if version != 1
        || (size as usize) < size_of::<a::GfxOutputPublication>()
        || initial.mode_count > a::gfx_output_max_modes
        || initial.edid_bytes > a::gfx_output_max_edid_bytes
    {
        return Err(Error::Invalid);
    }
    if initial.flags
        & (a::gfx_output_flag_firmware_snapshot
            | a::gfx_output_flag_active
            | a::gfx_output_flag_fixed_geometry)
        != 0
        || initial.identity.adapter_id != binding.adapter_id
        || initial.identity.device_generation != binding.device_generation
    {
        return Err(Error::Invalid);
    }
    let modeset = initial.limits.flags & a::gfx_output_limit_modeset != 0;
    if modeset && !super::native_driver::modes_enabled(owner, binding) {
        return Err(Error::Unsupported);
    }
    let modes = unsafe { ptr::addr_of!((*input).modes).cast::<a::GfxOutputMode>() };
    for i in initial.mode_count as usize..a::gfx_output_max_modes as usize {
        if !same(
            &unsafe { modes.add(i).read() },
            &a::GfxOutputMode::default(),
        ) {
            return Err(Error::Invalid);
        }
    }
    let edid = unsafe { ptr::addr_of!((*input).edid).cast::<u8>() };
    for i in initial.edid_bytes as usize..a::gfx_output_max_edid_bytes as usize {
        if unsafe { edid.add(i).read() } != 0 {
            return Err(Error::Invalid);
        }
    }
    let epoch = {
        let _state = ownership::enter_state();
        let meta = unsafe { &*META.0.get() };
        if meta.epoch_exhausted {
            return Err(Error::Exhausted);
        }
        meta.owner_epoch
    };
    queue_validate(owner, unsafe { ptr::addr_of!((*input).backend).read() })?;
    let identity = {
        let _state = ownership::enter_state();
        let meta = unsafe { &*META.0.get() };
        if meta.epoch_exhausted || meta.owner_epoch != epoch {
            return Err(Error::Stale);
        }
        let mut info = unsafe { ptr::addr_of!((*input).info).read() };
        if native_port_locked(owner, info.identity).is_some() && native_presence(info.flags, true) {
            info.flags |= a::gfx_output_flag_active
                | if modeset {
                    0
                } else {
                    a::gfx_output_flag_fixed_geometry
                }
        }
        // Match the original array-slice bounds even for an invalid caller that
        // changed its supposedly immutable counts across queue admission.
        assert!(
            info.mode_count <= a::gfx_output_max_modes
                && info.edid_bytes <= a::gfx_output_max_edid_bytes
        );
        let modes = unsafe { core::slice::from_raw_parts(modes, info.mode_count as usize) };
        let edid = unsafe { core::slice::from_raw_parts(edid, info.edid_bytes as usize) };
        unsafe { &mut *catalog() }.publish(owner, info, modes, edid, modeset)?
    };
    signal();
    Ok(identity)
}
pub fn withdraw(owner: u32, identity: a::GfxOutputId) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    {
        let _state = ownership::enter_state();
        let catalog = unsafe { &*catalog() };
        let entry = &catalog.entries[catalog.find(identity)?];
        if owner == 0 || entry.owner != owner || entry.receiver_source != 0 {
            return Err(Error::Stale);
        }
    }
    if super::mode_work::withdrawing_output(owner, identity) {
        return Err(Error::Busy);
    }
    {
        let _state = ownership::enter_state();
        unsafe { &mut *catalog() }.withdraw(owner, identity)?
    }
    signal();
    Ok(())
}
pub fn pause(owner: u32, identity: a::GfxOutputId, paused: bool) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let changed = {
        let _state = ownership::enter_state();
        if native_port_locked(owner, identity).is_none() {
            return Err(Error::Stale);
        }
        let catalog = unsafe { &mut *catalog() };
        let index = catalog.find(identity)?;
        if !paused && !native_presence(catalog.entries[index].info.flags, true) {
            return Err(Error::Stale);
        }
        catalog.pause(owner, identity, paused, true)?
    };
    if paused {
        let _ = super::mode_work::withdrawing_output(owner, identity);
    }
    if changed {
        signal()
    }
    Ok(())
}
pub fn native_paused(owner: u32, identity: a::GfxOutputId) -> bool {
    let _state = ownership::enter_state();
    let catalog = unsafe { &*catalog() };
    catalog
        .entries
        .iter()
        .find(|entry| {
            entry.owner == owner
                && entry.receiver_source == 0
                && same_port(entry.info.identity, identity)
        })
        .is_some_and(|entry| entry.paused)
}
pub fn receiver_epoch() -> u64 {
    let _state = ownership::enter_state();
    unsafe { (*META.0.get()).owner_epoch }
}
pub fn register_receiver_source(
    owner: Owner,
    adapter: u32,
    epoch: u64,
) -> Result<a::GfxReceiverSource, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let _state = ownership::enter_state();
    let meta = unsafe { &*META.0.get() };
    if meta.epoch_exhausted {
        return Err(Error::Exhausted);
    }
    if epoch != meta.owner_epoch {
        return Err(Error::Stale);
    }
    unsafe { &mut *catalog() }.register_source(owner, adapter)
}
/// # Safety
/// The R4D exclusively retains all counted resident records through this call;
/// no large record array is copied to a worker/task stack.
pub unsafe fn replace_receivers(owner: Owner, input: a::GfxReceiverUpdate) -> Result<(), Error> {
    if irq_router::in_dispatch()
        || input.version != 1
        || (input.size as usize) < size_of::<a::GfxReceiverUpdate>()
        || input.reserved0 != 0
        || input.count > a::gfx_receiver_max_outputs
        || (input.count == 0) != (input.receivers == 0)
        || !input
            .receivers
            .is_multiple_of(core::mem::align_of::<a::GfxReceiverInfo>() as u64)
        || input.receivers
            > u64::MAX - u64::from(input.count) * size_of::<a::GfxReceiverInfo>() as u64
    {
        return Err(Error::Invalid);
    }
    let records = if input.count == 0 {
        &[]
    } else {
        unsafe {
            core::slice::from_raw_parts(
                ptr::with_exposed_provenance::<a::GfxReceiverInfo>(input.receivers as usize),
                input.count as usize,
            )
        }
    };
    {
        let _state = ownership::enter_state();
        if unsafe { (*META.0.get()).epoch_exhausted } {
            return Err(Error::Exhausted);
        }
        unsafe { &mut *catalog() }.replace_receivers(
            owner,
            input.source,
            input.sequence,
            records,
        )?
    }
    signal();
    Ok(())
}
pub fn close_receiver_source(owner: Owner, binding: a::GfxReceiverSource) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let changed = {
        let _state = ownership::enter_state();
        unsafe { &mut *catalog() }.close_source(owner, binding)?
    };
    if changed {
        signal()
    }
    Ok(())
}
pub fn publish_audio(owner: Owner, input: &a::GfxAudioRoute) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let _state = ownership::enter_state();
    if unsafe { (*META.0.get()).epoch_exhausted } {
        return Err(Error::Exhausted);
    }
    unsafe { &mut *catalog() }.publish_audio(owner, input)
}
pub fn query_audio(location: u32, device: u32, index: u32) -> Option<a::GfxAudioRoute> {
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.audio.query(location, device, index)
}
pub fn stopped_driver(owner: u32) {
    if owner == 0 {
        return;
    }
    super::output_runtime::stopped_driver(owner);
    super::mode_work::stopped_driver(owner);
    super::cursor_work::stopped_driver(owner);
    let changed = {
        let _state = ownership::enter_state();
        let meta = unsafe { &mut *META.0.get() };
        if meta.owner_epoch == u64::MAX {
            meta.epoch_exhausted = true
        } else {
            meta.owner_epoch += 1
        }
        for port in &mut meta.native_ports {
            if port.owner == owner {
                *port = NativePort::EMPTY
            }
        }
        match unsafe { &mut *catalog() }.stop(owner) {
            Ok(changed) => changed,
            Err(_) => {
                for entry in unsafe { &mut *catalog() }.entries.iter_mut() {
                    if entry.owner == owner {
                        unsafe { model::Entry::init_in_place(entry) }
                    }
                }
                meta.epoch_exhausted = true;
                true
            }
        }
    };
    if changed {
        signal()
    }
}
pub fn boot_active(active: bool) {
    let changed = {
        let _state = ownership::enter_state();
        let catalog = unsafe { &mut *catalog() };
        let entry = &mut catalog.entries[0];
        if entry.info.identity.connector_id == 0 || entry.info.identity.adapter_id != 0 {
            return;
        }
        let was = entry.info.flags & a::gfx_output_flag_active != 0;
        if was == active {
            return;
        }
        if catalog.revision == u64::MAX {
            unsafe { (*META.0.get()).epoch_exhausted = true };
            return;
        }
        if active {
            entry.info.flags |= a::gfx_output_flag_active
        } else {
            entry.info.flags &= !a::gfx_output_flag_active
        }
        catalog.revision += 1;
        true
    };
    if changed {
        signal()
    }
}
pub fn validate_native(
    owner: u32,
    backend: a::GfxBackendBinding,
    identity: a::GfxOutputId,
    width: u32,
    height: u32,
    held: bool,
) -> Result<(), Error> {
    queue_validate(owner, backend)?;
    let _state = ownership::enter_state();
    if unsafe { (*META.0.get()).epoch_exhausted } {
        return Err(Error::Exhausted);
    }
    let catalog = unsafe { &*catalog() };
    let entry = &catalog.entries[catalog.find(identity)?];
    if owner == 0
        || entry.owner != owner
        || entry.receiver_source != 0
        || identity.adapter_id != backend.adapter_id
        || identity.device_generation != backend.device_generation
        || !native_presence(entry.info.flags, held)
    {
        return Err(Error::Stale);
    }
    if entry.modes[..entry.info.mode_count as usize]
        .iter()
        .any(|mode| mode.width == width && mode.height == height)
    {
        Ok(())
    } else {
        Err(Error::Unsupported)
    }
}
pub fn cursor_head(owner: u32, identity: a::GfxOutputId) -> Result<u32, Error> {
    let _state = ownership::enter_state();
    let route = native_port_locked(owner, identity).ok_or(Error::Stale)?;
    let meta = unsafe { &*META.0.get() };
    if meta.epoch_exhausted {
        return Err(Error::Stale);
    }
    if unsafe { &*catalog() }.entries.iter().any(|entry| {
        entry.owner == owner
            && same_port(identity, entry.info.identity)
            && entry.info.flags & a::gfx_output_flag_active != 0
    }) {
        Ok(meta.native_ports[route].head)
    } else {
        Err(Error::Unsupported)
    }
}
pub fn active_target(
    owner: u32,
    port: a::GfxOutputId,
    generation: u64,
) -> Result<a::GfxOutputTarget, Error> {
    let _state = ownership::enter_state();
    active_target_locked(owner, port, generation)
}
pub fn active_target_locked(
    owner: u32,
    port: a::GfxOutputId,
    generation: u64,
) -> Result<a::GfxOutputTarget, Error> {
    if unsafe { (*META.0.get()).epoch_exhausted } || generation == 0 {
        return Err(Error::Stale);
    }
    for entry in &unsafe { &*catalog() }.entries {
        if entry.owner == owner
            && entry.receiver_source == 0
            && same_port(port, entry.info.identity)
            && !entry.paused
            && entry.info.flags & a::gfx_output_flag_active != 0
        {
            let head = if let Some(route) = native_port_locked(owner, port) {
                unsafe { (*META.0.get()).native_ports[route].head }
            } else if owner == 0 && entry.info.possible_heads == 1 {
                0
            } else {
                return Err(Error::Stale);
            };
            return Ok(output_target::from_output(
                entry.info.identity,
                head,
                generation,
            ));
        }
    }
    Err(Error::Stale)
}
pub fn validate_target_locked(owner: u32, target: a::GfxOutputTarget) -> Result<(), Error> {
    if unsafe { (*META.0.get()).epoch_exhausted } || !output_target::valid(&target) {
        return Err(Error::Stale);
    }
    let catalog = unsafe { &*catalog() };
    let index = catalog.find(port(target)).map_err(|_| Error::Stale)?;
    let entry = &catalog.entries[index];
    if entry.owner != owner
        || entry.receiver_source != 0
        || entry.paused
        || entry.info.flags & a::gfx_output_flag_active == 0
        || entry.info.possible_heads & (1u32 << target.head_id) == 0
    {
        return Err(Error::Stale);
    }
    let route = native_port_locked(owner, entry.info.identity).ok_or(Error::Stale)?;
    if unsafe { (*META.0.get()).native_ports[route].head } != target.head_id {
        return Err(Error::Stale);
    }
    Ok(())
}
pub fn registered_head_locked(
    owner: u32,
    identity: a::GfxOutputId,
    head: u32,
) -> Result<(), Error> {
    let catalog = unsafe { &*catalog() };
    let entry = &catalog.entries[catalog.find(identity)?];
    if unsafe { (*META.0.get()).epoch_exhausted }
        || entry.owner != owner
        || entry.receiver_source != 0
        || head >= a::gfx_output_max_assignments
        || entry.info.possible_heads & (1u32 << head) == 0
    {
        return Err(Error::Stale);
    }
    for port in &unsafe { &*META.0.get() }.native_ports {
        if port.owner != 0
            && port.identity.adapter_id == identity.adapter_id
            && !same_port(port.identity, identity)
            && port.head == head
        {
            return Err(Error::Busy);
        }
    }
    Ok(())
}
pub fn validate_additional_locked(
    owner: u32,
    request: a::GfxAdditionalOutput,
) -> Result<(), Error> {
    registered_head_locked(owner, request.output, request.head_id)?;
    let catalog = unsafe { &*catalog() };
    let entry = &catalog.entries[catalog.find(request.output)?];
    if native_port_locked(owner, request.output).is_some()
        || !native_presence(entry.info.flags, false)
    {
        return Err(Error::Stale);
    }
    Ok(())
}
pub fn validate_additional_activation_locked(
    owner: u32,
    identity: a::GfxOutputId,
    head: u32,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    registered_head_locked(owner, identity, head)?;
    let catalog = unsafe { &*catalog() };
    let entry = &catalog.entries[catalog.find(identity)?];
    if !native_presence(entry.info.flags, false) {
        return Err(Error::Stale);
    }
    if entry.modes[..entry.info.mode_count as usize]
        .iter()
        .any(|mode| mode.width == width && mode.height == height)
    {
        Ok(())
    } else {
        Err(Error::Unsupported)
    }
}
pub fn pause_additional_locked(
    owner: u32,
    identity: a::GfxOutputId,
    paused: bool,
) -> Result<bool, Error> {
    unsafe { &mut *catalog() }.pause(owner, identity, paused, false)
}
pub fn current_identity_locked(owner: u32, identity: a::GfxOutputId) -> Option<a::GfxOutputId> {
    unsafe { &*catalog() }
        .entries
        .iter()
        .find(|entry| {
            entry.owner == owner
                && entry.receiver_source == 0
                && same_port(entry.info.identity, identity)
        })
        .map(|entry| entry.info.identity)
}
pub fn connected_locked(owner: u32, identity: a::GfxOutputId) -> bool {
    let catalog = unsafe { &*catalog() };
    let Ok(index) = catalog.find(identity) else {
        return false;
    };
    let entry = &catalog.entries[index];
    entry.owner == owner && !entry.paused && entry.info.flags & a::gfx_output_flag_connected != 0
}
pub fn forget_native_port_locked(owner: u32, identity: a::GfxOutputId) {
    if let Some(index) = native_port_locked(owner, identity) {
        unsafe { (*META.0.get()).native_ports[index] = NativePort::EMPTY }
    }
}
pub fn native_active(owner: u32, identity: a::GfxOutputId, active: bool) {
    let changed = {
        let _state = ownership::enter_state();
        let pointer = catalog();
        let mut result = false;
        for index in 0..model::CAPACITY {
            let info = unsafe { (*pointer).entries[index].info };
            let entry_owner = unsafe { (*pointer).entries[index].owner };
            if entry_owner != owner || !same_port(info.identity, identity) {
                continue;
            }
            let head = if let Some(port) = native_port_locked(owner, identity) {
                unsafe { (*META.0.get()).native_ports[port].head }
            } else if info.possible_heads.count_ones() == 1 {
                info.possible_heads.trailing_zeros()
            } else {
                break;
            };
            result = set_native_active_locked(owner, info.identity, head, active).unwrap_or(false);
            break;
        }
        result
    };
    if changed {
        signal()
    }
}
pub fn set_native_active_locked(
    owner: u32,
    identity: a::GfxOutputId,
    head: u32,
    active: bool,
) -> Result<bool, Error> {
    if owner == 0 || unsafe { (*META.0.get()).epoch_exhausted } {
        return Err(Error::Stale);
    }
    if active {
        registered_head_locked(owner, identity, head)?
    }
    let catalog = unsafe { &mut *catalog() };
    let index = catalog.find(identity)?;
    if catalog.revision == u64::MAX {
        return Err(Error::Exhausted);
    }
    let meta = unsafe { &mut *META.0.get() };
    let existing = meta
        .native_ports
        .iter()
        .position(|port| port.owner == owner && same_port(port.identity, identity));
    if active {
        let index = existing
            .or_else(|| meta.native_ports.iter().position(|port| port.owner == 0))
            .ok_or(Error::Capacity)?;
        meta.native_ports[index] = NativePort {
            owner,
            identity,
            head,
        };
    } else if let Some(index) = existing {
        meta.native_ports[index] = NativePort::EMPTY
    }
    let entry = &mut catalog.entries[index];
    let was = entry.info.flags & a::gfx_output_flag_active != 0;
    let enabled = active && !entry.paused && native_presence(entry.info.flags, true);
    if enabled {
        entry.info.flags |= a::gfx_output_flag_active
            | if entry.info.limits.flags & a::gfx_output_limit_modeset != 0 {
                0
            } else {
                a::gfx_output_flag_fixed_geometry
            }
    } else {
        entry.info.flags &= !(a::gfx_output_flag_active | a::gfx_output_flag_fixed_geometry)
    }
    if was == enabled {
        return Ok(false);
    }
    catalog.revision += 1;
    Ok(true)
}

struct BoLock;
impl BoLock {
    fn acquire() -> Self {
        buffers::lock();
        Self
    }
}
impl Drop for BoLock {
    fn drop(&mut self) {
        buffers::unlock()
    }
}
fn gather(
    owner: Owner,
    state: &a::GfxAtomicState,
    result: &mut [model::Fact; a::gfx_output_max_assignments as usize],
) -> Result<(), Error> {
    if state.count == 0 || state.count as usize > result.len() {
        return Err(Error::Invalid);
    }
    for (index, fact) in result.iter_mut().enumerate().take(state.count as usize) {
        let assignment = state.assignments[index];
        *fact = model::Fact::default();
        if !same(&assignment.buffer, &a::GfxBufferHandle::default()) {
            let reference = buffer_api::handle(assignment.buffer)?;
            let descriptor = {
                let _buffers = BoLock::acquire();
                unsafe { buffers::read() }.describe(reference, owner)?
            };
            fact.has_buffer = true;
            fact.buffer = descriptor;
        }
        if !same(&assignment.ready_fence, &a::GfxFence::default()) {
            let input = assignment.ready_fence;
            let fence = queue_state::Fence {
                slot: input.slot,
                timeline: input.timeline,
                point: input.point,
                binding: queue_state::Binding {
                    adapter: input.adapter_id,
                    device_generation: input.device_generation,
                    reset_generation: input.reset_generation,
                },
            };
            let value = super::queue::query(fence)?;
            fact.dependency = if value.phase != queue_state::TERMINAL
                || value.device_active
                || value.resources_held
            {
                2
            } else if value.result == queue_state::COMPLETE {
                1
            } else {
                3
            };
        }
    }
    Ok(())
}
struct Commit;
impl Drop for Commit {
    fn drop(&mut self) {
        super::manager::end_output_commit()
    }
}
pub fn atomic(
    owner: Owner,
    state: &a::GfxAtomicState,
    commit: bool,
) -> Result<a::GfxAtomicResult, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let mut facts = [model::Fact::default(); a::gfx_output_max_assignments as usize];
    gather(owner, state, &mut facts)?;
    if !commit {
        let _state = ownership::enter_state();
        return unsafe { &*catalog() }.validate(state, &facts[..state.count as usize]);
    }
    if !super::manager::begin_output_commit() {
        return Err(Error::Busy);
    }
    let _commit = Commit;
    if super::manager::backend_state().state != backend::BOOTFB
        || state.count != 1
        || state.assignments[0].output.adapter_id != 0
    {
        return Err(Error::Unsupported);
    }
    let result = {
        let _state = ownership::enter_state();
        let catalog = unsafe { &mut *catalog() };
        let ticket = catalog.begin(state, &facts[..state.count as usize])?;
        let mut result = catalog.finish(ticket, a::gfx_output_outcome_applied, 3)?;
        result.retained = 0;
        result
    };
    signal();
    Ok(result)
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeAdmission {
    pub ticket: model::Ticket,
    pub mode: a::GfxOutputMode,
    pub revision: u64,
}
pub fn native_mode(owner: Owner, state: &a::GfxAtomicState) -> Result<a::GfxOutputMode, Error> {
    let mut facts = [model::Fact::default(); a::gfx_output_max_assignments as usize];
    gather(owner, state, &mut facts)?;
    if state.count != 1 || state.assignments[0].output.adapter_id == 0 {
        return Err(Error::Unsupported);
    }
    let _state = ownership::enter_state();
    let catalog = unsafe { &*catalog() };
    let _ = catalog.validate(state, &facts[..1])?;
    let entry = &catalog.entries[catalog.find(state.assignments[0].output)?];
    entry.modes[..entry.info.mode_count as usize]
        .iter()
        .find(|mode| mode.mode_id == state.assignments[0].mode_id)
        .copied()
        .ok_or(Error::Stale)
}
pub fn begin_native(owner: Owner, state: &a::GfxAtomicState) -> Result<NativeAdmission, Error> {
    let mut facts = [model::Fact::default(); a::gfx_output_max_assignments as usize];
    gather(owner, state, &mut facts)?;
    if state.count != 1 || state.assignments[0].output.adapter_id == 0 {
        return Err(Error::Unsupported);
    }
    let _state = ownership::enter_state();
    if unsafe { (*META.0.get()).epoch_exhausted } {
        return Err(Error::Exhausted);
    }
    let catalog = unsafe { &mut *catalog() };
    if catalog.revision > u64::MAX - 3 || catalog.commit_sequence > u64::MAX - 2 {
        return Err(Error::Exhausted);
    }
    let index = catalog.find(state.assignments[0].output)?;
    let mode = catalog.entries[index].modes[..catalog.entries[index].info.mode_count as usize]
        .iter()
        .find(|mode| mode.mode_id == state.assignments[0].mode_id)
        .copied()
        .ok_or(Error::Stale)?;
    Ok(NativeAdmission {
        ticket: catalog.begin(state, &facts[..1])?,
        mode,
        revision: catalog.revision,
    })
}
pub fn finish_native(
    ticket: model::Ticket,
    operation: u32,
    outcome: u32,
    quiesced: u32,
) -> Result<a::GfxAtomicResult, Error> {
    let _state = ownership::enter_state();
    let catalog = unsafe { &mut *catalog() };
    if operation == a::gfx_mode_operation_apply {
        return catalog.finish(ticket, outcome, quiesced);
    }
    if catalog.pending.id != 0 || catalog.retained != 0 {
        return Err(Error::Busy);
    }
    if catalog.revision == u64::MAX || catalog.commit_sequence == u64::MAX {
        return Err(Error::Exhausted);
    }
    if outcome == a::gfx_output_outcome_lost {
        catalog.retained = 3
    }
    if operation == a::gfx_mode_operation_rollback && outcome == a::gfx_output_outcome_old_preserved
    {
        catalog.commit_sequence += 1
    }
    catalog.revision += 1;
    Ok(a::GfxAtomicResult {
        topology_revision: catalog.revision,
        commit_sequence: catalog.commit_sequence,
        outcome,
        retained: if outcome == a::gfx_output_outcome_lost {
            3
        } else if outcome == a::gfx_output_outcome_applied {
            2
        } else {
            1
        },
        ..a::GfxAtomicResult::default()
    })
}
pub fn can_finish_native(ticket: model::Ticket, operation: u32) -> Result<(), Error> {
    let _state = ownership::enter_state();
    let catalog = unsafe { &*catalog() };
    if operation == a::gfx_mode_operation_apply {
        if ticket.id == 0 || catalog.pending.id != ticket.id {
            return Err(Error::Stale);
        }
    } else if catalog.pending.id != 0 || catalog.retained != 0 {
        return Err(Error::Busy);
    }
    if catalog.revision == u64::MAX || catalog.commit_sequence == u64::MAX {
        return Err(Error::Exhausted);
    }
    Ok(())
}
pub fn retire_native_after_reset(ticket: model::Ticket) -> Result<(), Error> {
    let _state = ownership::enter_state();
    unsafe { &mut *catalog() }.retire_after_reset(ticket)
}
pub fn brightness_at(identity: a::GfxOutputId) -> Result<a::GfxOutputBrightness, Error> {
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.brightness_at(identity)
}
pub fn publish_brightness(owner: u32, value: a::GfxOutputBrightness) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let changed = {
        let _state = ownership::enter_state();
        if native_port_locked(owner, value.identity).is_none() {
            return Err(Error::Stale);
        }
        unsafe { &mut *catalog() }.publish_brightness(owner, value)?
    };
    if changed {
        signal()
    }
    Ok(())
}
pub fn request_brightness(
    actor: Owner,
    input: a::GfxBrightnessRequest,
) -> Result<a::GfxBrightnessRequest, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let (owner, value) = {
        let _state = ownership::enter_state();
        let catalog = unsafe { &mut *catalog() };
        let i = catalog.power_entry(input.identity)?;
        (
            catalog.entries[i].owner,
            catalog.request_brightness(actor, input)?,
        )
    };
    wake_power(owner, input.identity);
    Ok(value)
}
pub fn read_brightness(
    owner: u32,
    identity: a::GfxOutputId,
) -> Result<a::GfxBrightnessRequest, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Invalid);
    }
    let _state = ownership::enter_state();
    unsafe { &*catalog() }.read_brightness(owner, identity)
}
const _: () = {
    assert!(
        size_of::<NativeAdmission>() == 80
            && core::mem::offset_of!(NativeAdmission, revision) == 72
    );
};

#[repr(C)]
pub struct Args {
    pub owner: u32,
    pub actor: Owner,
    pub identity: a::GfxOutputId,
    pub target: a::GfxOutputTarget,
    pub binding: a::GfxBackendBinding,
    pub index: u32,
    pub head: u32,
    pub width: u32,
    pub height: u32,
    pub value: u64,
    pub other: u64,
    pub ticket: model::Ticket,
    pub operation: u32,
    pub outcome: u32,
    pub quiesced: u32,
    pub flag: bool,
    pub other_flag: bool,
    pub input: *const core::ffi::c_void,
    pub frame: *const Framebuffer,
}
#[repr(C)]
pub struct Optional<T> {
    pub has: bool,
    pub value: T,
}
impl<T: Default> From<Option<T>> for Optional<T> {
    fn from(value: Option<T>) -> Self {
        Self {
            has: value.is_some(),
            value: value.unwrap_or_default(),
        }
    }
}
enum Output {
    None,
    Revision(a::GfxDisplayRevision),
    Info(Optional<a::GfxOutputInfo>),
    Color(a::GfxOutputColorState),
    Refresh(a::GfxOutputRefresh),
    RefreshRequest(a::GfxRefreshRequest),
    Power(a::GfxOutputPower),
    PowerRequest(a::GfxPowerRequest),
    Mode(Optional<a::GfxOutputMode>),
    Edid(Optional<a::GfxEdidBlock>),
    Id(a::GfxOutputId),
    Boolean(bool),
    Number(u64),
    Word(u32),
    Source(a::GfxReceiverSource),
    Audio(Optional<a::GfxAudioRoute>),
    Target(a::GfxOutputTarget),
    OptionalId(Optional<a::GfxOutputId>),
    Atomic(a::GfxAtomicResult),
    NativeMode(a::GfxOutputMode),
    Admission(NativeAdmission),
    Brightness(a::GfxOutputBrightness),
    BrightnessRequest(a::GfxBrightnessRequest),
}
/// # Safety
/// Operation-specific exact initialized C inputs, stable through foreign queue
/// admission/waits; caller retains its original R4X/R4D lifetime pins. Locked
/// operations require the established program owner. Output is exclusive and
/// receives only a complete copied result after short metadata owners end.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_outputs(
    op: u32,
    args: *const Args,
    out: *mut core::ffi::c_void,
) -> i32 {
    let args = unsafe { &*args };
    let result = match op {
        0 => {
            init_boot(unsafe { &*args.frame });
            Ok(Output::None)
        }
        1 => Ok(Output::Revision(revision())),
        2 => Ok(Output::Info(info_at(args.index).into())),
        3 => color_at(args.identity).map(Output::Color),
        4 => publish_color(args.owner, unsafe {
            args.input.cast::<a::GfxOutputColorState>().read()
        })
        .map(|()| Output::None),
        5 => refresh_at(args.target).map(Output::Refresh),
        6 => publish_refresh(args.owner, unsafe {
            args.input.cast::<a::GfxOutputRefresh>().read()
        })
        .map(|()| Output::None),
        7 => request_refresh(args.actor, unsafe {
            args.input.cast::<a::GfxRefreshRequest>().read()
        })
        .map(Output::RefreshRequest),
        8 => read_refresh(args.owner, args.target).map(Output::RefreshRequest),
        9 => {
            refresh_stopped(args.actor);
            Ok(Output::None)
        }
        10 => power_at(args.identity).map(Output::Power),
        11 => publish_power(args.owner, unsafe {
            args.input.cast::<a::GfxOutputPower>().read()
        })
        .map(|()| Output::None),
        12 => request_power(args.actor, unsafe {
            args.input.cast::<a::GfxPowerRequest>().read()
        })
        .map(Output::PowerRequest),
        13 => read_power(args.owner, args.identity).map(Output::PowerRequest),
        14 => {
            power_stopped(args.actor);
            Ok(Output::None)
        }
        15 => mode_at(args.identity, args.index).map(|v| Output::Mode(v.into())),
        16 => edid_at(args.identity, args.index).map(|v| Output::Edid(v.into())),
        17 => unsafe { publish(args.owner, args.input.cast::<a::GfxOutputPublication>()) }
            .map(Output::Id),
        18 => withdraw(args.owner, args.identity).map(|()| Output::None),
        19 => pause(args.owner, args.identity, args.flag).map(|()| Output::None),
        20 => Ok(Output::Boolean(native_paused(args.owner, args.identity))),
        21 => Ok(Output::Number(receiver_epoch())),
        22 => register_receiver_source(args.actor, args.index, args.value).map(Output::Source),
        23 => unsafe {
            replace_receivers(args.actor, args.input.cast::<a::GfxReceiverUpdate>().read())
        }
        .map(|()| Output::None),
        24 => close_receiver_source(args.actor, unsafe {
            args.input.cast::<a::GfxReceiverSource>().read()
        })
        .map(|()| Output::None),
        25 => publish_audio(args.actor, unsafe {
            &*args.input.cast::<a::GfxAudioRoute>()
        })
        .map(|()| Output::None),
        26 => Ok(Output::Audio(
            query_audio(args.owner, args.head, args.index).into(),
        )),
        27 => {
            stopped_driver(args.owner);
            Ok(Output::None)
        }
        28 => {
            boot_active(args.flag);
            Ok(Output::None)
        }
        29 => validate_native(
            args.owner,
            args.binding,
            args.identity,
            args.width,
            args.height,
            args.flag,
        )
        .map(|()| Output::None),
        30 => cursor_head(args.owner, args.identity).map(Output::Word),
        31 => active_target(args.owner, args.identity, args.value).map(Output::Target),
        32 => active_target_locked(args.owner, args.identity, args.value).map(Output::Target),
        33 => validate_target_locked(args.owner, args.target).map(|()| Output::None),
        34 => registered_head_locked(args.owner, args.identity, args.head).map(|()| Output::None),
        35 => validate_additional_locked(args.owner, unsafe {
            args.input.cast::<a::GfxAdditionalOutput>().read()
        })
        .map(|()| Output::None),
        36 => validate_additional_activation_locked(
            args.owner,
            args.identity,
            args.head,
            args.width,
            args.height,
        )
        .map(|()| Output::None),
        37 => pause_additional_locked(args.owner, args.identity, args.flag).map(Output::Boolean),
        38 => Ok(Output::OptionalId(
            current_identity_locked(args.owner, args.identity).into(),
        )),
        39 => Ok(Output::Boolean(connected_locked(args.owner, args.identity))),
        40 => {
            forget_native_port_locked(args.owner, args.identity);
            Ok(Output::None)
        }
        41 => {
            native_active(args.owner, args.identity, args.flag);
            Ok(Output::None)
        }
        42 => set_native_active_locked(args.owner, args.identity, args.head, args.flag)
            .map(Output::Boolean),
        43 => atomic(
            args.actor,
            unsafe { &*args.input.cast::<a::GfxAtomicState>() },
            args.flag,
        )
        .map(Output::Atomic),
        44 => native_mode(args.actor, unsafe {
            &*args.input.cast::<a::GfxAtomicState>()
        })
        .map(Output::NativeMode),
        45 => begin_native(args.actor, unsafe {
            &*args.input.cast::<a::GfxAtomicState>()
        })
        .map(Output::Admission),
        46 => finish_native(args.ticket, args.operation, args.outcome, args.quiesced)
            .map(Output::Atomic),
        47 => can_finish_native(args.ticket, args.operation).map(|()| Output::None),
        48 => retire_native_after_reset(args.ticket).map(|()| Output::None),
        49 => brightness_at(args.identity).map(Output::Brightness),
        50 => publish_brightness(args.owner, unsafe {
            args.input.cast::<a::GfxOutputBrightness>().read()
        })
        .map(|()| Output::None),
        51 => request_brightness(args.actor, unsafe {
            args.input.cast::<a::GfxBrightnessRequest>().read()
        })
        .map(Output::BrightnessRequest),
        52 => read_brightness(args.owner, args.identity).map(Output::BrightnessRequest),
        _ => panic!("output runtime operation"),
    };
    match result {
        Err(error) => error as i32,
        Ok(value) => {
            unsafe {
                match value {
                    Output::None => {}
                    Output::Revision(v) => out.cast::<a::GfxDisplayRevision>().write(v),
                    Output::Info(v) => out.cast::<Optional<a::GfxOutputInfo>>().write(v),
                    Output::Color(v) => out.cast::<a::GfxOutputColorState>().write(v),
                    Output::Refresh(v) => out.cast::<a::GfxOutputRefresh>().write(v),
                    Output::RefreshRequest(v) => out.cast::<a::GfxRefreshRequest>().write(v),
                    Output::Power(v) => out.cast::<a::GfxOutputPower>().write(v),
                    Output::PowerRequest(v) => out.cast::<a::GfxPowerRequest>().write(v),
                    Output::Mode(v) => out.cast::<Optional<a::GfxOutputMode>>().write(v),
                    Output::Edid(v) => out.cast::<Optional<a::GfxEdidBlock>>().write(v),
                    Output::Id(v) => out.cast::<a::GfxOutputId>().write(v),
                    Output::Boolean(v) => out.cast::<bool>().write(v),
                    Output::Number(v) => out.cast::<u64>().write(v),
                    Output::Word(v) => out.cast::<u32>().write(v),
                    Output::Source(v) => out.cast::<a::GfxReceiverSource>().write(v),
                    Output::Audio(v) => out.cast::<Optional<a::GfxAudioRoute>>().write(v),
                    Output::Target(v) => out.cast::<a::GfxOutputTarget>().write(v),
                    Output::OptionalId(v) => out.cast::<Optional<a::GfxOutputId>>().write(v),
                    Output::Atomic(v) => out.cast::<a::GfxAtomicResult>().write(v),
                    Output::NativeMode(v) => out.cast::<a::GfxOutputMode>().write(v),
                    Output::Admission(v) => out.cast::<NativeAdmission>().write(v),
                    Output::Brightness(v) => out.cast::<a::GfxOutputBrightness>().write(v),
                    Output::BrightnessRequest(v) => out.cast::<a::GfxBrightnessRequest>().write(v),
                }
            };
            0
        }
    }
}
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Args>() == 208
            && offset_of!(Args, actor) == 8
            && offset_of!(Args, binding) == 104
            && offset_of!(Args, input) == 192
            && offset_of!(Args, frame) == 200
    );
};
