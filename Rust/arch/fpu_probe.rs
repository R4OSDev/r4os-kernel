//! Explicit diagnostic FP-preserving bridge into reviewed integer-only kernel
//! helpers. This is not the ordinary SysV caller-saved-register contract.
pub(crate) mod data {
    include!("fpu_probe_data.rs");
}
use crate::arch::fpu;
use data::{Control, Output, Pattern};
core::arch::global_asm!(include_str!("fpu_probe.S"), options(att_syntax));
core::arch::global_asm!(include_str!("fpu_probe_aux.S"), options(att_syntax));
pub(crate) type Boundary = unsafe extern "C" fn(usize);
unsafe extern "C" {
    fn r4os_fp_matrix(
        pattern: *const Pattern,
        out: *mut Output,
        control: *const Control,
        boundary: Option<Boundary>,
        context: usize,
        rounds: u64,
    );
    fn r4os_fp_scribble(avx: u32);
    fn r4os_fp_vectors(out: *mut u8, avx: u32);
}
unsafe extern "sysv64" {
    fn r4os_fp_abort_body(frame: usize) -> i32;
}
#[repr(C, align(64))]
pub(crate) struct Legacy {
    pub bytes: [u8; 512],
}
type State = fpu::Image;
pub(crate) fn snapshot() -> Legacy {
    let mut image = Legacy { bytes: [0; 512] };
    unsafe {
        core::arch::asm!("fxsave64 [{}]",in(reg)image.bytes.as_mut_ptr(),options(nostack));
    }
    image
}
pub(crate) fn defaults() -> bool {
    let image = snapshot();
    data::default_environment(&image.bytes)
}
pub(crate) fn fresh() -> bool {
    if !defaults() {
        return false;
    }
    let mut lanes = [0u8; 512];
    let avx = fpu::snapshot().avx_enabled;
    unsafe {
        r4os_fp_vectors(lanes.as_mut_ptr(), avx as u32);
    }
    for lane in 0..16 {
        let width = if avx { 32 } else { 16 };
        if lanes[lane * 32..lane * 32 + width].iter().any(|&b| b != 0) {
            return false;
        }
    }
    true
}
pub(crate) unsafe fn scribble() {
    unsafe {
        r4os_fp_scribble(fpu::snapshot().avx_enabled as u32);
    }
}
pub(crate) unsafe fn controls(cw: u16, mxcsr: u32) {
    unsafe {
        core::arch::asm!("fninit","fldcw [{}]","ldmxcsr [{}]",in(reg)&cw,in(reg)&mxcsr,options(nostack));
    }
}
pub(crate) fn run(
    seed: u32,
    boundary: Option<Boundary>,
    context: usize,
    rounds: u64,
) -> Result<u32, &'static [u8]> {
    let avx = fpu::snapshot().avx_enabled;
    let mask = data::mxcsr_mask(&snapshot().bytes);
    let pattern = Pattern::new(seed);
    let mut out = Output::new();
    let mut cases = 0;
    for round in 0..4 {
        for variant in 0..4 {
            let extras =
                if variant & 1 != 0 { 0x40 } else { 0 } | if variant & 2 != 0 { 0x8000 } else { 0 };
            if extras & !mask != 0 {
                continue;
            }
            for phase in 0..3 {
                let control = Control::new(round, extras, avx, phase);
                unsafe {
                    r4os_fp_matrix(&pattern, &mut out, &control, boundary, context, rounds);
                }
                data::validate(&pattern, control, &out)?;
                cases += 1;
            }
        }
    }
    Ok(cases)
}
/// Controlled nonlocal callback and separate-stack returns; no CPU fault is
/// raised, no Rust/Drop frame lies between the assembly callback and resume.
pub(crate) fn control_abort() -> bool {
    use crate::arch::callback_abort as abort;
    let before = snapshot();
    let mut frame = abort::Frame::default();
    #[repr(align(16))]
    struct Stack([u8; 16384]);
    let mut stack = Stack([0; 16384]);
    let mut ok = true;
    for separate in [false, true] {
        unsafe {
            controls(0x077f, 0x3f80);
        }
        let context = core::ptr::addr_of_mut!(frame) as usize;
        let result = unsafe {
            if separate {
                abort::r4os_rust_program_invoke(
                    r4os_fp_abort_body as *const () as usize,
                    context,
                    stack.0.as_mut_ptr().add(stack.0.len()) as u64,
                    &mut frame,
                )
            } else {
                abort::r4os_rust_callback_invoke(r4os_fp_abort_body, context, &mut frame)
            }
        };
        let restored = snapshot();
        ok &= result == 46
            && data::u16_at(&restored.bytes, 0) == 0x077f
            && data::u32_at(&restored.bytes, 24) == 0x3f80
            && restored.bytes[4] == 0;
    }
    unsafe {
        controls(
            data::u16_at(&before.bytes, 0),
            data::u32_at(&before.bytes, 24),
        );
    }
    ok
}
struct Raw {
    saved: State,
    avx: bool,
    ok: bool,
}
unsafe extern "C" fn raw_boundary(context: usize) {
    let raw = context as *mut Raw;
    unsafe {
        (*raw).ok &=
            fpu::r4os_rust_fpu_buffer(1, (*raw).saved.bytes.as_mut_ptr(), fpu::STORAGE_BYTES);
        (*raw).ok &= fpu::restore_initial();
        r4os_fp_scribble((*raw).avx as u32);
        (*raw).ok &=
            fpu::r4os_rust_fpu_buffer(2, (*raw).saved.bytes.as_mut_ptr(), fpu::STORAGE_BYTES);
    }
}
pub(crate) fn backend_diagnostic() -> ! {
    use crate::boot::log_route::{dec, puts};
    let status = fpu::snapshot();
    let mut raw = Raw {
        saved: State::filled(0xa5),
        avx: status.avx_enabled,
        ok: true,
    };
    raw.ok =
        unsafe { fpu::r4os_rust_fpu_buffer(0, raw.saved.bytes.as_mut_ptr(), fpu::STORAGE_BYTES) };
    let initial = fresh();
    let result = run(
        0x465842,
        Some(raw_boundary),
        core::ptr::addr_of_mut!(raw) as usize,
        64,
    );
    let tail = raw.saved.bytes[status.state_bytes as usize..]
        .iter()
        .all(|&b| b == 0xa5);
    let pass = initial
        && raw.ok
        && tail
        && result.is_ok()
        && !status.avx_enabled
        && matches!(status.backend, 1 | 2);
    puts(b"[FPUBACKEND46] backend=");
    dec(status.backend.into());
    puts(b" mask=");
    dec(status.xcr0_mask);
    puts(b" active_bytes=");
    dec(status.state_bytes.into());
    puts(b" cases=");
    dec(result.unwrap_or(0).into());
    puts(b" xmm=16 x87=8 mmx=8 bsp_only=1 modules_admitted=0 tail=");
    dec(tail as u64);
    puts(if pass {
        b" result=PASS\r\n"
    } else {
        b" result=FAILED\r\n"
    });
    if let Err(reason) = result {
        puts(b"[FPUBACKEND46] reason=");
        puts(reason);
        puts(b"\r\n");
    }
    // No timer/drain task has been admitted in this early fixture.
    crate::boot::log_route::serial_flush();
    crate::boot::fatal::halt()
}
