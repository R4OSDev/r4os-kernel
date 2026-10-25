//! Exact storage_cases operations of the parent owner; no duplicated state.
use super::*;

pub(super) unsafe fn failed(case_id: u32, heap: &heap::Stats, storage: &t::Stats) -> bool {
    unsafe {
        let heap_ok = heap_equal(heap, &heap_stats());
        let storage_ok = storage_equal(storage, &stats());
        (*state()).report = t::SelfTestReport {
            cases: case_id,
            failed_case: case_id,
            peak_payload_bytes: (*state()).self_test_peak_bytes,
            heap_baseline_ok: heap_ok,
            storage_baseline_ok: storage_ok,
            zero_init_ok: false,
        };
        log(b"[PINSTSTOR] result=FAILED case=");
        dec(case_id.into());
        log(b" heap_baseline=");
        log(if heap_ok { b"ok" } else { b"failed" });
        log(b" storage_baseline=");
        log(if storage_ok { b"ok" } else { b"failed" });
        if !heap_ok {
            let after = heap_stats();
            log(b" used=");
            dec(heap.used_bytes as u64);
            log(b"/");
            dec(after.used_bytes as u64);
            log(b" blocks=");
            dec(heap.active_blocks as u64);
            log(b"/");
            dec(after.active_blocks as u64);
            log(b" committed=");
            dec(heap.committed_bytes as u64);
            log(b"/");
            dec(after.committed_bytes as u64);
            log(b" free-blocks=");
            dec(heap.free_blocks as u64);
            log(b"/");
            dec(after.free_blocks as u64);
        }
        log(b"\r\n");
        false
    }
}

pub(super) unsafe fn lazy(
    id: u32,
    which: u8,
    fail: u32,
    heap: &heap::Stats,
    base: &t::Stats,
) -> bool {
    unsafe {
        let class = if which <= 1 { 2 } else { 1 };
        let Some(mut storage) = storage::allocate(id, class, false) else {
            return false;
        };
        let mut fake = fake(id, class, storage);
        let frame = if which == 3 {
            let frame = gs::allocate_frame(id, false);
            if frame.is_null() {
                storage::rollback(id, &mut storage);
                return false;
            }
            life::set_building(gui(&fake), frame);
            frame
        } else {
            ptr::null_mut()
        };
        fail_after(Some(fail));
        let failed = match which {
            0 => process_data::ensure_environment(&mut fake).is_null(),
            1 => {
                let value = process_data::ensure_gui(&mut fake).cast::<t::Gui>();
                if !value.is_null() {
                    storage.gui = value;
                }
                value.is_null()
            }
            2 => flow::ensure(&mut fake).is_null(),
            3 => {
                let prepared = gs::prepare_blit(&mut fake, frame, 1, 2);
                let failed = prepared.is_none();
                if let Some(mut render) = prepared {
                    gs::cancel_blit(&mut fake, &mut render);
                }
                failed
            }
            _ => unreachable!(),
        };
        fail_after(None);
        storage::rollback(id, &mut storage);
        failed && baseline(heap, base)
    }
}

pub(super) unsafe fn zero_initialization(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut first) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut first_fake = fake(id, 1, first);
        let environment =
            process_data::ensure_environment(&mut first_fake).cast::<t::Environment>();
        if environment.is_null() {
            storage::rollback(id, &mut first);
            return false;
        }
        let frame = gs::allocate_frame(id, false);
        if frame.is_null() {
            storage::rollback(id, &mut first);
            return false;
        }
        life::set_building(gui(&first_fake), frame);
        let Some(mut render) = gs::prepare_blit(&mut first_fake, frame, 1, 2) else {
            storage::rollback(id, &mut first);
            return false;
        };
        (*environment).bytes[0] = 0xa5;
        (*commands(render.command.payload)).kind = 0xa5a5_a5a5;
        resource_data(render.resource.payload)
            .cast::<u32>()
            .write(0xa5a5_a5a5);
        gs::cancel_blit(&mut first_fake, &mut render);
        storage::rollback(id, &mut first);
        let Some(mut second) = storage::allocate(id + 1, 1, false) else {
            return false;
        };
        let mut second_fake = fake(id + 1, 1, second);
        let environment =
            process_data::ensure_environment(&mut second_fake).cast::<t::Environment>();
        if environment.is_null() {
            storage::rollback(id + 1, &mut second);
            return false;
        }
        let frame = gs::allocate_frame(id + 1, false);
        if frame.is_null() {
            storage::rollback(id + 1, &mut second);
            return false;
        }
        life::set_building(gui(&second_fake), frame);
        let Some(mut render) = gs::prepare_blit(&mut second_fake, frame, 1, 2) else {
            storage::rollback(id + 1, &mut second);
            return false;
        };
        let zeroed = (*environment).bytes[0] == 0
            && (*commands(render.command.payload)).kind == 0
            && resource_data(render.resource.payload).cast::<u32>().read() == 0;
        gs::cancel_blit(&mut second_fake, &mut render);
        storage::rollback(id + 1, &mut second);
        zeroed && baseline(heap, base)
    }
}

pub(super) unsafe fn atomic_growth_failure(id: u32, heap: &heap::Stats, base: &t::Stats) -> bool {
    unsafe {
        let Some(mut storage) = storage::allocate(id, 1, false) else {
            return false;
        };
        let mut fake = fake(id, 1, storage);
        let initial = flow::replace(&mut fake, true);
        if initial.is_null() {
            return false;
        }
        let Some(mut prepared) = gs::prepare_command(&mut fake, initial) else {
            return false;
        };
        let mut value = command(c::gui_frame_command_kind_clear);
        value.rgb = 0x123456;
        gs::commit_command(&mut prepared, value);
        if flow::commit(&mut fake) != 0 {
            return false;
        }
        let gui = gui(&fake);
        let committed = (*gui).committed_frame;
        if committed.is_null() {
            return false;
        }
        let generation = (*committed).generation;
        let old_hash = hash(committed);
        for boundary in [0usize, 8, 24, 56, 120, 248] {
            if flow::begin(&mut fake, 0, &[]) != 0 {
                return false;
            }
            let building = (*gui).building_frame;
            if building.is_null() {
                return false;
            }
            for index in 0..boundary {
                let Some(mut prepared) = gs::prepare_command(&mut fake, building) else {
                    return false;
                };
                let mut value = command(c::gui_frame_command_kind_rect);
                value.rgb = index as u32;
                gs::commit_command(&mut prepared, value);
            }
            fail_after(Some(0));
            let rejected = gs::prepare_command(&mut fake, building).is_none();
            fail_after(None);
            flow::failed(gui, building);
            if !rejected
                || flow::commit(&mut fake) != c::gui_frame_error_state
                || (*gui).committed_frame != committed
                || (*committed).generation != generation
                || hash(committed) != old_hash
                || flow::cancel(&mut fake) != 0
                || accounting::checked(gui).is_none()
            {
                storage::rollback(id, &mut storage);
                return false;
            }
        }
        const BYTES: usize = 64 * 1024 + 1;
        let resource = heap::r4os_rust_heap_alloc(BYTES, 1);
        if resource.is_null() {
            storage::rollback(id, &mut storage);
            return false;
        }
        ptr::write_bytes(resource, 0, BYTES);
        let mut commands = [c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_rect,
            ..Default::default()
        }; 9];
        commands[8] = c::GuiFrameCommand {
            kind: c::gui_frame_command_kind_text,
            resource_bytes: BYTES as u64,
            ..Default::default()
        };
        let mut batches_ok = true;
        for failure in 0..5 {
            let iteration_heap = heap_stats();
            let iteration_storage = stats();
            if flow::begin(&mut fake, 0, &[]) != 0 {
                batches_ok = false;
                break;
            }
            fail_after(Some(failure));
            let rejected = batch(&mut fake, &commands, slice::from_raw_parts(resource, BYTES))
                == c::gui_frame_error_oom;
            fail_after(None);
            let failed_build =
                !(*gui).building_frame.is_null() && (*(*gui).building_frame).build_failed;
            let old_unchanged = flow::commit(&mut fake) == c::gui_frame_error_state
                && (*gui).committed_frame == committed
                && (*committed).generation == generation
                && hash(committed) == old_hash;
            let cancelled = flow::cancel(&mut fake) == 0 && accounting::checked(gui).is_some();
            if !rejected
                || !failed_build
                || !old_unchanged
                || !cancelled
                || !baseline(&iteration_heap, &iteration_storage)
            {
                batches_ok = false;
                break;
            }
        }
        fail_after(None);
        if !(*gui).building_frame.is_null() {
            flow::cancel(&mut fake);
        }
        let resource_released = heap::r4os_rust_heap_free(resource, BYTES) == 0;
        let old_ready =
            gui_events::r4os_rust_program_gui_text(&mut fake, c"old".as_ptr().cast()) == 3;
        let text_committed = (*gui).committed_frame;
        if text_committed.is_null() {
            return false;
        }
        let text_generation = (*text_committed).generation;
        let text_hash = hash(text_committed);
        fail_after(Some(0));
        let text_rejected =
            gui_events::r4os_rust_program_gui_text(&mut fake, c"new".as_ptr().cast()) == -2;
        fail_after(None);
        let text_atomic = old_ready
            && text_rejected
            && (*gui).building_frame.is_null()
            && (*gui).committed_frame == text_committed
            && (*text_committed).generation == text_generation
            && hash(text_committed) == text_hash
            && (&(*gui).text)[..3] == *b"old"
            && (*gui).text[3] == 0;
        storage::rollback(id, &mut storage);
        batches_ok && resource_released && text_atomic && baseline(heap, base)
    }
}

#[inline(never)]
pub(super) fn owner_stack_body() -> bool {
    let mut scratch = [MaybeUninit::<u8>::uninit(); 32 * 1024];
    let bytes = scratch.as_mut_ptr().cast::<u8>();
    unsafe {
        for i in (0..scratch.len()).step_by(4096) {
            ptr::write_volatile(bytes.add(i), 0x79);
        }
        ptr::write_volatile(bytes.add(scratch.len() - 1), 0x05);
        blocks::r4os_rust_blocks_invariant()
            && ptr::read_volatile(bytes) == 0x79
            && ptr::read_volatile(bytes.add(scratch.len() - 1)) == 0x05
    }
}

pub(super) unsafe extern "C" fn owner_stack_entry(arg: usize) -> i32 {
    unsafe {
        let probe = &*(arg as *const OwnerProbe);
        if probe.runtime {
            let token = runtime::save_runtime();
            let before = ptr::read_volatile(probe.committed);
            let result = owner_stack_body() && ptr::read_volatile(probe.committed) == before;
            runtime::restore_runtime(token);
            i32::from(result)
        } else {
            let _guard = Guard::acquire(7);
            let before = ptr::read_volatile(probe.committed);
            i32::from(owner_stack_body() && ptr::read_volatile(probe.committed) == before)
        }
    }
}

pub(super) unsafe fn stack_headroom() -> bool {
    unsafe {
        let thread = threads::current();
        if thread.is_null() {
            return false;
        }
        let instance = (*thread).owner_instance;
        if instance.is_null() || (*thread).flags & 1 == 0 {
            return false;
        }
        for runtime in [false, true] {
            let flags = crate::arch::runtime::r4os_rust_runtime_control(2, 0);
            crate::memory::owner::prepare_stack();
            crate::arch::runtime::r4os_rust_runtime_control(3, flags);
            let rsp: u64;
            core::arch::asm!("mov {},rsp",out(reg)rsp,options(nomem,nostack,preserves_flags));
            let top = (*instance).program_stack_committed_base + 4096;
            if top + 32 * 1024 >= rsp {
                return false;
            }
            let before = (*instance).program_stack_committed_size;
            let probe = OwnerProbe {
                committed: ptr::addr_of!((*instance).program_stack_committed_size),
                runtime,
            };
            if r4os_call_program(
                owner_stack_entry as *const () as usize,
                ptr::from_ref(&probe).addr(),
                top,
            ) != 1
                || (*instance).program_stack_committed_size <= before
            {
                return false;
            }
        }
        log(b"[STACKOWNER] result=OK physical=resident runtime=resident index=ok\r\n");
        true
    }
}
