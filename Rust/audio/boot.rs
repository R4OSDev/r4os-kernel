//! Original BSP audio boot boundary and copied R4D-family status owner.
use super::{core as audio, types::Span};
use crate::{boot::log as bootlog, loader::boot as loader, platform::irq_boot};
use core::{cell::UnsafeCell, ffi::c_void, ptr, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BackendStatus {
    pub state: u32,
    pub selected: bool,
    pub disabled: bool,
    pub attempted: bool,
    pub failed: bool,
    pub r4d_expected: bool,
    pub r4d_loaded: bool,
    pub registered: bool,
    pub active: bool,
    pub reason: Span,
}
impl BackendStatus {
    const EMPTY: Self = Self {
        state: 0,
        selected: false,
        disabled: false,
        attempted: false,
        failed: false,
        r4d_expected: false,
        r4d_loaded: false,
        registered: false,
        active: false,
        reason: Span::literal(b"not selected"),
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub initialized: bool,
    pub core_initialized: bool,
    pub platform_irq_ready: bool,
    pub config_visible: bool,
    pub sid_option_present: bool,
    pub sid_option_valid: bool,
    pub sid_requested_model: [u8; 16],
    pub sid_requested_model_len: usize,
    pub sid_model: [u8; 16],
    pub sid_model_len: usize,
    pub sid_reason: Span,
    pub hda: BackendStatus,
    pub ac97: BackendStatus,
    pub reason: Span,
}
impl Status {
    const EMPTY: Self = Self {
        initialized: false,
        core_initialized: false,
        platform_irq_ready: false,
        config_visible: false,
        sid_option_present: false,
        sid_option_valid: false,
        sid_requested_model: [0; 16],
        sid_requested_model_len: 0,
        sid_model: [0; 16],
        sid_model_len: 0,
        sid_reason: Span::literal(b"not initialized"),
        hda: BackendStatus::EMPTY,
        ac97: BackendStatus::EMPTY,
        reason: Span::literal(b"not initialized"),
    };
}
struct Resident(UnsafeCell<Status>);
// SAFETY: original linear BSP boot/driver-plan reports serialize writes; the
// public observer copies fixed metadata, never another state owner or borrow.
unsafe impl Sync for Resident {}
static STATE: Resident = Resident(UnsafeCell::new(Status::EMPTY));
macro_rules! g {
    ($f:ident) => {
        (*STATE.0.get()).$f
    };
}
#[repr(C)]
pub struct OptionalSpan {
    pub present: bool,
    pub value: Span,
}
unsafe extern "C" {
    fn r4os_zig_audio_boot_fail(text: *const u8, len: usize) -> bool;
    fn r4os_zig_audio_boot_status(text: *const u8, len: usize);
}
fn log(text: &[u8]) {
    audio::log(text)
}
fn status_line(text: &[u8]) {
    unsafe { r4os_zig_audio_boot_status(text.as_ptr(), text.len()) }
}
fn boot(text: &[u8]) {
    unsafe { bootlog::r4os_rust_bootlog_write(text.as_ptr(), text.len()) }
}
fn yes(v: bool) -> &'static [u8] {
    if v { b"yes" } else { b"no" }
}
unsafe fn fail(message: &[u8], reason: &'static [u8]) -> bool {
    unsafe {
        g!(reason) = Span::literal(reason);
        r4os_zig_audio_boot_fail(message.as_ptr(), message.len())
    }
}
unsafe fn driver(config: *const c_void, name: &[u8], disabled: bool) -> bool {
    unsafe {
        if disabled {
            crate::boot::config::is_disabled(config.cast(), name)
        } else {
            crate::boot::config::has_driver(config.cast(), name)
        }
    }
}
pub(crate) unsafe fn init() -> bool {
    unsafe {
        if g!(initialized) {
            return true;
        }
        STATE.0.get().write(Status::EMPTY);
        if !irq_boot::initialized() {
            return fail(
                b"Audio boot before platform IRQ boot",
                b"platform irq missing",
            );
        }
        g!(platform_irq_ready) = true;
        let config = loader::r4os_rust_loader_boot_config();
        if config.is_null() {
            return fail(b"Audio boot before loader config", b"loader config missing");
        }
        g!(config_visible) = true;
        audio::init();
        g!(core_initialized) = true;
        apply_sid_model(config);
        let actual = audio::sid_model_name_z();
        copy_slice(
            slice::from_raw_parts(actual, 4),
            ptr::addr_of_mut!(g!(sid_model)).cast(),
            16,
            ptr::addr_of_mut!(g!(sid_model_len)),
        );
        g!(hda) = start_hda(config);
        g!(ac97) = plan_ac97(config);
        g!(initialized) = true;
        g!(reason) = Span::literal(b"ok");
        status_line(b"  Audio boot [OK]\r\n");
        log_status();
        true
    }
}
unsafe fn apply_sid_model(config: *const c_void) {
    unsafe {
        let mut value = OptionalSpan {
            present: false,
            value: Span::EMPTY,
        };
        if let Some(text) = crate::boot::config::option_value(config.cast(), b"SID", b"model") {
            value.present = true;
            value.value = Span {
                pointer: text.as_ptr(),
                len: text.len(),
            };
        }

        if value.present {
            let model = value.value.bytes();
            g!(sid_option_present) = true;
            g!(sid_option_valid) = known_sid_model(model);
            copy_slice(
                model,
                ptr::addr_of_mut!(g!(sid_requested_model)).cast(),
                16,
                ptr::addr_of_mut!(g!(sid_requested_model_len)),
            );
            audio::configure_sid_model(model);
            g!(sid_reason) = Span::literal(if g!(sid_option_valid) {
                b"configured from BOOT.R4S"
            } else {
                b"invalid BOOT.R4S model; default retained"
            });
            return;
        }
        g!(sid_reason) = Span::literal(b"default model");
    }
}
unsafe fn start_hda(config: *const c_void) -> BackendStatus {
    unsafe {
        if driver(config, b"HDA", true) {
            log(b"[HDA] skipped, disabled by CONFIG.R4S\r\n");
            return BackendStatus {
                state: 1,
                disabled: true,
                reason: Span::literal(b"disabled"),
                ..BackendStatus::EMPTY
            };
        }
        if !driver(config, b"HDA", false) {
            log(b"[HDA] skipped, not selected by DRIVER=HDA\r\n");
            return BackendStatus::EMPTY;
        }
        log(b"[HDA] R4D planned via DRIVER=HDA\r\n");
        BackendStatus {
            state: 4,
            selected: true,
            r4d_expected: true,
            reason: Span::literal(b"planned for HDA.R4D"),
            ..BackendStatus::EMPTY
        }
    }
}
unsafe fn plan_ac97(config: *const c_void) -> BackendStatus {
    unsafe {
        let selected = driver(config, b"AC97", false);
        if driver(config, b"AC97", true) {
            log(b"[AC97] R4D skipped, disabled by CONFIG.R4S\r\n");
            if selected {
                status_line(b"  AC97.R4D [disabled]\r\n");
            }
            return BackendStatus {
                state: 1,
                selected,
                disabled: true,
                reason: Span::literal(b"disabled"),
                ..BackendStatus::EMPTY
            };
        }
        if !selected {
            log(b"[AC97] R4D skipped, not selected by DRIVER=AC97\r\n");
            return BackendStatus::EMPTY;
        }
        log(b"[AC97] R4D planned via DRIVER=AC97\r\n");
        BackendStatus {
            state: 4,
            selected: true,
            r4d_expected: true,
            reason: Span::literal(b"planned for AC97.R4D"),
            ..BackendStatus::EMPTY
        }
    }
}
pub(crate) unsafe fn report_ac97(result: u32) {
    unsafe {
        if !g!(ac97).r4d_expected {
            return;
        }
        match result {
            0 | 1 => {
                refresh_ac97(b"r4d loaded");
                visible_line(false);
            }
            2 => {
                g!(ac97).state = 5;
                g!(ac97).attempted = true;
                g!(ac97).failed = false;
                g!(ac97).reason = Span::literal(b"AC97.R4D missing");
                status_line(b"  AC97.R4D [missing]\r\n");
            }
            3 => {
                g!(ac97).state = 3;
                g!(ac97).attempted = true;
                g!(ac97).failed = true;
                g!(ac97).reason = Span::literal(b"AC97.R4D init failed");
                status_line(b"  AC97.R4D [init failed]\r\n");
            }
            4 => {
                g!(ac97).state = 3;
                g!(ac97).attempted = true;
                g!(ac97).failed = true;
                g!(ac97).reason = Span::literal(b"AC97.R4D load failed");
                status_line(b"  AC97.R4D [load failed]\r\n");
            }
            _ => panic!("Invalid private audio plan result"),
        }
        log_status();
    }
}
pub(crate) unsafe fn report_hda(result: u32) {
    unsafe {
        if !g!(hda).r4d_expected {
            return;
        }
        match result {
            0 | 1 => {
                refresh_hda(b"r4d loaded");
                visible_line(true);
            }
            2 => {
                g!(hda).state = 5;
                g!(hda).attempted = true;
                g!(hda).failed = false;
                g!(hda).reason = Span::literal(b"HDA.R4D missing");
                status_line(b"  HDA.R4D [missing]\r\n");
            }
            3 => {
                g!(hda).state = 3;
                g!(hda).attempted = true;
                g!(hda).failed = true;
                g!(hda).reason = Span::literal(b"HDA.R4D init failed");
                status_line(b"  HDA.R4D [init failed]\r\n");
            }
            4 => {
                g!(hda).state = 3;
                g!(hda).attempted = true;
                g!(hda).failed = true;
                g!(hda).reason = Span::literal(b"HDA.R4D load failed");
                status_line(b"  HDA.R4D [load failed]\r\n");
            }
            _ => panic!("Invalid private audio plan result"),
        }
        log_status();
    }
}
unsafe fn refresh_hda(reason: &'static [u8]) {
    unsafe {
        g!(hda).r4d_loaded = true;
        g!(hda).attempted = true;
        g!(hda).failed = false;
        let backend = audio::audio_backend_family_status(b"HDA");
        g!(hda).registered = backend.registered;
        g!(hda).active = backend.active;
        g!(hda).state = if g!(hda).active {
            8
        } else if g!(hda).registered && backend.has_output {
            7
        } else {
            6
        };
        g!(hda).reason = Span::literal(if g!(hda).active {
            b"HDA.R4D backend active"
        } else if g!(hda).registered {
            b"HDA.R4D backend registered"
        } else {
            reason
        });
    }
}
unsafe fn refresh_ac97(reason: &'static [u8]) {
    unsafe {
        g!(ac97).r4d_loaded = true;
        g!(ac97).attempted = true;
        g!(ac97).failed = false;
        g!(ac97).registered = audio::audio_backend_registered(b"AC97");
        g!(ac97).active = audio::audio_backend_active(b"AC97");
        g!(ac97).state = if g!(ac97).active {
            8
        } else if g!(ac97).registered && audio::audio_backend_has_output(b"AC97") {
            7
        } else {
            6
        };
        g!(ac97).reason = Span::literal(if g!(ac97).active {
            b"AC97.R4D backend active"
        } else if g!(ac97).registered {
            b"AC97.R4D backend registered"
        } else {
            reason
        });
    }
}
unsafe fn visible_line(hda: bool) {
    unsafe {
        let state = if hda { g!(hda).state } else { g!(ac97).state };
        match (hda, state) {
            (true, 8) => status_line(b"  HDA.R4D [active]\r\n"),
            (true, 7) => status_line(b"  HDA.R4D [registered]\r\n"),
            (true, 6) => status_line(b"  HDA.R4D [loaded]\r\n"),
            (false, 8) => status_line(b"  AC97.R4D [active]\r\n"),
            (false, 7) => status_line(b"  AC97.R4D [registered]\r\n"),
            (false, 6) => status_line(b"  AC97.R4D [loaded]\r\n"),
            _ => {}
        }
    }
}
fn backend_state_name(state: u32) -> &'static [u8] {
    match state {
        0 => b"skipped",
        1 => b"disabled",
        2 => b"selected",
        3 => b"failed",
        4 => b"r4d-planned",
        5 => b"r4d-missing",
        6 => b"r4d-loaded",
        7 => b"r4d-registered",
        8 => b"r4d-active",
        _ => panic!("Invalid private audio boot state"),
    }
}
unsafe fn log_status() {
    unsafe {
        boot(b"[AUDIOBOOT] initialized=");
        boot(yes(g!(initialized)));
        boot(b" core=");
        boot(yes(g!(core_initialized)));
        boot(b" config=");
        boot(yes(g!(config_visible)));
        boot(b" sid_option=");
        boot(if g!(sid_option_present) {
            b"present"
        } else {
            b"default"
        });
        if g!(sid_option_present) {
            boot(b" requested=");
            boot(sid_requested().bytes());
            boot(b" valid=");
            boot(yes(g!(sid_option_valid)));
        }
        boot(b" sid_model=");
        boot(sid_model().bytes());
        boot(b" sid_reason=");
        boot(g!(sid_reason).bytes());
        boot(b" hda=");
        boot(backend_state_name(g!(hda).state));
        boot(b" hda_reason=");
        boot(g!(hda).reason.bytes());
        if g!(hda).r4d_expected {
            boot(b" hda_r4d_loaded=");
            boot(yes(g!(hda).r4d_loaded));
            boot(b" hda_registered=");
            boot(yes(g!(hda).registered));
            boot(b" hda_active=");
            boot(yes(g!(hda).active));
        }
        boot(b" ac97=");
        boot(backend_state_name(g!(ac97).state));
        boot(b" ac97_reason=");
        boot(g!(ac97).reason.bytes());
        if g!(ac97).r4d_expected {
            boot(b" ac97_r4d_loaded=");
            boot(yes(g!(ac97).r4d_loaded));
            boot(b" ac97_registered=");
            boot(yes(g!(ac97).registered));
            boot(b" ac97_active=");
            boot(yes(g!(ac97).active));
        }
        boot(b"\r\n");
    }
}
unsafe fn sid_model() -> Span {
    unsafe {
        Span {
            pointer: ptr::addr_of!(g!(sid_model)).cast(),
            len: g!(sid_model_len),
        }
    }
}
unsafe fn sid_requested() -> Span {
    unsafe {
        Span {
            pointer: ptr::addr_of!(g!(sid_requested_model)).cast(),
            len: g!(sid_requested_model_len),
        }
    }
}
pub(crate) unsafe fn dump_status() {
    unsafe {
        log(b"Audio boot:\r\n");
        log(b"  Core: ");
        log(if g!(core_initialized) {
            b"ready"
        } else {
            b"not ready"
        });
        log(b" config=");
        log(yes(g!(config_visible)));
        log(b"\r\n");
        dump_backend(b"HDA", g!(hda));
        dump_backend(b"AC97", g!(ac97));
        log(b"  SID boot: model=");
        log(sid_model().bytes());
        log(b" reason=");
        log(g!(sid_reason).bytes());
        log(b"\r\n");
    }
}
unsafe fn dump_backend(name: &[u8], backend: BackendStatus) {
    unsafe {
        log(b"  ");
        log(name);
        log(b": state=");
        log(backend_state_name(backend.state));
        log(b" selected=");
        log(yes(backend.selected));
        log(b" disabled=");
        log(yes(backend.disabled));
        if backend.r4d_expected {
            log(b" r4d=");
            log(if backend.r4d_loaded {
                b"loaded"
            } else {
                b"expected"
            });
            log(b" registered=");
            log(yes(backend.registered));
            log(b" active=");
            log(yes(backend.active));
        }
        log(b" reason=");
        log(backend.reason.bytes());
        log(b"\r\n");
    }
}
fn known_sid_model(model: &[u8]) -> bool {
    [&b"6581"[..], &b"MOS6581"[..], &b"8580"[..], &b"MOS8580"[..]]
        .iter()
        .any(|s| model.eq_ignore_ascii_case(s))
}
unsafe fn copy_slice(src: &[u8], dst: *mut u8, capacity: usize, length: *mut usize) {
    unsafe {
        let len = if src.len() < capacity {
            src.len()
        } else {
            capacity - 1
        };
        if len > 0 {
            ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
        }
        if len < capacity {
            dst.add(len).write(0);
        }
        length.write(len);
    }
}
/// # Safety
/// Original serialized BSP/report caller supplies copied C status/span storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_audio_boot(op: u32, value: u32, out: *mut c_void) -> bool {
    unsafe {
        match op {
            0 => init(),
            1 => g!(initialized),
            2 => {
                out.cast::<Status>().write(STATE.0.get().read());
                true
            }
            3 => {
                out.cast::<BackendStatus>().write(g!(hda));
                true
            }
            4 => {
                out.cast::<BackendStatus>().write(g!(ac97));
                true
            }
            5 => {
                out.cast::<Span>().write(sid_model());
                true
            }
            6 => {
                out.cast::<Span>().write(sid_requested());
                true
            }
            7 => {
                report_ac97(value);
                true
            }
            8 => {
                report_hda(value);
                true
            }
            9 => {
                out.cast::<Span>()
                    .write(Span::literal(backend_state_name(value)));
                true
            }
            10 => {
                dump_status();
                true
            }
            _ => false,
        }
    }
}
const _: () = {
    assert!(size_of::<BackendStatus>() == 32);
    assert!(size_of::<Status>() == 152);
    assert!(size_of::<OptionalSpan>() == 24);
};
