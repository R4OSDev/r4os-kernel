//! Bounded ACPI 6.6 SRAT/SLIT byte decoder. Proximity domains are opaque
//! firmware IDs; neither APIC IDs nor domains are assumed to be dense.
use crate::arch::smp_policy::MAX_CPUS;

pub(crate) const MAX_NODES: usize = 32;
pub(crate) const MAX_RANGES: usize = 128;
const MAX_AFFINITIES: usize = 256;
const MAX_RECORDS: usize = 1024;
const MAX_SLIT: usize = 256;
const MAX_BYTES: usize = 1024 * 1024;
const PAGE: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    Missing,
    Firmware,
    Invalid,
    Bounds,
    CpuMap,
    Known,
}
#[derive(Clone, Copy)]
pub(crate) struct Range {
    pub base: u64,
    pub end: u64,
    pub node: u8,
}
impl Range {
    const EMPTY: Self = Self {
        base: 0,
        end: 0,
        node: 0,
    };
}
pub(crate) struct Topology {
    pub reason: Reason,
    pub qualified: u64,
    pub nodes: [u32; MAX_NODES],
    pub node_count: usize,
    pub cpu_nodes: [Option<u8>; MAX_CPUS],
    pub ranges: [Range; MAX_RANGES],
    pub range_count: usize,
    pub distances: [[u8; MAX_NODES]; MAX_NODES],
    pub slit_valid: bool,
}
impl Topology {
    pub const EMPTY: Self = Self {
        reason: Reason::Missing,
        qualified: 0,
        nodes: [0; MAX_NODES],
        node_count: 0,
        cpu_nodes: [None; MAX_CPUS],
        ranges: [Range::EMPTY; MAX_RANGES],
        range_count: 0,
        distances: [[0; MAX_NODES]; MAX_NODES],
        slit_valid: false,
    };
    pub fn active(&self) -> bool {
        self.reason == Reason::Known && self.node_count > 1
    }
    pub fn frame_node(&self, frame: u64) -> Option<u8> {
        if self.reason != Reason::Known || frame == 0 || !frame.is_multiple_of(PAGE) {
            return None;
        }
        let end = frame.checked_add(PAGE)?;
        self.ranges[..self.range_count]
            .iter()
            .find(|r| r.base <= frame && end <= r.end)
            .map(|r| r.node)
    }
    // This is a policy key, never an exported or invented distance. Equal
    // SRAT domains prove local membership without requiring a SLIT. Unknown
    // remote distances all tie; only a complete finite matrix orders them.
    pub fn key(&self, cpu: u32, memory_node: u8) -> Option<u16> {
        if !self.active() || memory_node as usize >= self.node_count {
            return None;
        }
        let node = self.cpu_nodes.get(cpu as usize).copied().flatten()?;
        Some(if node == memory_node {
            0
        } else if self.slit_valid {
            u16::from(self.distances[node as usize][memory_node as usize])
        } else {
            1
        })
    }
    fn node(&mut self, id: u32) -> Result<u8, Reason> {
        if let Some(index) = self.nodes[..self.node_count].iter().position(|&n| n == id) {
            return Ok(index as u8);
        }
        if self.node_count == MAX_NODES {
            return Err(Reason::Bounds);
        }
        let index = self.node_count;
        self.nodes[index] = id;
        self.node_count += 1;
        Ok(index as u8)
    }
}
#[derive(Clone, Copy)]
struct Affinity {
    apic: u32,
    node: u8,
}
const EMPTY_AFFINITY: Affinity = Affinity { apic: 0, node: 0 };

fn table(bytes: &[u8], signature: &[u8; 4], minimum: usize) -> bool {
    bytes.len() >= minimum
        && bytes.len() <= MAX_BYTES
        && &bytes[..4] == signature
        && read32(bytes, 4) as usize == bytes.len()
        && bytes.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)) == 0
}
fn read32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn read64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn srat(out: &mut Topology, bytes: &[u8], apics: &[u32], qualified: u64) -> Result<(), Reason> {
    if !table(bytes, b"SRAT", 48)
        || !(1..=3).contains(&bytes[8])
        || read32(bytes, 36) != 1
        || read64(bytes, 40) != 0
    {
        return Err(Reason::Invalid);
    }
    if apics.is_empty()
        || apics.len() > MAX_CPUS
        || qualified == 0
        || qualified & !(u64::MAX >> (64 - apics.len())) != 0
    {
        return Err(Reason::CpuMap);
    }
    for (i, &apic) in apics.iter().enumerate() {
        if apic == u32::MAX || apics[..i].contains(&apic) {
            return Err(Reason::CpuMap);
        }
    }
    let mut affinities = [EMPTY_AFFINITY; MAX_AFFINITIES];
    let mut affinity_count = 0;
    let mut cursor = 48;
    let mut records = 0;
    while cursor < bytes.len() {
        if records == MAX_RECORDS {
            return Err(Reason::Bounds);
        }
        records += 1;
        let header = bytes.get(cursor..cursor + 2).ok_or(Reason::Invalid)?;
        let len = header[1] as usize;
        if len < 2 {
            return Err(Reason::Invalid);
        }
        let entry = bytes.get(cursor..cursor + len).ok_or(Reason::Invalid)?;
        let cpu = match entry[0] {
            0 => {
                if len != 16 || read32(entry, 4) & !1 != 0 {
                    return Err(Reason::Invalid);
                }
                if read32(entry, 4) & 1 != 0 {
                    // Local SAPIC EIDs are not x86 local/x2APIC extensions.
                    if entry[8] != 0 {
                        return Err(Reason::CpuMap);
                    }
                    let high = if bytes[8] >= 2 {
                        u32::from_le_bytes([0, entry[9], entry[10], entry[11]])
                    } else {
                        0
                    };
                    Some((u32::from(entry[3]), u32::from(entry[2]) | high))
                } else {
                    None
                }
            }
            2 => {
                if len != 24 || bytes[8] < 2 || read32(entry, 12) & !1 != 0 {
                    return Err(Reason::Invalid);
                }
                (read32(entry, 12) & 1 != 0).then(|| (read32(entry, 8), read32(entry, 4)))
            }
            1 => {
                if len != 40 || read32(entry, 28) & !15 != 0 {
                    return Err(Reason::Invalid);
                }
                if read32(entry, 28) & 1 != 0 {
                    let base = read64(entry, 8);
                    let length = read64(entry, 16);
                    let end = base
                        .checked_add(length)
                        .filter(|_| length != 0)
                        .ok_or(Reason::Invalid)?;
                    if out.range_count == MAX_RANGES {
                        return Err(Reason::Bounds);
                    }
                    if out.ranges[..out.range_count]
                        .iter()
                        .any(|r| base < r.end && r.base < end)
                    {
                        return Err(Reason::Invalid);
                    }
                    let node = out.node(read32(entry, 2))?;
                    out.ranges[out.range_count] = Range { base, end, node };
                    out.range_count += 1;
                }
                None
            }
            // Other architectures, generic initiators/ports and future
            // bounded records do not describe schedulable x86 CPUs or RAM.
            _ => None,
        };
        if let Some((apic, domain)) = cpu {
            if apic == u32::MAX || affinities[..affinity_count].iter().any(|a| a.apic == apic) {
                return Err(Reason::CpuMap);
            }
            if affinity_count == MAX_AFFINITIES {
                return Err(Reason::Bounds);
            }
            let node = out.node(domain)?;
            affinities[affinity_count] = Affinity { apic, node };
            affinity_count += 1;
        }
        cursor += len;
    }
    if out.range_count == 0 {
        return Err(Reason::Invalid);
    }
    for (i, &apic) in apics.iter().enumerate() {
        if qualified & (1 << i) == 0 {
            continue;
        }
        let affinity = affinities[..affinity_count]
            .iter()
            .find(|a| a.apic == apic)
            .ok_or(Reason::CpuMap)?;
        out.cpu_nodes[i] = Some(affinity.node);
    }
    out.ranges[..out.range_count].sort_unstable_by_key(|r| r.base);
    out.qualified = qualified;
    out.reason = Reason::Known;
    Ok(())
}
fn slit(out: &mut Topology, bytes: &[u8]) {
    if !table(bytes, b"SLIT", 44) || bytes[8] != 1 {
        return;
    }
    let n = read64(bytes, 36);
    if n == 0 || n > MAX_SLIT as u64 || bytes.len() != 44 + (n * n) as usize {
        return;
    }
    let n = n as usize;
    // A valid unreachable255 entry remains unavailable for finite scheduling
    // guidance. SRAT membership still works, with no fabricated remote value.
    for i in 0..n {
        for j in 0..n {
            let d = bytes[44 + i * n + j];
            if (i == j && d != 10) || (i != j && !(11..255).contains(&d)) {
                return;
            }
        }
    }
    if out.nodes[..out.node_count]
        .iter()
        .any(|&id| id as usize >= n)
    {
        return;
    }
    for (i, &from) in out.nodes[..out.node_count].iter().enumerate() {
        for (j, &to) in out.nodes[..out.node_count].iter().enumerate() {
            out.distances[i][j] = bytes[44 + from as usize * n + to as usize];
        }
    }
    out.slit_valid = true;
}
pub(crate) fn decode(
    srat_bytes: Option<&[u8]>,
    slit_bytes: Option<&[u8]>,
    apics: &[u32],
    qualified: u64,
) -> Topology {
    let Some(bytes) = srat_bytes else {
        return Topology::EMPTY;
    };
    let mut out = Topology::EMPTY;
    if let Err(reason) = srat(&mut out, bytes, apics, qualified) {
        return Topology {
            reason,
            ..Topology::EMPTY
        };
    }
    if let Some(bytes) = slit_bytes {
        slit(&mut out, bytes);
    }
    out
}
