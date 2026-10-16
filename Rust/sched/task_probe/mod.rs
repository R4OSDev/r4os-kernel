//! Canonical native orchestration of the existing explicit TaskRegistry cases.
//! Only still-Zig owner snapshots/operations cross fixed private C cuts.
mod deadlines;
mod fpu;
mod helpers;
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
