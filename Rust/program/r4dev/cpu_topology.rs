//! Passive projections of the confirmed immutable CPU owner. Caller writes
//! occur without runtime admission or a borrowed global reference.
use crate::{
    arch::percpu,
    contract as c,
    platform::{
        acpi, cpu_topology as catalog,
        cpu_topology_decode::{CacheState, Problem},
    },
};
use core::{mem, ptr};
fn firmware_uid(apic: u32) -> Result<Option<u32>, ()> {
    let info = acpi::snapshot();
    let entries =
        &info.madt_cpu_entries[..(info.madt_lapic_count as usize).min(info.madt_cpu_entries.len())];
    let mut uid = None;
    for entry in entries
        .iter()
        .filter(|e| e.flags & 3 != 0 && e.apic_id == apic)
    {
        if uid.is_some_and(|u| u != entry.acpi_uid) {
            return Err(());
        }
        uid = Some(entry.acpi_uid);
    }
    if let Some(uid) = uid
        && entries
            .iter()
            .any(|e| e.flags & 3 != 0 && e.acpi_uid == uid && e.apic_id != apic)
    {
        return Err(());
    }
    Ok(uid)
}
fn cache_status(s: CacheState) -> u32 {
    match s {
        CacheState::Unavailable => c::cpu_topology_cache_unavailable,
        CacheState::Complete => c::cpu_topology_cache_complete,
        CacheState::Invalid => c::cpu_topology_cache_invalid,
        CacheState::Truncated => c::cpu_topology_cache_truncated,
    }
}
fn problem(s: Problem) -> u32 {
    match s {
        Problem::Unavailable => c::cpu_topology_geometry_unavailable,
        Problem::Malformed => c::cpu_topology_geometry_malformed,
        Problem::Identity => c::cpu_topology_geometry_identity,
        Problem::LegacyModel => c::cpu_topology_geometry_legacy_model,
    }
}
fn base(id: u32, bits: u32) -> u32 {
    id & !((1u64 << bits) - 1) as u32
}
/// # Safety
/// Exclusive aligned resident output spans the caller's advertised capacity;
/// the first8 bytes are initialized. No output or tail changes on refusal/end.
pub(crate) unsafe extern "C" fn info(index: u32, out: *mut c::CpuTopologyInfo) -> i32 {
    if out.is_null() || !out.is_aligned() {
        return c::cpu_topology_result_invalid;
    }
    if unsafe { ptr::addr_of!((*out).version).read() } != 1
        || unsafe { ptr::addr_of!((*out).size).read() }
            < mem::size_of::<c::CpuTopologyInfo>() as u32
    {
        return c::cpu_topology_result_invalid;
    }
    if catalog::count() == 0 {
        return c::cpu_topology_result_busy;
    }
    let Some(s) = catalog::snapshot(index) else {
        return c::cpu_topology_result_end;
    };
    let mut value = c::CpuTopologyInfo {
        cpu_index: s.index,
        apic_id: s.apic_id,
        start_epoch: s.start_epoch,
        configured_cpus: catalog::count(),
        ..Default::default()
    };
    if index == 0 {
        value.flags |= c::cpu_topology_flag_bsp;
    }
    if s.qualified {
        value.flags |= c::cpu_topology_flag_qualified;
    }
    if percpu::state(index) == 4 {
        value.flags |= c::cpu_topology_flag_online;
    }
    if percpu::is_schedulable(index) {
        value.flags |= c::cpu_topology_flag_schedulable;
    }
    match firmware_uid(s.apic_id) {
        Ok(Some(uid)) => {
            value.acpi_uid = uid;
            value.flags |= c::cpu_topology_flag_acpi_uid;
        }
        Err(()) => value.flags |= c::cpu_topology_flag_acpi_conflict,
        Ok(None) => {}
    }
    if let Some(d) = s.data {
        value.flags |= c::cpu_topology_flag_captured;
        value.vendor_name = d.vendor;
        value.cache_source_leaf = d.cache_leaf;
        value.cache_count = d.cache_count as u32;
        value.cache_status = cache_status(d.cache_state);
        value.geometry_status = if !s.sealed {
            c::cpu_topology_geometry_starting
        } else if !s.qualified {
            c::cpu_topology_geometry_unqualified
        } else {
            match d.geometry {
                Err(e) => problem(e),
                Ok(_) => c::cpu_topology_geometry_inconsistent,
            }
        };
        if let Ok(g) = d.geometry {
            value.source_leaf = g.leaf;
            if s.trusted {
                value.flags |= c::cpu_topology_flag_geometry;
                value.geometry_status = c::cpu_topology_geometry_known;
                // Global APIC-prefix keys also distinguish disjoint packages
                // with different widths and coincident raw shifted numbers.
                value.package_id = base(s.apic_id, g.package_shift);
                value.core_id = base(s.apic_id, g.smt_shift);
                value.thread_id = g.thread;
                value.smt_shift = g.smt_shift;
                value.package_shift = g.package_shift;
                value.threads_per_core_bound = g.threads;
                value.logical_per_package_bound = g.logical_bound;
            }
        }
        if let Some(class) = d.class {
            value.flags |= c::cpu_topology_flag_core_type;
            value.core_type_source = class.leaf;
            value.core_type = class.kind;
            value.native_model_id = class.model;
            if let Some(rank) = class.efficiency {
                value.flags |= c::cpu_topology_flag_efficiency;
                value.efficiency_rank = rank;
            }
            if class.heterogeneous {
                value.flags |= c::cpu_topology_flag_heterogeneous;
            }
            if class.asymmetric {
                value.flags |= c::cpu_topology_flag_asymmetric;
            }
        }
    }
    unsafe { out.write(value) };
    c::cpu_topology_result_ok
}
/// # Safety
/// Same caller-buffer/header contract as info, for one64-byte cache record.
pub(crate) unsafe extern "C" fn cache(
    index: u32,
    ordinal: u32,
    out: *mut c::CpuCacheTopologyInfo,
) -> i32 {
    if out.is_null() || !out.is_aligned() {
        return c::cpu_topology_result_invalid;
    }
    if unsafe { ptr::addr_of!((*out).version).read() } != 1
        || unsafe { ptr::addr_of!((*out).size).read() }
            < mem::size_of::<c::CpuCacheTopologyInfo>() as u32
    {
        return c::cpu_topology_result_invalid;
    }
    if catalog::count() == 0 {
        return c::cpu_topology_result_busy;
    }
    let Some(s) = catalog::snapshot(index) else {
        return c::cpu_topology_result_end;
    };
    let Some(d) = s.data else {
        return c::cpu_topology_result_end;
    };
    if ordinal as usize >= d.cache_count {
        return c::cpu_topology_result_end;
    }
    let entry = d.caches[ordinal as usize];
    let value = c::CpuCacheTopologyInfo {
        cpu_index: index,
        cache_index: ordinal,
        start_epoch: s.start_epoch,
        source_leaf: d.cache_leaf,
        kind: entry.kind,
        level: entry.level,
        line_bytes: entry.line_bytes,
        sharing_logical_bound: entry.sharing,
        sharing_shift: entry.sharing_shift,
        cache_bytes: entry.bytes,
        property_flags: entry.properties,
        ..Default::default()
    };
    unsafe { out.write(value) };
    c::cpu_topology_result_ok
}
