//! Bootloader-owned responses are read through volatile raw pointers.
//! Original protocol provenance and 0BSD notice: bootloader/LIMINE-PROTOCOL-LICENSE.txt.
use core::ptr::{addr_of, addr_of_mut, null_mut, read_volatile};

#[repr(C)]
pub struct Request<T> {
    id: [u64; 4],
    revision: u64,
    response: *mut T,
}
impl<T> Request<T> {
    const fn new(first: u64, second: u64) -> Self {
        Self {
            id: [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b, first, second],
            revision: 0,
            response: null_mut(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Framebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub unused: [u8; 5],
    pub edid_size: u64,
    pub edid: *const core::ffi::c_void,
}
#[repr(C)]
pub struct FramebufferResponse {
    pub revision: u64,
    pub framebuffer_count: u64,
    pub framebuffers: *const *mut Framebuffer,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub length: u64,
    pub kind: u64,
}
#[repr(C)]
pub struct MemoryMapResponse {
    pub revision: u64,
    pub entry_count: u64,
    pub entries: *const *const MemoryMapEntry,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Uuid {
    pub a: u32,
    pub b: u16,
    pub c: u16,
    pub d: [u8; 8],
}
#[repr(C)]
pub struct File {
    pub revision: u64,
    pub address: *const u8,
    pub size: u64,
    pub path: *const u8,
    pub cmdline: *const u8,
    pub media_type: u32,
    pub unused: u32,
    pub tftp_ipv4: [u8; 4],
    pub tftp_port: u32,
    pub partition_index: u32,
    pub mbr_disk_id: u32,
    pub gpt_disk_uuid: Uuid,
    pub gpt_part_uuid: Uuid,
    pub part_uuid: Uuid,
}
#[repr(C)]
pub struct ExecutableFileResponse {
    pub revision: u64,
    pub file: *const File,
}
#[repr(C)]
pub struct ModuleResponse {
    pub revision: u64,
    pub module_count: u64,
    pub modules: *const *const File,
}
#[repr(C)]
pub struct RsdpResponse {
    pub revision: u64,
    pub address: u64,
}
#[repr(C)]
pub struct HhdmResponse {
    pub revision: u64,
    pub offset: u64,
}

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".limine_reqs_a_start")]
pub static mut requests_start_marker: [u64; 4] = [
    0xf6b8f4b39de7d1ae,
    0xfab91a6940fcb9cf,
    0x785c6ed015d3e316,
    0x181e920a7852b9d9,
];
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".limine_reqs_z_end")]
pub static mut requests_end_marker: [u64; 2] = [0xadc0e0531bb10d03, 0x9572709f31764c62];
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".limine_reqs_m_data")]
pub static mut framebuffer_request: Request<FramebufferResponse> =
    Request::new(0x9d5827dcd881dd75, 0xa3148604f6fab11b);
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".limine_reqs_m_data")]
pub static mut memory_map_request: Request<MemoryMapResponse> =
    Request::new(0x67cf3d9d378a806f, 0xe304acdfc50c3c62);
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".limine_reqs_m_data")]
pub static mut module_request: Request<ModuleResponse> =
    Request::new(0x3e7e279702be32af, 0xca1c4f3bd1280cee);
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".limine_reqs_m_data")]
pub static mut executable_file_request: Request<ExecutableFileResponse> =
    Request::new(0xad97e90e83f1ed67, 0x31eb5d1c5ff23b69);
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".limine_reqs_m_data")]
pub static mut rsdp_request: Request<RsdpResponse> =
    Request::new(0xc5e77b6b397e7b43, 0x27637845accdcf3c);
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".limine_reqs_m_data")]
pub static mut hhdm_request: Request<HhdmResponse> =
    Request::new(0x48dcf1cb8ad2b852, 0x63984e959a98244b);

const _: () = {
    assert!(core::mem::size_of::<File>() == 112);
    assert!(core::mem::offset_of!(File, gpt_disk_uuid) == 64);
    assert!(core::mem::offset_of!(File, gpt_part_uuid) == 80);
    assert!(core::mem::size_of::<Request<HhdmResponse>>() == 48);
    assert!(core::mem::offset_of!(Request<HhdmResponse>, response) == 40);
};

/// # Safety
/// Limine has populated resident response storage before entering kmain.
pub unsafe fn keep_requests() {
    // SAFETY: only read references; no Rust reference aliases bootloader writes.
    unsafe {
        let _ = read_volatile(addr_of!(requests_start_marker).cast::<u64>());
        let _ = read_volatile(addr_of!(framebuffer_request).cast::<u64>());
        let _ = read_volatile(addr_of!(memory_map_request).cast::<u64>());
        let _ = read_volatile(addr_of!(module_request).cast::<u64>());
        let _ = read_volatile(addr_of!(executable_file_request).cast::<u64>());
        let _ = read_volatile(addr_of!(rsdp_request).cast::<u64>());
        let _ = read_volatile(addr_of!(hhdm_request).cast::<u64>());
        let _ = read_volatile(addr_of!(requests_end_marker).cast::<u64>());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_limine_keep() {
    // SAFETY: called only after the bootloader handoff.
    unsafe { keep_requests() }
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_limine_response(kind: u32) -> *const core::ffi::c_void {
    // SAFETY: each exact request is resident static storage owned by this module;
    // Limine response publication is read volatile without creating Rust references.
    unsafe {
        match kind {
            0 => read_volatile(addr_of_mut!(framebuffer_request.response)).cast(),
            1 => read_volatile(addr_of_mut!(memory_map_request.response)).cast(),
            2 => read_volatile(addr_of_mut!(module_request.response)).cast(),
            3 => read_volatile(addr_of_mut!(executable_file_request.response)).cast(),
            4 => read_volatile(addr_of_mut!(rsdp_request.response)).cast(),
            5 => read_volatile(addr_of_mut!(hhdm_request.response)).cast(),
            _ => core::ptr::null(),
        }
    }
}
