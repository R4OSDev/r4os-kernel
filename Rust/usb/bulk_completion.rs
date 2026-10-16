//! Original exact/short bulk completion contract; residue is untransferred data.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Evaluation {
    pub accepted: bool,
    pub actual_len: u32,
}
pub const SUCCESS: u8 = 1;
pub const SHORT_PACKET: u8 = 13;
pub fn evaluate(input: bool, requested: u32, code: u8, residue: u32) -> Evaluation {
    if residue > requested {
        return Evaluation {
            accepted: false,
            actual_len: 0,
        };
    }
    let accepted = (code == SUCCESS && residue == 0) || (input && code == SHORT_PACKET);
    Evaluation {
        accepted,
        actual_len: if accepted { requested - residue } else { 0 },
    }
}
/// # Safety
/// Exclusive initialized-layout output for this synchronous scalar evaluation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_bulk_completion(
    input: bool,
    requested: u32,
    code: u8,
    residue: u32,
    out: *mut Evaluation,
) {
    unsafe { out.write(evaluate(input, requested, code, residue)) };
}
const _: () = {
    assert!(core::mem::size_of::<Evaluation>() == 8);
    assert!(core::mem::align_of::<Evaluation>() == 4);
    assert!(core::mem::offset_of!(Evaluation, accepted) == 0);
    assert!(core::mem::offset_of!(Evaluation, actual_len) == 4);
};
