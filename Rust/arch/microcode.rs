//! Mandatory initial local microcode mechanism. The only data source is the
//! pinned early preload; there is no runtime or public arbitrary load API.
use super::{
    microcode_boot as boot,
    microcode_catalog::{PINS, Pin},
    microcode_format::Patch,
    microcode_package, percpu, runtime, safe_msr,
    smp_policy::MAX_CPUS,
};
use crate::{
    boot::info,
    contract as c,
    platform::{
        cpu_capabilities_decode::{self as decode, Raw},
        cpu_capabilities_store::Store,
        cpu_topology, firmware_tables,
    },
};
use core::{
    cell::UnsafeCell,
    mem, ptr, slice,
    sync::atomic::{AtomicU32, Ordering},
};
const MISSING: u32 = 1;
const VALID: u32 = 2;
const REJECTED: u32 = 3;
static CACHE_STATE: AtomicU32 = AtomicU32::new(0);
#[repr(align(4096))]
struct Cache(UnsafeCell<[[u8; super::microcode_format::MAX_FILE]; 2]>);
// SAFETY: only the sole pre-SMP BSP writes. A release publication seals both
// files forever; APs acquire that receipt before constructing immutable views.
unsafe impl Sync for Cache {}
static CACHE: Cache = Cache(UnsafeCell::new([[0; super::microcode_format::MAX_FILE]; 2]));
#[repr(align(4096))]
struct PatchBuffer(UnsafeCell<[u8; 32_768]>);
// SAFETY: the nonblocking initial-entry claim is held through exact revision
// readback. A failed/uncertain load retains it for the rest of this boot.
unsafe impl Sync for PatchBuffer {}
static PATCH: PatchBuffer = PatchBuffer(UnsafeCell::new([0; 32_768]));
static PATCH_HELD: AtomicU32 = AtomicU32::new(0);
#[derive(Clone, Copy)]
struct Staged {
    expected_epoch: u64,
    info: c::CpuMicrocodeInfo,
}
struct Stages(UnsafeCell<[Staged; MAX_CPUS]>);
// SAFETY: each IF=0 bootstrap CPU alone writes/reads its own stage. No stage
// reference crosses instructions/callbacks. Foreign/public readers use only
// the separately atomically published Store, including failed AP evidence.
unsafe impl Sync for Stages {}
static STAGED: Stages = Stages(UnsafeCell::new(
    [Staged {
        expected_epoch: 0,
        info: unsafe { mem::zeroed() },
    }; MAX_CPUS],
));
static RECORDS: [Store<18>; MAX_CPUS] = [const { Store::new() }; MAX_CPUS];
unsafe fn preload() -> Result<Option<&'static [u8]>, ()> {
    let snapshot = unsafe { &*info::r4os_rust_boot_info() };
    if snapshot.module_count > 16 {
        return Err(());
    }
    let modules = if snapshot.module_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(snapshot.modules, snapshot.module_count) }
    };
    let mut found = None;
    for module in modules {
        if !module.valid {
            continue;
        }
        let command = if module.cmdline_len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(module.cmdline, module.cmdline_len) }
        };
        if command == b"r4os.preload.image=PRELOAD.R4I" {
            if found.is_some() || module.size < 64 || module.size > 64 * 1024 * 1024 {
                return Err(());
            }
            found = Some(unsafe { slice::from_raw_parts(module.address, module.size) });
        }
    }
    Ok(found)
}
fn initialize_cache() {
    let state = match unsafe { preload() } {
        Ok(None) => MISSING,
        Ok(Some(bytes)) => match microcode_package::validate(bytes, firmware_tables::sha256) {
            Ok(None) => MISSING,
            Ok(Some(files)) => {
                let mut valid = true;
                for (index, file) in files.into_iter().enumerate() {
                    let destination = unsafe {
                        CACHE
                            .0
                            .get()
                            .cast::<u8>()
                            .add(index * super::microcode_format::MAX_FILE)
                    };
                    unsafe {
                        ptr::copy_nonoverlapping(
                            bytes.as_ptr().add(file.offset),
                            destination,
                            file.bytes,
                        );
                    }
                    // Check the resident copy too. No original Limine borrow
                    // survives boot memory reclamation or a logical restart.
                    let retained = unsafe { slice::from_raw_parts(destination, file.bytes) };
                    valid &= firmware_tables::sha256(retained) == PINS[index].sha256;
                }
                if valid { VALID } else { REJECTED }
            }
            Err(_) => REJECTED,
        },
        Err(()) => REJECTED,
    };
    CACHE_STATE.store(state, Ordering::Release);
}
fn files(state: u32) -> [&'static [u8]; 2] {
    core::array::from_fn(|index| {
        if state == VALID {
            unsafe {
                slice::from_raw_parts(
                    CACHE
                        .0
                        .get()
                        .cast::<u8>()
                        .add(index * super::microcode_format::MAX_FILE),
                    PINS[index].bytes,
                )
            }
        } else {
            &[]
        }
    })
}
struct Native {
    leased_target: Option<u32>,
}
impl boot::Hardware for Native {
    fn capture(&mut self) -> Raw {
        decode::capture(core::arch::x86_64::__cpuid_count)
    }
    fn revision(&mut self, vendor: u32) -> Result<u32, ()> {
        let result = if vendor == 1 {
            // Intel's documented revision query, not an update trigger.
            unsafe { safe_msr::write(0x8b, 0) }.map_err(|_| ())?;
            let _ = core::arch::x86_64::__cpuid_count(1, 0);
            unsafe { safe_msr::read(0x8b) }
                .map(|v| (v >> 32) as u32)
                .map_err(|_| ())
        } else if vendor == 2 {
            unsafe { safe_msr::read(0x8b) }
                .map(|v| v as u32)
                .map_err(|_| ())
        } else {
            Err(())
        };
        if let Ok(value) = result
            && self.leased_target == Some(value)
        {
            self.leased_target = None;
            PATCH_HELD.store(0, Ordering::Release);
        }
        result
    }
    fn platform(&mut self) -> Result<u32, ()> {
        unsafe { safe_msr::read(0x17) }
            .map(|v| 1u32 << ((v >> 50) & 7))
            .map_err(|_| ())
    }
    fn apply(&mut self, pin: Pin, patch: Patch, file: &[u8]) -> boot::LoadResult {
        // Only this internal initial-entry controller reaches these two exact
        // triggers. Offsets were checked against the complete pinned file.
        let register = match pin.vendor {
            1 => 0x79,
            2 => 0xc001_0020,
            _ => return boot::LoadResult::NotSubmitted,
        };
        if patch.bytes > 32_768
            || patch
                .offset
                .checked_add(patch.bytes)
                .is_none_or(|end| end > file.len())
            || PATCH_HELD
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return boot::LoadResult::NotSubmitted;
        }
        // Vendor sections inside an original container need not start at a
        //16-byte boundary. Copy the unchanged selected bytes to resident,
        // aligned owned storage instead of submitting an unaligned address.
        let address = PATCH.0.get().cast::<u8>();
        unsafe {
            ptr::copy_nonoverlapping(file.as_ptr().add(patch.offset), address, patch.bytes);
        }
        self.leased_target = Some(pin.revision);
        if pin.vendor == 1 {
            // Retained ordinary kernel BSS, with existing boot cache
            // mappings; flush per Intel's native early-load sequence.
            unsafe {
                core::arch::asm!("wbinvd", options(nostack, preserves_flags));
            }
        }
        // AMD's long-mode loader takes the complete linear address. Intel
        // receives its aligned encrypted-data address, after the48-byte header.
        match unsafe { safe_msr::write(register, address as u64) } {
            Ok(()) => boot::LoadResult::Written,
            Err(safe_msr::Error::Fault) => boot::LoadResult::Faulted,
            Err(safe_msr::Error::Busy) => {
                // The exact safe-instruction owner refused before WRMSR.
                // Only our fresh, unsubmitted patch claim can be returned.
                self.leased_target = None;
                PATCH_HELD.store(0, Ordering::Release);
                boot::LoadResult::NotSubmitted
            }
        }
    }
}
fn local(index: u32, expected_epoch: u64, initial: bool) -> bool {
    if index as usize >= MAX_CPUS || index != percpu::current_index() {
        return false;
    }
    let flags = runtime::save_local();
    if flags & (1 << 9) != 0 {
        runtime::restore_local(flags);
        return false;
    }
    let state = CACHE_STATE.load(Ordering::Acquire);
    if state == 0 {
        return false;
    }
    let bundle = match state {
        MISSING => None,
        VALID => Some(true),
        _ => Some(false),
    };
    let mut hardware = Native {
        leased_target: None,
    };
    let value = boot::prepare(index, initial, bundle, files(state), &mut hardware);
    unsafe {
        STAGED
            .0
            .get()
            .cast::<Staged>()
            .add(index as usize)
            .write(Staged {
                expected_epoch,
                info: value,
            });
    }
    let ok = !boot::failed(&value);
    if !ok && expected_epoch != 0 {
        let _ = publish(index, expected_epoch, value);
    }
    ok
}
pub(crate) fn prepare_bsp() -> bool {
    if CACHE_STATE.load(Ordering::Acquire) != 0 {
        return false;
    }
    initialize_cache();
    local(0, 0, true)
}
pub(crate) fn prepare_ap(index: u32, epoch: u64) -> bool {
    if index == 0 || epoch == 0 || cpu_topology::epoch(index) != epoch {
        return false;
    }
    local(index, epoch, epoch == 1)
}
fn publish(index: u32, epoch: u64, info: c::CpuMicrocodeInfo) -> bool {
    // All144 bytes are initialized integers/arrays with a checked fixed ABI.
    let words: [u64; 18] = unsafe { mem::transmute(info) };
    RECORDS[index as usize].publish(epoch, words).is_ok()
}
/// Exact local post-PAT/FPU/module-baseline capture. Diagnostic hashes never
/// substitute for those owners' actual decisions or admit a CPU themselves.
pub(crate) fn finalize(index: u32, epoch: u64, raw: &Raw) -> bool {
    if index as usize >= MAX_CPUS
        || index != percpu::current_index()
        || epoch == 0
        || cpu_topology::epoch(index) != epoch
    {
        return false;
    }
    let mut stage = unsafe { STAGED.0.get().cast::<Staged>().add(index as usize).read() };
    if stage.info.version != 1
        || stage.info.size != 144
        || !(stage.expected_epoch == epoch || index == 0 && stage.expected_epoch == 0 && epoch == 1)
        || !boot::admit(&mut stage.info, raw)
    {
        return false;
    }
    publish(index, epoch, stage.info)
}
pub(crate) fn snapshot(index: u32) -> Result<c::CpuMicrocodeInfo, i32> {
    if index >= cpu_topology::count() {
        return Err(c::cpu_capabilities_error_index);
    }
    let record = RECORDS[index as usize]
        .snapshot()
        .ok_or(c::cpu_capabilities_error_busy)?;
    let mut info: c::CpuMicrocodeInfo = unsafe { mem::transmute(record.words) };
    info.epoch = record.epoch;
    info.sequence = record.sequence;
    info.configured_cpus = cpu_topology::count();
    Ok(info)
}
const _: () = {
    assert!(mem::size_of::<c::CpuMicrocodeInfo>() == 144);
    assert!(mem::align_of::<c::CpuMicrocodeInfo>() == 8);
};
