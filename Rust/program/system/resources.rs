//! Original on-demand R4M0 non-alloc resource reads from arbitrary module
//! paths. The program loader/image is deliberately not the resource owner.
use super::super::registry_format as wire;
use super::{
    context::{self as ctx, PathBuffers},
    fs,
};
struct File {
    volume: fs::Volume,
    entry: fs::Entry,
    letter: u8,
}
struct Resource {
    offset: u32,
    size: u32,
}
unsafe fn resolve(input: *const u8) -> Result<File, i32> {
    unsafe {
        let mut buffers = PathBuffers::new();
        let target = buffers.resolve(input).ok_or(-1)?;
        let volume = ctx::volume(target).ok_or(-2)?;
        let letter = (*target.drive_ref).letter;
        let mut request = fs::Request::volume(12, letter, volume).ok_or(-8)?;
        let mut entry = fs::Entry::EMPTY;
        match fs::resolve(volume, target.path.slice(), &mut entry) {
            0 => {}
            1 => return Err(-3),
            2 => return Err(-8),
            _ => unreachable!(),
        }
        if entry.is_dir() {
            return Err(-3);
        }
        request.ok = true;
        Ok(File {
            volume,
            entry,
            letter,
        })
    }
}
unsafe fn name(input: *const u8, out: &mut [u8; 64]) -> Option<&[u8]> {
    unsafe {
        if input.is_null() {
            return None;
        }
        let mut len = 0;
        while input.add(len).read() != 0 && len < out.len() {
            out[len] = input.add(len).read();
            len += 1;
        }
        if input.add(len).read() != 0 {
            Some(&out[..0])
        } else {
            Some(&out[..len])
        }
    }
}
unsafe fn read(file: &File, offset: u64, out: &mut [u8]) -> Result<usize, i32> {
    unsafe {
        fs::read(
            file.volume,
            &file.entry,
            offset,
            out.as_mut_ptr(),
            out.len(),
            true,
        )
        .ok_or(-8)
    }
}
unsafe fn locate(
    file: &File,
    kind: u32,
    want_index: u32,
    want_name: Option<&[u8]>,
) -> Result<Resource, i32> {
    unsafe {
        let mut header = [0u8; 64];
        if read(file, 0, &mut header)? != header.len() {
            return Err(-4);
        }
        if &header[..4] != b"R4M0"
            || wire::read16(&header, 4) != 1
            || wire::read16(&header, 6) != 1
            || wire::read16(&header, 10) != 64
        {
            return Err(-4);
        }
        let section_offset = wire::read32(&header, 16);
        let section_count = wire::read32(&header, 20);
        if section_count == 0 || section_count > 16 {
            return Err(-4);
        }
        let mut rsrc_offset = 0;
        let mut rsrc_size = 0;
        let mut found = false;
        for index in 0..section_count {
            let mut record = [0u8; 32];
            if read(
                file,
                u64::from(section_offset) + u64::from(index) * 32,
                &mut record,
            )? != record.len()
            {
                return Err(-4);
            }
            let flags = wire::read32(&record, 8);
            if flags & 1 != 0 {
                continue;
            }
            if flags != 0 || &record[..6] != b".rsrc\0" || found {
                return Err(-4);
            }
            found = true;
            rsrc_offset = wire::read32(&record, 12);
            rsrc_size = wire::read32(&record, 16);
        }
        if !found || rsrc_size < 4 {
            return Err(-5);
        }
        if u64::from(rsrc_offset) + u64::from(rsrc_size) > file.entry.size {
            return Err(-4);
        }
        let mut count = [0u8; 4];
        if read(file, u64::from(rsrc_offset), &mut count)? != 4 {
            return Err(-4);
        }
        let count = wire::read32(&count, 0);
        if count == 0 || count > 64 || 4 + u64::from(count) * 16 > u64::from(rsrc_size) {
            return Err(-4);
        }
        for index in 0..count {
            let mut record = [0u8; 16];
            if read(
                file,
                u64::from(rsrc_offset) + 4 + u64::from(index) * 16,
                &mut record,
            )? != record.len()
            {
                return Err(-4);
            }
            let typ = wire::read16(&record, 0);
            let entry_index = wire::read16(&record, 2);
            let name_offset = wire::read32(&record, 4);
            let data_offset = wire::read32(&record, 8);
            let size = wire::read32(&record, 12);
            if u32::from(typ) != kind {
                continue;
            }
            let matches = match kind {
                1 => u32::from(entry_index) == want_index,
                2 => true,
                3 => {
                    if let Some(want) = want_name {
                        if name_offset == 0 || name_offset >= rsrc_size {
                            false
                        } else {
                            let mut buffer = [0u8; 64];
                            let want_len = ((rsrc_size - name_offset) as usize).min(buffer.len());
                            let got = read(
                                file,
                                u64::from(rsrc_offset) + u64::from(name_offset),
                                &mut buffer[..want_len],
                            )?;
                            if let Some(len) = buffer[..got].iter().position(|&ch| ch == 0) {
                                buffer[..len].eq_ignore_ascii_case(want)
                            } else {
                                false
                            }
                        }
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if !matches {
                continue;
            }
            if size == 0 || u64::from(data_offset) + u64::from(size) > u64::from(rsrc_size) {
                return Err(-4);
            }
            return Ok(Resource {
                offset: rsrc_offset + data_offset,
                size,
            });
        }
        Err(-6)
    }
}
/// # Safety
/// Original immutable NUL path/optional name and exact admitted file lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_resource_stat(
    input: *const u8,
    kind: u32,
    index: u32,
    resource_name: *const u8,
) -> i32 {
    unsafe {
        if kind == 3 && resource_name.is_null() {
            return -1;
        }
        let file = match resolve(input) {
            Ok(file) => file,
            Err(error) => return error,
        };
        let mut buffer = [0u8; 64];
        let name = name(resource_name, &mut buffer);
        if name.is_some_and(|name| name.is_empty()) {
            return -1;
        }
        let Some(mut request) = fs::Request::volume(2, file.letter, file.volume) else {
            return -8;
        };
        let resource = match locate(&file, kind, index, name) {
            Ok(resource) => resource,
            Err(error) => return error,
        };
        request.ok = true;
        if resource.size > i32::MAX as u32 {
            return -4;
        }
        resource.size as i32
    }
}
/// # Safety
/// Original immutable path/optional name and exclusive caller output cover
/// complete resource read. A small buffer is an explicit error, no truncation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_resource_read(
    input: *const u8,
    kind: u32,
    index: u32,
    resource_name: *const u8,
    out: *mut u8,
    len: u32,
) -> i32 {
    unsafe {
        if kind == 3 && resource_name.is_null() || len == 0 {
            return -1;
        }
        let file = match resolve(input) {
            Ok(file) => file,
            Err(error) => return error,
        };
        let mut buffer = [0u8; 64];
        let name = name(resource_name, &mut buffer);
        if name.is_some_and(|name| name.is_empty()) {
            return -1;
        }
        let Some(mut request) = fs::Request::volume(2, file.letter, file.volume) else {
            return -8;
        };
        let resource = match locate(&file, kind, index, name) {
            Ok(resource) => resource,
            Err(error) => return error,
        };
        if resource.size > len {
            return -7;
        }
        let count = fs::read(
            file.volume,
            &file.entry,
            u64::from(resource.offset),
            out,
            resource.size as usize,
            true,
        );
        if count != Some(resource.size as usize) {
            return -8;
        }
        request.ok = true;
        i32::try_from(resource.size).expect("Module resource result size")
    }
}
