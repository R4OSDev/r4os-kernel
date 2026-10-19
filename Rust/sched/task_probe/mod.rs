//! Canonical native orchestration of the existing explicit TaskRegistry cases.
//! Native owner operations retain their exact task and resource identities.
mod balance;
mod deadlines;
mod fpu;
mod helpers;
pub(crate) mod live;
mod owner_snapshots;
mod resources;
mod roles;
mod run;
mod state;
mod workers;
#[repr(C)]
struct Span {
    pointer: *const u8,
    len: usize,
}

fn option_is(value: &[u8], wanted: &[u8]) -> bool {
    value.len() == wanted.len()
        && value
            .iter()
            .zip(wanted)
            .all(|(&a, &b)| a.to_ascii_lowercase() == b)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_task_registry_selftest() -> bool {
    let Some(mut gate) = crate::sched::kernel_check::gate() else {
        return false;
    };
    let result = boot_cases();
    if !result || resources::retained_work() {
        gate.retain();
    }
    result
}
fn boot_cases() -> bool {
    unsafe {
        let mut option = Span {
            pointer: core::ptr::null(),
            len: 0,
        };
        task_probe_option(&mut option);
        if option.pointer.is_null() {
            return true;
        }
        let value = core::slice::from_raw_parts(option.pointer, option.len);
        if option_is(value, b"tsc-deadline") {
            if !run::prepare() {
                return false;
            }
            // Reuse the original strict checkpoint/earlier-IRQ and bounded
            // idle-timeout oracles before the isolated hardware/source cases.
            let idle = deadlines::probe() && deadlines::storm();
            let line = if idle {
                b"[TSCDEADLINE-IDLE] result=PASS checkpoint=finite earlier=real-irq due=65 bounded=64+1 future=held\r\n".as_slice()
            } else {
                b"[TSCDEADLINE-CHECK] result=FAILED stage=legacy-idle\r\n".as_slice()
            };
            crate::boot::log::text(line);
            crate::boot::log_route::serial_write_raw(line);
            if !idle {
                crate::arch::lapic::log_timer(0);
                return false;
            }
            return crate::arch::lapic::deadline_boot_check();
        }
        if option_is(value, b"idle-deadline") {
            if !deadlines::storm() {
                return helpers::fail(b"idle-deadline");
            }
            helpers::puts(b"TASKREG08244 idle-timeout PASS due=65 bounded=64+1 future=held timer-irq=unchanged\r\n");
            return true;
        }
        if option_is(value, b"fpu-work") {
            if !resources::work_fpu() {
                return false;
            }
            if !resources::reserve_probe() {
                return helpers::fail(b"fpu-reserve");
            }
            helpers::puts(b"TASKREG08201 fpu-reserve PASS admission=oom reuse=2 initial=clean irq-preserved=yes\r\n");
            return true;
        }
        if !option_is(value, b"yes") {
            return true;
        }
        run::run()
    }
}

unsafe fn task_probe_option(out: *mut Span) {
    unsafe {
        let v = crate::boot::config::option_value(
            crate::boot::config::get(),
            b"TASKREGISTRY",
            b"selftest",
        )
        .unwrap_or(b"");
        out.write(Span {
            pointer: v.as_ptr(),
            len: v.len(),
        });
    }
}
