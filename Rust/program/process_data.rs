//! Process arguments, lazy GUI/environment ownership and DOS path context.
//! All mutable data stays in the same payload records. Filesystem inspection
//! completes before the registry lock; no payload reference spans that I/O.
use super::{
    local_context, payload, payload_types as t, registry, registry_types::ProgramInstance, threads,
};
use crate::{
    contract,
    fs::{drive, vfs_types::Bytes},
    memory::owner::Guard,
};
use core::{
    mem::{self, MaybeUninit},
    ptr, slice,
};
const ENV: usize = contract::environment_block_max;
const NAME: usize = contract::environment_name_max;
const VALUE: usize = contract::environment_value_max;
const PATH: usize = contract::file_path_max_bytes as usize + 1;
unsafe extern "C" {
    // The existing R4SYS filesystem owner retains target resolution and I/O.
    fn r4os_zig_program_data_file_info(path: *const u8, out: *mut contract::FileInfo) -> i32;
}
unsafe fn bytes<'a>(input: Bytes) -> &'a [u8] {
    if input.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(input.data, input.len) }
    }
}
pub(super) unsafe fn ensure_environment(instance: *mut ProgramInstance) -> *mut t::Environment {
    unsafe {
        let process = payload::process(instance);
        if !(*process).environment_payload.is_null() {
            return (*process).environment_payload;
        }
        let environment = payload::r4os_rust_program_payload_allocate(3, (*instance).id, 3)
            .cast::<t::Environment>();
        if environment.is_null() {
            return ptr::null_mut();
        }
        (*process).environment_payload = environment;
        environment
    }
}
pub(super) unsafe fn ensure_gui(instance: *mut ProgramInstance) -> *mut t::Gui {
    unsafe {
        let guard = Guard::acquire(0);
        let existing = (*instance).gui_payload;
        drop(guard);
        if !existing.is_null() {
            return existing.cast();
        }
        let Some(_operation) = threads::Operation::enter() else {
            return ptr::null_mut();
        };
        let candidate = payload::r4os_rust_program_payload_allocate(6, (*instance).id, 6);
        if candidate.is_null() {
            return ptr::null_mut();
        }
        let guard = Guard::acquire(0);
        let chosen = if (*instance).gui_payload.is_null() {
            candidate
        } else {
            (*instance).gui_payload
        };
        (*instance).gui_payload = chosen;
        drop(guard);
        if chosen != candidate {
            payload::r4os_rust_program_payload_release(6, candidate, (*instance).id, 6);
        }
        chosen.cast()
    }
}
unsafe fn arguments(instance: *mut ProgramInstance, args: &[u8]) {
    unsafe {
        let process = payload::process(instance);
        let out = ptr::addr_of_mut!((*process).args).cast::<u8>();
        ptr::write_bytes(out, 0, 128);
        let count = args.len().min(127);
        if count > 0 {
            ptr::copy_nonoverlapping(args.as_ptr(), out, count);
        }
        out.add(count).write(0);
    }
}
unsafe fn inherit(instance: *mut ProgramInstance, parent: *const ProgramInstance) {
    unsafe {
        let process = payload::process(instance);
        (*process).environment_len = 0;
        if parent.is_null() {
            return;
        }
        let source = payload::process(parent);
        if (*source).environment_len == 0 {
            return;
        }
        let destination = (*process).environment_payload;
        let environment = (*source).environment_payload;
        if destination.is_null() || environment.is_null() {
            return;
        }
        let out = ptr::addr_of_mut!((*destination).bytes).cast::<u8>();
        ptr::write_bytes(out, 0, ENV);
        let count = (*source).environment_len.min(ENV);
        ptr::copy_nonoverlapping(ptr::addr_of!((*environment).bytes).cast::<u8>(), out, count);
        (*process).environment_len = count;
    }
}
#[derive(Clone, Copy)]
struct Entry {
    start: usize,
    value_start: usize,
    end: usize,
}
unsafe fn env_byte(environment: *const t::Environment, index: usize) -> u8 {
    assert!(index < ENV);
    unsafe {
        ptr::addr_of!((*environment).bytes)
            .cast::<u8>()
            .add(index)
            .read()
    }
}
unsafe fn env_write(environment: *mut t::Environment, index: usize, value: u8) {
    assert!(index < ENV);
    unsafe {
        ptr::addr_of_mut!((*environment).bytes)
            .cast::<u8>()
            .add(index)
            .write(value)
    };
}
unsafe fn find(instance: *const ProgramInstance, name: &[u8]) -> Option<Entry> {
    unsafe {
        let process = payload::process(instance);
        let environment = (*process).environment_payload;
        if environment.is_null() {
            return None;
        }
        let mut start = 0;
        while start < (*process).environment_len {
            let mut end = start;
            while end < (*process).environment_len && env_byte(environment, end) != 0 {
                end += 1;
            }
            if end == start {
                start += 1;
                continue;
            }
            let mut equal = start;
            while equal < end && env_byte(environment, equal) != b'=' {
                equal += 1;
            }
            if equal < end {
                assert!(equal <= ENV);
                let key = slice::from_raw_parts(
                    ptr::addr_of!((*environment).bytes).cast::<u8>().add(start),
                    equal - start,
                );
                if key.eq_ignore_ascii_case(name) {
                    return Some(Entry {
                        start,
                        value_start: equal + 1,
                        end,
                    });
                }
            }
            start = end + 1;
        }
        None
    }
}
unsafe fn remove(instance: *mut ProgramInstance, entry: Entry) {
    unsafe {
        let process = payload::process(instance);
        let environment = (*process).environment_payload;
        if environment.is_null() {
            return;
        }
        if entry.end >= (*process).environment_len {
            (*process).environment_len = entry.start;
            assert!(entry.start <= ENV);
            ptr::write_bytes(
                ptr::addr_of_mut!((*environment).bytes)
                    .cast::<u8>()
                    .add(entry.start),
                0,
                ENV - entry.start,
            );
            return;
        }
        let next = entry.end + 1;
        let tail = (*process).environment_len - next;
        for index in 0..tail {
            let value = env_byte(environment, next + index);
            env_write(environment, entry.start + index, value);
        }
        (*process).environment_len -= next - entry.start;
        let len = (*process).environment_len;
        assert!(len <= ENV);
        ptr::write_bytes(
            ptr::addr_of_mut!((*environment).bytes)
                .cast::<u8>()
                .add(len),
            0,
            ENV - len,
        );
    }
}
unsafe fn env_get(
    instance: *const ProgramInstance,
    name: &[u8],
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe {
        let process = payload::process(instance);
        let environment = (*process).environment_payload;
        if environment.is_null() {
            return -2;
        }
        let Some(entry) = find(instance, name) else {
            return -2;
        };
        assert!(entry.value_start <= entry.end && entry.end <= ENV);
        let len = entry.end - entry.value_start;
        if (capacity as usize) < len {
            return -3;
        }
        if len != 0 {
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*environment).bytes)
                    .cast::<u8>()
                    .add(entry.value_start),
                out,
                len,
            );
        }
        if capacity as usize > len {
            out.add(len).write(0);
        }
        i32::try_from(len).expect("Environment value length")
    }
}
unsafe fn env_set(instance: *mut ProgramInstance, name: &[u8], value: &[u8]) -> i32 {
    unsafe {
        if name.len() > NAME || value.len() > VALUE {
            return -5;
        }
        let process = payload::process(instance);
        let existing = find(instance, name);
        let entry_len = name.len() + 1 + value.len() + 1;
        let replaced = existing.map_or(0, |entry| entry.end - entry.start + 1);
        let retained = (*process).environment_len - replaced;
        if entry_len > ENV || retained > ENV - entry_len {
            return -5;
        }
        let environment = ensure_environment(instance);
        if environment.is_null() {
            return -6;
        }
        if let Some(entry) = existing {
            remove(instance, entry);
        }
        let mut pos = (*process).environment_len;
        assert!(pos + name.len() <= ENV);
        ptr::copy_nonoverlapping(
            name.as_ptr(),
            ptr::addr_of_mut!((*environment).bytes)
                .cast::<u8>()
                .add(pos),
            name.len(),
        );
        pos += name.len();
        env_write(environment, pos, b'=');
        pos += 1;
        if !value.is_empty() {
            assert!(pos + value.len() <= ENV);
            ptr::copy_nonoverlapping(
                value.as_ptr(),
                ptr::addr_of_mut!((*environment).bytes)
                    .cast::<u8>()
                    .add(pos),
                value.len(),
            );
        }
        pos += value.len();
        env_write(environment, pos, 0);
        pos += 1;
        (*process).environment_len = pos;
        0
    }
}
fn name_valid(name: &[u8]) -> bool {
    if name.is_empty() || name.len() > NAME {
        return false;
    }
    !name.iter().any(|ch| *ch <= b' ' || *ch == b'=')
}
unsafe fn copy_z(source: *const u8, out: *mut u8, capacity: usize) -> Option<usize> {
    unsafe {
        let mut len = 0;
        while len < capacity && source.add(len).read() != 0 {
            out.add(len).write(source.add(len).read());
            len += 1;
        }
        if len == capacity { None } else { Some(len) }
    }
}
unsafe fn copy_slice_z(out: *mut u8, capacity: usize, value: &[u8]) {
    unsafe {
        if capacity != 0 {
            ptr::write_bytes(out, 0, capacity);
        }
        if capacity == 0 {
            return;
        }
        let count = value.len().min(capacity - 1);
        if count > 0 {
            ptr::copy_nonoverlapping(value.as_ptr(), out, count);
        }
    }
}
unsafe fn canonical(input: &[u8], out: *mut u8) -> Option<usize> {
    unsafe {
        if input.is_empty() || !matches!(input[0], b'\\' | b'/') {
            return None;
        }
        out.write(b'\\');
        let mut len = 1;
        let mut starts = MaybeUninit::<[usize; 160]>::uninit();
        let starts = starts.as_mut_ptr().cast::<usize>();
        let mut segments = 0;
        let mut pos = 0;
        while pos < input.len() {
            while pos < input.len() && matches!(input[pos], b'\\' | b'/') {
                pos += 1;
            }
            if pos >= input.len() {
                break;
            }
            let start = pos;
            while pos < input.len() && !matches!(input[pos], b'\\' | b'/') {
                pos += 1;
            }
            let segment = &input[start..pos];
            if segment == b"." {
                continue;
            }
            if segment == b".." {
                if segments == 0 {
                    return None;
                }
                segments -= 1;
                len = starts.add(segments).read();
                continue;
            }
            if segments >= 160 {
                return None;
            }
            let before = len;
            if len > 1 {
                if len >= PATH {
                    return None;
                }
                out.add(len).write(b'\\');
                len += 1;
            }
            if segment.len() > PATH - len {
                return None;
            }
            starts.add(segments).write(before);
            segments += 1;
            ptr::copy_nonoverlapping(segment.as_ptr(), out.add(len), segment.len());
            len += segment.len();
        }
        Some(len)
    }
}
unsafe fn cwd_get(instance: *const ProgramInstance, out: *mut u8, capacity: u32) -> i32 {
    if !registry::lock() {
        return -4;
    }
    unsafe {
        let process = payload::process(instance);
        let len = (*process).work_cwd_len + 2;
        if (capacity as usize) < len {
            registry::unlock();
            return -3;
        }
        out.write((*process).work_drive_letter);
        out.add(1).write(b':');
        assert!((*process).work_cwd_len <= drive::MAX_PATH);
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*process).work_cwd).cast::<u8>(),
            out.add(2),
            (*process).work_cwd_len,
        );
        if capacity as usize > len {
            out.add(len).write(0);
        }
        let len = i32::try_from(len).expect("Working directory length");
        registry::unlock();
        len
    }
}
unsafe fn cwd_set(instance: *mut ProgramInstance, value: &[u8]) -> i32 {
    unsafe {
        if value.len() < 3
            || !value[0].is_ascii_alphabetic()
            || value[1] != b':'
            || value.contains(&0)
        {
            return -1;
        }
        let mut normalized = MaybeUninit::<[u8; PATH]>::uninit();
        let path = normalized.as_mut_ptr().cast::<u8>();
        let Some(len) = canonical(&value[2..], path) else {
            return -1;
        };
        if len >= drive::MAX_PATH || len + 2 >= PATH {
            return -5;
        }
        let mut absolute = [0u8; PATH];
        let letter = value[0].to_ascii_uppercase();
        absolute[0] = letter;
        absolute[1] = b':';
        ptr::copy_nonoverlapping(path, absolute.as_mut_ptr().add(2), len);
        let mut info = contract::FileInfo::default();
        if r4os_zig_program_data_file_info(absolute.as_ptr(), &mut info) != 1
            || info.exists == 0
            || info.is_dir == 0
        {
            return -1;
        }
        if !registry::lock() {
            return -4;
        }
        let process = payload::process(instance);
        (*process).work_drive_letter = letter;
        ptr::copy_nonoverlapping(
            path,
            ptr::addr_of_mut!((*process).work_cwd).cast::<u8>(),
            len,
        );
        (*process).work_cwd[len] = 0;
        (*process).work_cwd_len = len;
        registry::unlock();
        0
    }
}
unsafe fn copy_cwd(
    instance: *mut ProgramInstance,
    parent: *const ProgramInstance,
    drive: *const drive::Drive,
) -> bool {
    unsafe {
        let process = payload::process(instance);
        if !parent.is_null() {
            if !registry::lock() {
                return false;
            }
            let source = payload::process(parent);
            (*process).work_drive_letter = (*source).work_drive_letter;
            (*process).work_cwd_len = (*source).work_cwd_len;
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*source).work_cwd).cast::<u8>(),
                ptr::addr_of_mut!((*process).work_cwd).cast::<u8>(),
                drive::MAX_PATH,
            );
            registry::unlock();
            return true;
        }
        (*process).work_drive_letter = (*drive).letter;
        ptr::write_bytes(
            ptr::addr_of_mut!((*process).work_cwd).cast::<u8>(),
            0,
            drive::MAX_PATH,
        );
        (*process).work_cwd_len = (*drive).cwd_len;
        if (*process).work_cwd_len == 0 || (*process).work_cwd_len > drive::MAX_PATH {
            (*process).work_cwd[0] = b'\\';
            (*process).work_cwd_len = 1;
            return true;
        }
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*drive).cwd).cast::<u8>(),
            ptr::addr_of_mut!((*process).work_cwd).cast::<u8>(),
            (*process).work_cwd_len,
        );
        true
    }
}
unsafe fn local_owner() -> *mut local_context::Owner {
    unsafe {
        let thread = threads::current();
        if thread.is_null() {
            return ptr::null_mut();
        }
        let instance = (*thread).owner_instance;
        if instance.is_null() {
            return ptr::null_mut();
        }
        let runtime = (*instance).runtime_payload.cast::<t::Runtime>();
        if runtime.is_null() {
            return ptr::null_mut();
        }
        ptr::addr_of_mut!((*runtime).local_context)
    }
}
/// # Safety
/// Exact resident instance; lazy allocation retains original pin/unwind and
/// double-checked GUI publication. environment=true selects process environment.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_ensure(
    instance: *mut ProgramInstance,
    environment: bool,
) -> *mut core::ffi::c_void {
    unsafe {
        if environment {
            ensure_environment(instance).cast()
        } else {
            ensure_gui(instance).cast()
        }
    }
}
/// # Safety
/// Exact resident destination and immutable original argument/environment source.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_arguments(
    instance: *mut ProgramInstance,
    args: Bytes,
) {
    unsafe { arguments(instance, bytes(args)) };
}
/// # Safety
/// Optional resident parent and original destination lifetime; source bytes are
/// immutable/disjoint as in the original inherited environment copy contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_inherit(
    instance: *mut ProgramInstance,
    parent: *const ProgramInstance,
) {
    unsafe { inherit(instance, parent) };
}
/// # Safety
/// Original resident instance/name and exclusive disjoint output capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_env_get(
    instance: *const ProgramInstance,
    name: Bytes,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe { env_get(instance, bytes(name), out, capacity) }
}
/// # Safety
/// Original resident instance and immutable name/value for this update.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_env_set(
    instance: *mut ProgramInstance,
    name: Bytes,
    value: Bytes,
) -> i32 {
    unsafe { env_set(instance, bytes(name), bytes(value)) }
}
/// # Safety
/// Immutable resident name span under the original API buffer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_env_name(name: Bytes) -> bool {
    name_valid(unsafe { bytes(name) })
}
/// # Safety
/// Original resident process and exclusive disjoint output capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_cwd_get(
    instance: *const ProgramInstance,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe { cwd_get(instance, out, capacity) }
}
/// # Safety
/// Original resident process/value; filesystem work occurs without registry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_cwd_set(
    instance: *mut ProgramInstance,
    value: Bytes,
) -> i32 {
    unsafe { cwd_set(instance, bytes(value)) }
}
/// # Safety
/// Exact process/optional parent and the existing native mounted-drive descriptor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_cwd_copy(
    instance: *mut ProgramInstance,
    parent: *const ProgramInstance,
    drive: *const drive::Drive,
) -> bool {
    unsafe { copy_cwd(instance, parent, drive) }
}
/// # Safety
/// Original current-thread execution lifetime; returned owner never survives
/// its exact process/runtime payload lifetime without the existing caller pin.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_local_owner() -> *mut local_context::Owner {
    unsafe { local_owner() }
}
/// # Safety
/// Exact process/runtime lifetime at the existing persistent cleanup boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_local_cleanup(
    instance: *mut ProgramInstance,
) -> bool {
    unsafe {
        let runtime = (*instance).runtime_payload.cast::<t::Runtime>();
        runtime.is_null()
            || local_context::r4os_rust_program_local_cleanup(ptr::addr_of_mut!(
                (*runtime).local_context
            ))
    }
}
/// # Safety
/// Original local-context caller/output contract. publish=false performs get;
/// publish=true performs publication using the unchanged native owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_local(
    key: u64,
    value: u64,
    out: *mut u64,
    publish: bool,
) -> i32 {
    let owner = unsafe { local_owner() };
    if owner.is_null() {
        return contract::program_local_error_context;
    }
    unsafe {
        if publish {
            local_context::r4os_rust_program_local_publish(owner, key, value, out)
        } else {
            local_context::r4os_rust_program_local_get(owner, key, out)
        }
    }
}
/// # Safety
/// Original current-instance and resident Z-name/output API buffer contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_env_api_get(
    name: *const u8,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe {
        let instance = super::context::current();
        if instance.is_null() {
            return -4;
        }
        let mut buffer = MaybeUninit::<[u8; NAME + 1]>::uninit();
        let b = buffer.as_mut_ptr().cast::<u8>();
        let Some(len) = copy_z(name, b, NAME + 1) else {
            return -1;
        };
        let name = slice::from_raw_parts(b, len);
        if !name_valid(name) {
            return -1;
        }
        if name.eq_ignore_ascii_case(b"CWD") {
            cwd_get(instance, out, capacity)
        } else {
            env_get(instance, name, out, capacity)
        }
    }
}
/// # Safety
/// Original current-instance, resident Z-name and immutable value API contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_env_api_set(
    name: *const u8,
    value: *const u8,
    len: u32,
) -> i32 {
    unsafe {
        let instance = super::context::current();
        if instance.is_null() {
            return -4;
        }
        let mut buffer = MaybeUninit::<[u8; NAME + 1]>::uninit();
        let b = buffer.as_mut_ptr().cast::<u8>();
        let Some(n) = copy_z(name, b, NAME + 1) else {
            return -1;
        };
        let name = slice::from_raw_parts(b, n);
        if !name_valid(name) {
            return -1;
        }
        if len as usize > VALUE {
            return -5;
        }
        let value = bytes(Bytes {
            data: value,
            len: len as usize,
        });
        if name.eq_ignore_ascii_case(b"CWD") {
            cwd_set(instance, value)
        } else {
            env_set(instance, name, value)
        }
    }
}
/// # Safety
/// Resident immutable input and exclusive1024-byte output; partial prefixes on
/// failure match the original normalizer and no final NUL is appended here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_canonical(
    input: Bytes,
    out: *mut u8,
    len: *mut usize,
) -> bool {
    if let Some(n) = unsafe { canonical(bytes(input), out) } {
        unsafe { len.write(n) };
        true
    } else {
        false
    }
}
/// # Safety
/// Resident Z-source, exclusive disjoint output capacity and scalar result.
/// Copies the same prefix and intentionally does not append a NUL byte.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_copy_z(
    source: *const u8,
    out: *mut u8,
    capacity: usize,
    len: *mut usize,
) -> bool {
    if let Some(n) = unsafe { copy_z(source, out, capacity) } {
        unsafe { len.write(n) };
        true
    } else {
        false
    }
}
/// # Safety
/// Resident immutable source and exclusive disjoint bounded output capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_copy_slice(
    out: *mut u8,
    capacity: usize,
    value: Bytes,
) {
    unsafe { copy_slice_z(out, capacity, bytes(value)) };
}
/// # Safety
/// Resident immutable byte spans, with their original alias and lifetime rules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_data_equal(a: Bytes, b: Bytes) -> bool {
    unsafe { bytes(a).eq_ignore_ascii_case(bytes(b)) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_data_upper(value: u8) -> u8 {
    value.to_ascii_uppercase()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_data_alpha(value: u8) -> bool {
    value.is_ascii_alphabetic()
}
const _: () = {
    assert!(ENV == 2048);
    assert!(NAME == 32);
    assert!(VALUE == 512);
    assert!(PATH == 1024);
    assert!(mem::size_of::<t::Process>() == 1200);
};
