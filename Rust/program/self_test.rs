//! Original program-storage/GUI diagnostic cases, with unchanged failure seams,
//! expected values, rollback ordering and ownership baselines. No new test suite.
use super::{
    domain_api::{
        program_domain_gfx_buffer_map, program_domain_gfx_buffer_release,
        program_domain_gfx_buffer_unmap,
    },
    gui_batch, gui_draw, gui_events, gui_flow as flow, gui_lifetime as life, gui_storage as gs,
    gui_view as view, instance_storage as storage, memory, payload, payload_types as t,
    process_data,
    registry_types::ProgramInstance,
    shared_raster, threads,
};
use crate::{
    arch::runtime,
    boot::foundation,
    contract as c,
    fs::vfs_types::Bytes,
    memory::{blocks, heap, owner::Guard},
};
use core::{
    mem::{self, MaybeUninit},
    ptr, slice,
};
const PIXELS: usize = 128 * 128;
fn state() -> *mut payload::State {
    payload::r4os_rust_program_payload_state()
}
fn shared() -> *mut shared_raster::Debug {
    shared_raster::r4os_rust_program_shared_debug()
}
fn span(s: &[u8]) -> Bytes {
    Bytes {
        data: s.as_ptr(),
        len: s.len(),
    }
}
fn log(s: &[u8]) {
    foundation::puts(s)
}
fn dec(n: u64) {
    foundation::dec(n)
}
unsafe fn heap_stats() -> heap::Stats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        heap::r4os_rust_heap_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
unsafe fn stats() -> t::Stats {
    unsafe {
        let mut out = MaybeUninit::uninit();
        payload::r4os_rust_program_payload_stats(out.as_mut_ptr());
        out.assume_init()
    }
}
fn fail_after(value: Option<u32>) {
    unsafe {
        payload::r4os_rust_program_payload_failure(value.is_some(), value.unwrap_or(0));
    }
}
unsafe fn fake(id: u32, class: u8, storage: t::InstanceStorage) -> ProgramInstance {
    unsafe {
        let mut instance: ProgramInstance = mem::zeroed();
        instance.id = id;
        instance.app_class = class;
        instance.memory_profile = 2;
        instance.memory_limits = memory::Limits {
            vm_reserve_limit: 1024 * 1024 * 1024,
            vm_commit_limit: 256 * 1024 * 1024,
            resident_limit: 256 * 1024 * 1024,
            stack_reserve: 4 * 1024 * 1024,
            stack_initial_commit: 64 * 1024,
        };
        instance.runtime_payload = storage.runtime.cast();
        instance.process_payload = storage.process.cast();
        instance.console_payload = storage.console.cast();
        instance.gui_payload = storage.gui.cast();
        instance
    }
}
unsafe fn gui(instance: *const ProgramInstance) -> *mut t::Gui {
    unsafe { (*instance).gui_payload.cast() }
}
unsafe fn commands(payload: *mut t::Commands) -> *mut t::GuiCommand {
    unsafe {
        payload::r4os_rust_program_payload_span(0, payload.cast())
            .data
            .cast()
    }
}
unsafe fn resource_data(payload: *mut t::Resource) -> *mut u8 {
    unsafe {
        payload::r4os_rust_program_payload_span(1, payload.cast())
            .data
            .cast()
    }
}
fn command(kind: u32) -> t::GuiCommand {
    let mut c: t::GuiCommand = unsafe { mem::zeroed() };
    c.version = 1;
    c.size = 96;
    c.kind = kind;
    c
}
fn heap_equal(a: &heap::Stats, b: &heap::Stats) -> bool {
    a.used_bytes == b.used_bytes
        && a.active_blocks == b.active_blocks
        && a.free_blocks == b.free_blocks
        && a.invalid_free_errors == b.invalid_free_errors
        && a.double_free_errors == b.double_free_errors
        && a.size_mismatch_errors == b.size_mismatch_errors
        && a.reentry_errors == b.reentry_errors
}
fn storage_equal(a: &t::Stats, b: &t::Stats) -> bool {
    a.live_core_bytes == b.live_core_bytes
        && a.active_instance_bytes == b.active_instance_bytes
        && a.reserved_instance_bytes == b.reserved_instance_bytes
        && a.current_payload_bytes == b.current_payload_bytes
        && a.current_runtime_bytes == b.current_runtime_bytes
        && a.current_console_bytes == b.current_console_bytes
        && a.current_gui_bytes == b.current_gui_bytes
        && a.active_instances == b.active_instances
        && a.active_service_instances == b.active_service_instances
        && a.active_console_instances == b.active_console_instances
        && a.active_gui_instances == b.active_gui_instances
        && a.active_runtime_payloads == b.active_runtime_payloads
        && a.active_process_payloads == b.active_process_payloads
        && a.active_environment_payloads == b.active_environment_payloads
        && a.active_console_payloads == b.active_console_payloads
        && a.active_console_output_payloads == b.active_console_output_payloads
        && a.active_gui_payloads == b.active_gui_payloads
        && a.active_gui_frame_payloads == b.active_gui_frame_payloads
        && a.active_gui_command_payloads == b.active_gui_command_payloads
        && a.active_gui_raster_payloads == b.active_gui_raster_payloads
        && a.active_gui_data_payloads == b.active_gui_data_payloads
        && a.current_gui_frame_bytes == b.current_gui_frame_bytes
        && a.current_gui_frame_commands == b.current_gui_frame_commands
        && a.current_gui_frame_nodes == b.current_gui_frame_nodes
        && a.owner_mismatches == b.owner_mismatches
        && a.header_errors == b.header_errors
        && a.free_failures == b.free_failures
        && a.quarantined_payloads == b.quarantined_payloads
        && a.quarantined_bytes == b.quarantined_bytes
}
unsafe fn baseline(heap: &heap::Stats, storage: &t::Stats) -> bool {
    unsafe { heap_equal(heap, &heap_stats()) && storage_equal(storage, &stats()) }
}
unsafe fn warm() -> bool {
    unsafe {
        let Some(mut console) = storage::allocate(0xffff_ff01, 0, true) else {
            return false;
        };
        storage::rollback(0xffff_ff01, &mut console);
        let Some(mut storage) = storage::allocate(0xffff_ff02, 1, false) else {
            return false;
        };
        let mut fake = fake(0xffff_ff02, 1, storage);
        let frame = gs::allocate_frame(fake.id, false);
        if frame.is_null() {
            storage::rollback(fake.id, &mut storage);
            return false;
        }
        (*gui(&fake)).building_frame = frame;
        let Some(mut render) = gs::prepare_blit(&mut fake, frame, PIXELS, 2) else {
            storage::rollback(fake.id, &mut storage);
            return false;
        };
        gs::cancel_blit(&mut fake, &mut render);
        if gui_draw::text(
            &mut fake,
            0,
            0,
            c"warm".as_ptr().cast(),
            0x00ff_ffff,
            0,
            0,
            0,
        ) < 0
            || !append_rasters(&mut fake, 6)
        {
            storage::rollback(fake.id, &mut storage);
            return false;
        }
        storage::rollback(fake.id, &mut storage);
        true
    }
}

unsafe extern "C" {
    fn r4os_call_program(entry: usize, arg: usize, stack: u64) -> i32;
}

struct OwnerProbe {
    committed: *const u64,
    runtime: bool,
}

/// # Safety
/// Original admitted single-test context. Every failure seam is restored on exit.
pub(super) unsafe fn run() -> bool {
    unsafe {
        let s = state();
        let shared = self::shared();
        let saved_failure = (
            (*s).failure_after_present,
            (*s).failure_after_value,
            (*s).failure_cursor,
        );
        let saved_active = (*s).self_test_active;
        let saved_peak = (*s).self_test_peak_bytes;
        let saved_trace = ((*s).release_trace_enabled, (*s).release_trace_len);
        let saved_shared = (
            (*shared).failure_present,
            (*shared).failure_value,
            (*shared).failure_cursor,
        );
        struct Restore {
            failure: (bool, u32, u32),
            active: bool,
            peak: u64,
            trace: (bool, usize),
            shared: (bool, u32, u32),
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    let s = state();
                    (*s).failure_after_present = self.failure.0;
                    (*s).failure_after_value = self.failure.1;
                    (*s).failure_cursor = self.failure.2;
                    (*s).self_test_active = self.active;
                    (*s).self_test_peak_bytes = self.peak;
                    (*s).release_trace_enabled = self.trace.0;
                    (*s).release_trace_len = self.trace.1;
                    let shared = self::shared();
                    (*shared).failure_present = self.shared.0;
                    (*shared).failure_value = self.shared.1;
                    (*shared).failure_cursor = self.shared.2;
                }
            }
        }
        let _restore = Restore {
            failure: saved_failure,
            active: saved_active,
            peak: saved_peak,
            trace: saved_trace,
            shared: saved_shared,
        };
        (*s).report = t::EMPTY_REPORT;
        fail_after(None);
        shared_raster::r4os_rust_program_shared_failure(0, false, true);
        if !warm() {
            (*s).report.failed_case = 1;
            return false;
        }
        let heap = heap_stats();
        let base = stats();
        (*s).self_test_active = true;
        (*s).self_test_peak_bytes = base.current_payload_bytes;
        (*s).release_trace_enabled = false;
        (*s).release_trace_len = 0;
        let mut case = 0;
        for (class, environment, stages) in [
            (2, false, 2),
            (2, true, 3),
            (0, false, 4),
            (0, true, 5),
            (1, false, 3),
            (1, true, 4),
        ] {
            for failure in 0..stages {
                case += 1;
                fail_after(Some(failure));
                if let Some(mut storage) = storage::allocate(0xffff_0000 + case, class, environment)
                {
                    fail_after(None);
                    storage::rollback(0xffff_0000 + case, &mut storage);
                    return failed(case, &heap, &base);
                }
                fail_after(None);
                if !baseline(&heap, &base) {
                    return failed(case, &heap, &base);
                }
            }
        }
        macro_rules! check {
            ($value:expr) => {{
                case += 1;
                if !$value {
                    return failed(case, &heap, &base);
                }
            }};
        }
        check!(lazy(0xfffe_0001, 0, 0, &heap, &base));
        check!(lazy(0xfffe_0002, 1, 0, &heap, &base));
        check!(lazy(0xfffe_0003, 2, 0, &heap, &base));
        check!(lazy(0xfffe_0004, 3, 0, &heap, &base));
        check!(lazy(0xfffe_0005, 3, 1, &heap, &base));
        check!(zero_initialization(0xfffe_0006, &heap, &base));
        check!(raster_growth(0xfffe_0007, &heap, &base));
        check!(raster_growth_failure(0xfffe_0008, &heap, &base));
        check!(dynamic_commands(0xfffe_0009, &heap, &base));
        check!(mixed_resources(0xfffe_000a, &heap, &base));
        check!(atomic_growth_failure(0xfffe_000b, &heap, &base));
        check!(retired_reader(0xfffe_000c, &heap, &base));
        check!(delta_indexed(0xfffe_000d, &heap, &base));
        check!(replacement_xrgb(0xfffe_000e, &heap, &base));
        check!(shared_lifecycle(&heap, &base));
        check!(stack_headroom());
        (*s).report = t::SelfTestReport {
            cases: case,
            failed_case: 0,
            peak_payload_bytes: (*s).self_test_peak_bytes,
            heap_baseline_ok: true,
            storage_baseline_ok: true,
            zero_init_ok: true,
        };
        log(b"[PINSTSTOR] result=OK cases=");
        dec(case.into());
        log(b" heap_baseline=ok storage_baseline=ok zero_init=ok\r\n");
        true
    }
}
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct BufferStats {
    objects: u64,
    references: u64,
    leases: u64,
    bytes: u64,
    retained_bytes: u64,
    system_bytes: u64,
    device_bytes: u64,
    system_backed_bytes: u64,
    device_backed_bytes: u64,
    system_pinned_bytes: u64,
    device_pinned_bytes: u64,
    scanout_pinned_bytes: u64,
    device_mapped_bytes: u64,
    allocating_bytes: u64,
    destroying_bytes: u64,
}

const _: () = {
    assert!(mem::size_of::<BufferStats>() == 120);
    assert!(mem::offset_of!(BufferStats, objects) == 0);
    assert!(mem::offset_of!(BufferStats, references) == 8);
    assert!(mem::offset_of!(BufferStats, leases) == 16);
    assert!(mem::offset_of!(BufferStats, bytes) == 24);
    assert!(mem::offset_of!(BufferStats, retained_bytes) == 32);
    assert!(mem::offset_of!(BufferStats, system_bytes) == 40);
    assert!(mem::offset_of!(BufferStats, device_bytes) == 48);
    assert!(mem::offset_of!(BufferStats, system_backed_bytes) == 56);
    assert!(mem::offset_of!(BufferStats, device_backed_bytes) == 64);
    assert!(mem::offset_of!(BufferStats, system_pinned_bytes) == 72);
    assert!(mem::offset_of!(BufferStats, device_pinned_bytes) == 80);
    assert!(mem::offset_of!(BufferStats, scanout_pinned_bytes) == 88);
    assert!(mem::offset_of!(BufferStats, device_mapped_bytes) == 96);
    assert!(mem::offset_of!(BufferStats, allocating_bytes) == 104);
    assert!(mem::offset_of!(BufferStats, destroying_bytes) == 112);
};

unsafe fn program_test_buffer_stats(out: *mut BufferStats) {
    unsafe {
        crate::gfx::buffers::lock();
        let value = crate::gfx::buffers::read().stats();
        crate::gfx::buffers::unlock();
        out.write(BufferStats {
            objects: value.objects as u64,
            references: value.references as u64,
            leases: value.leases as u64,
            bytes: value.bytes,
            retained_bytes: value.retained_bytes,
            system_bytes: value.system_bytes,
            device_bytes: value.device_bytes,
            system_backed_bytes: value.system_backed_bytes,
            device_backed_bytes: value.device_backed_bytes,
            system_pinned_bytes: value.system_pinned_bytes,
            device_pinned_bytes: value.device_pinned_bytes,
            scanout_pinned_bytes: value.scanout_pinned_bytes,
            device_mapped_bytes: value.device_mapped_bytes,
            allocating_bytes: value.allocating_bytes,
            destroying_bytes: value.destroying_bytes,
        });
    }
}

// Private operation modules share the sole state and original typed owners.
mod command_rasters;
mod resource_cases;
mod shared_cases;
mod storage_cases;
use command_rasters::{
    append_rasters, at, batch, bytes_of, dynamic_commands, external, hash, legacy, raster_growth,
    raster_growth_failure, retired_reader,
};
use resource_cases::{delta_indexed, mixed_resources, replacement_xrgb};
use shared_cases::shared_lifecycle;
use storage_cases::{atomic_growth_failure, failed, lazy, stack_headroom, zero_initialization};
