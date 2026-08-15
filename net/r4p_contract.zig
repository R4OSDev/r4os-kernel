pub const ETHERNET_OP_HANDLE_RX: u32 = 1;
pub const ETHERNET_OP_HANDLE_TX: u32 = 2;
pub const ETHERNET_OP_FRAME_TYPE: u32 = 3;
pub const ETHERNET_OP_BUILD_DIAG_FRAME: u32 = 4;

pub const ETHERNET_FLAG_BROADCAST: u32 = 1 << 0;
pub const ETHERNET_FLAG_OWN_UNICAST: u32 = 1 << 1;
pub const ETHERNET_FLAG_IPV4: u32 = 1 << 2;
pub const ETHERNET_FLAG_ARP: u32 = 1 << 3;
pub const ETHERNET_FLAG_R4OS_DIAG: u32 = 1 << 4;
pub const ETHERNET_FLAG_UNKNOWN_TYPE: u32 = 1 << 5;

pub const ETHERNET_RESULT_OK: i32 = 0;
pub const ETHERNET_RESULT_SHORT: i32 = -1;
pub const ETHERNET_RESULT_FILTERED: i32 = -2;
pub const ETHERNET_RESULT_BUFFER_SMALL: i32 = -3;

pub const EthernetFrameOp = extern struct {
    own_mac: [6]u8 = .{0} ** 6,
    source_mac: [6]u8 = .{0} ** 6,
    frame_len: u32 = 0,
    ethertype: u16 = 0,
    result: i32 = 0,
    flags: u32 = 0,
    frame: [1536]u8 = .{0} ** 1536,
};

pub const ARP_OP_HANDLE_RX: u32 = 1;
pub const ARP_OP_HANDLE_TX: u32 = 2;
pub const ARP_OP_BUILD_REQUEST: u32 = 3;

pub const ARP_FLAG_REQUEST: u32 = 1 << 0;
pub const ARP_FLAG_REPLY: u32 = 1 << 1;

pub const ARP_RESULT_OK: i32 = 0;
pub const ARP_RESULT_NOT_ARP: i32 = 1;
pub const ARP_RESULT_SHORT: i32 = -1;
pub const ARP_RESULT_SHAPE: i32 = -2;
pub const ARP_RESULT_OPCODE: i32 = -3;
pub const ARP_RESULT_BUFFER_SMALL: i32 = -4;

pub const ArpOp = extern struct {
    local_ip: [4]u8 = .{0} ** 4,
    source_mac: [6]u8 = .{0} ** 6,
    target_ip: [4]u8 = .{0} ** 4,
    sender_mac: [6]u8 = .{0} ** 6,
    sender_ip: [4]u8 = .{0} ** 4,
    target_mac: [6]u8 = .{0} ** 6,
    seen_target_ip: [4]u8 = .{0} ** 4,
    frame_len: u32 = 0,
    opcode: u16 = 0,
    result: i32 = 0,
    flags: u32 = 0,
    frame: [1536]u8 = .{0} ** 1536,
};

pub const IPV4_OP_HANDLE_RX: u32 = 1;
pub const IPV4_OP_HANDLE_TX: u32 = 2;
pub const IPV4_OP_BUILD_PACKET: u32 = 3;

pub const IPV4_RESULT_OK: i32 = 0;
pub const IPV4_RESULT_NOT_IPV4: i32 = 1;
pub const IPV4_RESULT_SHORT: i32 = -1;
pub const IPV4_RESULT_VERSION: i32 = -2;
pub const IPV4_RESULT_LENGTH: i32 = -3;
pub const IPV4_RESULT_FRAGMENT: i32 = -4;
pub const IPV4_RESULT_CHECKSUM: i32 = -5;
pub const IPV4_RESULT_DESTINATION: i32 = -6;
pub const IPV4_RESULT_BUFFER_SMALL: i32 = -7;

pub const Ipv4Op = extern struct {
    local_ip: [4]u8 = .{0} ** 4,
    source_mac: [6]u8 = .{0} ** 6,
    dest_mac: [6]u8 = .{0} ** 6,
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    protocol: u8 = 0,
    ttl: u8 = 64,
    frame_len: u32 = 0,
    payload_len: u32 = 0,
    ethertype: u16 = 0,
    result: i32 = 0,
    flags: u32 = 0,
    frame: [1536]u8 = .{0} ** 1536,
    payload: [1536]u8 = .{0} ** 1536,
};

pub const UDP_OP_HANDLE_RX: u32 = 1;
pub const UDP_OP_HANDLE_TX: u32 = 2;
pub const UDP_OP_BUILD_DATAGRAM: u32 = 3;

pub const UDP_RESULT_OK: i32 = 0;
pub const UDP_RESULT_NOT_UDP: i32 = 1;
pub const UDP_RESULT_SHORT: i32 = -1;
pub const UDP_RESULT_LENGTH: i32 = -2;
pub const UDP_RESULT_CHECKSUM: i32 = -3;
pub const UDP_RESULT_BUFFER_SMALL: i32 = -4;

pub const UdpOp = extern struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    length: u16 = 0,
    payload_len: u32 = 0,
    datagram_len: u32 = 0,
    result: i32 = 0,
    flags: u32 = 0,
    datagram: [1536]u8 = .{0} ** 1536,
    payload: [1536]u8 = .{0} ** 1536,
};

pub const ICMP_OP_HANDLE_RX: u32 = 1;
pub const ICMP_OP_HANDLE_TX: u32 = 2;
pub const ICMP_OP_BUILD_ECHO_REQUEST: u32 = 3;
pub const ICMP_OP_BUILD_ECHO_REPLY: u32 = 4;
pub const ICMP_OP_IS_ECHO_REQUEST: u32 = 5;

pub const ICMP_FLAG_ECHO_REQUEST: u32 = 1 << 0;
pub const ICMP_FLAG_ECHO_REPLY: u32 = 1 << 1;

pub const ICMP_RESULT_OK: i32 = 0;
pub const ICMP_RESULT_NOT_ICMP: i32 = 1;
pub const ICMP_RESULT_SHORT: i32 = -1;
pub const ICMP_RESULT_CHECKSUM: i32 = -2;
pub const ICMP_RESULT_BUFFER_SMALL: i32 = -3;

pub const IcmpOp = extern struct {
    ident: u16 = 0,
    seq: u16 = 0,
    payload_len: u32 = 0,
    result: i32 = 0,
    typ: u8 = 0,
    code: u8 = 0,
    reserved: u16 = 0,
    flags: u32 = 0,
    payload: [1536]u8 = .{0} ** 1536,
};

pub const DHCP_OP_BUILD_DISCOVER: u32 = 1;
pub const DHCP_OP_BUILD_REQUEST: u32 = 2;
pub const DHCP_OP_HANDLE_MESSAGE: u32 = 3;
pub const DHCP_OP_BUILD_RELEASE: u32 = 4;

pub const DHCP_FLAG_DISCOVER: u32 = 1 << 0;
pub const DHCP_FLAG_OFFER: u32 = 1 << 1;
pub const DHCP_FLAG_REQUEST: u32 = 1 << 2;
pub const DHCP_FLAG_ACK: u32 = 1 << 3;
pub const DHCP_FLAG_BOUND: u32 = 1 << 4;
pub const DHCP_FLAG_NAK: u32 = 1 << 5;
pub const DHCP_FLAG_RELEASE: u32 = 1 << 6;

pub const DHCP_RESULT_OK: i32 = 0;
pub const DHCP_RESULT_IGNORED: i32 = 1;
pub const DHCP_RESULT_SHAPE: i32 = -1;
pub const DHCP_RESULT_NO_TYPE: i32 = -2;
pub const DHCP_RESULT_BUFFER_SMALL: i32 = -3;

pub const DhcpOp = extern struct {
    xid: u32 = 0,
    mac: [6]u8 = .{0} ** 6,
    requested_ip: [4]u8 = .{0} ** 4,
    server_ip: [4]u8 = .{0} ** 4,
    offered_ip: [4]u8 = .{0} ** 4,
    client_ip: [4]u8 = .{0} ** 4,
    netmask: [4]u8 = .{ 255, 255, 255, 0 },
    gateway_ip: [4]u8 = .{0} ** 4,
    dns_ip: [4]u8 = .{0} ** 4,
    lease_seconds: u32 = 0,
    renew_seconds: u32 = 0,
    rebind_seconds: u32 = 0,
    message_type: u8 = 0,
    dns_configured: u8 = 0,
    reserved: [2]u8 = .{0} ** 2,
    payload_len: u32 = 0,
    result: i32 = 0,
    flags: u32 = 0,
    payload: [1536]u8 = .{0} ** 1536,
};

pub const DNS_OP_BUILD_A_QUERY: u32 = 1;
pub const DNS_OP_HANDLE_RESPONSE: u32 = 2;

pub const DNS_FLAG_A_RECORD: u32 = 1 << 0;

pub const DNS_RESULT_OK: i32 = 0;
pub const DNS_RESULT_SHORT: i32 = -1;
pub const DNS_RESULT_HEADER: i32 = -2;
pub const DNS_RESULT_QNAME: i32 = -3;
pub const DNS_RESULT_QUESTION: i32 = -4;
pub const DNS_RESULT_ANAME: i32 = -5;
pub const DNS_RESULT_ANSWER: i32 = -6;
pub const DNS_RESULT_ATYPE: i32 = -7;
pub const DNS_RESULT_BUFFER_SMALL: i32 = -8;
pub const DNS_RESULT_NAME: i32 = -9;
pub const DNS_RESULT_NXDOMAIN: i32 = -10;
pub const DNS_RESULT_TIMEOUT: i32 = -11;

pub const DnsOp = extern struct {
    id: u16 = 0,
    name_len: u16 = 0,
    payload_len: u32 = 0,
    result: i32 = 0,
    flags: u32 = 0,
    answer: [4]u8 = .{0} ** 4,
    name: [256]u8 = .{0} ** 256,
    payload: [1536]u8 = .{0} ** 1536,
};

pub const TCP_OP_CONNECT: u32 = 1;
pub const TCP_OP_WRITE: u32 = 2;
pub const TCP_OP_READ: u32 = 3;
pub const TCP_OP_CLOSE: u32 = 4;
pub const TCP_OP_SUMMARY: u32 = 5;
pub const TCP_OP_CONNECTION_INFO: u32 = 6;
pub const TCP_OP_HANDLE_RX: u32 = 7;
pub const TCP_OP_HANDLE_TX: u32 = 8;
pub const TCP_OP_BUILD_SEGMENT: u32 = 9;

pub const TCP_FLAG_FIN: u16 = 0x001;
pub const TCP_FLAG_SYN: u16 = 0x002;
pub const TCP_FLAG_RST: u16 = 0x004;
pub const TCP_FLAG_PSH: u16 = 0x008;
pub const TCP_FLAG_ACK: u16 = 0x010;

pub const TCP_RESULT_OK: i32 = 0;
pub const TCP_RESULT_NOT_TCP: i32 = 1;
pub const TCP_RESULT_NO_CONNECTION: i32 = -1;
pub const TCP_RESULT_BAD_STATE: i32 = -2;
pub const TCP_RESULT_BUFFER_SMALL: i32 = -3;
pub const TCP_RESULT_SHORT: i32 = -4;
pub const TCP_RESULT_CHECKSUM: i32 = -5;

pub const TcpR4pSummary = extern struct {
    max_connections: u32 = 0,
    active_connections: u32 = 0,
    buffer_size: u32 = 0,
    syn_tx: u64 = 0,
    synack_rx: u64 = 0,
    ack_tx: u64 = 0,
    data_tx: u64 = 0,
    data_rx: u64 = 0,
    fin_tx: u64 = 0,
    rst_rx: u64 = 0,
    checksum_errors: u64 = 0,
    timeouts: u64 = 0,
    self_tests: u64 = 0,
};

pub const TcpR4pConnectionInfo = extern struct {
    id: u32 = 0,
    state: u8 = 0,
    local_port: u16 = 0,
    remote_port: u16 = 0,
    remote_ip: [4]u8 = .{0} ** 4,
    tx_bytes: u64 = 0,
    rx_bytes: u64 = 0,
    pending_rx: u32 = 0,
};

pub const TcpOp = extern struct {
    source_ip: [4]u8 = .{0} ** 4,
    dest_ip: [4]u8 = .{0} ** 4,
    remote_ip: [4]u8 = .{0} ** 4,
    conn_id: u32 = 0,
    index: u32 = 0,
    local_port: u16 = 0,
    remote_port: u16 = 0,
    source_port: u16 = 0,
    dest_port: u16 = 0,
    seq: u32 = 0,
    ack: u32 = 0,
    flags: u16 = 0,
    reserved: u16 = 0,
    payload_len: u32 = 0,
    segment_len: u32 = 0,
    result: i32 = 0,
    summary: TcpR4pSummary = .{},
    info: TcpR4pConnectionInfo = .{},
    payload: [1536]u8 = .{0} ** 1536,
    segment: [1536]u8 = .{0} ** 1536,
};

pub const R4SL_OP_BUILD_FRAME: u32 = 1;
pub const R4SL_OP_PARSE_BYTES: u32 = 2;
pub const R4SL_OP_RESET_PARSER: u32 = 3;
pub const R4SL_OP_SUMMARY: u32 = 5;

pub const R4SL_RESULT_OK: i32 = 0;
pub const R4SL_RESULT_NEED_MORE: i32 = 1;
pub const R4SL_RESULT_BAD_MAGIC: i32 = -1;
pub const R4SL_RESULT_BAD_VERSION: i32 = -2;
pub const R4SL_RESULT_BAD_LENGTH: i32 = -3;
pub const R4SL_RESULT_CHECKSUM: i32 = -4;
pub const R4SL_RESULT_OVERFLOW: i32 = -5;
pub const R4SL_RESULT_BUFFER_SMALL: i32 = -6;
pub const R4SL_RESULT_BAD_STATE: i32 = -7;

pub const R4slSummary = extern struct {
    parsed_frames: u64 = 0,
    built_frames: u64 = 0,
    loopback_tests: u64 = 0,
    bad_magic: u64 = 0,
    bad_version: u64 = 0,
    bad_length: u64 = 0,
    checksum_errors: u64 = 0,
    overflows: u64 = 0,
};

pub const R4slOp = extern struct {
    frame_type: u8 = 0,
    completed: u8 = 0,
    reserved0: u16 = 0,
    payload_len: u32 = 0,
    frame_len: u32 = 0,
    input_len: u32 = 0,
    result: i32 = 0,
    summary: R4slSummary = .{},
    payload: [256]u8 = .{0} ** 256,
    frame: [266]u8 = .{0} ** 266,
    input: [266]u8 = .{0} ** 266,
};

pub const HID_REPORT_MAX_DESCRIPTOR: usize = 512;
pub const HID_REPORT_MAX_FIELDS: usize = 24;
pub const HID_REPORT_MAX_FIELD_USAGES: usize = 8;

pub const HID_REPORT_OP_PARSE: u32 = 1;

pub const HID_REPORT_RESULT_OK: i32 = 0;
pub const HID_REPORT_RESULT_BUFFER_SMALL: i32 = -1;
pub const HID_REPORT_RESULT_BAD_LENGTH: i32 = -2;

pub const HID_REPORT_KIND_INPUT: u8 = 0;
pub const HID_REPORT_KIND_OUTPUT: u8 = 1;
pub const HID_REPORT_KIND_FEATURE: u8 = 2;

pub const HID_REPORT_REASON_PARSED: u16 = 0;
pub const HID_REPORT_REASON_NOT_PARSED: u16 = 1;
pub const HID_REPORT_REASON_TRUNCATED_LONG_ITEM: u16 = 2;
pub const HID_REPORT_REASON_TRUNCATED_LONG_PAYLOAD: u16 = 3;
pub const HID_REPORT_REASON_TRUNCATED_SHORT_ITEM: u16 = 4;

pub const HidReportField = extern struct {
    kind: u8 = HID_REPORT_KIND_INPUT,
    usage_page: u16 = 0xFFFF,
    usage_min: u32 = 0,
    usage_max: u32 = 0,
    usages: [HID_REPORT_MAX_FIELD_USAGES]u32 = .{0} ** HID_REPORT_MAX_FIELD_USAGES,
    usage_count: u8 = 0,
    report_id: u8 = 0,
    bit_offset: u16 = 0,
    bit_size: u8 = 0,
    count: u8 = 0,
    logical_min: i32 = 0,
    logical_max: i32 = 0,
    flags: u8 = 0,
    relative: u8 = 0,
    variable: u8 = 0,
    constant: u8 = 0,
    reserved: u8 = 0,
};

pub const HidReportSummary = extern struct {
    parsed: u8 = 0,
    malformed: u8 = 0,
    has_report_id: u8 = 0,
    report_ids: u8 = 0,
    input_fields: u8 = 0,
    output_fields: u8 = 0,
    feature_fields: u8 = 0,
    field_count: u8 = 0,
    input_bits: [16]u16 = .{0} ** 16,
    usage_keyboard: u8 = 0,
    usage_mouse: u8 = 0,
    usage_pointer: u8 = 0,
    usage_x: u8 = 0,
    usage_y: u8 = 0,
    usage_wheel: u8 = 0,
    usage_buttons: u8 = 0,
    reserved0: u8 = 0,
    reason_code: u16 = HID_REPORT_REASON_NOT_PARSED,
    reserved1: u16 = 0,
    fields: [HID_REPORT_MAX_FIELDS]HidReportField = .{HidReportField{}} ** HID_REPORT_MAX_FIELDS,
};

pub const HidReportOp = extern struct {
    descriptor_len: u32 = 0,
    result: i32 = 0,
    summary: HidReportSummary = .{},
    descriptor: [HID_REPORT_MAX_DESCRIPTOR]u8 = .{0} ** HID_REPORT_MAX_DESCRIPTOR,
};

pub const USB_HID_BOOT_MAX_REPORT: usize = 32;
pub const USB_HID_BOOT_MAX_KEYS: usize = 8;

pub const USB_HID_BOOT_OP_CLASSIFY_INTERFACE: u32 = 1;
pub const USB_HID_BOOT_OP_DECODE_KEYBOARD: u32 = 2;
pub const USB_HID_BOOT_OP_DECODE_MOUSE: u32 = 3;

pub const USB_HID_BOOT_RESULT_OK: i32 = 0;
pub const USB_HID_BOOT_RESULT_IGNORED: i32 = 1;
pub const USB_HID_BOOT_RESULT_SHORT: i32 = -1;
pub const USB_HID_BOOT_RESULT_BAD_INTERFACE: i32 = -2;

pub const USB_HID_BOOT_KIND_NONE: u8 = 0;
pub const USB_HID_BOOT_KIND_KEYBOARD: u8 = 1;
pub const USB_HID_BOOT_KIND_MOUSE: u8 = 2;

pub const USB_HID_BOOT_FLAG_REPORT_ID_HEURISTIC: u32 = 1 << 0;

pub const UsbHidBootOp = extern struct {
    class_code: u8 = 0,
    subclass: u8 = 0,
    protocol: u8 = 0,
    endpoint_address: u8 = 0,
    endpoint_attributes: u8 = 0,
    endpoint_max_packet: u16 = 0,
    protocol_ok: u8 = 0,
    kind: u8 = USB_HID_BOOT_KIND_NONE,
    report_len: u8 = 0,
    previous_len: u8 = 0,
    report_offset: u8 = 0,
    previous_offset: u8 = 0,
    reserved0: u16 = 0,
    result: i32 = 0,
    flags: u32 = 0,
    old_modifiers: u8 = 0,
    new_modifiers: u8 = 0,
    modifiers_pressed: u8 = 0,
    modifiers_released: u8 = 0,
    key_count: u8 = 0,
    mouse_buttons: u8 = 0,
    reserved1: u16 = 0,
    mouse_dx: i32 = 0,
    mouse_dy: i32 = 0,
    mouse_wheel: i32 = 0,
    keys: [USB_HID_BOOT_MAX_KEYS]u8 = .{0} ** USB_HID_BOOT_MAX_KEYS,
    report: [USB_HID_BOOT_MAX_REPORT]u8 = .{0} ** USB_HID_BOOT_MAX_REPORT,
    previous: [USB_HID_BOOT_MAX_REPORT]u8 = .{0} ** USB_HID_BOOT_MAX_REPORT,
};

pub const USB_MSC_BOT_CBW_LEN: usize = 31;
pub const USB_MSC_BOT_CSW_LEN: usize = 13;
pub const USB_MSC_BOT_MAX_CDB: usize = 16;

pub const USB_MSC_BOT_OP_BUILD_CBW: u32 = 1;
pub const USB_MSC_BOT_OP_PARSE_CSW: u32 = 2;

pub const USB_MSC_BOT_RESULT_OK: i32 = 0;
pub const USB_MSC_BOT_RESULT_COMMAND_FAILED: i32 = 1;
pub const USB_MSC_BOT_RESULT_BAD_CDB: i32 = -1;
pub const USB_MSC_BOT_RESULT_BAD_CSW: i32 = -2;
pub const USB_MSC_BOT_RESULT_TAG_MISMATCH: i32 = -3;
pub const USB_MSC_BOT_RESULT_RESIDUE: i32 = -4;
pub const USB_MSC_BOT_RESULT_PHASE_ERROR: i32 = -5;
pub const USB_MSC_BOT_RESULT_UNSUPPORTED_STATUS: i32 = -6;

pub const USB_MSC_BOT_DIR_NONE: u8 = 0;
pub const USB_MSC_BOT_DIR_IN: u8 = 1;
pub const USB_MSC_BOT_DIR_OUT: u8 = 2;

pub const UsbMscBotOp = extern struct {
    tag: u32 = 0,
    transfer_len: u32 = 0,
    residue: u32 = 0,
    cdb_len: u8 = 0,
    direction: u8 = USB_MSC_BOT_DIR_NONE,
    lun: u8 = 0,
    status: u8 = 0xFF,
    result: i32 = 0,
    csw_tag: u32 = 0,
    cdb: [USB_MSC_BOT_MAX_CDB]u8 = .{0} ** USB_MSC_BOT_MAX_CDB,
    cbw: [USB_MSC_BOT_CBW_LEN]u8 = .{0} ** USB_MSC_BOT_CBW_LEN,
    csw: [USB_MSC_BOT_CSW_LEN]u8 = .{0} ** USB_MSC_BOT_CSW_LEN,
};

pub const USB_SCSI_MAX_CDB: usize = 16;
pub const USB_SCSI_MAX_DATA: usize = 64;

pub const USB_SCSI_OP_BUILD_INQUIRY: u32 = 1;
pub const USB_SCSI_OP_BUILD_TEST_UNIT_READY: u32 = 2;
pub const USB_SCSI_OP_BUILD_REQUEST_SENSE: u32 = 3;
pub const USB_SCSI_OP_BUILD_READ_CAPACITY10: u32 = 4;
pub const USB_SCSI_OP_BUILD_MODE_SENSE6: u32 = 5;
pub const USB_SCSI_OP_BUILD_READ10: u32 = 6;
pub const USB_SCSI_OP_BUILD_WRITE10: u32 = 7;
pub const USB_SCSI_OP_BUILD_SYNC_CACHE10: u32 = 8;
pub const USB_SCSI_OP_PARSE_SENSE: u32 = 9;
pub const USB_SCSI_OP_PARSE_CAPACITY10: u32 = 10;
pub const USB_SCSI_OP_PARSE_MODE_SENSE6: u32 = 11;

pub const USB_SCSI_RESULT_OK: i32 = 0;
pub const USB_SCSI_RESULT_BAD_PARAM: i32 = -1;
pub const USB_SCSI_RESULT_BAD_RESPONSE: i32 = -2;
pub const USB_SCSI_RESULT_UNSUPPORTED: i32 = -3;

pub const USB_SCSI_DIR_NONE: u8 = 0;
pub const USB_SCSI_DIR_IN: u8 = 1;
pub const USB_SCSI_DIR_OUT: u8 = 2;

pub const UsbScsiBlockOp = extern struct {
    lba: u32 = 0,
    sectors: u16 = 0,
    allocation_len: u32 = 0,
    transfer_len: u32 = 0,
    sector_count: u64 = 0,
    sector_size: u32 = 0,
    cdb_len: u8 = 0,
    direction: u8 = USB_SCSI_DIR_NONE,
    result: i32 = 0,
    failed_opcode: u8 = 0,
    sense_key: u8 = 0,
    sense_asc: u8 = 0,
    sense_ascq: u8 = 0,
    write_protected_known: u8 = 0,
    write_protected: u8 = 0,
    reserved: [2]u8 = .{0} ** 2,
    cdb: [USB_SCSI_MAX_CDB]u8 = .{0} ** USB_SCSI_MAX_CDB,
    data: [USB_SCSI_MAX_DATA]u8 = .{0} ** USB_SCSI_MAX_DATA,
};

pub const AUDIO_MIDI_OP_CLASSIFY_EVENT: u32 = 1;

pub const AUDIO_MIDI_RESULT_OK: i32 = 0;
pub const AUDIO_MIDI_RESULT_BAD_EVENT: i32 = -1;
pub const AUDIO_MIDI_RESULT_UNSUPPORTED: i32 = -2;

pub const AUDIO_MIDI_EVENT_IGNORE: u8 = 0;
pub const AUDIO_MIDI_EVENT_NOTE_OFF: u8 = 1;
pub const AUDIO_MIDI_EVENT_NOTE_ON: u8 = 2;
pub const AUDIO_MIDI_EVENT_CONTROL: u8 = 3;
pub const AUDIO_MIDI_EVENT_PROGRAM: u8 = 4;
pub const AUDIO_MIDI_EVENT_CHANNEL_PRESSURE: u8 = 5;
pub const AUDIO_MIDI_EVENT_PITCH_BEND: u8 = 6;

pub const AudioMidiOp = extern struct {
    channel: u8 = 0,
    status: u8 = 0,
    data1: u8 = 0,
    data2: u8 = 0,
    event: u8 = AUDIO_MIDI_EVENT_IGNORE,
    normalized_status: u8 = 0,
    note: u8 = 0,
    velocity: u8 = 0,
    controller: u8 = 0,
    value: u8 = 0,
    program: u8 = 0,
    result: i32 = 0,
};

pub const AUDIO_OPL3_OP_RESET: u32 = 1;
pub const AUDIO_OPL3_OP_WRITE_REGISTER: u32 = 2;
pub const AUDIO_OPL3_OP_MIDI_EVENT: u32 = 3;

pub const AUDIO_OPL3_RESULT_OK: i32 = 0;
pub const AUDIO_OPL3_RESULT_BAD_REGISTER: i32 = -1;
pub const AUDIO_OPL3_RESULT_BAD_EVENT: i32 = -2;
pub const AUDIO_OPL3_RESULT_UNSUPPORTED: i32 = -3;

pub const AUDIO_OPL3_WRITE_OTHER: u8 = 0;
pub const AUDIO_OPL3_WRITE_GLOBAL: u8 = 1;
pub const AUDIO_OPL3_WRITE_OPERATOR: u8 = 2;
pub const AUDIO_OPL3_WRITE_CHANNEL: u8 = 3;

pub const AUDIO_OPL3_ACTION_IGNORE: u8 = 0;
pub const AUDIO_OPL3_ACTION_NOTE_ON: u8 = 1;
pub const AUDIO_OPL3_ACTION_NOTE_OFF: u8 = 2;
pub const AUDIO_OPL3_ACTION_PROGRAM: u8 = 3;
pub const AUDIO_OPL3_ACTION_CONTROL: u8 = 4;
pub const AUDIO_OPL3_ACTION_ALL_NOTES_OFF: u8 = 5;

pub const AudioOpl3Op = extern struct {
    bank: u8 = 0,
    register: u8 = 0,
    value: u8 = 0,
    channel: u8 = 0,
    status: u8 = 0,
    data1: u8 = 0,
    data2: u8 = 0,
    write_kind: u8 = AUDIO_OPL3_WRITE_OTHER,
    action: u8 = AUDIO_OPL3_ACTION_IGNORE,
    normalized_status: u8 = 0,
    note: u8 = 0,
    velocity: u8 = 0,
    controller: u8 = 0,
    program: u8 = 0,
    result: i32 = 0,
};

pub const AUDIO_SID_OP_CONFIGURE_MODEL: u32 = 1;
pub const AUDIO_SID_OP_WRITE_REGISTER: u32 = 2;
pub const AUDIO_SID_OP_RESOLVE_IO: u32 = 3;

pub const AUDIO_SID_RESULT_OK: i32 = 0;
pub const AUDIO_SID_RESULT_BAD_MODEL: i32 = -1;
pub const AUDIO_SID_RESULT_BAD_REGISTER: i32 = -2;
pub const AUDIO_SID_RESULT_BAD_ADDRESS: i32 = -3;
pub const AUDIO_SID_RESULT_UNSUPPORTED: i32 = -4;

pub const AUDIO_SID_MODEL_8580: u8 = 1;
pub const AUDIO_SID_MODEL_6581: u8 = 2;

pub const AUDIO_SID_REGISTER_OTHER: u8 = 0;
pub const AUDIO_SID_REGISTER_VOICE: u8 = 1;
pub const AUDIO_SID_REGISTER_FILTER: u8 = 2;
pub const AUDIO_SID_REGISTER_VOLUME: u8 = 3;
pub const AUDIO_SID_REGISTER_READBACK: u8 = 4;

pub const AudioSidOp = extern struct {
    model: u8 = AUDIO_SID_MODEL_8580,
    address: u16 = 0,
    register: u8 = 0,
    value: u8 = 0,
    kind: u8 = AUDIO_SID_REGISTER_OTHER,
    voice: u8 = 0,
    mirrored: u8 = 0,
    result: i32 = 0,
};
