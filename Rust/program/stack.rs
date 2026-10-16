//! R4X image/stack resources and atomic per-profile telemetry. Every release
//! retains its exact partial-commit state until the lower VM owner acknowledges.
//! Metrics only control poison/scans/timing; guards and ownership are functional.
use super::{image::Image, memory, registry_types::ProgramInstance, vm};
use crate::{
    boot::{foundation as console, log as bootlog},
    build_config::config,
    contract::ProgramStackPerformanceInfo,
    memory::{vm_query, vm_ranges, vm_transactions},
    platform::timer,
    sched::task::{task_registry, task_resources, task_types::Role},
};
use core::{
    mem::MaybeUninit,
    ptr, slice,
    sync::atomic::{AtomicU64, Ordering},
};

const PAGE: u64 = 4096;
const GUARD: u64 = PAGE;
const GROW: u64 = 64 * 1024;
const PATTERN: u8 = 0xa5;
const DETAIL: bool = config::ENABLE_METRICS;
const PROFILES: usize = 9;
pub(crate) const PROFILE_COUNT: usize = PROFILES;
const PROFILE_NAMES: [&[u8]; PROFILES] = [
    b"unknown",
    b"tiny",
    b"normal",
    b"desktop",
    b"service",
    b"large-service",
    b"build-tool",
    b"browser",
    b"workstation",
];
const PROFILE_TAGS: [&[u8]; PROFILES] = [
    b"unknown",
    b"tiny",
    b"normal",
    b"desktop",
    b"service",
    b"large_service",
    b"build_tool",
    b"browser",
    b"workstation",
];
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stack {
    pub base: u64,
    pub reserve_size: u64,
    pub committed_base: u64,
    pub committed_size: u64,
    pub guard_base: u64,
    pub guard_size: u64,
    pub top: u64,
    pub initial_commit_size: u64,
    pub create_cycles: u64,
    pub telemetry_high_water: u64,
    pub range_id: u32,
    pub owner_id: u32,
    pub telemetry_committed_pages: u32,
    pub profile: u8,
    pub serial_telemetry: bool,
    pub telemetry_measured: bool,
    _padding0: [u8; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Resources {
    pub image_base: u64,
    pub image_size: usize,
    pub stack: Stack,
    pub image_range_id: u32,
    pub image_owner_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Telemetry {
    pub creates: u64,
    pub releases: u64,
    pub reserve_max_bytes: u64,
    pub initial_commit_max_bytes: u64,
    pub committed_max_bytes: u64,
    pub high_water_max_bytes: u64,
    pub create_cycles_total: u64,
    pub create_cycles_max: u64,
    pub release_cycles_total: u64,
    pub release_cycles_max: u64,
}

static TELEMETRY: [[AtomicU64; 10]; PROFILES] =
    [const { [const { AtomicU64::new(0) }; 10] }; PROFILES];
pub(super) fn error(prefix: &[u8], code: u32) {
    console::puts(prefix);
    console::puts(match code {
        1 => b"NotInitialized",
        2 => b"TableFull",
        3 => b"EmptyRange",
        4 => b"BadAlignment",
        5 => b"Overflow",
        6 => b"NoSpace",
        7 => b"Overlap",
        8 => b"NotFound",
        9 => b"AlreadyCommitted",
        10 => b"NotCommitted",
        11 => b"GuardRange",
        12 => b"OutOfMemory",
        13 => b"MapFailed",
        14 => b"Busy",
        15 => b"OutsideWindow",
        _ => unreachable!(),
    });
    console::puts(b"\r\n");
}
// Preserve the original independent reservation fields at this private boundary.
#[allow(clippy::too_many_arguments)]
pub(super) fn reserve(
    window: u8,
    kind: u8,
    owner: u32,
    len: u64,
    alignment: u64,
    name: &'static [u8],
    flags: u64,
    evict: bool,
) -> Result<u32, u32> {
    let request = vm_ranges::Reserve {
        eviction_allowed: evict,
        window,
        kind,
        owner: 3,
        owner_id: u64::from(owner),
        name_ptr: name.as_ptr(),
        name_len: name.len(),
        flags,
        base: 0,
        len,
    };
    let mut id = 0;
    let code = unsafe { vm_ranges::r4os_rust_vm_reserve(&request, alignment, &mut id) };
    if code == 0 { Ok(id) } else { Err(code) }
}
pub(super) fn range(id: u32) -> Option<vm_ranges::Range> {
    let mut out = MaybeUninit::uninit();
    if unsafe { vm_query::r4os_rust_vm_range_info(id, out.as_mut_ptr()) } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
fn record_create(profile: u8, reserve: u64, initial: u64, cycles: u64) {
    let f = &TELEMETRY[profile as usize];
    f[0].fetch_add(1, Ordering::Relaxed);
    f[2].fetch_max(reserve, Ordering::Relaxed);
    f[3].fetch_max(initial, Ordering::Relaxed);
    if DETAIL {
        f[6].fetch_add(cycles, Ordering::Relaxed);
        f[7].fetch_max(cycles, Ordering::Relaxed);
    }
}
fn record_release(stack: Stack, committed: u64, cycles: u64) {
    let f = &TELEMETRY[stack.profile as usize];
    f[1].fetch_add(1, Ordering::Relaxed);
    f[4].fetch_max(committed, Ordering::Relaxed);
    if DETAIL {
        f[5].fetch_max(stack.telemetry_high_water, Ordering::Relaxed);
        f[8].fetch_add(cycles, Ordering::Relaxed);
        f[9].fetch_max(cycles, Ordering::Relaxed);
    }
}
fn telemetry(profile: u8) -> Telemetry {
    let f = &TELEMETRY[profile as usize];
    Telemetry {
        creates: f[0].load(Ordering::Relaxed),
        releases: f[1].load(Ordering::Relaxed),
        reserve_max_bytes: f[2].load(Ordering::Relaxed),
        initial_commit_max_bytes: f[3].load(Ordering::Relaxed),
        committed_max_bytes: f[4].load(Ordering::Relaxed),
        high_water_max_bytes: f[5].load(Ordering::Relaxed),
        create_cycles_total: f[6].load(Ordering::Relaxed),
        create_cycles_max: f[7].load(Ordering::Relaxed),
        release_cycles_total: f[8].load(Ordering::Relaxed),
        release_cycles_max: f[9].load(Ordering::Relaxed),
    }
}
/// # Safety
/// Exclusive resident canonical output. Invalid index leaves it unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_stack_telemetry(
    index: u32,
    out: *mut ProgramStackPerformanceInfo,
) -> bool {
    if index as usize >= PROFILES {
        return false;
    }
    let s = telemetry(index as u8);
    let mut info = ProgramStackPerformanceInfo {
        kind: 1,
        flags: u32::from(DETAIL),
        created: s.creates,
        released: s.releases,
        reserve_max_bytes: s.reserve_max_bytes,
        initial_commit_max_bytes: s.initial_commit_max_bytes,
        committed_max_bytes: s.committed_max_bytes,
        high_water_max_bytes: s.high_water_max_bytes,
        create_cycles_total: s.create_cycles_total,
        create_cycles_max: s.create_cycles_max,
        release_cycles_total: s.release_cycles_total,
        release_cycles_max: s.release_cycles_max,
        ..ProgramStackPerformanceInfo::default()
    };
    let name = PROFILE_TAGS[index as usize];
    info.name[..name.len()].copy_from_slice(name);
    unsafe { out.write(info) };
    true
}
fn timestamp() -> u64 {
    if !DETAIL {
        return 0;
    }
    let lo: u32;
    let hi: u32;
    unsafe { core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack)) };
    u64::from(lo) | (u64::from(hi) << 32)
}
unsafe fn poison(base: u64, len: u64) {
    if DETAIL && base != 0 && len != 0 {
        unsafe {
            ptr::write_bytes(
                ptr::with_exposed_provenance_mut::<u8>(base as usize),
                PATTERN,
                len as usize,
            )
        };
    }
}
/// # Safety
/// Exact resident stack record and its committed span retain their owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_stack_measure(stack: *mut Stack) {
    unsafe {
        if (*stack).telemetry_measured {
            return;
        }
        (*stack).telemetry_measured = true;
        (*stack).telemetry_committed_pages =
            u32::try_from((*stack).committed_size / PAGE).unwrap_or(u32::MAX);
        if !DETAIL || (*stack).committed_base == 0 || (*stack).top <= (*stack).committed_base {
            return;
        }
        let len = ((*stack).top - (*stack).committed_base) as usize;
        let bytes = ptr::with_exposed_provenance::<u8>((*stack).committed_base as usize);
        let mut untouched = 0;
        while untouched < len && bytes.add(untouched).read() == PATTERN {
            untouched += 1;
        }
        (*stack).telemetry_high_water = (len - untouched) as u64;
    }
}
struct Trace {
    bytes: [MaybeUninit<u8>; 1536],
    len: usize,
    truncated: bool,
}
impl Trace {
    fn new() -> Self {
        Self {
            bytes: [MaybeUninit::uninit(); 1536],
            len: 0,
            truncated: false,
        }
    }
    fn puts(&mut self, text: &[u8]) {
        let count = text.len().min(self.bytes.len() - self.len);
        unsafe {
            ptr::copy_nonoverlapping(
                text.as_ptr(),
                self.bytes.as_mut_ptr().cast::<u8>().add(self.len),
                count,
            )
        };
        self.len += count;
        self.truncated |= count != text.len();
    }
    fn dec(&mut self, mut value: u64) {
        let mut bytes = [0u8; 20];
        let mut start = bytes.len();
        loop {
            start -= 1;
            bytes[start] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.puts(&bytes[start..]);
    }
    fn finish(&mut self) {
        if self.truncated {
            let marker = b" [trace truncated]\r\n";
            self.len = self.bytes.len() - marker.len();
            self.puts(marker);
        }
        let bytes = unsafe { slice::from_raw_parts(self.bytes.as_ptr().cast::<u8>(), self.len) };
        unsafe { bootlog::r4os_rust_bootlog_write(bytes.as_ptr(), bytes.len()) };
        console::puts(bytes);
    }
}
fn log_release(stack: Stack, committed: u64, cycles: u64, stats: Telemetry) {
    let mut task_stats = task_resources::StackTelemetry::default();
    for role in [
        Role::Input,
        Role::ShortCompletion,
        Role::Interactive,
        Role::Batch,
    ] {
        let s = task_resources::telemetry_for(role);
        task_stats.created = task_stats.created.wrapping_add(s.created);
        task_stats.released = task_stats.released.wrapping_add(s.released);
        task_stats.high_water_total_bytes = task_stats
            .high_water_total_bytes
            .wrapping_add(s.high_water_total_bytes);
        task_stats.high_water_max_bytes =
            task_stats.high_water_max_bytes.max(s.high_water_max_bytes);
        task_stats.create_cycles_total = task_stats
            .create_cycles_total
            .wrapping_add(s.create_cycles_total);
        task_stats.create_cycles_max = task_stats.create_cycles_max.max(s.create_cycles_max);
        task_stats.release_cycles_total = task_stats
            .release_cycles_total
            .wrapping_add(s.release_cycles_total);
        task_stats.release_cycles_max = task_stats.release_cycles_max.max(s.release_cycles_max);
    }
    // These passive scalar reads match the existing task cache/reserve views.
    let (cached, hits, misses, used) = unsafe {
        let r = task_registry::raw();
        (
            (*r).stack_cache_count as u64,
            (*r).stack_cache_hits,
            (*r).stack_cache_misses,
            (*r).critical_in_use as u32,
        )
    };
    let mut t = Trace::new();
    t.puts(b"[R4XSTACK] release owner=");
    t.dec(u64::from(stack.owner_id));
    t.puts(if DETAIL {
        b" detail=on"
    } else {
        b" detail=off"
    });
    t.puts(b" profile=");
    t.puts(PROFILE_NAMES[stack.profile as usize]);
    for (name, value) in [
        (b" reserve=".as_slice(), stack.reserve_size),
        (b" initial=".as_slice(), stack.initial_commit_size),
        (b" committed=".as_slice(), committed),
        (b" highwater=".as_slice(), stack.telemetry_high_water),
        (b" creates=".as_slice(), stats.creates),
        (b" releases=".as_slice(), stats.releases),
        (b" create_cycles=".as_slice(), stack.create_cycles),
        (b" create_cycles_max=".as_slice(), stats.create_cycles_max),
        (b" release_cycles=".as_slice(), cycles),
        (b" release_cycles_max=".as_slice(), stats.release_cycles_max),
        (
            b" kernel_highwater_max=".as_slice(),
            task_stats.high_water_max_bytes,
        ),
        (
            b" kernel_create_cycles_max=".as_slice(),
            task_stats.create_cycles_max,
        ),
        (
            b" kernel_release_cycles_max=".as_slice(),
            task_stats.release_cycles_max,
        ),
        (b" kernel_cache_cached=".as_slice(), cached),
        (b" kernel_cache_hits=".as_slice(), hits),
        (b" kernel_cache_misses=".as_slice(), misses),
        (b" critical_available=".as_slice(), u64::from(4 - used)),
        (b" critical_in_use=".as_slice(), u64::from(used)),
    ] {
        t.puts(name);
        t.dec(value);
    }
    t.puts(b"\r\n");
    t.finish();
}
/// # Safety
/// Instance/runtime and stack remain resident under the original execution pin.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_stack_log(
    instance: *const ProgramInstance,
    stack: *const Stack,
    thread: u32,
) {
    unsafe {
        let runtime = super::payload::runtime(instance);
        let module = crate::fs::vfs_types::Bytes {
            data: ptr::addr_of!((*runtime).module_path).cast(),
            len: ((*runtime).module_path_len as usize).min(128),
        };
        let s = stack.read();
        let mut t = Trace::new();
        t.puts(b"[R4XSTACK] highwater owner=");
        t.dec(u64::from((*instance).id));
        t.puts(if DETAIL {
            b" detail=on"
        } else {
            b" detail=off"
        });
        t.puts(b" thread=");
        t.dec(u64::from(thread));
        t.puts(b" module=");
        if module.len == 0 {
            t.puts(b"unknown");
        } else {
            t.puts(slice::from_raw_parts(module.data, module.len));
        }
        t.puts(b" profile=");
        t.puts(PROFILE_NAMES[s.profile as usize]);
        for (name, value) in [
            (b" reserve=".as_slice(), s.reserve_size),
            (b" initial=".as_slice(), s.initial_commit_size),
            (b" committed=".as_slice(), s.committed_size),
            (b" highwater=".as_slice(), s.telemetry_high_water),
            (b" create_cycles=".as_slice(), s.create_cycles),
        ] {
            t.puts(name);
            t.dec(value);
        }
        t.puts(b"\r\n");
        t.finish();
    }
}
/// # Safety
/// Exclusive image output; the returned VM range remains owned until release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_image_allocate(
    size: usize,
    owner: u32,
    out: *mut Image,
) -> bool {
    let Some(len) = memory::page_align(size as u64) else {
        return false;
    };
    let id = match reserve(2, 5, owner, len, PAGE, b"r4x-program-image", 2, true) {
        Ok(id) => id,
        Err(code) => {
            error(b"Program image reserve failed: ", code);
            return false;
        }
    };
    let code = vm_transactions::r4os_rust_vm_commit(id, 0, len);
    if code != 0 {
        error(b"Program image commit failed: ", code);
        vm_transactions::r4os_rust_vm_release(id);
        return false;
    }
    let Some(info) = range(id) else {
        vm_transactions::r4os_rust_vm_release(id);
        return false;
    };
    unsafe {
        out.write(Image {
            range_id: id,
            code: ptr::with_exposed_provenance_mut(info.base as usize),
            len: size,
            owner_id: owner,
        })
    };
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_image_release(id: u32) -> bool {
    if id == 0 {
        return true;
    }
    matches!(vm_transactions::r4os_rust_vm_release(id), 0 | 8)
}
/// # Safety
/// Exclusive stack output; profile is an admitted original MemoryProfile byte.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_stack_allocate(
    owner: u32,
    reserve_size: u64,
    initial: u64,
    profile: u8,
    serial: bool,
    out: *mut Stack,
) -> bool {
    let started = timestamp();
    let Some(reserve_len) = memory::page_align(reserve_size) else {
        return false;
    };
    let Some(initial_len) = memory::page_align(initial) else {
        return false;
    };
    if reserve_len <= GUARD || initial_len > reserve_len - GUARD {
        return false;
    }
    let id = match reserve(
        4,
        7,
        owner,
        reserve_len,
        PAGE,
        b"r4x-app-stack",
        2 | (1 << 63),
        true,
    ) {
        Ok(id) => id,
        Err(code) => {
            error(b"Program stack reserve failed: ", code);
            return false;
        }
    };
    let Some(info) = range(id) else {
        vm_transactions::r4os_rust_vm_release(id);
        return false;
    };
    let top = info.base + info.len;
    let committed_base = top - initial_len;
    let guard_base = committed_base - GUARD;
    let code =
        vm_transactions::r4os_rust_vm_protect_guard(id, guard_base - info.base, GUARD, false);
    if code != 0 {
        error(b"Program stack guard failed: ", code);
        vm_transactions::r4os_rust_vm_release(id);
        return false;
    }
    let code = vm_transactions::r4os_rust_vm_commit(id, committed_base - info.base, initial_len);
    if code != 0 {
        error(b"Program stack commit failed: ", code);
        vm_transactions::r4os_rust_vm_release(id);
        return false;
    }
    unsafe { poison(committed_base, initial_len) };
    let cycles = timestamp().wrapping_sub(started);
    record_create(profile, info.len, initial_len, cycles);
    unsafe {
        out.write(Stack {
            base: info.base,
            reserve_size: info.len,
            committed_base,
            committed_size: initial_len,
            guard_base,
            guard_size: GUARD,
            top,
            initial_commit_size: initial_len,
            create_cycles: cycles,
            range_id: id,
            owner_id: owner,
            profile,
            serial_telemetry: serial,
            ..Stack::default()
        })
    };
    true
}
unsafe fn finish_release(
    stack: *mut Stack,
    committed: u64,
    started_tick: u64,
    started_cycles: u64,
) -> bool {
    let finished = if DETAIL { timer::tick_count() } else { 0 };
    let ticks = finished.saturating_sub(started_tick);
    let cycles = timestamp().wrapping_sub(started_cycles);
    let s = unsafe { stack.read() };
    if ticks >= 25 {
        console::puts(b"[R4XTHREAD] slow stack release ticks=");
        console::dec(ticks);
        console::puts(b" reserve=");
        console::dec(s.reserve_size);
        console::puts(b" committed=");
        console::dec(committed);
        console::puts(b"\r\n");
    }
    record_release(s, committed, cycles);
    if s.serial_telemetry {
        log_release(s, committed, cycles, telemetry(s.profile));
    }
    unsafe { stack.write(Stack::default()) };
    true
}
/// # Safety
/// Exact retained stack record; failed release preserves its retry anchor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_stack_free(stack: *mut Stack) -> bool {
    unsafe {
        if (*stack).range_id == 0 {
            return true;
        }
        r4os_rust_program_stack_measure(stack);
        let committed = u64::from((*stack).telemetry_committed_pages) * PAGE;
        let started_tick = if DETAIL { timer::tick_count() } else { 0 };
        let started_cycles = timestamp();
        if (*stack).committed_size != 0 && (*stack).committed_base >= (*stack).base {
            let code = vm_transactions::r4os_rust_vm_uncommit(
                (*stack).range_id,
                (*stack).committed_base - (*stack).base,
                (*stack).committed_size,
            );
            if code != 0 {
                error(b"Program stack committed-span release failed: ", code);
                let code = vm_transactions::r4os_rust_vm_release((*stack).range_id);
                if code != 0 && code != 8 {
                    error(b"Program stack deferred release failed: ", code);
                    return false;
                }
                return finish_release(stack, committed, started_tick, started_cycles);
            }
            (*stack).committed_size = 0;
        }
        let code = vm_transactions::r4os_rust_vm_release((*stack).range_id);
        if code != 0 && code != 8 {
            error(b"Program stack release failed: ", code);
            return false;
        }
        finish_release(stack, committed, started_tick, started_cycles)
    }
}
pub(super) unsafe fn from_instance(instance: *const ProgramInstance) -> Option<Stack> {
    unsafe {
        if (*instance).program_stack_range_id == 0 {
            return None;
        }
        Some(Stack {
            range_id: (*instance).program_stack_range_id,
            base: (*instance).program_stack_base,
            reserve_size: (*instance).program_stack_reserve_size,
            committed_base: (*instance).program_stack_committed_base,
            committed_size: (*instance).program_stack_committed_size,
            guard_base: (*instance).program_stack_guard_base,
            guard_size: (*instance).program_stack_guard_size,
            top: (*instance).stack_top,
            owner_id: (*instance).id,
            profile: (*instance).memory_profile,
            initial_commit_size: (*instance).program_stack_initial_commit_size,
            create_cycles: (*instance).program_stack_create_cycles,
            telemetry_high_water: (*instance).program_stack_telemetry_high_water,
            telemetry_committed_pages: (*instance).program_stack_telemetry_committed_pages,
            serial_telemetry: (*instance).program_stack_serial_telemetry,
            telemetry_measured: (*instance).program_stack_telemetry_measured,
            ..Stack::default()
        })
    }
}
/// # Safety
/// Resident exact instance and exclusive output under its original lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_stack_from_instance(
    instance: *const ProgramInstance,
    out: *mut Stack,
) -> bool {
    let Some(stack) = (unsafe { from_instance(instance) }) else {
        return false;
    };
    unsafe { out.write(stack) };
    true
}
/// # Safety
/// Exact resident instance and immutable independent stack snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_stack_to_instance(
    instance: *mut ProgramInstance,
    stack: *const Stack,
) {
    unsafe {
        let s = stack.read();
        (*instance).program_stack_range_id = s.range_id;
        (*instance).program_stack_base = s.base;
        (*instance).program_stack_reserve_size = s.reserve_size;
        (*instance).program_stack_committed_base = s.committed_base;
        (*instance).program_stack_committed_size = s.committed_size;
        (*instance).program_stack_guard_base = s.guard_base;
        (*instance).program_stack_guard_size = s.guard_size;
        (*instance).program_stack_initial_commit_size = s.initial_commit_size;
        (*instance).program_stack_create_cycles = s.create_cycles;
        (*instance).program_stack_telemetry_high_water = s.telemetry_high_water;
        (*instance).program_stack_telemetry_committed_pages = s.telemetry_committed_pages;
        (*instance).program_stack_serial_telemetry = s.serial_telemetry;
        (*instance).program_stack_telemetry_measured = s.telemetry_measured;
        (*instance).stack_top = s.top;
    }
}
/// # Safety
/// Original instance lifetime and exclusive resource value output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_resources_instance(
    instance: *const ProgramInstance,
    out: *mut Resources,
) {
    unsafe {
        out.write(Resources {
            image_range_id: (*instance).program_image_range_id,
            image_base: (*instance).program_image_base,
            image_size: (*instance).program_image_size,
            image_owner_id: (*instance).id,
            stack: from_instance(instance).unwrap_or_default(),
        })
    };
}
/// # Safety
/// Independent immutable image/stack values and exclusive resource output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_resources_values(
    image: *const Image,
    stack: *const Stack,
    out: *mut Resources,
) {
    unsafe {
        out.write(Resources {
            image_range_id: (*image).range_id,
            image_base: (*image).code.addr() as u64,
            image_size: (*image).len,
            image_owner_id: (*image).owner_id,
            stack: stack.read(),
        })
    };
}
/// # Safety
/// Exact retained resource record; each successful subrelease is written back.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_resources_cleanup(resources: *mut Resources) -> bool {
    unsafe {
        if (*resources).stack.range_id != 0
            && !r4os_rust_program_stack_free(ptr::addr_of_mut!((*resources).stack))
        {
            return false;
        }
        if !r4os_rust_program_image_release((*resources).image_range_id) {
            return false;
        }
        (*resources).image_range_id = 0;
        (*resources).image_base = 0;
        (*resources).image_size = 0;
        (*resources).image_owner_id = 0;
    }
    true
}
/// # Safety
/// Resident independent values under their original failed-launch lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_resources_free(
    image: *const Image,
    stack: *const Stack,
) {
    let mut resources = MaybeUninit::<Resources>::uninit();
    unsafe {
        r4os_rust_program_resources_values(image, stack, resources.as_mut_ptr());
        r4os_rust_program_resources_cleanup(resources.as_mut_ptr());
    }
}
pub(super) unsafe fn contains_instance(instance: *const ProgramInstance, address: u64) -> bool {
    unsafe {
        if (*instance).program_stack_range_id == 0 || (*instance).program_stack_reserve_size == 0 {
            return false;
        }
        let end = (*instance).program_stack_base + (*instance).program_stack_reserve_size;
        address >= (*instance).program_stack_base && address < end
    }
}
pub(super) unsafe fn contains(stack: *const Stack, address: u64) -> bool {
    unsafe {
        if (*stack).range_id == 0 || (*stack).reserve_size == 0 {
            return false;
        }
        let end = (*stack).base + (*stack).reserve_size;
        address >= (*stack).base && address < end
    }
}
pub(super) unsafe fn grow_instance(instance: *mut ProgramInstance, address: u64) -> bool {
    unsafe {
        let Some(mut stack) = from_instance(instance) else {
            return false;
        };
        let before = stack.committed_size;
        if !grow(&mut stack, address) {
            return false;
        }
        r4os_rust_program_stack_to_instance(instance, &stack);
        if stack.serial_telemetry {
            blog(b"[R4X] stack grow owner=");
            bdec(u64::from((*instance).id));
            blog(b" committed=");
            bdec(before);
            blog(b" -> ");
            bdec(stack.committed_size);
            blog(b"\r\n");
        }
        true
    }
}
pub(super) fn blog(text: &[u8]) {
    unsafe { bootlog::r4os_rust_bootlog_write(text.as_ptr(), text.len()) };
}
pub(super) fn bdec(value: u64) {
    bootlog::r4os_rust_bootlog_dec(value);
}
pub(super) unsafe fn grow(stack: *mut Stack, address: u64) -> bool {
    unsafe {
        if (*stack).range_id == 0 || (*stack).guard_size == 0 {
            return false;
        }
        let fault_page = address & !(PAGE - 1);
        if fault_page < (*stack).base + (*stack).guard_size {
            if fault_page >= (*stack).base {
                blog(b"[R4X] STACK EXHAUSTED fault=");
                bootlog::r4os_rust_bootlog_hex(address, 16);
                blog(b" base=");
                bootlog::r4os_rust_bootlog_hex((*stack).base, 16);
                blog(b" committed_base=");
                bootlog::r4os_rust_bootlog_hex((*stack).committed_base, 16);
                blog(b" reserve=");
                bdec((*stack).reserve_size);
                blog(b"\r\n");
                console::puts(b"[R4X] STACK EXHAUSTED (app stack reserve aufgebraucht)\r\n");
            }
            return false;
        }
        if fault_page >= (*stack).committed_base {
            return false;
        }
        let mut desired = (*stack).committed_base
            - GROW.min((*stack).committed_base - ((*stack).base + (*stack).guard_size));
        if desired > fault_page {
            desired = fault_page;
        }
        if desired < (*stack).base + (*stack).guard_size {
            desired = (*stack).base + (*stack).guard_size;
        }
        if desired >= (*stack).committed_base {
            return false;
        }
        let len = (*stack).committed_base - desired;
        if !vm::system_limit(len) {
            return false;
        }
        let old_guard_base = (*stack).guard_base;
        let old_guard_size = (*stack).guard_size;
        if vm_transactions::r4os_rust_vm_protect_guard((*stack).range_id, 0, 0, true) != 0 {
            return false;
        }
        let code =
            vm_transactions::r4os_rust_vm_commit((*stack).range_id, desired - (*stack).base, len);
        if code != 0 {
            error(b"R4X stack grow failed: ", code);
            vm_transactions::r4os_rust_vm_protect_guard(
                (*stack).range_id,
                old_guard_base - (*stack).base,
                old_guard_size,
                false,
            );
            return false;
        }
        let new_guard_base = desired - (*stack).guard_size;
        let code = vm_transactions::r4os_rust_vm_protect_guard(
            (*stack).range_id,
            new_guard_base - (*stack).base,
            (*stack).guard_size,
            false,
        );
        if code != 0 {
            error(b"R4X stack guard move failed: ", code);
            vm_transactions::r4os_rust_vm_uncommit((*stack).range_id, desired - (*stack).base, len);
            vm_transactions::r4os_rust_vm_protect_guard(
                (*stack).range_id,
                old_guard_base - (*stack).base,
                old_guard_size,
                false,
            );
            return false;
        }
        poison(desired, len);
        (*stack).committed_base = desired;
        (*stack).committed_size += len;
        (*stack).guard_base = new_guard_base;
        true
    }
}
/// # Safety
/// Resident exact instance and exclusive scalar output. operation 0=reserve,
/// 1=initial commit; the latter receives an already admitted stack reserve.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_stack_limit(
    operation: u8,
    instance: *const ProgramInstance,
    requested: u64,
    out: *mut u64,
) -> bool {
    unsafe {
        let value = match operation {
            0 => {
                let raw = if requested == 0 {
                    (*instance).memory_limits.stack_reserve
                } else {
                    requested
                };
                let Some(len) = memory::page_align(raw) else {
                    return false;
                };
                if len < 64 * 1024
                    || len <= GUARD
                    || len > (*instance).memory_limits.vm_reserve_limit
                {
                    return false;
                }
                len
            }
            1 => {
                let Some(mut initial) =
                    memory::page_align((*instance).memory_limits.stack_initial_commit)
                else {
                    return false;
                };
                let max = requested - GUARD;
                if initial > max {
                    initial = max;
                }
                if initial == 0 {
                    return false;
                }
                initial
            }
            _ => unreachable!(),
        };
        out.write(value);
        true
    }
}

const _: () = {
    assert!(core::mem::size_of::<Stack>() == 96);
    assert!(core::mem::align_of::<Stack>() == 8);
    assert!(core::mem::offset_of!(Stack, range_id) == 80);
    assert!(core::mem::offset_of!(Stack, base) == 0);
    assert!(core::mem::offset_of!(Stack, reserve_size) == 8);
    assert!(core::mem::offset_of!(Stack, committed_base) == 16);
    assert!(core::mem::offset_of!(Stack, committed_size) == 24);
    assert!(core::mem::offset_of!(Stack, guard_base) == 32);
    assert!(core::mem::offset_of!(Stack, guard_size) == 40);
    assert!(core::mem::offset_of!(Stack, top) == 48);
    assert!(core::mem::offset_of!(Stack, owner_id) == 84);
    assert!(core::mem::offset_of!(Stack, profile) == 92);
    assert!(core::mem::offset_of!(Stack, initial_commit_size) == 56);
    assert!(core::mem::offset_of!(Stack, create_cycles) == 64);
    assert!(core::mem::offset_of!(Stack, telemetry_high_water) == 72);
    assert!(core::mem::offset_of!(Stack, telemetry_committed_pages) == 88);
    assert!(core::mem::offset_of!(Stack, serial_telemetry) == 93);
    assert!(core::mem::offset_of!(Stack, telemetry_measured) == 94);
    assert!(core::mem::size_of::<Resources>() == 120);
    assert!(core::mem::align_of::<Resources>() == 8);
    assert!(core::mem::offset_of!(Resources, image_range_id) == 112);
    assert!(core::mem::offset_of!(Resources, image_base) == 0);
    assert!(core::mem::offset_of!(Resources, image_size) == 8);
    assert!(core::mem::offset_of!(Resources, image_owner_id) == 116);
    assert!(core::mem::offset_of!(Resources, stack) == 16);
    assert!(core::mem::size_of::<Telemetry>() == 80);
    assert!(core::mem::align_of::<Telemetry>() == 8);
    assert!(core::mem::offset_of!(Telemetry, creates) == 0);
    assert!(core::mem::offset_of!(Telemetry, releases) == 8);
    assert!(core::mem::offset_of!(Telemetry, reserve_max_bytes) == 16);
    assert!(core::mem::offset_of!(Telemetry, initial_commit_max_bytes) == 24);
    assert!(core::mem::offset_of!(Telemetry, committed_max_bytes) == 32);
    assert!(core::mem::offset_of!(Telemetry, high_water_max_bytes) == 40);
    assert!(core::mem::offset_of!(Telemetry, create_cycles_total) == 48);
    assert!(core::mem::offset_of!(Telemetry, create_cycles_max) == 56);
    assert!(core::mem::offset_of!(Telemetry, release_cycles_total) == 64);
    assert!(core::mem::offset_of!(Telemetry, release_cycles_max) == 72);
};
