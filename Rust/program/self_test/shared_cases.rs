//! Exact shared_cases operations of the parent owner; no duplicated state.
use super::*;

pub(super) unsafe fn buffer_stats() -> BufferStats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        program_test_buffer_stats(out.as_mut_ptr());
        out.assume_init()
    }
}

pub(super) unsafe fn graphics_usage() -> crate::memory::vm_query::Usage {
    unsafe {
        let mut out = MaybeUninit::uninit();
        crate::memory::vm_query::r4os_rust_vm_window_usage(8, out.as_mut_ptr());
        out.assume_init()
    }
}

pub(super) unsafe fn shared_lifecycle(heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        if !shared_raster::empty() {
            return false;
        }
        let buffers = buffer_stats();
        let vm = graphics_usage();
        let passed = shared_inner();
        crate::gfx::buffers::collect();
        let after = buffer_stats();
        let vm_after = graphics_usage();
        if !passed
            || buffers != after
            || vm.active_ranges != vm_after.active_ranges
            || vm.committed_bytes != vm_after.committed_bytes
        {
            log(b"[GFXBO] raster balance inner=");
            dec(passed.into());
            for (label, a, b) in [
                (b" objects=" as &[u8], buffers.objects, after.objects),
                (b" refs=", buffers.references, after.references),
                (b" leases=", buffers.leases, after.leases),
                (b" bytes=", buffers.bytes, after.bytes),
                (
                    b" ranges=",
                    vm.active_ranges.into(),
                    vm_after.active_ranges.into(),
                ),
                (b" committed=", vm.committed_bytes, vm_after.committed_bytes),
            ] {
                log(label);
                dec(a);
                log(b"/");
                dec(b);
            }
            log(b"\n");
        }
        passed
            && shared_raster::empty()
            && baseline(heap, base)
            && buffers == after
            && vm.active_ranges == vm_after.active_ranges
            && vm.committed_bytes == vm_after.committed_bytes
            && vm.reserved_bytes == vm_after.reserved_bytes
    }
}

pub(super) fn shared_failure(line: u32) -> bool {
    log(b"[GFXBO] raster selftest failed line=");
    dec(line.into());
    log(b"\n");
    false
}

pub(super) unsafe fn shared_inner() -> bool {
    unsafe {
        let owner_a = c::ProgramProcessHandle {
            instance_id: 0xfffd_0001,
            generation: 101,
            reserved: 0,
        };
        let owner_b = c::ProgramProcessHandle {
            instance_id: 0xfffd_0002,
            generation: 102,
            reserved: 0,
        };
        let consumer_a = c::ProgramProcessHandle {
            instance_id: 0xfffd_0011,
            generation: 201,
            reserved: 0,
        };
        let consumer_b = c::ProgramProcessHandle {
            instance_id: 0xfffd_0012,
            generation: 202,
            reserved: 0,
        };
        let mut frames: [t::Frame; 4] = mem::zeroed();
        for frame in &mut frames {
            frame.chain_depth = 1;
            frame.header.kind = 1;
        }
        struct Cleanup {
            frames: *mut t::Frame,
            owners: [c::ProgramProcessHandle; 4],
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                unsafe {
                    shared_raster::r4os_rust_program_shared_failure(0, false, true);
                    for i in 0..4 {
                        shared_raster::release_frames(self.frames.add(i));
                    }
                    for owner in self.owners {
                        shared_raster::release_process(owner);
                    }
                }
            }
        }
        let _cleanup = Cleanup {
            frames: frames.as_mut_ptr(),
            owners: [consumer_a, consumer_b, owner_a, owner_b],
        };
        macro_rules! check {
            ($value:expr) => {
                if !$value {
                    return shared_failure(line!());
                }
            };
        }
        let info = c::GuiSharedRasterCreateInfo {
            format: c::gui_shared_raster_format_xrgb32,
            width: 4,
            height: 4,
            stride_bytes: 16,
            data_bytes: 64,
            ..Default::default()
        };
        let mut invalid = info;
        invalid.stride_bytes = 15;
        let mut invalid_handle = c::GuiSharedRasterHandle::default();
        check!(
            shared_raster::create(owner_a, invalid, &mut invalid_handle)
                == c::gui_frame_error_invalid
        );
        for failure in 0..c::gui_shared_raster_buffer_count {
            shared_raster::r4os_rust_program_shared_failure(failure, true, true);
            check!(
                shared_raster::create(owner_a, info, &mut invalid_handle) == c::gui_frame_error_oom
            );
        }
        shared_raster::r4os_rust_program_shared_failure(0, false, true);
        let mut handle = c::GuiSharedRasterHandle::default();
        check!(shared_raster::create(owner_a, info, &mut handle) == 0);
        let mut write = c::GuiSharedRasterWriteMap::default();
        check!(
            shared_raster::map_write(owner_a, handle, &mut write) == 0
                && write.byte_length == info.data_bytes
        );
        let bytes = write.data_address as *mut u8;
        let len = write.byte_length as usize;
        for i in 0..len {
            bytes.add(i).write((i * 13 + 7) as u8);
        }
        let mut generation = 0;
        check!(
            shared_raster::publish(owner_a, write, &mut generation) == 0
                && generation != 0
                && shared_raster::publish(owner_a, write, &mut generation)
                    == c::gui_frame_error_stale
        );
        let descriptor = c::GuiSharedRasterResource {
            handle,
            raster_generation: generation,
            format: info.format,
            source_w: info.width,
            source_h: info.height,
            guest_w: info.width,
            guest_h: info.height,
            viewport_w: info.width,
            viewport_h: info.height,
            ..Default::default()
        };
        check!(shared_raster::pin_frame(owner_a, &mut frames[0], descriptor).is_some());
        frames[0].shared_raster_count = 1;
        check!(
            shared_raster::pin_frame(owner_a, &mut frames[0], descriptor).is_some()
                && shared_raster::has_reference(&frames[0], handle, generation)
        );
        let mut map_a = c::GuiSharedRasterMap::default();
        let mut map_b = c::GuiSharedRasterMap::default();
        check!(
            shared_raster::acquire(consumer_a, owner_a, 301, handle, generation, &mut map_a) == 0
                && shared_raster::acquire(consumer_b, owner_a, 301, handle, generation, &mut map_b)
                    == 0
                && map_a.data_address == write.data_address
                && map_b.data_address == write.data_address
                && map_a.byte_length == info.data_bytes
        );
        check!(
            slice::from_raw_parts(map_a.data_address as *const u8, map_a.byte_length as usize)
                == slice::from_raw_parts(bytes, len)
        );
        let mut exported = c::GfxBufferReference::default();
        let mut exported_map = c::GfxBufferMap::default();
        check!(
            shared_raster::r4os_rust_program_shared_export(consumer_a, &map_a.lease, &mut exported)
                == c::gfx_buffer_result_ok
                && exported.flags == 1
                && program_domain_gfx_buffer_map(
                    consumer_a,
                    &exported.reference,
                    0,
                    0,
                    info.data_bytes,
                    &mut exported_map
                ) == c::gfx_buffer_result_ok
                && exported_map.cpu_address == map_a.data_address
        );
        check!(
            shared_raster::release(consumer_b, map_a.lease) == c::gui_frame_error_invalid
                && shared_raster::release(consumer_a, map_a.lease) == 0
                && shared_raster::release(consumer_a, map_a.lease) == c::gui_frame_error_stale
        );
        check!(
            shared_raster::destroy(owner_a, handle) == 0
                && shared_raster::map_write(owner_a, handle, &mut write)
                    == c::gui_frame_error_invalid
                && shared_raster::release_frames(&mut frames[0])
                && shared_raster::release(consumer_b, map_b.lease) == 0
        );
        shared_raster::release_process(owner_a);
        check!(
            slice::from_raw_parts(exported_map.cpu_address as *const u8, 64)
                == slice::from_raw_parts(bytes, len)
        );
        let mut denied = c::GfxBufferMap::default();
        check!(
            program_domain_gfx_buffer_map(consumer_a, &exported.reference, 1, 0, 64, &mut denied)
                == c::gfx_buffer_error_unsupported
                && program_domain_gfx_buffer_unmap(consumer_a, &exported_map.lease)
                    == c::gfx_buffer_result_ok
                && program_domain_gfx_buffer_release(consumer_a, &exported.reference)
                    == c::gfx_buffer_result_ok
        );
        check!(shared_raster::create(owner_a, info, &mut handle) == 0);
        for frame in frames
            .iter_mut()
            .take(c::gui_shared_raster_buffer_count as usize)
        {
            let mut map = c::GuiSharedRasterWriteMap::default();
            let mut generation = 0;
            check!(
                shared_raster::map_write(owner_a, handle, &mut map) == 0
                    && shared_raster::publish(owner_a, map, &mut generation) == 0
            );
            let descriptor = c::GuiSharedRasterResource {
                handle,
                raster_generation: generation,
                ..descriptor
            };
            check!(shared_raster::pin_frame(owner_a, frame, descriptor).is_some());
            frame.shared_raster_count = 1;
        }
        check!(shared_raster::map_write(owner_a, handle, &mut write) == c::gui_frame_error_state);
        let backpressure = shared_raster::snapshot(owner_a);
        check!(
            backpressure.publish_count == u64::from(c::gui_shared_raster_buffer_count)
                && backpressure.backpressure_count == 1
                && backpressure.published_bytes
                    == info.data_bytes * u64::from(c::gui_shared_raster_buffer_count)
        );
        check!(shared_raster::destroy(owner_a, handle) == 0);
        for frame in frames
            .iter_mut()
            .take(c::gui_shared_raster_buffer_count as usize)
        {
            check!(shared_raster::release_frames(frame));
        }
        shared_raster::release_process(owner_a);
        check!(
            shared_raster::create(owner_b, info, &mut handle) == 0
                && shared_raster::map_write(owner_b, handle, &mut write) == 0
                && shared_raster::publish(owner_b, write, &mut generation) == 0
        );
        let cleanup_descriptor = c::GuiSharedRasterResource {
            handle,
            raster_generation: generation,
            ..descriptor
        };
        check!(shared_raster::pin_frame(owner_b, &mut frames[3], cleanup_descriptor).is_some());
        frames[3].shared_raster_count = 1;
        check!(
            shared_raster::acquire(consumer_a, owner_b, 302, handle, generation, &mut map_a) == 0
        );
        shared_raster::release_process(consumer_a);
        check!(shared_raster::release(consumer_a, map_a.lease) == c::gui_frame_error_stale);
        shared_raster::release_process(owner_b);
        check!(shared_raster::release_frames(&mut frames[3]));
        let mut handle_a = c::GuiSharedRasterHandle::default();
        let mut handle_b = c::GuiSharedRasterHandle::default();
        check!(
            shared_raster::create(owner_a, info, &mut handle_a) == 0
                && shared_raster::create(owner_b, info, &mut handle_b) == 0
                && shared_raster::destroy(owner_b, handle_a) == c::gui_frame_error_invalid
                && shared_raster::destroy(owner_a, handle_a) == 0
                && shared_raster::destroy(owner_b, handle_b) == 0
        );
        shared_raster::release_process(owner_a);
        shared_raster::release_process(owner_b);
        true
    }
}
