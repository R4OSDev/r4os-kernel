//! Original R4XStart/parallel-owner metadata and natural-exit decisions.
//! These helpers own no state and perform no allocation or callbacks.
use core::slice;

unsafe fn bytes<'a>(data: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, len) }
    }
}

/// Export names distinguish incompatible generations; minimum revisions do not.
pub(super) fn start_export(v1: &mut bool, v2: &mut bool, name: &[u8], revision: u32) -> bool {
    let (seen, expected) = match name {
        b"R4XStart" => {
            *v1 = true;
            return false;
        }
        b"R4XStart2" => (v2, 2),
        _ => return true,
    };
    if *seen || revision != expected {
        return false;
    }
    *seen = true;
    true
}

pub(super) fn start_generation(metadata: &[u8], v1: bool, v2: bool, exports: u32) -> Option<u8> {
    if exports == 0 || v1 || !v2 {
        return None;
    }
    // New artifacts carry one complete, unambiguous SDK-owned start identity.
    for (key, expected) in [
        (b"r4x.start=".as_slice(), b"r4xstart2".as_slice()),
        (b"r4x.entry=", b"R4XStart2"),
        (b"r4x.start_abi=", b"2"),
        (b"r4x.context=", b"R4XStartContext2"),
    ] {
        let mut values = metadata
            .split(|b| *b == 0)
            .filter_map(|v| v.strip_prefix(key));
        if values.next() != Some(expected) || values.next().is_some() {
            return None;
        }
    }
    Some(2)
}

/// # Safety
/// The metadata pointer names the caller's resident bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_parallel(data: *const u8, len: usize) -> u8 {
    parallel_owner(unsafe { bytes(data, len) }, b"runtime.parallel=")
}

pub(super) fn parallel_owner(metadata: &[u8], key: &[u8]) -> u8 {
    let mut found = None;
    for item in metadata.split(|value| *value == 0) {
        if let Some(value) = item.strip_prefix(key) {
            if found.is_some() {
                return u8::MAX;
            }
            found = match value {
                b"bsp" => Some(0),
                b"owned-v1" => Some(1),
                _ => return u8::MAX,
            };
        }
    }
    found.unwrap_or(0)
}

/// # Safety
/// The metadata pointer names the caller's resident bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_desktop_host(data: *const u8, len: usize) -> u8 {
    let mut found = None;
    for item in unsafe { bytes(data, len) }.split(|value| *value == 0) {
        if let Some(value) = item.strip_prefix(b"app.role=") {
            if found.is_some() {
                return u8::MAX;
            }
            found = Some(u8::from(value == b"desktop_host"));
        }
    }
    found.unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_natural_exit(
    exited: bool,
    execution_pinned: bool,
    scheduler_task_present: bool,
) -> bool {
    exited && (execution_pinned || scheduler_task_present)
}

/// Immutable loaded R4X metadata, never a live window setter.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub flags: u32,
    pub background: u32,
    pub foreground: u32,
}
pub(super) fn title_appearance(metadata: &[u8]) -> Option<Appearance> {
    let mut result = Appearance::default();
    for item in metadata.split(|b| *b == 0) {
        let eq = item.iter().position(|b| *b == b'=');
        let key = &item[..eq.unwrap_or(item.len())];
        for (index, expected) in [b"gui.title.background".as_slice(), b"gui.title.foreground"]
            .iter()
            .enumerate()
        {
            if !key.eq_ignore_ascii_case(expected) {
                continue;
            }
            if key != *expected || eq.is_none() {
                return None;
            }
            let bit = 1u32 << index;
            if result.flags & bit != 0 {
                return None;
            }
            let text = &item[eq? + 1..];
            if text.len() != 6 {
                return None;
            }
            let mut rgb = 0u32;
            for ch in text {
                let digit = match ch {
                    b'0'..=b'9' => ch - b'0',
                    b'a'..=b'f' => ch - b'a' + 10,
                    b'A'..=b'F' => ch - b'A' + 10,
                    _ => return None,
                };
                rgb = (rgb << 4) | u32::from(digit);
            }
            result.flags |= bit;
            if index == 0 {
                result.background = rgb;
            } else {
                result.foreground = rgb;
            }
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{Appearance, parallel_owner, title_appearance};
    #[test]
    fn immutable_title_color_admission() {
        assert_eq!(
            title_appearance(b"feature=ordinary"),
            Some(Appearance::default())
        );
        assert_eq!(
            title_appearance(b"gui.title.background=000000\0gui.title.foreground=FfEeDd"),
            Some(Appearance {
                flags: 3,
                background: 0,
                foreground: 0xffeedd
            })
        );
        for invalid in [
            b"gui.title.background=123456\0gui.title.background=123456".as_slice(),
            b"GUI.title.background=123456",
            b"gui.title.foreground=fff",
            b"gui.title.foreground=#ffffff",
            b"gui.title.foreground=12345g",
            b"gui.title.foreground=ffffff00",
            b"gui.title.foreground= ffffff",
            b"gui.title.foreground=ffffff ",
            b"gui.title.background",
        ] {
            assert_eq!(title_appearance(invalid), None);
        }
    }
    #[test]
    fn separate_main_and_worker_declarations() {
        let meta = b"runtime.parallel=bsp\0runtime.workers=owned-v1\0";
        assert_eq!(parallel_owner(meta, b"runtime.parallel="), 0);
        assert_eq!(parallel_owner(meta, b"runtime.workers="), 1);
        assert_eq!(
            parallel_owner(b"runtime.parallel=owned-v1", b"runtime.workers="),
            0
        );
    }
    #[test]
    fn unknown_and_duplicate_worker_declarations_reject() {
        for meta in [
            b"runtime.workers=unknown".as_slice(),
            b"runtime.workers=owned-v1\0runtime.workers=bsp",
            b"runtime.workers=owned-v1\0runtime.workers=owned-v1",
        ] {
            assert_eq!(parallel_owner(meta, b"runtime.workers="), u8::MAX);
        }
    }
}
