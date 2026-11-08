//! Copied technical PPI link mechanism over the one existing NIC registry.
//! No IPv6 address, route, neighbor, reassembly or policy state lives here.
use super::{adapters, frames};
use crate::{contract as c, platform::monotonic};
use crate::{fs::vfs_types::Bytes, loader::r4p, platform::timer};
use core::mem;
use core::sync::atomic::{AtomicU64, Ordering};
const IPV6_ROLE: &[u8] = b"net.ipv6";
static NEXT_TICK: AtomicU64 = AtomicU64::new(0);

fn clock(info: &mut c::NetworkLinkInfo) {
    if unsafe { monotonic::r4os_rust_monotonic_clock_api(&mut info.clock) } > 0 {
        info.flags |= c::network_link_flag_clock_valid;
    }
}
pub(crate) fn snapshot(index: usize, provider: u64) -> Option<c::NetworkLinkInfo> {
    let adapter = adapters::snapshot(index)?;
    let mut info = c::NetworkLinkInfo {
        index: u32::try_from(index).ok()?,
        interface_id: adapter.registration_id,
        generation: adapter.link_generation,
        provider,
        mtu: u32::from(adapter.mtu),
        mac: adapter.mac,
        flags: c::network_link_flag_available,
        ..c::NetworkLinkInfo::default()
    };
    if adapter.link == 2 {
        info.flags |= c::network_link_flag_up;
    }
    if adapter.backend.accepted & c::net_backend_cap_rx_multicast != 0 {
        info.flags |= c::network_link_flag_multicast;
    }
    clock(&mut info);
    Some(info)
}
/// # Safety
/// The original PPI scope retains the exact initialized request, exclusive
/// output and module code through actual backend return. Driver callback
/// admission, copied frame ownership and TX acknowledgements remain original.
pub(crate) unsafe fn call(
    provider: u64,
    input: *const c::NetworkLinkRequest,
    output: *mut c::NetworkLinkInfo,
) -> i32 {
    if input.is_null()
        || output.is_null()
        || !(input as usize).is_multiple_of(mem::align_of::<c::NetworkLinkRequest>())
        || !(output as usize).is_multiple_of(mem::align_of::<c::NetworkLinkInfo>())
    {
        return c::network_error_invalid;
    }
    let request = unsafe { *input };
    if request.version != 1
        || request.size as usize != mem::size_of_val(&request)
        || request.reserved0 != 0
        || request.reserved1 != 0
    {
        return c::network_error_invalid;
    }
    let info = if request.op == c::network_link_op_identity {
        if request.index != 0
            || request.interface_id != 0
            || request.generation != 0
            || request.bytes != 0
            || request.frame.iter().any(|byte| *byte != 0)
        {
            return c::network_error_invalid;
        }
        let mut value = c::NetworkLinkInfo {
            provider,
            index: u32::MAX,
            ..c::NetworkLinkInfo::default()
        };
        clock(&mut value);
        value
    } else if request.op == c::network_link_op_info {
        if request.interface_id != 0
            || request.generation != 0
            || request.bytes != 0
            || request.frame.iter().any(|byte| *byte != 0)
        {
            return c::network_error_invalid;
        }
        let Some(info) = snapshot(request.index as usize, provider) else {
            return c::network_error_not_found;
        };
        info
    } else if request.op == c::network_link_op_transmit {
        if request.interface_id == 0
            || request.generation == 0
            || request.bytes < 14
            || request.bytes as usize > request.frame.len()
            || request.frame[request.bytes as usize..]
                .iter()
                .any(|byte| *byte != 0)
        {
            return c::network_error_invalid;
        }
        // Pin the original adapter enumeration before selecting its identity;
        // removal/compaction cannot switch this index before actual TX. This
        // is callback lifetime admission, not a held NIC/registry state lock.
        let Some(_callback) = super::backend_admission::CallbackGuard::acquire() else {
            return c::network_error_backend;
        };
        let mut selected = None;
        for index in 0..adapters::r4os_rust_net_adapter_count() {
            if let Some(info) = snapshot(index, provider)
                && info.interface_id == request.interface_id
                && info.generation == request.generation
            {
                selected = Some(info);
                break;
            }
        }
        let Some(info) = selected else {
            return c::network_error_provider_changed;
        };
        if request.index != info.index {
            return c::network_error_provider_changed;
        }
        if request.bytes > info.mtu + 14 {
            return c::network_error_capacity;
        }
        if info.flags & c::network_link_flag_up == 0 {
            return c::network_error_backend;
        }
        // No NIC metadata owner spans backend work. The existing transmit
        // mechanism owns its real callback/code/context admission and result.
        let result = unsafe {
            frames::r4os_rust_frames_transmit(
                info.index as usize,
                request.frame.as_ptr(),
                request.bytes as usize,
            )
        };
        if result != 0 {
            return if result == 3 {
                c::network_error_would_block
            } else {
                c::network_error_backend
            };
        }
        info
    } else {
        return c::network_error_unsupported_policy;
    };
    unsafe { output.write(info) };
    0
}

/// Minimal copied ingress. The original RX worker retains frame and adapter
/// callback admission. IPv6 policy/state belongs exclusively to the R4P.
/// No Kernel IPv6 cache, address catalogue, route or reassembly is added.
pub(crate) unsafe fn received(index: usize, data: *const u8, len: usize) -> bool {
    if data.is_null() || !(14..=1536).contains(&len) {
        return false;
    }
    let Some(link) = snapshot(index, 0) else {
        return false;
    };
    if link.flags & c::network_link_flag_multicast == 0 {
        return false;
    }
    let mut request = c::Ipv6FrameOp {
        link,
        bytes: len as u32,
        ..c::Ipv6FrameOp::default()
    };
    unsafe { core::ptr::copy_nonoverlapping(data, request.frame.as_mut_ptr(), len) };
    let input = c::ProtocolBuffer {
        data: (&raw mut request).cast(),
        len: mem::size_of_val(&request) as u32,
        capacity: mem::size_of_val(&request) as u32,
        ..c::ProtocolBuffer::default()
    };
    let mut output = c::ProtocolBuffer::default();
    unsafe {
        r4p::r4os_rust_r4p_dispatch(
            Bytes {
                data: IPV6_ROLE.as_ptr(),
                len: IPV6_ROLE.len(),
            },
            c::ipv6_op_handle_rx,
            &input,
            &mut output,
        ) == 0
    }
}
/// Reuse the original network maintenance task. Gating controls invocation
/// cost only; the external provider evaluates all timers against real clock
/// identity/time, including changes and regression. No protocol retry here.
pub(crate) fn maintenance(now: u64) {
    let next = NEXT_TICK.load(Ordering::Relaxed);
    let interval = u64::from(timer::frequency()).div_ceil(20).max(1);
    if now < next && next - now <= interval {
        return;
    }
    if NEXT_TICK
        .compare_exchange(
            next,
            now.saturating_add(interval),
            Ordering::Relaxed,
            Ordering::Relaxed,
        )
        .is_err()
    {
        return;
    }
    let role = Bytes {
        data: IPV6_ROLE.as_ptr(),
        len: IPV6_ROLE.len(),
    };
    // Avoid demand-loading an absent optional role on every maintenance tick.
    let status = unsafe { r4p::r4os_rust_r4p_status(role) };
    // An installed lazy provider needs its first call to perform DAD/RS even
    // on a quiet link. Failed/blocked/absent roles are not retried each tick.
    if !status.active_r4p && status.state != 1 {
        return;
    }
    let input = c::ProtocolBuffer::default();
    let mut output = c::ProtocolBuffer::default();
    let _ = unsafe { r4p::r4os_rust_r4p_dispatch(role, c::ipv6_op_tick, &input, &mut output) };
}
