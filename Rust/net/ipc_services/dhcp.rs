//! Exact dhcp operations of the parent owner; no duplicated state.
use super::*;

pub(super) fn write_dhcp_status(w: &mut Writer<'_>) {
    let s = net::dhcp_stats();
    let timing = net::dhcp_lease_timing();
    w.text(b"state=".as_slice());
    w.text(if s.lease.bound {
        b"bound".as_slice()
    } else {
        b"released".as_slice()
    });
    w.text(b" pending=".as_slice());
    w.text(if s.operation_pending {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" op=".as_slice());
    w.text(s.pending_label);
    w.text(b" ip=".as_slice());
    w.ip(s.lease.offered_ip);
    w.text(b" server=".as_slice());
    w.ip(s.lease.server_ip);
    w.text(b" lease=".as_slice());
    w.num(s.lease.lease_seconds);
    w.text(b" elapsed=".as_slice());
    w.num(timing.elapsed_seconds);
    w.text(b" remaining=".as_slice());
    w.num(timing.remaining_seconds);
    w.text(b" renew_in=".as_slice());
    w.num(timing.renew_in_seconds);
    w.text(b" rebind_in=".as_slice());
    w.num(timing.rebind_in_seconds);
    w.text(b" discover=".as_slice());
    w.num(s.discover_tx);
    w.text(b" offer=".as_slice());
    w.num(s.offer_rx);
    w.text(b" request=".as_slice());
    w.num(s.request_tx);
    w.text(b" ack=".as_slice());
    w.num(s.ack_rx);
    w.text(b" nak=".as_slice());
    w.num(s.nak_rx);
    w.text(b" release=".as_slice());
    w.num(s.release_tx);
    w.text(b" retry=".as_slice());
    w.num(s.retries);
    w.text(b" timeout=".as_slice());
    w.num(s.timeouts);
    w.text(b" relerr=".as_slice());
    w.num(s.release_errors);
    w.text(b" malformed=".as_slice());
    w.num(s.malformed);
    w.text(b" inactive=".as_slice());
    w.num(s.inactive_rx);
    w.text(b" foreign=".as_slice());
    w.num(s.foreign_rx);
    w.text(b" phase=".as_slice());
    w.num(s.out_of_phase_rx);
    w.text(b" attempt=".as_slice());
    w.num(s.last_attempt);
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}

pub(super) fn make_dhcp_status_result() -> DhcpServiceStatus {
    let s = net::dhcp_stats();
    let timing = net::dhcp_lease_timing();
    let runtime = net::dhcp_runtime_status();
    let mut flags: u32 = 0;
    if s.lease.bound {
        flags |= DHCP_FLAG_BOUND;
    }
    if s.operation_pending {
        flags |= DHCP_FLAG_PENDING;
    }
    if s.lease.dns_configured {
        flags |= DHCP_FLAG_DNS_CONFIGURED;
    }
    if runtime.desired_dhcp {
        flags |= DHCP_FLAG_DESIRED;
    }
    if runtime.task_started {
        flags |= DHCP_FLAG_TASK_STARTED;
    }
    if runtime.link_up {
        flags |= DHCP_FLAG_LINK_UP;
    }
    if runtime.state == super::super::dhcp_runtime::State::RetryWait {
        flags |= DHCP_FLAG_RETRY_WAIT;
    }
    let mut out = DhcpServiceStatus {
        runtime_state: runtime.state as u16,
        flags: int(flags),
        xid: int(s.lease.xid),
        offered_ip: s.lease.offered_ip,
        server_ip: s.lease.server_ip,
        netmask: s.lease.netmask,
        gateway_ip: s.lease.gateway_ip,
        dns_ip: s.lease.dns_ip,
        lease_seconds: int(s.lease.lease_seconds),
        renew_seconds: int(s.lease.renew_seconds),
        rebind_seconds: int(s.lease.rebind_seconds),
        elapsed_seconds: int(timing.elapsed_seconds),
        remaining_seconds: int(timing.remaining_seconds),
        renew_in_seconds: int(timing.renew_in_seconds),
        rebind_in_seconds: int(timing.rebind_in_seconds),
        last_attempt: int(s.last_attempt),
        last_type: int(s.last_type),
        discover_tx: int(s.discover_tx),
        offer_rx: int(s.offer_rx),
        request_tx: int(s.request_tx),
        ack_rx: int(s.ack_rx),
        nak_rx: int(s.nak_rx),
        release_tx: int(s.release_tx),
        retries: int(s.retries),
        timeouts: int(s.timeouts),
        release_errors: int(s.release_errors),
        malformed: int(s.malformed),
        self_tests: int(s.self_tests),
        ..DhcpServiceStatus::EMPTY
    };
    copy_bounded(&mut out.pending_label[0..], s.pending_label);
    copy_bounded(&mut out.last_error[0..], s.last_error);
    out
}

pub(super) fn write_dhcp_operation(w: &mut Writer<'_>, op: u16, payload: &[u8]) {
    let request = split_service_deadline(payload);
    let result = match op {
        OP_DHCP_ACQUIRE => net::dhcp_acquire_default_until(request.deadline_tick),
        OP_DHCP_RENEW => net::dhcp_renew_default_until(request.deadline_tick),
        OP_DHCP_RELEASE => net::dhcp_release_default(),
        _ => unreachable!(),
    };
    let s = net::dhcp_stats();
    let timing = net::dhcp_lease_timing();
    w.text(b"action=".as_slice());
    w.text(match op {
        OP_DHCP_ACQUIRE => b"acquire".as_slice(),
        OP_DHCP_RENEW => b"renew".as_slice(),
        OP_DHCP_RELEASE => b"release".as_slice(),
        _ => b"unknown".as_slice(),
    });
    w.text(b" result=".as_slice());
    w.text(net::tx_result_name(result));
    w.text(b" code=".as_slice());
    w.signed(net_tx_result_code(result));
    w.text(b" state=".as_slice());
    w.text(if s.lease.bound {
        b"bound".as_slice()
    } else {
        b"released".as_slice()
    });
    w.text(b" pending=".as_slice());
    w.text(if s.operation_pending {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" ip=".as_slice());
    w.ip(s.lease.offered_ip);
    w.text(b" server=".as_slice());
    w.ip(s.lease.server_ip);
    w.text(b" remaining=".as_slice());
    w.num(timing.remaining_seconds);
    w.text(b" renew_in=".as_slice());
    w.num(timing.renew_in_seconds);
    w.text(b" rebind_in=".as_slice());
    w.num(timing.rebind_in_seconds);
    w.text(b" timeout=".as_slice());
    w.num(s.timeouts);
    w.text(b" retry=".as_slice());
    w.num(s.retries);
    w.text(b" relerr=".as_slice());
    w.num(s.release_errors);
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}

pub(super) fn make_dhcp_operation_result(op: u16, payload: &[u8]) -> DhcpServiceResult {
    let request = split_service_deadline(payload);
    let tx = match op {
        OP_DHCP_ACQUIRE_RESULT => net::dhcp_acquire_default_until(request.deadline_tick),
        OP_DHCP_RENEW_RESULT => net::dhcp_renew_default_until(request.deadline_tick),
        OP_DHCP_RELEASE_RESULT => net::dhcp_release_default(),
        _ => unreachable!(),
    };
    let s = net::dhcp_stats();
    let timing = net::dhcp_lease_timing();
    let runtime = net::dhcp_runtime_status();
    let mut flags: u32 = 0;
    if s.lease.bound {
        flags |= DHCP_FLAG_BOUND;
    }
    if s.operation_pending {
        flags |= DHCP_FLAG_PENDING;
    }
    if s.lease.dns_configured {
        flags |= DHCP_FLAG_DNS_CONFIGURED;
    }
    if runtime.desired_dhcp {
        flags |= DHCP_FLAG_DESIRED;
    }
    if runtime.task_started {
        flags |= DHCP_FLAG_TASK_STARTED;
    }
    if runtime.link_up {
        flags |= DHCP_FLAG_LINK_UP;
    }
    if runtime.state == super::super::dhcp_runtime::State::RetryWait {
        flags |= DHCP_FLAG_RETRY_WAIT;
    }
    flags = with_service_status(flags, service_status_from_tx_result(tx, s.last_error));
    DhcpServiceResult {
        action: int(match op {
            OP_DHCP_ACQUIRE_RESULT => DHCP_ACTION_ACQUIRE,
            OP_DHCP_RENEW_RESULT => DHCP_ACTION_RENEW,
            OP_DHCP_RELEASE_RESULT => DHCP_ACTION_RELEASE,
            _ => 0,
        }),
        result: int(net_tx_result_code(tx)),
        flags: int(flags),
        offered_ip: s.lease.offered_ip,
        server_ip: s.lease.server_ip,
        netmask: s.lease.netmask,
        gateway_ip: s.lease.gateway_ip,
        dns_ip: s.lease.dns_ip,
        lease_seconds: int(s.lease.lease_seconds),
        elapsed_seconds: int(timing.elapsed_seconds),
        remaining_seconds: int(timing.remaining_seconds),
        renew_in_seconds: int(timing.renew_in_seconds),
        rebind_in_seconds: int(timing.rebind_in_seconds),
        last_attempt: int(s.last_attempt),
        last_type: int(s.last_type),
        discover_tx: int(s.discover_tx),
        offer_rx: int(s.offer_rx),
        request_tx: int(s.request_tx),
        ack_rx: int(s.ack_rx),
        nak_rx: int(s.nak_rx),
        release_tx: int(s.release_tx),
        retries: int(s.retries),
        timeouts: int(s.timeouts),
        release_errors: int(s.release_errors),
        malformed: int(s.malformed),
        ..DhcpServiceResult::EMPTY
    }
}
