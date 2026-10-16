//! Existing network byte order, checksums, address classification and route
//! decisions. These helpers own no packet/global registry or I/O resource.
use super::config;
use crate::sched::sync_types::Text;
use core::slice;
pub(super) fn same_subnet(a: [u8; 4], b: [u8; 4], mask: [u8; 4]) -> bool {
    (0..4).all(|i| a[i] & mask[i] == b[i] & mask[i])
}
pub(super) fn is_limited_broadcast_ip(ip: [u8; 4]) -> bool {
    ip == [255; 4]
}
pub(super) fn is_subnet_broadcast_ip(ip: [u8; 4]) -> bool {
    let cfg = config::settings();
    (0..4).all(|i| ip[i] == (cfg.local_ip[i] & cfg.netmask[i]) | !cfg.netmask[i])
}
pub(super) fn is_multicast_ip(ip: [u8; 4]) -> bool {
    (224..=239).contains(&ip[0])
}
pub(super) fn multicast_mac_for_ip(ip: [u8; 4]) -> [u8; 6] {
    [1, 0, 0x5e, ip[1] & 0x7f, ip[2], ip[3]]
}
pub(super) fn copy_fixed_bytes(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    let len = value.len().min(out.len() - 1);
    out[..len].copy_from_slice(&value[..len]);
}
pub(super) fn fixed_text_equals_ignore_case(fixed: &[u8], value: &[u8]) -> bool {
    let len = fixed.iter().position(|&ch| ch == 0).unwrap_or(fixed.len());
    fixed[..len].eq_ignore_ascii_case(value)
}
pub(super) fn upper_ascii(ch: u8) -> u8 {
    ch.to_ascii_uppercase()
}
pub(super) fn mem_eql(a: &[u8], b: &[u8]) -> bool {
    a == b
}
pub(super) fn mem_contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|part| part == needle)
}
pub(super) fn copy_ip_bytes(dst: &mut [u8], src: [u8; 4]) {
    dst[..4].copy_from_slice(&src);
}
pub(super) fn copy_mac_bytes(dst: &mut [u8], src: [u8; 6]) {
    dst[..6].copy_from_slice(&src);
}
pub(super) fn copy_ip_from_bytes(dst: &mut [u8; 4], src: &[u8]) {
    dst.copy_from_slice(&src[..4]);
}
pub(super) fn copy_mac_from_bytes(dst: &mut [u8; 6], src: &[u8]) {
    dst.copy_from_slice(&src[..6]);
}
pub(super) fn mac_equals(a: [u8; 6], b: [u8; 6]) -> bool {
    a == b
}
pub(super) fn is_zero_ip(ip: [u8; 4]) -> bool {
    ip == [0; 4]
}
pub(super) fn is_zero_mac(mac: [u8; 6]) -> bool {
    mac == [0; 6]
}
pub(super) fn read_be16(buf: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([buf[offset], buf[offset + 1]])
}
pub(super) fn write_be16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}
pub(super) fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += (u32::from(data[i]) << 8) | u32::from(data[i + 1]);
        i += 2;
    }
    if i < data.len() {
        sum += u32::from(data[i]) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    (!sum & 0xffff) as u16
}
pub(super) fn write_be32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RouteDecision {
    pub result: u8,
    pub next_hop_ip: [u8; 4],
    pub dest_mac: [u8; 6],
    pub needs_arp: bool,
    pub last_error: Text,
}
impl RouteDecision {
    const EMPTY: Self = Self {
        result: 0,
        next_hop_ip: [0; 4],
        dest_mac: [0; 6],
        needs_arp: true,
        last_error: Text::literal(b"none"),
    };
}
pub(super) fn route_ipv4_target(target: [u8; 4]) -> RouteDecision {
    if is_zero_ip(target) {
        return RouteDecision {
            result: 6,
            last_error: Text::literal(b"route-zero"),
            ..RouteDecision::EMPTY
        };
    }
    if is_limited_broadcast_ip(target) || is_subnet_broadcast_ip(target) {
        return RouteDecision {
            next_hop_ip: target,
            dest_mac: [255; 6],
            needs_arp: false,
            last_error: Text::literal(b"route-broadcast"),
            ..RouteDecision::EMPTY
        };
    }
    if is_multicast_ip(target) {
        return RouteDecision {
            next_hop_ip: target,
            dest_mac: multicast_mac_for_ip(target),
            needs_arp: false,
            last_error: Text::literal(b"route-multicast"),
            ..RouteDecision::EMPTY
        };
    }
    let cfg = config::settings();
    if same_subnet(cfg.local_ip, target, cfg.netmask) {
        return RouteDecision {
            next_hop_ip: target,
            last_error: Text::literal(b"route-local"),
            ..RouteDecision::EMPTY
        };
    }
    if is_zero_ip(cfg.gateway_ip) {
        return RouteDecision {
            result: 6,
            last_error: Text::literal(b"route-no-gateway"),
            ..RouteDecision::EMPTY
        };
    }
    RouteDecision {
        next_hop_ip: cfg.gateway_ip,
        last_error: Text::literal(b"route-gateway"),
        ..RouteDecision::EMPTY
    }
}
unsafe fn bytes<'a>(value: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value, len) }
    }
}
unsafe fn bytes_mut<'a>(value: *mut u8, len: usize) -> &'a mut [u8] {
    if len == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(value, len) }
    }
}

const _: () = {
    assert!(core::mem::size_of::<RouteDecision>() == 32);
    assert!(core::mem::align_of::<RouteDecision>() == 8);
    assert!(core::mem::offset_of!(RouteDecision, result) == 0);
    assert!(core::mem::offset_of!(RouteDecision, next_hop_ip) == 1);
    assert!(core::mem::offset_of!(RouteDecision, dest_mac) == 5);
    assert!(core::mem::offset_of!(RouteDecision, needs_arp) == 11);
    assert!(core::mem::offset_of!(RouteDecision, last_error) == 16);
};

/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_route_ipv4_target(
    target_ip: *const [u8; 4],
    out: *mut RouteDecision,
) {
    unsafe { out.write(route_ipv4_target(*target_ip)) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_same_subnet(
    a: *const [u8; 4],
    b: *const [u8; 4],
    mask: *const [u8; 4],
) -> bool {
    unsafe { same_subnet(*a, *b, *mask) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_is_limited_broadcast_ip(ip: *const [u8; 4]) -> bool {
    unsafe { is_limited_broadcast_ip(*ip) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_is_subnet_broadcast_ip(ip: *const [u8; 4]) -> bool {
    unsafe { is_subnet_broadcast_ip(*ip) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_is_multicast_ip(ip: *const [u8; 4]) -> bool {
    unsafe { is_multicast_ip(*ip) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_multicast_mac_for_ip(
    ip: *const [u8; 4],
    out: *mut [u8; 6],
) {
    unsafe { out.write(multicast_mac_for_ip(*ip)) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_copy_fixed_bytes(
    out: *mut u8,
    out_len: usize,
    value: *const u8,
    value_len: usize,
) {
    if out_len != 0 {
        unsafe { core::ptr::write_bytes(out, 0, out_len) };
    }
    unsafe { copy_fixed_bytes(bytes_mut(out, out_len), bytes(value, value_len)) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_fixed_text_equals_ignore_case(
    fixed: *const u8,
    fixed_len: usize,
    value: *const u8,
    value_len: usize,
) -> bool {
    unsafe { fixed_text_equals_ignore_case(bytes(fixed, fixed_len), bytes(value, value_len)) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_bytes_upper_ascii(ch: u8) -> u8 {
    upper_ascii(ch)
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_mem_eql(
    a: *const u8,
    a_len: usize,
    b: *const u8,
    b_len: usize,
) -> bool {
    unsafe { mem_eql(bytes(a, a_len), bytes(b, b_len)) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_mem_contains(
    haystack: *const u8,
    haystack_len: usize,
    needle: *const u8,
    needle_len: usize,
) -> bool {
    unsafe { mem_contains(bytes(haystack, haystack_len), bytes(needle, needle_len)) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_copy_ip_bytes(
    dst: *mut u8,
    dst_len: usize,
    src: *const [u8; 4],
) {
    let mut staged = [0; 4];
    unsafe { copy_ip_bytes(&mut staged, *src) };
    assert!(dst_len >= staged.len(), "IPv4 output bounds");
    unsafe { core::ptr::copy_nonoverlapping(staged.as_ptr(), dst, staged.len()) };
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_copy_mac_bytes(
    dst: *mut u8,
    dst_len: usize,
    src: *const [u8; 6],
) {
    let mut staged = [0; 6];
    unsafe { copy_mac_bytes(&mut staged, *src) };
    assert!(dst_len >= staged.len(), "MAC output bounds");
    unsafe { core::ptr::copy_nonoverlapping(staged.as_ptr(), dst, staged.len()) };
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_copy_ip_from_bytes(
    dst: *mut [u8; 4],
    src: *const u8,
    src_len: usize,
) {
    let mut staged = [0; 4];
    unsafe {
        copy_ip_from_bytes(&mut staged, bytes(src, src_len));
        dst.write(staged);
    }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_copy_mac_from_bytes(
    dst: *mut [u8; 6],
    src: *const u8,
    src_len: usize,
) {
    let mut staged = [0; 6];
    unsafe {
        copy_mac_from_bytes(&mut staged, bytes(src, src_len));
        dst.write(staged);
    }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_mac_equals(
    a: *const [u8; 6],
    b: *const [u8; 6],
) -> bool {
    unsafe { mac_equals(*a, *b) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_is_zero_ip(ip: *const [u8; 4]) -> bool {
    unsafe { is_zero_ip(*ip) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_is_zero_mac(mac: *const [u8; 6]) -> bool {
    unsafe { is_zero_mac(*mac) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_read_be16(
    buf: *const u8,
    buf_len: usize,
    offset: usize,
) -> u16 {
    unsafe { read_be16(bytes(buf, buf_len), offset) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_write_be16(
    buf: *mut u8,
    buf_len: usize,
    offset: usize,
    value: u16,
) {
    let mut staged = [0; 2];
    write_be16(&mut staged, 0, value);
    for (index, byte) in staged.into_iter().enumerate() {
        assert!(offset + index < buf_len, "Network u16 output bounds");
        unsafe { buf.add(offset + index).write(byte) };
    }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_internet_checksum(
    data: *const u8,
    data_len: usize,
) -> u16 {
    unsafe { internet_checksum(bytes(data, data_len)) }
}
/// # Safety
/// Exact readable ranges/arrays and exclusive separate writable output;
/// normal original bounds, lifetime and nonaliasing requirements apply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_bytes_write_be32(
    buf: *mut u8,
    buf_len: usize,
    offset: usize,
    value: u32,
) {
    let mut staged = [0; 4];
    write_be32(&mut staged, 0, value);
    for (index, byte) in staged.into_iter().enumerate() {
        assert!(offset + index < buf_len, "Network u32 output bounds");
        unsafe { buf.add(offset + index).write(byte) };
    }
}
