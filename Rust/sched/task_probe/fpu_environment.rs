//! Existing native task/work/reserve owners, with explicit FP-preserving private
//! no-SIMD boundaries. No general SysV register-liveness promise is introduced.
use super::{helpers::*, resources};
use crate::{
    arch::{fpu_probe as fp, percpu, runtime},
    contract as c,
    sched::{
        scheduler,
        task::{
            task_api, task_lifecycle, task_resources,
            task_types::{Bytes, CreateFailure, Role, State},
            thread_placement,
        },
    },
};
use core::{
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering},
};
static KIND: AtomicU32 = AtomicU32::new(0);
static RESULT: AtomicI32 = AtomicI32::new(-1);
static CASES: AtomicU32 = AtomicU32::new(0);
static MIGRATIONS: AtomicU32 = AtomicU32::new(0);
static YIELDS: AtomicU32 = AtomicU32::new(0);
static FAIL: AtomicU32 = AtomicU32::new(0);
// A failed boot probe stops before another job. Keep exact ownership visible
// until reboot instead of treating an uncertain child as released.
static RETAINED_ID: AtomicU32 = AtomicU32::new(0);
static RETAINED_GENERATION: AtomicU64 = AtomicU64::new(0);
fn retain(id: u32, generation: u64) {
    RETAINED_ID.store(id, Ordering::Release);
    RETAINED_GENERATION.store(generation, Ordering::Release);
    puts(b"[FPUENV46] retained native=");
    dec(id.into());
    puts(b" generation=");
    dec(generation);
    puts(b" until=reboot\r\n");
}
pub(super) unsafe extern "C" fn guard(_: usize) {
    unsafe {
        let flags = runtime::save_runtime();
        let mut saved = MaybeUninit::uninit();
        scheduler::r4os_rust_scheduler_irq_enter(saved.as_mut_ptr());
        fp::scribble();
        scheduler::r4os_rust_scheduler_irq_leave(saved.as_ptr());
        runtime::restore_runtime(flags);
    }
}
unsafe extern "C" fn sleeping(_: usize) {
    sleep(1, b"fpu-env-sleep");
}
unsafe extern "C" fn yielding(_: usize) {
    unsafe {
        let task = current();
        let before = view(task);
        yield_task();
        let after = view(task);
        if before.id == after.id
            && before.generation == after.generation
            && after.switches > before.switches
        {
            YIELDS.fetch_add(1, Ordering::Relaxed);
        } else {
            FAIL.fetch_or(1, Ordering::Relaxed);
        }
    }
}
unsafe extern "C" fn migrating(_: usize) {
    unsafe {
        let task = current();
        let identity = view(task);
        let from = percpu::current_index();
        let target = if from == 0 { 1 } else { 0 };
        let policy = thread_placement::Policy {
            cpus: 1u64 << target,
            preferred: Some(target),
            class: c::thread_placement_class_batch,
        };
        if !percpu::is_schedulable(target)
            || thread_placement::set_probe_task(identity.id, identity.generation, policy)
                != c::thread_ok
        {
            FAIL.fetch_or(2, Ordering::Relaxed);
            return;
        }
        yield_task();
        let after = view(task);
        if percpu::current_index() == target
            && after.id == identity.id
            && after.generation == identity.generation
        {
            MIGRATIONS.fetch_add(1, Ordering::Relaxed);
        } else {
            FAIL.fetch_or(4, Ordering::Relaxed);
        }
    }
}
unsafe extern "C" fn worker() {
    let kind = KIND.load(Ordering::Acquire);
    let initial = fp::fresh();
    let boundary = match kind {
        0 => yielding as fp::Boundary,
        1 => migrating as fp::Boundary,
        _ => guard as fp::Boundary,
    };
    let result = fp::run(0x4600 + kind, Some(boundary), 0, 256);
    if let Err(reason) = result {
        puts(b"[FPUENV46] worker reason=");
        puts(reason);
        puts(b"\r\n");
    }
    CASES.store(result.unwrap_or(0), Ordering::Release);
    RESULT.store(
        if initial && result.is_ok() { 0 } else { -1 },
        Ordering::Release,
    );
    // Voluntary terminal exit skips a normal callee return. The next real task
    // or reserve reuse must start from its own canonical environment.
    unsafe {
        fp::controls(0x0b7f, 0x5f80);
        scheduler::r4os_rust_scheduler_control(13, 0, 0);
    }
    panic!("FPU diagnostic terminal exit returned")
}
unsafe fn run_worker(kind: u32, critical: bool) -> Result<usize, &'static [u8]> {
    unsafe {
        KIND.store(kind, Ordering::Release);
        RESULT.store(-1, Ordering::Release);
        CASES.store(0, Ordering::Release);
        FAIL.store(0, Ordering::Release);
        let mut failure = CreateFailure::None;
        let task = if critical {
            task_resources::create_critical(
                Bytes {
                    pointer: b"fpu-env-reserve".as_ptr(),
                    len: 15,
                },
                State::Blocked,
                worker,
                Role::Interactive,
                &mut failure,
            )
        } else {
            let request = task_api::CreateRequest {
                name: b"fpu-environment".as_ptr(),
                name_len: 15,
                state: 3,
                role: 1,
                needs_fpu: true,
                smp_eligible: true,
                smp_r4x: false,
                owner_kind: 0,
                owner_context: ptr::null_mut(),
                entry: Some(worker),
            };
            let mut code = 0;
            task_api::r4os_rust_task_create(&request, false, &mut code).cast()
        };
        if task.is_null() || failure != CreateFailure::None {
            return Err(b"create");
        }
        if !pin(task) {
            let receipt = view(task);
            retain(receipt.id, receipt.generation);
            return Err(b"pin");
        }
        let receipt = view(task);
        let Some(fpu) = receipt.fpu else {
            retain(receipt.id, receipt.generation);
            return Err(b"fpu-buffer-retained");
        };
        let buffer = fpu.pointer.addr();
        if !critical {
            let policy = thread_placement::Policy {
                cpus: 1,
                preferred: Some(0),
                class: c::thread_placement_class_batch,
            };
            if thread_placement::set_probe_task(receipt.id, receipt.generation, policy)
                != c::thread_ok
            {
                retain(receipt.id, receipt.generation);
                return Err(b"initial-placement");
            }
        }
        if scheduler::r4os_rust_scheduler_task(3, task.cast(), 0, 0, ptr::null(), 0) == 0 {
            retain(receipt.id, receipt.generation);
            return Err(b"start");
        }
        let begin = crate::platform::monotonic::capture();
        let tick = now();
        while view(task).state != State::Dead {
            let timed =
                crate::platform::monotonic::elapsed(begin, crate::platform::monotonic::capture())
                    .is_some_and(|ns| ns >= 5_000_000_000);
            if timed || now().wrapping_sub(tick) > 5000 {
                retain(receipt.id, receipt.generation);
                return Err(b"terminal-timeout-retained");
            }
            yield_task();
        }
        let result = RESULT.load(Ordering::Acquire);
        let count = CASES.load(Ordering::Acquire);
        let failed = FAIL.load(Ordering::Acquire);
        // A terminal child requested retirement. Dropping the final pin may
        // already free it; use the existing single atomic retirement receipt.
        loop {
            match task_lifecycle::retire_diagnostic_pin(task, receipt.id, receipt.generation) {
                task_lifecycle::DiagnosticRetire::Released => break,
                task_lifecycle::DiagnosticRetire::Retained => {
                    retain(receipt.id, receipt.generation);
                    return Err(b"release-retained");
                }
                task_lifecycle::DiagnosticRetire::Pending => {
                    if now().wrapping_sub(tick) > 5000 {
                        retain(receipt.id, receipt.generation);
                        return Err(b"release-timeout-retained");
                    }
                    yield_task();
                }
            }
        }
        if result != 0 || count == 0 || failed != 0 {
            return Err(b"worker-state");
        }
        puts(b"[FPUENV46] task kind=");
        dec(kind.into());
        puts(b" critical=");
        dec(critical as u64);
        puts(b" native=");
        dec(receipt.id.into());
        puts(b" generation=");
        dec(receipt.generation);
        puts(b" cases=");
        dec(count.into());
        puts(b" fresh=1 terminal=1 released=1\r\n");
        Ok(buffer)
    }
}
pub(super) fn run() -> bool {
    unsafe {
        if crate::sched::kernel_check::live() {
            return fail(b"fpu-environment-boot-only");
        }
        if !fp::defaults() {
            return fail(b"fpu-incoming-control");
        }
        let controls = fp::control_abort();
        puts(if controls {
            b"[FPUENV46] controlled-resume callback=1 separate-stack=1 mode=x87 controls=exact result=PASS\r\n"
        } else {
            b"[FPUENV46] controlled-resume result=FAILED\r\n"
        });
        if !controls || !resources::work_fpu() {
            return false;
        }
        let main = match fp::run(0x46b5, Some(sleeping), 0, 256) {
            Ok(cases) => cases,
            Err(reason) => return fail(reason),
        };
        puts(b"[FPUENV46] kernel-main sleep-wake cases=");
        dec(main.into());
        puts(b" result=PASS\r\n");
        YIELDS.store(0, Ordering::Relaxed);
        if let Err(reason) = run_worker(0, false) {
            return fail(reason);
        }
        if YIELDS.load(Ordering::Relaxed) == 0 {
            return fail(b"no-yield-switch");
        }
        let migration = percpu::is_schedulable(1);
        MIGRATIONS.store(0, Ordering::Relaxed);
        if migration {
            if let Err(reason) = run_worker(1, false) {
                return fail(reason);
            }
            if MIGRATIONS.load(Ordering::Relaxed) == 0 {
                return fail(b"no-migration");
            }
        }
        let used = r4os_rust_task_control(4, 0, 0);
        let first = match run_worker(2, true) {
            Ok(p) => p,
            Err(r) => return fail(r),
        };
        let second = match run_worker(2, true) {
            Ok(p) => p,
            Err(r) => return fail(r),
        };
        if first != second || r4os_rust_task_control(4, 0, 0) != used {
            return fail(b"reserve-identity-or-release");
        }
        puts(b"[FPUENV46] result=PASS xmm=16 ymm=16 x87=8 top=rotated tags=full-sparse mmx=8 rounding=4 valid-ftz-daz=yes yield_switches=");
        dec(YIELDS.load(Ordering::Relaxed).into());
        puts(b" migrations=");
        dec(MIGRATIONS.load(Ordering::Relaxed).into());
        puts(b" reserve_reuses=2 injection=none\r\n");
        true
    }
}
