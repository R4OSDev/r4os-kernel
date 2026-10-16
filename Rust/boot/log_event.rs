//! Original bounded LOG1 bytes. Drivers publish one complete record; protocol
//! fragments retain their existing publication order and 512-byte text limit.
use super::log;

fn puts(bytes: &[u8]) {
    unsafe { log::r4os_rust_bootlog_write(bytes.as_ptr(), bytes.len()) };
}
fn severity(level: u8) -> &'static [u8] {
    match level {
        0 => b"Info",
        1 => b"Warn",
        2 => b"Error",
        _ => unreachable!(),
    }
}
struct Line {
    bytes: [u8; 640],
    len: usize,
}
impl Line {
    fn push(&mut self, bytes: &[u8]) {
        let end = self.len + bytes.len();
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
    }
    fn number(&mut self, mut value: u32) {
        let mut bytes = [0; 10];
        let mut at = bytes.len();
        loop {
            at -= 1;
            bytes[at] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.push(&bytes[at..]);
    }
}
/// # Safety
/// Text is resident and zero-terminated, with the original readable limit of
/// up to 513 bytes when no earlier NUL occurs. No text reference escapes.
pub unsafe fn driver(level: u8, owner: u32, text: *const u8) {
    let mut length = 0;
    while length < 512 && unsafe { text.add(length).read() } != 0 {
        length += 1;
    }
    let truncated = length == 512 && unsafe { text.add(length).read() } != 0;
    let mut line = Line {
        bytes: [0; 640],
        len: 0,
    };
    line.push(b"[LOG1] source=Driver severity=");
    line.push(severity(level));
    line.push(b" owner=");
    line.number(owner);
    line.push(b" text=");
    line.push(unsafe { core::slice::from_raw_parts(text, length) });
    if truncated {
        line.push(b" [truncated]");
    }
    line.push(b"\r\n");
    puts(&line.bytes[..line.len]);
}
/// # Safety
/// Text retains the same bounded readable zero-terminated lifetime as driver.
pub unsafe fn protocol(level: u8, slot: u32, text: *const u8) {
    puts(b"[LOG1] source=");
    puts(b"Protocol");
    puts(b" severity=");
    puts(severity(level));
    puts(b" slot=");
    log::r4os_rust_bootlog_dec(u64::from(slot));
    puts(b" text=");
    let mut index = 0;
    while index < 512 && unsafe { text.add(index).read() } != 0 {
        let byte = unsafe { text.add(index).read() };
        puts(&[byte]);
        index += 1;
    }
    if index == 512 && unsafe { text.add(index).read() } != 0 {
        puts(b" [truncated]");
    }
    puts(b"\r\n");
}
/// # Safety
/// Same resident zero-terminated text lifetime as the typed native log owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_log_event(
    is_protocol: bool,
    level: u8,
    owner: u32,
    text: *const u8,
) {
    if is_protocol {
        unsafe { protocol(level, owner, text) };
    } else {
        unsafe { driver(level, owner, text) };
    }
}
