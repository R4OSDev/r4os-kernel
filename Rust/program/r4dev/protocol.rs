//! Original passive protocol status and explicit protocol dispatch.
use crate::{
    contract as c, driver::protocol_registry as registry, fs::vfs_types::Bytes, loader::r4p,
};
/// # Safety
/// Role bytes and exclusive canonical output are resident for the original
/// bounded lookup. Empty/oversized roles retain their original error result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_protocol_status(
    role: *const u8,
    len: u32,
    out: *mut c::ProtocolStatus,
) -> i32 {
    unsafe {
        if len == 0 || len as usize >= registry::MAX_ROLE {
            out.write(Default::default());
            return -2;
        }
        let Some(entry) =
            registry::diagnostic_entry(core::slice::from_raw_parts(role, len as usize))
        else {
            out.write(Default::default());
            return -5;
        };
        let mut value = c::ProtocolStatus {
            state: match entry.state {
                1..=6 => u32::from(entry.state),
                _ => 0,
            },
            flags: match entry.source {
                0 => 1,
                1 => 2,
                2 => 4,
                _ => unreachable!(),
            },
            last_error: entry.last_error,
            ..Default::default()
        };
        super::fixed_z(&mut value.note, &entry.note[..entry.note_len]);
        out.write(value);
        0
    }
}
/// # Safety
/// Resident role and exact input/output buffers retain the established R4P
/// execution/lifetime contract for the explicitly requested dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_protocol_dispatch(
    role: *const u8,
    len: u32,
    op: u32,
    input: *const c::ProtocolBuffer,
    output: *mut c::ProtocolBuffer,
) -> i32 {
    if len == 0 || len as usize >= registry::MAX_ROLE {
        return -2;
    }
    unsafe {
        r4p::r4os_rust_r4p_dispatch(
            Bytes {
                data: role,
                len: len as usize,
            },
            op,
            input,
            output,
        )
    }
}
