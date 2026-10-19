//! Immutable boot NUMA facts. Only proximity preferences live here; usable
//! frames, commit, reclaim and task admission retain their existing owners.
use super::{
    firmware_tables,
    numa_decode::{self, Range, Reason, Topology},
};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};
struct Store(UnsafeCell<Topology>);
// Sole BSP write before AP release; Acquire readers get immutable boot data.
unsafe impl Sync for Store {}
static STORE: Store = Store(UnsafeCell::new(Topology::EMPTY));
static READY: AtomicBool = AtomicBool::new(false);
static EMPTY: Topology = Topology::EMPTY;
fn topology() -> &'static Topology {
    if READY.load(Ordering::Acquire) {
        unsafe { &*STORE.0.get() }
    } else {
        &EMPTY
    }
}
pub(crate) fn stack_anchor_needed() -> bool {
    if READY.load(Ordering::Acquire) {
        topology().active()
    } else {
        // Critical stacks are prepared before CPU sealing. Do not lose their
        // exact frame when a valid boot SRAT can later describe several nodes.
        firmware_tables::may_have_srat()
    }
}
pub(crate) fn seal(apics: &[u32], qualified: u64) {
    if READY.load(Ordering::Acquire) {
        return;
    }
    let value = match firmware_tables::boot_table(*b"SRAT") {
        Ok(srat) => numa_decode::decode(
            srat,
            firmware_tables::boot_table(*b"SLIT").ok().flatten(),
            apics,
            qualified,
        ),
        Err(_) => Topology {
            reason: Reason::Firmware,
            ..Topology::EMPTY
        },
    };
    unsafe {
        STORE.0.get().write(value);
    }
    READY.store(true, Ordering::Release);
    log(apics);
}
pub(crate) struct LocalMemory {
    pub node: u8,
    pub ranges: &'static [Range],
}
pub(crate) fn local_memory(cpu: u32) -> Option<LocalMemory> {
    let data = topology();
    if !data.active() {
        return None;
    }
    let node = data.cpu_nodes.get(cpu as usize).copied().flatten()?;
    Some(LocalMemory {
        node,
        ranges: &data.ranges[..data.range_count],
    })
}
pub(crate) fn range_end(frame: u64) -> Option<u64> {
    let data = topology();
    data.frame_node(frame)?;
    data.ranges[..data.range_count]
        .iter()
        .find(|r| r.base <= frame && frame < r.end)
        .map(|r| r.end)
}
#[derive(Clone, Copy)]
pub(crate) struct Preference {
    data: &'static Topology,
    node: u8,
}
impl Preference {
    pub fn key(self, cpu: u32) -> u16 {
        // Construction validates every CPU in the admitted mask. An old CPU
        // outside that mask can never gain a preference from missing data.
        self.data.key(cpu, self.node).unwrap_or(u16::MAX)
    }
}
pub(crate) fn preference(mask: u64, frame: u64) -> Option<Preference> {
    let data = topology();
    if !data.active() || mask == 0 || mask & !data.qualified != 0 {
        return None;
    }
    Some(Preference {
        data,
        node: data.frame_node(frame)?,
    })
}
fn log(apics: &[u32]) {
    use crate::boot::log_route as log;
    let data = topology();
    log::puts(b"[NUMA] status=");
    log::puts(match data.reason {
        Reason::Missing => b"missing-flat",
        Reason::Firmware => b"firmware-flat",
        Reason::Invalid => b"invalid-flat",
        Reason::Bounds => b"bounded-flat",
        Reason::CpuMap => b"cpu-map-flat",
        Reason::Known => b"known",
    });
    log::puts(b" nodes=");
    log::dec(data.node_count as u64);
    log::puts(b" ranges=");
    log::dec(data.range_count as u64);
    log::puts(b" cpus=");
    log::dec(u64::from(data.qualified.count_ones()));
    log::puts(b" slit=");
    log::puts(if data.slit_valid {
        b"known"
    } else {
        b"unknown"
    });
    log::puts(b"\r\n");
    if data.reason != Reason::Known {
        return;
    }
    for (cpu, &apic) in apics.iter().enumerate() {
        if let Some(node) = data.cpu_nodes[cpu] {
            log::puts(b"[NUMA] cpu=");
            log::dec(cpu as u64);
            log::puts(b" apic=");
            log::dec(u64::from(apic));
            log::puts(b" domain=");
            log::dec(u64::from(data.nodes[node as usize]));
            log::puts(b"\r\n");
        }
    }
}
