//! Normalize bootloader data once, before any allocator or task exists.
//! Zig readers receive only C layouts and immutable views of resident storage.
use super::limine;
use core::ptr::{addr_of, addr_of_mut, null, null_mut};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryEntry {
    pub base: u64,
    pub length: u64,
    pub end: u64,
    pub usable_base: u64,
    pub usable_len: u64,
    pub kind: u8,
    pub valid: bool,
}
impl MemoryEntry {
    const EMPTY: Self = Self {
        base: 0,
        length: 0,
        end: 0,
        usable_base: 0,
        usable_len: 0,
        kind: 8,
        valid: false,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootModule {
    address: *const u8,
    size: usize,
    path: *const u8,
    path_len: usize,
    cmdline: *const u8,
    cmdline_len: usize,
    valid: bool,
}
impl BootModule {
    const EMPTY: Self = Self {
        address: null(),
        size: 0,
        path: null(),
        path_len: 0,
        cmdline: null(),
        cmdline_len: 0,
        valid: false,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExecutableSource {
    present: bool,
    media_type: u32,
    partition_index: u32,
    mbr_disk_id: u32,
    disk_guid: [u8; 16],
    partition_guid: [u8; 16],
    filesystem_uuid: [u8; 16],
    path_bytes: [u8; 256],
    path_len: usize,
    path_truncated: bool,
}
impl ExecutableSource {
    const EMPTY: Self = Self {
        present: false,
        media_type: 0,
        partition_index: 0,
        mbr_disk_id: 0,
        disk_guid: [0; 16],
        partition_guid: [0; 16],
        filesystem_uuid: [0; 16],
        path_bytes: [0; 256],
        path_len: 0,
        path_truncated: false,
    };
}
#[repr(C)]
pub struct Snapshot {
    pub flags: u32,
    pub invalid_entries: u64,
    pub hhdm: u64,
    pub rsdp: u64,
    pub memory: *const MemoryEntry,
    pub memory_count: usize,
    pub modules: *const BootModule,
    pub module_count: usize,
    pub executable: *const ExecutableSource,
    pub framebuffer: *mut limine::Framebuffer,
}
impl Snapshot {
    const EMPTY: Self = Self {
        flags: 0,
        invalid_entries: 0,
        hhdm: 0,
        rsdp: 0,
        memory: null(),
        memory_count: 0,
        modules: null(),
        module_count: 0,
        executable: null(),
        framebuffer: null_mut(),
    };
}
static mut MEMORY: [MemoryEntry; 128] = [MemoryEntry::EMPTY; 128];
static mut MODULES: [BootModule; 16] = [BootModule::EMPTY; 16];
static mut EXECUTABLE: ExecutableSource = ExecutableSource::EMPTY;
static mut FRAMEBUFFER: core::mem::MaybeUninit<limine::Framebuffer> =
    core::mem::MaybeUninit::uninit();
static mut CURRENT: Snapshot = Snapshot::EMPTY;

const _: () = {
    assert!(
        core::mem::size_of::<MemoryEntry>() == 48 && core::mem::offset_of!(MemoryEntry, kind) == 40
    );
    assert!(
        core::mem::size_of::<BootModule>() == 56 && core::mem::offset_of!(BootModule, valid) == 48
    );
    assert!(
        core::mem::size_of::<ExecutableSource>() == 336
            && core::mem::offset_of!(ExecutableSource, path_len) == 320
    );
    assert!(
        core::mem::size_of::<Snapshot>() == 80 && core::mem::offset_of!(Snapshot, memory) == 32
    );
};
fn normalize(base: u64, length: u64, raw_kind: u64) -> Option<MemoryEntry> {
    if length == 0 {
        return None;
    }
    let end = base.checked_add(length)?;
    let kind = if raw_kind <= 7 { raw_kind as u8 } else { 8 };
    let mut entry = MemoryEntry {
        base,
        length,
        end,
        kind,
        valid: true,
        ..MemoryEntry::EMPTY
    };
    if kind == 0 {
        entry.usable_base = base.checked_add(4095)? & !4095;
        entry.usable_len = (end & !4095).saturating_sub(entry.usable_base);
    }
    Some(entry)
}
unsafe fn zlen(text: *const u8, limit: usize) -> usize {
    if text.is_null() {
        return 0;
    }
    let mut len = 0;
    while len < limit && unsafe { text.add(len).read() } != 0 {
        len += 1;
    }
    len
}
fn uuid_bytes(uuid: limine::Uuid) -> [u8; 16] {
    let mut result = [0; 16];
    result[..4].copy_from_slice(&uuid.a.to_le_bytes());
    result[4..6].copy_from_slice(&uuid.b.to_le_bytes());
    result[6..8].copy_from_slice(&uuid.c.to_le_bytes());
    result[8..].copy_from_slice(&uuid.d);
    result
}
fn software_once(text: &[u8]) -> bool {
    text.split(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .any(|token| token.eq_ignore_ascii_case(b"r4os.graphics=software"))
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_info() -> *const Snapshot {
    addr_of!(CURRENT)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_info_init() -> bool {
    // SAFETY: only the BSP calls this before publication, reclaim and AP start.
    // Raw writes avoid exclusive global borrows; all later consumers read only.
    unsafe {
        let current = addr_of_mut!(CURRENT);
        current.write(Snapshot::EMPTY);
        addr_of_mut!(EXECUTABLE).write(ExecutableSource::EMPTY);
        (*current).executable = addr_of!(EXECUTABLE);
        let mem = limine::r4os_rust_limine_response(1).cast::<limine::MemoryMapResponse>();
        if mem.is_null() {
            return false;
        }
        let mut stored = 0;
        for index in 0..(*mem).entry_count.min(128) as usize {
            let source = (*mem).entries.add(index).read();
            if let Some(entry) = normalize((*source).base, (*source).length, (*source).kind) {
                addr_of_mut!(MEMORY)
                    .cast::<MemoryEntry>()
                    .add(stored)
                    .write(entry);
                stored += 1;
            } else {
                (*current).invalid_entries += 1;
            }
        }
        (*current).memory = addr_of!(MEMORY).cast();
        (*current).memory_count = stored;
        if (*mem).entry_count > 128 {
            (*current).flags |= 2;
        }
        let hhdm = limine::r4os_rust_limine_response(5).cast::<limine::HhdmResponse>();
        if hhdm.is_null() {
            return false;
        }
        (*current).hhdm = (*hhdm).offset;
        (*current).flags |= 4;
        let modules = limine::r4os_rust_limine_response(2).cast::<limine::ModuleResponse>();
        (*current).modules = addr_of!(MODULES).cast();
        if !modules.is_null() {
            for index in 0..(*modules).module_count.min(16) as usize {
                let file = (*modules).modules.add(index).read();
                if (*file).address.is_null() || (*file).size == 0 {
                    continue;
                }
                let slot = (*current).module_count;
                addr_of_mut!(MODULES)
                    .cast::<BootModule>()
                    .add(slot)
                    .write(BootModule {
                        address: (*file).address,
                        size: (*file).size as usize,
                        path: (*file).path,
                        path_len: zlen((*file).path, 256),
                        cmdline: (*file).cmdline,
                        cmdline_len: zlen((*file).cmdline, 256),
                        valid: true,
                    });
                (*current).module_count += 1;
            }
        }
        let executable =
            limine::r4os_rust_limine_response(3).cast::<limine::ExecutableFileResponse>();
        if !executable.is_null() {
            let file = (*executable).file;
            let output = addr_of_mut!(EXECUTABLE);
            (*output).present = true;
            (*output).media_type = (*file).media_type;
            (*output).partition_index = (*file).partition_index;
            (*output).mbr_disk_id = (*file).mbr_disk_id;
            (*output).disk_guid = uuid_bytes((*file).gpt_disk_uuid);
            (*output).partition_guid = uuid_bytes((*file).gpt_part_uuid);
            (*output).filesystem_uuid = uuid_bytes((*file).part_uuid);
            let len = zlen((*file).path, 256);
            (*output).path_len = len;
            (*output).path_truncated = len == 256;
            if len != 0 {
                core::ptr::copy_nonoverlapping(
                    (*file).path,
                    addr_of_mut!((*output).path_bytes).cast::<u8>(),
                    len,
                );
            }
            let len = zlen((*file).cmdline, 512);
            if len != 0
                && len < 512
                && software_once(core::slice::from_raw_parts((*file).cmdline, len))
            {
                (*current).flags |= 32;
            }
        }
        let fb = limine::r4os_rust_limine_response(0).cast::<limine::FramebufferResponse>();
        if !fb.is_null() && (*fb).framebuffer_count != 0 {
            let source = (*fb).framebuffers.read();
            let output = addr_of_mut!(FRAMEBUFFER).cast::<limine::Framebuffer>();
            output.write(source.read());
            (*current).framebuffer = output;
            (*current).flags |= 8;
        }
        let rsdp = limine::r4os_rust_limine_response(4).cast::<limine::RsdpResponse>();
        if !rsdp.is_null() && (*rsdp).address != 0 {
            let address = (*rsdp).address;
            (*current).rsdp = address.checked_sub((*current).hhdm).unwrap_or(address);
            (*current).flags |= 16;
        }
        (*current).flags |= 1;
        true
    }
}
