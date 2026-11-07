//! One explicit case in the existing program-storage diagnostic. Independent
//! initial slot map, both real registry exports and a real new-table call.
use crate::{contract as c, loader::registry_control as registry, program::runtime_tables};
use core::{mem, ptr};
#[derive(Clone, Copy)]
#[repr(C)]
struct Header {
    magic: u32,
    version: u32,
    size: u32,
    flags: u32,
}

pub(super) fn providers() -> bool {
    let names: [&[u8]; 6] = [
        b"R4SYS", b"R4DESK", b"R4DRAW", b"R4NET", b"R4AUDIO", b"R4DEV",
    ];
    // Immutable 0.90.11 mapping, rather than regenerating expected slots
    // with the production projector being checked.
    let sizes = [
        (1416, 1408),
        (632, 632),
        (912, 912),
        (296, 296),
        (232, 232),
        (736, 712),
    ];
    for (index, name) in names.iter().enumerate() {
        let group = (index + 1) as u32;
        let old = runtime_tables::interface(group);
        let new = runtime_tables::interface2(group);
        if old == 0 || new == 0 || old == new {
            return false;
        }
        let before = unsafe { ptr::read(old as *const Header) };
        let after = unsafe { ptr::read(new as *const Header) };
        if before.size != sizes[index].0
            || after.size != sizes[index].1
            || before.magic >> 24 != u32::from(b'1')
            || after.magic >> 24 != u32::from(b'2')
            || (before.magic & 0xffffff) != (after.magic & 0xffffff)
            || after.version != 1
            || before.flags != after.flags
        {
            return false;
        }
        let mut target = 0;
        for slot in 0..((before.size as usize - 16) / 8) {
            let address = unsafe { ptr::read((old + 16 + 8 * slot) as *const u64) };
            let dropped =
                (group == 1 && slot == 49) || (group == 6 && [27, 29, 30].contains(&slot));
            if dropped {
                if address != 0 {
                    return false;
                }
            } else {
                if unsafe { ptr::read((new + 16 + 8 * target) as *const u64) } != address {
                    return false;
                }
                target += 1;
            }
        }
        if target * 8 + 16 != after.size as usize {
            return false;
        }
        let mut first = mem::MaybeUninit::uninit();
        let mut second = mem::MaybeUninit::uninit();
        if !unsafe {
            registry::r4os_rust_mod_resolve_info(
                registry::span(name),
                registry::span(b"Query"),
                1,
                first.as_mut_ptr(),
            )
        } || !unsafe {
            registry::r4os_rust_mod_resolve_info(
                registry::span(name),
                registry::span(b"Query2"),
                2,
                second.as_mut_ptr(),
            )
        } {
            return false;
        }
        let first = unsafe { first.assume_init() };
        let second = unsafe { second.assume_init() };
        if first.available_size != 32
            || second.available_size != 24
            || first.version != 1
            || second.version != 2
            || first.module_slot != second.module_slot
            || first.generation != second.generation
            || first.address == second.address
        {
            return false;
        }
        let old_query = unsafe { &*(first.address as *const c::R4LQuery) };
        let new_query = unsafe { &*(second.address as *const c::generation2::R4LQuery2) };
        if old_query.magic != c::r4l_abi_magic
            || old_query.abi_version != 1
            || old_query.size != 32
            || old_query.group != group
            || old_query.kernel_bridge != 0
            || new_query.magic != c::generation2::QUERY_MAGIC
            || new_query.abi_version != 2
            || new_query.size != 24
            || new_query.group != group
            || new_query.reserved != 0
        {
            return false;
        }
        let mut rejected = mem::MaybeUninit::<registry::ExportInfo>::uninit();
        if unsafe {
            registry::r4os_rust_mod_resolve_info(
                registry::span(name),
                registry::span(b"Query"),
                2,
                rejected.as_mut_ptr(),
            )
        } || unsafe {
            registry::r4os_rust_mod_resolve_info(
                registry::span(name),
                registry::span(b"Query2"),
                3,
                rejected.as_mut_ptr(),
            )
        } {
            return false;
        }
    }
    let sys = unsafe { &*(runtime_tables::interface2(1) as *const c::generation2::R4XStartR4Sys2) };
    if sys.ticks == 0 {
        return false;
    }
    let ticks: c::generation2::R4SysFns2::ticks = unsafe { mem::transmute(sys.ticks) };
    let before = unsafe { ticks() };
    if unsafe { ticks() } < before {
        return false;
    }
    super::log(
        b"[API2] queries=6 slots=512 shared_callbacks=ok legacy_tombstones=zero call=ok\r\n",
    );
    true
}
