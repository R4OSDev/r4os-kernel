//! Existing complete native matrix, with exact instruction execution receipts.
use super::helpers::*;
use crate::arch::{fpu, xsaveopt};
pub(super) fn run() -> bool {
    let (mode, known, supported) = xsaveopt::status();
    if mode == 0 {
        return fail(b"xsaveopt-not-sealed");
    }
    let before = xsaveopt::totals();
    if !super::fpu_contract::run() {
        return false;
    }
    let after = xsaveopt::totals();
    let delta_c = after.0.wrapping_sub(before.0);
    let delta_o = after.1.wrapping_sub(before.1);
    if (mode == 2 && delta_o == 0) || (mode != 2 && delta_o != 0) {
        return fail(b"xsaveopt-execution-receipt");
    }
    let Some(d) = fpu::descriptor() else {
        return fail(b"xsaveopt-descriptor");
    };
    puts(b"[XSAVEOPT48] matrix result=PASS mode=");
    dec(mode.into());
    puts(b" task_classic_delta=");
    dec(delta_c);
    puts(b" task_xsaveopt_delta=");
    dec(delta_o);
    puts(b" known=");
    dec(known);
    puts(b" supported=");
    dec(supported);
    puts(b" mask=");
    dec(d.contract.mask);
    puts(b" active_bytes=");
    dec(d.contract.bytes.into());
    puts(b" layout=standard no_new_components=1\r\n");
    true
}
