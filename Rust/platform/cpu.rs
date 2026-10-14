//! BSP CPU discovery and the existing module-SIMD/OSXSAVE publication.
//! Discovery finishes before AP observers; no State reference crosses logging.
use crate::arch::msr;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Features {
    pub tsc: bool,
    pub msr: bool,
    pub apic: bool,
    pub mtrr: bool,
    pub pat: bool,
    pub fxsr: bool,
    pub sse: bool,
    pub sse2: bool,
    pub sse3: bool,
    pub ssse3: bool,
    pub sse41: bool,
    pub sse42: bool,
    pub x2apic: bool,
    pub tsc_deadline: bool,
    pub xsave: bool,
    pub osxsave: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fsgsbase: bool,
    pub erms: bool,
    pub invpcid: bool,
    pub clflushopt: bool,
    pub syscall: bool,
    pub nx: bool,
    pub page1gb: bool,
    pub rdtscp: bool,
    pub long_mode: bool,
    pub invariant_tsc: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub detected: bool,
    pub vendor: [u8; 12],
    pub vendor_len: u64,
    pub brand: [u8; 48],
    pub brand_len: u64,
    pub max_basic_leaf: u32,
    pub max_extended_leaf: u32,
    pub stepping: u8,
    pub base_model: u8,
    pub base_family: u8,
    pub display_model: u16,
    pub display_family: u16,
    pub processor_type: u8,
    pub apic_id: u8,
    pub logical_processors: u8,
    pub clflush_line_bytes: u16,
    pub physical_address_bits: u8,
    pub virtual_address_bits: u8,
    pub l2_cache_kb: u32,
    pub tsc_denominator: u32,
    pub tsc_numerator: u32,
    pub crystal_hz: u32,
    pub base_mhz: u16,
    pub max_mhz: u16,
    pub bus_mhz: u16,
    pub xcr0: u64,
    pub pat_msr: u64,
    pub mtrr_cap: u64,
    pub features: Features,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            detected: false,
            vendor: [0; 12],
            vendor_len: 0,
            brand: [0; 48],
            brand_len: 0,
            max_basic_leaf: 0,
            max_extended_leaf: 0,
            stepping: 0,
            base_model: 0,
            base_family: 0,
            display_model: 0,
            display_family: 0,
            processor_type: 0,
            apic_id: 0,
            logical_processors: 0,
            clflush_line_bytes: 0,
            physical_address_bits: 0,
            virtual_address_bits: 0,
            l2_cache_kb: 0,
            tsc_denominator: 0,
            tsc_numerator: 0,
            crystal_hz: 0,
            base_mhz: 0,
            max_mhz: 0,
            bus_mhz: 0,
            xcr0: 0,
            pat_msr: 0,
            mtrr_cap: 0,
            features: Features::default(),
        }
    }
}
struct Storage(UnsafeCell<core::mem::MaybeUninit<Status>>);
// SAFETY: the BSP initializes/redetects only before concurrent observers.
// DETECTED publishes the immutable inventory; later FPU notes are separate
// atomics and do not mutate its storage. No borrowed inventory enters C.
unsafe impl Sync for Storage {}
static INFO: Storage = Storage(UnsafeCell::new(core::mem::MaybeUninit::uninit()));
static DETECTED: AtomicBool = AtomicBool::new(false);
static OSXSAVE: AtomicBool = AtomicBool::new(false);
static XCR0: AtomicU64 = AtomicU64::new(0);
static MODULE_SIMD: AtomicBool = AtomicBool::new(false);
unsafe extern "C" {
    fn r4os_zig_cpu_log(status: *const Status, modules_simd: bool);
}
fn cpuid(leaf: u32, subleaf: u32) -> core::arch::x86_64::CpuidResult {
    core::arch::x86_64::__cpuid_count(leaf, subleaf)
}
fn xgetbv(index: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("xgetbv", in("ecx") index, out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags))
    };
    (u64::from(high) << 32) | u64::from(low)
}
fn bit(value: u32, index: u32) -> bool {
    value & (1 << index) != 0
}
fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn trimmed(bytes: &[u8]) -> u64 {
    let mut len = bytes.len();
    while len != 0 && (bytes[len - 1] == 0 || bytes[len - 1] == b' ') {
        len -= 1;
    }
    len as u64
}
pub fn detect() -> Status {
    let mut value = Status::default();
    let leaf = cpuid(0, 0);
    value.max_basic_leaf = leaf.eax;
    put32(&mut value.vendor, 0, leaf.ebx);
    put32(&mut value.vendor, 4, leaf.edx);
    put32(&mut value.vendor, 8, leaf.ecx);
    value.vendor_len = trimmed(&value.vendor);
    value.max_extended_leaf = cpuid(0x8000_0000, 0).eax;
    if value.max_basic_leaf >= 1 {
        let r = cpuid(1, 0);
        value.stepping = (r.eax & 15) as u8;
        value.base_model = ((r.eax >> 4) & 15) as u8;
        value.base_family = ((r.eax >> 8) & 15) as u8;
        value.processor_type = ((r.eax >> 12) & 3) as u8;
        let model = ((r.eax >> 16) & 15) as u16;
        let family = ((r.eax >> 20) & 255) as u16;
        value.display_family = u16::from(value.base_family);
        if value.base_family == 15 {
            value.display_family += family;
        }
        value.display_model = u16::from(value.base_model);
        if value.base_family == 6 || value.base_family == 15 {
            value.display_model += model << 4;
        }
        value.apic_id = (r.ebx >> 24) as u8;
        value.logical_processors = (r.ebx >> 16) as u8;
        value.clflush_line_bytes = u16::from((r.ebx >> 8) as u8) * 8;
        value.features.tsc = bit(r.edx, 4);
        value.features.msr = bit(r.edx, 5);
        value.features.apic = bit(r.edx, 9);
        value.features.mtrr = bit(r.edx, 12);
        value.features.pat = bit(r.edx, 16);
        value.features.fxsr = bit(r.edx, 24);
        value.features.sse = bit(r.edx, 25);
        value.features.sse2 = bit(r.edx, 26);
        value.features.sse3 = bit(r.ecx, 0);
        value.features.ssse3 = bit(r.ecx, 9);
        value.features.sse41 = bit(r.ecx, 19);
        value.features.sse42 = bit(r.ecx, 20);
        value.features.x2apic = bit(r.ecx, 21);
        value.features.tsc_deadline = bit(r.ecx, 24);
        value.features.xsave = bit(r.ecx, 26);
        value.features.osxsave = bit(r.ecx, 27);
        value.features.avx = bit(r.ecx, 28);
    }
    if value.max_basic_leaf >= 7 {
        let r = cpuid(7, 0);
        value.features.fsgsbase = bit(r.ebx, 0);
        value.features.avx2 = bit(r.ebx, 5);
        value.features.erms = bit(r.ebx, 9);
        value.features.invpcid = bit(r.ebx, 10);
        value.features.clflushopt = bit(r.ebx, 23);
    }
    if value.max_basic_leaf >= 0x15 {
        let r = cpuid(0x15, 0);
        value.tsc_denominator = r.eax;
        value.tsc_numerator = r.ebx;
        value.crystal_hz = r.ecx;
    }
    if value.max_basic_leaf >= 0x16 {
        let r = cpuid(0x16, 0);
        value.base_mhz = r.eax as u16;
        value.max_mhz = r.ebx as u16;
        value.bus_mhz = r.ecx as u16;
    }
    if value.max_extended_leaf >= 0x8000_0004 {
        for (index, leaf) in (0x8000_0002..=0x8000_0004).enumerate() {
            let r = cpuid(leaf, 0);
            let offset = index * 16;
            put32(&mut value.brand, offset, r.eax);
            put32(&mut value.brand, offset + 4, r.ebx);
            put32(&mut value.brand, offset + 8, r.ecx);
            put32(&mut value.brand, offset + 12, r.edx);
        }
        value.brand_len = trimmed(&value.brand);
    }
    if value.max_extended_leaf >= 0x8000_0001 {
        let r = cpuid(0x8000_0001, 0);
        value.features.syscall = bit(r.edx, 11);
        value.features.nx = bit(r.edx, 20);
        value.features.page1gb = bit(r.edx, 26);
        value.features.rdtscp = bit(r.edx, 27);
        value.features.long_mode = bit(r.edx, 29);
    }
    if value.max_extended_leaf >= 0x8000_0006 {
        value.l2_cache_kb = (cpuid(0x8000_0006, 0).ecx >> 16) & 0xffff;
    }
    if value.max_extended_leaf >= 0x8000_0007 {
        value.features.invariant_tsc = bit(cpuid(0x8000_0007, 0).edx, 8);
    }
    if value.max_extended_leaf >= 0x8000_0008 {
        let r = cpuid(0x8000_0008, 0);
        value.physical_address_bits = r.eax as u8;
        value.virtual_address_bits = (r.eax >> 8) as u8;
    }
    if value.features.osxsave {
        value.xcr0 = xgetbv(0);
    }
    if value.features.msr && value.features.pat {
        value.pat_msr = unsafe { msr::read(0x277) };
    }
    if value.features.msr && value.features.mtrr {
        value.mtrr_cap = unsafe { msr::read(0xfe) };
    }
    value.detected = true;
    unsafe { (*INFO.0.get()).write(value) };
    XCR0.store(value.xcr0, Ordering::Release);
    OSXSAVE.store(value.features.osxsave, Ordering::Release);
    DETECTED.store(true, Ordering::Release);
    unsafe { r4os_zig_cpu_log(&value, MODULE_SIMD.load(Ordering::Acquire)) };
    value
}
pub fn snapshot() -> Status {
    if !DETECTED.load(Ordering::Acquire) {
        return detect();
    }
    let mut value = unsafe { (*INFO.0.get()).assume_init_read() };
    value.features.osxsave = OSXSAVE.load(Ordering::Acquire);
    value.xcr0 = XCR0.load(Ordering::Acquire);
    value
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_cpu_control(operation: u8, value: u64) -> u64 {
    match operation {
        0 => u64::from({
            let s = snapshot();
            s.features.pat && s.features.msr
        }),
        1 => u64::from({
            let s = snapshot();
            s.features.mtrr && s.features.msr
        }),
        2 => u64::from({
            let s = snapshot();
            s.features.pat && s.features.mtrr && s.features.msr && ((s.pat_msr >> 40) & 255) == 1
        }),
        3 => u64::from({
            let s = snapshot();
            s.features.pat && s.features.msr && ((s.pat_msr >> 40) & 255) == 1
        }),
        4 => u64::from(MODULE_SIMD.load(Ordering::Acquire)),
        5 => {
            MODULE_SIMD.store(value != 0, Ordering::Release);
            0
        }
        6 => {
            if !DETECTED.load(Ordering::Acquire) {
                detect();
            }
            XCR0.store(value, Ordering::Release);
            OSXSAVE.store(true, Ordering::Release);
            0
        }
        7 => u64::from(cpuid(1, 0).ebx >> 24),
        _ => panic!("Invalid private CPU operation"),
    }
}
/// # Safety
/// BSP discovery before AP observers; caller owns an aligned fixed C output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_cpu_detect(output: *mut Status) {
    unsafe { output.write(detect()) };
}
/// # Safety
/// Same discovery admission if not initialized; caller owns a fixed C output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_cpu_status(output: *mut Status) {
    unsafe { output.write(snapshot()) };
}
const _: () = {
    assert!(core::mem::size_of::<Features>() == 28);
    assert!(core::mem::size_of::<Status>() == 184);
    assert!(core::mem::offset_of!(Status, features) == 152);
    assert!(core::mem::offset_of!(Status, vendor_len) == 16);
};
