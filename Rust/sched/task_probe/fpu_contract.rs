//! Nonfaulting storage/metadata acceptance; never corrupts a task or raises an
//! exception. Existing full activated FP execution tests follow separately.
use super::helpers::*;
use crate::arch::{fpu, fpu_fault as fault, runtime};
use core::{
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
struct Buffers(UnsafeCell<[fpu::Image; 2]>);
// Boot-only single controller, protected by the explicit nonblocking gate.
unsafe impl Sync for Buffers {}
static BUFFERS: Buffers = Buffers(UnsafeCell::new([const { fpu::Image::filled(0xa5) }; 2]));
static BUSY: AtomicBool = AtomicBool::new(false);
unsafe fn buffer(index: usize) -> *mut u8 {
    unsafe { BUFFERS.0.get().cast::<fpu::Image>().add(index).cast() }
}
unsafe fn digest(p: *const u8, len: usize) -> u64 {
    let mut n = 0xcbf29ce484222325u64;
    for i in 0..len {
        n = (n ^ u64::from(unsafe { p.add(i).read() })).wrapping_mul(0x100000001b3);
    }
    n
}
unsafe fn storage() -> bool {
    unsafe {
        let Some(d) = fpu::descriptor() else {
            return false;
        };
        let flags = runtime::save_local();
        let initial = buffer(0);
        let saved = buffer(1);
        let initialized = fpu::r4os_rust_fpu_buffer(0, initial, fpu::STORAGE_BYTES)
            && fpu::r4os_rust_fpu_buffer(0, saved, fpu::STORAGE_BYTES);
        if !initialized {
            runtime::restore_local(flags);
            return false;
        }
        let original = fpu::r4os_rust_fpu_buffer(1, saved, fpu::STORAGE_BYTES);
        if !original {
            runtime::restore_local(flags);
            return false;
        }
        let before = digest(initial, d.contract.bytes as usize);
        let mut rejected = 0;
        for op in 0..3 {
            rejected += u32::from(!fpu::r4os_rust_fpu_buffer(op, ptr::null_mut(), 0));
            rejected += u32::from(!fpu::r4os_rust_fpu_buffer(
                op,
                initial.add(1),
                fpu::STORAGE_BYTES - 1,
            ));
            rejected += u32::from(!fpu::r4os_rust_fpu_buffer(
                op,
                initial,
                d.contract.bytes as usize - 1,
            ));
        }
        let unchanged = before == digest(initial, d.contract.bytes as usize);
        let canonical = fpu::r4os_rust_fpu_buffer(2, initial, fpu::STORAGE_BYTES);
        // Once original is saved, return only after the exact original restoration.
        assert!(
            fpu::r4os_rust_fpu_buffer(2, saved, fpu::STORAGE_BYTES),
            "FP diagnostic original restore rejected"
        );
        let mut canary = true;
        for index in 0..2 {
            for offset in d.contract.bytes as usize..fpu::STORAGE_BYTES {
                canary &= buffer(index).add(offset).read() == 0xa5;
            }
        }
        runtime::restore_local(flags);
        rejected == 9 && unchanged && canonical && canary
    }
}
pub(super) fn run() -> bool {
    unsafe {
        if crate::sched::kernel_check::live()
            || BUSY
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
        {
            return fail(b"xstate-contract-boot-only");
        }
        let identity = view(current());
        let before = fault::diagnostic();
        let stored = storage();
        let after = fault::diagnostic();
        let Some(d) = fpu::descriptor() else {
            BUSY.store(false, Ordering::Release);
            return fail(b"xstate-descriptor");
        };
        let snapshots = match (before, after) {
            (Some(a), Some(b)) => {
                a.environment.is_some()
                    && a.environment == b.environment
                    && a.cpu == b.cpu
                    && a.cpu.is_some_and(|(i, _)| i == 0)
                    && a.task == Some((identity.id, identity.generation))
                    && a.task == b.task
                    && a.backend == d.contract.backend
                    && a.mask == d.contract.mask
                    && a.bytes == d.contract.bytes
                    && a.unavailable == 0
                    && b.unavailable == 0
            }
            _ => false,
        };
        let terminal = fault::fatal_records();
        let no_terminal = terminal == 0 && fault::retained(0).is_none();
        BUSY.store(false, Ordering::Release);
        if !stored || !snapshots || !no_terminal {
            return fail(b"xstate-storage-or-live-snapshot");
        }
        puts(b"[XSTATE47] contract backend=");
        dec(d.contract.backend.into());
        puts(b" layout=");
        puts(if d.contract.standard() {
            b"standard"
        } else {
            b"legacy"
        });
        puts(b" mask=");
        dec(d.contract.mask);
        puts(b" active_bytes=");
        dec(d.contract.bytes.into());
        puts(b" alignment=");
        dec(fpu::STORAGE_ALIGN as u64);
        puts(b" capacity=");
        dec(fpu::STORAGE_BYTES as u64);
        puts(b" rejections=9 canary=1 snapshot=live native=");
        dec(identity.id.into());
        puts(b" generation=");
        dec(identity.generation);
        puts(b" terminal_records=0 fault_injection=none result=PASS\r\n");
        super::fpu_environment::run()
    }
}
