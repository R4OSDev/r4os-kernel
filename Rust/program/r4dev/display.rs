//! Original passive display state and loaded-driver identity projections.
use crate::{contract as c, display::manager as display, loader::r4d};
use core::mem;
/// # Safety
/// Exclusive aligned canonical output for a passive display snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_display_summary(out: *mut c::DisplaySummary) -> i32 {
    unsafe {
        let (s, name) = display::diagnostic_stats();
        let mut flags = u32::from(s.registered) * c::display_summary_flag_registered;
        if s.flags & display::VISIBLE != 0 {
            flags |= c::display_summary_flag_visible
        }
        if s.flags & display::FIXED_MODE != 0 {
            flags |= c::display_summary_flag_fixed_mode
        }
        if s.flags & display::CPU_PRESENT != 0 {
            flags |= c::display_summary_flag_cpu_present
        }
        if s.flags & display::XRGB32 != 0 {
            flags |= c::display_summary_flag_xrgb32
        }
        out.write(c::DisplaySummary {
            flags,
            backend_kind: s.kind,
            cache_policy: s.mapping.cache_policy,
            bpp: s.mode.bpp,
            width: s.mode.width,
            height: s.mode.height,
            pitch: s.mode.pitch,
            last_present_reason: s.last_present_reason,
            last_present_converted: u8::from(s.last_present_converted),
            last_present_x: s.last_present_rect.x,
            last_present_y: s.last_present_rect.y,
            last_present_w: s.last_present_rect.w,
            last_present_h: s.last_present_rect.h,
            present_count: s.present_count,
            present_pixels_total: s.present_pixels_total,
            present_bytes_total: s.present_bytes_total,
            last_present_pixels: s.last_present_pixels,
            last_present_bytes: s.last_present_bytes,
            full_present_count: s.full_present_count,
            partial_present_count: s.partial_present_count,
            fill_present_count: s.fill_present_count,
            rect_present_count: s.rect_present_count,
            packed32_present_count: s.packed32_present_count,
            xrgb32_present_count: s.xrgb32_present_count,
            conversion_present_count: s.conversion_present_count,
            present_total_ticks: s.present_total_ticks,
            present_max_ticks: s.present_max_ticks,
            present_last_ticks: s.present_last_ticks,
            present_slow_count: s.present_slow_count,
            backend_name: name,
            ..Default::default()
        });
        i32::from(s.registered)
    }
}
/// # Safety
/// Caller retains a readable version/size header and exclusive canonical
/// result. Invalid or null original headers are rejected before observation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_display_state(out: *mut c::DisplayStateInfo) -> i32 {
    unsafe {
        if out.is_null()
            || (*out).version != 1
            || (*out).size < mem::size_of::<c::DisplayStateInfo>() as u32
        {
            return -1;
        }
        let view = display::backend_view();
        let current = view.state;
        let device = view.device;
        let mut capabilities = 0;
        if view.firmware_writable && view.boot_mode.width != 0 {
            capabilities |= c::display_state_cap_firmware_writable
        }
        if device.registered {
            if device.flags & display::CPU_PRESENT != 0
                && (device.kind != display::BOOTFB || view.firmware_writable)
            {
                capabilities |=
                    c::display_state_cap_cpu_present | c::display_state_cap_software_fallback
            }
            if device.kind == display::NATIVE {
                capabilities |= c::display_state_cap_native_scanout
            }
        }
        let mut value = c::DisplayStateInfo {
            state: current.state,
            policy: current.policy,
            reason: current.reason,
            revision: current.revision,
            device_generation: current.generation,
            reset_generation: current.reset_generation,
            pending_generation: current.pending_generation,
            adapter_id: current.adapter_id,
            pending_adapter_id: current.pending_adapter_id,
            driver_owner: current
                .owner
                .try_into()
                .expect("original driver owner fits u32"),
            pending_driver_owner: current
                .pending_owner
                .try_into()
                .expect("original driver owner fits u32"),
            backend_kind: u32::from(device.kind),
            capabilities,
            width: device.mode.width,
            height: device.mode.height,
            pitch: device.mode.pitch,
            bpp: device.mode.bpp,
            cache_policy: device.mapping.cache_policy,
            mapping_kind: device.mapping.kind,
            boot_width: view.boot_mode.width,
            boot_height: view.boot_mode.height,
            boot_pitch: view.boot_mode.pitch,
            boot_bpp: view.boot_mode.bpp,
            boot_byte_length: view.boot_mapping.byte_len,
            byte_length: device.mapping.byte_len,
            backend_name: view.name,
            ..Default::default()
        };
        if capabilities & c::display_state_cap_software_fallback != 0 {
            let label: &[u8] = if device.kind == display::BOOTFB {
                b"bootfb-cpu"
            } else {
                b"native-cpu"
            };
            value.fallback_name[..label.len()].copy_from_slice(label)
        }
        out.write(value);
        1
    }
}
/// # Safety
/// Original owner and exclusive canonical version/size output remain valid
/// for the loaded-module identity query; this never reads a module file.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_driver_module_info(
    owner: u32,
    out: *mut c::DriverModuleInfo,
) -> i32 {
    unsafe {
        if owner == 0
            || out.is_null()
            || (*out).version != 1
            || (*out).size < mem::size_of::<c::DriverModuleInfo>() as u32
        {
            return -1;
        }
        let mut value = c::DriverModuleInfo::default();
        match r4d::r4os_rust_r4d_info(owner, &mut value) {
            0 => 0,
            1 => -2,
            2 => {
                out.write(value);
                1
            }
            _ => unreachable!(),
        }
    }
}
