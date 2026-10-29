//! Actual path-mount name policy. Capability snapshots perform no lookup,
//! free-space scan or mutation; creation still revalidates its own request.
use super::{context as ctx, fs, path};
use crate::{contract as c, memory::owner, program::program_api, sched::task_context};
use core::mem;

pub(super) unsafe fn resolve(
    raw: &[u8],
    output: &mut path::Buffer<1024>,
) -> Result<ctx::Target, i32> {
    if raw.len() > c::file_path_max_bytes as usize {
        return Err(c::file_stream_error_name_too_long);
    }
    match path::unicode_error(raw) {
        Some(path::UnicodeError::Malformed) => return Err(c::file_stream_error_invalid),
        Some(path::UnicodeError::TooLong) => return Err(c::file_stream_error_name_too_long),
        None => {}
    }
    let target = unsafe { ctx::resolve_checked(raw, output) }.map_err(|error| match error {
        program_api::ResolveError::Invalid => c::file_stream_error_invalid,
        program_api::ResolveError::TooLong => c::file_stream_error_name_too_long,
    })?;
    // The resolved visible path includes its two-byte drive prefix too.
    if target.path.len + 2 > c::file_path_max_bytes as usize {
        return Err(c::file_stream_error_name_too_long);
    }
    Ok(target)
}

/// Check the actual provider before stream reservation, directory traversal,
/// truncation, or atomic publication. This reuses its real name predicate.
pub(super) fn provider_error(kind: u8, resolved: &[u8]) -> Option<i32> {
    match kind {
        0 if !crate::fs::fat_names::valid_path(resolved) => {
            Some(c::file_stream_error_name_not_supported)
        }
        0 | 1 => None,
        _ => Some(c::file_stream_error_unsupported),
    }
}

/// # Safety
/// Caller supplies a readable NUL path and exclusive resident output. Exact
/// task admission and a live mount request span resolution and snapshot; no
/// retained mutable Rust reference crosses either wait.
pub(crate) unsafe extern "C" fn query(input: *const u8, out: *mut c::FileNameLimits) -> i32 {
    if input.is_null() || out.is_null() || crate::program::threads::compute_worker() {
        return c::file_stream_error_invalid;
    }
    let Some(_caller) = task_context::Admission::acquire(true) else {
        return c::file_stream_error_io;
    };
    owner::prepare_stack();
    unsafe {
        let mut raw_buffer = path::Buffer::<1024>::new();
        let Some(raw) = path::copy_z(input, &mut raw_buffer) else {
            return c::file_stream_error_name_too_long;
        };
        let mut output = path::Buffer::<1024>::new();
        let target = match resolve(raw, &mut output) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let Some(volume) = ctx::volume(target) else {
            return c::file_stream_error_unsupported;
        };
        let (provider, name_rules, atomic_rules) = match volume.kind {
            0 => (
                c::file_name_provider_fat32,
                c::file_name_rule_fat_long
                    | c::file_name_rule_append_create_short
                    | c::file_name_rule_ascii_casefold,
                c::file_name_atomic_stage_short
                    | c::file_name_atomic_backup_short
                    | c::file_name_atomic_owned_target_short,
            ),
            1 => (
                c::file_name_provider_ntfs,
                c::file_name_rule_ntfs_upcase,
                c::file_name_atomic_stage_short | c::file_name_atomic_backup_short,
            ),
            _ => return c::file_stream_error_unsupported,
        };
        let letter = (*target.drive_ref).letter;
        let Some(mut request) = fs::Request::volume(1, letter, volume) else {
            return c::file_stream_error_io;
        };
        let mut info = c::FileNameLimits {
            size: mem::size_of::<c::FileNameLimits>() as u32,
            flags: c::file_name_limits_flag_known,
            provider,
            drive_letter: u32::from(letter),
            max_path_bytes: u32::from(c::file_path_max_bytes),
            // A valid scalar consumes at most three UTF-8 bytes per UTF-16
            // unit. Both current providers admit at most 255 such units.
            max_component_bytes: 255 * 3,
            max_component_utf16_units: 255,
            short_base_bytes: 8,
            short_extension_bytes: 3,
            name_rules,
            atomic_rules,
            ..Default::default()
        };
        if volume.has_ref {
            info.flags |= c::file_name_limits_flag_mount_identity;
            info.mount_slot = volume.reference.slot;
            info.mount_generation = volume.reference.generation;
        }
        request.ok = true;
        drop(request);
        out.write(info);
        c::file_stream_result_ok
    }
}
