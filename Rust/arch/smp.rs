//! Sequential AP startup/shutdown, private low-memory trampolines and IPIs.
//! The BSP owns configuration until execution publication; terminal shutdown
//! owns the stop counter. Native per-CPU lifecycle atomics publish AP progress.
//! Raw Task callbacks bind the native task owner without a second lifecycle.
use super::{
    ap_admission,
    apic_target::{self, Target},
    fpu, lapic, msr, pat, percpu, runtime,
    smp_policy::{self, Candidate, MAX_CPUS, Plan},
    tables, tlb,
};
use crate::{
    arch::pit,
    build_config::config,
    memory::{blocks, paging, phys},
    platform::{cpu, cpu_topology, monotonic},
};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
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
    epoch: u64,
    shared_1g: bool,
}
struct Storage(UnsafeCell<Boot>);
// SAFETY: startup/activation are single BSP operations before public task
// admission; shutdown is terminal after new public queries have closed. No
// state reference crosses logging, native/foreign calls, or bounded waits.
unsafe impl Sync for Storage {}
static BOOT: Storage = Storage(UnsafeCell::new(Boot {
    status: Status::EMPTY,
    topology: Plan::EMPTY,
    reported_failures: 0,
    low_pages: [0; MAX_CPUS],
    epoch: 0,
    shared_1g: false,
}));
#[repr(align(64))]
struct BootStacks(UnsafeCell<[[u8; STACK_SIZE]; MAX_CPUS]>);
// SAFETY: each AP alone owns the private stack after its trampoline receives
// the raw top address. Rust never lends or initializes a live AP stack slice.
unsafe impl Sync for BootStacks {}
static STACKS: BootStacks = BootStacks(UnsafeCell::new([[0; STACK_SIZE]; MAX_CPUS]));
static RELEASE_EPOCH: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
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
    epoch: u64,
    apic_id: u32,
    early_failure: u32,
    observed_id: u32,
    page_constraints: u32,
}
unsafe extern "C" {
    static r4os_ap_trampoline_start: u8;
    static r4os_ap_trampoline_protected_mode: u8;
    static r4os_ap_trampoline_long_mode: u8;
    static r4os_ap_trampoline_end: u8;
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
    let _runtime = runtime::RuntimeGuard::acquire();
    unsafe { ptr::addr_of!((*boot()).status).read() }
}
fn boot_epoch() -> u64 {
    unsafe { ptr::addr_of!((*boot()).epoch).read() }
}
fn epoch(index: u32) -> u64 {
    cpu_topology::epoch(index)
}
fn publish(value: Status) {
    let _runtime = runtime::RuntimeGuard::acquire();
    unsafe { ptr::addr_of_mut!((*boot()).status).write(value) }
}
fn snapshot() -> Status {
    let mut value = current();
    value.online = 0;
    value.failed = 0;
    for index in 0..configured() {
        match percpu::state(index) {
            4 | percpu::DRAINING | percpu::PARKED | percpu::STOP_PENDING => value.online += 1,
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
    u32::try_from(id).ok()
}
fn identity_target(index: u32) -> Option<Target> {
    if index >= MAX_CPUS as u32 {
        return None;
    }
    let id = apic(index)?;
    let generation = if index == 0 {
        // The BSP has one confirmed identity for this boot, including the
        // early phase before the AP plan/epoch has been published.
        if lapic::bsp_apic_id() != Some(id) || !pat::qualified(0) {
            return None;
        }
        1
    } else {
        let generation = epoch(index);
        if !ap_admission::qualified(index, id, generation) {
            return None;
        }
        generation
    };
    Some(Target {
        index,
        apic_id: id,
        epoch: generation,
    })
}
fn live_target(index: u32) -> Option<Target> {
    if percpu::state(index) != 4 || percpu::r4os_rust_percpu_control(13, index, 0, 0) == 0 {
        return None;
    }
    identity_target(index)
}
pub(crate) fn legacy_irq_target(ordinal: u32) -> Option<Target> {
    apic_target::select(ordinal, live_target)
}
pub(crate) fn irq_message(target: Target, vector: u32) -> Option<apic_target::Message> {
    if !apic_target::current(target, live_target) {
        return None;
    }
    apic_target::message(u64::from(target.apic_id), vector)
}
pub(crate) fn irq_redirection(id: u32) -> Option<u32> {
    for index in 0..MAX_CPUS as u32 {
        let Some(target) = live_target(index).filter(|t| t.apic_id == id) else {
            continue;
        };
        if apic_target::current(target, live_target) {
            return apic_target::redirection_high(u64::from(id));
        }
    }
    None
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
        if wanted == 3 && startup_failure(index).is_some() {
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
fn early_rejection(index: u32) -> Option<*const TrampolineConfig> {
    let low = low_page(index);
    if low == 0 || progress(index) != 0xfe {
        return None;
    }
    core::sync::atomic::fence(Ordering::Acquire);
    let config =
        (phys::r4os_rust_phys_to_virt(low) as usize + CONFIG_OFFSET) as *const TrampolineConfig;
    if unsafe { ptr::addr_of!((*config).magic).read_unaligned() } != 0x5234_4f53_4150_3031
        || unsafe { ptr::addr_of!((*config).cpu_index).read_unaligned() } != u64::from(index)
        || unsafe { ptr::addr_of!((*config).epoch).read_unaligned() } != epoch(index)
        || unsafe { ptr::addr_of!((*config).apic_id).read_unaligned() } != apic(index)?
    {
        return None;
    }
    Some(config)
}
fn startup_failure(index: u32) -> Option<&'static [u8]> {
    if let Some(reason) = ap_admission::failure(index, epoch(index)) {
        return Some(reason);
    }
    let config = early_rejection(index)?;
    Some(
        match unsafe { ptr::addr_of!((*config).early_failure).read_volatile() } {
            1 => b"msr",
            2 => b"pae",
            3 => b"long_mode",
            4 => b"nx",
            5 => b"ap-identity",
            6 => b"ap-bootstrap-config",
            7 => b"1g-pages",
            _ => b"ap-bootstrap",
        },
    )
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
        // No earlier boot's progress/epoch may be interpreted before this
        // boot prepares the private trampoline (including pre-SIPI failure).
        unsafe {
            ptr::write_bytes(phys::r4os_rust_phys_to_virt(page) as *mut u8, 0, PAGE);
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
    let root = unsafe { r4os_read_cr3() } & 0x000f_ffff_ffff_f000;
    // The SIPI trampoline loads a 32-bit CR3 into the established four-level
    // layout. Unsupported roots exclude the AP instead of truncating CR3.
    if root == 0 || root > u64::from(u32::MAX) || unsafe { r4os_read_cr4() } & (1 << 12) != 0 {
        log(7, index, root, b"root-not-representable");
        return None;
    }
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
        cr3: root,
        cr0: 0x8001_0033,
        cr4: 1 << 5,
        // Only LME and shared-page NXE. The AP validates both locally before
        // WRMSR; no optional BSP EFER state is inherited.
        efer: (unsafe { msr::read(0xc000_0080) } & (1 << 11)) | (1 << 8),
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
        epoch: epoch(index),
        apic_id: apic(index)?,
        early_failure: 0,
        observed_id: u32::MAX,
        page_constraints: u32::from(unsafe { ptr::addr_of!((*boot()).shared_1g).read() }),
    };
    unsafe {
        page.add(CONFIG_OFFSET)
            .cast::<TrampolineConfig>()
            .write_unaligned(config)
    };
    Some(low)
}
fn launch_secondary(index: u32, id: u32) -> bool {
    if !lapic::can_target(id) {
        fail_ap(index, false, b"apic-transport");
        return false;
    }
    if index == config::SMP_FAIL_AP_INDEX {
        fail_ap(index, false, b"diagnostic-injection");
        return false;
    }
    let Some(page) = prepare_trampoline(index) else {
        fail_ap(index, false, b"trampoline");
        return false;
    };
    if !crate::sched::scheduler::prepare_secondary(index) {
        fail_ap(index, false, b"idle-task");
        return false;
    }
    if !transition(index, 1, 2) {
        return false;
    }
    let vector = u8::try_from(page >> 12).expect("SIPI vector overflow");
    if lapic::r4os_rust_lapic_control(4, u64::from(id), u32::from(vector)) == 0 {
        fail_ap(index, false, b"init-sipi");
        return false;
    }
    if !wait_for(index, 3, START_TIMEOUT) {
        if let Some(reason) = startup_failure(index) {
            fail_ap(index, false, reason);
        } else {
            fail_ap(index, true, b"startup-timeout");
        }
        return false;
    }
    if !ap_admission::qualified(index, id, epoch(index))
        || percpu::state(index) != 3
        || !crate::platform::clocksource::finalize_cpu(index)
        || !transition(index, 3, 8)
    {
        fail_ap(index, false, b"clock-registration");
        return false;
    }
    pat::log_ap(index, epoch(index) > boot_epoch());
    log(8, index, epoch(index), b"qualified");
    true
}
fn start(plan: Plan) -> bool {
    if current().initialized {
        return true;
    }
    if !lapic::is_enabled() {
        cpu_topology::seal(1);
        publish(Status {
            initialized: true,
            ..Status::EMPTY
        });
        log(3, 0, 0, b"lapic-unavailable");
        return true;
    }
    let count = plan.count;
    log(9, plan.omitted, u64::from(plan.invalid), b"");
    let value = Status {
        initialized: true,
        discovered: count,
        duplicates: plan.duplicates,
        disabled: plan.disabled,
        ..Status::EMPTY
    };
    unsafe { ptr::addr_of_mut!((*boot()).topology).write(plan) };
    publish(value);
    let start_epoch = boot_epoch().checked_add(1).expect("AP epoch overflow");
    let ids = unsafe { ptr::addr_of!((*boot()).topology.apic_ids).read() };
    if !cpu_topology::seed_plan(&ids[..count as usize], start_epoch) {
        crate::boot::fatal::kernel(11, b"cpu-topology-plan");
    }
    let root = unsafe { r4os_read_cr3() } & 0x000f_ffff_ffff_f000;
    let Some(shared_1g) = paging::ap_boot_uses_1g(root) else {
        log(7, 0, root, b"shared-root-unconfirmed");
        return false;
    };
    unsafe {
        ptr::addr_of_mut!((*boot()).epoch).write(start_epoch);
        ptr::addr_of_mut!((*boot()).shared_1g).write(shared_1g);
    }
    for release in &RELEASE_EPOCH {
        release.store(0, Ordering::Release);
    }
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
        if !ap_admission::prepare(index, id, epoch(index)) {
            fail_ap(index, false, b"ap-admission-identity");
            continue;
        }
        if !launch_secondary(index, id) {
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
    cpu_topology::seal(percpu::r4os_rust_percpu_control(6, 0, 0, 0));
    for index in 1..configured() {
        if percpu::state(index) == 8
            && apic(index).is_some_and(|id| ap_admission::qualified(index, id, epoch(index)))
        {
            RELEASE_EPOCH[index as usize].store(epoch(index), Ordering::Release);
        }
    }
    for index in 1..configured() {
        let before = percpu::state(index);
        if before == 8
            && !apic(index).is_some_and(|id| ap_admission::qualified(index, id, epoch(index)))
        {
            fail_ap(index, false, b"ap-admission-identity");
            continue;
        }
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
    super::cpu_lifecycle::terminal();
    if !current().initialized {
        return;
    }
    for index in 1..configured() {
        let before = percpu::state(index);
        if matches!(
            before,
            percpu::PARKED | percpu::STOP_PENDING | percpu::OFFLINE
        ) {
            continue;
        }
        let Some(target) = identity_target(index) else {
            continue;
        };
        if !matches!(before, 4 | percpu::DRAINING) || !transition(index, before, 5) {
            continue;
        }
        if percpu::state(index) == 5 && apic_target::current(target, identity_target) {
            lapic::r4os_rust_lapic_control(6, u64::from(target.apic_id), 0xf1);
        }
    }
    for index in 1..configured() {
        if !matches!(
            percpu::state(index),
            5 | percpu::PARKED | percpu::STOP_PENDING
        ) {
            continue;
        }
        if !wait_for(index, 6, STOP_TIMEOUT) {
            let mut value = current();
            value.stop_timeouts += 1;
            publish(value);
        }
    }
}
pub(crate) fn send_reschedule(index: u32) {
    if index == percpu::current_index() {
        return;
    }
    if let Some(target) = live_target(index).filter(|t| apic_target::current(*t, live_target)) {
        lapic::r4os_rust_lapic_control(5, u64::from(target.apic_id), 0xf0);
    }
}
unsafe extern "C" fn send_tlb_ipi(index: u32, vector: u8) -> bool {
    if index == percpu::current_index() {
        return false;
    }
    let Some(target) = identity_target(index).filter(|_| percpu::tlb_mask() & (1u64 << index) != 0)
    else {
        return false;
    };
    lapic::r4os_rust_lapic_control(7, u64::from(target.apic_id), u32::from(vector)) != 0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_ap_entry(index: u64, start_epoch: u64, apic_id: u64) -> ! {
    if index == 0 || index >= MAX_CPUS as u64 {
        runtime::halt_forever();
    }
    let index = index as u32;
    let reject = |reason: &'static [u8]| -> ! {
        ap_admission::reject(index, start_epoch, reason);
        runtime::halt_forever()
    };
    if apic_id > u64::from(u32::MAX) {
        reject(b"ap-identity");
    }
    if let Err(reason) = ap_admission::begin(index, apic_id as u32, start_epoch) {
        reject(reason);
    }
    // Rust's bare target uses integer registers only. The module baseline
    // and common XSAVE image must be admitted before any FPU/SIMD operation.
    if let Some(reason) = crate::platform::module_cpu::hardware_failure() {
        reject(reason);
    }
    if !lapic::local_capabilities_valid() {
        reject(b"lapic-capabilities");
    }
    if crate::platform::clocksource::active_source() == 1
        && core::arch::x86_64::__cpuid_count(1, 0).edx & (1 << 4) == 0
    {
        reject(b"tsc-capabilities");
    }
    tables::r4os_rust_gdt_init(index);
    percpu::r4os_rust_percpu_control(1, index, 0, 0);
    super::cpu_lifecycle::install(index, start_epoch);
    tlb::flush_all_local();
    tables::r4os_rust_idt_load();
    if percpu::state(index) != 2 {
        runtime::halt_forever();
    }
    if let Err(reason) = pat::init_ap(index) {
        reject(reason);
    }
    if let Err(reason) = fpu::init_ap(index) {
        reject(reason);
    }
    if !ap_admission::qualify(index, start_epoch) {
        runtime::halt_forever();
    }
    if lapic::r4os_rust_lapic_control(1, 0, 0) == 0 {
        percpu::r4os_rust_percpu_control(5, index, 0, 0);
        runtime::halt_forever();
    }
    if !cpu_topology::capture(
        index,
        apic_id as u32,
        start_epoch,
        core::arch::x86_64::__cpuid_count,
    ) {
        reject(b"cpu-topology-identity");
    }
    if !crate::sched::scheduler::init_secondary(index) {
        percpu::r4os_rust_percpu_control(5, index, 0, 0);
        runtime::halt_forever();
    }
    crate::platform::clocksource::register_cpu(index);
    if !transition(index, 2, 3) {
        runtime::halt_forever();
    }
    while RELEASE_EPOCH[index as usize].load(Ordering::Acquire) != start_epoch {
        let state = percpu::state(index);
        if state == 7 || state == 6 || super::cpu_lifecycle::is_terminal() {
            runtime::halt_forever();
        }
        core::hint::spin_loop();
    }
    if percpu::state(index) != 8 {
        runtime::halt_forever();
    }
    if !ap_admission::qualified(index, apic_id as u32, start_epoch) {
        runtime::halt_forever();
    }
    if lapic::r4os_rust_lapic_control(3, 0, pit::DEFAULT_HZ) == 0 {
        percpu::r4os_rust_percpu_control(5, index, 0, 0);
        runtime::halt_forever();
    }
    if !publish_online(index, start_epoch) {
        runtime::halt_forever();
    }
    crate::sched::scheduler::secondary_loop()
}
fn publish_online(index: u32, start_epoch: u64) -> bool {
    let started = monotonic::now_nanoseconds();
    for _ in 0..20_000_000 {
        if percpu::state(index) != 8
            || epoch(index) != start_epoch
            || super::cpu_lifecycle::is_terminal()
            || started.is_some_and(|start| {
                monotonic::now_nanoseconds()
                    .is_some_and(|now| now.saturating_sub(start) >= START_TIMEOUT)
            })
        {
            return false;
        }
        // Runtime precedes PageTableOwner everywhere else too. Both attempts
        // are bounded; no owner is retained while waiting for either one.
        if let Some(_runtime) = runtime::RuntimeGuard::try_acquire()
            && let Some(_pages) = crate::memory::owner::Guard::try_acquire(6)
        {
            if percpu::state(index) != 8 || epoch(index) != start_epoch {
                return false;
            }
            // Serialize the final full/global flush and membership grant
            // against the exact invalidation request/mapping owner. A request
            // cannot miss the new CPU after this barrier.
            tlb::flush_all_local();
            return transition(index, 8, 4);
        }
        tlb::service_pending();
        core::hint::spin_loop();
    }
    false
}
/// Permanent private bootstrap/idle storage is retained across logical stops.
pub(crate) fn idle_stack_contains(index: u32, rsp: u64) -> bool {
    if index == 0 || index as usize >= MAX_CPUS {
        return false;
    }
    let start = unsafe { STACKS.0.get().cast::<u8>().add(index as usize * STACK_SIZE) } as u64;
    (start..start + STACK_SIZE as u64).contains(&rsp)
}
pub(crate) fn request_drain_reschedule(index: u32, expected: u64) -> bool {
    let Some(target) = identity_target(index)
        .filter(|t| t.epoch == expected && percpu::state(index) == percpu::DRAINING)
    else {
        return false;
    };
    lapic::r4os_rust_lapic_control(5, u64::from(target.apic_id), 0xf0) != 0
}
pub(crate) fn notify_irq(target: Target, vector: u32) -> bool {
    apic_target::current(target, live_target)
        && lapic::r4os_rust_lapic_control(7, u64::from(target.apic_id), vector) != 0
}
/// Exact stop consumption and owner withdrawal precede a fresh execution epoch.
/// The same real trampoline/qualification path is used for boot and reuse.
pub(crate) fn restart(stopped: &super::cpu_lifecycle::Stopped) -> bool {
    if !stopped.current() {
        return false;
    }
    let index = stopped.index();
    let Some(next) = stopped.epoch().checked_add(1) else {
        return false;
    };
    {
        let Some(_tables) = crate::memory::owner::Guard::try_acquire(6) else {
            return false;
        };
        if !tlb::rearm_cpu(stopped) {
            return false;
        }
    }
    if !ap_admission::rearm(stopped, next) || !cpu_topology::rearm(stopped, next) {
        return false;
    }
    RELEASE_EPOCH[index as usize].store(0, Ordering::Release);
    unsafe {
        let old = ptr::addr_of!((*boot()).reported_failures).read();
        ptr::addr_of_mut!((*boot()).reported_failures).write(old & !(1u64 << index));
    }
    if !percpu::rearm(stopped, next) || !launch_secondary(index, stopped.apic_id()) {
        return false;
    }
    if !cpu_topology::qualify_epoch(index, next) {
        fail_ap(index, false, b"cpu-requalification");
        return false;
    }
    RELEASE_EPOCH[index as usize].store(next, Ordering::Release);
    if !wait_for(index, 4, START_TIMEOUT) {
        fail_ap(index, true, b"activation-timeout");
        return false;
    }
    true
}
/// The complete boot decoder already selected bounded dense slots. No raw
/// firmware list or temporary borrow survives AP notification.
pub(crate) fn start_firmware(plan: Plan) -> bool {
    if current().initialized {
        return true;
    }
    if !lapic::is_enabled() {
        return start(Plan::EMPTY);
    }
    let Some(bsp) = lapic::bsp_apic_id() else {
        return false;
    };
    if apic(0) != Some(bsp) {
        crate::boot::fatal::kernel(11, b"bsp-apic-identity");
    }
    let plan = if plan.count == 0 {
        Plan::new(bsp)
    } else {
        plan
    };
    if plan.count as usize > MAX_CPUS || plan.apic_ids[0] != bsp {
        crate::boot::fatal::kernel(11, b"firmware-cpu-plan");
    }
    start(plan)
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
        return start(Plan::EMPTY);
    }
    let plan = {
        let values = if len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(candidates, len) }
        };
        let Some(bsp) = lapic::bsp_apic_id() else {
            return false;
        };
        if apic(0) != Some(bsp) {
            crate::boot::fatal::kernel(11, b"bsp-apic-identity");
        }
        smp_policy::plan(bsp, values)
    };
    start(plan)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_smp_control(operation: u8, index: u32) -> u64 {
    match operation {
        0 => {
            let id = cpu::local_apic_id();
            if id == u32::MAX {
                crate::boot::fatal::kernel(2, b"bsp-apic-identity");
            }
            if !cpu_topology::capture(0, id, 1, core::arch::x86_64::__cpuid_count) {
                crate::boot::fatal::kernel(2, b"bsp-topology-identity");
            }
            percpu::r4os_rust_percpu_control(0, id, 0, 0);
            super::cpu_lifecycle::install(0, 1);
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
        5 => legacy_irq_target(index).map_or(u64::MAX, |t| u64::from(t.apic_id)),
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
    assert!(core::mem::size_of::<TrampolineConfig>() == 112);
    assert!(core::mem::offset_of!(TrampolineConfig, progress) == 0x4e);
    assert!(core::mem::offset_of!(TrampolineConfig, protected_offset) == 0x50);
    assert!(core::mem::offset_of!(TrampolineConfig, epoch) == 0x58);
    assert!(core::mem::offset_of!(TrampolineConfig, apic_id) == 0x60);
    assert!(core::mem::offset_of!(TrampolineConfig, early_failure) == 0x64);
    assert!(core::mem::offset_of!(TrampolineConfig, observed_id) == 0x68);
    assert!(core::mem::offset_of!(TrampolineConfig, page_constraints) == 0x6c);
};

unsafe fn smp_log(
    event: u8,
    index: u32,
    value: u64,
    text: *const u8,
    len: usize,
    status: *const Status,
) {
    // Runtime CPU control reports through its explicit caller. Retain fresh
    // qualification/failure records without injecting them into queued module
    // text/JSON. Cold-boot diagnostics keep their original console route.
    let record_only = matches!(event, 1 | 2 | 8) && epoch(index) > boot_epoch();
    let puts: fn(&[u8]) = if record_only {
        crate::boot::log::text
    } else {
        crate::boot::log_route::puts
    };
    let dec: fn(u64) = if record_only {
        |value| crate::boot::log::r4os_rust_bootlog_dec(value)
    } else {
        crate::boot::log_route::dec
    };
    match event {
        0 => {
            puts(b"[SMP] low-memory trampolines reserved=");
            dec(value);
        }
        1 | 2 => {
            // Only an authenticated AP result carries observed identity.
            // Pre-SIPI failures retain the original diagnostic marker.
            let early = early_rejection(index);
            let admission_failure = ap_admission::failure(index, epoch(index)).is_some();
            puts(b"[SMP] ap=");
            dec(u64::from(index));
            puts(b" apic=");
            dec(u64::from(apic(index).unwrap_or(0)));
            if early.is_some() || admission_failure {
                puts(b" epoch=");
                dec(epoch(index));
                puts(b" observed-apic=");
                if let Some(config) = early {
                    dec(u64::from(unsafe {
                        ptr::addr_of!((*config).observed_id).read_volatile()
                    }));
                } else if let Some(id) = ap_admission::observed(index, epoch(index)) {
                    dec(u64::from(id));
                } else {
                    puts(b"unknown");
                }
            }
            puts(b" failed=");
            puts(unsafe { core::slice::from_raw_parts(text, len) });
            if early.is_some() {
                puts(b" phase=pre-cr4");
            } else if admission_failure {
                let reason = unsafe { core::slice::from_raw_parts(text, len) };
                puts(
                    if [
                        b"osxsave".as_slice(),
                        b"xcr0",
                        b"xsave-size",
                        b"fpu-controls",
                    ]
                    .contains(&reason)
                    {
                        b" phase=state-readback"
                    } else {
                        b" phase=pre-fpu-controls"
                    },
                );
            }
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
        7 => {
            puts(b"[SMP] bootstrap-root rejected=");
            puts(unsafe { core::slice::from_raw_parts(text, len) });
        }
        8 => {
            puts(b"[SMP] ap=");
            dec(u64::from(index));
            puts(b" apic=");
            dec(u64::from(apic(index).unwrap_or(0)));
            puts(b" epoch=");
            dec(value);
            puts(b" qualification=allowed baseline=r4os-avx2-v1 xcr0=7 image=verified");
        }
        9 => {
            puts(b"[SMP] capacity=");
            dec(MAX_CPUS as u64);
            puts(b" omitted-eligible-records=");
            dec(u64::from(index));
            puts(b" invalid-apic-records=");
            dec(value);
        }
        _ => unreachable!(),
    }
    puts(b"\r\n");
}
