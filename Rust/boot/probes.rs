//! Explicit original boot diagnostics. No probe runs without its existing flag.
use super::{config, crash, fatal, log_route};
use crate::{
    display::{crash_screen, diag_screen},
    sched::task::task_api,
};
use core::{mem::MaybeUninit, ptr};

pub(super) fn smp_enabled() -> bool {
    unsafe { config::option_value(config::get(), b"SMP", b"selftest") }
        .is_some_and(|s| s.eq_ignore_ascii_case(b"yes"))
}

pub(super) fn crash_renderer() -> ! {
    let first = diag_screen::begin_resolvable();
    diag_screen::text(b"Resolved diagnostic predecessor.");
    diag_screen::end_line();
    diag_screen::reset_external();
    if first == 0 || !diag_screen::resolve(first) {
        fatal::kernel(1, b"Diagnostic handoff token mismatch");
    }
    let retained = diag_screen::begin_resolvable();
    diag_screen::text(b"Retained diagnostic incident after handoff.");
    diag_screen::end_line();
    diag_screen::begin();
    if retained == 0 || diag_screen::resolve(first) || diag_screen::resolve(retained) {
        fatal::kernel(1, b"Diagnostic incident seal mismatch");
    }
    let frame = crash::CpuFrameSnapshot {
        registers: crash::CpuRegisters {
            rax: 1,
            rbx: 2,
            rcx: 3,
            rdx: 4,
            rbp: 0xffff_ffff_8000_1000,
            ..crash::CpuRegisters::default()
        },
        vector: 14,
        error_code: 0b10111,
        rip: 0xffff_ffff_8000_2000,
        rsp: 0xffff_ffff_8000_3000,
        cs: 8,
        rflags: 0x202,
    };
    let memory = crash::FaultMemoryInfo {
        state: 1,
        owner: 254,
        kind: 13,
        status: 254,
        ..crash::FaultMemoryInfo::default()
    };
    let message = b"Crash screen renderer test";
    let input = crash::BuildInput {
        cause: crash::cause_vector(14),
        phase: 1,
        ticks: 0,
        cpu: &frame,
        cr2: 0xffff_ffff_dead_beef,
        memory: &memory,
        context: ptr::null(),
        message: message.as_ptr(),
        message_len: message.len(),
    };
    let report = unsafe { crash::build(&input) };
    unsafe {
        crash_screen::r4os_rust_crash_screen_render(&report);
    }
    log_route::serial_write_raw(b"[CRASH-RENDER] completed\r\n");
    fatal::halt()
}

pub(super) fn nvme_interrupt() -> bool {
    if !unsafe { crate::storage::block_diagnostics::r4os_rust_block_nvme_probe() } {
        return false;
    }
    const REQUIRED: u32 = 1 | (1 << 8) | (1 << 9) | (1 << 10) | (1 << 11) | (1 << 12);
    const FORBIDDEN: u32 = (1 << 13) | (1 << 14) | (1 << 15);
    let count = crate::storage::block_registry::r4os_rust_block_registry_query(1, 0);
    for index in 0..count {
        let Some(config) = crate::storage::block_device::config(index) else {
            continue;
        };
        if config.bus != 3 {
            continue;
        }
        let mut status = crate::contract::StorageBackendStatus::default();
        if unsafe {
            crate::driver::api_storage::r4os_rust_driver_storage_status(index, &mut status)
        } && status.state & REQUIRED == REQUIRED
            && status.state & FORBIDDEN == 0
        {
            log_route::puts(b"[NVMEIRQ] result=OK mode=");
            log_route::puts(if status.state & (1 << 16) != 0 {
                b"msix"
            } else {
                b"msi"
            });
            log_route::puts(b" irq=nonzero work=nonzero completions=nonzero poll_fallback=0\r\n");
            return true;
        }
        log_route::puts(b"[NVMEIRQ] result=FAILED state=0x");
        log_route::hex(u64::from(status.state), 8);
        log_route::puts(b"\r\n");
        return false;
    }
    log_route::puts(b"[NVMEIRQ] result=FAILED reason=device-missing\r\n");
    false
}

#[inline(never)]
#[allow(unconditional_recursion)] // Deliberately exercises the task's guard page.
fn overflow(depth: u64) -> u64 {
    let mut pad = MaybeUninit::<[u8; 512]>::uninit();
    let p = pad.as_mut_ptr().cast::<u8>();
    unsafe {
        p.write(depth as u8);
    }
    core::hint::black_box(p);
    let next = overflow(depth + 1);
    next.wrapping_add(u64::from(unsafe { p.read() }))
}
unsafe extern "C" fn stack_guard() {
    core::hint::black_box(overflow(1));
}
pub(super) fn stack_guard_task() {
    let r = task_api::CreateRequest {
        name: b"st-overflow".as_ptr(),
        name_len: 11,
        state: 1,
        role: 1,
        needs_fpu: true,
        smp_eligible: false,
        smp_r4x: false,
        owner_kind: 0,
        owner_context: ptr::null_mut(),
        entry: Some(stack_guard),
    };
    let mut failure = 0;
    unsafe {
        task_api::r4os_rust_task_create(&r, false, &mut failure);
    }
}
