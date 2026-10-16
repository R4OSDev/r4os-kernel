//! Resident device/request leaf storage at the single native block registry.
//! Admission, generations, pins, backend callbacks and retirement retain the
//! original owners; no Device reference spans a callback or a wait.
use super::{
    access_state::Reference,
    block_accounting as accounting, block_device_state, block_diagnostics, block_execution,
    block_io,
    block_registry::{self, Registration, Text},
    block_requests, block_state, block_workers,
};
use crate::sched::sync_types::{Mutex, Text as SyncText, WaitQueue};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, ptr};
pub type Read = unsafe extern "C" fn(*mut c_void, u64, u16, *mut u8, usize) -> bool;
pub type Write = unsafe extern "C" fn(*mut c_void, u64, u16, *const u8, usize) -> bool;
pub type Flush = unsafe extern "C" fn(*mut c_void) -> bool;
pub type Submit = unsafe extern "C" fn(*mut c_void, *const AsyncRequest) -> i32;
pub type Cancel = unsafe extern "C" fn(*mut c_void, u64, u32) -> i32;
pub type Reset = unsafe extern "C" fn(*mut c_void, u32) -> i32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AsyncRequest {
    pub handle: u64,
    pub kind: u8,
    pub lba: u64,
    pub sectors: u16,
    pub buffer: *mut u8,
    pub const_buffer: *const u8,
    pub buffer_len: usize,
    pub complete: unsafe extern "C" fn(u64, i32, u32),
}
#[derive(Clone, Copy)]
pub struct Config {
    pub name: Text,
    pub model: Text,
    pub driver: Text,
    pub controller: Text,
    pub bus: u8,
    pub port: u8,
    pub sector_size: u32,
    pub sector_count: u64,
    pub max_sectors: u16,
    pub queue_depth: u16,
    pub timeout: u64,
    pub removable: bool,
    pub writable: bool,
    pub owns_retry: bool,
    pub source: u8,
    pub owner: u32,
    pub context: *mut c_void,
    pub read: Read,
    pub write: Option<Write>,
    pub flush: Option<Flush>,
    pub submit: Option<Submit>,
    pub cancel: Option<Cancel>,
    pub reset: Option<Reset>,
}
impl Config {
    pub fn new(name: Text, read: Read) -> Self {
        Self {
            name,
            model: text(b""),
            driver: text(b"unknown"),
            controller: text(b"unknown"),
            bus: 0,
            port: 0,
            sector_size: 0,
            sector_count: 0,
            max_sectors: 0,
            queue_depth: 1,
            timeout: 0,
            removable: false,
            writable: false,
            owns_retry: false,
            source: 0,
            owner: 0,
            context: ptr::null_mut(),
            read,
            write: None,
            flush: None,
            submit: None,
            cancel: None,
            reset: None,
        }
    }
}
pub const fn text(value: &'static [u8]) -> Text {
    Text {
        data: value.as_ptr(),
        len: value.len(),
    }
}
pub(crate) fn config(index: usize) -> Option<Config> {
    let _guard = crate::memory::owner::Guard::acquire(1);
    let mut v = MaybeUninit::uninit();
    if !unsafe { block_registry::r4os_rust_block_registry_slot(index, 1, 0, true, v.as_mut_ptr()) }
    {
        return None;
    }
    let v = unsafe { v.assume_init() };
    Some(unsafe { ptr::addr_of!((*v.device.cast::<Device>()).config).read() })
}
#[repr(C)]
pub struct Device {
    pub config: Config,
    pub reference: Reference,
    pub state: u8,
    pub resetting: bool,
    pub index: u8,
    pub lane: u8,
    pub stats: accounting::Stats,
    pub active: u32,
    pub slots: [block_requests::Slot; 16],
    pub slot_available: WaitQueue,
    pub completion_available: WaitQueue,
    pub mutex: Mutex,
}
struct Storage([UnsafeCell<MaybeUninit<Device>>; 16]);
// SAFETY: boot installs stationary addresses; original registry pin/retirement,
// queue mutex and short storage admission own every subsequent field access.
unsafe impl Sync for Storage {}
static DEVICES: Storage = Storage([const { UnsafeCell::new(MaybeUninit::uninit()) }; 16]);
pub(crate) unsafe fn init() {
    unsafe { super::access_runtime::r4os_rust_storage_access_init() };
    let addresses: [*mut c_void; 16] = core::array::from_fn(|i| DEVICES.0[i].get().cast());
    unsafe {
        block_registry::r4os_rust_block_registry_init(addresses.as_ptr());
    }
    unsafe {
        block_workers::r4os_rust_block_workers_init();
        accounting::r4os_rust_block_runtime_reset();
    }
}
unsafe extern "C" fn initialize(
    out: *mut c_void,
    input: *const c_void,
    reference: *const Reference,
    lane: u8,
) {
    let out = out.cast::<Device>();
    let config = unsafe { input.cast::<Config>().read() };
    let reference = unsafe { reference.read() };
    // Initialize fields in resident storage; never materialize the request pool
    // or a Device aggregate on the shared task stack.
    unsafe {
        ptr::addr_of_mut!((*out).config).write(config);
        ptr::addr_of_mut!((*out).reference).write(reference);
        ptr::addr_of_mut!((*out).state).write(0);
        ptr::addr_of_mut!((*out).resetting).write(false);
        ptr::addr_of_mut!((*out).active).write(0);
        ptr::addr_of_mut!((*out).index).write(u8::try_from(reference.slot).expect("block slot"));
        ptr::addr_of_mut!((*out).lane).write(lane);
        ptr::write_bytes(ptr::addr_of_mut!((*out).stats), 0, 1);
        let view = execution_view(out.cast());
        block_execution::r4os_rust_block_sync_init(&view);
    }
}
/// Config text and callback context retain the original backend lifetime until
/// confirmed unregister. Registration and worker startup retain their owners.
pub(crate) unsafe fn register(config: Config) -> Option<usize> {
    let info = Registration {
        name: config.name,
        model: config.model,
        driver: config.driver,
        controller: config.controller,
        sectors: config.sector_count,
        sector_bytes: config.sector_size,
        queue_depth: config.queue_depth,
        max_sectors: config.max_sectors,
        bus: config.bus,
        writable: config.writable,
        owns_retry: config.owns_retry,
        async_submit: config.submit.is_some(),
    };
    let mut slot = MaybeUninit::uninit();
    if unsafe {
        block_execution::r4os_rust_block_register(
            &info,
            ptr::from_ref(&config).cast(),
            initialize,
            slot.as_mut_ptr(),
        )
    } {
        Some(unsafe { slot.assume_init() }.index)
    } else {
        None
    }
}
pub(crate) unsafe fn io_view(raw: *mut c_void) -> block_io::View {
    let p = raw.cast::<Device>();
    unsafe {
        block_io::View {
            reference: (*p).reference,
            sector_size: (*p).config.sector_size,
            sector_count: (*p).config.sector_count,
            max_sectors: (*p).config.max_sectors,
            queue_depth: (*p).config.queue_depth,
            timeout: (*p).config.timeout,
            writable: (*p).config.writable,
            has_writer: (*p).config.write.is_some(),
            has_submit: (*p).config.submit.is_some(),
            has_flush: (*p).config.flush.is_some(),
            stats: ptr::addr_of_mut!((*p).stats),
        }
    }
}
pub(crate) unsafe fn execution_view(raw: *mut c_void) -> block_execution::View {
    let p = raw.cast::<Device>();
    unsafe {
        block_execution::View {
            io: io_view(raw),
            state: block_device_state::DeviceState {
                state: ptr::addr_of_mut!((*p).state),
                resetting: ptr::addr_of_mut!((*p).resetting),
                active: ptr::addr_of_mut!((*p).active),
                stats: ptr::addr_of_mut!((*p).stats),
            },
            slots: ptr::addr_of_mut!((*p).slots).cast(),
            mutex: ptr::addr_of_mut!((*p).mutex),
            slot_available: ptr::addr_of_mut!((*p).slot_available),
            completion_available: ptr::addr_of_mut!((*p).completion_available),
            name: SyncText {
                pointer: (*p).config.name.data,
                len: (*p).config.name.len,
            },
            driver: SyncText {
                pointer: (*p).config.driver.data,
                len: (*p).config.driver.len,
            },
            index: (*p).index,
            lane: (*p).lane,
            has_cancel: (*p).config.cancel.is_some(),
            has_reset: (*p).config.reset.is_some(),
            bus: (*p).config.bus,
            queue_depth: ptr::addr_of_mut!((*p).config.queue_depth),
        }
    }
}
/// Registry pin and exact execution record remain admitted until completion.
/// All callback fields/context are copied before invoking the backend.
pub(crate) unsafe fn backend(
    raw: *mut c_void,
    request: *const block_state::Execution,
    operation: u8,
    reason: u32,
) -> i32 {
    let config = unsafe { ptr::addr_of!((*raw.cast::<Device>()).config).read() };
    unsafe {
        match operation {
            1 => {
                let r = request.read();
                i32::from((config.read)(
                    config.context,
                    r.lba,
                    r.sectors,
                    r.buffer,
                    r.buffer_len,
                ))
            }
            2 => {
                let r = request.read();
                i32::from(config.write.expect("block writer")(
                    config.context,
                    r.lba,
                    r.sectors,
                    r.const_buffer,
                    r.buffer_len,
                ))
            }
            3 => i32::from(config.flush.expect("block flush")(config.context)),
            4 => {
                let r = request.read();
                let input = AsyncRequest {
                    handle: r.backend_handle,
                    kind: r.kind,
                    lba: r.lba,
                    sectors: r.sectors,
                    buffer: r.buffer,
                    const_buffer: r.const_buffer,
                    buffer_len: r.buffer_len,
                    complete: block_execution::r4os_rust_block_complete,
                };
                config.submit.expect("block submit")(config.context, &input)
            }
            5 => config.cancel.expect("block cancel")(
                config.context,
                (*request).backend_handle,
                reason,
            ),
            6 => config.reset.expect("block reset")(config.context, reason),
            _ => unreachable!(),
        }
    }
}
fn label_copy(out: &mut [u8; 32], text: Text) {
    out.fill(0);
    let n = text.len.min(31);
    if n != 0 {
        unsafe { ptr::copy_nonoverlapping(text.data, out.as_mut_ptr(), n) }
    }
}
/// Called only under the existing short storage owner with a visible slot;
/// copied labels and stats never lend Device storage to passive callers.
pub(crate) unsafe fn diagnostic(raw: *mut c_void, out: *mut block_diagnostics::DeviceSnapshot) {
    let p = raw.cast::<Device>();
    let config = unsafe { ptr::addr_of!((*p).config).read() };
    let mut stats = accounting::Stats::default();
    unsafe { accounting::r4os_rust_block_stats_snapshot(ptr::addr_of!((*p).stats), &mut stats) };
    let mut snapshot = block_diagnostics::DeviceSnapshot {
        bus: config.bus,
        state: unsafe { (*p).state },
        source: config.source,
        removable: config.removable,
        writable: config.writable,
        port: config.port.into(),
        sector_size: config.sector_size,
        max_sectors: config.max_sectors.into(),
        queue_depth: config.queue_depth.into(),
        queue_used: unsafe {
            block_requests::r4os_rust_block_select(ptr::addr_of_mut!((*p).slots).cast(), 3, 0)
        } as u32,
        timeout_ticks: config.timeout,
        sector_count: config.sector_count,
        name: [0; 32],
        driver: [0; 32],
        controller: [0; 32],
        stats,
    };
    label_copy(&mut snapshot.name, config.name);
    label_copy(&mut snapshot.driver, config.driver);
    label_copy(&mut snapshot.controller, config.controller);
    unsafe { out.write(snapshot) };
}
