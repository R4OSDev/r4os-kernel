//! One explicit case in the existing program-storage diagnostic. Independent
//! published2 slot/header map, retired-name refusal and a real table call.
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
    // Independent frozen94 published2 ABI. These literal positions and sizes
    // survive removal of the old live providers; no production regeneration
    // constructs this expected map. Every original active tail/call follows.
    let sizes = [1552, 632, 920, 296, 240, 720];
    let magics = [
        0x32595352, 0x32454452, 0x32574452, 0x32454e52, 0x32554152, 0x32564452,
    ];
    for (index, name) in names.iter().enumerate() {
        let group = (index + 1) as u32;
        let new = runtime_tables::interface2(group);
        if new == 0 {
            return false;
        }
        let after = unsafe { ptr::read(new as *const Header) };
        if after.size != sizes[index]
            || after.magic != magics[index]
            || after.version
                != if group == 1 {
                    6
                } else if group == 3 || group == 5 || group == 6 {
                    2
                } else {
                    1
                }
            || after.flags != 0
        {
            return false;
        }
        for slot in 0..((after.size as usize - 16) / 8) {
            let reserved = match group {
                1 => [24, 25].contains(&slot),
                2 => slot == 37,
                5 => [19, 20].contains(&slot),
                6 => [25, 26].contains(&slot),
                _ => false,
            };
            let address = unsafe { ptr::read((new + 16 + 8 * slot) as *const u64) };
            if (address == 0) != reserved {
                return false;
            }
        }
        let mut info = mem::MaybeUninit::uninit();
        if !unsafe {
            registry::r4os_rust_mod_resolve_info(
                registry::span(name),
                registry::span(b"Query2"),
                2,
                info.as_mut_ptr(),
            )
        } {
            return false;
        }
        let info = unsafe { info.assume_init() };
        if info.available_size != 24 || info.version != 2 || info.kind != 5 || info.generation == 0
        {
            return false;
        }
        let query = unsafe { &*(info.address as *const c::generation2::R4LQuery2) };
        if query.magic != c::generation2::QUERY_MAGIC
            || query.abi_version != 2
            || query.size != 24
            || query.group != group
            || query.reserved != 0
        {
            return false;
        }
        let mut rejected = mem::MaybeUninit::<registry::ExportInfo>::uninit();
        for minimum in [1, 2] {
            if unsafe {
                registry::r4os_rust_mod_resolve_info(
                    registry::span(name),
                    registry::span(b"Query"),
                    minimum,
                    rejected.as_mut_ptr(),
                )
            } {
                return false;
            }
        }
        if unsafe {
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
        b"[API2] queries=6 slots=533 active_callbacks=526 reserves=7 retired_query1=absent waitset_tail=present source_adapters=present pipe_launch_capture_tail=present call=ok\r\n",
    );
    true
}

fn start_admission() -> bool {
    use crate::program::policy;
    let old = b"r4x.start=r4xstart\0";
    let new = b"r4x.start=r4xstart2\0r4x.entry=R4XStart2\0r4x.start_abi=2\0r4x.context=R4XStartContext2\0";
    for (metadata, first, second, expected) in [
        (old.as_slice(), true, false, None),
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
        || policy::start_export(&mut first, &mut second, b"R4XStart", 1)
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

/// The sole active120-byte context borrows the resident instance storage.
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
        super::storage::rollback(owner, &mut storage);
        let valid = valid && super::baseline(heap, baseline);
        if valid {
            super::log(
                b"[API2] context=120 retired_start1=reject borrowed_args=shared imports=shared rollback=ok\r\n",
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
    // Frozen94 exact positive Query1 prefix: magic/ABI1,32 bytes, independent
    // group0 and zero bridge/reserved. It is now refused before record access.
    let legacy: [u64; 4] = [0x00000001314c3452, 32, 0, 0];
    section.file_size = 32;
    section.mem_size = 32;
    input.name = super::span(b"Query");
    input.version = 1;
    input.image = crate::fs::vfs_types::Bytes {
        data: legacy.as_ptr().cast(),
        len: 32,
    };
    let reason = unsafe { r4os_rust_mod_loaded_export(&input) };
    if unsafe { core::slice::from_raw_parts(reason.data, reason.len) }
        != b"retired-platform-generation"
    {
        return false;
    }
    // Empty section/image bounds remain valid inputs; the same retirement
    // result must precede even section/range or payload interpretation.
    input.section_count = 0;
    input.offset_count = 0;
    input.image = super::span(&[]);
    let reason = unsafe { r4os_rust_mod_loaded_export(&input) };
    if unsafe { core::slice::from_raw_parts(reason.data, reason.len) }
        != b"retired-platform-generation"
    {
        return false;
    }
    core::hint::black_box((&query, &section, &legacy));
    if valid {
        super::log(b"[API2] runtime_query2=24 bad_revision/size/group/reserved/range=reject\r\n");
    }
    valid
}
