//! Original bounded CONFIG.R4S rewrite. No filesystem or configuration owner
//! is borrowed here; callers keep the existing transactional write path.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Addresses {
    pub local_ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway_ip: [u8; 4],
    pub dns_ip: [u8; 4],
    pub dns_configured: bool,
}
fn trim(mut text: &[u8]) -> &[u8] {
    while matches!(text.first(), Some(b' ' | b'\t')) {
        text = &text[1..];
    }
    while matches!(text.last(), Some(b' ' | b'\t')) {
        text = &text[..text.len() - 1];
    }
    text
}
pub fn is_managed_net_option(raw: &[u8]) -> bool {
    let line = trim(&raw[..raw.iter().position(|&v| v == b'#').unwrap_or(raw.len())]);
    if line.len() < 7 || !line[..7].eq_ignore_ascii_case(b"OPTION ") {
        return false;
    }
    let rest = trim(&line[7..]);
    let Some(space) = rest.iter().position(|&v| v == b' ') else {
        return false;
    };
    if !rest[..space].eq_ignore_ascii_case(b"NET") {
        return false;
    }
    let body = trim(&rest[space + 1..]);
    let Some(eq) = body.iter().position(|&v| v == b'=') else {
        return false;
    };
    let key = trim(&body[..eq]);
    [
        b"dhcp".as_slice(),
        b"ip",
        b"addr",
        b"address",
        b"mask",
        b"netmask",
        b"gateway",
        b"gw",
        b"dns",
        b"nameserver",
    ]
    .iter()
    .any(|v| key.eq_ignore_ascii_case(v))
}
struct Writer<'a> {
    out: &'a mut [u8],
    len: usize,
}
impl Writer<'_> {
    fn append(&mut self, text: &[u8]) -> Option<()> {
        if text.len() > self.out.len() - self.len {
            return None;
        }
        self.out[self.len..self.len + text.len()].copy_from_slice(text);
        self.len += text.len();
        Some(())
    }
    fn ip(&mut self, ip: [u8; 4]) -> Option<()> {
        for (index, byte) in ip.into_iter().enumerate() {
            if index != 0 {
                self.append(b".")?;
            }
            let mut number = byte;
            let mut digits = [0u8; 3];
            let mut at = 3;
            loop {
                at -= 1;
                digits[at] = b'0' + number % 10;
                number /= 10;
                if number == 0 {
                    break;
                }
            }
            self.append(&digits[at..])?;
        }
        Some(())
    }
    fn network(&mut self, a: Addresses) -> Option<()> {
        self.append(b"OPTION NET ip=")?;
        self.ip(a.local_ip)?;
        self.append(b"\r\nOPTION NET mask=")?;
        self.ip(a.netmask)?;
        self.append(b"\r\nOPTION NET gateway=")?;
        self.ip(a.gateway_ip)?;
        if a.dns_configured {
            self.append(b"\r\nOPTION NET dns=")?;
            self.ip(a.dns_ip)?;
        }
        self.append(b"\r\n")
    }
}
pub fn rewrite_config(mut data: &[u8], output: &mut [u8], addresses: Addresses) -> Option<usize> {
    let mut writer = Writer {
        out: output,
        len: 0,
    };
    let mut inserted = false;
    let mut start = 0;
    if data.starts_with(&[0xef, 0xbb, 0xbf]) {
        writer.append(&data[..3])?;
        data = &data[3..];
    }
    while start < data.len() {
        let mut end = start;
        while end < data.len() && !matches!(data[end], b'\n' | b'\r') {
            end += 1;
        }
        let line = &data[start..end];
        if is_managed_net_option(line) {
            if !inserted {
                writer.network(addresses)?;
                inserted = true;
            }
        } else {
            writer.append(line)?;
            writer.append(b"\r\n")?;
        }
        start = end;
        while start < data.len() && matches!(data[start], b'\n' | b'\r') {
            start += 1;
        }
    }
    if !inserted {
        if writer.len > 0 && writer.out[writer.len - 1] != b'\n' {
            writer.append(b"\r\n")?;
        }
        if writer.len > 0 {
            writer.append(b"\r\n")?;
        }
        writer.network(addresses)?;
    }
    Some(writer.len)
}
/// # Safety
/// Separate readable input and writable bounded output, exact address record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_rewrite(
    input: *const u8,
    input_len: usize,
    output: *mut u8,
    output_len: usize,
    addresses: *const Addresses,
    result: *mut usize,
) -> bool {
    let input = if input_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(input, input_len) }
    };
    let output = if output_len == 0 {
        &mut []
    } else {
        // Zig may supply uninitialized scratch. Initialize it before forming
        // a Rust byte slice; only the returned prefix is part of the result.
        unsafe { core::ptr::write_bytes(output, 0, output_len) };
        unsafe { core::slice::from_raw_parts_mut(output, output_len) }
    };
    if let Some(len) = rewrite_config(input, output, unsafe { *addresses }) {
        unsafe { result.write(len) };
        true
    } else {
        false
    }
}
/// # Safety
/// Readable immutable line for this pure classification.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_config_managed(input: *const u8, len: usize) -> bool {
    is_managed_net_option(if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(input, len) }
    })
}

const _: () = {
    assert!(core::mem::size_of::<Addresses>() == 17);
    assert!(core::mem::align_of::<Addresses>() == 1);
    assert!(core::mem::offset_of!(Addresses, local_ip) == 0);
    assert!(core::mem::offset_of!(Addresses, netmask) == 4);
    assert!(core::mem::offset_of!(Addresses, gateway_ip) == 8);
    assert!(core::mem::offset_of!(Addresses, dns_ip) == 12);
    assert!(core::mem::offset_of!(Addresses, dns_configured) == 16);
};
