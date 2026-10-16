//! Original frame TX admission, backend callback, link/error cleanup and
//! backpressure counters. Protocol accounting follows a successful send;
//! callback pins span the work, while CPU metadata admission never does.
use super::{adapters, backend_admission::CallbackGuard};
use crate::{memory::owner::Guard, sched::sync_types::Text};
use core::cell::UnsafeCell;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeTxCounters {
    pub failures: u64,
    pub no_adapter: u64,
    pub link_down: u64,
    pub busy: u64,
    pub too_large: u64,
    pub unsupported: u64,
    pub backend_error: u64,
    pub last_result: u8,
    pub has_result: bool,
}
impl NativeTxCounters {
    const EMPTY: Self = Self {
        failures: 0,
        no_adapter: 0,
        link_down: 0,
        busy: 0,
        too_large: 0,
        unsupported: 0,
        backend_error: 0,
        last_result: 0,
        has_result: false,
    };
}
struct Storage(UnsafeCell<NativeTxCounters>);
// SAFETY: every counter access holds the short common network metadata owner.
unsafe impl Sync for Storage {}
static COUNTERS: Storage = Storage(UnsafeCell::new(NativeTxCounters::EMPTY));
pub(super) fn result_name(result: u8) -> &'static [u8] {
    match result {
        0 => b"ok",
        1 => b"no-adapter",
        2 => b"link-down",
        3 => b"busy",
        4 => b"too-large",
        5 => b"unsupported",
        6 => b"backend-error",
        _ => panic!("Exact TX result"),
    }
}
pub(super) fn record(result: u8) -> u8 {
    assert!(result <= 6, "Exact TX result");
    let _guard = Guard::acquire(2);
    unsafe {
        let s = &mut *COUNTERS.0.get();
        s.last_result = result;
        s.has_result = true;
        if result != 0 {
            s.failures += 1;
            match result {
                1 => s.no_adapter += 1,
                2 => s.link_down += 1,
                3 => s.busy += 1,
                4 => s.too_large += 1,
                5 => s.unsupported += 1,
                6 => s.backend_error += 1,
                _ => unreachable!(),
            }
        }
    }
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tx_reset() {
    let _guard = Guard::acquire(2);
    unsafe {
        COUNTERS.0.get().write(NativeTxCounters::EMPTY);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tx_record(result: u8) {
    record(result);
}
/// # Safety
/// Writable exact paired output; no output write spans metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tx_counters(out: *mut NativeTxCounters) {
    let value = {
        let _guard = Guard::acquire(2);
        unsafe { *COUNTERS.0.get() }
    };
    unsafe {
        out.write(value);
    }
}
#[repr(C)]
pub struct TransmitPeers {
    pub sent: unsafe extern "C" fn(*const u8, usize),
    pub cleanup: unsafe extern "C" fn(*const u8, usize),
}
/// # Safety
/// Retained readable frame and immutable resident typed owner callbacks through
/// backend execution, protocol accounting and cleanup. No no-sleep owner spans
/// these calls. Adapter code/context lifetime is protected by callback admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_transmit(
    index: usize,
    data: *const u8,
    len: usize,
    peers: *const TransmitPeers,
) -> u8 {
    let Some(_callback) = CallbackGuard::acquire() else {
        return record(2);
    };
    let Some(adapter) = adapters::snapshot(index) else {
        return record(1);
    };
    if len == 0 || len > 1536 || len > usize::from(adapter.mtu) + 18 {
        unsafe {
            adapters::r4os_rust_net_adapter_drop(index, Text::literal(b"tx-size"));
        }
        return record(4);
    }
    if adapter.link == 1 {
        unsafe {
            adapters::r4os_rust_net_adapter_error(index, Text::literal(b"tx-link-down"));
        }
        adapters::r4os_rust_net_adapter_lifecycle(index, 3);
        return record(2);
    }
    let Some(callback) = adapter.ops.transmit else {
        unsafe {
            adapters::r4os_rust_net_adapter_error(index, Text::literal(b"tx-unsupported"));
        }
        return record(5);
    };
    let result = unsafe { callback(adapter.ops.transmit_context, index, data, len) };
    let peers = unsafe { &*peers };
    if result == 0 {
        record(0);
        adapters::r4os_rust_net_adapter_tx_ok(index, len);
        unsafe {
            (peers.sent)(data, len);
        }
    } else {
        if result == 2 {
            let was_up = adapters::snapshot(index).is_some_and(|e| e.link == 2);
            adapters::r4os_rust_net_adapter_link(index, 1);
            if was_up {
                unsafe {
                    (peers.cleanup)(b"link-down".as_ptr(), 9);
                }
            }
        }
        unsafe {
            adapters::r4os_rust_net_adapter_tx_error(index, Text::literal(result_name(result)));
        }
        record(result);
    }
    result
}

const _: () = {
    assert!(core::mem::size_of::<NativeTxCounters>() == 64);
    assert!(core::mem::align_of::<NativeTxCounters>() == 8);
    assert!(core::mem::offset_of!(NativeTxCounters, failures) == 0);
    assert!(core::mem::offset_of!(NativeTxCounters, no_adapter) == 8);
    assert!(core::mem::offset_of!(NativeTxCounters, link_down) == 16);
    assert!(core::mem::offset_of!(NativeTxCounters, busy) == 24);
    assert!(core::mem::offset_of!(NativeTxCounters, too_large) == 32);
    assert!(core::mem::offset_of!(NativeTxCounters, unsupported) == 40);
    assert!(core::mem::offset_of!(NativeTxCounters, backend_error) == 48);
    assert!(core::mem::offset_of!(NativeTxCounters, last_result) == 56);
    assert!(core::mem::offset_of!(NativeTxCounters, has_result) == 57);
};
const _: () = {
    assert!(core::mem::size_of::<TransmitPeers>() == 16);
    assert!(core::mem::align_of::<TransmitPeers>() == 8);
    assert!(core::mem::offset_of!(TransmitPeers, sent) == 0);
    assert!(core::mem::offset_of!(TransmitPeers, cleanup) == 8);
};
