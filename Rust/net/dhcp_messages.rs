//! Sole original DHCP reply expectation and bounded test-loss injection.
//! Expectation checks are short metadata operations; R4P work and diagnostics
//! run after admission is released. Other clients' broadcasts never bind leases.
use super::{dhcp, r4p_contract as wire, readers};
use crate::{
    boot::{foundation, log},
    memory::owner::Guard,
    sched::sync_types::Text,
};
use core::{cell::UnsafeCell, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DhcpExpectation {
    pub expected: u8,
    pub xid: u32,
    pub mac: [u8; 6],
}
impl DhcpExpectation {
    const EMPTY: Self = Self {
        expected: 0,
        xid: 0,
        mac: [0; 6],
    };
}
struct State {
    expectation: DhcpExpectation,
    drop_offers: u8,
    drop_acks: u8,
}
struct Storage(UnsafeCell<State>);
// SAFETY: every resident access is bounded under short network admission;
// no reference or token spans R4P execution, caller copies or logging.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    expectation: DhcpExpectation::EMPTY,
    drop_offers: 0,
    drop_acks: 0,
}));
#[repr(u8)]
#[derive(Clone, Copy)]
enum Action {
    WireFailure,
    Inactive,
    Malformed,
    Foreign,
    OutOfPhase,
    DropOffer,
    DropAck,
    Accept,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DhcpMessageReport {
    pub dispatch_failures: u64,
    pub reason: Text,
    pub action: u8,
    pub remaining: u8,
}
impl DhcpMessageReport {
    const EMPTY: Self = Self {
        dispatch_failures: 0,
        reason: Text::literal(b""),
        action: Action::WireFailure as u8,
        remaining: 0,
    };
}
fn parse_count(text: &[u8]) -> u8 {
    let mut parsed = 0u16;
    for &ch in text {
        if !ch.is_ascii_digit() {
            return 0;
        }
        parsed = parsed * 10 + u16::from(ch - b'0');
        if parsed > 9 {
            return 9;
        }
    }
    parsed as u8
}
fn bootlog(text: &[u8]) {
    unsafe {
        log::r4os_rust_bootlog_write(text.as_ptr(), text.len());
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dhcp_messages_reset() {
    let _guard = Guard::acquire(2);
    unsafe {
        STATE.0.get().write(State {
            expectation: DhcpExpectation::EMPTY,
            drop_offers: 0,
            drop_acks: 0,
        });
    }
}
/// # Safety
/// Immutable boot-option text, empty for absence. No configuration pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_test_injection(offers: Text, acks: Text) {
    let offers = parse_count(if offers.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(offers.pointer, offers.len) }
    });
    let acks = parse_count(if acks.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(acks.pointer, acks.len) }
    });
    {
        let _guard = Guard::acquire(2);
        unsafe {
            (*STATE.0.get()).drop_offers = offers;
            (*STATE.0.get()).drop_acks = acks;
        }
    }
    if offers != 0 || acks != 0 {
        bootlog(b"[DHCP05913] test injection offer=");
        log::r4os_rust_bootlog_dec(u64::from(offers));
        bootlog(b" ack=");
        log::r4os_rust_bootlog_dec(u64::from(acks));
        bootlog(b"\r\n");
    }
}
/// # Safety
/// Writable exact paired expectation output; no reference escapes admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_expectation(out: *mut DhcpExpectation) {
    let value = {
        let _guard = Guard::acquire(2);
        unsafe { (*STATE.0.get()).expectation }
    };
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Readable exact paired expectation. Only the original three expected phases exist.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_expectation_set(value: *const DhcpExpectation) {
    let value = unsafe { *value };
    assert!(value.expected <= 2, "Original DHCP response expectation");
    let _guard = Guard::acquire(2);
    unsafe {
        (*STATE.0.get()).expectation = value;
    }
}
fn expected(value: DhcpExpectation, flags: u32) -> bool {
    match value.expected {
        0 => false,
        1 => flags & wire::DHCP_FLAG_OFFER != 0,
        2 => flags & (wire::DHCP_FLAG_ACK | wire::DHCP_FLAG_NAK) != 0,
        _ => unreachable!(),
    }
}
/// # Safety
/// Retained readable packet and disjoint writable paired outputs. The operation
/// output is initialized only for Malformed/Foreign/OutOfPhase/Drop/Accept actions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_message(
    pending: bool,
    data: *const u8,
    len: usize,
    operation: *mut wire::DhcpOp,
    out: *mut DhcpMessageReport,
) {
    let mut r = DhcpMessageReport::EMPTY;
    let active = {
        let _guard = Guard::acquire(2);
        pending && unsafe { (*STATE.0.get()).expectation.expected != 0 }
    };
    if !active {
        r.action = Action::Inactive as u8;
        unsafe {
            out.write(r);
        }
        return;
    }
    let (report, op) = unsafe { readers::dhcp(data, len) };
    r.dispatch_failures = report.dispatch_failures;
    r.reason = report.reason;
    if let Some(op) = op {
        let action = if op.result != wire::DHCP_RESULT_OK {
            Action::Malformed
        } else {
            let _guard = Guard::acquire(2);
            unsafe {
                let s = &mut *STATE.0.get();
                let e = s.expectation;
                if op.xid != e.xid || op.mac != e.mac {
                    Action::Foreign
                } else if !expected(e, op.flags) {
                    Action::OutOfPhase
                } else if op.flags & wire::DHCP_FLAG_OFFER != 0 && s.drop_offers != 0 {
                    s.drop_offers -= 1;
                    r.remaining = s.drop_offers;
                    Action::DropOffer
                } else if op.flags & wire::DHCP_FLAG_ACK != 0 && s.drop_acks != 0 {
                    s.drop_acks -= 1;
                    r.remaining = s.drop_acks;
                    Action::DropAck
                } else {
                    Action::Accept
                }
            }
        };
        r.action = action as u8;
        unsafe {
            operation.write(op);
        }
    }
    unsafe {
        out.write(r);
    }
}
/// # Safety
/// Exclusive paired stack statistics/report. The wire record is initialized
/// for every action that reads it; no record read occurs for failure/inactive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_message_apply(
    stats: *mut dhcp::Stats,
    op: *const wire::DhcpOp,
    report: *const DhcpMessageReport,
) -> bool {
    unsafe {
        let s = &mut *stats;
        let r = *report;
        if r.reason.len != 0 {
            s.last_error = r.reason;
        }
        match r.action {
            x if x == Action::WireFailure as u8 => false,
            x if x == Action::Inactive as u8 => {
                s.inactive_rx = s.inactive_rx.wrapping_add(1);
                false
            }
            x if x == Action::Malformed as u8 => {
                dhcp::apply_rx_result(s, &*op);
                false
            }
            x if x == Action::Foreign as u8 => {
                s.foreign_rx = s.foreign_rx.wrapping_add(1);
                false
            }
            x if x == Action::OutOfPhase as u8 => {
                s.out_of_phase_rx = s.out_of_phase_rx.wrapping_add(1);
                false
            }
            x if x == Action::DropOffer as u8 => {
                s.last_error = Text::literal(b"test-drop-offer");
                false
            }
            x if x == Action::DropAck as u8 => {
                s.last_error = Text::literal(b"test-drop-ack");
                false
            }
            x if x == Action::Accept as u8 => {
                dhcp::apply_rx_result(s, &*op);
                true
            }
            _ => panic!("Exact DHCP message action"),
        }
    }
}
/// # Safety
/// Readable exact report after the caller has published its statistics effect.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dhcp_message_log(report: *const DhcpMessageReport) {
    let r = unsafe { *report };
    if r.action == Action::DropOffer as u8 {
        foundation::puts(b"DHCP05913 inject=drop-offer remaining=");
    } else if r.action == Action::DropAck as u8 {
        foundation::puts(b"DHCP05913 inject=drop-ack remaining=");
    } else {
        return;
    }
    foundation::dec(u64::from(r.remaining));
    foundation::puts(b"\r\n");
}

const _: () = {
    assert!(core::mem::size_of::<DhcpExpectation>() == 16);
    assert!(core::mem::align_of::<DhcpExpectation>() == 4);
    assert!(core::mem::offset_of!(DhcpExpectation, expected) == 0);
    assert!(core::mem::offset_of!(DhcpExpectation, xid) == 4);
    assert!(core::mem::offset_of!(DhcpExpectation, mac) == 8);
};
const _: () = {
    assert!(core::mem::size_of::<DhcpMessageReport>() == 32);
    assert!(core::mem::align_of::<DhcpMessageReport>() == 8);
    assert!(core::mem::offset_of!(DhcpMessageReport, dispatch_failures) == 0);
    assert!(core::mem::offset_of!(DhcpMessageReport, reason) == 8);
    assert!(core::mem::offset_of!(DhcpMessageReport, action) == 24);
    assert!(core::mem::offset_of!(DhcpMessageReport, remaining) == 25);
};
