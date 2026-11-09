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
    if !start_admission() {
        return false;
    }
    let names: [&[u8]; 6] = [
        b"R4SYS", b"R4DESK", b"R4DRAW", b"R4NET", b"R4AUDIO", b"R4DEV",
    ];
    // Independent original mapping through SYS35/AUDIO37 appends;
    // never regenerate expected positions with the production projector.
    let sizes = [
        (1560, 1552),
        (632, 632),
        (920, 920),
        (296, 296),
        (240, 240),
        (744, 720),
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
            || after.version
                != if group == 1 {
                    6
                } else if group == 3 || group == 5 || group == 6 {
                    2
                } else {
                    1
                }
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
    if sys.ticks == 0
        || sys.waitset_create == 0
        || sys.waitset_enroll == 0
        || sys.waitset_rearm == 0
        || sys.waitset_remove == 0
        || sys.waitset_wait == 0
        || sys.waitset_close == 0
        || sys.waitset_source_prepare == 0
        || sys.byte_pipe_create == 0
        || sys.byte_pipe_dup == 0
        || sys.byte_pipe_read == 0
        || sys.byte_pipe_write == 0
        || sys.byte_pipe_cancel == 0
        || sys.byte_pipe_close == 0
        || sys.byte_pipe_info == 0
        || sys.program_launch_io == 0
        || sys.program_stdio == 0
    {
        return false;
    }
    let ticks: c::generation2::R4SysFns2::ticks = unsafe { mem::transmute(sys.ticks) };
    let before = unsafe { ticks() };
    if unsafe { ticks() } < before {
        return false;
    }
    let audio =
        unsafe { &*(runtime_tables::interface2(5) as *const c::generation2::R4XStartR4Audio2) };
    if audio.audio_capture_io == 0 {
        return false;
    }
    super::log(
        b"[API2] queries=6 slots=533 shared_callbacks=ok legacy_tombstones=zero waitset_tail=present source_adapters=present pipe_launch_capture_tail=present call=ok\r\n",
    );
    true
}

fn start_admission() -> bool {
    use crate::program::policy;
    let old = b"r4x.start=r4xstart\0";
    let new = b"r4x.start=r4xstart2\0r4x.entry=R4XStart2\0r4x.start_abi=2\0r4x.context=R4XStartContext2\0";
    for (metadata, first, second, expected) in [
        (old.as_slice(), true, false, Some(1)),
        (new.as_slice(), false, true, Some(2)),
        (old.as_slice(), false, true, None),
        (new.as_slice(), true, false, None),
        (new.as_slice(), true, true, None),
        (new.as_slice(), false, false, None),
        (b"r4x.start=r4xstart2\0", false, true, None),
        (b"r4x.start=r4xstart2\0r4x.entry=R4XStart2\0r4x.start_abi=1\0r4x.context=R4XStartContext2\0", false, true, None),
        (b"r4x.start=r4xstart2\0r4x.start=r4xstart2\0r4x.entry=R4XStart2\0r4x.start_abi=2\0r4x.context=R4XStartContext2\0", false, true, None),
    ] {
        if policy::start_generation(metadata, first, second, 1) != expected { return false; }
    }
    let (mut first, mut second) = (false, false);
    if policy::start_export(&mut first, &mut second, b"R4XStart", 2)
        || policy::start_export(&mut first, &mut second, b"R4XStart2", 1)
        || !policy::start_export(&mut first, &mut second, b"R4XStart2", 2)
        || policy::start_export(&mut first, &mut second, b"R4XStart2", 2)
    {
        return false;
    }
    super::log(
        b"[API2] start_identity=exact wrong_revision=reject duplicate=reject mixed=reject\r\n",
    );
    true
}

/// Both physical context layouts borrow the same resident instance storage.
pub(super) unsafe fn contexts(heap: &super::heap::Stats, baseline: &super::t::Stats) -> bool {
    unsafe {
        let owner = 0xfffd_0014;
        let Some(mut storage) = super::storage::allocate(owner, 2, false) else {
            return false;
        };
        let mut instance = super::fake(owner, 2, storage);
        let runtime = storage.runtime;
        (*runtime).start_generation = 2;
        (*runtime).r4xstart_import_count = 1;
        (*runtime).r4xstart_imports[0] = c::R4XStartImport {
            group_id: 1,
            min_version: 2,
            resolved_version: 2,
            flags: c::r4xstart_import_flag_group_interface,
            table: runtime_tables::interface2(1) as u64,
            ..c::R4XStartImport::default()
        };
        crate::program::entry::r4os_rust_program_entry_context(&mut instance);
        let second = &(*runtime).r4xstart_context2;
        let valid = second.magic == 0x3258_3452
            && second.size == 120
            && second.abi_major == 2
            && second.instance_id == u64::from(owner)
            && second.app_class == 3
            && second.import_count == 1
            && second.imports == ptr::addr_of!((*runtime).r4xstart_imports) as u64
            && second.args == ptr::addr_of!((*storage.process).args) as u64
            && second.args_len == 0
            && second.reserved2 == 0
            && second.should_close != 0
            && second.r#yield != 0;
        (*runtime).start_generation = 1;
        crate::program::entry::r4os_rust_program_entry_context(&mut instance);
        let first = &(*runtime).r4xstart_context;
        let valid = valid
            && first.magic == c::r4xstart_magic
            && first.size == 128
            && first.abi_major == 1
            && first.reserved_runtime == 0
            && first.args == (*runtime).r4xstart_context2.args
            && first.imports == (*runtime).r4xstart_context2.imports;
        super::storage::rollback(owner, &mut storage);
        let valid = valid && super::baseline(heap, baseline);
        if valid {
            super::log(
                b"[API2] contexts=128/120 borrowed_args=shared imports=shared rollback=ok\r\n",
            );
        }
        valid
    }
}

/// Test the real loaded-export admission on exact resident byte prefixes.
pub(super) fn query_admission() -> bool {
    use crate::loader::format::{LoadedExport, Section, r4os_rust_mod_loaded_export};
    let mut query = c::generation2::R4LQuery2::default();
    let mut section = Section {
        name: [0; 8],
        name_len: 0,
        flags: 1,
        file_off: 0,
        file_size: 24,
        mem_size: 24,
        alignment: 8,
    };
    let offset = 0usize;
    let mut input = LoadedExport {
        name: super::span(b"Query2"),
        version: 2,
        section_index: 0,
        export_offset: 0,
        sections: ptr::addr_of!(section),
        section_count: 1,
        offsets: ptr::addr_of!(offset),
        offset_count: 1,
        image: crate::fs::vfs_types::Bytes {
            data: ptr::addr_of!(query).cast(),
            len: 24,
        },
    };
    let accepted = |value: &LoadedExport| unsafe { r4os_rust_mod_loaded_export(value).len == 0 };
    if !accepted(&input) {
        return false;
    }
    input.version = 1;
    if accepted(&input) {
        return false;
    }
    input.version = 2;
    query.size = 32;
    if accepted(&input) {
        return false;
    }
    query.size = 24;
    query.group = 1;
    if accepted(&input) {
        return false;
    }
    query.group = 0;
    query.reserved = 1;
    if accepted(&input) {
        return false;
    }
    query.reserved = 0;
    section.mem_size = 23;
    if accepted(&input) {
        return false;
    }
    section.mem_size = 24;
    let valid = accepted(&input);
    core::hint::black_box((&query, &section));
    if valid {
        super::log(b"[API2] runtime_query2=24 bad_revision/size/group/reserved/range=reject\r\n");
    }
    valid
}
