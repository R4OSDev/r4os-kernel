//! Original bounded plane/backing validation. CPU, DMA and virtual-address
//! leases remain distinct; an opaque native modifier grants no hardware policy.
pub const BYTES: u32 = 0;
pub const XRGB8888: u32 = 0x34325258;
pub const ARGB8888: u32 = 0x34325241;
pub const XRGB2101010: u32 = 0x30335258;
pub const ARGB2101010: u32 = 0x30335241;
pub const ABGR16161616F: u32 = 0x48344241;
pub const R8: u32 = 0x20203852;
pub const NV12: u32 = 0x3231564e;
pub const P010: u32 = 0x30313050;
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Binding {
    pub adapter: u32,
    pub driver_owner: u32,
    pub device_generation: u64,
}
impl Binding {
    pub fn portable(self) -> bool {
        self.adapter == 0 && self.driver_owner == 0 && self.device_generation == 0
    }
    pub fn valid(self) -> bool {
        self.portable()
            || (self.adapter != 0 && self.driver_owner != 0 && self.device_generation != 0)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Plane {
    pub offset: u64,
    pub pitch: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Descriptor {
    pub bytes: u64,
    pub alignment: u64,
    pub modifier: u64,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub plane_count: u32,
    pub planes: [Plane; 4],
    pub usage: u32,
    pub location: u32,
    pub binding: Binding,
}
impl Descriptor {
    pub const EMPTY: Self = Self {
        bytes: 0,
        alignment: 4096,
        modifier: 0,
        width: 0,
        height: 0,
        format: BYTES,
        plane_count: 0,
        planes: [Plane {
            offset: 0,
            pitch: 0,
        }; 4],
        usage: 3,
        location: 0,
        binding: Binding {
            adapter: 0,
            driver_owner: 0,
            device_generation: 0,
        },
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Range {
    pub offset: u64,
    pub bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Layout {
    pub allocation_bytes: u64,
    pub planes: [Range; 4],
    pub plane_count: u32,
}
#[repr(i32)]
#[derive(Clone, Copy, Debug)]
pub enum Error {
    Invalid = 1,
    Overflow = 2,
    Unsupported = 3,
}
pub fn span_fits(total: u64, offset: u64, bytes: u64) -> bool {
    bytes != 0 && offset < total && bytes <= total - offset
}
pub fn validate(d: Descriptor, owned: bool) -> Result<Layout, Error> {
    if owned && (d.location != 1 || d.binding.portable() || d.usage & 3 != 0) {
        return Err(Error::Unsupported);
    }
    if d.bytes == 0
        || !d.alignment.is_power_of_two()
        || d.alignment > (1 << 30)
        || d.usage == 0
        || d.usage & !63 != 0
        || !d.binding.valid()
    {
        return Err(Error::Invalid);
    }
    let native = d.modifier != 0;
    if native && !owned {
        return Err(Error::Unsupported);
    }
    if d.location == 1 && d.binding.portable() {
        return Err(Error::Invalid);
    }
    let align = d.alignment.max(4096);
    let padded = d.bytes.checked_add(align - 1).ok_or(Error::Overflow)?;
    let mut out = Layout {
        allocation_bytes: padded & !(align - 1),
        plane_count: d.plane_count,
        ..Layout::default()
    };
    if d.format == BYTES {
        if native || d.width != 0 || d.height != 0 || d.plane_count != 0 || d.usage & 48 != 0 {
            return Err(Error::Invalid);
        }
    } else {
        let multi = matches!(d.format, NV12 | P010);
        let count = if multi { 2 } else { 1 };
        if d.width == 0 || d.height == 0 || d.plane_count != count {
            return Err(Error::Invalid);
        }
        for index in 0..count as usize {
            let plane = d.planes[index];
            let sample = match d.format {
                XRGB8888 | ARGB8888 | XRGB2101010 | ARGB2101010 => 4,
                ABGR16161616F => 8,
                P010 => 2,
                NV12 | R8 => 1,
                _ => return Err(Error::Unsupported),
            };
            let columns = if multi && index == 1 {
                u64::from(d.width).div_ceil(2) * 2
            } else {
                u64::from(d.width)
            };
            let rows = if multi && index == 1 {
                u64::from(d.height).div_ceil(2)
            } else {
                u64::from(d.height)
            };
            let row_bytes = columns.checked_mul(sample).ok_or(Error::Overflow)?;
            if plane.pitch < row_bytes
                || !plane.pitch.is_multiple_of(sample)
                || !plane.offset.is_multiple_of(sample)
            {
                return Err(Error::Invalid);
            }
            let mut bytes = plane.pitch.checked_mul(rows).ok_or(Error::Overflow)?;
            if !span_fits(d.bytes, plane.offset, bytes) {
                return Err(Error::Invalid);
            }
            if native {
                let end = if index + 1 < count as usize {
                    d.planes[index + 1].offset
                } else {
                    d.bytes
                };
                if end > d.bytes || end <= plane.offset || bytes > end - plane.offset {
                    return Err(Error::Invalid);
                }
                bytes = end - plane.offset;
            }
            for previous in &out.planes[..index] {
                if plane.offset < previous.offset + previous.bytes
                    && previous.offset < plane.offset + bytes
                {
                    return Err(Error::Invalid);
                }
            }
            out.planes[index] = Range {
                offset: plane.offset,
                bytes,
            };
        }
    }
    for p in &d.planes[d.plane_count as usize..] {
        if p.offset != 0 || p.pitch != 0 {
            return Err(Error::Invalid);
        }
    }
    Ok(out)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gfx_layout_span(total: u64, offset: u64, bytes: u64) -> bool {
    span_fits(total, offset, bytes)
}
/// # Safety
/// Caller retains one initialized binding for this synchronous copied query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_layout_binding(p: *const Binding, portable: bool) -> bool {
    let value = unsafe { p.read() };
    if portable {
        value.portable()
    } else {
        value.valid()
    }
}
/// # Safety
/// Descriptor contains the original valid format/location enum values. Caller
/// retains readable descriptor and owns aligned exclusive layout output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_layout_validate(
    p: *const Descriptor,
    owned: bool,
    out: *mut Layout,
) -> i32 {
    match validate(unsafe { p.read() }, owned) {
        Ok(value) => {
            unsafe { out.write(value) }
            0
        }
        Err(e) => e as i32,
    }
}

const _: () = {
    assert!(core::mem::size_of::<Binding>() == 16);
    assert!(core::mem::align_of::<Binding>() == 8);
    assert!(core::mem::offset_of!(Binding, adapter) == 0);
    assert!(core::mem::offset_of!(Binding, driver_owner) == 4);
    assert!(core::mem::offset_of!(Binding, device_generation) == 8);
    assert!(core::mem::size_of::<Plane>() == 16);
    assert!(core::mem::align_of::<Plane>() == 8);
    assert!(core::mem::offset_of!(Plane, offset) == 0);
    assert!(core::mem::offset_of!(Plane, pitch) == 8);
    assert!(core::mem::size_of::<Descriptor>() == 128);
    assert!(core::mem::align_of::<Descriptor>() == 8);
    assert!(core::mem::offset_of!(Descriptor, bytes) == 0);
    assert!(core::mem::offset_of!(Descriptor, alignment) == 8);
    assert!(core::mem::offset_of!(Descriptor, modifier) == 16);
    assert!(core::mem::offset_of!(Descriptor, width) == 24);
    assert!(core::mem::offset_of!(Descriptor, height) == 28);
    assert!(core::mem::offset_of!(Descriptor, format) == 32);
    assert!(core::mem::offset_of!(Descriptor, plane_count) == 36);
    assert!(core::mem::offset_of!(Descriptor, planes) == 40);
    assert!(core::mem::offset_of!(Descriptor, usage) == 104);
    assert!(core::mem::offset_of!(Descriptor, location) == 108);
    assert!(core::mem::offset_of!(Descriptor, binding) == 112);
    assert!(core::mem::size_of::<Range>() == 16);
    assert!(core::mem::align_of::<Range>() == 8);
    assert!(core::mem::offset_of!(Range, offset) == 0);
    assert!(core::mem::offset_of!(Range, bytes) == 8);
    assert!(core::mem::size_of::<Layout>() == 80);
    assert!(core::mem::align_of::<Layout>() == 8);
    assert!(core::mem::offset_of!(Layout, allocation_bytes) == 0);
    assert!(core::mem::offset_of!(Layout, planes) == 8);
    assert!(core::mem::offset_of!(Layout, plane_count) == 72);
};
