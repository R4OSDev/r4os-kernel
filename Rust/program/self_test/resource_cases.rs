//! Exact resource_cases operations of the parent owner; no duplicated state.
use super::*;

pub(super) unsafe fn mixed_resources(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let frame = flow::replace(&mut fake, true);
        if frame.is_null() {
            storage::rollback(id, &mut storage);
            return false;
        }
        let expected_alpha = [1u8, 2, 3, 4, 5, 6];
        let mut resources = [0u8; 171];
        for (i, byte) in resources[..161].iter_mut().enumerate() {
            *byte = b'A' + (i % 26) as u8;
        }
        resources[161..165].copy_from_slice(&[0x33, 0x22, 0x11, 0]);
        resources[165..].copy_from_slice(&expected_alpha);
        let commands = [
            c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_text,
                resource_bytes: 161,
                ..Default::default()
            },
            c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_raster,
                w: 1,
                h: 1,
                resource_offset: 161,
                resource_bytes: 4,
                parameter0: 3,
                ..Default::default()
            },
            c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_alpha8,
                w: 3,
                h: 2,
                resource_offset: 165,
                resource_bytes: 6,
                ..Default::default()
            },
        ];
        if batch(&mut fake, &commands, &resources) != 0 || !life::valid(frame, id) {
            storage::rollback(id, &mut storage);
            return false;
        }
        let mut copied_text = [0u8; 161];
        let text_roundtrip = view::resources(frame, 0, copied_text.as_mut_ptr(), 161) == 161
            && copied_text == resources[..161];
        let mut legacy_text = c::GuiCommand::default();
        let legacy_prefix = legacy(frame, 0, &mut legacy_text)
            && legacy_text.text[0] == b'A'
            && legacy_text.text[62] == resources[62]
            && legacy_text.text[63] == 0;
        let mut legacy_raster = c::GuiCommand::default();
        let mut legacy_alpha = c::GuiCommand::default();
        let legacy_commands = legacy(frame, 1, &mut legacy_raster)
            && legacy(frame, 2, &mut legacy_alpha)
            && legacy_raster.flags == 3
            && legacy_raster.rgb == 0
            && legacy_alpha.flags == 1
            && legacy_alpha.fg == 1
            && legacy_alpha.bg == 2;
        let mut words = [0u32; 3];
        let raster_roundtrip = view::rasters(frame, 0, words.as_mut_ptr(), 3) == 3
            && words == [0x00112233, 0x04030201, 0x00000605];
        let mut snapshot = [0u8; 171];
        let text = external(frame, 0);
        let raster = external(frame, 1);
        let alpha = external(frame, 2);
        let snapshot_roundtrip = view::resources(frame, 0, snapshot.as_mut_ptr(), 171) == 171
            && text.flags == 0
            && text.resource_offset == 0
            && text.resource_bytes == 161
            && raster.flags == 0
            && raster.resource_offset == 161
            && raster.resource_bytes == 4
            && raster.parameter0 == 3
            && alpha.flags == 0
            && alpha.resource_offset == 165
            && alpha.resource_bytes == 6
            && alpha.parameter0 == 0
            && alpha.parameter1 == 0
            && snapshot[161..165] == [0x33, 0x22, 0x11, 0]
            && snapshot[165..] == expected_alpha;
        let clone = flow::clone(&mut fake, frame);
        if clone.is_null() {
            return false;
        }
        let clone_ok = (*clone).raster_words == (*frame).raster_words
            && life::valid(clone, id)
            && hash(clone) == hash(frame);
        life::release_frame(clone, id);
        let committed = flow::commit(&mut fake) == 0;
        let mut mixed_append = false;
        if committed {
            let building = flow::ensure(&mut fake);
            if !building.is_null()
                && let Some(mut render) = gs::prepare_blit(&mut fake, building, 1, 2)
            {
                resource_data(render.resource.payload)
                    .cast::<u32>()
                    .write(0x0055_4433);
                let mut value = command(c::gui_frame_command_kind_raster);
                value.w = 1;
                value.h = 1;
                value.flags = 1;
                value.resource_kind = 2;
                value.payload_offset = (*render.resource.payload).logical_offset;
                value.payload_bytes = 4;
                value.raster_word_offset = (*render.resource.payload).raster_word_offset;
                value.parameter0 = 1;
                mixed_append = gs::commit_blit(&mut render, value) == 4;
            }
        }
        let gui = gui(&fake);
        let mixed = (*gui).building_frame;
        let mixed_ok = !mixed.is_null()
            && (*mixed).raster_words == 4
            && (*mixed).command_count == 4
            && (*mixed).resource_len == 175
            && (*(*mixed).resource_tail).resource_kind == 2
            && (*(*mixed).resource_tail).raster_word_offset == 3
            && life::valid(mixed, id);
        let mixed_committed =
            mixed_ok && flow::commit(&mut fake) == 0 && (*(*gui).committed_frame).raster_words == 4;
        let mut reclone_append = false;
        if mixed_committed {
            let building = flow::ensure(&mut fake);
            if !building.is_null()
                && life::valid(building, id)
                && (*(*building).resource_tail).raster_word_offset == 3
                && let Some(mut render) = gs::prepare_blit(&mut fake, building, 1, 2)
            {
                resource_data(render.resource.payload)
                    .cast::<u32>()
                    .write(0x0066_5544);
                let mut value = command(c::gui_frame_command_kind_raster);
                value.w = 1;
                value.h = 1;
                value.flags = 1;
                value.resource_kind = 2;
                value.payload_offset = (*render.resource.payload).logical_offset;
                value.payload_bytes = 4;
                value.raster_word_offset = (*render.resource.payload).raster_word_offset;
                value.parameter0 = 1;
                reclone_append = gs::commit_blit(&mut render, value) == 5;
            }
        }
        let reclone = (*gui).building_frame;
        let reclone_ok = !reclone.is_null()
            && (*reclone).raster_words == 5
            && (*reclone).command_count == 5
            && (*reclone).resource_len == 179
            && (*(*(*reclone).resource_payload).next).raster_word_offset == 3
            && (*(*reclone).resource_tail).raster_word_offset == 4
            && life::valid(reclone, id);
        let reclone_committed = reclone_ok
            && flow::commit(&mut fake) == 0
            && (*(*gui).committed_frame).raster_words == 5;
        let alpha_frame = flow::replace(&mut fake, true);
        if alpha_frame.is_null() {
            return false;
        }
        let Some(mut alpha) = gs::prepare_blit(&mut fake, alpha_frame, 2, 3) else {
            return false;
        };
        let data = resource_data(alpha.resource.payload);
        ptr::write_bytes(data, 0, (*alpha.resource.payload).byte_count as usize);
        ptr::copy_nonoverlapping(expected_alpha.as_ptr(), data, 6);
        let mut value = command(c::gui_frame_command_kind_alpha8);
        value.w = 3;
        value.h = 2;
        value.resource_kind = 3;
        value.payload_offset = (*alpha.resource.payload).logical_offset;
        value.payload_bytes = 6;
        value.raster_word_offset = (*alpha.resource.payload).raster_word_offset;
        gs::commit_blit(&mut alpha, value);
        let snapshot = external(alpha_frame, 0);
        let mut blob = [0xccu8; 8];
        let mut words = [0u32; 2];
        let sequence_ok = (*(*alpha_frame).command_tail).allocation_sequence == 1
            && (*(*alpha_frame).resource_tail).allocation_sequence == 2
            && (*alpha_frame).node_sequence == 2;
        let alpha_ok = life::valid(alpha_frame, id)
            && (*alpha_frame).resource_len == 8
            && snapshot.resource_offset == 0
            && snapshot.resource_bytes == 6
            && snapshot.parameter0 == 0
            && snapshot.parameter1 == 0
            && view::resources(alpha_frame, 0, blob.as_mut_ptr(), 8) == 8
            && blob[..6] == expected_alpha
            && blob[6] == 0
            && blob[7] == 0
            && view::rasters(alpha_frame, 0, words.as_mut_ptr(), 2) == 2
            && words == [0x04030201, 0x00000605];
        (*state()).release_trace_len = 0;
        (*state()).release_trace_enabled = true;
        let cancelled = flow::cancel(&mut fake) == 0;
        (*state()).release_trace_enabled = false;
        let reverse_ok =
            (*state()).release_trace_len == 3 && (&(*state()).release_trace)[..3] == [8, 7, 9];
        let shape_frame = flow::replace(&mut fake, true);
        if shape_frame.is_null() {
            return false;
        }
        let header = c::GuiShapeResource {
            geometry_kind: c::gui_shape_geometry_kind_path,
            segment_count: 2,
            fill_rule: c::gui_shape_fill_rule_nonzero,
            line_join: c::gui_shape_line_join_miter,
            line_cap: c::gui_shape_line_cap_square,
            stroke_argb: 0xff112233,
            stroke_width_bits: 2f32.to_bits(),
            miter_limit_bits: 4f32.to_bits(),
            ..Default::default()
        };
        let segments = [
            c::GuiPathSegment {
                kind: c::gui_path_segment_kind_move,
                x1_bits: 1f32.to_bits(),
                y1_bits: 2f32.to_bits(),
                ..Default::default()
            },
            c::GuiPathSegment {
                kind: c::gui_path_segment_kind_line,
                x1_bits: 20f32.to_bits(),
                y1_bits: 9f32.to_bits(),
                ..Default::default()
            },
        ];
        const SHAPE: usize = mem::size_of::<c::GuiShapeResource>();
        let mut shape = [0u8; SHAPE + 2 * mem::size_of::<c::GuiPathSegment>()];
        shape[..SHAPE].copy_from_slice(bytes_of(&header));
        shape[SHAPE..].copy_from_slice(bytes_of(&segments));
        let value = c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_path_stroke,
            w: 32,
            h: 16,
            resource_bytes: shape.len() as u64,
            ..Default::default()
        };
        let shape_ok = batch(&mut fake, &[value], &shape) == 0
            && life::valid(shape_frame, id)
            && (*shape_frame).command_count == 1
            && (*shape_frame).resource_len == shape.len() as u64
            && (*at(shape_frame, 0)).resource_kind == 4;
        let mut truncated = value;
        truncated.resource_bytes -= 1;
        let mut offset = value;
        offset.resource_offset = 1;
        let mut oversized = value;
        oversized.w = c::gui_shape_max_dimension + 1;
        let mut invalid_kind = shape;
        invalid_kind[SHAPE + mem::offset_of!(c::GuiPathSegment, kind)] = 99;
        let mut non_finite = shape;
        let coordinate = SHAPE + mem::offset_of!(c::GuiPathSegment, x1_bits);
        non_finite[coordinate..coordinate + 4].copy_from_slice(&[0, 0, 0x80, 0x7f]);
        let shape_negative = !super::super::gui_resources::batch(&truncated, &shape)
            && !super::super::gui_resources::batch(&offset, &shape)
            && !super::super::gui_resources::batch(&oversized, &shape)
            && !super::super::gui_resources::batch(&value, &invalid_kind)
            && !super::super::gui_resources::batch(&value, &non_finite);
        let shape_cancelled = flow::cancel(&mut fake) == 0;
        let argb_frame = flow::replace(&mut fake, true);
        if argb_frame.is_null() {
            return false;
        }
        let argb = [0x33u8, 0x22, 0x11, 0x80, 0x66, 0x55, 0x44, 0xff];
        let value = c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_argb32,
            w: 2,
            h: 1,
            resource_bytes: argb.len() as u64,
            parameter0: 2,
            ..Default::default()
        };
        let argb_ok = batch(&mut fake, &[value], &argb) == 0
            && life::valid(argb_frame, id)
            && (*argb_frame).command_count == 1
            && (*argb_frame).raster_words == 0
            && (*at(argb_frame, 0)).resource_kind == 4;
        let mut legacy_argb = c::GuiCommand::default();
        let mut invalid_size = value;
        invalid_size.resource_bytes -= 1;
        let mut invalid_scale = value;
        invalid_scale.parameter0 = 0;
        let mut invalid_width = value;
        invalid_width.w = c::gui_argb32_max_width + 1;
        let argb_negative = !legacy(argb_frame, 0, &mut legacy_argb)
            && !super::super::gui_resources::batch(&invalid_size, &argb)
            && !super::super::gui_resources::batch(&invalid_scale, &argb)
            && !super::super::gui_resources::batch(&invalid_width, &argb);
        let argb_cancelled = flow::cancel(&mut fake) == 0;
        let invalid_header = c::GuiFrameInfo {
            version: 0,
            size: c::gui_frame_info_size,
            last_error: 0x12345678,
            ..Default::default()
        };
        let before = invalid_header;
        let invalid_ok = !view::valid_info(&invalid_header) && info_equal(&invalid_header, &before);
        let future = c::GuiFrameInfo {
            version: c::gui_frame_info_version + 1,
            size: c::gui_frame_info_size,
            ..Default::default()
        };
        let future_ok = view::valid_info(&future);
        let valid = text_roundtrip
            && legacy_prefix
            && legacy_commands
            && raster_roundtrip
            && snapshot_roundtrip
            && clone_ok
            && mixed_append
            && mixed_committed
            && reclone_append
            && reclone_committed
            && sequence_ok
            && alpha_ok
            && cancelled
            && reverse_ok
            && shape_ok
            && shape_negative
            && shape_cancelled
            && argb_ok
            && argb_negative
            && argb_cancelled
            && invalid_ok
            && future_ok;
        storage::rollback(id, &mut storage);
        valid && baseline(heap, base)
    }
}

pub(super) unsafe fn delta_indexed(
    id: u32,
    heap: &heap::Stats,
    baseline_storage: &t::Stats,
) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let valid = (|| {
            if flow::begin(&mut fake, 0, &[]) != 0 {
                return false;
            }
            let commands = [
                c::GuiFrameCommand {
                    kind: c::gui_frame_command_kind_clear,
                    rgb: 0x010203,
                    ..Default::default()
                },
                c::GuiFrameCommand {
                    kind: c::gui_frame_command_kind_text,
                    resource_bytes: 1,
                    ..Default::default()
                },
            ];
            if batch(&mut fake, &commands, b"B") != 0 || flow::commit(&mut fake) != 0 {
                return false;
            }
            let gui = gui(&fake);
            let base = (*gui).committed_frame;
            if base.is_null() {
                return false;
            }
            let generation = (*base).generation;
            let base_bytes = life::frame_bytes(base);
            let damage = [
                c::DisplayDamageRect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
                c::DisplayDamageRect {
                    x: 3,
                    y: 3,
                    w: 1,
                    h: 1,
                },
            ];
            if flow::begin(&mut fake, 1, &damage) != 0 {
                return false;
            }
            let mut resource = [0u8; c::gui_indexed8_pixels_offset as usize + 4];
            let header = c::GuiIndexed8Resource {
                source_w: 2,
                source_h: 2,
                guest_w: 2,
                guest_h: 2,
                viewport_w: 4,
                viewport_h: 4,
                pixel_stride: 2,
                ..Default::default()
            };
            resource[..mem::size_of::<c::GuiIndexed8Resource>()].copy_from_slice(bytes_of(&header));
            let palette = c::gui_indexed8_palette_offset as usize;
            resource[palette + 4..palette + 8].copy_from_slice(&[0x33, 0x22, 0x11, 0]);
            resource[c::gui_indexed8_pixels_offset as usize..].copy_from_slice(&[0, 1, 1, 0]);
            let value = c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_indexed8,
                w: 4,
                h: 4,
                resource_bytes: resource.len() as u64,
                ..Default::default()
            };
            if !super::super::gui_resources::batch(&value, &resource)
                || batch(&mut fake, &[value], &resource) != 0
                || flow::commit(&mut fake) != 0
            {
                return false;
            }
            let delta = (*gui).committed_frame;
            if delta.is_null() {
                return false;
            }
            if (*delta).base_frame != base
                || (*delta).chain_depth != 2
                || (*delta).damage_count != damage.len() as u32
                || (*delta).command_count != 1
                || life::chain_count(delta, 0) != 3
                || life::chain_count(delta, 1) != resource.len() as u64 + 1
                || (*gui).frame_delta_commits != 1
                || (*gui).frame_full_commits != 1
                || (*gui).frame_indexed8_commands != 1
                || (*gui).frame_indexed8_resource_bytes != resource.len() as u64
                || (*gui).frame_avoided_clone_bytes < base_bytes
                || !accounting::at_peak(gui)
            {
                return false;
            }
            let mut info = c::GuiFrameGenerationInfo::default();
            view::generation(
                gui,
                c::ProgramProcessHandle {
                    instance_id: id,
                    generation: 1,
                    reserved: 0,
                },
                delta,
                &mut info,
            );
            if info.flags & c::gui_frame_generation_flag_delta == 0
                || info.flags & c::gui_frame_generation_flag_indexed8 == 0
                || info.base_generation != generation
                || info.command_count != 1
                || info.total_command_count != 3
                || info.damage_count != damage.len() as u32
            {
                return false;
            }
            let mut local = [c::GuiFrameCommand::default(); 1];
            let mut full = [c::GuiFrameCommand::default(); 3];
            let mut local_resource = [0u8; c::gui_indexed8_pixels_offset as usize + 4];
            if view::local_commands(delta, 0, local.as_mut_ptr(), 1) != 1
                || view::commands(delta, full.as_mut_ptr(), 3) != 3
                || view::local_resources(
                    delta,
                    0,
                    local_resource.as_mut_ptr(),
                    local_resource.len(),
                ) != local_resource.len()
            {
                return false;
            }
            let local_damage =
                ptr::addr_of!((*delta).damage_regions).cast::<c::DisplayDamageRect>();
            if local[0].kind != c::gui_frame_command_kind_indexed8
                || local[0].resource_offset != 0
                || full[2].resource_offset != 1
                || local_resource != resource
                || !(0..damage.len()).all(|i| {
                    let a = local_damage.add(i).read();
                    let b = damage[i];
                    a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h
                })
            {
                return false;
            }
            let mut bad = header;
            bad.pixel_stride = 3;
            let mut invalid = resource;
            invalid[..mem::size_of::<c::GuiIndexed8Resource>()].copy_from_slice(bytes_of(&bad));
            if super::super::gui_resources::batch(&value, &invalid) {
                return false;
            }
            let Some(mut capture) = flow::capture(&mut fake, Some(generation), true) else {
                return false;
            };
            if capture.frame != base || (*base).reader_refs != 1 {
                return false;
            }
            if flow::begin(&mut fake, 0, &[]) != 0 {
                return false;
            }
            let replacement = [c::GuiFrameCommand {
                kind: c::gui_frame_command_kind_clear,
                rgb: 0x0a0b0c,
                ..Default::default()
            }];
            if batch(&mut fake, &replacement, &[]) != 0
                || flow::commit(&mut fake) != 0
                || (*gui).retired_frames != delta
                || !accounting::at_peak(gui)
            {
                return false;
            }
            let final_peak = accounting::peak(gui);
            flow::release_capture(&mut fake, &mut capture);
            if !(*gui).retired_frames.is_null()
                || accounting::checked(gui).is_none()
                || accounting::peak(gui) != final_peak
            {
                return false;
            }
            true
        })();
        storage::rollback(id, &mut storage);
        valid && baseline(heap, baseline_storage)
    }
}

pub(super) unsafe fn replacement_xrgb(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let damage = [c::DisplayDamageRect {
            x: 1,
            y: 1,
            w: 1,
            h: 1,
        }];
        let mut resource = [0u8; c::gui_xrgb32_pixels_offset as usize + 4 * mem::size_of::<u32>()];
        let header = c::GuiXrgb32Resource {
            source_w: 2,
            source_h: 2,
            guest_w: 2,
            guest_h: 2,
            viewport_w: 2,
            viewport_h: 2,
            pixel_stride: 2,
            ..Default::default()
        };
        resource[..mem::size_of::<c::GuiXrgb32Resource>()].copy_from_slice(bytes_of(&header));
        let value = c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_xrgb32_nearest,
            w: 2,
            h: 2,
            resource_bytes: resource.len() as u64,
            ..Default::default()
        };
        let mut invalid = resource;
        invalid[c::gui_xrgb32_pixels_offset as usize + 3] = 1;
        if super::super::gui_resources::batch(&value, &invalid) {
            storage::rollback(id, &mut storage);
            return false;
        }
        let mut capture = None;
        let mut lifecycle_ok = true;
        let gui = gui(&fake);
        for index in 0u32..64 {
            resource[c::gui_xrgb32_pixels_offset as usize] = index as u8;
            let begin = if index == 0 {
                flow::begin(&mut fake, 0, &[])
            } else {
                flow::begin(&mut fake, 2, &damage)
            };
            if begin != 0
                || batch(&mut fake, &[value], &resource) != 0
                || flow::commit(&mut fake) != 0
            {
                lifecycle_ok = false;
                break;
            }
            let committed = (*gui).committed_frame;
            if committed.is_null() {
                lifecycle_ok = false;
                break;
            }
            if !(*committed).base_frame.is_null()
                || (*committed).chain_depth != 1
                || (*committed).replacement != (index != 0)
                || life::chain_count(committed, 0) != 1
                || life::chain_count(committed, 1) != resource.len() as u64
                || accounting::checked(gui).is_none()
            {
                lifecycle_ok = false;
                break;
            }
            if index == 0 {
                let Some(held) = flow::capture(&mut fake, None, false) else {
                    lifecycle_ok = false;
                    break;
                };
                capture = Some(held);
            } else if index == 1 {
                if (*gui).retired_frames.is_null() || life::live_generations(gui) != 2 {
                    lifecycle_ok = false;
                    break;
                }
                flow::release_capture(&mut fake, capture.as_mut().expect("First frame capture"));
                capture = None;
                if !(*gui).retired_frames.is_null()
                    || life::live_generations(gui) != 1
                    || accounting::checked(gui).is_none()
                {
                    lifecycle_ok = false;
                    break;
                }
            }
        }
        if let Some(ref mut held) = capture {
            flow::release_capture(&mut fake, held);
        }
        let latest = (*gui).committed_frame;
        let owner = c::ProgramProcessHandle {
            instance_id: id,
            generation: 77,
            reserved: 0,
        };
        let mut info = c::GuiFrameGenerationInfo::default();
        if !latest.is_null() {
            view::generation(gui, owner, latest, &mut info);
        }
        let mut stream = c::GuiFrameStreamInfo::default();
        view::stream(gui, owner, &mut stream);
        let committed_before_oom = latest;
        fail_after(Some(0));
        let rejected = flow::begin(&mut fake, 2, &damage) == c::gui_frame_error_oom;
        fail_after(None);
        let counters = lifecycle_ok
            && !latest.is_null()
            && (*gui).frame_commits == 64
            && (*gui).frame_full_commits == 64
            && (*gui).frame_replacement_commits == 63
            && (*gui).frame_superseded_generations == 63
            && (*gui).frame_coalesced_generations == 63
            && (*gui).frame_reader_retired_generations == 1
            && (*gui).frame_xrgb32_nearest_commands == 64
            && (*gui).frame_xrgb32_nearest_resource_bytes == 64 * resource.len() as u64
            && life::live_generations(gui) == 1
            && info.flags & c::gui_frame_generation_flag_full != 0
            && info.flags & c::gui_frame_generation_flag_replacement != 0
            && info.chain_depth == 1
            && stream.live_generation_count == 1
            && stream.replacement_commit_count == 63
            && stream.coalesced_generation_count == 63
            && stream.xrgb32_nearest_command_count == 64
            && stream.current_frame_bytes == life::frame_bytes(latest)
            && stream.peak_frame_bytes >= stream.current_frame_bytes
            && rejected
            && (*gui).building_frame.is_null()
            && (*gui).committed_frame == committed_before_oom;
        storage::rollback(id, &mut storage);
        counters && baseline(heap, base)
    }
}

pub(super) fn info_equal(a: &c::GuiFrameInfo, b: &c::GuiFrameInfo) -> bool {
    a.version == b.version
        && a.size == b.size
        && a.state == b.state
        && a.flags == b.flags
        && super::super::registry::equal(a.owner, b.owner)
        && a.committed_generation == b.committed_generation
        && a.building_generation == b.building_generation
        && a.committed_command_count == b.committed_command_count
        && a.committed_resource_bytes == b.committed_resource_bytes
        && a.building_command_count == b.building_command_count
        && a.building_resource_bytes == b.building_resource_bytes
        && a.current_frame_bytes == b.current_frame_bytes
        && a.peak_frame_bytes == b.peak_frame_bytes
        && a.commit_count == b.commit_count
        && a.cancel_count == b.cancel_count
        && a.oom_count == b.oom_count
        && a.snapshot_read_count == b.snapshot_read_count
        && a.last_error == b.last_error
        && a.command_version == b.command_version
        && a.command_size == b.command_size
        && a.reserved0 == b.reserved0
        && a.reserved1 == b.reserved1
        && a.reserved2 == b.reserved2
        && a.reserved3 == b.reserved3
        && a.reserved4 == b.reserved4
}
