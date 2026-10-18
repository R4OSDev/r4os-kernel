//! Singleton BSP FPU selection and canonical task/AP/IRQ initial image.
//! Configuration is immutable before AP release; no live state borrow crosses
//! an assembly or foreign call. Task buffers retain the existing exact owner.
use crate::platform::cpu;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
pub const STORAGE_BYTES: usize = 4096;
pub const STORAGE_ALIGN: usize = 64;
const CR0_MP: u64 = 1 << 1;
const CR0_EM: u64 = 1 << 2;
const CR0_TS: u64 = 1 << 3;
const CR0_NE: u64 = 1 << 5;
const CR4_OSFXSR: u64 = 1 << 9;
const CR4_OSXMMEXCPT: u64 = 1 << 10;
const CR4_OSXSAVE: u64 = 1 << 18;
const X87_SSE: u64 = 3;
const AVX: u64 = 4;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub initialized: bool,
    pub enabled: bool,
    pub backend: u32,
    pub state_bytes: u32,
    pub state_storage_bytes: u32,
    pub xcr0_mask: u64,
    pub avx_supported: bool,
    pub avx_enabled: bool,
    pub avx2_supported: bool,
    pub avx2_enabled: bool,
    pub simd_abi: u32,
    pub cr0: u64,
    pub cr4: u64,
    pub save_count: u64,
    pub restore_count: u64,
    pub task_init_count: u64,
    pub task_state_bytes: u64,
}
impl Status {
    const EMPTY: Self = Self {
        initialized: false,
        enabled: false,
        backend: 0,
        state_bytes: 0,
        state_storage_bytes: STORAGE_BYTES as u32,
        xcr0_mask: 0,
        avx_supported: false,
        avx_enabled: false,
        avx2_supported: false,
        avx2_enabled: false,
        simd_abi: 0,
        cr0: 0,
        cr4: 0,
        save_count: 0,
        restore_count: 0,
        task_init_count: 0,
        task_state_bytes: 0,
    };
}
struct Config(UnsafeCell<Status>);
// SAFETY: singleton BSP writes before INITIALIZED release and AP admission.
// Subsequent configuration reads are copies; task/IRQ counts are separate.
unsafe impl Sync for Config {}
#[repr(align(64))]
struct Image(UnsafeCell<[u8; STORAGE_BYTES]>);
// SAFETY: BSP fills once before publication, all later consumers only read.
unsafe impl Sync for Image {}
static CONFIG: Config = Config(UnsafeCell::new(Status::EMPTY));
static INITIAL: Image = Image(UnsafeCell::new([0; STORAGE_BYTES]));
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static SAVES: AtomicU64 = AtomicU64::new(0);
static RESTORES: AtomicU64 = AtomicU64::new(0);
static TASK_INITS: AtomicU64 = AtomicU64::new(0);
static TASK_BYTES: AtomicU64 = AtomicU64::new(0);
unsafe extern "C" {
    fn r4os_read_cr0() -> u64;
    fn r4os_write_cr0(value: u64);
    fn r4os_read_cr4() -> u64;
    fn r4os_write_cr4(value: u64);
    fn r4os_xsetbv(index: u32, value: u64);
    fn r4os_fninit();
    fn r4os_init_simd(avx_enabled: u32, inherited: *mut u64);

    fn r4os_fxsave(dest: *mut u8);

    fn r4os_fxrstor(source: *const u8);

    fn r4os_xsave(dest: *mut u8, mask: u64);

    fn r4os_xrstor(source: *const u8, mask: u64);

}
fn config() -> Status {
    unsafe { CONFIG.0.get().read() }
}
fn publish(mut value: Status) {
    value.initialized = true;
    value.enabled = value.backend != 0;
    unsafe { CONFIG.0.get().write(value) };
    INITIALIZED.store(true, Ordering::Release);
}
pub(crate) fn snapshot() -> Status {
    if !INITIALIZED.load(Ordering::Acquire) {
        return Status::EMPTY;
    }
    let mut value = config();
    value.save_count = SAVES.load(Ordering::Relaxed);
    value.restore_count = RESTORES.load(Ordering::Relaxed);
    value.task_init_count = TASK_INITS.load(Ordering::Relaxed);
    value.task_state_bytes = TASK_BYTES.load(Ordering::Relaxed);
    value
}
fn log(event: u8, inherited: &[u64; 3]) {
    let value = snapshot();
    unsafe { fpu_log(event, &value, inherited.as_ptr()) };
}
fn cpuid(leaf: u32, subleaf: u32) -> core::arch::x86_64::CpuidResult {
    core::arch::x86_64::__cpuid_count(leaf, subleaf)
}
fn xgetbv() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("xgetbv", in("ecx") 0u32, out("eax") low, out("edx") high,
        options(nomem, nostack, preserves_flags))
    };
    u64::from(low) | (u64::from(high) << 32)
}
fn component_supported(info: cpu::Status, mask: u64) -> bool {
    if !info.features.xsave || info.max_basic_leaf < 0x0d {
        return false;
    }
    let leaf = cpuid(0x0d, 0);
    (u64::from(leaf.eax) | (u64::from(leaf.edx) << 32)) & mask == mask
}
fn detect_bytes(mask: u64) -> Option<u32> {
    if cpu::snapshot().max_basic_leaf < 0x0d {
        return None;
    }
    let leaf = cpuid(0x0d, 0);
    let mut required = 576u64;
    if mask & AVX != 0 {
        let component = cpuid(0x0d, 2);
        if component.eax == 0 {
            return None;
        }
        required = required.max(u64::from(component.ebx) + u64::from(component.eax));
    }
    if u64::from(leaf.ebx) < required || leaf.ebx as usize > STORAGE_BYTES {
        return None;
    }
    Some(leaf.ebx)
}
fn save_raw(value: Status, output: *mut u8) {
    match value.backend {
        1 => unsafe { r4os_fxsave(output) },
        2 => unsafe { r4os_xsave(output, value.xcr0_mask) },
        _ => {}
    }
}
fn restore_raw(value: Status, source: *const u8) {
    match value.backend {
        1 => unsafe { r4os_fxrstor(source) },
        2 => unsafe { r4os_xrstor(source, value.xcr0_mask) },
        _ => {}
    }
}
pub fn init() -> bool {
    if INITIALIZED.load(Ordering::Acquire) {
        return config().backend != 0;
    }
    let info = cpu::snapshot();
    let mut value = Status::EMPTY;
    if !info.features.fxsr || !info.features.sse || !info.features.sse2 {
        publish(value);
        log(0, &[0; 3]);
        return false;
    }
    let cr0 = (unsafe { r4os_read_cr0() } & !(CR0_EM | CR0_TS)) | CR0_MP | CR0_NE;
    unsafe { r4os_write_cr0(cr0) };
    let mut cr4 = unsafe { r4os_read_cr4() } | CR4_OSFXSR | CR4_OSXMMEXCPT;
    value.avx_supported = info.features.avx && component_supported(info, AVX);
    value.avx2_supported = value.avx_supported && info.features.avx2;
    let mask = if crate::build_config::config::CPU_BASELINE_YMM_TEST {
        X87_SSE
    } else if value.avx_supported {
        X87_SSE | AVX
    } else {
        X87_SSE
    };
    if info.features.xsave {
        cr4 |= CR4_OSXSAVE;
        unsafe {
            r4os_write_cr4(cr4);
            r4os_xsetbv(0, mask)
        };
        let xcr0 = xgetbv();
        if xcr0 & mask == mask {
            let Some(bytes) = detect_bytes(mask) else {
                // Preserve the hard failure; a guessed XSAVE size may overwrite
                // a task buffer or restore a truncated architectural state.
                unsafe { r4os_write_cr4(cr4 & !CR4_OSXSAVE) };
                value.cr0 = unsafe { r4os_read_cr0() };
                value.cr4 = unsafe { r4os_read_cr4() };
                publish(value);
                log(1, &[0; 3]);
                return false;
            };
            value.backend = 2;
            value.xcr0_mask = mask;
            value.state_bytes = bytes;
            value.avx_enabled = mask & AVX != 0;
            value.avx2_enabled = value.avx_enabled && value.avx2_supported;
            cpu::r4os_rust_cpu_control(6, xcr0);
        }
    }
    if value.backend == 0 {
        unsafe { r4os_write_cr4(cr4 & !CR4_OSXSAVE) };
        value.backend = 1;
        value.state_bytes = 512;
    }
    value.simd_abi = if value.avx2_enabled {
        3
    } else if value.avx_enabled {
        2
    } else {
        1
    };
    value.cr0 = unsafe { r4os_read_cr0() };
    value.cr4 = unsafe { r4os_read_cr4() };
    let mut inherited = [0u64; 3];
    unsafe {
        r4os_fninit();
        r4os_init_simd(u32::from(value.avx_enabled), inherited.as_mut_ptr())
    };
    let initial = INITIAL.0.get().cast::<u8>();
    save_raw(value, initial);
    restore_raw(value, initial);
    publish(value);
    log(2, &inherited);
    true
}
pub fn active_bytes() -> usize {
    if !INITIALIZED.load(Ordering::Acquire) {
        return 0;
    }
    let value = config();
    if value.backend == 0 {
        0
    } else {
        value.state_bytes as usize
    }
}
fn valid_buffer(pointer: *const u8, len: usize, bytes: usize) -> bool {
    bytes != 0 && len >= bytes && (pointer as usize & (STORAGE_ALIGN - 1)) == 0
}
pub fn restore_initial() -> bool {
    if active_bytes() == 0 {
        return false;
    }
    restore_raw(config(), INITIAL.0.get().cast::<u8>());
    RESTORES.fetch_add(1, Ordering::Relaxed);
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fpu_control(operation: u8) -> u64 {
    match operation {
        0 => u64::from(init()),
        1 => u64::from({
            if active_bytes() == 0 {
                false
            } else {
                let value = config();
                unsafe {
                    r4os_write_cr0(value.cr0);
                    r4os_write_cr4(value.cr4);
                    if value.backend == 2 {
                        r4os_xsetbv(0, value.xcr0_mask)
                    }
                    r4os_fninit();
                }
                restore_raw(value, INITIAL.0.get().cast::<u8>());
                true
            }
        }),
        2 => u64::from(restore_initial()),
        3 => active_bytes() as u64,
        _ => panic!("Invalid private FPU control"),
    }
}
/// # Safety
/// Caller owns a resident buffer of len bytes for the exact task under runtime
/// admission; save/init permit writes, restore permits reads only. No alias to
/// INITIAL is allowed. Existing buffer size/alignment rejection is preserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fpu_buffer(operation: u8, buffer: *mut u8, len: usize) -> bool {
    let bytes = active_bytes();
    if !valid_buffer(buffer, len, bytes) {
        return false;
    }
    let value = config();
    match operation {
        0 => {
            unsafe { core::ptr::copy_nonoverlapping(INITIAL.0.get().cast::<u8>(), buffer, bytes) };
            TASK_INITS.fetch_add(1, Ordering::Relaxed);
            TASK_BYTES.fetch_add(u64::from(value.state_bytes), Ordering::Relaxed);
        }
        1 => {
            save_raw(value, buffer);
            SAVES.fetch_add(1, Ordering::Relaxed);
        }
        2 => {
            restore_raw(value, buffer);
            RESTORES.fetch_add(1, Ordering::Relaxed);
        }
        _ => panic!("Invalid private FPU buffer operation"),
    }
    true
}
/// # Safety
/// Caller owns a fixed aligned C output until return; no buffer is borrowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fpu_status(output: *mut Status) {
    unsafe { output.write(snapshot()) };
}
const _: () = {
    assert!(core::mem::size_of::<Status>() == 80);
    assert!(core::mem::offset_of!(Status, xcr0_mask) == 16);
    assert!(core::mem::offset_of!(Status, cr0) == 32);
    assert!(core::mem::offset_of!(Status, task_state_bytes) == 72);
};

unsafe fn fpu_log(event: u8, status: *const Status, inherited: *const u64) {
    use crate::boot::log::{
        r4os_rust_bootlog_dec as dec, r4os_rust_bootlog_hex as hex, text as puts,
    };
    let s = unsafe { status.read() };

    if event == 2 {
        puts(b"[FPU] initial SIMD normalized inherited-xmm0=");
        puts(
            if unsafe { inherited.read() != 0 || inherited.add(1).read() != 0 } {
                b"nonzero"
            } else {
                b"zero"
            },
        );
        puts(b" inherited-mxcsr=0x");
        hex(unsafe { inherited.add(2).read() }, 8);
        puts(b" initial-mxcsr=0x00001F80\r\n");
    }
    puts(b"[FPU] ");
    puts(match event {
        0 => b"missing fxsr/sse/sse2",
        1 => b"invalid xsave state size",
        2 => b"ready",
        _ => unreachable!(),
    });
    puts(b" backend=");
    puts(match s.backend {
        1 => b"fxsave",
        2 => b"xsave",
        _ => b"none",
    });
    puts(b" state=");
    dec(u64::from(s.state_bytes));
    puts(b"/");
    dec(u64::from(s.state_storage_bytes));
    puts(b" xcr0=0x");
    hex(s.xcr0_mask, 16);
    puts(b" abi=");
    puts(match s.simd_abi {
        1 => b"sse2",
        2 => b"avx",
        3 => b"avx2",
        _ => b"none",
    });
    puts(b" avx=");
    puts(if s.avx_enabled {
        b"on"
    } else if s.avx_supported {
        b"off"
    } else {
        b"no"
    });
    puts(b" avx2=");
    puts(if s.avx2_enabled {
        b"on"
    } else if s.avx2_supported {
        b"off"
    } else {
        b"no"
    });
    puts(b" cr0=0x");
    hex(s.cr0, 16);
    puts(b" cr4=0x");
    hex(s.cr4, 16);
    puts(b"\r\n");
}
