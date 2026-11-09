//! Copied optional Start2 bindings. The existing loader, process registry,
//! resident byte-pipe owner and physical reaper retain every actual resource.
//! No new console route, foreign pointer or public parallel process model.
use super::{byte_pipes, context, notifications, payload, waitsets};
use crate::contract as c;
use core::mem::size_of;

pub(super) fn error(code: i32) -> i32 {
    match code {
        0 => 0,
        c::launch_io_error_busy..=c::launch_io_error_invalid => code,
        c::byte_pipe_error_invalid | -1 => c::launch_io_error_invalid,
        c::byte_pipe_error_context => c::launch_io_error_context,
        c::byte_pipe_error_stale => c::launch_io_error_stale,
        c::byte_pipe_error_owner => c::launch_io_error_owner,
        c::byte_pipe_error_direction => c::launch_io_error_direction,
        c::byte_pipe_error_capacity => c::launch_io_error_capacity,
        c::byte_pipe_error_memory | -8 => c::launch_io_error_memory,
        c::byte_pipe_error_release => c::launch_io_error_release,
        c::byte_pipe_error_exhausted | -11 => c::launch_io_error_exhausted,
        c::byte_pipe_error_unsupported => c::launch_io_error_unsupported,
        -2 => c::launch_io_error_not_found,
        -9 => c::launch_io_error_load,
        -7 => c::launch_io_error_busy,
        _ => c::launch_io_error_start,
    }
}
fn empty(end: c::BytePipeEnd) -> bool {
    end.handle == 0 && end.generation == 0 && end.direction == 0 && end.reserved == 0
}
pub(super) fn validate(io: &c::LaunchIo) -> Result<(), i32> {
    if io.version != 1
        || io.size as usize != size_of::<c::LaunchIo>()
        || io.policy > 2
        || io.console_host > 2
    {
        return Err(c::launch_io_error_invalid);
    }
    for (i, b) in [io.stdin_binding, io.stdout_binding, io.stderr_binding]
        .iter()
        .enumerate()
    {
        if b.reserved != 0 {
            return Err(c::launch_io_error_invalid);
        }
        match b.kind {
            c::launch_io_default if empty(b.end) => {}
            c::launch_io_pipe => {
                if b.end.handle == 0 || b.end.generation == 0 || b.end.reserved != 0 {
                    return Err(c::launch_io_error_invalid);
                }
                let direction = if i == 0 {
                    c::byte_pipe_reader
                } else {
                    c::byte_pipe_writer
                };
                if b.end.direction != direction {
                    return Err(c::launch_io_error_direction);
                }
            }
            _ => return Err(c::launch_io_error_invalid),
        }
    }
    Ok(())
}
/// Private exclusive unpublished construction value; the reservation owns the
/// full child generation until successful publication or acknowledged rollback.
pub(super) unsafe fn bind(
    io: &c::LaunchIo,
    child: c::ProgramProcessHandle,
) -> Result<c::LaunchIo, i32> {
    validate(io)?;
    let parent = unsafe { context::handle() }.ok_or(c::launch_io_error_context)?;
    let mut ends = [
        io.stdin_binding.end,
        io.stdout_binding.end,
        io.stderr_binding.end,
    ];
    let selected = [
        io.stdin_binding.kind == c::launch_io_pipe,
        io.stdout_binding.kind == c::launch_io_pipe,
        io.stderr_binding.kind == c::launch_io_pipe,
    ];
    unsafe { byte_pipes::bind_launch(parent, child, &mut ends, selected) }.map_err(error)?;
    let mut result = *io;
    result.stdin_binding.end = ends[0];
    result.stdout_binding.end = ends[1];
    result.stderr_binding.end = ends[2];
    Ok(result)
}
pub(super) unsafe extern "C" fn query(output: *mut c::LaunchIo) -> i32 {
    if !waitsets::valid(output, 1) {
        return c::launch_io_error_invalid;
    }
    let Some(_operation) = notifications::Operation::enter() else {
        return c::launch_io_error_context;
    };
    unsafe {
        let instance = context::current();
        if instance.is_null() || (*instance).runtime_payload.is_null() {
            return c::launch_io_error_context;
        }
        let runtime = payload::runtime(instance);
        if (*runtime).start_generation != 2 {
            return c::launch_io_error_unsupported;
        }
        // Immutable after first publication. A query copies borrowed identity
        // values; it never reopens a closed ref or retains another reference.
        output.write((*runtime).launch_io);
    }
    0
}
const _: c::R4SysFns::program_stdio = query;
