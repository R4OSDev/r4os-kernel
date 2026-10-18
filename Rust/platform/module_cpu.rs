//! Admission of the SDK's exact module ISA before any foreign code runs.
//! Build constants come from Contract/Module/CpuBaseline.json; the FPU
//! backend alone never authorizes the AVX2 module profile.
use crate::{arch::fpu, build_config::config};
use core::{
    arch::x86_64::CpuidResult,
    sync::atomic::{AtomicBool, Ordering},
};

static ALLOWED: AtomicBool = AtomicBool::new(false);

pub(crate) fn allowed() -> bool {
    ALLOWED.load(Ordering::Acquire)
}

fn cpuid(leaf: u32, subleaf: u32) -> CpuidResult {
    core::arch::x86_64::__cpuid_count(leaf, subleaf)
}

/// Raw local hardware admission, before XSETBV or SIMD state accesses.
/// The same descriptor is suitable for a subsequently qualified AP.
pub(crate) fn hardware_failure() -> Option<&'static [u8]> {
    let basic = cpuid(0, 0).eax;
    let extended = cpuid(0x8000_0000, 0).eax;
    for &(name, leaf, subleaf, register, bit) in &config::MODULE_CPU_REQUIREMENTS {
        let maximum = if leaf >= 0x8000_0000 { extended } else { basic };
        if leaf > maximum {
            return Some(name);
        }
        let value = cpuid(leaf, subleaf);
        let value = match register {
            0 => value.eax,
            1 => value.ebx,
            2 => value.ecx,
            3 => value.edx,
            _ => unreachable!(),
        };
        if value & (1u32 << bit) == 0 {
            return Some(name);
        }
    }
    if basic < 0x0d {
        return Some(b"xsave-components");
    }
    let components = cpuid(0x0d, 0);
    let supported = u64::from(components.eax) | (u64::from(components.edx) << 32);
    for (mask, name) in [
        (1, b"x87-state".as_slice()),
        (2, b"sse-state"),
        (4, b"ymm-state"),
    ] {
        if config::MODULE_CPU_XCR0_MASK & mask != 0 && supported & mask == 0 {
            return Some(name);
        }
    }
    None
}

pub(crate) fn require_hardware() {
    if config::CPU_BASELINE_YMM_TEST {
        crate::boot::log_route::puts(b"[CPU] baseline-test injected=XCR0-without-YMM\r\n");
    }
    if let Some(reason) = hardware_failure() {
        reject(reason);
    }
}

fn state_failure() -> Option<&'static [u8]> {
    let state = fpu::snapshot();
    if !state.enabled || state.backend != 2 {
        return Some(b"xsave-backend");
    }
    // OSXSAVE is checked after our own CR4 initialization. Firmware need
    // not have enabled it on entry. Never execute XGETBV without this gate.
    if cpuid(1, 0).ecx & (1 << 27) == 0 || state.cr4 & (1 << 18) == 0 {
        return Some(b"osxsave");
    }
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("xgetbv", in("ecx") 0u32, out("eax") low, out("edx") high,
            options(nomem, nostack, preserves_flags));
    }
    let active = u64::from(low) | (u64::from(high) << 32);
    let required = config::MODULE_CPU_XCR0_MASK;
    if active & required != required || state.xcr0_mask != required {
        return Some(b"xcr0");
    }
    if !state.avx_enabled || !state.avx2_enabled {
        return Some(b"avx2-state");
    }
    let layout = cpuid(0x0d, 0);
    let ymm = cpuid(0x0d, 2);
    if state.state_bytes != layout.ebx
        || state.state_bytes > state.state_storage_bytes
        || ymm.eax != 256
        || ymm.ebx < 576
        || u64::from(ymm.ebx) + u64::from(ymm.eax) > u64::from(state.state_bytes)
    {
        return Some(b"xsave-size");
    }
    if state.cr0 & ((1 << 2) | (1 << 3)) != 0
        || state.cr0 & ((1 << 1) | (1 << 5)) != (1 << 1) | (1 << 5)
        || state.cr4 & ((1 << 9) | (1 << 10)) != (1 << 9) | (1 << 10)
    {
        return Some(b"fpu-controls");
    }
    None
}

pub(crate) fn admit_bsp() {
    if let Some(reason) = hardware_failure().or_else(state_failure) {
        reject(reason);
    }
    ALLOWED.store(true, Ordering::Release);
    crate::boot::log_route::puts(b"[CPU] module-baseline=");
    crate::boot::log_route::puts(config::MODULE_CPU_NAME);
    crate::boot::log_route::puts(b" status=allowed xcr0=");
    crate::boot::log_route::hex(config::MODULE_CPU_XCR0_MASK, 16);
    crate::boot::log_route::puts(b"\r\n");
}

fn reject(reason: &[u8]) -> ! {
    use crate::boot::log_route as log;
    log::puts(b"[CPU] module-baseline=");
    log::puts(config::MODULE_CPU_NAME);
    log::puts(b" status=blocked missing=");
    log::puts(reason);
    log::puts(b" modules=0\r\n");
    let prefix = b"CPU module baseline missing ";
    let mut message = [0; 128];
    message[..prefix.len()].copy_from_slice(prefix);
    let len = reason.len().min(message.len() - prefix.len());
    message[prefix.len()..prefix.len() + len].copy_from_slice(&reason[..len]);
    crate::boot::fatal::kernel(2, &message[..prefix.len() + len])
}
