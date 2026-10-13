//! Boot root construction, device aliases and exact root-switch rollback.
//! The singleton BSP mutates this resident owner before AP publication;
//! afterward it is a read-only diagnostic catalogue.
use super::{
    blocks, layout,
    page_tables::{Builder, RangeStats, walk},
    paging, phys,
};
use crate::boot::info;
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU64, Ordering},
};
const PAGE: u64 = 4096;
const MASK: u64 = 0x000f_ffff_ffff_f000;
const HUGE: u64 = 1 << 7;
const HUGE_BYTES: u64 = 2 * layout::MIB;
const RW: u64 = 2 | (1 << 63);
const RO: u64 = 1 << 63;
const DIRECT: u64 = RW | (1 << 8);
const FB: u64 = RW | (1 << 3) | (1 << 7);
const MMIO: u64 = RW | (1 << 4);
const PROBES: u32 = (1 << 9) - 1;
const POST: u32 = (1 << 8) - 1;
const DEV_FB: u32 = 1;
const DEV_LAPIC: u32 = 1 << 1;
const DEV_IOAPIC: u32 = 1 << 2;
const DEV_HPET: u32 = 1 << 3;
const DEV_ACPI: u32 = 1 << 4;
const DEV_ECAM: u32 = 1 << 5;
const DEV_RESET: u32 = 1 << 7;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub boot_sanity_runs: u64,
    pub boot_sanity_failures: u64,
    pub last_sanity_root_phys: u64,
    pub last_sanity_table_frames: u64,
    pub last_sanity_frame: u64,
    pub kernel_space_runs: u64,
    pub kernel_space_failures: u64,
    pub kernel_space_root_phys: u64,
    pub kernel_space_table_frames: u64,
    pub kernel_space_source_root_phys: u64,
    pub kernel_space_hhdm_offset: u64,
    pub hhdm_pages_4k: u64,
    pub hhdm_pages_2m: u64,
    pub kernel_image_pages: u64,
    pub kernel_image_skipped: u64,
    pub boot_probe_pages: u64,
    pub boot_probe_skipped: u64,
    pub probe_bits: u32,
    pub debug_probe_bit: u32,
    pub debug_probe_virt: u64,
    pub debug_probe_step: u64,
    pub debug_probe_source_phys: u64,
    pub debug_probe_existing_phys: u64,
    pub uses_limine_subtables: bool,
    pub device_space_runs: u64,
    pub device_space_failures: u64,
    pub device_mapping_count: u64,
    pub device_mapping_truncated: bool,
    pub device_probe_bits: u32,
    pub framebuffer_bytes: u64,
    pub acpi_bytes: u64,
    pub mmio_bytes: u64,
    pub pcie_bar_count: u64,
    pub pcie_bar_bytes: u64,
    pub cr3_switch_runs: u64,
    pub cr3_switch_failures: u64,
    pub old_cr3: u64,
    pub new_cr3: u64,
    pub active_cr3: u64,
    pub post_switch_bits: u32,
    pub cr3_switch_done: bool,
    pub heap_committed_bytes: u64,
    pub heap_committed_pages: u64,
    pub heap_committed_skipped: u64,
    pub limine_pt_scan_runs: u64,
    pub limine_pt_scan_failures: u64,
    pub limine_old_table_frames: u64,
    pub limine_active_table_frames: u64,
    pub limine_referenced_frames: u64,
    pub limine_release_candidates: u64,
    pub limine_quarantined_frames: u64,
    pub limine_released_frames: u64,
    pub limine_retained_frames: u64,
    pub limine_quarantine_failures: u64,
    pub limine_old_scan_truncated: bool,
    pub limine_active_scan_truncated: bool,
    pub limine_old_root_active: bool,
    pub limine_release_safe: bool,
}
impl Stats {
    const EMPTY: Self = Self {
        boot_sanity_runs: 0,
        boot_sanity_failures: 0,
        last_sanity_root_phys: 0,
        last_sanity_table_frames: 0,
        last_sanity_frame: 0,
        kernel_space_runs: 0,
        kernel_space_failures: 0,
        kernel_space_root_phys: 0,
        kernel_space_table_frames: 0,
        kernel_space_source_root_phys: 0,
        kernel_space_hhdm_offset: 0,
        hhdm_pages_4k: 0,
        hhdm_pages_2m: 0,
        kernel_image_pages: 0,
        kernel_image_skipped: 0,
        boot_probe_pages: 0,
        boot_probe_skipped: 0,
        probe_bits: 0,
        debug_probe_bit: 0,
        debug_probe_virt: 0,
        debug_probe_step: 0,
        debug_probe_source_phys: 0,
        debug_probe_existing_phys: 0,
        uses_limine_subtables: false,
        device_space_runs: 0,
        device_space_failures: 0,
        device_mapping_count: 0,
        device_mapping_truncated: false,
        device_probe_bits: 0,
        framebuffer_bytes: 0,
        acpi_bytes: 0,
        mmio_bytes: 0,
        pcie_bar_count: 0,
        pcie_bar_bytes: 0,
        cr3_switch_runs: 0,
        cr3_switch_failures: 0,
        old_cr3: 0,
        new_cr3: 0,
        active_cr3: 0,
        post_switch_bits: 0,
        cr3_switch_done: false,
        heap_committed_bytes: 0,
        heap_committed_pages: 0,
        heap_committed_skipped: 0,
        limine_pt_scan_runs: 0,
        limine_pt_scan_failures: 0,
        limine_old_table_frames: 0,
        limine_active_table_frames: 0,
        limine_referenced_frames: 0,
        limine_release_candidates: 0,
        limine_quarantined_frames: 0,
        limine_released_frames: 0,
        limine_retained_frames: 0,
        limine_quarantine_failures: 0,
        limine_old_scan_truncated: false,
        limine_active_scan_truncated: false,
        limine_old_root_active: false,
        limine_release_safe: false,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Mapping {
    pub name_ptr: *const u8,
    pub name_len: usize,
    pub phys_base: u64,
    pub virt_base: u64,
    pub len: u64,
    pub flags: u64,
    pub cache_policy: u8,
    pub mapped: bool,
}
impl Mapping {
    const EMPTY: Self = Self {
        name_ptr: b"".as_ptr(),
        name_len: 0,
        phys_base: 0,
        virt_base: 0,
        len: 0,
        flags: 0,
        cache_policy: 4,
        mapped: false,
    };
}
#[repr(C)]
#[derive(Default)]
pub struct Inputs {
    pub legacy_info: u64,
    pub map_address: u64,
    pub map_bytes: u64,
    pub framebuffer_record: u64,
    pub framebuffer_address: u64,
    pub framebuffer_bytes: u64,
    pub heap_metadata_base: u64,
    pub heap_metadata_bytes: u64,
    pub heap_base: u64,
    pub heap_committed: u64,
}
#[repr(C)]
pub struct AcpiInput {
    pub rsdp: u64,
    pub xsdt: u64,
    pub rsdt: u64,
    pub mcfg: u64,
    pub madt: u64,
    pub fadt: u64,
    pub hpet: u64,
    pub dsdt: u64,
    pub x_dsdt: u64,
    pub lapic: u64,
    pub first_ioapic: u64,
    pub hpet_base: u64,
    pub mcfg_base: u64,
    pub reset_address: u64,
    pub ioapics: [u64; 4],
    pub ioapic_count: u32,
    pub start_bus: u8,
    pub end_bus: u8,
    pub reset_address_space: u8,
    pub found: bool,
    pub reset_supported: bool,
    pub reset_valid: bool,
}
unsafe extern "C" {
    fn r4os_read_cr3() -> u64;
    fn r4os_write_cr3(value: u64);
    fn r4os_zig_root_inputs(output: *mut Inputs);
    fn r4os_zig_root_ecam(
        base: u64,
        start: u8,
        end: u8,
        output_base: *mut u64,
        output_bytes: *mut u64,
    ) -> bool;
}
fn inputs() -> Inputs {
    let mut output = Inputs::default();
    unsafe {
        r4os_zig_root_inputs(&mut output);
    }
    output
}
fn boot() -> &'static info::Snapshot {
    unsafe { &*info::r4os_rust_boot_info() }
}
fn entries() -> &'static [info::MemoryEntry] {
    let boot = boot();
    if boot.memory_count == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(boot.memory, boot.memory_count) }
    }
}
fn align_down(value: u64) -> u64 {
    value & !(PAGE - 1)
}
fn align_up(value: u64) -> Option<u64> {
    Some(value.checked_add(PAGE - 1)? & !(PAGE - 1))
}
fn aligned(value: u64) -> bool {
    value & (PAGE - 1) == 0
}
fn alloc(kind: u8, name: &'static [u8]) -> Option<u64> {
    let mut frame = 0;
    if !unsafe { phys::r4os_rust_phys_alloc(&mut frame) } {
        return None;
    }
    if unsafe { blocks::claim(blocks::Block::physical(frame, PAGE, kind, 0, 0, 2, name)) }.is_err()
    {
        phys::r4os_rust_phys_free(frame);
        return None;
    }
    Some(frame)
}
fn release(frame: u64) {
    if frame == 0 {
        return;
    }
    let mut plan = MaybeUninit::<blocks::ReleasePlan>::uninit();
    if unsafe { blocks::r4os_rust_blocks_prepare(frame, PAGE, plan.as_mut_ptr()) } != 0 {
        return;
    }
    phys::r4os_rust_phys_free(frame);
    unsafe {
        blocks::r4os_rust_blocks_finish(plan.as_mut_ptr(), false, false);
    }
}
static BSS_PROBE: AtomicU64 = AtomicU64::new(0);
static SWITCH_GUARD: AtomicU64 = AtomicU64::new(0);
struct State {
    stats: Stats,
    builder: Builder,
    ready: bool,
    mappings: [Mapping; 64],
    mapping_count: usize,
    mmio_offset: u64,
    framebuffer_offset: u64,
    limine_frames: [u64; 1024],
    active_frames: [u64; 1024],
}
impl State {
    const EMPTY: Self = Self {
        stats: Stats::EMPTY,
        builder: Builder {
            pml4_phys: 0,
            table_frames: [0; 512],
            table_frame_count: 0,
            owned: false,
        },
        ready: false,
        mappings: [Mapping::EMPTY; 64],
        mapping_count: 0,
        mmio_offset: 0,
        framebuffer_offset: 0,
        limine_frames: [0; 1024],
        active_frames: [0; 1024],
    };
    fn fail_root(&mut self) -> bool {
        self.stats.kernel_space_failures += 1;
        if !self.ready {
            self.builder.deinit();
        }
        false
    }
    fn record(
        &mut self,
        name: &'static [u8],
        physical: u64,
        virtual_base: u64,
        len: u64,
        flags: u64,
        policy: u8,
    ) {
        if self.mapping_count == self.mappings.len() {
            self.stats.device_mapping_truncated = true;
            return;
        }
        self.mappings[self.mapping_count] = Mapping {
            name_ptr: name.as_ptr(),
            name_len: name.len(),
            phys_base: physical,
            virt_base: virtual_base,
            len,
            flags,
            cache_policy: policy,
            mapped: true,
        };
        self.mapping_count += 1;
    }
    fn sanity(&mut self) -> bool {
        self.stats.boot_sanity_runs += 1;
        let mut builder = Builder {
            pml4_phys: 0,
            table_frames: [0; 512],
            table_frame_count: 0,
            owned: false,
        };
        let result = (|| {
            if !builder.init() {
                return false;
            }
            let Some(frame) = alloc(1, b"pt-sanity-frame") else {
                return false;
            };
            let result = (|| {
                let address = layout::KERNEL_DYNAMIC_BASE + 0x20_0000;
                if !builder.map_page(address, frame, RW, false) {
                    return false;
                }
                let mapped = walk(builder.pml4_phys, address);
                if !mapped.mapped
                    || mapped.phys != frame
                    || mapped.level != 1
                    || mapped.huge
                    || mapped.flags & 2 == 0
                    || mapped.flags & (1 << 63) == 0
                    || walk(builder.pml4_phys, address + PAGE).mapped
                {
                    return false;
                }
                self.stats.last_sanity_root_phys = builder.pml4_phys;
                self.stats.last_sanity_table_frames = builder.table_frame_count as u64;
                self.stats.last_sanity_frame = frame;
                true
            })();
            release(frame);
            result
        })();
        builder.deinit();
        if !result {
            self.stats.boot_sanity_failures += 1;
        }
        result
    }
    fn build(&mut self) -> bool {
        if self.ready {
            return true;
        }
        self.stats.kernel_space_runs += 1;
        self.stats.probe_bits = 0;
        self.stats.hhdm_pages_4k = 0;
        self.stats.hhdm_pages_2m = 0;
        self.stats.kernel_image_pages = 0;
        self.stats.kernel_image_skipped = 0;
        self.stats.boot_probe_pages = 0;
        self.stats.boot_probe_skipped = 0;
        self.stats.uses_limine_subtables = false;
        let root = paging::r4os_rust_paging_root();
        if root == 0 {
            return self.fail_root();
        }
        self.stats.kernel_space_source_root_phys = root;
        if !self.builder.init()
            || !self.map_hhdm()
            || !self.map_image(root)
            || !self.boot_probes(root)
            || self.stats.probe_bits & PROBES != PROBES
        {
            return self.fail_root();
        }
        self.stats.kernel_space_root_phys = self.builder.pml4_phys;
        self.stats.kernel_space_table_frames = self.builder.table_frame_count as u64;
        self.ready = true;
        true
    }
    fn map_hhdm(&mut self) -> bool {
        let boot = boot();
        if boot.flags & 4 == 0 {
            return false;
        }
        self.stats.kernel_space_hhdm_offset = boot.hhdm;
        for entry in entries() {
            if !entry.valid || entry.length == 0 {
                continue;
            }
            let base = align_down(entry.base);
            let Some(end) = align_up(entry.end) else {
                return false;
            };
            if end <= base {
                continue;
            }
            let Some(address) = boot.hhdm.checked_add(base) else {
                return false;
            };
            let mut mapped = RangeStats::default();
            if !self
                .builder
                .map_range(address, base, end - base, DIRECT, true, &mut mapped)
            {
                return false;
            }
            self.stats.hhdm_pages_4k += mapped.pages_4k;
            self.stats.hhdm_pages_2m += mapped.pages_2m;
        }
        true
    }
    fn map_image(&mut self, root: u64) -> bool {
        let mut min = u64::MAX;
        let mut max = 0;
        for entry in entries() {
            if !entry.valid || entry.kind != 6 {
                continue;
            }
            min = min.min(entry.base);
            max = max.max(entry.end);
        }
        let span = if max <= min {
            16 * layout::MIB
        } else {
            align_up((max - min).min(layout::KERNEL_IMAGE_WINDOW_BYTES)).unwrap_or(16 * layout::MIB)
        };
        let mut mapped = RangeStats::default();
        if !sparse(
            &mut self.builder,
            root,
            layout::KERNEL_IMAGE_BASE,
            span,
            &mut mapped,
        ) {
            return false;
        }
        self.stats.kernel_image_pages += mapped.pages_4k;
        self.stats.kernel_image_skipped += mapped.skipped;
        mapped.pages_4k != 0
    }
    fn probe(&mut self, root: u64, address: u64, bit: u32, mapped: &mut RangeStats) -> bool {
        let page = align_down(address);
        self.stats.debug_probe_bit = bit;
        self.stats.debug_probe_virt = page;
        self.stats.debug_probe_step = 1;
        self.stats.debug_probe_source_phys = 0;
        self.stats.debug_probe_existing_phys = 0;
        let source = walk(root, page);
        if !source.mapped {
            return false;
        }
        self.stats.debug_probe_source_phys = align_down(source.phys);
        let previous = walk(self.builder.pml4_phys, page);
        if previous.mapped {
            self.stats.debug_probe_existing_phys = align_down(previous.phys);
        }
        if !self
            .builder
            .ensure_page(page, self.stats.debug_probe_source_phys, source.flags)
        {
            self.stats.debug_probe_step = 2;
            return false;
        }
        mapped.pages_4k += 1;
        if !walk(self.builder.pml4_phys, page).mapped {
            self.stats.debug_probe_step = 3;
            return false;
        }
        self.stats.probe_bits |= bit;
        self.stats.debug_probe_step = 0;
        true
    }
    fn boot_probes(&mut self, root: u64) -> bool {
        let mut stack = 0u64;
        let stack_address = core::ptr::addr_of_mut!(stack) as u64;
        let mut mapped = RangeStats::default();
        let input = inputs();
        if !self.probe(root, layout::KERNEL_IMAGE_BASE, 1, &mut mapped)
            || !self.probe(
                root,
                b"r4os-pml4-rodata-probe".as_ptr() as u64,
                2,
                &mut mapped,
            )
            || !self.probe(root, core::ptr::addr_of!(self.stats) as u64, 4, &mut mapped)
            || !self.probe(root, core::ptr::addr_of!(BSS_PROBE) as u64, 8, &mut mapped)
            || !self.probe(root, stack_address, 16, &mut mapped)
            || !stack_window(&mut self.builder, root, stack_address, &mut mapped)
            || !self.probe(root, input.legacy_info, 128, &mut mapped)
        {
            return false;
        }
        if input.map_bytes != 0
            && !sparse(
                &mut self.builder,
                root,
                input.map_address,
                input.map_bytes,
                &mut mapped,
            )
        {
            return false;
        }
        if input.framebuffer_record != 0
            && !self.probe(root, input.framebuffer_record, 128, &mut mapped)
        {
            return false;
        }
        if input.heap_metadata_bytes == 0
            || !sparse(
                &mut self.builder,
                root,
                input.heap_metadata_base,
                input.heap_metadata_bytes,
                &mut mapped,
            )
            || !walk(self.builder.pml4_phys, input.heap_metadata_base).mapped
        {
            return false;
        }
        self.stats.probe_bits |= 256;
        if self.check_hhdm(self.builder.pml4_phys) {
            self.stats.probe_bits |= 32;
        }
        if check_pmm(self.builder.pml4_phys) {
            self.stats.probe_bits |= 64;
        }
        if self.stats.probe_bits & (32 | 64) != 32 | 64 {
            return false;
        }
        self.stats.boot_probe_pages += mapped.pages_4k;
        self.stats.boot_probe_skipped += mapped.skipped;
        true
    }
    fn check_hhdm(&self, root: u64) -> bool {
        let boot = boot();
        for entry in entries() {
            if !entry.valid || entry.length == 0 || boot.flags & 4 == 0 {
                continue;
            }
            if let Some(address) = entry.base.checked_add(boot.hhdm)
                && check_physical(root, address, entry.base)
            {
                return true;
            }
        }
        false
    }
}
fn sparse(
    builder: &mut Builder,
    root: u64,
    address: u64,
    bytes: u64,
    mapped: &mut RangeStats,
) -> bool {
    if bytes == 0 {
        return false;
    }
    let Some(end) = address.checked_add(bytes).and_then(align_up) else {
        return false;
    };
    let mut current = align_down(address);
    while current < end {
        let source = walk(root, current);
        if !source.mapped || !builder.ensure_page(current, align_down(source.phys), source.flags) {
            mapped.skipped += 1;
        } else {
            mapped.pages_4k += 1;
        }
        current += PAGE;
    }
    true
}
fn stack_window(builder: &mut Builder, root: u64, stack: u64, mapped: &mut RangeStats) -> bool {
    let page = align_down(stack);
    let half = layout::MIB / 2;
    let base = if page > half { page - half } else { page };
    let before = mapped.pages_4k;
    sparse(builder, root, base, layout::MIB, mapped) && mapped.pages_4k > before
}
fn check_physical(root: u64, address: u64, physical: u64) -> bool {
    let mapped = walk(root, align_down(address));
    mapped.mapped && align_down(mapped.phys) == align_down(physical)
}
fn check_virtual(root: u64, address: u64) -> bool {
    walk(root, align_down(address)).mapped
}
fn check_pmm(root: u64) -> bool {
    let mut pmm = phys::Stats::default();
    unsafe {
        phys::r4os_rust_phys_stats(&mut pmm);
    }
    pmm.bitmap_base != 0
        && pmm.bitmap_bytes != 0
        && check_physical(
            root,
            phys::r4os_rust_phys_to_virt(pmm.bitmap_base),
            pmm.bitmap_base,
        )
}

impl State {
    fn devices(&mut self, acpi: &AcpiInput) -> bool {
        self.stats.device_space_runs += 1;
        self.mapping_count = 0;
        self.mmio_offset = 0;
        self.framebuffer_offset = 0;
        self.mappings.fill(Mapping::EMPTY);
        self.stats.device_mapping_count = 0;
        self.stats.device_mapping_truncated = false;
        self.stats.device_probe_bits = 0;
        self.stats.framebuffer_bytes = 0;
        self.stats.acpi_bytes = 0;
        self.stats.mmio_bytes = 0;
        self.stats.pcie_bar_count = 0;
        self.stats.pcie_bar_bytes = 0;
        let result = (|| {
            if !self.ready {
                return false;
            }
            let root = paging::r4os_rust_paging_root();
            if root == 0 {
                return false;
            }
            let mut required = DEV_FB;
            if acpi.found {
                required |= DEV_ACPI;
            }
            if acpi.lapic != 0 {
                required |= DEV_LAPIC;
            }
            if acpi.first_ioapic != 0 {
                required |= DEV_IOAPIC;
            }
            if acpi.hpet_base != 0 {
                required |= DEV_HPET;
            }
            if acpi.mcfg_base != 0 && acpi.start_bus <= acpi.end_bus {
                required |= DEV_ECAM;
            }
            if acpi.reset_supported && acpi.reset_valid && acpi.reset_address_space == 0 {
                required |= DEV_RESET;
            }
            if !self.framebuffer(root)
                || !self.acpi(acpi)
                || !self.platform(acpi)
                || !self.ecam(acpi)
            {
                return false;
            }
            self.stats.device_mapping_count = self.mapping_count as u64;
            self.stats.device_probe_bits & required == required
        })();
        if !result {
            self.stats.device_space_failures += 1;
            self.stats.device_mapping_count = self.mapping_count as u64;
        }
        result
    }
    fn framebuffer(&mut self, root: u64) -> bool {
        let input = inputs();
        if input.framebuffer_record == 0 || input.framebuffer_bytes == 0 {
            return false;
        }
        let start = align_down(input.framebuffer_address);
        let Some(end) = input
            .framebuffer_address
            .checked_add(input.framebuffer_bytes)
            .and_then(align_up)
        else {
            return false;
        };
        let mut offset = 0;
        let mut current = start;
        while current < end {
            let source = walk(root, current);
            if !source.mapped {
                return false;
            }
            let Some(address) =
                layout::FRAMEBUFFER_BASE.checked_add(self.framebuffer_offset + offset)
            else {
                return false;
            };
            if !self
                .builder
                .map_page(address, align_down(source.phys), FB, true)
            {
                return false;
            }
            current += PAGE;
            offset += PAGE;
        }
        let len = end - start;
        self.record(
            b"framebuffer-window",
            align_down(walk(root, start).phys),
            layout::FRAMEBUFFER_BASE + self.framebuffer_offset,
            len,
            FB,
            1,
        );
        self.framebuffer_offset += len;
        self.stats.framebuffer_bytes += len;
        self.stats.device_probe_bits |= DEV_FB;
        true
    }
    fn acpi(&mut self, acpi: &AcpiInput) -> bool {
        if !acpi.found {
            return true;
        }
        let mut ok = self.verify_acpi(b"acpi-rsdp", acpi.rsdp, 4096);
        for (name, address) in [
            (b"acpi-xsdt".as_slice(), acpi.xsdt),
            (b"acpi-rsdt".as_slice(), acpi.rsdt),
            (b"acpi-mcfg".as_slice(), acpi.mcfg),
            (b"acpi-madt".as_slice(), acpi.madt),
            (b"acpi-fadt".as_slice(), acpi.fadt),
            (b"acpi-hpet".as_slice(), acpi.hpet),
            (
                b"acpi-dsdt".as_slice(),
                if acpi.x_dsdt != 0 {
                    acpi.x_dsdt
                } else {
                    acpi.dsdt
                },
            ),
        ] {
            if address != 0 {
                ok = self.verify_acpi(name, address, acpi_length(address)) && ok;
            }
        }
        if ok {
            self.stats.device_probe_bits |= DEV_ACPI;
        }
        ok
    }
    fn platform(&mut self, acpi: &AcpiInput) -> bool {
        let mut ok = true;
        if acpi.lapic != 0 {
            ok = self.mmio(b"lapic-mmio", acpi.lapic, PAGE, DEV_LAPIC) && ok;
        }
        let count = (acpi.ioapic_count as usize).min(4);
        for index in 0..count {
            let address = acpi.ioapics[index];
            if address == 0 {
                continue;
            }
            ok = self.mmio(
                if index == 0 {
                    b"ioapic-mmio"
                } else {
                    b"ioapic-extra-mmio"
                },
                address,
                PAGE,
                DEV_IOAPIC,
            ) && ok;
        }
        if count == 0 && acpi.first_ioapic != 0 {
            ok = self.mmio(b"ioapic-mmio", acpi.first_ioapic, PAGE, DEV_IOAPIC) && ok;
        }
        if acpi.hpet_base != 0 {
            ok = self.mmio(b"hpet-mmio", acpi.hpet_base, PAGE, DEV_HPET) && ok;
        }
        if acpi.reset_supported
            && acpi.reset_valid
            && acpi.reset_address_space == 0
            && acpi.reset_address != 0
        {
            ok = self.mmio(b"acpi-reset-mmio", acpi.reset_address, 1, DEV_RESET) && ok;
        }
        ok
    }
    fn ecam(&mut self, acpi: &AcpiInput) -> bool {
        if acpi.mcfg_base == 0 || acpi.start_bus > acpi.end_bus {
            return true;
        }
        let mut base = 0;
        let mut bytes = 0;
        if !unsafe {
            r4os_zig_root_ecam(
                acpi.mcfg_base,
                acpi.start_bus,
                acpi.end_bus,
                &mut base,
                &mut bytes,
            )
        } {
            return false;
        }
        self.mmio(b"pcie-ecam", base, bytes, DEV_ECAM)
    }
    fn mmio(&mut self, name: &'static [u8], physical: u64, bytes: u64, bit: u32) -> bool {
        if physical == 0 || bytes == 0 {
            return true;
        }
        let base = align_down(physical);
        let Some(end) = physical.checked_add(bytes).and_then(align_up) else {
            return false;
        };
        if end <= base {
            return false;
        }
        let len = end - base;
        let Some(cursor) = layout::MMIO_BASE.checked_add(self.mmio_offset) else {
            return false;
        };
        let desired = base & (HUGE_BYTES - 1);
        let present = cursor & (HUGE_BYTES - 1);
        let padding = (desired + HUGE_BYTES - present) & (HUGE_BYTES - 1);
        let Some(address) = cursor.checked_add(padding) else {
            return false;
        };
        let Some(address_end) = address.checked_add(len) else {
            return false;
        };
        if address < layout::MMIO_BASE || address_end > layout::MMIO_END {
            return false;
        }
        let mut mapped = RangeStats::default();
        if !self
            .builder
            .map_range(address, base, len, MMIO, true, &mut mapped)
            || !self.direct_mmio(base, len)
        {
            return false;
        }
        let Some(consumed) = padding.checked_add(len) else {
            return false;
        };
        let Some(next) = self.mmio_offset.checked_add(consumed) else {
            return false;
        };
        self.mmio_offset = next;
        self.stats.mmio_bytes += len;
        self.stats.device_probe_bits |= bit;
        self.record(name, base, address, len, MMIO, 2);
        true
    }
    fn direct_mmio(&mut self, physical: u64, bytes: u64) -> bool {
        if physical == 0 || bytes == 0 {
            return true;
        }
        let base = align_down(physical);
        let Some(end) = physical.checked_add(bytes).and_then(align_up) else {
            return false;
        };
        let mut offset = 0;
        while base + offset < end {
            let frame = base + offset;
            let address = phys::r4os_rust_phys_to_virt(frame);
            let previous = walk(self.builder.pml4_phys, address);
            if previous.mapped {
                if align_down(previous.phys) != frame
                    || !self.builder.map_page(address, frame, MMIO, true)
                {
                    return false;
                }
                offset += PAGE;
                continue;
            }
            if end - frame >= HUGE_BYTES
                && address & (HUGE_BYTES - 1) == 0
                && frame & (HUGE_BYTES - 1) == 0
            {
                if !self.builder.map_huge(address, frame, MMIO) {
                    return false;
                }
                offset += HUGE_BYTES;
            } else {
                if !self.builder.map_page(address, frame, MMIO, false) {
                    return false;
                }
                offset += PAGE;
            }
        }
        true
    }
    fn verify_acpi(&mut self, name: &'static [u8], physical: u64, bytes: u64) -> bool {
        if physical == 0 || bytes == 0 {
            return true;
        }
        let base = align_down(physical);
        let Some(end) = physical.checked_add(bytes).and_then(align_up) else {
            return false;
        };
        if end <= base {
            return false;
        }
        let mut page = base;
        while page < end {
            if !check_physical(
                self.builder.pml4_phys,
                phys::r4os_rust_phys_to_virt(page),
                page,
            ) {
                return false;
            }
            page += PAGE;
        }
        self.stats.acpi_bytes += end - base;
        self.record(
            name,
            base,
            phys::r4os_rust_phys_to_virt(base),
            end - base,
            RO,
            3,
        );
        true
    }
    fn runtime_maps(&mut self, root: u64) -> bool {
        if root == 0 {
            return false;
        }
        let input = inputs();
        if input.heap_committed == 0 {
            return false;
        }
        let mut mapped = RangeStats::default();
        if !sparse(
            &mut self.builder,
            root,
            input.heap_base,
            input.heap_committed,
            &mut mapped,
        ) {
            return false;
        }
        self.stats.heap_committed_bytes = input.heap_committed;
        self.stats.heap_committed_pages = mapped.pages_4k;
        self.stats.heap_committed_skipped = mapped.skipped;
        let mut stack = 0u64;
        let mut stack_mapped = RangeStats::default();
        if !stack_window(
            &mut self.builder,
            root,
            core::ptr::addr_of_mut!(stack) as u64,
            &mut stack_mapped,
        ) {
            return false;
        }
        self.stats.boot_probe_pages += stack_mapped.pages_4k;
        self.stats.boot_probe_skipped += stack_mapped.skipped;
        mapped.pages_4k != 0
    }
    fn switch(&mut self) -> bool {
        self.stats.cr3_switch_runs += 1;
        self.stats.post_switch_bits = 0;
        self.stats.cr3_switch_done = false;
        let result = (|| {
            if !self.ready
                || self.stats.device_space_runs == 0
                || self.stats.device_space_failures != 0
            {
                return false;
            }
            let root = self.builder.pml4_phys;
            if root == 0
                || !aligned(root)
                || phys::r4os_rust_phys_to_virt(root) == 0
                || !page_table_block(root)
                || !self.runtime_maps(paging::r4os_rust_paging_root())
                || !self.check_switch(root, false)
            {
                return false;
            }
            let old_raw = unsafe { r4os_read_cr3() };
            let old = old_raw & MASK;
            self.stats.old_cr3 = old;
            self.stats.new_cr3 = root;
            unsafe {
                r4os_write_cr3(root);
            }
            let active = unsafe { r4os_read_cr3() } & MASK;
            self.stats.active_cr3 = active;
            if active != root
                || !paging::r4os_rust_paging_adopt(root, 2)
                || !self.check_switch(root, true)
            {
                unsafe {
                    r4os_write_cr3(old_raw);
                }
                paging::r4os_rust_paging_adopt(old, 1);
                self.stats.active_cr3 = unsafe { r4os_read_cr3() } & MASK;
                return false;
            }
            self.stats.cr3_switch_done = true;
            self.inspect_limine(old, root);
            true
        })();
        if !result {
            self.stats.cr3_switch_failures += 1;
        }
        result
    }
    fn check_switch(&mut self, root: u64, after: bool) -> bool {
        let mut bits = 0;
        let mut stack = 0u64;
        let input = inputs();
        if check_virtual(root, layout::KERNEL_IMAGE_BASE) {
            bits |= 1;
        }
        if check_virtual(root, core::ptr::addr_of_mut!(stack) as u64) {
            bits |= 2;
        }
        if self.check_hhdm(root) {
            bits |= 4;
        }
        if check_pmm(root) {
            bits |= 8;
        }
        if check_physical(root, phys::r4os_rust_phys_to_virt(root), root) {
            bits |= 16;
        }
        if check_virtual(root, input.legacy_info)
            && (input.map_bytes == 0 || check_virtual(root, input.map_address))
            && (input.framebuffer_record == 0 || check_virtual(root, input.framebuffer_record))
        {
            bits |= 32;
        }
        if input.framebuffer_record != 0
            && walk(root, input.framebuffer_address).mapped
            && walk(root, layout::FRAMEBUFFER_BASE).mapped
        {
            bits |= 64;
        }
        if input.heap_committed != 0
            && check_virtual(root, input.heap_base)
            && input
                .heap_base
                .checked_add(input.heap_committed - 1)
                .is_some_and(|last| check_virtual(root, last))
        {
            bits |= 128;
        }
        if after {
            SWITCH_GUARD.fetch_add(1, Ordering::Relaxed);
            if !check_virtual(root, core::ptr::addr_of!(SWITCH_GUARD) as u64) {
                bits &= !1;
            }
        }
        self.stats.post_switch_bits = bits;
        bits & POST == POST
    }
    fn inspect_limine(&mut self, old: u64, active: u64) {
        self.stats.limine_pt_scan_runs += 1;
        self.limine_frames.fill(0);
        self.active_frames.fill(0);
        self.stats.limine_old_table_frames = 0;
        self.stats.limine_active_table_frames = 0;
        self.stats.limine_referenced_frames = 0;
        self.stats.limine_release_candidates = 0;
        self.stats.limine_quarantined_frames = 0;
        self.stats.limine_released_frames = 0;
        self.stats.limine_retained_frames = 0;
        self.stats.limine_quarantine_failures = 0;
        self.stats.limine_old_scan_truncated = false;
        self.stats.limine_active_scan_truncated = false;
        self.stats.limine_old_root_active = false;
        self.stats.limine_release_safe = false;
        if old == 0 || active == 0 || !aligned(old) || !aligned(active) {
            self.stats.limine_pt_scan_failures += 1;
            return;
        }
        let mut old_count = 0;
        let mut active_count = 0;
        collect(
            old,
            4,
            &mut self.limine_frames,
            &mut old_count,
            &mut self.stats.limine_old_scan_truncated,
            &mut self.stats.limine_pt_scan_failures,
        );
        collect(
            active,
            4,
            &mut self.active_frames,
            &mut active_count,
            &mut self.stats.limine_active_scan_truncated,
            &mut self.stats.limine_pt_scan_failures,
        );
        self.stats.limine_old_table_frames = old_count as u64;
        self.stats.limine_active_table_frames = active_count as u64;
        self.stats.limine_old_root_active = old == active;
        for frame in &self.limine_frames[..old_count] {
            if self.active_frames[..active_count].contains(frame) {
                self.stats.limine_referenced_frames += 1;
            }
        }
        self.stats.limine_release_safe = old != active
            && !self.stats.limine_old_scan_truncated
            && !self.stats.limine_active_scan_truncated
            && self.stats.limine_referenced_frames == 0
            && old_count != 0;
        if !self.stats.limine_release_safe {
            self.stats.limine_retained_frames = old_count as u64;
            return;
        }
        self.stats.limine_release_candidates = old_count as u64;
        for frame in &self.limine_frames[..old_count] {
            let request = blocks::Block::physical(*frame, PAGE, 3, 6, 0, 1, b"limine-page-table");
            let mut id = 0;
            if *frame != 0
                && aligned(*frame)
                && unsafe { blocks::r4os_rust_blocks_range(&request, true, &mut id) } == 0
            {
                self.stats.limine_quarantined_frames += 1;
            } else {
                self.stats.limine_quarantine_failures += 1;
            }
        }
        if self.stats.limine_quarantine_failures != 0 {
            self.stats.limine_retained_frames = self.stats.limine_quarantine_failures;
        }
    }
}
fn acpi_length(physical: u64) -> u64 {
    if physical == 0 || phys::r4os_rust_phys_to_virt(align_down(physical)) == 0 {
        return PAGE;
    }
    // Original SDT length is the unaligned 32-bit wire field at byte 4.
    let pointer = phys::r4os_rust_phys_to_virt(physical) as *const u8;
    let length = unsafe { core::ptr::read_unaligned(pointer.add(4).cast::<u32>()) } as u64;
    if !(36..=layout::MIB).contains(&length) {
        PAGE
    } else {
        length
    }
}
fn page_table_block(root: u64) -> bool {
    let mut summary = blocks::Summary::default();
    unsafe {
        blocks::r4os_rust_blocks_summary(&mut summary);
    }
    let Some(root_end) = root.checked_add(PAGE) else {
        return false;
    };
    for index in 0..summary.active_blocks {
        let mut record = MaybeUninit::<blocks::Block>::uninit();
        if !unsafe { blocks::r4os_rust_blocks_read(2, 0, index, record.as_mut_ptr()) } {
            return false;
        }
        let block = unsafe { record.assume_init() };
        if block.kind != 3 || block.phys_len == 0 {
            continue;
        }
        if block
            .phys_base
            .checked_add(block.phys_len)
            .is_some_and(|end| root >= block.phys_base && root_end <= end)
        {
            return true;
        }
    }
    false
}
fn collect(
    root: u64,
    level: u8,
    frames: &mut [u64; 1024],
    count: &mut usize,
    truncated: &mut bool,
    failures: &mut u64,
) {
    if root == 0 || !aligned(root) {
        *failures += 1;
        return;
    }
    if frames[..*count].contains(&root) {
        return;
    }
    if *count == frames.len() {
        *truncated = true;
        return;
    }
    frames[*count] = root;
    *count += 1;
    if level <= 1 {
        return;
    }
    let address = phys::r4os_rust_phys_to_virt(root);
    if address == 0 {
        *failures += 1;
        return;
    }
    let table = address as *const u64;
    for slot in 0..512 {
        let entry = unsafe { table.add(slot).read_volatile() };
        if entry & 1 == 0 || entry & HUGE != 0 {
            continue;
        }
        let next = entry & MASK;
        if next == 0 {
            continue;
        }
        collect(next, level - 1, frames, count, truncated, failures);
    }
}
struct Storage(UnsafeCell<State>);
// SAFETY: mutation is confined to singleton BSP boot before AP/task
// publication. Later readers see this immutable published catalogue.
unsafe impl Sync for Storage {}
static ROOT: Storage = Storage(UnsafeCell::new(State::EMPTY));
/// # Safety
/// Called only by the singleton BSP memory boot owner before AP publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_root_control(operation: u8) -> bool {
    let state = unsafe { &mut *ROOT.0.get() };
    match operation {
        0 => state.sanity(),
        1 => state.build(),
        2 => state.switch(),
        _ => false,
    }
}
/// # Safety
/// input is a live normalized ACPI C record; singleton BSP admission applies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_root_devices(input: *const AcpiInput) -> bool {
    unsafe { (&mut *ROOT.0.get()).devices(&*input) }
}
/// # Safety
/// output is exclusive C storage; caller observes completed boot publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_root_stats(output: *mut Stats) {
    unsafe {
        output.write((*ROOT.0.get()).stats);
    }
}
/// # Safety
/// output is exclusive C storage; caller observes completed boot publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_root_mapping(index: usize, output: *mut Mapping) -> bool {
    let state = unsafe { &*ROOT.0.get() };
    if index >= state.mapping_count {
        return false;
    }
    unsafe {
        output.write(state.mappings[index]);
    }
    true
}
/// # Safety
/// output is exclusive C storage; caller retains the published device catalogue.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_root_alias(physical: u64, bytes: u64, output: *mut u64) -> bool {
    if physical == 0 || bytes == 0 {
        return false;
    }
    let Some(end) = physical.checked_add(bytes) else {
        return false;
    };
    let state = unsafe { &*ROOT.0.get() };
    for mapping in &state.mappings[..state.mapping_count] {
        if !mapping.mapped || mapping.cache_policy != 2 {
            continue;
        }
        if mapping
            .phys_base
            .checked_add(mapping.len)
            .is_some_and(|mapping_end| physical >= mapping.phys_base && end <= mapping_end)
        {
            let Some(address) = mapping.virt_base.checked_add(physical - mapping.phys_base) else {
                return false;
            };
            unsafe {
                output.write(address);
            }
            return true;
        }
    }
    false
}
const _: () = {
    assert!(core::mem::size_of::<Inputs>() == 80);
    assert!(core::mem::size_of::<AcpiInput>() == 160);
    assert!(core::mem::size_of::<Mapping>() == 56);
};
