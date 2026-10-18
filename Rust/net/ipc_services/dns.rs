//! Exact dns operations of the parent owner; no duplicated state.
use super::*;

pub(super) fn service_status_from_dns_result(result: i32) -> Status {
    match result {
        super::super::dns::RESULT_OK => Status::Ok,
        super::super::dns::RESULT_TIMEOUT => Status::Timeout,
        _ => Status::Failed,
    }
}

pub(super) fn write_dns_status(w: &mut Writer<'_>) {
    let s = net::dns_stats();
    let cache = net::dns_cache_status();
    w.text(b"pending=".as_slice());
    w.text(if s.operation_pending {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" name=".as_slice());
    w.text(if s.operation_pending {
        &s.pending_name[0..]
    } else {
        &s.last_name[0..]
    });
    w.text(b" queries=".as_slice());
    w.num(s.queries_tx);
    w.text(b" resolve=".as_slice());
    w.num(s.resolve_requests);
    w.text(b" responses=".as_slice());
    w.num(s.responses_rx);
    w.text(b" a=".as_slice());
    w.num(s.a_records);
    w.text(b" timeout=".as_slice());
    w.num(s.timeouts);
    w.text(b" nxdomain=".as_slice());
    w.num(s.nxdomain);
    w.text(b" txerr=".as_slice());
    w.num(s.tx_errors);
    w.text(b" malformed=".as_slice());
    w.num(s.malformed);
    w.text(b" server=".as_slice());
    w.ip(s.last_server);
    w.text(b" id=".as_slice());
    w.num(s.last_id);
    w.text(b" answer=".as_slice());
    w.ip(s.last_answer);
    w.text(b" result=".as_slice());
    w.signed(s.last_result);
    w.text(b" cache=".as_slice());
    w.text(if cache.valid {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" cache_name=".as_slice());
    w.text(&s.cache_name[0..]);
    w.text(b" cache_answer=".as_slice());
    w.ip(s.cache_answer);
    w.text(b" cache_hits=".as_slice());
    w.num(s.cache_hits);
    w.text(b" cache_stores=".as_slice());
    w.num(s.cache_stores);
    w.text(b" cache_age=".as_slice());
    w.num(cache.age_seconds);
    w.text(b"/".as_slice());
    w.num(cache.ttl_seconds);
    w.text(b" cache_remaining=".as_slice());
    w.num(cache.remaining_seconds);
    w.text(b" cache_entries=".as_slice());
    w.num(net::dns_cache_entry_count());
    w.text(b" cache_negative=".as_slice());
    w.num(net::dns_cache_negative_count());
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}

pub(super) fn make_dns_status_result() -> DnsServiceStatus {
    let s = net::dns_stats();
    let cache = net::dns_cache_status();
    let mut flags: u32 = 0;
    if s.last_result == 0 && (s.responses_rx != 0 || s.a_records != 0) {
        flags |= DNS_FLAG_OK;
    }
    if s.operation_pending {
        flags |= DNS_FLAG_PENDING;
    }
    if cache.valid {
        flags |= DNS_FLAG_CACHE_VALID;
    }
    let mut out = DnsServiceStatus {
        flags: int(flags),
        last_result: int(s.last_result),
        last_id: int(s.last_id),
        last_server: s.last_server,
        last_answer: s.last_answer,
        cache_server: s.cache_server,
        cache_answer: s.cache_answer,
        cache_age_seconds: int(cache.age_seconds),
        cache_ttl_seconds: int(cache.ttl_seconds),
        cache_remaining_seconds: int(cache.remaining_seconds),
        queries_tx: int(s.queries_tx),
        resolve_requests: int(s.resolve_requests),
        responses_rx: int(s.responses_rx),
        a_records: int(s.a_records),
        timeouts: int(s.timeouts),
        nxdomain: int(s.nxdomain),
        tx_errors: int(s.tx_errors),
        malformed: int(s.malformed),
        self_tests: int(s.self_tests),
        cache_hits: int(s.cache_hits),
        cache_stores: int(s.cache_stores),
        ..DnsServiceStatus::EMPTY
    };
    copy_bounded(&mut out.name[0..], &s.last_name[0..]);
    out.name_len = int(string_len_z(&out.name[0..]));
    copy_bounded(&mut out.pending_name[0..], &s.pending_name[0..]);
    out.pending_name_len = int(string_len_z(&out.pending_name[0..]));
    copy_bounded(&mut out.cache_name[0..], &s.cache_name[0..]);
    out.cache_name_len = int(string_len_z(&out.cache_name[0..]));
    copy_bounded(&mut out.last_error[0..], s.last_error);
    out
}

pub(super) fn write_dns_resolve(w: &mut Writer<'_>, op: u16, payload: &[u8]) {
    let mut server: [u8; 4] = [0; 4];
    let mut name = payload;
    let explicit_server = op == OP_DNS_RESOLVE_A_SERVER;
    if explicit_server {
        if payload.len() < 5 {
            w.text(b"name= result=name code=-9".as_slice());
            w.text(b" ip=0.0.0.0 server=0.0.0.0 pending=no cache=no cache_hits=0 cache_stores=0 timeout=0 last=bad-request".as_slice());
            return;
        }
        server = [payload[0], payload[1], payload[2], payload[3]];
        name = &payload[4..];
    }
    let mut answer: [u8; 4] = [0; 4];
    let result = if explicit_server {
        net::dns_resolve_a_with_server(name, server, &mut answer)
    } else {
        net::dns_resolve_a(name, &mut answer)
    };
    w.text(b"name=".as_slice());
    w.text(name);
    w.text(b" result=".as_slice());
    w.text(net::dns_result_name(result));
    w.text(b" code=".as_slice());
    w.signed(result);
    w.text(b" ip=".as_slice());
    w.ip(answer);
    let s = net::dns_stats();
    let cache = net::dns_cache_status();
    w.text(b" server=".as_slice());
    w.ip(if explicit_server {
        server
    } else {
        s.last_server
    });
    w.text(b" pending=".as_slice());
    w.text(if s.operation_pending {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" cache=".as_slice());
    w.text(if cache.valid {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" cache_hits=".as_slice());
    w.num(s.cache_hits);
    w.text(b" cache_stores=".as_slice());
    w.num(s.cache_stores);
    w.text(b" timeout=".as_slice());
    w.num(s.timeouts);
    w.text(b" cache_entries=".as_slice());
    w.num(net::dns_cache_entry_count());
    w.text(b" cache_negative=".as_slice());
    w.num(net::dns_cache_negative_count());
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}

pub(super) fn make_dns_resolve_result(op: u16, payload: &[u8]) -> DnsServiceResult {
    let mut server: [u8; 4] = [0; 4];
    let mut name = payload;
    let explicit_server = op == OP_DNS_RESOLVE_A_SERVER_RESULT;
    if explicit_server {
        if payload.len() < 5 {
            return dns_bad_request_result(op, b"".as_slice());
        }
        server = [payload[0], payload[1], payload[2], payload[3]];
        name = &payload[4..];
    }
    let mut answer: [u8; 4] = [0; 4];
    let before = net::dns_stats();
    let result = if explicit_server {
        net::dns_resolve_a_with_server(name, server, &mut answer)
    } else {
        net::dns_resolve_a(name, &mut answer)
    };
    let after = net::dns_stats();
    let cache = net::dns_cache_status();
    let mut flags: u32 = 0;
    if result == 0 {
        flags |= DNS_FLAG_OK;
    }
    if after.operation_pending {
        flags |= DNS_FLAG_PENDING;
    }
    if cache.valid {
        flags |= DNS_FLAG_CACHE_VALID;
    }
    if after.cache_hits > before.cache_hits {
        flags |= DNS_FLAG_CACHE_HIT;
    }
    if explicit_server {
        flags |= DNS_FLAG_EXPLICIT_SERVER;
    }
    flags = with_service_status(flags, service_status_from_dns_result(result));
    let mut out = DnsServiceResult {
        action: int(if explicit_server {
            DNS_ACTION_RESOLVE_A_SERVER
        } else {
            DNS_ACTION_RESOLVE_A
        }),
        result: int(result),
        flags: int(flags),
        answer,
        server: if explicit_server {
            server
        } else {
            after.last_server
        },
        cache_answer: after.cache_answer,
        cache_age_seconds: int(cache.age_seconds),
        cache_ttl_seconds: int(cache.ttl_seconds),
        cache_remaining_seconds: int(cache.remaining_seconds),
        queries_tx: int(after.queries_tx),
        resolve_requests: int(after.resolve_requests),
        responses_rx: int(after.responses_rx),
        a_records: int(after.a_records),
        timeouts: int(after.timeouts),
        nxdomain: int(after.nxdomain),
        tx_errors: int(after.tx_errors),
        malformed: int(after.malformed),
        cache_hits: int(after.cache_hits),
        cache_stores: int(after.cache_stores),
        last_id: int(after.last_id),
        ..DnsServiceResult::EMPTY
    };
    copy_bounded(&mut out.name[0..], name);
    out.name_len = int(core::cmp::min(name.len(), out.name.len()));
    copy_bounded(&mut out.last_error[0..], after.last_error);
    out
}

pub(super) fn dns_bad_request_result(op: u16, name: &[u8]) -> DnsServiceResult {
    let mut out = DnsServiceResult {
        action: int(if op == OP_DNS_RESOLVE_A_SERVER_RESULT {
            DNS_ACTION_RESOLVE_A_SERVER
        } else {
            DNS_ACTION_RESOLVE_A
        }),
        result: int(-9),
        flags: int(with_service_status(
            if op == OP_DNS_RESOLVE_A_SERVER_RESULT {
                DNS_FLAG_EXPLICIT_SERVER
            } else {
                0
            },
            Status::Failed,
        )),
        ..DnsServiceResult::EMPTY
    };
    copy_bounded(&mut out.name[0..], name);
    out.name_len = int(core::cmp::min(name.len(), out.name.len()));
    copy_bounded(&mut out.last_error[0..], b"bad-request".as_slice());
    out
}
