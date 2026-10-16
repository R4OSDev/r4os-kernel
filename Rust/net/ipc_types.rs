//! Existing NIPC constants and fixed wire records. Encoding writes fields into
//! zeroed bytes; Rust struct padding is never read or exported as initialized data.
use super::tcp;
use crate::service::ipc;
pub trait Wire {
    const SIZE: usize;
    fn encode(&self, out: &mut [u8]) -> usize;
}
pub const MAGIC: u32 = 0x4350494E;
pub const VERSION: u16 = 1;
pub const OP_STATUS: u16 = 1;
pub const OP_SERVICE_RESTART: u16 = 0x00F1;
pub const OP_DHCP_STATUS_RESULT: u16 = 0x0210;
pub const OP_DHCP_ACQUIRE: u16 = 0x0201;
pub const OP_DHCP_RENEW: u16 = 0x0202;
pub const OP_DHCP_RELEASE: u16 = 0x0203;
pub const OP_DHCP_ACQUIRE_RESULT: u16 = 0x0211;
pub const OP_DHCP_RENEW_RESULT: u16 = 0x0212;
pub const OP_DHCP_RELEASE_RESULT: u16 = 0x0213;
pub const OP_DNS_RESOLVE_A: u16 = 0x0101;
pub const OP_DNS_RESOLVE_A_SERVER: u16 = 0x0102;
pub const OP_DNS_STATUS_RESULT: u16 = 0x0110;
pub const OP_DNS_RESOLVE_A_RESULT: u16 = 0x0111;
pub const OP_DNS_RESOLVE_A_SERVER_RESULT: u16 = 0x0112;
pub const OP_TCP_CONNECT: u16 = 0x0301;
pub const OP_TCP_WRITE: u16 = 0x0302;
pub const OP_TCP_READ: u16 = 0x0303;
pub const OP_TCP_CLOSE: u16 = 0x0304;
pub const OP_TCP_LISTEN: u16 = 0x0305;
pub const OP_TCP_ACCEPT_READ: u16 = 0x0306;
pub const OP_TCP_CLOSE_LISTEN: u16 = 0x0307;
pub const OP_TCP_CONNECTIONS: u16 = 0x0308;
pub const OP_TCP_POLL: u16 = 0x0309;
pub const OP_TCP_ACCEPT: u16 = 0x030A;
pub const OP_TCP_STATUS_RESULT: u16 = 0x0310;
pub const OP_TCP_CONNECT_RESULT: u16 = 0x0311;
pub const OP_TCP_WRITE_RESULT: u16 = 0x0312;
pub const OP_TCP_READ_RESULT: u16 = 0x0313;
pub const OP_TCP_CLOSE_RESULT: u16 = 0x0314;
pub const OP_TCP_LISTEN_RESULT: u16 = 0x0315;
pub const OP_TCP_ACCEPT_READ_RESULT: u16 = 0x0316;
pub const OP_TCP_CLOSE_LISTEN_RESULT: u16 = 0x0317;
pub const OP_TCP_POLL_RESULT: u16 = 0x0318;
pub const OP_TCP_ACCEPT_RESULT: u16 = 0x0319;
pub const OP_TCP_ABORT_RESULT: u16 = 0x031A;
pub const OP_TCP_ACCEPT_POLL_RESULT: u16 = 0x031B;
pub const OP_TCP_RETRANSMIT_RESULT: u16 = 0x031C;
pub const OP_UDP_BIND: u16 = 0x0401;
pub const OP_UDP_SENDTO: u16 = 0x0402;
pub const OP_UDP_RECV: u16 = 0x0403;
pub const OP_UDP_CLOSE: u16 = 0x0404;
pub const OP_UDP_STATUS_RESULT: u16 = 0x0410;
pub const OP_UDP_BIND_RESULT: u16 = 0x0411;
pub const OP_UDP_SENDTO_RESULT: u16 = 0x0412;
pub const OP_UDP_RECV_RESULT: u16 = 0x0413;
pub const OP_UDP_CLOSE_RESULT: u16 = 0x0414;
pub const RESULT_OK: i32 = 0;
pub const RESULT_BAD_REQUEST: i32 = -1;
pub const RESULT_BAD_SERVICE: i32 = -2;
pub const RESULT_BAD_OP: i32 = -3;
pub const HEADER_SIZE: usize = 24;
pub const TCP_SERVICE_HANDLES: usize = 16;
pub const TCP_STALE_HANDLE_TOMBSTONES: usize = 16;
pub const TCP_MESSAGE_PAYLOAD_MAX: usize = ipc::MAX_MESSAGE_SIZE - HEADER_SIZE;
pub const TCP_WRITE_PAYLOAD_MAX: usize = TCP_MESSAGE_PAYLOAD_MAX - 4;
pub const TCP_TEXT_METADATA_MAX: usize = 256;
pub const TCP_TEXT_DATA_MAX: usize = TCP_MESSAGE_PAYLOAD_MAX - TCP_TEXT_METADATA_MAX;
pub const DHCP_STATUS_MAGIC: u32 = 0x53504844;
pub const DHCP_STATUS_VERSION: u16 = 1;
pub const DHCP_RESULT_MAGIC: u32 = 0x50514844;
pub const DHCP_RESULT_VERSION: u16 = 1;
pub const DNS_STATUS_MAGIC: u32 = 0x53534844;
pub const DNS_STATUS_VERSION: u16 = 1;
pub const DNS_RESULT_MAGIC: u32 = 0x53524444;
pub const DNS_RESULT_VERSION: u16 = 1;
pub const TCP_STATUS_MAGIC: u32 = 0x53544354;
pub const TCP_STATUS_VERSION: u16 = 2;
pub const TCP_RESULT_MAGIC: u32 = 0x53524354;
pub const TCP_RESULT_VERSION: u16 = 2;
pub const UDP_STATUS_MAGIC: u32 = 0x53504455;
pub const UDP_STATUS_VERSION: u16 = 2;
pub const UDP_RESULT_MAGIC: u32 = 0x52504455;
pub const UDP_RESULT_VERSION: u16 = 2;
pub const DHCP_ACTION_ACQUIRE: u16 = 1;
pub const DHCP_ACTION_RENEW: u16 = 2;
pub const DHCP_ACTION_RELEASE: u16 = 3;
pub const DNS_ACTION_RESOLVE_A: u16 = 1;
pub const DNS_ACTION_RESOLVE_A_SERVER: u16 = 2;
pub const TCP_ACTION_CONNECT: u16 = 1;
pub const TCP_ACTION_WRITE: u16 = 2;
pub const TCP_ACTION_READ: u16 = 3;
pub const TCP_ACTION_CLOSE: u16 = 4;
pub const TCP_ACTION_LISTEN: u16 = 5;
pub const TCP_ACTION_ACCEPT_READ: u16 = 6;
pub const TCP_ACTION_CLOSE_LISTEN: u16 = 7;
pub const TCP_ACTION_POLL: u16 = 8;
pub const TCP_ACTION_ACCEPT: u16 = 9;
pub const TCP_ACTION_ABORT: u16 = 10;
pub const TCP_ACTION_ACCEPT_POLL: u16 = 11;
pub const TCP_ACTION_RETRANSMIT: u16 = 12;
pub const UDP_ACTION_BIND: u16 = 1;
pub const UDP_ACTION_SENDTO: u16 = 2;
pub const UDP_ACTION_RECV: u16 = 3;
pub const UDP_ACTION_CLOSE: u16 = 4;
pub const DHCP_FLAG_BOUND: u32 = 1 << 0;
pub const DHCP_FLAG_PENDING: u32 = 1 << 1;
pub const DHCP_FLAG_DNS_CONFIGURED: u32 = 1 << 2;
pub const DHCP_FLAG_DESIRED: u32 = 1 << 3;
pub const DHCP_FLAG_TASK_STARTED: u32 = 1 << 4;
pub const DHCP_FLAG_LINK_UP: u32 = 1 << 5;
pub const DHCP_FLAG_RETRY_WAIT: u32 = 1 << 6;
pub const DNS_FLAG_OK: u32 = 1 << 0;
pub const DNS_FLAG_PENDING: u32 = 1 << 1;
pub const DNS_FLAG_CACHE_VALID: u32 = 1 << 2;
pub const DNS_FLAG_CACHE_HIT: u32 = 1 << 3;
pub const DNS_FLAG_EXPLICIT_SERVER: u32 = 1 << 4;
pub const DNS_FLAG_CACHE_VALID_PUBLIC: u32 = DNS_FLAG_CACHE_VALID;
pub const DNS_FLAG_CACHE_HIT_PUBLIC: u32 = DNS_FLAG_CACHE_HIT;
pub const TCP_FLAG_OK: u32 = 1 << 0;
pub const TCP_FLAG_TIMEOUT: u32 = 1 << 1;
pub const TCP_FLAG_DATA: u32 = 1 << 2;
pub const TCP_FLAG_HANDLE_VALID: u32 = 1 << 3;
pub const TCP_FLAG_CONN_VALID: u32 = 1 << 4;
pub const TCP_FLAG_REMOTE_VALID: u32 = 1 << 5;
pub const TCP_FLAG_LISTENER: u32 = 1 << 6;
pub const TCP_FLAG_LIFECYCLE_VALID: u32 = 1 << 7;
pub const TCP_STATUS_FLAG_LISTENER_ACTIVE: u32 = 1 << 0;
pub const TCP_STATUS_FLAG_LAST_SEGMENT: u32 = 1 << 1;
pub const TCP_STATUS_FLAG_LIFECYCLE_VALID: u32 = 1 << 2;
pub const UDP_FLAG_OK: u32 = 1 << 0;
pub const UDP_FLAG_DATA: u32 = 1 << 1;
pub const UDP_FLAG_HANDLE_VALID: u32 = 1 << 2;
pub const UDP_FLAG_REMOTE_VALID: u32 = 1 << 3;
pub const UDP_FLAG_LIFECYCLE_VALID: u32 = 1 << 4;
pub const UDP_STATUS_FLAG_LIFECYCLE_VALID: u32 = UDP_FLAG_LIFECYCLE_VALID;
pub const SERVICE_STATUS_SHIFT: u8 = 24;
pub const SERVICE_STATUS_MASK: u32 = 0xFF000000;
pub const SOCKET_LIFECYCLE_UNKNOWN_PUBLIC: u32 = 0;
pub const SOCKET_LIFECYCLE_ACTIVE_PUBLIC: u32 = 1;
pub const SOCKET_LIFECYCLE_CLOSED_PUBLIC: u32 = 2;
pub const SOCKET_LIFECYCLE_RESET_PUBLIC: u32 = 3;
pub const SOCKET_LIFECYCLE_TIMEOUT_PUBLIC: u32 = 4;
pub const SOCKET_LIFECYCLE_PEER_GONE_PUBLIC: u32 = 5;
pub const SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC: u32 = 6;
pub const SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC: u32 = 7;
pub const SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC: u32 = 8;
pub const SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC: u32 = 9;
pub const SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC: u32 = 10;
pub const SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC: u32 = 11;
pub const SOCKET_LIFECYCLE_LISTENER_PUBLIC: u32 = 12;
pub const SOCKET_LIFECYCLE_DROPPED_PUBLIC: u32 = 13;
pub const TCP_FLAG_TIMEOUT_PUBLIC: u32 = TCP_FLAG_TIMEOUT;
pub const TCP_FLAG_DATA_PUBLIC: u32 = TCP_FLAG_DATA;
pub const TCP_FLAG_HANDLE_VALID_PUBLIC: u32 = TCP_FLAG_HANDLE_VALID;
pub const TCP_FLAG_CONN_VALID_PUBLIC: u32 = TCP_FLAG_CONN_VALID;
pub const TCP_FLAG_LIFECYCLE_VALID_PUBLIC: u32 = TCP_FLAG_LIFECYCLE_VALID;
pub const TCP_STATUS_FLAG_LISTENER_ACTIVE_PUBLIC: u32 = TCP_STATUS_FLAG_LISTENER_ACTIVE;
pub const TCP_STATUS_FLAG_LAST_SEGMENT_PUBLIC: u32 = TCP_STATUS_FLAG_LAST_SEGMENT;
pub const TCP_STATUS_FLAG_LIFECYCLE_VALID_PUBLIC: u32 = TCP_STATUS_FLAG_LIFECYCLE_VALID;
pub const UDP_FLAG_DATA_PUBLIC: u32 = UDP_FLAG_DATA;
pub const UDP_FLAG_HANDLE_VALID_PUBLIC: u32 = UDP_FLAG_HANDLE_VALID;
pub const UDP_FLAG_REMOTE_VALID_PUBLIC: u32 = UDP_FLAG_REMOTE_VALID;
pub const UDP_FLAG_LIFECYCLE_VALID_PUBLIC: u32 = UDP_FLAG_LIFECYCLE_VALID;
pub const UDP_STATUS_FLAG_LIFECYCLE_VALID_PUBLIC: u32 = UDP_STATUS_FLAG_LIFECYCLE_VALID;
pub const SERVICE_STATUS_MASK_PUBLIC: u32 = SERVICE_STATUS_MASK;
pub const TCP_RESULT_DATA_MAX: usize =
    TCP_MESSAGE_PAYLOAD_MAX - core::mem::size_of::<TcpServiceResult>();
pub const UDP_SEND_PAYLOAD_MAX: usize = TCP_MESSAGE_PAYLOAD_MAX - 10;
pub const UDP_RESULT_DATA_MAX: usize =
    TCP_MESSAGE_PAYLOAD_MAX - core::mem::size_of::<UdpServiceResult>();

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DhcpServiceStatus {
    pub magic: u32,
    pub version: u16,
    pub runtime_state: u16,
    pub flags: u32,
    pub xid: u32,
    pub offered_ip: [u8; 4],
    pub server_ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub dns_ip: [u8; 4],
    pub lease_seconds: u32,
    pub renew_seconds: u32,
    pub rebind_seconds: u32,
    pub elapsed_seconds: u32,
    pub remaining_seconds: u32,
    pub renew_in_seconds: u32,
    pub rebind_in_seconds: u32,
    pub last_attempt: u32,
    pub last_type: u32,
    pub discover_tx: u64,
    pub offer_rx: u64,
    pub request_tx: u64,
    pub ack_rx: u64,
    pub nak_rx: u64,
    pub release_tx: u64,
    pub retries: u64,
    pub timeouts: u64,
    pub release_errors: u64,
    pub malformed: u64,
    pub self_tests: u64,
    pub pending_label: [u8; 16],
    pub last_error: [u8; 32],
}
impl DhcpServiceStatus {
    pub const EMPTY: Self = Self {
        magic: DHCP_STATUS_MAGIC,
        version: DHCP_STATUS_VERSION,
        runtime_state: 0,
        flags: 0,
        xid: 0,
        offered_ip: [0; 4],
        server_ip: [0; 4],
        netmask: [0; 4],
        gateway_ip: [0; 4],
        dns_ip: [0; 4],
        lease_seconds: 0,
        renew_seconds: 0,
        rebind_seconds: 0,
        elapsed_seconds: 0,
        remaining_seconds: 0,
        renew_in_seconds: 0,
        rebind_in_seconds: 0,
        last_attempt: 0,
        last_type: 0,
        discover_tx: 0,
        offer_rx: 0,
        request_tx: 0,
        ack_rx: 0,
        nak_rx: 0,
        release_tx: 0,
        retries: 0,
        timeouts: 0,
        release_errors: 0,
        malformed: 0,
        self_tests: 0,
        pending_label: [0; 16],
        last_error: [0; 32],
    };
}
const _: () = {
    assert!(core::mem::size_of::<DhcpServiceStatus>() == 208);
    assert!(core::mem::align_of::<DhcpServiceStatus>() == 8);
    assert!(core::mem::offset_of!(DhcpServiceStatus, magic) == 0);
    assert!(core::mem::offset_of!(DhcpServiceStatus, version) == 4);
    assert!(core::mem::offset_of!(DhcpServiceStatus, runtime_state) == 6);
    assert!(core::mem::offset_of!(DhcpServiceStatus, flags) == 8);
    assert!(core::mem::offset_of!(DhcpServiceStatus, xid) == 12);
    assert!(core::mem::offset_of!(DhcpServiceStatus, offered_ip) == 16);
    assert!(core::mem::offset_of!(DhcpServiceStatus, server_ip) == 20);
    assert!(core::mem::offset_of!(DhcpServiceStatus, netmask) == 24);
    assert!(core::mem::offset_of!(DhcpServiceStatus, gateway_ip) == 28);
    assert!(core::mem::offset_of!(DhcpServiceStatus, dns_ip) == 32);
    assert!(core::mem::offset_of!(DhcpServiceStatus, lease_seconds) == 36);
    assert!(core::mem::offset_of!(DhcpServiceStatus, renew_seconds) == 40);
    assert!(core::mem::offset_of!(DhcpServiceStatus, rebind_seconds) == 44);
    assert!(core::mem::offset_of!(DhcpServiceStatus, elapsed_seconds) == 48);
    assert!(core::mem::offset_of!(DhcpServiceStatus, remaining_seconds) == 52);
    assert!(core::mem::offset_of!(DhcpServiceStatus, renew_in_seconds) == 56);
    assert!(core::mem::offset_of!(DhcpServiceStatus, rebind_in_seconds) == 60);
    assert!(core::mem::offset_of!(DhcpServiceStatus, last_attempt) == 64);
    assert!(core::mem::offset_of!(DhcpServiceStatus, last_type) == 68);
    assert!(core::mem::offset_of!(DhcpServiceStatus, discover_tx) == 72);
    assert!(core::mem::offset_of!(DhcpServiceStatus, offer_rx) == 80);
    assert!(core::mem::offset_of!(DhcpServiceStatus, request_tx) == 88);
    assert!(core::mem::offset_of!(DhcpServiceStatus, ack_rx) == 96);
    assert!(core::mem::offset_of!(DhcpServiceStatus, nak_rx) == 104);
    assert!(core::mem::offset_of!(DhcpServiceStatus, release_tx) == 112);
    assert!(core::mem::offset_of!(DhcpServiceStatus, retries) == 120);
    assert!(core::mem::offset_of!(DhcpServiceStatus, timeouts) == 128);
    assert!(core::mem::offset_of!(DhcpServiceStatus, release_errors) == 136);
    assert!(core::mem::offset_of!(DhcpServiceStatus, malformed) == 144);
    assert!(core::mem::offset_of!(DhcpServiceStatus, self_tests) == 152);
    assert!(core::mem::offset_of!(DhcpServiceStatus, pending_label) == 160);
    assert!(core::mem::offset_of!(DhcpServiceStatus, last_error) == 176);
};
impl Wire for DhcpServiceStatus {
    const SIZE: usize = 208;
    fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.runtime_state.to_le_bytes());
        out[8..12].copy_from_slice(&self.flags.to_le_bytes());
        out[12..16].copy_from_slice(&self.xid.to_le_bytes());
        out[16..20].copy_from_slice(&self.offered_ip);
        out[20..24].copy_from_slice(&self.server_ip);
        out[24..28].copy_from_slice(&self.netmask);
        out[28..32].copy_from_slice(&self.gateway_ip);
        out[32..36].copy_from_slice(&self.dns_ip);
        out[36..40].copy_from_slice(&self.lease_seconds.to_le_bytes());
        out[40..44].copy_from_slice(&self.renew_seconds.to_le_bytes());
        out[44..48].copy_from_slice(&self.rebind_seconds.to_le_bytes());
        out[48..52].copy_from_slice(&self.elapsed_seconds.to_le_bytes());
        out[52..56].copy_from_slice(&self.remaining_seconds.to_le_bytes());
        out[56..60].copy_from_slice(&self.renew_in_seconds.to_le_bytes());
        out[60..64].copy_from_slice(&self.rebind_in_seconds.to_le_bytes());
        out[64..68].copy_from_slice(&self.last_attempt.to_le_bytes());
        out[68..72].copy_from_slice(&self.last_type.to_le_bytes());
        out[72..80].copy_from_slice(&self.discover_tx.to_le_bytes());
        out[80..88].copy_from_slice(&self.offer_rx.to_le_bytes());
        out[88..96].copy_from_slice(&self.request_tx.to_le_bytes());
        out[96..104].copy_from_slice(&self.ack_rx.to_le_bytes());
        out[104..112].copy_from_slice(&self.nak_rx.to_le_bytes());
        out[112..120].copy_from_slice(&self.release_tx.to_le_bytes());
        out[120..128].copy_from_slice(&self.retries.to_le_bytes());
        out[128..136].copy_from_slice(&self.timeouts.to_le_bytes());
        out[136..144].copy_from_slice(&self.release_errors.to_le_bytes());
        out[144..152].copy_from_slice(&self.malformed.to_le_bytes());
        out[152..160].copy_from_slice(&self.self_tests.to_le_bytes());
        out[160..176].copy_from_slice(&self.pending_label);
        out[176..208].copy_from_slice(&self.last_error);
        Self::SIZE
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DhcpServiceResult {
    pub magic: u32,
    pub version: u16,
    pub action: u16,
    pub result: i32,
    pub flags: u32,
    pub offered_ip: [u8; 4],
    pub server_ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub dns_ip: [u8; 4],
    pub lease_seconds: u32,
    pub elapsed_seconds: u32,
    pub remaining_seconds: u32,
    pub renew_in_seconds: u32,
    pub rebind_in_seconds: u32,
    pub last_attempt: u32,
    pub last_type: u32,
    pub discover_tx: u64,
    pub offer_rx: u64,
    pub request_tx: u64,
    pub ack_rx: u64,
    pub nak_rx: u64,
    pub release_tx: u64,
    pub retries: u64,
    pub timeouts: u64,
    pub release_errors: u64,
    pub malformed: u64,
}
impl DhcpServiceResult {
    pub const EMPTY: Self = Self {
        magic: DHCP_RESULT_MAGIC,
        version: DHCP_RESULT_VERSION,
        action: 0,
        result: 6,
        flags: 0,
        offered_ip: [0; 4],
        server_ip: [0; 4],
        netmask: [0; 4],
        gateway_ip: [0; 4],
        dns_ip: [0; 4],
        lease_seconds: 0,
        elapsed_seconds: 0,
        remaining_seconds: 0,
        renew_in_seconds: 0,
        rebind_in_seconds: 0,
        last_attempt: 0,
        last_type: 0,
        discover_tx: 0,
        offer_rx: 0,
        request_tx: 0,
        ack_rx: 0,
        nak_rx: 0,
        release_tx: 0,
        retries: 0,
        timeouts: 0,
        release_errors: 0,
        malformed: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<DhcpServiceResult>() == 144);
    assert!(core::mem::align_of::<DhcpServiceResult>() == 8);
    assert!(core::mem::offset_of!(DhcpServiceResult, magic) == 0);
    assert!(core::mem::offset_of!(DhcpServiceResult, version) == 4);
    assert!(core::mem::offset_of!(DhcpServiceResult, action) == 6);
    assert!(core::mem::offset_of!(DhcpServiceResult, result) == 8);
    assert!(core::mem::offset_of!(DhcpServiceResult, flags) == 12);
    assert!(core::mem::offset_of!(DhcpServiceResult, offered_ip) == 16);
    assert!(core::mem::offset_of!(DhcpServiceResult, server_ip) == 20);
    assert!(core::mem::offset_of!(DhcpServiceResult, netmask) == 24);
    assert!(core::mem::offset_of!(DhcpServiceResult, gateway_ip) == 28);
    assert!(core::mem::offset_of!(DhcpServiceResult, dns_ip) == 32);
    assert!(core::mem::offset_of!(DhcpServiceResult, lease_seconds) == 36);
    assert!(core::mem::offset_of!(DhcpServiceResult, elapsed_seconds) == 40);
    assert!(core::mem::offset_of!(DhcpServiceResult, remaining_seconds) == 44);
    assert!(core::mem::offset_of!(DhcpServiceResult, renew_in_seconds) == 48);
    assert!(core::mem::offset_of!(DhcpServiceResult, rebind_in_seconds) == 52);
    assert!(core::mem::offset_of!(DhcpServiceResult, last_attempt) == 56);
    assert!(core::mem::offset_of!(DhcpServiceResult, last_type) == 60);
    assert!(core::mem::offset_of!(DhcpServiceResult, discover_tx) == 64);
    assert!(core::mem::offset_of!(DhcpServiceResult, offer_rx) == 72);
    assert!(core::mem::offset_of!(DhcpServiceResult, request_tx) == 80);
    assert!(core::mem::offset_of!(DhcpServiceResult, ack_rx) == 88);
    assert!(core::mem::offset_of!(DhcpServiceResult, nak_rx) == 96);
    assert!(core::mem::offset_of!(DhcpServiceResult, release_tx) == 104);
    assert!(core::mem::offset_of!(DhcpServiceResult, retries) == 112);
    assert!(core::mem::offset_of!(DhcpServiceResult, timeouts) == 120);
    assert!(core::mem::offset_of!(DhcpServiceResult, release_errors) == 128);
    assert!(core::mem::offset_of!(DhcpServiceResult, malformed) == 136);
};
impl Wire for DhcpServiceResult {
    const SIZE: usize = 144;
    fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.action.to_le_bytes());
        out[8..12].copy_from_slice(&self.result.to_le_bytes());
        out[12..16].copy_from_slice(&self.flags.to_le_bytes());
        out[16..20].copy_from_slice(&self.offered_ip);
        out[20..24].copy_from_slice(&self.server_ip);
        out[24..28].copy_from_slice(&self.netmask);
        out[28..32].copy_from_slice(&self.gateway_ip);
        out[32..36].copy_from_slice(&self.dns_ip);
        out[36..40].copy_from_slice(&self.lease_seconds.to_le_bytes());
        out[40..44].copy_from_slice(&self.elapsed_seconds.to_le_bytes());
        out[44..48].copy_from_slice(&self.remaining_seconds.to_le_bytes());
        out[48..52].copy_from_slice(&self.renew_in_seconds.to_le_bytes());
        out[52..56].copy_from_slice(&self.rebind_in_seconds.to_le_bytes());
        out[56..60].copy_from_slice(&self.last_attempt.to_le_bytes());
        out[60..64].copy_from_slice(&self.last_type.to_le_bytes());
        out[64..72].copy_from_slice(&self.discover_tx.to_le_bytes());
        out[72..80].copy_from_slice(&self.offer_rx.to_le_bytes());
        out[80..88].copy_from_slice(&self.request_tx.to_le_bytes());
        out[88..96].copy_from_slice(&self.ack_rx.to_le_bytes());
        out[96..104].copy_from_slice(&self.nak_rx.to_le_bytes());
        out[104..112].copy_from_slice(&self.release_tx.to_le_bytes());
        out[112..120].copy_from_slice(&self.retries.to_le_bytes());
        out[120..128].copy_from_slice(&self.timeouts.to_le_bytes());
        out[128..136].copy_from_slice(&self.release_errors.to_le_bytes());
        out[136..144].copy_from_slice(&self.malformed.to_le_bytes());
        Self::SIZE
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DnsServiceStatus {
    pub magic: u32,
    pub version: u16,
    pub reserved: u16,
    pub flags: u32,
    pub last_result: i32,
    pub last_id: u16,
    pub name_len: u16,
    pub pending_name_len: u16,
    pub cache_name_len: u16,
    pub last_server: [u8; 4],
    pub last_answer: [u8; 4],
    pub cache_server: [u8; 4],
    pub cache_answer: [u8; 4],
    pub cache_age_seconds: u32,
    pub cache_ttl_seconds: u32,
    pub cache_remaining_seconds: u32,
    pub queries_tx: u64,
    pub resolve_requests: u64,
    pub responses_rx: u64,
    pub a_records: u64,
    pub timeouts: u64,
    pub nxdomain: u64,
    pub tx_errors: u64,
    pub malformed: u64,
    pub self_tests: u64,
    pub cache_hits: u64,
    pub cache_stores: u64,
    pub name: [u8; 64],
    pub pending_name: [u8; 64],
    pub cache_name: [u8; 64],
    pub last_error: [u8; 32],
}
impl DnsServiceStatus {
    pub const EMPTY: Self = Self {
        magic: DNS_STATUS_MAGIC,
        version: DNS_STATUS_VERSION,
        reserved: 0,
        flags: 0,
        last_result: 0,
        last_id: 0,
        name_len: 0,
        pending_name_len: 0,
        cache_name_len: 0,
        last_server: [0; 4],
        last_answer: [0; 4],
        cache_server: [0; 4],
        cache_answer: [0; 4],
        cache_age_seconds: 0,
        cache_ttl_seconds: 0,
        cache_remaining_seconds: 0,
        queries_tx: 0,
        resolve_requests: 0,
        responses_rx: 0,
        a_records: 0,
        timeouts: 0,
        nxdomain: 0,
        tx_errors: 0,
        malformed: 0,
        self_tests: 0,
        cache_hits: 0,
        cache_stores: 0,
        name: [0; 64],
        pending_name: [0; 64],
        cache_name: [0; 64],
        last_error: [0; 32],
    };
}
const _: () = {
    assert!(core::mem::size_of::<DnsServiceStatus>() == 368);
    assert!(core::mem::align_of::<DnsServiceStatus>() == 8);
    assert!(core::mem::offset_of!(DnsServiceStatus, magic) == 0);
    assert!(core::mem::offset_of!(DnsServiceStatus, version) == 4);
    assert!(core::mem::offset_of!(DnsServiceStatus, reserved) == 6);
    assert!(core::mem::offset_of!(DnsServiceStatus, flags) == 8);
    assert!(core::mem::offset_of!(DnsServiceStatus, last_result) == 12);
    assert!(core::mem::offset_of!(DnsServiceStatus, last_id) == 16);
    assert!(core::mem::offset_of!(DnsServiceStatus, name_len) == 18);
    assert!(core::mem::offset_of!(DnsServiceStatus, pending_name_len) == 20);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_name_len) == 22);
    assert!(core::mem::offset_of!(DnsServiceStatus, last_server) == 24);
    assert!(core::mem::offset_of!(DnsServiceStatus, last_answer) == 28);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_server) == 32);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_answer) == 36);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_age_seconds) == 40);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_ttl_seconds) == 44);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_remaining_seconds) == 48);
    assert!(core::mem::offset_of!(DnsServiceStatus, queries_tx) == 56);
    assert!(core::mem::offset_of!(DnsServiceStatus, resolve_requests) == 64);
    assert!(core::mem::offset_of!(DnsServiceStatus, responses_rx) == 72);
    assert!(core::mem::offset_of!(DnsServiceStatus, a_records) == 80);
    assert!(core::mem::offset_of!(DnsServiceStatus, timeouts) == 88);
    assert!(core::mem::offset_of!(DnsServiceStatus, nxdomain) == 96);
    assert!(core::mem::offset_of!(DnsServiceStatus, tx_errors) == 104);
    assert!(core::mem::offset_of!(DnsServiceStatus, malformed) == 112);
    assert!(core::mem::offset_of!(DnsServiceStatus, self_tests) == 120);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_hits) == 128);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_stores) == 136);
    assert!(core::mem::offset_of!(DnsServiceStatus, name) == 144);
    assert!(core::mem::offset_of!(DnsServiceStatus, pending_name) == 208);
    assert!(core::mem::offset_of!(DnsServiceStatus, cache_name) == 272);
    assert!(core::mem::offset_of!(DnsServiceStatus, last_error) == 336);
};
impl Wire for DnsServiceStatus {
    const SIZE: usize = 368;
    fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.reserved.to_le_bytes());
        out[8..12].copy_from_slice(&self.flags.to_le_bytes());
        out[12..16].copy_from_slice(&self.last_result.to_le_bytes());
        out[16..18].copy_from_slice(&self.last_id.to_le_bytes());
        out[18..20].copy_from_slice(&self.name_len.to_le_bytes());
        out[20..22].copy_from_slice(&self.pending_name_len.to_le_bytes());
        out[22..24].copy_from_slice(&self.cache_name_len.to_le_bytes());
        out[24..28].copy_from_slice(&self.last_server);
        out[28..32].copy_from_slice(&self.last_answer);
        out[32..36].copy_from_slice(&self.cache_server);
        out[36..40].copy_from_slice(&self.cache_answer);
        out[40..44].copy_from_slice(&self.cache_age_seconds.to_le_bytes());
        out[44..48].copy_from_slice(&self.cache_ttl_seconds.to_le_bytes());
        out[48..52].copy_from_slice(&self.cache_remaining_seconds.to_le_bytes());
        out[56..64].copy_from_slice(&self.queries_tx.to_le_bytes());
        out[64..72].copy_from_slice(&self.resolve_requests.to_le_bytes());
        out[72..80].copy_from_slice(&self.responses_rx.to_le_bytes());
        out[80..88].copy_from_slice(&self.a_records.to_le_bytes());
        out[88..96].copy_from_slice(&self.timeouts.to_le_bytes());
        out[96..104].copy_from_slice(&self.nxdomain.to_le_bytes());
        out[104..112].copy_from_slice(&self.tx_errors.to_le_bytes());
        out[112..120].copy_from_slice(&self.malformed.to_le_bytes());
        out[120..128].copy_from_slice(&self.self_tests.to_le_bytes());
        out[128..136].copy_from_slice(&self.cache_hits.to_le_bytes());
        out[136..144].copy_from_slice(&self.cache_stores.to_le_bytes());
        out[144..208].copy_from_slice(&self.name);
        out[208..272].copy_from_slice(&self.pending_name);
        out[272..336].copy_from_slice(&self.cache_name);
        out[336..368].copy_from_slice(&self.last_error);
        Self::SIZE
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DnsServiceResult {
    pub magic: u32,
    pub version: u16,
    pub action: u16,
    pub result: i32,
    pub flags: u32,
    pub answer: [u8; 4],
    pub server: [u8; 4],
    pub cache_answer: [u8; 4],
    pub cache_age_seconds: u32,
    pub cache_ttl_seconds: u32,
    pub cache_remaining_seconds: u32,
    pub queries_tx: u64,
    pub resolve_requests: u64,
    pub responses_rx: u64,
    pub a_records: u64,
    pub timeouts: u64,
    pub nxdomain: u64,
    pub tx_errors: u64,
    pub malformed: u64,
    pub cache_hits: u64,
    pub cache_stores: u64,
    pub last_id: u16,
    pub name_len: u16,
    pub name: [u8; 96],
    pub last_error: [u8; 32],
}
impl DnsServiceResult {
    pub const EMPTY: Self = Self {
        magic: DNS_RESULT_MAGIC,
        version: DNS_RESULT_VERSION,
        action: 0,
        result: -13,
        flags: 0,
        answer: [0; 4],
        server: [0; 4],
        cache_answer: [0; 4],
        cache_age_seconds: 0,
        cache_ttl_seconds: 0,
        cache_remaining_seconds: 0,
        queries_tx: 0,
        resolve_requests: 0,
        responses_rx: 0,
        a_records: 0,
        timeouts: 0,
        nxdomain: 0,
        tx_errors: 0,
        malformed: 0,
        cache_hits: 0,
        cache_stores: 0,
        last_id: 0,
        name_len: 0,
        name: [0; 96],
        last_error: [0; 32],
    };
}
const _: () = {
    assert!(core::mem::size_of::<DnsServiceResult>() == 256);
    assert!(core::mem::align_of::<DnsServiceResult>() == 8);
    assert!(core::mem::offset_of!(DnsServiceResult, magic) == 0);
    assert!(core::mem::offset_of!(DnsServiceResult, version) == 4);
    assert!(core::mem::offset_of!(DnsServiceResult, action) == 6);
    assert!(core::mem::offset_of!(DnsServiceResult, result) == 8);
    assert!(core::mem::offset_of!(DnsServiceResult, flags) == 12);
    assert!(core::mem::offset_of!(DnsServiceResult, answer) == 16);
    assert!(core::mem::offset_of!(DnsServiceResult, server) == 20);
    assert!(core::mem::offset_of!(DnsServiceResult, cache_answer) == 24);
    assert!(core::mem::offset_of!(DnsServiceResult, cache_age_seconds) == 28);
    assert!(core::mem::offset_of!(DnsServiceResult, cache_ttl_seconds) == 32);
    assert!(core::mem::offset_of!(DnsServiceResult, cache_remaining_seconds) == 36);
    assert!(core::mem::offset_of!(DnsServiceResult, queries_tx) == 40);
    assert!(core::mem::offset_of!(DnsServiceResult, resolve_requests) == 48);
    assert!(core::mem::offset_of!(DnsServiceResult, responses_rx) == 56);
    assert!(core::mem::offset_of!(DnsServiceResult, a_records) == 64);
    assert!(core::mem::offset_of!(DnsServiceResult, timeouts) == 72);
    assert!(core::mem::offset_of!(DnsServiceResult, nxdomain) == 80);
    assert!(core::mem::offset_of!(DnsServiceResult, tx_errors) == 88);
    assert!(core::mem::offset_of!(DnsServiceResult, malformed) == 96);
    assert!(core::mem::offset_of!(DnsServiceResult, cache_hits) == 104);
    assert!(core::mem::offset_of!(DnsServiceResult, cache_stores) == 112);
    assert!(core::mem::offset_of!(DnsServiceResult, last_id) == 120);
    assert!(core::mem::offset_of!(DnsServiceResult, name_len) == 122);
    assert!(core::mem::offset_of!(DnsServiceResult, name) == 124);
    assert!(core::mem::offset_of!(DnsServiceResult, last_error) == 220);
};
impl Wire for DnsServiceResult {
    const SIZE: usize = 256;
    fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.action.to_le_bytes());
        out[8..12].copy_from_slice(&self.result.to_le_bytes());
        out[12..16].copy_from_slice(&self.flags.to_le_bytes());
        out[16..20].copy_from_slice(&self.answer);
        out[20..24].copy_from_slice(&self.server);
        out[24..28].copy_from_slice(&self.cache_answer);
        out[28..32].copy_from_slice(&self.cache_age_seconds.to_le_bytes());
        out[32..36].copy_from_slice(&self.cache_ttl_seconds.to_le_bytes());
        out[36..40].copy_from_slice(&self.cache_remaining_seconds.to_le_bytes());
        out[40..48].copy_from_slice(&self.queries_tx.to_le_bytes());
        out[48..56].copy_from_slice(&self.resolve_requests.to_le_bytes());
        out[56..64].copy_from_slice(&self.responses_rx.to_le_bytes());
        out[64..72].copy_from_slice(&self.a_records.to_le_bytes());
        out[72..80].copy_from_slice(&self.timeouts.to_le_bytes());
        out[80..88].copy_from_slice(&self.nxdomain.to_le_bytes());
        out[88..96].copy_from_slice(&self.tx_errors.to_le_bytes());
        out[96..104].copy_from_slice(&self.malformed.to_le_bytes());
        out[104..112].copy_from_slice(&self.cache_hits.to_le_bytes());
        out[112..120].copy_from_slice(&self.cache_stores.to_le_bytes());
        out[120..122].copy_from_slice(&self.last_id.to_le_bytes());
        out[122..124].copy_from_slice(&self.name_len.to_le_bytes());
        out[124..220].copy_from_slice(&self.name);
        out[220..252].copy_from_slice(&self.last_error);
        Self::SIZE
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TcpServiceStatus {
    pub magic: u32,
    pub version: u16,
    pub reserved: u16,
    pub flags: u32,
    pub request_owner_id: u16,
    pub owned_handles: u16,
    pub legacy_handles: u16,
    pub reserved3: u16,
    pub active_connections: u32,
    pub max_connections: u32,
    pub active_listeners: u32,
    pub handle_count: u32,
    pub max_handles: u32,
    pub tcp_buffer_size: u32,
    pub message_payload_max: u32,
    pub write_max: u32,
    pub read_max: u32,
    pub rx_segments: u64,
    pub tx_segments: u64,
    pub syn_tx: u64,
    pub synack_rx: u64,
    pub ack_tx: u64,
    pub data_tx: u64,
    pub data_rx: u64,
    pub fin_tx: u64,
    pub rst_rx: u64,
    pub checksum_errors: u64,
    pub timeouts: u64,
    pub self_tests: u64,
    pub synack_tx: u64,
    pub listen_syn_rx: u64,
    pub accepts: u64,
    pub retransmits: u64,
    pub rx_drops: u64,
    pub last_source_port: u16,
    pub last_dest_port: u16,
    pub last_flags: u16,
    pub reserved2: u16,
    pub last_seq: u32,
    pub last_ack: u32,
    pub last_payload_len: u32,
    pub owner_mismatches: u64,
    pub last_error: [u8; 32],
    pub stale_handles_reaped: u64,
    pub stale_tombstones: u32,
    pub last_lifecycle_cause: u32,
    pub lifecycle_closed: u64,
    pub lifecycle_reset: u64,
    pub lifecycle_timeout: u64,
    pub lifecycle_peer_gone: u64,
    pub lifecycle_local_abort: u64,
    pub lifecycle_local_close: u64,
    pub lifecycle_pending_close: u64,
    pub lifecycle_would_block: u64,
    pub lifecycle_bad_handle: u64,
    pub lifecycle_owner_mismatch: u64,
    pub read_wait_timeouts: u64,
    pub accept_wait_timeouts: u64,
    pub write_would_block: u64,
    pub close_cancelled: u64,
}
impl TcpServiceStatus {
    pub const EMPTY: Self = Self {
        magic: TCP_STATUS_MAGIC,
        version: TCP_STATUS_VERSION,
        reserved: 0,
        flags: 0,
        request_owner_id: 0,
        owned_handles: 0,
        legacy_handles: 0,
        reserved3: 0,
        active_connections: 0,
        max_connections: 0,
        active_listeners: 0,
        handle_count: 0,
        max_handles: TCP_SERVICE_HANDLES as u32,
        tcp_buffer_size: (tcp::BUFFER_SIZE) as u32,
        message_payload_max: TCP_MESSAGE_PAYLOAD_MAX as u32,
        write_max: TCP_WRITE_PAYLOAD_MAX as u32,
        read_max: 0,
        rx_segments: 0,
        tx_segments: 0,
        syn_tx: 0,
        synack_rx: 0,
        ack_tx: 0,
        data_tx: 0,
        data_rx: 0,
        fin_tx: 0,
        rst_rx: 0,
        checksum_errors: 0,
        timeouts: 0,
        self_tests: 0,
        synack_tx: 0,
        listen_syn_rx: 0,
        accepts: 0,
        retransmits: 0,
        rx_drops: 0,
        last_source_port: 0,
        last_dest_port: 0,
        last_flags: 0,
        reserved2: 0,
        last_seq: 0,
        last_ack: 0,
        last_payload_len: 0,
        owner_mismatches: 0,
        last_error: [0; 32],
        stale_handles_reaped: 0,
        stale_tombstones: 0,
        last_lifecycle_cause: SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
        lifecycle_closed: 0,
        lifecycle_reset: 0,
        lifecycle_timeout: 0,
        lifecycle_peer_gone: 0,
        lifecycle_local_abort: 0,
        lifecycle_local_close: 0,
        lifecycle_pending_close: 0,
        lifecycle_would_block: 0,
        lifecycle_bad_handle: 0,
        lifecycle_owner_mismatch: 0,
        read_wait_timeouts: 0,
        accept_wait_timeouts: 0,
        write_would_block: 0,
        close_cancelled: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<TcpServiceStatus>() == 384);
    assert!(core::mem::align_of::<TcpServiceStatus>() == 8);
    assert!(core::mem::offset_of!(TcpServiceStatus, magic) == 0);
    assert!(core::mem::offset_of!(TcpServiceStatus, version) == 4);
    assert!(core::mem::offset_of!(TcpServiceStatus, reserved) == 6);
    assert!(core::mem::offset_of!(TcpServiceStatus, flags) == 8);
    assert!(core::mem::offset_of!(TcpServiceStatus, request_owner_id) == 12);
    assert!(core::mem::offset_of!(TcpServiceStatus, owned_handles) == 14);
    assert!(core::mem::offset_of!(TcpServiceStatus, legacy_handles) == 16);
    assert!(core::mem::offset_of!(TcpServiceStatus, reserved3) == 18);
    assert!(core::mem::offset_of!(TcpServiceStatus, active_connections) == 20);
    assert!(core::mem::offset_of!(TcpServiceStatus, max_connections) == 24);
    assert!(core::mem::offset_of!(TcpServiceStatus, active_listeners) == 28);
    assert!(core::mem::offset_of!(TcpServiceStatus, handle_count) == 32);
    assert!(core::mem::offset_of!(TcpServiceStatus, max_handles) == 36);
    assert!(core::mem::offset_of!(TcpServiceStatus, tcp_buffer_size) == 40);
    assert!(core::mem::offset_of!(TcpServiceStatus, message_payload_max) == 44);
    assert!(core::mem::offset_of!(TcpServiceStatus, write_max) == 48);
    assert!(core::mem::offset_of!(TcpServiceStatus, read_max) == 52);
    assert!(core::mem::offset_of!(TcpServiceStatus, rx_segments) == 56);
    assert!(core::mem::offset_of!(TcpServiceStatus, tx_segments) == 64);
    assert!(core::mem::offset_of!(TcpServiceStatus, syn_tx) == 72);
    assert!(core::mem::offset_of!(TcpServiceStatus, synack_rx) == 80);
    assert!(core::mem::offset_of!(TcpServiceStatus, ack_tx) == 88);
    assert!(core::mem::offset_of!(TcpServiceStatus, data_tx) == 96);
    assert!(core::mem::offset_of!(TcpServiceStatus, data_rx) == 104);
    assert!(core::mem::offset_of!(TcpServiceStatus, fin_tx) == 112);
    assert!(core::mem::offset_of!(TcpServiceStatus, rst_rx) == 120);
    assert!(core::mem::offset_of!(TcpServiceStatus, checksum_errors) == 128);
    assert!(core::mem::offset_of!(TcpServiceStatus, timeouts) == 136);
    assert!(core::mem::offset_of!(TcpServiceStatus, self_tests) == 144);
    assert!(core::mem::offset_of!(TcpServiceStatus, synack_tx) == 152);
    assert!(core::mem::offset_of!(TcpServiceStatus, listen_syn_rx) == 160);
    assert!(core::mem::offset_of!(TcpServiceStatus, accepts) == 168);
    assert!(core::mem::offset_of!(TcpServiceStatus, retransmits) == 176);
    assert!(core::mem::offset_of!(TcpServiceStatus, rx_drops) == 184);
    assert!(core::mem::offset_of!(TcpServiceStatus, last_source_port) == 192);
    assert!(core::mem::offset_of!(TcpServiceStatus, last_dest_port) == 194);
    assert!(core::mem::offset_of!(TcpServiceStatus, last_flags) == 196);
    assert!(core::mem::offset_of!(TcpServiceStatus, reserved2) == 198);
    assert!(core::mem::offset_of!(TcpServiceStatus, last_seq) == 200);
    assert!(core::mem::offset_of!(TcpServiceStatus, last_ack) == 204);
    assert!(core::mem::offset_of!(TcpServiceStatus, last_payload_len) == 208);
    assert!(core::mem::offset_of!(TcpServiceStatus, owner_mismatches) == 216);
    assert!(core::mem::offset_of!(TcpServiceStatus, last_error) == 224);
    assert!(core::mem::offset_of!(TcpServiceStatus, stale_handles_reaped) == 256);
    assert!(core::mem::offset_of!(TcpServiceStatus, stale_tombstones) == 264);
    assert!(core::mem::offset_of!(TcpServiceStatus, last_lifecycle_cause) == 268);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_closed) == 272);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_reset) == 280);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_timeout) == 288);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_peer_gone) == 296);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_local_abort) == 304);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_local_close) == 312);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_pending_close) == 320);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_would_block) == 328);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_bad_handle) == 336);
    assert!(core::mem::offset_of!(TcpServiceStatus, lifecycle_owner_mismatch) == 344);
    assert!(core::mem::offset_of!(TcpServiceStatus, read_wait_timeouts) == 352);
    assert!(core::mem::offset_of!(TcpServiceStatus, accept_wait_timeouts) == 360);
    assert!(core::mem::offset_of!(TcpServiceStatus, write_would_block) == 368);
    assert!(core::mem::offset_of!(TcpServiceStatus, close_cancelled) == 376);
};
impl Wire for TcpServiceStatus {
    const SIZE: usize = 384;
    fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.reserved.to_le_bytes());
        out[8..12].copy_from_slice(&self.flags.to_le_bytes());
        out[12..14].copy_from_slice(&self.request_owner_id.to_le_bytes());
        out[14..16].copy_from_slice(&self.owned_handles.to_le_bytes());
        out[16..18].copy_from_slice(&self.legacy_handles.to_le_bytes());
        out[18..20].copy_from_slice(&self.reserved3.to_le_bytes());
        out[20..24].copy_from_slice(&self.active_connections.to_le_bytes());
        out[24..28].copy_from_slice(&self.max_connections.to_le_bytes());
        out[28..32].copy_from_slice(&self.active_listeners.to_le_bytes());
        out[32..36].copy_from_slice(&self.handle_count.to_le_bytes());
        out[36..40].copy_from_slice(&self.max_handles.to_le_bytes());
        out[40..44].copy_from_slice(&self.tcp_buffer_size.to_le_bytes());
        out[44..48].copy_from_slice(&self.message_payload_max.to_le_bytes());
        out[48..52].copy_from_slice(&self.write_max.to_le_bytes());
        out[52..56].copy_from_slice(&self.read_max.to_le_bytes());
        out[56..64].copy_from_slice(&self.rx_segments.to_le_bytes());
        out[64..72].copy_from_slice(&self.tx_segments.to_le_bytes());
        out[72..80].copy_from_slice(&self.syn_tx.to_le_bytes());
        out[80..88].copy_from_slice(&self.synack_rx.to_le_bytes());
        out[88..96].copy_from_slice(&self.ack_tx.to_le_bytes());
        out[96..104].copy_from_slice(&self.data_tx.to_le_bytes());
        out[104..112].copy_from_slice(&self.data_rx.to_le_bytes());
        out[112..120].copy_from_slice(&self.fin_tx.to_le_bytes());
        out[120..128].copy_from_slice(&self.rst_rx.to_le_bytes());
        out[128..136].copy_from_slice(&self.checksum_errors.to_le_bytes());
        out[136..144].copy_from_slice(&self.timeouts.to_le_bytes());
        out[144..152].copy_from_slice(&self.self_tests.to_le_bytes());
        out[152..160].copy_from_slice(&self.synack_tx.to_le_bytes());
        out[160..168].copy_from_slice(&self.listen_syn_rx.to_le_bytes());
        out[168..176].copy_from_slice(&self.accepts.to_le_bytes());
        out[176..184].copy_from_slice(&self.retransmits.to_le_bytes());
        out[184..192].copy_from_slice(&self.rx_drops.to_le_bytes());
        out[192..194].copy_from_slice(&self.last_source_port.to_le_bytes());
        out[194..196].copy_from_slice(&self.last_dest_port.to_le_bytes());
        out[196..198].copy_from_slice(&self.last_flags.to_le_bytes());
        out[198..200].copy_from_slice(&self.reserved2.to_le_bytes());
        out[200..204].copy_from_slice(&self.last_seq.to_le_bytes());
        out[204..208].copy_from_slice(&self.last_ack.to_le_bytes());
        out[208..212].copy_from_slice(&self.last_payload_len.to_le_bytes());
        out[216..224].copy_from_slice(&self.owner_mismatches.to_le_bytes());
        out[224..256].copy_from_slice(&self.last_error);
        out[256..264].copy_from_slice(&self.stale_handles_reaped.to_le_bytes());
        out[264..268].copy_from_slice(&self.stale_tombstones.to_le_bytes());
        out[268..272].copy_from_slice(&self.last_lifecycle_cause.to_le_bytes());
        out[272..280].copy_from_slice(&self.lifecycle_closed.to_le_bytes());
        out[280..288].copy_from_slice(&self.lifecycle_reset.to_le_bytes());
        out[288..296].copy_from_slice(&self.lifecycle_timeout.to_le_bytes());
        out[296..304].copy_from_slice(&self.lifecycle_peer_gone.to_le_bytes());
        out[304..312].copy_from_slice(&self.lifecycle_local_abort.to_le_bytes());
        out[312..320].copy_from_slice(&self.lifecycle_local_close.to_le_bytes());
        out[320..328].copy_from_slice(&self.lifecycle_pending_close.to_le_bytes());
        out[328..336].copy_from_slice(&self.lifecycle_would_block.to_le_bytes());
        out[336..344].copy_from_slice(&self.lifecycle_bad_handle.to_le_bytes());
        out[344..352].copy_from_slice(&self.lifecycle_owner_mismatch.to_le_bytes());
        out[352..360].copy_from_slice(&self.read_wait_timeouts.to_le_bytes());
        out[360..368].copy_from_slice(&self.accept_wait_timeouts.to_le_bytes());
        out[368..376].copy_from_slice(&self.write_would_block.to_le_bytes());
        out[376..384].copy_from_slice(&self.close_cancelled.to_le_bytes());
        Self::SIZE
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TcpServiceResult {
    pub magic: u32,
    pub version: u16,
    pub action: u16,
    pub result: i32,
    pub flags: u32,
    pub handle: u32,
    pub conn_id: u32,
    pub bytes: u32,
    pub requested_bytes: u32,
    pub port: u16,
    pub remote_port: u16,
    pub remote_ip: [u8; 4],
    pub active_connections: u32,
    pub max_connections: u32,
    pub active_listeners: u32,
    pub handle_count: u32,
    pub max_handles: u32,
    pub tcp_buffer_size: u32,
    pub message_payload_max: u32,
    pub write_max: u32,
    pub read_max: u32,
    pub pending_rx: u32,
    pub rx_window: u32,
    pub tx_window: u32,
    pub tx_seq: u32,
    pub tx_ack: u32,
    pub retransmits: u32,
    pub rx_drops: u32,
    pub local_ip: [u8; 4],
    pub local_port: u16,
    pub reserved3: u16,
    pub last_error: [u8; 32],
    pub lifecycle_cause: u32,
    pub service_status: u32,
    pub owner_id: u16,
    pub reserved4: u16,
}
impl TcpServiceResult {
    pub const EMPTY: Self = Self {
        magic: TCP_RESULT_MAGIC,
        version: TCP_RESULT_VERSION,
        action: 0,
        result: -1,
        flags: 0,
        handle: 0,
        conn_id: 0,
        bytes: 0,
        requested_bytes: 0,
        port: 0,
        remote_port: 0,
        remote_ip: [0; 4],
        active_connections: 0,
        max_connections: 0,
        active_listeners: 0,
        handle_count: 0,
        max_handles: TCP_SERVICE_HANDLES as u32,
        tcp_buffer_size: (tcp::BUFFER_SIZE) as u32,
        message_payload_max: TCP_MESSAGE_PAYLOAD_MAX as u32,
        write_max: TCP_WRITE_PAYLOAD_MAX as u32,
        read_max: 0,
        pending_rx: 0,
        rx_window: 0,
        tx_window: 0,
        tx_seq: 0,
        tx_ack: 0,
        retransmits: 0,
        rx_drops: 0,
        local_ip: [0; 4],
        local_port: 0,
        reserved3: 0,
        last_error: [0; 32],
        lifecycle_cause: SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
        service_status: 0,
        owner_id: 0,
        reserved4: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<TcpServiceResult>() == 156);
    assert!(core::mem::align_of::<TcpServiceResult>() == 4);
    assert!(core::mem::offset_of!(TcpServiceResult, magic) == 0);
    assert!(core::mem::offset_of!(TcpServiceResult, version) == 4);
    assert!(core::mem::offset_of!(TcpServiceResult, action) == 6);
    assert!(core::mem::offset_of!(TcpServiceResult, result) == 8);
    assert!(core::mem::offset_of!(TcpServiceResult, flags) == 12);
    assert!(core::mem::offset_of!(TcpServiceResult, handle) == 16);
    assert!(core::mem::offset_of!(TcpServiceResult, conn_id) == 20);
    assert!(core::mem::offset_of!(TcpServiceResult, bytes) == 24);
    assert!(core::mem::offset_of!(TcpServiceResult, requested_bytes) == 28);
    assert!(core::mem::offset_of!(TcpServiceResult, port) == 32);
    assert!(core::mem::offset_of!(TcpServiceResult, remote_port) == 34);
    assert!(core::mem::offset_of!(TcpServiceResult, remote_ip) == 36);
    assert!(core::mem::offset_of!(TcpServiceResult, active_connections) == 40);
    assert!(core::mem::offset_of!(TcpServiceResult, max_connections) == 44);
    assert!(core::mem::offset_of!(TcpServiceResult, active_listeners) == 48);
    assert!(core::mem::offset_of!(TcpServiceResult, handle_count) == 52);
    assert!(core::mem::offset_of!(TcpServiceResult, max_handles) == 56);
    assert!(core::mem::offset_of!(TcpServiceResult, tcp_buffer_size) == 60);
    assert!(core::mem::offset_of!(TcpServiceResult, message_payload_max) == 64);
    assert!(core::mem::offset_of!(TcpServiceResult, write_max) == 68);
    assert!(core::mem::offset_of!(TcpServiceResult, read_max) == 72);
    assert!(core::mem::offset_of!(TcpServiceResult, pending_rx) == 76);
    assert!(core::mem::offset_of!(TcpServiceResult, rx_window) == 80);
    assert!(core::mem::offset_of!(TcpServiceResult, tx_window) == 84);
    assert!(core::mem::offset_of!(TcpServiceResult, tx_seq) == 88);
    assert!(core::mem::offset_of!(TcpServiceResult, tx_ack) == 92);
    assert!(core::mem::offset_of!(TcpServiceResult, retransmits) == 96);
    assert!(core::mem::offset_of!(TcpServiceResult, rx_drops) == 100);
    assert!(core::mem::offset_of!(TcpServiceResult, local_ip) == 104);
    assert!(core::mem::offset_of!(TcpServiceResult, local_port) == 108);
    assert!(core::mem::offset_of!(TcpServiceResult, reserved3) == 110);
    assert!(core::mem::offset_of!(TcpServiceResult, last_error) == 112);
    assert!(core::mem::offset_of!(TcpServiceResult, lifecycle_cause) == 144);
    assert!(core::mem::offset_of!(TcpServiceResult, service_status) == 148);
    assert!(core::mem::offset_of!(TcpServiceResult, owner_id) == 152);
    assert!(core::mem::offset_of!(TcpServiceResult, reserved4) == 154);
};
impl Wire for TcpServiceResult {
    const SIZE: usize = 156;
    fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.action.to_le_bytes());
        out[8..12].copy_from_slice(&self.result.to_le_bytes());
        out[12..16].copy_from_slice(&self.flags.to_le_bytes());
        out[16..20].copy_from_slice(&self.handle.to_le_bytes());
        out[20..24].copy_from_slice(&self.conn_id.to_le_bytes());
        out[24..28].copy_from_slice(&self.bytes.to_le_bytes());
        out[28..32].copy_from_slice(&self.requested_bytes.to_le_bytes());
        out[32..34].copy_from_slice(&self.port.to_le_bytes());
        out[34..36].copy_from_slice(&self.remote_port.to_le_bytes());
        out[36..40].copy_from_slice(&self.remote_ip);
        out[40..44].copy_from_slice(&self.active_connections.to_le_bytes());
        out[44..48].copy_from_slice(&self.max_connections.to_le_bytes());
        out[48..52].copy_from_slice(&self.active_listeners.to_le_bytes());
        out[52..56].copy_from_slice(&self.handle_count.to_le_bytes());
        out[56..60].copy_from_slice(&self.max_handles.to_le_bytes());
        out[60..64].copy_from_slice(&self.tcp_buffer_size.to_le_bytes());
        out[64..68].copy_from_slice(&self.message_payload_max.to_le_bytes());
        out[68..72].copy_from_slice(&self.write_max.to_le_bytes());
        out[72..76].copy_from_slice(&self.read_max.to_le_bytes());
        out[76..80].copy_from_slice(&self.pending_rx.to_le_bytes());
        out[80..84].copy_from_slice(&self.rx_window.to_le_bytes());
        out[84..88].copy_from_slice(&self.tx_window.to_le_bytes());
        out[88..92].copy_from_slice(&self.tx_seq.to_le_bytes());
        out[92..96].copy_from_slice(&self.tx_ack.to_le_bytes());
        out[96..100].copy_from_slice(&self.retransmits.to_le_bytes());
        out[100..104].copy_from_slice(&self.rx_drops.to_le_bytes());
        out[104..108].copy_from_slice(&self.local_ip);
        out[108..110].copy_from_slice(&self.local_port.to_le_bytes());
        out[110..112].copy_from_slice(&self.reserved3.to_le_bytes());
        out[112..144].copy_from_slice(&self.last_error);
        out[144..148].copy_from_slice(&self.lifecycle_cause.to_le_bytes());
        out[148..152].copy_from_slice(&self.service_status.to_le_bytes());
        out[152..154].copy_from_slice(&self.owner_id.to_le_bytes());
        out[154..156].copy_from_slice(&self.reserved4.to_le_bytes());
        Self::SIZE
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UdpServiceStatus {
    pub magic: u32,
    pub version: u16,
    pub reserved: u16,
    pub flags: u32,
    pub active_sockets: u32,
    pub max_sockets: u32,
    pub queued_packets: u32,
    pub queue_limit: u32,
    pub payload_max: u32,
    pub message_payload_max: u32,
    pub send_max: u32,
    pub recv_max: u32,
    pub delivered: u64,
    pub drops: u64,
    pub last_error: [u8; 32],
    pub last_lifecycle_cause: u32,
    pub reserved2: u32,
    pub lifecycle_closed: u64,
    pub lifecycle_timeout: u64,
    pub lifecycle_local_close: u64,
    pub lifecycle_would_block: u64,
    pub lifecycle_bad_handle: u64,
    pub lifecycle_dropped: u64,
}
impl UdpServiceStatus {
    pub const EMPTY: Self = Self {
        magic: UDP_STATUS_MAGIC,
        version: UDP_STATUS_VERSION,
        reserved: 0,
        flags: 0,
        active_sockets: 0,
        max_sockets: 0,
        queued_packets: 0,
        queue_limit: 0,
        payload_max: 0,
        message_payload_max: TCP_MESSAGE_PAYLOAD_MAX as u32,
        send_max: 0,
        recv_max: 0,
        delivered: 0,
        drops: 0,
        last_error: [0; 32],
        last_lifecycle_cause: SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
        reserved2: 0,
        lifecycle_closed: 0,
        lifecycle_timeout: 0,
        lifecycle_local_close: 0,
        lifecycle_would_block: 0,
        lifecycle_bad_handle: 0,
        lifecycle_dropped: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<UdpServiceStatus>() == 152);
    assert!(core::mem::align_of::<UdpServiceStatus>() == 8);
    assert!(core::mem::offset_of!(UdpServiceStatus, magic) == 0);
    assert!(core::mem::offset_of!(UdpServiceStatus, version) == 4);
    assert!(core::mem::offset_of!(UdpServiceStatus, reserved) == 6);
    assert!(core::mem::offset_of!(UdpServiceStatus, flags) == 8);
    assert!(core::mem::offset_of!(UdpServiceStatus, active_sockets) == 12);
    assert!(core::mem::offset_of!(UdpServiceStatus, max_sockets) == 16);
    assert!(core::mem::offset_of!(UdpServiceStatus, queued_packets) == 20);
    assert!(core::mem::offset_of!(UdpServiceStatus, queue_limit) == 24);
    assert!(core::mem::offset_of!(UdpServiceStatus, payload_max) == 28);
    assert!(core::mem::offset_of!(UdpServiceStatus, message_payload_max) == 32);
    assert!(core::mem::offset_of!(UdpServiceStatus, send_max) == 36);
    assert!(core::mem::offset_of!(UdpServiceStatus, recv_max) == 40);
    assert!(core::mem::offset_of!(UdpServiceStatus, delivered) == 48);
    assert!(core::mem::offset_of!(UdpServiceStatus, drops) == 56);
    assert!(core::mem::offset_of!(UdpServiceStatus, last_error) == 64);
    assert!(core::mem::offset_of!(UdpServiceStatus, last_lifecycle_cause) == 96);
    assert!(core::mem::offset_of!(UdpServiceStatus, reserved2) == 100);
    assert!(core::mem::offset_of!(UdpServiceStatus, lifecycle_closed) == 104);
    assert!(core::mem::offset_of!(UdpServiceStatus, lifecycle_timeout) == 112);
    assert!(core::mem::offset_of!(UdpServiceStatus, lifecycle_local_close) == 120);
    assert!(core::mem::offset_of!(UdpServiceStatus, lifecycle_would_block) == 128);
    assert!(core::mem::offset_of!(UdpServiceStatus, lifecycle_bad_handle) == 136);
    assert!(core::mem::offset_of!(UdpServiceStatus, lifecycle_dropped) == 144);
};
impl Wire for UdpServiceStatus {
    const SIZE: usize = 152;
    fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.reserved.to_le_bytes());
        out[8..12].copy_from_slice(&self.flags.to_le_bytes());
        out[12..16].copy_from_slice(&self.active_sockets.to_le_bytes());
        out[16..20].copy_from_slice(&self.max_sockets.to_le_bytes());
        out[20..24].copy_from_slice(&self.queued_packets.to_le_bytes());
        out[24..28].copy_from_slice(&self.queue_limit.to_le_bytes());
        out[28..32].copy_from_slice(&self.payload_max.to_le_bytes());
        out[32..36].copy_from_slice(&self.message_payload_max.to_le_bytes());
        out[36..40].copy_from_slice(&self.send_max.to_le_bytes());
        out[40..44].copy_from_slice(&self.recv_max.to_le_bytes());
        out[48..56].copy_from_slice(&self.delivered.to_le_bytes());
        out[56..64].copy_from_slice(&self.drops.to_le_bytes());
        out[64..96].copy_from_slice(&self.last_error);
        out[96..100].copy_from_slice(&self.last_lifecycle_cause.to_le_bytes());
        out[100..104].copy_from_slice(&self.reserved2.to_le_bytes());
        out[104..112].copy_from_slice(&self.lifecycle_closed.to_le_bytes());
        out[112..120].copy_from_slice(&self.lifecycle_timeout.to_le_bytes());
        out[120..128].copy_from_slice(&self.lifecycle_local_close.to_le_bytes());
        out[128..136].copy_from_slice(&self.lifecycle_would_block.to_le_bytes());
        out[136..144].copy_from_slice(&self.lifecycle_bad_handle.to_le_bytes());
        out[144..152].copy_from_slice(&self.lifecycle_dropped.to_le_bytes());
        Self::SIZE
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UdpServiceResult {
    pub magic: u32,
    pub version: u16,
    pub action: u16,
    pub result: i32,
    pub flags: u32,
    pub handle: u32,
    pub bytes: u32,
    pub requested_bytes: u32,
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub source_port: u16,
    pub dest_port: u16,
    pub active_sockets: u32,
    pub max_sockets: u32,
    pub queued_packets: u32,
    pub queue_limit: u32,
    pub payload_max: u32,
    pub message_payload_max: u32,
    pub send_max: u32,
    pub recv_max: u32,
    pub delivered: u64,
    pub drops: u64,
    pub last_error: [u8; 32],
    pub lifecycle_cause: u32,
    pub service_status: u32,
}
impl UdpServiceResult {
    pub const EMPTY: Self = Self {
        magic: UDP_RESULT_MAGIC,
        version: UDP_RESULT_VERSION,
        action: 0,
        result: -1,
        flags: 0,
        handle: 0,
        bytes: 0,
        requested_bytes: 0,
        source_ip: [0; 4],
        dest_ip: [0; 4],
        source_port: 0,
        dest_port: 0,
        active_sockets: 0,
        max_sockets: 0,
        queued_packets: 0,
        queue_limit: 0,
        payload_max: 0,
        message_payload_max: TCP_MESSAGE_PAYLOAD_MAX as u32,
        send_max: 0,
        recv_max: 0,
        delivered: 0,
        drops: 0,
        last_error: [0; 32],
        lifecycle_cause: SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
        service_status: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<UdpServiceResult>() == 128);
    assert!(core::mem::align_of::<UdpServiceResult>() == 8);
    assert!(core::mem::offset_of!(UdpServiceResult, magic) == 0);
    assert!(core::mem::offset_of!(UdpServiceResult, version) == 4);
    assert!(core::mem::offset_of!(UdpServiceResult, action) == 6);
    assert!(core::mem::offset_of!(UdpServiceResult, result) == 8);
    assert!(core::mem::offset_of!(UdpServiceResult, flags) == 12);
    assert!(core::mem::offset_of!(UdpServiceResult, handle) == 16);
    assert!(core::mem::offset_of!(UdpServiceResult, bytes) == 20);
    assert!(core::mem::offset_of!(UdpServiceResult, requested_bytes) == 24);
    assert!(core::mem::offset_of!(UdpServiceResult, source_ip) == 28);
    assert!(core::mem::offset_of!(UdpServiceResult, dest_ip) == 32);
    assert!(core::mem::offset_of!(UdpServiceResult, source_port) == 36);
    assert!(core::mem::offset_of!(UdpServiceResult, dest_port) == 38);
    assert!(core::mem::offset_of!(UdpServiceResult, active_sockets) == 40);
    assert!(core::mem::offset_of!(UdpServiceResult, max_sockets) == 44);
    assert!(core::mem::offset_of!(UdpServiceResult, queued_packets) == 48);
    assert!(core::mem::offset_of!(UdpServiceResult, queue_limit) == 52);
    assert!(core::mem::offset_of!(UdpServiceResult, payload_max) == 56);
    assert!(core::mem::offset_of!(UdpServiceResult, message_payload_max) == 60);
    assert!(core::mem::offset_of!(UdpServiceResult, send_max) == 64);
    assert!(core::mem::offset_of!(UdpServiceResult, recv_max) == 68);
    assert!(core::mem::offset_of!(UdpServiceResult, delivered) == 72);
    assert!(core::mem::offset_of!(UdpServiceResult, drops) == 80);
    assert!(core::mem::offset_of!(UdpServiceResult, last_error) == 88);
    assert!(core::mem::offset_of!(UdpServiceResult, lifecycle_cause) == 120);
    assert!(core::mem::offset_of!(UdpServiceResult, service_status) == 124);
};
impl Wire for UdpServiceResult {
    const SIZE: usize = 128;
    fn encode(&self, out: &mut [u8]) -> usize {
        assert!(out.len() >= Self::SIZE);
        out[..Self::SIZE].fill(0);
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.action.to_le_bytes());
        out[8..12].copy_from_slice(&self.result.to_le_bytes());
        out[12..16].copy_from_slice(&self.flags.to_le_bytes());
        out[16..20].copy_from_slice(&self.handle.to_le_bytes());
        out[20..24].copy_from_slice(&self.bytes.to_le_bytes());
        out[24..28].copy_from_slice(&self.requested_bytes.to_le_bytes());
        out[28..32].copy_from_slice(&self.source_ip);
        out[32..36].copy_from_slice(&self.dest_ip);
        out[36..38].copy_from_slice(&self.source_port.to_le_bytes());
        out[38..40].copy_from_slice(&self.dest_port.to_le_bytes());
        out[40..44].copy_from_slice(&self.active_sockets.to_le_bytes());
        out[44..48].copy_from_slice(&self.max_sockets.to_le_bytes());
        out[48..52].copy_from_slice(&self.queued_packets.to_le_bytes());
        out[52..56].copy_from_slice(&self.queue_limit.to_le_bytes());
        out[56..60].copy_from_slice(&self.payload_max.to_le_bytes());
        out[60..64].copy_from_slice(&self.message_payload_max.to_le_bytes());
        out[64..68].copy_from_slice(&self.send_max.to_le_bytes());
        out[68..72].copy_from_slice(&self.recv_max.to_le_bytes());
        out[72..80].copy_from_slice(&self.delivered.to_le_bytes());
        out[80..88].copy_from_slice(&self.drops.to_le_bytes());
        out[88..120].copy_from_slice(&self.last_error);
        out[120..124].copy_from_slice(&self.lifecycle_cause.to_le_bytes());
        out[124..128].copy_from_slice(&self.service_status.to_le_bytes());
        Self::SIZE
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RestartStatus {
    pub restarts: u64,
    pub last_queued: u32,
    pub last_handles: u32,
    pub last_tombstones: u32,
}
impl RestartStatus {
    pub const EMPTY: Self = Self {
        restarts: 0,
        last_queued: 0,
        last_handles: 0,
        last_tombstones: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<RestartStatus>() == 24);
    assert!(core::mem::align_of::<RestartStatus>() == 8);
    assert!(core::mem::offset_of!(RestartStatus, restarts) == 0);
    assert!(core::mem::offset_of!(RestartStatus, last_queued) == 8);
    assert!(core::mem::offset_of!(RestartStatus, last_handles) == 12);
    assert!(core::mem::offset_of!(RestartStatus, last_tombstones) == 16);
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ServiceBackpressureStatus {
    pub payload_too_large: u64,
    pub response_buffer_small: u64,
    pub send_failures: u64,
    pub stale_skips: u64,
    pub request_payload_limit: u32,
    pub response_buffer_limit: u32,
}
impl ServiceBackpressureStatus {
    pub const EMPTY: Self = Self {
        payload_too_large: 0,
        response_buffer_small: 0,
        send_failures: 0,
        stale_skips: 0,
        request_payload_limit: (TCP_MESSAGE_PAYLOAD_MAX) as u32,
        response_buffer_limit: (ipc::MAX_MESSAGE_SIZE) as u32,
    };
}
const _: () = {
    assert!(core::mem::size_of::<ServiceBackpressureStatus>() == 40);
    assert!(core::mem::align_of::<ServiceBackpressureStatus>() == 8);
    assert!(core::mem::offset_of!(ServiceBackpressureStatus, payload_too_large) == 0);
    assert!(core::mem::offset_of!(ServiceBackpressureStatus, response_buffer_small) == 8);
    assert!(core::mem::offset_of!(ServiceBackpressureStatus, send_failures) == 16);
    assert!(core::mem::offset_of!(ServiceBackpressureStatus, stale_skips) == 24);
    assert!(core::mem::offset_of!(ServiceBackpressureStatus, request_payload_limit) == 32);
    assert!(core::mem::offset_of!(ServiceBackpressureStatus, response_buffer_limit) == 36);
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ServiceLifecycleStatus {
    pub close_ok: u64,
    pub close_failed: u64,
    pub close_cancelled: u64,
    pub cleanup_aborts: u64,
    pub cleanup_poweroff: u64,
    pub cleanup_reboot: u64,
}
impl ServiceLifecycleStatus {
    pub const EMPTY: Self = Self {
        close_ok: 0,
        close_failed: 0,
        close_cancelled: 0,
        cleanup_aborts: 0,
        cleanup_poweroff: 0,
        cleanup_reboot: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<ServiceLifecycleStatus>() == 48);
    assert!(core::mem::align_of::<ServiceLifecycleStatus>() == 8);
    assert!(core::mem::offset_of!(ServiceLifecycleStatus, close_ok) == 0);
    assert!(core::mem::offset_of!(ServiceLifecycleStatus, close_failed) == 8);
    assert!(core::mem::offset_of!(ServiceLifecycleStatus, close_cancelled) == 16);
    assert!(core::mem::offset_of!(ServiceLifecycleStatus, cleanup_aborts) == 24);
    assert!(core::mem::offset_of!(ServiceLifecycleStatus, cleanup_poweroff) == 32);
    assert!(core::mem::offset_of!(ServiceLifecycleStatus, cleanup_reboot) == 40);
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SocketLifecycleCounters {
    pub last_cause: u32,
    pub closed: u64,
    pub reset: u64,
    pub timeout: u64,
    pub peer_gone: u64,
    pub local_abort: u64,
    pub local_close: u64,
    pub pending_close: u64,
    pub would_block: u64,
    pub bad_handle: u64,
    pub owner_mismatch: u64,
    pub dropped: u64,
}
impl SocketLifecycleCounters {
    pub const EMPTY: Self = Self {
        last_cause: SOCKET_LIFECYCLE_UNKNOWN_PUBLIC,
        closed: 0,
        reset: 0,
        timeout: 0,
        peer_gone: 0,
        local_abort: 0,
        local_close: 0,
        pending_close: 0,
        would_block: 0,
        bad_handle: 0,
        owner_mismatch: 0,
        dropped: 0,
    };
}
const _: () = {
    assert!(core::mem::size_of::<SocketLifecycleCounters>() == 96);
    assert!(core::mem::align_of::<SocketLifecycleCounters>() == 8);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, last_cause) == 0);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, closed) == 8);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, reset) == 16);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, timeout) == 24);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, peer_gone) == 32);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, local_abort) == 40);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, local_close) == 48);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, pending_close) == 56);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, would_block) == 64);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, bad_handle) == 72);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, owner_mismatch) == 80);
    assert!(core::mem::offset_of!(SocketLifecycleCounters, dropped) == 88);
};
