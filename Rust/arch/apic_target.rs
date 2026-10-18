//! Physical interrupt destinations. CPU identity and boot epochs are kept
//! intact; each transport validates its own format before forming fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub index: u32,
    pub apic_id: u32,
    pub epoch: u64,
}
pub(crate) fn legacy_id(id: u64) -> Option<u8> {
    // FF is the legacy physical broadcast destination, never one CPU.
    (id < 0xff).then_some(id as u8)
}
pub(crate) fn select(ordinal: u32, mut probe: impl FnMut(u32) -> Option<Target>) -> Option<Target> {
    let mut candidates = [None; super::smp_policy::MAX_CPUS];
    let mut count = 0;
    for index in 0..super::smp_policy::MAX_CPUS as u32 {
        if let Some(target) = probe(index).filter(|t| {
            t.index == index && t.epoch != 0 && legacy_id(u64::from(t.apic_id)).is_some()
        }) {
            candidates[count] = Some(target);
            count += 1;
        }
    }
    if count == 0 {
        None
    } else {
        candidates[ordinal as usize % count]
    }
}
pub(crate) fn current(target: Target, mut probe: impl FnMut(u32) -> Option<Target>) -> bool {
    target.index < super::smp_policy::MAX_CPUS as u32
        && target.apic_id != u32::MAX
        && target.epoch != 0
        && probe(target.index) == Some(target)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Icr {
    pub low: u32,
    pub high: u32,
}
impl Icr {
    pub(crate) fn msr(self) -> u64 {
        (u64::from(self.high) << 32) | u64::from(self.low)
    }
}
pub(crate) fn icr(id: u64, low: u32, x2: bool) -> Option<Icr> {
    let id = u32::try_from(id).ok()?;
    if id == u32::MAX || (!x2 && legacy_id(u64::from(id)).is_none()) {
        return None;
    }
    // This owner sends individual physical fixed/INIT/SIPI messages only.
    // Logical destinations, shorthand broadcasts and reserved fields cannot
    // silently turn a checked single-CPU request into another operation.
    if low & !0xc7ff != 0 {
        return None;
    }
    let vector = low & 255;
    match (low >> 8) & 7 {
        0 if vector >= 0x20 && low & 0xc000 == 0 => {}
        5 if vector == 0 => {}
        6 if vector != 0 && low & 0xc000 == 0 => {}
        _ => return None,
    }
    Some(Icr {
        low,
        high: if x2 { id } else { id << 24 },
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Message {
    pub address: u32,
    pub data: u32,
}
pub(crate) fn message(id: u64, vector: u32) -> Option<Message> {
    let id = legacy_id(id)?;
    if !(0x20..=0xff).contains(&vector) {
        return None;
    }
    Some(Message {
        address: 0xfee0_0000 | (u32::from(id) << 12),
        data: vector,
    })
}
pub(crate) fn redirection_high(id: u64) -> Option<u32> {
    Some(u32::from(legacy_id(id)?) << 24)
}
