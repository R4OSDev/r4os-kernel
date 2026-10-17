//! Original stable block registry, retirement, operation pins and controller identities.
//! Device/request storage remains with its storage owner; metadata mutations
//! occur only here under the original short storage owner, never across I/O.
use crate::{
    memory::owner::Guard,
    sched::task_context,
    storage::{
        access_runtime,
        access_state::{Input, Reference},
        dispatch,
    },
};
use core::{cell::UnsafeCell, ffi::c_void, ptr, slice};
const MAX: usize = 16;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Text {
    pub data: *const u8,
    pub len: usize,
}
impl Text {
    const EMPTY: Self = Self {
        data: ptr::null(),
        len: 0,
    };
    unsafe fn bytes<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.data, self.len) }
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Registration {
    pub name: Text,
    pub model: Text,
    pub driver: Text,
    pub controller: Text,
    pub sectors: u64,
    pub sector_bytes: u32,
    pub queue_depth: u16,
    pub max_sectors: u16,
    pub bus: u8,
    pub writable: bool,
    pub owns_retry: bool,
    pub async_submit: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SlotView {
    pub device: *mut c_void,
    pub index: usize,
    pub retire_generation: u64,
    pub reference: Reference,
    pub pins: u32,
    pub used: bool,
    pub retiring: bool,
    pub lane: u8,
}
#[derive(Clone, Copy)]
struct Slot {
    device: *mut c_void,
    used: bool,
    retiring: bool,
    pins: u32,
    retire_generation: u64,
    reference: Reference,
    info: Registration,
    lane: u8,
}
impl Slot {
    const EMPTY: Self = Self {
        device: ptr::null_mut(),
        used: false,
        retiring: false,
        pins: 0,
        retire_generation: 0,
        reference: Reference {
            slot: 0,
            generation: 0,
        },
        info: Registration {
            name: Text::EMPTY,
            model: Text::EMPTY,
            driver: Text::EMPTY,
            controller: Text::EMPTY,
            sectors: 0,
            sector_bytes: 0,
            queue_depth: 0,
            max_sectors: 0,
            bus: 0,
            writable: false,
            owns_retry: false,
            async_submit: false,
        },
        lane: 255,
    };
    fn view(self, index: usize) -> SlotView {
        SlotView {
            device: self.device,
            index,
            retire_generation: self.retire_generation,
            reference: self.reference,
            pins: self.pins,
            used: self.used,
            retiring: self.retiring,
            lane: self.lane,
        }
    }
}
struct Registry {
    slots: [Slot; MAX],
    count: usize,
    slot_count: usize,
    excluded: u8,
    controllers: dispatch::Controllers,
}
impl Registry {
    const EMPTY: Self = Self {
        slots: [Slot::EMPTY; MAX],
        count: 0,
        slot_count: 0,
        excluded: 255,
        controllers: dispatch::Controllers {
            used: [false; 8],
            names: [[0; 32]; 8],
            lengths: [0; 8],
        },
    };
}
struct Canonical(UnsafeCell<Registry>);
// Only original storage admission grants metadata access; boot reset is
// exclusive. No pool/reference borrow is retained through a transport shim.
unsafe impl Sync for Canonical {}
static REGISTRY: Canonical = Canonical(UnsafeCell::new(Registry::EMPTY));
type Initialize = unsafe extern "C" fn(*mut c_void, *const c_void, *const Reference, u8);
unsafe fn access(op: u8, reference: Reference, value: u64) -> Option<u64> {
    let a = Input {
        reference,
        value,
        ..Input::default()
    };
    let mut out = 0;
    let code = unsafe {
        access_runtime::r4os_rust_storage_access_runtime(op, true, &a, &mut out, ptr::null_mut())
    };
    if code == 0 { Some(out) } else { None }
}
unsafe fn find(name: Text) -> Option<usize> {
    let r = unsafe { &*REGISTRY.0.get() };
    for i in 0..r.slot_count {
        let s = r.slots[i];
        if s.used
            && s.info.bus != r.excluded
            && unsafe { s.info.name.bytes().eq_ignore_ascii_case(name.bytes()) }
        {
            return Some(i);
        }
    }
    None
}
fn visible(s: Slot, excluded: u8) -> bool {
    s.used && !s.retiring && s.info.bus != excluded
}
unsafe fn copy_text<const N: usize>(out: &mut [u8; N], text: Text) {
    let n = text.len.min(N - 1);
    if n != 0 {
        unsafe { ptr::copy_nonoverlapping(text.data, out.as_mut_ptr(), n) }
    }
}
/// # Safety
/// Exclusive original boot reset. All16 distinct resident device addresses
/// remain stable until shutdown; no registration/pin/wait can survive reset.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_init(devices: *const *mut c_void) {
    unsafe { REGISTRY.0.get().write(Registry::EMPTY) };
    for i in 0..MAX {
        unsafe { (*REGISTRY.0.get()).slots[i].device = devices.add(i).read() }
    }
}
/// # Safety
/// Input text/payload remain readable; permanent text belongs to the backend
/// registration until its successful unregister. Initializer only copies and
/// resets one exact resident device's fields/sync records; no wait, backend,
/// callback, registry access or allocation. It must initialize before return.
/// Output receives one exact C slot snapshot only after successful publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_register(
    info: *const Registration,
    payload: *const c_void,
    initialize: Initialize,
    out: *mut SlotView,
) -> bool {
    let info = unsafe { info.read() };
    let _owner = Guard::acquire(1);
    if unsafe { (*REGISTRY.0.get()).count } >= MAX
        || unsafe { find(info.name) }.is_some()
        || info.queue_depth == 0
        || info.queue_depth > 16
    {
        return false;
    }
    let mut index = 0;
    while index < unsafe { (*REGISTRY.0.get()).slot_count }
        && unsafe { (*REGISTRY.0.get()).slots[index].used }
    {
        index += 1;
    }
    if index == unsafe { (*REGISTRY.0.get()).slot_count } && index >= MAX {
        return false;
    }
    let controllers = unsafe { ptr::addr_of_mut!((*REGISTRY.0.get()).controllers) };
    let lane = unsafe {
        dispatch::r4os_rust_storage_controller_assign(
            controllers,
            info.controller.data,
            info.controller.len,
        )
    };
    if lane == 255 {
        return false;
    }
    if index == unsafe { (*REGISTRY.0.get()).slot_count } {
        unsafe { (*REGISTRY.0.get()).slot_count += 1 };
    }
    let Some(generation) = (unsafe {
        access(
            1,
            Reference {
                slot: index as u32,
                generation: 0,
            },
            info.sectors,
        )
    }) else {
        return false;
    };
    let reference = Reference {
        slot: index as u32,
        generation,
    };
    let device = unsafe { (*REGISTRY.0.get()).slots[index].device };
    unsafe { initialize(device, payload, &reference, lane) };
    // Original tombstone retirement generation survives registration reuse.
    let generation = unsafe { (*REGISTRY.0.get()).slots[index].retire_generation };
    let row = Slot {
        device,
        used: true,
        retiring: false,
        pins: 0,
        retire_generation: generation,
        reference,
        info,
        lane,
    };
    unsafe {
        (*REGISTRY.0.get()).slots[index] = row;
        (*REGISTRY.0.get()).count += 1;
        out.write(row.view(index))
    };
    true
}
/// # Safety
/// Caller output is aligned exclusive C storage. Mode0 reads any used slot
/// (including retiring/hidden); mode1 requires normal visible admission;
/// mode2 authenticates an exact prepared retirement generation. `locked`
/// is true only while the caller already holds the same storage owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_slot(
    index: usize,
    mode: u8,
    generation: u64,
    locked: bool,
    out: *mut SlotView,
) -> bool {
    let _owner = if locked {
        None
    } else {
        Some(Guard::acquire(1))
    };
    let r = unsafe { &*REGISTRY.0.get() };
    if index >= r.slot_count {
        return false;
    }
    let s = r.slots[index];
    let allowed = match mode {
        0 => s.used,
        1 => visible(s, r.excluded),
        2 => s.used && s.retiring && s.retire_generation == generation,
        _ => unreachable!(),
    };
    if allowed {
        unsafe { out.write(s.view(index)) }
    }
    allowed
}
/// # Safety
/// Same exact index/incarnation under original unregister orchestration.
/// Op0 begins retirement,1 cancels,2 commits. Output is exclusive C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_retire(
    index: usize,
    op: u8,
    out: *mut SlotView,
) -> bool {
    unsafe { retire(index, op, None, out) }
}
pub(crate) unsafe fn begin_retirement(reference: Reference, out: *mut SlotView) -> bool {
    unsafe { retire(reference.slot as usize, 0, Some(reference), out) }
}
#[inline(always)]
unsafe fn retire(index: usize, op: u8, expected: Option<Reference>, out: *mut SlotView) -> bool {
    let _owner = Guard::acquire(1);
    let r = REGISTRY.0.get();
    if index >= unsafe { (*r).slot_count } {
        return false;
    }
    let mut s = unsafe { (*r).slots[index] };
    if expected.is_some_and(|v| v != s.reference) {
        return false;
    }
    match op {
        0 => {
            if !s.used || s.retiring || s.retire_generation == u64::MAX {
                return false;
            }
            if unsafe { access(2, s.reference, 0) }.is_none() {
                return false;
            }
            s.retiring = true;
            s.retire_generation += 1;
            if s.pins != 0 {
                s.retiring = false;
                unsafe { access(3, s.reference, 0) };
                unsafe { (*r).slots[index] = s };
                return false;
            }
        }
        1 => {
            if s.used {
                s.retiring = false;
                unsafe { access(3, s.reference, 0) };
            }
        }
        2 => {
            if !s.used || !s.retiring || s.pins != 0 {
                return false;
            }
            if unsafe { access(4, s.reference, 0) }.is_none() {
                return false;
            }
            s.used = false;
            s.retiring = false;
            unsafe {
                (*r).slots[index] = s;
                if (*r).count != 0 {
                    (*r).count -= 1;
                }
            }
            let used = unsafe {
                (&(*r).slots)[..(*r).slot_count]
                    .iter()
                    .any(|v| v.used && v.lane == s.lane)
            };
            if !used {
                unsafe {
                    dispatch::r4os_rust_storage_controller(
                        ptr::addr_of_mut!((*r).controllers),
                        true,
                        s.lane,
                    )
                };
            }
        }
        _ => unreachable!(),
    }
    unsafe {
        (*r).slots[index] = s;
        out.write(s.view(index))
    };
    true
}
#[repr(C)]
pub struct Pin {
    pub index: usize,
    pub device: *mut c_void,
    pub unwind: task_context::Unwind,
    pub active: bool,
}
/// # Safety
/// Output receives an exact admitted operation pin plus task-owned unwind.
/// Its resident device remains alive until this token is consumed by unpin.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_pin(index: usize, out: *mut Pin) -> bool {
    unsafe { pin(index, None, out) }
}
pub(crate) unsafe fn pin_exact(reference: Reference, out: *mut Pin) -> bool {
    unsafe { pin(reference.slot as usize, Some(reference), out) }
}
#[inline(always)]
unsafe fn pin(index: usize, expected: Option<Reference>, out: *mut Pin) -> bool {
    let unwind = unsafe { task_context::enter() };
    if !unwind.admitted() {
        return false;
    }
    let device = {
        let _owner = Guard::acquire(1);
        let r = unsafe { &mut *REGISTRY.0.get() };
        if index >= r.slot_count
            || !visible(r.slots[index], r.excluded)
            || r.slots[index].pins == u32::MAX
            || expected.is_some_and(|v| v != r.slots[index].reference)
        {
            None
        } else {
            r.slots[index].pins += 1;
            Some(r.slots[index].device)
        }
    };
    let Some(device) = device else {
        unsafe { task_context::leave(unwind) };
        return false;
    };
    unsafe {
        out.write(Pin {
            index,
            device,
            unwind,
            active: true,
        })
    };
    true
}
/// # Safety
/// Exact live pin token is exclusive and consumed once, after all physical
/// completion obligations. The original task counter remains resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_unpin(pin: *mut Pin) {
    let pin = unsafe { &mut *pin };
    if !pin.active {
        return;
    }
    {
        let _owner = Guard::acquire(1);
        let r = unsafe { &mut *REGISTRY.0.get() };
        if r.slots[pin.index].pins != 0 {
            r.slots[pin.index].pins -= 1;
        }
        pin.active = false;
    }
    unsafe { task_context::leave(pin.unwind) };
}
/// # Safety
/// Name owns len readable bytes; lookup never lends registry text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_find(data: *const u8, len: usize) -> usize {
    let _owner = Guard::acquire(1);
    unsafe { find(Text { data, len }) }.unwrap_or(usize::MAX)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_registry_query(op: u8, arg: u8) -> usize {
    let _owner = Guard::acquire(1);
    let r = unsafe { &*REGISTRY.0.get() };
    match op {
        0 => r.slots[..r.slot_count]
            .iter()
            .filter(|s| visible(**s, r.excluded))
            .count(),
        1 => r.slot_count,
        2 => usize::from(arg != r.excluded),
        3 => r.controllers.used.iter().filter(|v| **v).count(),
        4 => usize::from(usize::from(arg) < 8 && r.controllers.used[usize::from(arg)]),
        _ => unreachable!(),
    }
}
/// # Safety
/// Boot-only exclusion. Caller has checked actual mounted drives first and
/// supplies whether same-bus active executions exist. No unmount/revocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_exclude(bus: u8, active: bool) -> bool {
    if bus == 5 {
        return false;
    }
    let _owner = Guard::acquire(1);
    let r = unsafe { &mut *REGISTRY.0.get() };
    if r.excluded != 255 {
        return r.excluded == bus;
    }
    if active
        || r.slots[..r.slot_count]
            .iter()
            .any(|s| s.used && s.info.bus == bus && s.pins != 0)
    {
        return false;
    }
    r.excluded = bus;
    true
}
#[repr(C)]
pub struct Identity {
    pub reference: Reference,
    pub sectors: u64,
    pub sector_bytes: u32,
    pub writable: bool,
    pub bus: u8,
    pub name: [u8; 32],
    pub model: [u8; 64],
    pub driver: [u8; 32],
}
/// # Safety
/// Exclusive aligned output gets copied identity text under registry owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_identity(
    index: usize,
    out: *mut Identity,
) -> bool {
    let _owner = Guard::acquire(1);
    let r = unsafe { &*REGISTRY.0.get() };
    if index >= r.slot_count || !visible(r.slots[index], r.excluded) {
        return false;
    }
    let s = r.slots[index];
    let mut value = Identity {
        reference: s.reference,
        sectors: s.info.sectors,
        sector_bytes: s.info.sector_bytes,
        writable: s.info.writable,
        bus: s.info.bus,
        name: [0; 32],
        model: [0; 64],
        driver: [0; 32],
    };
    unsafe {
        copy_text(&mut value.name, s.info.name);
        copy_text(&mut value.model, s.info.model);
        copy_text(&mut value.driver, s.info.driver);
        out.write(value)
    };
    true
}
#[repr(C)]
pub struct WriteLimits {
    pub reference: Reference,
    pub max_sectors: u16,
    pub owns_retry: bool,
}
/// # Safety
/// Exclusive C output. Immutable registered capability snapshot only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_registry_limits(
    index: usize,
    out: *mut WriteLimits,
) -> bool {
    let _owner = Guard::acquire(1);
    let r = unsafe { &*REGISTRY.0.get() };
    if index >= r.slot_count || !visible(r.slots[index], r.excluded) {
        return false;
    }
    let s = r.slots[index];
    if s.info.sector_bytes != 512 || !s.info.writable {
        return false;
    }
    unsafe {
        out.write(WriteLimits {
            reference: s.reference,
            max_sectors: if s.info.max_sectors == 0 {
                65535
            } else {
                s.info.max_sectors
            },
            owns_retry: s.info.owns_retry,
        })
    };
    true
}
const _: () = {
    assert!(core::mem::size_of::<Registration>() == 88);
    assert!(core::mem::size_of::<SlotView>() == 48);
    assert!(core::mem::size_of::<Pin>() == 40);
    assert!(core::mem::size_of::<Identity>() == 160);
    assert!(core::mem::size_of::<WriteLimits>() == 24);
};

// Immutable cache capability snapshot, protected by the existing registry owner.
#[derive(Clone, Copy)]
pub(crate) struct CacheView {
    pub sector_bytes: u32,
    pub sectors: u64,
    pub max_sectors: u16,
    pub owns_retry: bool,
}
pub(crate) fn cache_view(index: usize) -> Option<CacheView> {
    let _owner = Guard::acquire(1);
    let r = unsafe { &*REGISTRY.0.get() };
    if index >= r.slot_count || !visible(r.slots[index], r.excluded) {
        return None;
    }
    let info = r.slots[index].info;
    Some(CacheView {
        sector_bytes: info.sector_bytes,
        sectors: info.sectors,
        max_sectors: info.max_sectors,
        owns_retry: info.owns_retry,
    })
}

// Typed passive bounds for other native kernel domains.
pub(crate) fn count() -> usize {
    let _owner = Guard::acquire(1);
    unsafe { (*REGISTRY.0.get()).count }
}
pub(crate) fn slot_count() -> usize {
    let _owner = Guard::acquire(1);
    unsafe { (*REGISTRY.0.get()).slot_count }
}
pub(crate) const fn max_devices() -> usize {
    MAX
}
