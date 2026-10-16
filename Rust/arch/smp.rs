//! Sequential AP startup/shutdown, private low-memory trampolines and IPIs.
//! The BSP owns configuration until execution publication; terminal shutdown
//! owns the stop counter. Native per-CPU lifecycle atomics publish AP progress.
//! Raw Task callbacks bind the native task owner without a second lifecycle.
use super::{
    fpu, lapic, msr, percpu, runtime,
    smp_policy::{self, Candidate, MAX_CPUS, Plan},
    tables, tlb,
};
use crate::{
    arch::pit,
    build_config::config,
    memory::{blocks, paging, phys},
    platform::{cpu, monotonic},
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
const PAGE: usize = 4096;
const CONFIG_OFFSET: usize = 0x800;
const GDT_OFFSET: usize = 0x900;
const STACK_SIZE: usize = 64 * 1024;
const START_TIMEOUT: u64 = 250_000_000;
const STOP_TIMEOUT: u64 = 100_000_000;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub initialized: bool,
    pub activated: bool,
    pub discovered: u32,
    pub started: u32,
    pub online: u32,
    pub failed: u32,
    pub duplicates: u32,
    pub disabled: u32,
    pub startup_timeouts: u32,
    pub stop_timeouts: u32,
}
impl Status {
    const EMPTY: Self = Self {
        initialized: false,
        activated: false,
        discovered: 1,
        started: 0,
        online: 1,
        failed: 0,
        duplicates: 0,
        disabled: 0,
        startup_timeouts: 0,
        stop_timeouts: 0,
    };
}
struct Boot {
    status: Status,
    topology: Plan,
    reported_failures: u64,
    low_pages: [u64; MAX_CPUS],
}
struct Storage(UnsafeCell<Boot>);
// SAFETY: startup/activation are single BSP operations before public task
// admission; shutdown is terminal after new public queries have closed. No
// state reference crosses logging, native/foreign calls, or bounded waits.
unsafe impl Sync for Storage {}
static BOOT: Storage = Storage(UnsafeCell::new(Boot {
    status: Status::EMPTY,
    topology: Plan {
        apic_ids: [0; MAX_CPUS],
        count: 0,
        duplicates: 0,
        disabled: 0,
    },
    reported_failures: 0,
    low_pages: [0; MAX_CPUS],
}));
#[repr(align(64))]
struct BootStacks(UnsafeCell<[[u8; STACK_SIZE]; MAX_CPUS]>);
// SAFETY: each AP alone owns the private stack after its trampoline receives
// the raw top address. Rust never lends or initializes a live AP stack slice.
unsafe impl Sync for BootStacks {}
static STACKS: BootStacks = BootStacks(UnsafeCell::new([[0; STACK_SIZE]; MAX_CPUS]));
static RELEASE: AtomicBool = AtomicBool::new(false);
#[repr(C, packed)]
struct TrampolineConfig {
    magic: u64,
    cr3: u64,
    cr0: u64,
    cr4: u64,
    efer: u64,
    entry: u64,
    stack: u64,
    cpu_index: u64,
    far_offset: u32,
    far_selector: u16,
    far_padding: u16,
    gdt_limit: u16,
    gdt_base: u32,
    progress: u8,
    progress_padding: u8,
    protected_offset: u32,
    protected_selector: u16,
    protected_padding: u16,
}
unsafe extern "C" {
    static r4os_ap_trampoline_start: u8;
    static r4os_ap_trampoline_protected_mode: u8;
    static r4os_ap_trampoline_long_mode: u8;
    static r4os_ap_trampoline_end: u8;
    fn r4os_read_cr0() -> u64;
    fn r4os_read_cr3() -> u64;
    fn r4os_read_cr4() -> u64;

}
fn boot() -> *mut Boot {
    BOOT.0.get()
}
fn configured() -> u32 {
    unsafe { ptr::addr_of!((*boot()).topology.count).read() }.max(1)
}
fn low_page(index: u32) -> u64 {
    unsafe {
        ptr::addr_of!((*boot()).low_pages)
            .cast::<u64>()
            .add(index as usize)
            .read()
    }
}
fn current() -> Status {
    unsafe { ptr::addr_of!((*boot()).status).read() }
}
fn publish(value: Status) {
    unsafe { ptr::addr_of_mut!((*boot()).status).write(value) }
}
fn snapshot() -> Status {
    let mut value = current();
    value.online = 0;
    value.failed = 0;
    for index in 0..configured() {
        match percpu::state(index) {
            4 => value.online += 1,
            7 => value.failed += 1,
            _ => {}
        }
    }
    value
}
fn log(event: u8, index: u32, value: u64, text: &[u8]) {
    let status = snapshot();
    unsafe { smp_log(event, index, value, text.as_ptr(), text.len(), &status) };
}
fn apic(index: u32) -> Option<u32> {
    let id = percpu::r4os_rust_percpu_control(8, index, 0, 0);
    if id == u64::MAX {
        None
    } else {
        Some(id as u32)
    }
}
fn transition(index: u32, from: u8, to: u8) -> bool {
    percpu::r4os_rust_percpu_control(4, index, u64::from(from), to) != 0
}
fn progress(index: u32) -> u8 {
    if index as usize >= MAX_CPUS {
        return 0;
    }
    let low = low_page(index);
    if low == 0 {
        return 0;
    }
    unsafe {
        ptr::with_exposed_provenance::<u8>(
            (phys::r4os_rust_phys_to_virt(low) as usize) + CONFIG_OFFSET + 0x4e,
        )
        .read_volatile()
    }
}
fn wait_for(index: u32, wanted: u8, timeout: u64) -> bool {
    let started = monotonic::now_nanoseconds();
    let mut spins = 0u64;
    while percpu::state(index) != wanted {
        let state = percpu::state(index);
        if state == 7 || state == 6 {
            return false;
        }
        if let Some(start) = started {
            if monotonic::now_nanoseconds()
                .unwrap_or(start)
                .wrapping_sub(start)
                >= timeout
            {
                return false;
            }
        } else if spins >= 100_000_000 {
            return false;
        }
        core::hint::spin_loop();
        spins += 1;
    }
    true
}
fn fail_ap(index: u32, timeout: bool, reason: &[u8]) {
    if percpu::r4os_rust_percpu_control(5, index, 0, 0) == 0 {
        return;
    }
    let mask = unsafe { ptr::addr_of!((*boot()).reported_failures).read() };
    let bit = 1u64 << index;
    if mask & bit != 0 {
        return;
    }
    unsafe { ptr::addr_of_mut!((*boot()).reported_failures).write(mask | bit) };
    if timeout {
        let mut value = current();
        value.startup_timeouts += 1;
        publish(value);
    }
    log(
        if timeout { 2 } else { 1 },
        index,
        u64::from(progress(index)),
        reason,
    );
}
fn reserve_low_memory() {
    let mut reserved = 0u32;
    for index in 1..MAX_CPUS as u32 {
        let mut page = 0;
        if !unsafe { phys::r4os_rust_phys_contiguous(1, 0x000f_ffff, false, &mut page) } {
            break;
        }
        let block = blocks::Block::physical(
            page,
            PAGE as u64,
            1,
            0,
            u64::from(index),
            1,
            b"smp-trampoline",
        );
        let mut id = 0;
        if unsafe { blocks::r4os_rust_blocks_range(&block, false, &mut id) } != 0 {
            phys::r4os_rust_phys_free_contiguous(page, 1);
            break;
        }
        unsafe {
            ptr::addr_of_mut!((*boot()).low_pages)
                .cast::<u64>()
                .add(index as usize)
                .write(page)
        };
        reserved += 1;
    }
    log(0, 0, u64::from(reserved), b"");
}
fn prepare_trampoline(index: u32) -> Option<u64> {
    let start = ptr::addr_of!(r4os_ap_trampoline_start).addr();
    let protected = ptr::addr_of!(r4os_ap_trampoline_protected_mode).addr();
    let long = ptr::addr_of!(r4os_ap_trampoline_long_mode).addr();
    let end = ptr::addr_of!(r4os_ap_trampoline_end).addr();
    if end <= start || end - start >= CONFIG_OFFSET {
        log(4, 0, end.saturating_sub(start) as u64, b"");
        return None;
    }
    let low = low_page(index);
    if low == 0 {
        log(5, 0, 0, b"");
        return None;
    }
    let mut ignored = 0;
    if !unsafe { paging::r4os_rust_paging_read(low, 3, false, &mut ignored) }
        && !paging::r4os_rust_paging_map(low, low, 2, false)
    {
        log(6, 0, 0, b"");
        return None;
    }
    let page = ptr::with_exposed_provenance_mut::<u8>(phys::r4os_rust_phys_to_virt(low) as usize);
    unsafe {
        ptr::write_bytes(page, 0, PAGE);
        ptr::copy_nonoverlapping(ptr::addr_of!(r4os_ap_trampoline_start), page, end - start)
    };
    let gdt = [
        0u64,
        0x00af_9a00_0000_ffff,
        0x00cf_9200_0000_ffff,
        0x00cf_9a00_0000_ffff,
    ];
    unsafe { page.add(GDT_OFFSET).cast::<[u64; 4]>().write_unaligned(gdt) };
    let stack =
        unsafe { STACKS.0.get().cast::<u8>().add(index as usize * STACK_SIZE) }.addr() + STACK_SIZE;
    let config = TrampolineConfig {
        magic: 0x5234_4f53_4150_3031,
        cr3: unsafe { r4os_read_cr3() },
        cr0: unsafe { r4os_read_cr0() },
        cr4: unsafe { r4os_read_cr4() },
        // LMA is read-only current-CPU state and must not enter the AP WRMSR.
        efer: (unsafe { msr::read(0xc000_0080) } & !(1 << 10)) | (1 << 8),
        entry: r4os_ap_entry as *const () as u64,
        stack: stack as u64,
        cpu_index: u64::from(index),
        far_offset: u32::try_from(low + (long - start) as u64).expect("AP long jump overflow"),
        far_selector: 8,
        far_padding: 0,
        gdt_limit: 31,
        gdt_base: u32::try_from(low + GDT_OFFSET as u64).expect("AP GDT address overflow"),
        progress: 0,
        progress_padding: 0,
        protected_offset: u32::try_from(low + (protected - start) as u64)
            .expect("AP protected jump overflow"),
        protected_selector: 24,
        protected_padding: 0,
    };
    unsafe {
        page.add(CONFIG_OFFSET)
            .cast::<TrampolineConfig>()
            .write_unaligned(config)
    };
    Some(low)
}
fn start(plan: Plan) -> bool {
    if current().initialized {
        return true;
    }
    if !lapic::is_enabled() {
        publish(Status {
            initialized: true,
            ..Status::EMPTY
        });
        log(3, 0, 0, b"lapic-unavailable");
        return true;
    }
    let count = plan.count;
    let value = Status {
        initialized: true,
        discovered: count,
        duplicates: plan.duplicates,
        disabled: plan.disabled,
        ..Status::EMPTY
    };
    unsafe { ptr::addr_of_mut!((*boot()).topology).write(plan) };
    publish(value);
    RELEASE.store(false, Ordering::Release);
    if count > 1 {
        runtime::r4os_rust_runtime_control(6, 0);
    }
    for index in 1..count {
        let id = unsafe {
            ptr::addr_of!((*boot()).topology.apic_ids)
                .cast::<u32>()
                .add(index as usize)
                .read()
        };
        if percpu::r4os_rust_percpu_control(3, index, u64::from(id), 0) == 0 {
            continue;
        }
        if index == config::SMP_FAIL_AP_INDEX {
            fail_ap(index, false, b"diagnostic-injection");
            continue;
        }
        let Some(page) = prepare_trampoline(index) else {
            fail_ap(index, false, b"trampoline");
            continue;
        };
        if !crate::sched::scheduler::prepare_secondary(index) {
            fail_ap(index, false, b"idle-task");
            continue;
        }
        if !transition(index, 1, 2) {
            continue;
        }
        let vector = u8::try_from(page >> 12).expect("SIPI vector overflow");
        if lapic::r4os_rust_lapic_control(4, u64::from(id), u32::from(vector)) == 0 {
            fail_ap(index, false, b"init-sipi");
            continue;
        }
        if !wait_for(index, 3, START_TIMEOUT) {
            fail_ap(index, true, b"startup-timeout");
            continue;
        }
        if percpu::state(index) != 3
            || !monotonic::r4os_rust_monotonic_control(5, index)
            || !transition(index, 3, 8)
        {
            fail_ap(index, false, b"clock-registration");
            continue;
        }
        let mut value = current();
        value.started += 1;
        publish(value);
    }
    log(3, 0, 0, b"parked");
    true
}
fn activate() {
    let value = current();
    if !value.initialized || value.activated {
        return;
    }
    unsafe { tlb::r4os_rust_tlb_register(Some(send_tlb_ipi)) };
    RELEASE.store(true, Ordering::Release);
    for index in 1..configured() {
        let before = percpu::state(index);
        if before == 8 && !wait_for(index, 4, START_TIMEOUT) {
            fail_ap(index, true, b"activation-timeout");
        } else if before == 7 {
            fail_ap(index, false, b"ap-init");
        }
    }
    let mut value = current();
    value.activated = true;
    publish(value);
    log(3, 0, 0, b"active");
}
fn stop_others() {
    if !current().initialized {
        return;
    }
    for index in 1..configured() {
        if !transition(index, 4, 5) {
            continue;
        }
        if let Some(id) = apic(index) {
            lapic::r4os_rust_lapic_control(6, u64::from(id), 0xf1);
        }
    }
    for index in 1..configured() {
        if percpu::state(index) != 5 {
            continue;
        }
        if !wait_for(index, 6, STOP_TIMEOUT) {
            let mut value = current();
            value.stop_timeouts += 1;
            publish(value);
        }
    }
}
fn send_reschedule(index: u32) {
    if index == percpu::current_index() || percpu::r4os_rust_percpu_control(13, index, 0, 0) == 0 {
        return;
    }
    if let Some(id) = apic(index) {
        lapic::r4os_rust_lapic_control(5, u64::from(id), 0xf0);
    }
}
unsafe extern "C" fn send_tlb_ipi(index: u32, vector: u8) -> bool {
    if index == percpu::current_index() || percpu::state(index) != 4 {
        return false;
    }
    let Some(id) = apic(index) else {
        return false;
    };
    lapic::r4os_rust_lapic_control(7, u64::from(id), u32::from(vector)) != 0
}
fn irq_target(ordinal: u32) -> u32 {
    let mask = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let bsp = unsafe {
        ptr::addr_of!((*boot()).topology.apic_ids)
            .cast::<u32>()
            .read()
    };
    if mask.count_ones() <= 1 {
        return bsp;
    }
    let mut remaining = ordinal % mask.count_ones();
    for index in 0..MAX_CPUS as u32 {
        if mask & (1 << index) == 0 {
            continue;
        }
        if remaining == 0 {
            return apic(index).unwrap_or(bsp);
        }
        remaining -= 1;
    }
    bsp
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_ap_entry(index: u64) -> ! {
    if index == 0 || index >= MAX_CPUS as u64 {
        runtime::halt_forever();
    }
    let index = index as u32;
    tables::r4os_rust_gdt_init(index);
    percpu::r4os_rust_percpu_control(1, index, 0, 0);
    tables::r4os_rust_idt_load();
    if percpu::state(index) != 2 {
        runtime::halt_forever();
    }
    if fpu::r4os_rust_fpu_control(1) == 0
        || lapic::r4os_rust_lapic_control(1, 0, 0) == 0
        || !crate::sched::scheduler::init_secondary(index)
    {
        percpu::r4os_rust_percpu_control(5, index, 0, 0);
        runtime::halt_forever();
    }
    monotonic::r4os_rust_monotonic_control(4, index);
    if !transition(index, 2, 3) {
        runtime::halt_forever();
    }
    while !RELEASE.load(Ordering::Acquire) {
        let state = percpu::state(index);
        if state == 7 || state == 6 {
            runtime::halt_forever();
        }
        core::hint::spin_loop();
    }
    if percpu::state(index) != 8 {
        runtime::halt_forever();
    }
    if lapic::r4os_rust_lapic_control(3, 0, pit::DEFAULT_HZ) == 0 {
        percpu::r4os_rust_percpu_control(5, index, 0, 0);
        runtime::halt_forever();
    }
    if !transition(index, 8, 4) {
        runtime::halt_forever();
    }
    crate::sched::scheduler::secondary_loop()
}
/// # Safety
/// Caller retains len resident initialized Candidate records, with no concurrent
/// mutation. The input borrow ends before AP notification or scheduler calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_smp_start(candidates: *const Candidate, len: usize) -> bool {
    if current().initialized {
        return true;
    }
    if !lapic::is_enabled() {
        return start(Plan {
            apic_ids: [0; MAX_CPUS],
            count: 0,
            duplicates: 0,
            disabled: 0,
        });
    }
    let plan = {
        let values = if len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(candidates, len) }
        };
        smp_policy::plan(lapic::r4os_rust_lapic_control(2, 0, 0) as u32, values)
    };
    start(plan)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_smp_control(operation: u8, index: u32) -> u64 {
    match operation {
        0 => {
            percpu::r4os_rust_percpu_control(0, cpu::r4os_rust_cpu_control(7, 0) as u32, 0, 0);
            0
        }
        1 => {
            reserve_low_memory();
            0
        }
        2 => {
            activate();
            0
        }
        3 => {
            stop_others();
            0
        }
        4 => {
            send_reschedule(index);
            0
        }
        5 => u64::from(irq_target(index)),
        6 => u64::from(configured()),
        _ => panic!("Invalid private SMP operation"),
    }
}
/// # Safety
/// Caller retains a fixed aligned exclusive C output through this passive copy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_smp_status(output: *mut Status) {
    unsafe { output.write(snapshot()) }
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 36);
    assert!(core::mem::offset_of!(Status, discovered) == 4);
    assert!(core::mem::size_of::<TrampolineConfig>() == 88);
    assert!(core::mem::offset_of!(TrampolineConfig, progress) == 0x4e);
    assert!(core::mem::offset_of!(TrampolineConfig, protected_offset) == 0x50);
};

unsafe fn smp_log(
    event: u8,
    index: u32,
    value: u64,
    text: *const u8,
    len: usize,
    status: *const Status,
) {
    use crate::boot::log_route::{dec, puts};
    match event {
        0 => {
            puts(b"[SMP] low-memory trampolines reserved=");
            dec(value);
        }
        1 | 2 => {
            puts(b"[SMP] ap=");
            dec(u64::from(index));
            puts(b" apic=");
            dec(u64::from(apic(index).unwrap_or(0)));
            puts(b" failed=");
            puts(unsafe { core::slice::from_raw_parts(text, len) });
            if event == 2 {
                puts(b" progress=");
                dec(value);
            }
        }
        3 => {
            let s = unsafe { status.read() };
            puts(b"[SMP] stage=");
            puts(unsafe { core::slice::from_raw_parts(text, len) });
            puts(b" discovered=");
            dec(u64::from(s.discovered));
            puts(b" started=");
            dec(u64::from(s.started));
            puts(b" online=");
            dec(u64::from(s.online));
            puts(b" failed=");
            dec(u64::from(s.failed));
            puts(b" fallback=");
            puts(if s.online == 1 { b"1cpu" } else { b"no" });
        }
        4 => {
            puts(b"[SMP] trampoline invalid size=");
            dec(value);
        }
        5 => puts(b"[SMP] trampoline low-memory reservation missing"),
        6 => puts(b"[SMP] trampoline identity mapping failed"),
        _ => unreachable!(),
    }
    puts(b"\r\n");
}
