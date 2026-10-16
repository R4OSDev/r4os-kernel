//! Original bounded telemetry cache and finite demand. No sensor collection,
//! hardware policy or callback runs under this caller-owned metadata view.
use super::types::{Error, Owner};
use crate::contract as a;
use core::{mem::size_of, slice};
pub const MAX_AGE_NS: u64 = 3_000_000_000;
pub const DEMAND_NS: u64 = 10_000_000_000;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Entry {
    pub has_owner: bool,
    pub owner: Owner,
    pub data: a::GfxTelemetryState,
    pub demand: a::GfxTelemetryDemand,
}
pub fn validate(input: &a::GfxTelemetryState, now: u64) -> Result<(), Error> {
    if input.version != 1
        || (input.size as usize) < size_of::<a::GfxTelemetryState>()
        || input.adapter_id == 0
        || input.memory_generation == 0
        || input.reserved0 != 0
        || input.reserved1 != 0
        || input.source > a::gfx_telemetry_source_smu10
        || input.state > 4
        || input.policy > 9
        || input.boost > 2
        || now == 0
        || input.sampled_ns == 0
        || input.sampled_ns > now
        || input.valid_until_ns < input.sampled_ns
        || input.valid_until_ns - input.sampled_ns > MAX_AGE_NS
    {
        return Err(Error::Invalid);
    }
    for (i, metric) in input.metrics.iter().enumerate() {
        let mut allowed = 0;
        if metric.status == a::gfx_telemetry_fresh {
            if i == 9 {
                allowed = a::gfx_telemetry_timer_delta_valid;
            }
            if i == 0 || i == 2 {
                allowed = a::gfx_telemetry_partial_values | a::gfx_telemetry_value_mask;
            }
            if i == 0 {
                allowed |= a::gfx_telemetry_current_clocks | a::gfx_telemetry_fabric_clock;
            }
        }
        if metric.status > a::gfx_telemetry_stale || metric.flags & !allowed != 0 {
            return Err(Error::Invalid);
        }
        let known = (metric.flags & a::gfx_telemetry_value_mask) >> 8;
        if metric.flags & a::gfx_telemetry_partial_values != 0 {
            if known == 0 || (i == 2 && known & !3 != 0) {
                return Err(Error::Invalid);
            }
            for (component, &value) in metric.values.iter().enumerate() {
                if known & (1 << component) == 0 && value != 0 {
                    return Err(Error::Invalid);
                }
            }
        } else if known != 0 {
            return Err(Error::Invalid);
        }
        if metric.status == a::gfx_telemetry_fresh {
            if metric.source_stamp == 0 {
                return Err(Error::Invalid);
            }
        } else if metric.values.iter().any(|&v| v != 0) {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
fn demand(entry: &mut Entry, now: u64) -> a::GfxTelemetryDemand {
    if now >= entry.demand.until_ns {
        entry.demand.metric_mask = 0;
        entry.demand.until_ns = 0;
    }
    entry.demand
}
pub fn publish(
    entries: &mut [Entry],
    owner: Owner,
    input: a::GfxTelemetryState,
    now: u64,
) -> Result<a::GfxTelemetryDemand, Error> {
    if owner.kind != 2 || !owner.valid() {
        return Err(Error::WrongOwner);
    }
    validate(&input, now)?;
    let mut empty = None;
    for (i, entry) in entries.iter_mut().enumerate() {
        if !entry.has_owner {
            if empty.is_none() {
                empty = Some(i)
            }
            continue;
        }
        if entry.data.adapter_id != input.adapter_id {
            continue;
        }
        if entry.owner != owner {
            return Err(Error::WrongOwner);
        }
        if input.memory_generation < entry.data.memory_generation
            || input.sampled_ns < entry.data.sampled_ns
        {
            return Err(Error::Stale);
        }
        if entry.data.memory_generation != input.memory_generation {
            entry.demand = a::GfxTelemetryDemand {
                adapter_id: input.adapter_id,
                memory_generation: input.memory_generation,
                ..a::GfxTelemetryDemand::default()
            };
        }
        entry.data = input;
        return Ok(demand(entry, now));
    }
    let index = empty.ok_or(Error::Capacity)?;
    let entry = &mut entries[index];
    *entry = Entry {
        has_owner: true,
        owner,
        data: input,
        demand: a::GfxTelemetryDemand {
            adapter_id: input.adapter_id,
            memory_generation: input.memory_generation,
            ..a::GfxTelemetryDemand::default()
        },
    };
    Ok(entry.demand)
}
pub fn query(
    entries: &mut [Entry],
    request: a::GfxTelemetryRequest,
    now: u64,
) -> Result<a::GfxTelemetryState, Error> {
    if request.version != 1
        || (request.size as usize) < size_of::<a::GfxTelemetryRequest>()
        || request.reserved0 != 0
        || request.adapter_id == 0
        || request.memory_generation == 0
        || request.metric_mask & !a::gfx_telemetry_metric_mask != 0
        || now == 0
    {
        return Err(Error::Invalid);
    }
    for entry in entries {
        if !entry.has_owner || entry.data.adapter_id != request.adapter_id {
            continue;
        }
        if entry.data.memory_generation != request.memory_generation || now < entry.data.sampled_ns
        {
            return Err(Error::Stale);
        }
        let _ = demand(entry, now);
        if request.metric_mask != 0 {
            entry.demand.metric_mask |= request.metric_mask;
            entry.demand.until_ns = now.saturating_add(DEMAND_NS);
        }
        let mut out = entry.data;
        if now >= out.valid_until_ns {
            for metric in &mut out.metrics {
                if metric.status == a::gfx_telemetry_fresh {
                    metric.status = a::gfx_telemetry_stale;
                    metric.flags = 0;
                    metric.values = [0; 4];
                }
            }
        }
        return Ok(out);
    }
    Err(Error::Unavailable)
}
unsafe fn entries<'a>(p: *mut Entry, len: usize) -> &'a mut [Entry] {
    if len == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(p, len) }
    }
}
/// # Safety
/// Caller holds the shared BO metadata owner over initialized resident entries
/// and readable copied input; output is aligned and exclusive until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_telemetry_publish(
    p: *mut Entry,
    len: usize,
    owner: *const Owner,
    input: *const a::GfxTelemetryState,
    now: u64,
    out: *mut a::GfxTelemetryDemand,
) -> i32 {
    match publish(
        unsafe { entries(p, len) },
        unsafe { owner.read() },
        unsafe { input.read() },
        now,
    ) {
        Ok(v) => {
            unsafe { out.write(v) }
            0
        }
        Err(e) => e as i32,
    }
}
/// # Safety
/// Same serialized metadata lifetime; complete request and output are resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_telemetry_query(
    p: *mut Entry,
    len: usize,
    input: *const a::GfxTelemetryRequest,
    now: u64,
    out: *mut a::GfxTelemetryState,
) -> i32 {
    match query(unsafe { entries(p, len) }, unsafe { input.read() }, now) {
        Ok(v) => {
            unsafe { out.write(v) }
            0
        }
        Err(e) => e as i32,
    }
}
/// # Safety
/// Caller owns serialized metadata access to the complete initialized array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_telemetry_close(p: *mut Entry, len: usize, id: u32) {
    close(unsafe { entries(p, len) }, id);
}
/// # Safety
/// Caller retains one initialized readable complete state record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_telemetry_validate(
    input: *const a::GfxTelemetryState,
    now: u64,
) -> i32 {
    match validate(unsafe { &*input }, now) {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
const _: () = {
    assert!(size_of::<Entry>() == 616);
    assert!(core::mem::align_of::<Entry>() == 8);
    assert!(core::mem::offset_of!(Entry, has_owner) == 0);
    assert!(core::mem::offset_of!(Entry, owner) == 8);
    assert!(core::mem::offset_of!(Entry, data) == 32);
    assert!(core::mem::offset_of!(Entry, demand) == 576);
};

pub(crate) fn close(entries: &mut [Entry], id: u32) {
    for entry in entries {
        if entry.has_owner && entry.owner.id == u64::from(id) {
            *entry = Entry::default();
        }
    }
}
