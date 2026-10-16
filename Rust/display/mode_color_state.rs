//! Copied color-mode admission. Link policy, EDID and transfer execution remain
//! outside the kernel; this model retains the existing integer/layout contract.
use super::{equal::same, error::Error};
use crate::{contract as a, gfx::layout};
use core::mem::size_of;
pub fn validate(
    request: &a::GfxModeColorRequest,
    source: a::GfxOutputColorState,
    image: layout::Descriptor,
) -> Result<(), Error> {
    if request.version != 1
        || request.size as usize != size_of::<a::GfxModeColorRequest>()
        || request.state.count != 1
        || request.image.id == 0
        || request.image.generation == 0
        || request.image.reserved0 != 0
    {
        return Err(Error::Invalid);
    }
    let assignment = request.state.assignments[0];
    if !same(&source.identity, &assignment.output)
        || source.revision != request.state.topology_revision
    {
        return Err(Error::Stale);
    }
    let signal = request.signal;
    if signal.version != 1
        || signal.size as usize != size_of::<a::GfxColorSignal>()
        || signal.reserved0 != 0
        || signal.metadata_valid > 1
        || signal.reference_white == 0
        || signal.peak < signal.reference_white
        || signal.peak > 100_000_000
        || signal.black >= signal.reference_white
    {
        return Err(Error::Invalid);
    }
    if source.version != 1
        || (source.size as usize) < size_of::<a::GfxOutputColorState>()
        || source.flags & 7 != 7
        || signal.pipeline != 7
    {
        return Err(Error::Unsupported);
    }
    let format = match signal.format {
        a::gfx_buffer_format_xrgb8888 => 1,
        a::gfx_buffer_format_xrgb2101010 => 2,
        _ => return Err(Error::Unsupported),
    };
    let depth = match signal.bpc {
        8 => 1,
        10 => 2,
        _ => return Err(Error::Unsupported),
    };
    let primaries = match signal.primaries {
        1 => 1,
        3 => 2,
        _ => return Err(Error::Unsupported),
    };
    let transfer = match signal.transfer {
        1 => 1,
        3 => 4,
        4 => 8,
        _ => return Err(Error::Unsupported),
    };
    let range = match signal.range {
        1 => 1,
        2 => 2,
        _ => return Err(Error::Unsupported),
    };
    if format != depth
        || source.formats & format == 0
        || source.depths & depth == 0
        || source.color_spaces & primaries == 0
        || source.transfers & transfer == 0
        || source.ranges & range == 0
    {
        return Err(Error::Unsupported);
    }
    if transfer == 1 {
        if signal.metadata_valid != 0 || !same(&signal.metadata, &a::GfxHdrMetadata::default()) {
            return Err(Error::Invalid);
        }
    } else if format != 2
        || primaries != 2
        || signal.metadata_valid != 1
        || source.flags & (a::gfx_output_color_hdmi_metadata | a::gfx_output_color_dp_metadata) == 0
    {
        return Err(Error::Unsupported);
    }
    if image.format != signal.format
        || image.location != 0
        || !image.binding.portable()
        || image.modifier != 0
        || image.plane_count != 1
        || image.planes[0].offset != 0
        || image.width != assignment.source_width
        || image.height != assignment.source_height
        || image.planes[0].pitch != u64::from(image.width) * 4
        || u128::from(image.bytes) != u128::from(image.planes[0].pitch) * u128::from(image.height)
        || image.usage & 6 != 6
    {
        return Err(Error::Unsupported);
    }
    Ok(())
}
/// # Safety
/// Exact initialized Contract inputs and native private BO descriptor remain
/// readable through this copied validation; no pixel or device access occurs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_mode_color_validate(
    request: *const a::GfxModeColorRequest,
    source: *const a::GfxOutputColorState,
    image: *const layout::Descriptor,
) -> i32 {
    match validate(unsafe { &*request }, unsafe { source.read() }, unsafe {
        image.read()
    }) {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}
