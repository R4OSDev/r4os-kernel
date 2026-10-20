//! Original installed-font scan. The exact catalogue writer reservation owns
//! the128KB resident scratch across I/O; no metadata/spin owner spans a wait.
use super::{owner, types::*};
use crate::fs::{request::ScopedRead, vfs};
use core::{cell::UnsafeCell, mem::MaybeUninit, slice};
pub const MAX_R4F_FILE_BYTES: usize = 128 * 1024;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ReloadResult {
    pub scanned: u32,
    pub registered: u32,
    pub rejected: u32,
    pub unavailable: bool,
}
struct Buffer(UnsafeCell<MaybeUninit<[u8; MAX_R4F_FILE_BYTES]>>);
// SAFETY: only the sole admitted catalogue writer accesses this scratch;
// no input byte reference is retained after register() copies/decodes the file.
unsafe impl Sync for Buffer {}
static FILE_BUFFER: Buffer = Buffer(UnsafeCell::new(MaybeUninit::uninit()));
pub fn reload_installed() -> ReloadResult {
    let mut result = ReloadResult::default();
    let Some(mut replacement) = owner::begin(false) else {
        result.unavailable = true;
        return result;
    };
    let candidate = replacement.state();
    let Some(volume) = vfs::mounted_volume(b'C') else {
        result.unavailable = true;
        return result;
    };
    let Some(mut resolve_request) = ScopedRead::begin(b'C') else {
        result.unavailable = true;
        return result;
    };
    let directory = vfs::resolve_path(volume, b"/R4OS/FONTS");
    resolve_request.finish(directory.is_some());
    let Some(directory) = directory else {
        result.unavailable = true;
        return result;
    };
    let mut index = 0usize;
    let mut name_buffer = [0u8; 768];
    loop {
        let Some(mut entry_request) = ScopedRead::begin(b'C') else {
            result.unavailable = true;
            break;
        };
        let status = vfs::directory_entry(volume, directory, index, &mut name_buffer);
        entry_request.finish(!matches!(status, vfs::DirectoryStatus::Io));
        let entry = match status {
            vfs::DirectoryStatus::Found(entry) => entry,
            vfs::DirectoryStatus::NotFound => break,
            vfs::DirectoryStatus::Io => {
                result.unavailable = true;
                break;
            }
        };
        let name_len = name_buffer
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(name_buffer.len());
        let name = &name_buffer[..name_len];
        if entry.is_dir() || name.len() < 4 || !name[name.len() - 4..].eq_ignore_ascii_case(b".r4f")
        {
            index += 1;
            continue;
        }
        result.scanned += 1;
        if entry.size == 0 || entry.size > MAX_R4F_FILE_BYTES as u64 {
            result.rejected += 1;
            index += 1;
            continue;
        }
        let Some(mut read_request) = ScopedRead::begin(b'C') else {
            result.unavailable = true;
            break;
        };
        let want = usize::try_from(entry.size).expect("Original bounded R4F read");
        let buffer = FILE_BUFFER.0.get().cast::<u8>();
        let got = unsafe { vfs::read_file_raw(volume, &entry, buffer, want) };
        let read_ok = got == Some(want);
        read_request.finish(read_ok);
        if !read_ok {
            result.unavailable = true;
            break;
        }
        let mut path_buffer = [0u8; MAX_FONT_PATH];
        let prefix = b"C:\\R4OS\\FONTS\\";
        if prefix.len() + name.len() > path_buffer.len() {
            result.rejected += 1;
            index += 1;
            continue;
        }
        path_buffer[..prefix.len()].copy_from_slice(prefix);
        path_buffer[prefix.len()..prefix.len() + name.len()].copy_from_slice(name);
        let path = &path_buffer[..prefix.len() + name.len()];
        if unsafe { (*candidate).catalog_count() } == MAX_CATALOG_FONTS {
            result.unavailable = true;
            break;
        }
        // Only successful exact-length I/O initializes this span. register() owns
        // no persistent borrow of it, so the next file may reuse the same scratch.
        if unsafe { (*candidate).register(path, slice::from_raw_parts(buffer, want)) } {
            result.registered += 1
        } else {
            result.rejected += 1
        }
        index += 1;
    }
    if !result.unavailable {
        replacement.commit();
    }
    result
}
/// # Safety
/// Caller owns one aligned exclusive reload-result record until scan completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_font_reload_installed(out: *mut ReloadResult) {
    let result = reload_installed();
    unsafe { out.write(result) }
}
const _: () = {
    assert!(core::mem::size_of::<ReloadResult>() == 16);
    assert!(core::mem::align_of::<ReloadResult>() == 4);
    assert!(core::mem::offset_of!(ReloadResult, scanned) == 0);
    assert!(core::mem::offset_of!(ReloadResult, registered) == 4);
    assert!(core::mem::offset_of!(ReloadResult, rejected) == 8);
    assert!(core::mem::offset_of!(ReloadResult, unavailable) == 12);
};
