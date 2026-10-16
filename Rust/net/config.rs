//! Sole network configuration state. Short network admission protects copies;
//! boot-option callbacks run before it, without a live state reference.
use super::config_writer::Addresses;
use crate::{memory::owner::Guard, sched::sync_types::Text};
use core::{cell::UnsafeCell, ffi::c_void};
pub const DEFAULT_LOCAL_IP: [u8; 4] = [10, 0, 2, 15];
pub const DEFAULT_NETMASK: [u8; 4] = [255, 255, 255, 0];
pub const DEFAULT_GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];
pub const DEFAULT_DNS_IP: [u8; 4] = [10, 0, 2, 3];
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Settings {
    pub local_ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub dns_ip: [u8; 4],
    pub dns_configured: bool,
    pub dhcp_enabled: bool,
    pub dhcp_bound: bool,
    pub configured: bool,
    pub invalid_options: u64,
    pub last_error: Text,
}
impl Settings {
    pub const DEFAULT: Self = Self {
        local_ip: DEFAULT_LOCAL_IP,
        netmask: DEFAULT_NETMASK,
        gateway_ip: DEFAULT_GATEWAY_IP,
        dns_ip: DEFAULT_DNS_IP,
        dns_configured: false,
        dhcp_enabled: false,
        dhcp_bound: false,
        configured: false,
        invalid_options: 0,
        last_error: Text::literal(b"default"),
    };
    fn clear_lease(&mut self, reason: Text) {
        self.local_ip = [0; 4];
        self.netmask = DEFAULT_NETMASK;
        self.gateway_ip = [0; 4];
        self.dns_ip = [0; 4];
        self.dns_configured = false;
        self.dhcp_bound = false;
        self.configured = true;
        self.invalid_options = 0;
        self.last_error = reason;
    }
}
struct Storage(UnsafeCell<Settings>);
// SAFETY: every access is a bounded copy under the common network owner.
unsafe impl Sync for Storage {}
static CURRENT: Storage = Storage(UnsafeCell::new(Settings::DEFAULT));
pub fn settings() -> Settings {
    let _guard = Guard::acquire(2);
    unsafe { CURRENT.0.get().read() }
}
pub fn restore(saved: Settings) {
    let _guard = Guard::acquire(2);
    unsafe { CURRENT.0.get().write(saved) };
}
pub fn reset() {
    restore(Settings::DEFAULT);
}
pub fn parse_ipv4(value: &[u8]) -> Option<[u8; 4]> {
    let mut out = [0; 4];
    let mut part = 0;
    let mut accum = 0u16;
    let mut digits = 0;
    for &ch in value {
        if ch.is_ascii_digit() {
            accum = accum * 10 + u16::from(ch - b'0');
            if accum > 255 {
                return None;
            }
            digits += 1;
            if digits > 3 {
                return None;
            }
        } else if ch == b'.' {
            if digits == 0 || part >= 3 {
                return None;
            }
            out[part] = accum as u8;
            part += 1;
            accum = 0;
            digits = 0;
        } else {
            return None;
        }
    }
    if digits == 0 || part != 3 {
        return None;
    }
    out[part] = accum as u8;
    Some(out)
}
fn parse_bool(value: &[u8]) -> Option<bool> {
    if [b"on".as_slice(), b"yes", b"true", b"1"]
        .iter()
        .any(|v| value.eq_ignore_ascii_case(v))
    {
        Some(true)
    } else if [b"off".as_slice(), b"no", b"false", b"0"]
        .iter()
        .any(|v| value.eq_ignore_ascii_case(v))
    {
        Some(false)
    } else {
        None
    }
}
unsafe extern "C" {
    fn r4os_zig_net_boot_option(
        config: *const c_void,
        key: *const u8,
        key_len: usize,
        out: *mut Text,
    ) -> bool;
}
unsafe fn option<'a>(config: *const c_void, key: &[u8]) -> Option<&'a [u8]> {
    let mut out = Text::literal(b"");
    if unsafe { r4os_zig_net_boot_option(config, key.as_ptr(), key.len(), &mut out) } {
        Some(if out.len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(out.pointer, out.len) }
        })
    } else {
        None
    }
}
/// # Safety
/// Config is the boot owner's immutable exact instance through this call.
pub unsafe fn apply_boot_config(config: *const c_void) {
    apply_options(|key| unsafe { option(config, key) });
}
// Shared original option application; the existing malformed-config fixture
// supplies the same single option without assuming the private Zig Config ABI.
pub(super) fn apply_options<'a>(mut option: impl FnMut(&[u8]) -> Option<&'a [u8]>) {
    let mut next = Settings::DEFAULT;
    if let Some(value) = option(b"dhcp") {
        if let Some(enabled) = parse_bool(value) {
            next.dhcp_enabled = enabled;
            if enabled {
                next.configured = true;
            }
        } else {
            next.invalid_options += 1;
            next.last_error = Text::literal(b"bad-dhcp");
        }
    }
    for (group, keys) in [
        [b"ip".as_slice(), b"addr", b"address"].as_slice(),
        [b"mask".as_slice(), b"netmask"].as_slice(),
        [b"gateway".as_slice(), b"gw"].as_slice(),
        [b"dns".as_slice(), b"nameserver"].as_slice(),
    ]
    .iter()
    .enumerate()
    {
        for key in *keys {
            if let Some(value) = option(key) {
                if let Some(ip) = parse_ipv4(value) {
                    match group {
                        0 => next.local_ip = ip,
                        1 => next.netmask = ip,
                        2 => next.gateway_ip = ip,
                        3 => {
                            next.dns_ip = ip;
                            next.dns_configured = true;
                        }
                        _ => unreachable!(),
                    };
                    next.configured = true;
                } else {
                    next.invalid_options += 1;
                    next.last_error = Text::literal(b"bad-ipv4");
                }
                break;
            }
        }
    }
    if next.invalid_options == 0 {
        next.last_error = Text::literal(if next.configured {
            b"config"
        } else {
            b"default"
        });
    }
    if next.dhcp_enabled {
        next.clear_lease(Text::literal(b"dhcp-pending"));
    }
    restore(next);
}
pub fn apply_addresses(addresses: Addresses, dhcp: bool) {
    let _guard = Guard::acquire(2);
    let current = unsafe { &mut *CURRENT.0.get() };
    current.local_ip = addresses.local_ip;
    current.netmask = addresses.netmask;
    current.gateway_ip = addresses.gateway_ip;
    current.dns_ip = addresses.dns_ip;
    current.dns_configured = addresses.dns_configured;
    current.dhcp_enabled = dhcp;
    current.dhcp_bound = dhcp;
    current.configured = true;
    current.invalid_options = 0;
    current.last_error = Text::literal(if dhcp { b"dhcp" } else { b"runtime" });
}
pub fn clear_dhcp_lease() {
    let _guard = Guard::acquire(2);
    let current = unsafe { &mut *CURRENT.0.get() };
    current.dhcp_enabled = false;
    current.clear_lease(Text::literal(b"dhcp-released"));
}
/// # Safety
/// The diagnostic reason has the original stable kernel-string lifetime.
pub unsafe fn clear_preserving_mode(reason: Text) {
    let _guard = Guard::acquire(2);
    unsafe { (*CURRENT.0.get()).clear_lease(reason) };
}
pub fn enable_dhcp() {
    let _guard = Guard::acquire(2);
    let current = unsafe { &mut *CURRENT.0.get() };
    current.dhcp_enabled = true;
    current.configured = true;
    if !current.dhcp_bound {
        current.clear_lease(Text::literal(b"dhcp-pending"));
    }
}
pub fn source_name() -> Text {
    let current = settings();
    if current.invalid_options > 0 {
        return current.last_error;
    }
    if current.dhcp_enabled && current.dhcp_bound {
        return Text::literal(b"dhcp");
    }
    if current.dhcp_enabled {
        return Text::literal(b"dhcp-pending");
    }
    let last = if current.last_error.len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(current.last_error.pointer, current.last_error.len) }
    };
    if last.eq_ignore_ascii_case(b"dhcp-released") {
        return Text::literal(b"released");
    }
    Text::literal(if current.configured {
        b"config"
    } else {
        b"default"
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_config_reset() {
    reset();
}
/// # Safety
/// Exact writable paired settings copy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_settings(out: *mut Settings) {
    unsafe { out.write(settings()) };
}
/// # Safety
/// Readable paired settings; borrowed diagnostic text retains its kernel lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_restore(saved: *const Settings) {
    restore(unsafe { *saved });
}
/// # Safety
/// Immutable boot configuration owned by the existing boot loader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_boot(config: *const c_void) {
    unsafe { apply_boot_config(config) };
}
/// # Safety
/// Readable fixed address record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_addresses(addresses: *const Addresses, dhcp: bool) {
    apply_addresses(unsafe { *addresses }, dhcp);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_config_clear() {
    clear_dhcp_lease();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_config_enable() {
    enable_dhcp();
}
/// # Safety
/// Exact reason text with the stable kernel-string lifetime of the old API.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_clear_preserving(reason: *const Text) {
    unsafe { clear_preserving_mode(*reason) };
}
/// # Safety
/// Exact writable text record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_source(out: *mut Text) {
    unsafe { out.write(source_name()) };
}
/// # Safety
/// Readable value and separate writable IPv4 output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_parse(
    value: *const u8,
    len: usize,
    out: *mut [u8; 4],
) -> bool {
    let value = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(value, len) }
    };
    if let Some(ip) = parse_ipv4(value) {
        unsafe { out.write(ip) };
        true
    } else {
        false
    }
}

const _: () = {
    assert!(core::mem::size_of::<Settings>() == 48);
    assert!(core::mem::align_of::<Settings>() == 8);
    assert!(core::mem::offset_of!(Settings, local_ip) == 0);
    assert!(core::mem::offset_of!(Settings, netmask) == 4);
    assert!(core::mem::offset_of!(Settings, gateway_ip) == 8);
    assert!(core::mem::offset_of!(Settings, dns_ip) == 12);
    assert!(core::mem::offset_of!(Settings, dns_configured) == 16);
    assert!(core::mem::offset_of!(Settings, dhcp_enabled) == 17);
    assert!(core::mem::offset_of!(Settings, dhcp_bound) == 18);
    assert!(core::mem::offset_of!(Settings, configured) == 19);
    assert!(core::mem::offset_of!(Settings, invalid_options) == 24);
    assert!(core::mem::offset_of!(Settings, last_error) == 32);
};
