//! Original bounded physical page chain; a segment never crosses one page.
pub const PAGE_BYTES: u32 = 4096;
pub const MAX_SEGMENTS: usize = 16;
pub const MAX_TRANSFER_BYTES: u32 = PAGE_BYTES * MAX_SEGMENTS as u32;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Segment {
    pub phys: u64,
    pub len: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Plan {
    pub segments: [Segment; MAX_SEGMENTS],
    pub count: u8,
    pub bytes: u32,
}
pub fn build(phys: u64, bytes: u32) -> Option<Plan> {
    if phys == 0 || bytes == 0 || bytes > MAX_TRANSFER_BYTES {
        return None;
    }
    let mut out = Plan {
        segments: [Segment::default(); MAX_SEGMENTS],
        count: 0,
        bytes,
    };
    let (mut cursor, mut remaining) = (phys, bytes);
    while remaining != 0 {
        if usize::from(out.count) >= MAX_SEGMENTS {
            return None;
        }
        let offset = (cursor & u64::from(PAGE_BYTES - 1)) as u32;
        let length = remaining.min(PAGE_BYTES - offset);
        out.segments[usize::from(out.count)] = Segment {
            phys: cursor,
            len: length,
        };
        out.count += 1;
        cursor += u64::from(length);
        remaining -= length;
    }
    Some(out)
}
/// # Safety
/// Exclusive fixed-layout output; false leaves it untouched as the original
/// optional builder did. No memory at the physical addresses is dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_usb_trb_chain(phys: u64, bytes: u32, out: *mut Plan) -> bool {
    if let Some(plan) = build(phys, bytes) {
        unsafe { out.write(plan) };
        true
    } else {
        false
    }
}
const _: () = {
    assert!(core::mem::size_of::<Segment>() == 16);
    assert!(core::mem::align_of::<Segment>() == 8);
    assert!(core::mem::offset_of!(Segment, phys) == 0);
    assert!(core::mem::offset_of!(Segment, len) == 8);
    assert!(core::mem::size_of::<Plan>() == 264);
    assert!(core::mem::align_of::<Plan>() == 8);
    assert!(core::mem::offset_of!(Plan, segments) == 0);
    assert!(core::mem::offset_of!(Plan, count) == 256);
    assert!(core::mem::offset_of!(Plan, bytes) == 260);
};
