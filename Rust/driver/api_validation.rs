//! Original descriptor checks and optional prefixes. Short v1 records never
//! become references to the full later descriptor or read its trailing fields.
use crate::{boot::log, contract as a};
use core::{
    mem::{offset_of, size_of},
    ptr,
};
fn error(message: &[u8]) -> bool {
    unsafe { log::r4os_rust_bootlog_write(message.as_ptr(), message.len()) };
    false
}
macro_rules! field {
    ($d:expr,$f:ident) => {
        unsafe { ptr::read(ptr::addr_of!((*$d).$f)) }
    };
}
/// # Safety
/// The resident descriptor has its declared C prefix and retains all optional
/// callback addresses through its original driver/backend lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_submit(
    d: *const a::StorageBackend,
) -> Option<a::StorageSubmitFn> {
    if field!(d, version) < 2
        || field!(d, size) < (offset_of!(a::StorageBackend, submit) + 8) as u32
    {
        None
    } else {
        field!(d, submit)
    }
}
/// # Safety
/// The resident descriptor has its declared C prefix under driver admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_cancel(
    d: *const a::StorageBackend,
) -> Option<a::StorageCancelFn> {
    if field!(d, version) < 2
        || field!(d, size) < (offset_of!(a::StorageBackend, cancel) + 8) as u32
    {
        None
    } else {
        field!(d, cancel)
    }
}
/// # Safety
/// The resident descriptor has its declared C prefix under driver admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_reset(
    d: *const a::StorageBackend,
) -> Option<a::StorageResetFn> {
    if field!(d, version) < 2 || field!(d, size) < (offset_of!(a::StorageBackend, reset) + 8) as u32
    {
        None
    } else {
        field!(d, reset)
    }
}
/// # Safety
/// The resident descriptor has its declared C prefix under driver admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_valid(d: *const a::StorageBackend) -> bool {
    let version = field!(d, version);
    if version == 0 || version > a::storage_backend_version {
        return error(b"[R4D][ERROR] storage backend version mismatch\r\n");
    }
    if field!(d, size) < offset_of!(a::StorageBackend, submit) as u32 {
        return error(b"[R4D][ERROR] storage backend descriptor too small\r\n");
    }
    let submit = unsafe { r4os_rust_driver_storage_submit(d) };
    if submit.is_none() && field!(d, read).is_none() {
        return error(b"[R4D][ERROR] storage backend missing read callback\r\n");
    }
    if submit.is_none()
        && field!(d, flags) & a::storage_backend_flag_writable != 0
        && field!(d, write).is_none()
    {
        return error(b"[R4D][ERROR] storage backend writable without write callback\r\n");
    }
    if submit.is_none()
        && (unsafe { r4os_rust_driver_storage_cancel(d) }.is_some()
            || unsafe { r4os_rust_driver_storage_reset(d) }.is_some())
    {
        return error(b"[R4D][ERROR] storage backend async controls without submit\r\n");
    }
    if field!(d, sector_count) == 0 {
        return error(b"[R4D][ERROR] storage backend empty device\r\n");
    }
    if !matches!(field!(d, sector_size), 512 | 1024 | 2048 | 4096) {
        return error(b"[R4D][ERROR] storage backend invalid sector size\r\n");
    }
    let depth = field!(d, queue_depth);
    if depth == 0 {
        return error(b"[R4D][ERROR] storage backend invalid queue depth\r\n");
    }
    if depth > 16 {
        return error(b"[R4D][ERROR] storage backend queue depth exceeds block queue\r\n");
    }
    if field!(d, source) > 2 {
        return error(b"[R4D][ERROR] storage backend invalid source\r\n");
    }
    true
}
pub type RenderPcm =
    unsafe extern "C" fn(*mut core::ffi::c_void, *mut u8, u32, u32, u16, u16) -> i32;
/// # Safety
/// The resident descriptor has its declared C prefix under driver admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_synth_pcm(d: *const a::SynthEngine) -> Option<RenderPcm> {
    if field!(d, size) < (offset_of!(a::SynthEngine, render_pcm) + 8) as u32 {
        None
    } else {
        field!(d, render_pcm)
    }
}
/// # Safety
/// The resident descriptor has its declared C prefix under driver admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_synth_valid(d: *const a::SynthEngine) -> bool {
    if field!(d, version) != a::synth_engine_version {
        return error(b"[R4D][ERROR] synth engine version mismatch\r\n");
    }
    if field!(d, size) < offset_of!(a::SynthEngine, render_pcm) as u32 {
        return error(b"[R4D][ERROR] synth engine descriptor too small\r\n");
    }
    if field!(d, midi_send).is_none()
        && field!(d, render).is_none()
        && field!(d, stop).is_none()
        && field!(d, status).is_none()
        && field!(d, opl3_reset).is_none()
        && field!(d, opl3_write_register).is_none()
        && field!(d, sid_acquire).is_none()
        && field!(d, sid_release).is_none()
        && field!(d, sid_set_model).is_none()
        && field!(d, sid_write_register).is_none()
        && field!(d, sid_load_data).is_none()
        && field!(d, sid_init).is_none()
        && field!(d, sid_play_frame).is_none()
        && field!(d, sid_render_pcm).is_none()
        && unsafe { r4os_rust_driver_synth_pcm(d) }.is_none()
    {
        return error(b"[R4D][ERROR] synth engine has no operations\r\n");
    }
    true
}
/// # Safety
/// The resident descriptor has its declared C prefix under driver admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_usb_valid(d: *const a::UsbHostController) -> bool {
    if field!(d, version) != a::usb_host_backend_version {
        return error(b"[R4D][ERROR] usb host version mismatch\r\n");
    }
    if field!(d, size) < size_of::<a::UsbHostController>() as u32 {
        return error(b"[R4D][ERROR] usb host descriptor too small\r\n");
    }
    let value = unsafe { ptr::read(d) };
    if value.source > 2 {
        return error(b"[R4D][ERROR] usb host invalid source\r\n");
    }
    if value.port_scan.is_none() {
        return error(b"[R4D][ERROR] usb host missing port_scan\r\n");
    }
    if value.control_transfer.is_none() {
        return error(b"[R4D][ERROR] usb host missing control_transfer\r\n");
    }
    let mandatory = a::usb_host_flag_port_scan
        | a::usb_host_flag_control
        | a::usb_host_flag_bulk
        | a::usb_host_flag_interrupt
        | a::usb_host_flag_poll_fallback;
    let known = mandatory
        | a::usb_host_flag_event_irq
        | a::usb_host_flag_multi_transfer
        | a::usb_host_flag_hotplug;
    if value.flags & !known != 0 {
        return error(b"[R4D][ERROR] usb host unknown capability flags\r\n");
    }
    if value.flags & mandatory != mandatory
        || value.address_device.is_none()
        || value.configure_device.is_none()
        || value.bulk_transfer.is_none()
        || value.interrupt_transfer.is_none()
        || value.reset_port.is_none()
        || value.clear_halt.is_none()
        || value.reset_endpoint.is_none()
        || value.poll.is_none()
        || value.shutdown.is_none()
        || value.status.is_none()
    {
        return error(b"[R4D][ERROR] usb host v2 capability/callback mismatch\r\n");
    }
    true
}
pub type TransmitPacket =
    unsafe extern "C" fn(*mut core::ffi::c_void, *const a::NetTxRequest) -> i32;
/// # Safety
/// The resident descriptor has its declared C prefix under driver admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_packet(
    d: *const a::NetBackend,
) -> Option<TransmitPacket> {
    if field!(d, version) < 2 || field!(d, size) < size_of::<a::NetBackend>() as u32 {
        None
    } else {
        field!(d, transmit_packet)
    }
}
/// # Safety
/// The resident descriptor has its declared C prefix under driver admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_net_valid(d: *const a::NetBackend) -> bool {
    let version = field!(d, version);
    if version != 1 && version != a::net_backend_version {
        return error(b"[R4D][ERROR] net backend version mismatch\r\n");
    }
    let required = if version == 1 {
        offset_of!(a::NetBackend, offered_capabilities)
    } else {
        size_of::<a::NetBackend>()
    };
    if field!(d, size) < required as u32 {
        return error(b"[R4D][ERROR] net backend descriptor too small\r\n");
    }
    if !(576..=1500).contains(&field!(d, mtu)) {
        return error(b"[R4D][ERROR] net backend invalid mtu\r\n");
    }
    if field!(d, mac) == [0; 6] {
        return error(b"[R4D][ERROR] net backend missing mac\r\n");
    }
    let mut valid = true;
    if let Some(f) = field!(d, transmit) {
        valid &= f as *const () as u64 >= 0xffff800000000000;
    }
    if let Some(f) = field!(d, poll) {
        valid &= f as *const () as u64 >= 0xffff800000000000;
    }
    if let Some(f) = field!(d, status) {
        valid &= f as *const () as u64 >= 0xffff800000000000;
    }
    if let Some(f) = field!(d, shutdown) {
        valid &= f as *const () as u64 >= 0xffff800000000000;
    }
    if let Some(f) = unsafe { r4os_rust_driver_net_packet(d) } {
        valid &= f as *const () as u64 >= 0xffff800000000000;
    }
    if !valid {
        return error(b"[R4D][ERROR] NETBACKEND BAD FNPTR (unrelocated?)\r\n");
    }
    true
}
