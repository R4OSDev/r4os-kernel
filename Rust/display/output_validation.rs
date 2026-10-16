//! Original copied output metadata/prefix checks. No EDID policy, driver call,
//! hardware probing or resource lifetime is introduced by validation.
use super::error::Error;
use crate::contract as a;
use core::{mem::size_of, ptr};
pub fn unknown_limits() -> a::GfxDisplayLimits {
    // SAFETY: this unchanged Contract record consists solely of integer fields.
    let mut value = unsafe { core::mem::zeroed::<a::GfxDisplayLimits>() };
    value.version = 1;
    value.size = size_of::<a::GfxDisplayLimits>() as u32;
    value
}
pub fn valid_color(value: &a::GfxOutputColorState) -> bool {
    if value.version != 1
        || (value.size as usize) < size_of::<a::GfxOutputColorState>()
        || value.reserved0 != 0
        || !valid_color_link(value)
        || value.flags & !127 != 0
        || value.flags & a::gfx_output_color_known == 0
        || value.formats & !3 != 0
        || value.depths & !3 != 0
        || value.color_spaces & !3 != 0
        || value.transfers & !13 != 0
        || value.ranges & !3 != 0
        || value.gamma_entries > 65536
        || value.degamma_entries > 65536
        || value.ctm_fraction_bits > 32
    {
        return false;
    }
    if value.flags & a::gfx_output_color_active == 0 {
        return value.format == 0
            && value.bpc == 0
            && value.primaries == 0
            && value.transfer == 0
            && value.range == 0
            && value.reference_white == 0
            && value.peak == 0
            && value.black == 0;
    }
    let format = match value.format {
        a::gfx_buffer_format_xrgb8888 => 1,
        a::gfx_buffer_format_xrgb2101010 => 2,
        _ => return false,
    };
    let depth = match value.bpc {
        8 => 1,
        10 => 2,
        _ => return false,
    };
    let primaries = match value.primaries {
        1 => 1,
        3 => 2,
        _ => return false,
    };
    let transfer = match value.transfer {
        1 => 1,
        3 => 4,
        4 => 8,
        _ => return false,
    };
    let range = match value.range {
        1 => 1,
        2 => 2,
        _ => return false,
    };
    value.formats & format != 0
        && value.depths & depth != 0
        && value.color_spaces & primaries != 0
        && value.transfers & transfer != 0
        && value.ranges & range != 0
        && value.reference_white != 0
        && value.peak != 0
}
fn valid_color_link(value: &a::GfxOutputColorState) -> bool {
    if value.reserved_link != 0
        || value.link_kind > a::gfx_output_link_dp_mst
        || value.link_flags & !3 != 0
        || value.dsc_depths & !3 != 0
        || value.max_frl_rate > 6
        || value.compressed_bpp_x16 > 1023
        || value.link_lanes > 4
    {
        return false;
    }
    if value.link_kind == 0 || value.flags & a::gfx_output_color_active == 0 {
        return value.link_kind == 0
            && value.link_flags == 0
            && value.compressed_bpp_x16 == 0
            && value.link_lanes == 0
            && value.link_rate_mbps == 0
            && value.h_active == 0
            && value.h_total == 0
            && value.v_active == 0
            && value.link_payload_bits_per_second == 0
            && value.pixel_clock_numerator == 0
            && value.pixel_clock_denominator == 0;
    }
    (value.compressed_bpp_x16 != 0) == (value.link_flags & a::gfx_output_link_dsc != 0)
        && value.h_active != 0
        && value.h_total > value.h_active
        && value.v_active != 0
        && value.pixel_clock_numerator != 0
        && value.pixel_clock_denominator != 0
}
pub fn valid_receiver(value: &a::GfxReceiverInfo) -> Result<(), Error> {
    let flags = a::gfx_output_flag_connected
        | a::gfx_output_flag_connection_unknown
        | a::gfx_output_flag_edid_missing
        | a::gfx_output_flag_edid_invalid
        | a::gfx_output_flag_receiver_incomplete
        | a::gfx_output_flag_query_failed;
    if value.connector_id == 0
        || value.reserved0 != 0
        || value.flags & !flags != 0
        || (value.connector_kind > a::gfx_output_kind_virtual
            && value.connector_kind != a::gfx_output_kind_dvi)
        || value.mode_count > a::gfx_output_max_modes
        || value.edid_bytes > a::gfx_output_max_edid_bytes
        || !value.edid_bytes.is_multiple_of(128)
    {
        return Err(Error::Invalid);
    }
    let connected = value.flags & a::gfx_output_flag_connected != 0;
    if connected && value.flags & a::gfx_output_flag_connection_unknown != 0 {
        return Err(Error::Invalid);
    }
    if !connected
        && (value.mode_count != 0
            || value.edid_bytes != 0
            || value.preferred_mode_id != 0
            || value.flags & (a::gfx_output_flag_edid_missing | a::gfx_output_flag_edid_invalid)
                != 0)
    {
        return Err(Error::Invalid);
    }
    if value.flags & a::gfx_output_flag_edid_missing != 0 && value.edid_bytes != 0 {
        return Err(Error::Invalid);
    }
    if value.flags & a::gfx_output_flag_edid_invalid != 0 && value.mode_count != 0 {
        return Err(Error::Invalid);
    }
    let mut preferred = value.preferred_mode_id == 0;
    for (index, mode) in value.modes[..value.mode_count as usize].iter().enumerate() {
        if !valid_mode(*mode) || mode.flags & a::gfx_output_mode_geometry_only != 0 {
            return Err(Error::Invalid);
        }
        if value.modes[..index]
            .iter()
            .any(|prior| prior.mode_id == mode.mode_id)
        {
            return Err(Error::Invalid);
        }
        if mode.mode_id == value.preferred_mode_id {
            preferred = true
        }
    }
    if preferred {
        Ok(())
    } else {
        Err(Error::Invalid)
    }
}
pub fn bit(index: u32) -> Result<u32, Error> {
    if index >= 32 {
        Err(Error::Routing)
    } else {
        Ok(1u32 << index)
    }
}
pub fn fits(total: u32, offset: u32, size: u32) -> bool {
    size != 0 && offset < total && size <= total - offset
}
fn original_power_of_two(value: u32) -> bool {
    assert!(value > 0, "original output pitch precondition");
    value.is_power_of_two()
}
pub fn valid_limits(value: a::GfxDisplayLimits) -> Result<(), Error> {
    if value.head_mask == 0
        || value.plane_mask == 0
        || value.pll_mask == 0
        || value.rotations == 0
        || value.rotations & !15 != 0
        || value.colors == 0
        || value.colors & !15 != 0
        || value.bpc_mask & 0x1_5555 == 0
        || value.bpc_mask & !0x1_5540 != 0
        || value.flags & !7 != 0
        || !original_power_of_two(value.pitch_alignment)
        || value.pitch_alignment > 65536
        || value.max_width == 0
        || value.max_height == 0
        || value.max_width > 65536
        || value.max_height > 65536
        || value.formats == 0
        || value.formats & !15 != 0
        || value.modifiers != 1
    {
        return Err(Error::Invalid);
    }
    if value.flags & a::gfx_output_limit_modeset != 0
        && (value.bandwidth_bytes_per_second == 0
            || value.max_pixel_clock_hz == 0
            || value.total_pixel_clock_hz == 0)
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
pub fn valid_mode(mode: a::GfxOutputMode) -> bool {
    if mode.version != 1
        || (mode.size as usize) < size_of::<a::GfxOutputMode>()
        || mode.reserved0 != 0
        || mode.mode_id == 0
        || mode.width == 0
        || mode.height == 0
        || mode.width > 65536
        || mode.height > 65536
        || mode.flags & !127 != 0
    {
        return false;
    }
    if mode.flags & a::gfx_output_mode_geometry_only != 0 {
        return mode.pixel_clock_hz == 0
            && mode.refresh_millihz == 0
            && mode.h_total == 0
            && mode.v_total == 0
            && mode.h_sync_start == 0
            && mode.h_sync_end == 0
            && mode.v_sync_start == 0
            && mode.v_sync_end == 0;
    }
    if mode.pixel_clock_hz == 0
        || mode.pixel_clock_hz > 20_000_000_000
        || mode.h_total > 131072
        || mode.v_total > 131072
        || mode.width > mode.h_sync_start
        || mode.h_sync_start >= mode.h_sync_end
        || mode.h_sync_end > mode.h_total
        || mode.height > mode.v_sync_start
        || mode.v_sync_start >= mode.v_sync_end
        || mode.v_sync_end > mode.v_total
    {
        return false;
    }
    let expected = mode.pixel_clock_hz * 1000 * if mode.flags & 1 != 0 { 2 } else { 1 }
        / (u64::from(mode.h_total) * u64::from(mode.v_total));
    expected > 0 && expected == u64::from(mode.refresh_millihz)
}
/// # Safety
/// Output holds an initialized8-byte header and its advertised writable prefix.
/// Functions is a fully initialized exact table; no full output reference is formed.
pub unsafe fn driver_table(
    output: *mut a::GfxDriverOutputApi,
    functions: a::GfxDriverOutputApi,
) -> i32 {
    if output.is_null() {
        return a::gfx_output_error_invalid;
    }
    let version = unsafe { ptr::addr_of!((*output).version).read() };
    let size = unsafe { ptr::addr_of!((*output).size).read() };
    if version != 1 || size < 24 {
        return a::gfx_output_error_invalid;
    }
    let mut value = functions;
    value.size = (size & !7).min(size_of::<a::GfxDriverOutputApi>() as u32);
    unsafe {
        ptr::copy_nonoverlapping(
            (&value as *const a::GfxDriverOutputApi).cast::<u8>(),
            output.cast::<u8>(),
            value.size as usize,
        )
    };
    a::gfx_output_ok
}

/// # Safety
/// Exact initialized Contract input. Table output admits only its initialized header/prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_output_valid_color(
    value: *const a::GfxOutputColorState,
) -> bool {
    valid_color(unsafe { &*value })
}

/// # Safety
/// Exact initialized Contract input. Table output admits only its initialized header/prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_output_valid_mode(
    value: *const a::GfxOutputMode,
) -> bool {
    valid_mode(unsafe { value.read() })
}

/// # Safety
/// Exact initialized Contract input. Table output admits only its initialized header/prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_output_valid_limits(
    value: *const a::GfxDisplayLimits,
) -> i32 {
    match valid_limits(unsafe { value.read() }) {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized Contract input. Table output admits only its initialized header/prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_output_valid_receiver(
    value: *const a::GfxReceiverInfo,
) -> i32 {
    match valid_receiver(unsafe { &*value }) {
        Ok(()) => 0,
        Err(error) => error as i32,
    }
}

/// # Safety
/// Exact initialized Contract input. Table output admits only its initialized header/prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_display_output_driver_table(
    output: *mut a::GfxDriverOutputApi,
    functions: *const a::GfxDriverOutputApi,
) -> i32 {
    unsafe { driver_table(output, functions.read()) }
}
