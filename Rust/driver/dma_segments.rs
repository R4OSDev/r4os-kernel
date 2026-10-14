//! Existing DMA descriptor geometry and generation handles. This policy grants
//! no mapping, pin, device admission or release; those exact owners remain.
use core::ptr;
const PAGE: u64 = 4096;
const MAX: usize = 64;
const GENERATION_MASK: u64 = 0x00ff_ffff_ffff_ffff;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Segment {
    pub phys_addr: u64,
    pub bytes: u32,
    pub reserved: u32,
}
#[repr(C)]
pub struct SegmentList {
    pub count: u16,
    pub segments: [Segment; MAX],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Constraints {
    pub dma_mask: u64,
    pub boundary: u64,
    pub max_segment_bytes: u32,
    pub alignment: u32,
    pub max_segment_count: u16,
}
const _: () = {
    assert!(core::mem::size_of::<Segment>() == 16);
    assert!(core::mem::size_of::<SegmentList>() == 1032);
    assert!(core::mem::offset_of!(SegmentList, segments) == 8);
    assert!(core::mem::size_of::<Constraints>() == 32);
    assert!(core::mem::offset_of!(Constraints, max_segment_count) == 24);
};
#[repr(u8)]
enum Error {
    Invalid = 1,
    AddressLimit = 2,
    Alignment = 3,
    TooMany = 4,
    Overflow = 5,
}
fn validate(c: Constraints) -> Result<(), Error> {
    if c.max_segment_count == 0
        || c.max_segment_count as usize > MAX
        || (c.boundary != 0 && !c.boundary.is_power_of_two())
        || !c.alignment.is_power_of_two()
        || (c.boundary != 0 && c.boundary < u64::from(c.alignment))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn append(
    list: &mut SegmentList,
    start: u64,
    byte_count: u64,
    c: Constraints,
) -> Result<(), Error> {
    if byte_count == 0 {
        return Ok(());
    }
    let last = start.checked_add(byte_count - 1).ok_or(Error::Overflow)?;
    if last > c.dma_mask {
        return Err(Error::AddressLimit);
    }
    let max_bytes = if c.max_segment_bytes == 0 {
        u64::from(u32::MAX)
    } else {
        u64::from(c.max_segment_bytes)
    };
    let mut current = start;
    let mut remaining = byte_count;
    while remaining != 0 {
        let mut take = remaining.min(max_bytes);
        if c.boundary != 0 {
            take = take.min(c.boundary - (current & (c.boundary - 1)));
        }
        if take < remaining && c.alignment > 1 {
            take &= !(u64::from(c.alignment) - 1);
        }
        if take == 0 || take > u64::from(u32::MAX) {
            return Err(Error::Overflow);
        }
        if list.count != 0 {
            let previous = &mut list.segments[list.count as usize - 1];
            let end = previous
                .phys_addr
                .checked_add(u64::from(previous.bytes))
                .ok_or(Error::Overflow)?;
            let merged = u64::from(previous.bytes) + take;
            let same_boundary =
                c.boundary == 0 || previous.phys_addr / c.boundary == current / c.boundary;
            if end == current && merged <= max_bytes && same_boundary {
                previous.bytes = u32::try_from(merged).expect("Validated DMA merge exceeds u32");
                current += take;
                remaining -= take;
                continue;
            }
        }
        if current & (u64::from(c.alignment) - 1) != 0 {
            return Err(Error::Alignment);
        }
        if list.count >= c.max_segment_count {
            return Err(Error::TooMany);
        }
        list.segments[list.count as usize] = Segment {
            phys_addr: current,
            bytes: u32::try_from(take).expect("Validated DMA segment exceeds u32"),
            reserved: 0,
        };
        list.count += 1;
        current += take;
        remaining -= take;
    }
    Ok(())
}
unsafe fn initialize(output: *mut SegmentList) {
    // Resident caller storage is initialized directly, avoiding another full
    // 1-KB list value/copy on the shared bounded kernel stack.
    unsafe {
        ptr::addr_of_mut!((*output).count).write(0);
        ptr::addr_of_mut!((*output).segments)
            .cast::<Segment>()
            .write_bytes(0, MAX);
    }
}
fn status(result: Result<(), Error>) -> u8 {
    match result {
        Ok(()) => 0,
        Err(e) => e as u8,
    }
}

/// # Safety
/// Frames are aligned readable storage of `len` u64s. Constraints and exclusive
/// output have exact C layouts and disjoint lifetimes. Output is consumed only
/// on success; no pointer escapes and no callback/wait occurs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dma_from_frames(
    frames: *const u64,
    len: usize,
    first_offset: u32,
    bytes: u32,
    constraints: *const Constraints,
    output: *mut SegmentList,
) -> u8 {
    let c = unsafe { constraints.read() };
    status((|| {
        validate(c)?;
        if bytes == 0 || u64::from(first_offset) >= PAGE {
            return Err(Error::Invalid);
        }
        let covered = u64::from(first_offset)
            .checked_add(u64::from(bytes))
            .ok_or(Error::Overflow)?;
        if covered.div_ceil(PAGE) != len as u64 {
            return Err(Error::Invalid);
        }
        unsafe { initialize(output) };
        let result = unsafe { &mut *output };
        let mut remaining = u64::from(bytes);
        for index in 0..len {
            let frame = unsafe { frames.add(index).read() };
            if frame & (PAGE - 1) != 0 {
                return Err(Error::Invalid);
            }
            let offset = if index == 0 {
                u64::from(first_offset)
            } else {
                0
            };
            let take = remaining.min(PAGE - offset);
            append(result, frame + offset, take, c)?;
            remaining -= take;
        }
        if remaining != 0 {
            return Err(Error::Invalid);
        }
        Ok(())
    })())
}

/// # Safety
/// Existing source and constraints are readable exact C records; exclusive
/// output is disjoint. Only successful output is published to the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dma_constrain(
    source: *const SegmentList,
    constraints: *const Constraints,
    output: *mut SegmentList,
) -> u8 {
    let c = unsafe { constraints.read() };
    status((|| {
        validate(c)?;
        let source = unsafe { &*source };
        if source.count == 0 || source.count as usize > MAX {
            return Err(Error::Invalid);
        }
        unsafe { initialize(output) };
        let result = unsafe { &mut *output };
        for segment in &source.segments[..source.count as usize] {
            if segment.bytes == 0 {
                return Err(Error::Invalid);
            }
            append(result, segment.phys_addr, u64::from(segment.bytes), c)?;
        }
        Ok(())
    })())
}

/// # Safety
/// Constraints are readable and output is exclusive exact C storage. `append`
/// requires an initialized valid list and retains the original partial append
/// error semantics. New-list output is consumed only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dma_range(
    output: *mut SegmentList,
    start: u64,
    bytes: u32,
    constraints: *const Constraints,
    append_existing: bool,
) -> u8 {
    let c = unsafe { constraints.read() };
    status((|| {
        validate(c)?;
        if bytes == 0 {
            return Err(Error::Invalid);
        }
        if !append_existing {
            unsafe { initialize(output) };
        }
        append(unsafe { &mut *output }, start, u64::from(bytes), c)
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dma_handle(operation: u8, a: u64, b: u64) -> u64 {
    match operation {
        0 => {
            let next = a.wrapping_add(1) & GENERATION_MASK;
            if next == 0 { 1 } else { next }
        }
        1 => {
            if a >= 255 || b == 0 || b > GENERATION_MASK {
                0
            } else {
                (b << 8) | (a + 1)
            }
        }
        2 => {
            let encoded = a & 255;
            if encoded == 0 || encoded > b {
                u64::MAX
            } else {
                encoded - 1
            }
        }
        3 => a >> 8,
        _ => panic!("Invalid DMA handle operation"),
    }
}
