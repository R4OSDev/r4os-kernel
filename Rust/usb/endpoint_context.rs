//! Original asynchronous bulk context fields: no invented periodic ESIT payload.
pub const BULK_AVERAGE_TRB_LENGTH: u16 = 0;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BulkFields {
    pub dword0: u32,
    pub dword1: u32,
    pub dword4: u32,
    pub max_packet: u16,
    pub max_burst: u8,
}
pub fn bulk_fields(
    speed: u8,
    input: bool,
    descriptor_packet: u16,
    descriptor_burst: u8,
) -> BulkFields {
    let superspeed = speed >= 4;
    let masked = descriptor_packet & 0x07ff;
    let packet = if masked == 0 {
        if superspeed { 1024 } else { 512 }
    } else {
        masked
    };
    let burst = if superspeed {
        descriptor_burst.min(15)
    } else {
        0
    };
    let kind = if input { 6u32 } else { 2u32 };
    BulkFields {
        dword0: 0,
        dword1: (3 << 1) | (kind << 3) | (u32::from(burst) << 8) | (u32::from(packet) << 16),
        dword4: u32::from(BULK_AVERAGE_TRB_LENGTH),
        max_packet: packet,
        max_burst: burst,
    }
}
/// # Safety
/// Exclusive fixed-layout output; original bounded descriptor scalars.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_bulk_fields(
    speed: u8,
    input: bool,
    packet: u16,
    burst: u8,
    out: *mut BulkFields,
) {
    unsafe { out.write(bulk_fields(speed, input, packet, burst)) };
}
const _: () = {
    assert!(core::mem::size_of::<BulkFields>() == 16);
    assert!(core::mem::align_of::<BulkFields>() == 4);
    assert!(core::mem::offset_of!(BulkFields, dword0) == 0);
    assert!(core::mem::offset_of!(BulkFields, dword1) == 4);
    assert!(core::mem::offset_of!(BulkFields, dword4) == 8);
    assert!(core::mem::offset_of!(BulkFields, max_packet) == 12);
    assert!(core::mem::offset_of!(BulkFields, max_burst) == 14);
};
