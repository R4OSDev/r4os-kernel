//! Original bounded topology, placement, TLB-mask and synthetic-stack policy.
//! Fixed C snapshots preserve the existing Zig fixtures and assembly contract.
pub const MAX_CPUS: usize = 32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Candidate {
    pub apic_id: u32,
    pub enabled: bool,
    pub online_capable: bool,
}
#[repr(C)]
pub struct Plan {
    pub apic_ids: [u32; MAX_CPUS],
    pub count: u32,
    pub duplicates: u32,
    pub disabled: u32,
}
pub(crate) fn plan(bsp: u32, candidates: &[Candidate]) -> Plan {
    let mut result = Plan {
        apic_ids: [0; MAX_CPUS],
        count: 1,
        duplicates: 0,
        disabled: 0,
    };
    result.apic_ids[0] = bsp;
    for candidate in candidates {
        if !candidate.enabled && !candidate.online_capable {
            result.disabled += 1;
            continue;
        }
        if result.apic_ids[..result.count as usize].contains(&candidate.apic_id) {
            result.duplicates += 1;
            continue;
        }
        if result.count as usize >= MAX_CPUS {
            break;
        }
        result.apic_ids[result.count as usize] = candidate.apic_id;
        result.count += 1;
    }
    result
}
/// # Safety
/// Caller owns aligned initialized candidates for len elements and exclusive
/// aligned Plan output. A zero-length input may be null. No callback occurs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_smp_plan(
    bsp: u32,
    candidates: *const Candidate,
    len: usize,
    output: *mut Plan,
) {
    let candidates = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(candidates, len) }
    };
    unsafe { output.write(plan(bsp, candidates)) };
}
/// # Safety
/// Caller owns aligned initialized loads for len elements until return. A
/// zero-length input may be null; selection preserves permanent CPU0 fallback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_smp_least_loaded(
    mask: u64,
    loads: *const usize,
    len: usize,
) -> u32 {
    if mask & 1 == 0 || len == 0 {
        return 0;
    }
    let loads = unsafe { core::slice::from_raw_parts(loads, len) };
    let mut selected = 0;
    let mut selected_load = loads[0];
    for (index, &load) in loads.iter().enumerate().take(MAX_CPUS).skip(1) {
        if mask & (1 << index) != 0 && load < selected_load {
            selected = index as u32;
            selected_load = load;
        }
    }
    selected
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tlb_mask(operation: u8, first: u64, second: u64) -> u64 {
    match operation {
        0 => {
            if second >= 64 {
                first
            } else {
                first & !(1 << second)
            }
        }
        1 => u64::from(second & first == first),
        2 => first & !second,
        _ => panic!("Invalid private TLB policy operation"),
    }
}
#[repr(C)]
pub struct InitialStack {
    pub aligned_top: u64,
    pub entry_rsp: u64,
    pub return_slot: u64,
    pub switch_rsp: u64,
}
/// # Safety
/// Caller owns aligned InitialStack output. Stack top follows the original
/// resident stack contract; insufficient space preserves checked underflow.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_initial_stack(top: u64, output: *mut InitialStack) {
    let aligned_top = top & !15;
    let entry_rsp = aligned_top - 8;
    let return_slot = entry_rsp - 8;
    unsafe {
        output.write(InitialStack {
            aligned_top,
            entry_rsp,
            return_slot,
            switch_rsp: return_slot - 6 * 8,
        })
    };
}
const _: () = {
    assert!(core::mem::size_of::<Candidate>() == 8);
    assert!(core::mem::offset_of!(Candidate, enabled) == 4);
    assert!(core::mem::size_of::<Plan>() == 140);
    assert!(core::mem::offset_of!(Plan, count) == 128);
    assert!(core::mem::size_of::<InitialStack>() == 32);
};
