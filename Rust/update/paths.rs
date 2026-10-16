//! Original exact managed companion and preload path domain.
pub const COMPANION_RECOVERY_KERNEL: &str = "0.1.199";
pub const PRELOAD_RECOVERY_KERNEL: &str = "0.1.208";
fn same_path(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(&l, &r)| {
            let l = if l == b'\\' {
                b'/'
            } else {
                l.to_ascii_uppercase()
            };
            let r = if r == b'\\' {
                b'/'
            } else {
                r.to_ascii_uppercase()
            };
            l == r
        })
}
pub(crate) fn preload_target(path: &[u8]) -> bool {
    [
        &b"/boot/preload.r4i"[..],
        &b"/boot/preload/hidreport.r4p"[..],
        &b"/boot/preload/usbhid.r4p"[..],
        &b"/boot/preload/usbbot.r4p"[..],
        &b"/boot/preload/usbscsi.r4p"[..],
    ]
    .iter()
    .any(|target| same_path(path, target))
}
pub(crate) fn companion_kind(path: &[u8]) -> Option<u32> {
    if preload_target(path) {
        return Some(2);
    }
    for (prefix, kind) in [
        (&b"C:/R4OS/LICENSES/"[..], 0),
        (&b"C:/R4OS/SOURCES/"[..], 1),
    ] {
        if path.len() > prefix.len() && same_path(&path[..prefix.len()], prefix) {
            for part in path[prefix.len()..]
                .split(|b| *b == b'/' || *b == b'\\')
                .filter(|part| !part.is_empty())
            {
                if part == b"."
                    || part == b".."
                    || part[part.len() - 1] == b'.'
                    || part[part.len() - 1] == b' '
                {
                    return None;
                }
                for byte in part {
                    if *byte < 0x20 || *byte >= 0x7f || b"\"*:;<>?|".contains(byte) {
                        return None;
                    }
                }
            }
            if matches!(path[path.len() - 1], b'/' | b'\\') {
                return None;
            }
            return Some(kind);
        }
    }
    None
}
/// # Safety
/// Private caller retains the exact bounded readable ASCII path span.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_update_paths(op: u32, data: *const u8, len: usize) -> i32 {
    let data = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(data, len) }
    };
    match op {
        0 => companion_kind(data).map_or(-1, |v| v as i32),
        1 => i32::from(preload_target(data)),
        _ => panic!("Invalid private update path operation"),
    }
}
