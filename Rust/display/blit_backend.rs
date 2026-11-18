//! Single owner-bound synchronous blit callback. Admission/counter changes
//! stay under the established display state owner; callbacks run outside it.
use super::ownership;
use core::{cell::UnsafeCell, mem::size_of};
pub const NAME_BYTES: usize = 24;
pub const REQUIRED_FLAGS: u32 = crate::contract::display_blit_backend_flag_xrgb32
    | crate::contract::display_blit_backend_flag_synchronous;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Region {
    pub dst_x: u32,
    pub dst_y: u32,
    pub src_x: u32,
    pub src_y: u32,
    pub w: u32,
    pub h: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Job {
    pub version: u32,
    pub size: u32,
    pub format: u32,
    pub flags: u32,
    pub target_address: u64,
    pub target_width: u32,
    pub target_height: u32,
    pub target_pitch_pixels: u32,
    pub source_pixel_count: u32,
    pub source_address: u64,
    pub source_stride_pixels: u32,
    pub region_count: u32,
    pub regions_address: u64,
}
pub type PresentFn = unsafe extern "C" fn(usize, *const Job) -> i32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Descriptor {
    pub version: u32,
    pub size: u32,
    pub flags: u32,
    pub max_regions: u32,
    pub context: usize,
    pub present: PresentFn,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub active: bool,
    pub name: [u8; NAME_BYTES],
    pub flags: u32,
    pub max_regions: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InvokeResult {
    pub attempted: bool,
    pub result: i32,
    pub name: [u8; NAME_BYTES],
}
impl Default for InvokeResult {
    fn default() -> Self {
        Self {
            attempted: false,
            result: -1,
            name: [0; NAME_BYTES],
        }
    }
}
#[derive(Clone, Copy)]
struct Registration {
    active: bool,
    admissions_open: bool,
    busy: u32,
    owner: u32,
    name: [u8; NAME_BYTES],
    generation: u64,
    descriptor: Option<Descriptor>,
}
impl Registration {
    const EMPTY: Self = Self {
        active: false,
        admissions_open: false,
        busy: 0,
        owner: 0,
        name: [0; NAME_BYTES],
        generation: 0,
        descriptor: None,
    };
}
struct State {
    registration: Registration,
    generation_counter: u64,
}
struct Cell(UnsafeCell<State>);
// SAFETY: all metadata accesses have the original program-state owner. Exact
// callback admission pins the registration until its completion is reconciled.
unsafe impl Sync for Cell {}
static STATE: Cell = Cell(UnsafeCell::new(State {
    registration: Registration::EMPTY,
    generation_counter: 0,
}));
fn name_equal(stored: &[u8; NAME_BYTES], expected: &[u8]) -> bool {
    let len = stored.iter().position(|&v| v == 0).unwrap_or(NAME_BYTES);
    &stored[..len] == expected
}
pub fn register(owner: u32, name: &[u8], descriptor: Descriptor) -> i32 {
    if owner == 0 || name.is_empty() || name.len() >= NAME_BYTES {
        return -1;
    }
    if descriptor.version != 1 || (descriptor.size as usize) < size_of::<Descriptor>() {
        return -2;
    }
    if descriptor.flags & REQUIRED_FLAGS != REQUIRED_FLAGS || descriptor.max_regions == 0 {
        return -2;
    }
    let _state = ownership::enter_state();
    let state = unsafe { &mut *STATE.0.get() };
    if state.registration.active {
        return -3;
    }
    state.generation_counter = state.generation_counter.wrapping_add(1);
    if state.generation_counter == 0 {
        state.generation_counter = 1
    }
    state.registration = Registration {
        active: true,
        admissions_open: true,
        owner,
        generation: state.generation_counter,
        descriptor: Some(descriptor),
        ..Registration::EMPTY
    };
    state.registration.name[..name.len()].copy_from_slice(name);
    0
}
pub fn unregister(owner: u32, name: &[u8]) -> i32 {
    let _state = ownership::enter_state();
    let state = unsafe { &mut *STATE.0.get() };
    let row = &state.registration;
    if !row.active || row.owner != owner || !name_equal(&row.name, name) {
        return -1;
    }
    if row.busy != 0 {
        return -2;
    }
    state.registration = Registration::EMPTY;
    0
}
pub fn snapshot() -> Snapshot {
    let _state = ownership::enter_state();
    let row = unsafe { &(*STATE.0.get()).registration };
    let Some(descriptor) = row.descriptor else {
        return Snapshot::default();
    };
    if !row.active || !row.admissions_open {
        return Snapshot::default();
    }
    Snapshot {
        active: true,
        name: row.name,
        flags: descriptor.flags,
        max_regions: descriptor.max_regions,
    }
}
struct Call(crate::sched::task_context::Unwind);
impl Drop for Call {
    fn drop(&mut self) {
        ownership::release_call(self.0)
    }
}
/// # Safety
/// Caller retains the job, its pixels/regions and exact driver callback through
/// the synchronous invocation. No callback may retain or alter those inputs.
pub unsafe fn invoke(job: *const Job) -> InvokeResult {
    with_backend(unsafe { (*job).region_count }, false, |descriptor| unsafe {
        (descriptor.present)(descriptor.context, job)
    })
}

fn with_backend(
    region_count: u32,
    qualify: bool,
    callback: impl FnOnce(Descriptor) -> i32,
) -> InvokeResult {
    let call = ownership::retain_call();
    if !call.admitted() {
        return InvokeResult::default();
    }
    let _call = Call(call);
    let (descriptor, generation, owner, name) = {
        let _state = ownership::enter_state();
        let row = unsafe { &mut (*STATE.0.get()).registration };
        let Some(descriptor) = row.descriptor else {
            return InvokeResult::default();
        };
        if !row.active
            || !row.admissions_open
            || region_count > descriptor.max_regions
            || row.busy == u32::MAX
        {
            return InvokeResult::default();
        }
        row.busy += 1;
        (descriptor, row.generation, row.owner, row.name)
    };
    let result = callback(descriptor);
    {
        let _state = ownership::enter_state();
        let row = unsafe { &mut (*STATE.0.get()).registration };
        if row.active && row.owner == owner && row.generation == generation && row.busy != 0 {
            if qualify && let Some(active) = row.descriptor.as_mut() {
                let tested = crate::contract::display_blit_backend_flag_self_tested;
                if result == 0 {
                    active.flags |= tested;
                } else {
                    active.flags &= !tested;
                }
            }
            row.busy -= 1
        }
    }
    InvokeResult {
        attempted: true,
        result,
        name,
    }
}
/// Runs only on an explicit graphics diagnosis. The synchronous driver call
/// borrows private RAM. Its registration remains pinned throughout comparison.
pub extern "C" fn check_copy() -> i32 {
    let mut source = [0_u32; 96];
    for (i, pixel) in source.iter_mut().enumerate() {
        *pixel = 0x1100_0000 | i as u32;
    }
    const CLEAR: u32 = 0xa5a5_a5a5;
    let mut target = [CLEAR; 96];
    let regions = [
        Region {
            dst_x: 2,
            dst_y: 1,
            src_x: 5,
            src_y: 0,
            w: 10,
            h: 2,
        },
        Region {
            dst_x: 20,
            dst_y: 2,
            src_x: 1,
            src_y: 2,
            w: 5,
            h: 1,
        },
    ];
    let job = Job {
        version: 1,
        size: size_of::<Job>() as u32,
        format: 1,
        flags: 0,
        target_address: target.as_mut_ptr().addr() as u64,
        target_width: 32,
        target_height: 3,
        target_pitch_pixels: 32,
        source_address: source.as_ptr().addr() as u64,
        source_stride_pixels: 32,
        source_pixel_count: source.len() as u32,
        region_count: regions.len() as u32,
        regions_address: regions.as_ptr().addr() as u64,
    };
    with_backend(job.region_count, true, |descriptor| {
        let rc = unsafe { (descriptor.present)(descriptor.context, &job) };
        if rc != 0 {
            return rc;
        }
        for (i, actual) in target.iter().enumerate() {
            let (x, y) = (i % 32, i / 32);
            let expected = if y >= 1 && (2..12).contains(&x) {
                source[(y - 1) * 32 + 5 + (x - 2)]
            } else if y == 2 && (20..25).contains(&x) {
                source[2 * 32 + 1 + (x - 20)]
            } else {
                CLEAR
            };
            if *actual != expected {
                return -2;
            }
        }
        0
    })
    .result
}

pub fn prepare_owner_cleanup(owner: u32) -> bool {
    let _state = ownership::enter_state();
    let row = unsafe { &mut (*STATE.0.get()).registration };
    if !row.active || row.owner != owner {
        return true;
    }
    if row.busy != 0 {
        return false;
    }
    row.admissions_open = false;
    true
}
pub fn cancel_owner_cleanup(owner: u32) {
    let _state = ownership::enter_state();
    let row = unsafe { &mut (*STATE.0.get()).registration };
    if row.active && row.owner == owner {
        row.admissions_open = true
    }
}
pub fn cleanup_owner(owner: u32) -> u32 {
    let _state = ownership::enter_state();
    let row = unsafe { &mut (*STATE.0.get()).registration };
    if !row.active || row.owner != owner || row.busy != 0 {
        return 0;
    }
    *row = Registration::EMPTY;
    1
}
/// # Safety
/// Name span and exact initialized descriptor are readable through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_blit_register(
    owner: u32,
    name: *const u8,
    len: usize,
    descriptor: *const Descriptor,
) -> i32 {
    if owner == 0 || len == 0 || len >= NAME_BYTES {
        return -1;
    }
    register(
        owner,
        unsafe { core::slice::from_raw_parts(name, len) },
        unsafe { descriptor.read() },
    )
}
/// # Safety
/// Supplied name span is readable; zero length does not dereference its pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_blit_unregister(
    owner: u32,
    name: *const u8,
    len: usize,
) -> i32 {
    unregister(
        owner,
        if len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(name, len) }
        },
    )
}
/// # Safety
/// Exclusive output C snapshot remains writable through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_blit_snapshot(out: *mut Snapshot) {
    unsafe { out.write(snapshot()) }
}
/// # Safety
/// Same exact pixel/region/callback lifetime as invoke(), with exclusive output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_blit_invoke(job: *const Job, out: *mut InvokeResult) {
    unsafe { out.write(invoke(job)) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_display_blit_cleanup(op: u32, owner: u32) -> u32 {
    match op {
        0 => u32::from(prepare_owner_cleanup(owner)),
        1 => {
            cancel_owner_cleanup(owner);
            0
        }
        2 => cleanup_owner(owner),
        _ => panic!("display blit cleanup operation"),
    }
}
const _: () = {
    use core::mem::{align_of, offset_of};
    assert!(
        size_of::<Region>() == 24
            && size_of::<Job>() == 64
            && size_of::<Descriptor>() == 32
            && align_of::<Descriptor>() == 8
    );
    assert!(offset_of!(Job, regions_address) == 56 && offset_of!(Descriptor, present) == 24);
    assert!(size_of::<Snapshot>() == 36 && offset_of!(Snapshot, flags) == 28);
    assert!(size_of::<InvokeResult>() == 32 && offset_of!(InvokeResult, name) == 8);
};
