//! Original linear BSP module/filesystem bootstrap. Peers retain their own
//! implementations; only this owner publishes loader readiness and config.
use core::{
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

#[repr(C)]
pub struct ModulePeers {
    pub registry: unsafe extern "C" fn(),
    pub log: unsafe extern "C" fn(*const u8, usize),
}

#[repr(C)]
pub struct Fonts {
    pub registered: u32,
    pub rejected: u32,
}

#[repr(C)]
pub struct LoaderPeers {
    pub controllers: unsafe extern "C" fn() -> bool,
    pub fail: unsafe extern "C" fn(*const u8, usize) -> bool,
    pub libraries: unsafe extern "C" fn(),
    pub drivers: unsafe extern "C" fn(),
    pub summary: unsafe extern "C" fn(),
    pub config: unsafe extern "C" fn() -> *const c_void,
    pub fonts: unsafe extern "C" fn() -> Fonts,
    pub log: unsafe extern "C" fn(*const u8, usize),
    pub decimal: unsafe extern "C" fn(u64),
    pub status: unsafe extern "C" fn(*const u8, usize),
}

// As before, initialization is called by the linear BSP bootstrap. There is
// no additional reservation, retry policy or owner held across peer calls.
static MODULE_READY: AtomicBool = AtomicBool::new(false);
static LOADER_READY: AtomicBool = AtomicBool::new(false);
static CONFIG: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

/// # Safety
/// The BSP bootstrap supplies live peers that preserve their existing owner
/// contracts. Registry initialization has completed before it returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_boot_init(peers: &ModulePeers) {
    if MODULE_READY.load(Ordering::Acquire) {
        return;
    }
    unsafe { (peers.registry)() };
    MODULE_READY.store(true, Ordering::Release);
    unsafe { (peers.log)(b"  Module registry ".as_ptr(), 18) };
    unsafe { (peers.log)(b"[OK]\r\n".as_ptr(), 6) };
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_module_boot_ready() -> bool {
    MODULE_READY.load(Ordering::Acquire)
}

unsafe fn log(peers: &LoaderPeers, text: &[u8]) {
    unsafe { (peers.log)(text.as_ptr(), text.len()) };
}

/// # Safety
/// The linear BSP caller supplies live peer functions. Returned config stays
/// resident, and no peer recursively starts another bootstrap generation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_loader_boot_init(peers: &LoaderPeers) -> bool {
    if LOADER_READY.load(Ordering::Acquire) {
        return true;
    }
    if !unsafe { (peers.controllers)() } {
        let text = b"Loader boot before storage controllers";
        return unsafe { (peers.fail)(text.as_ptr(), text.len()) };
    }
    super::performance::begin_filesystem_loader();
    unsafe { (peers.libraries)() };
    unsafe { (peers.drivers)() };
    unsafe { (peers.summary)() };
    let config = unsafe { (peers.config)() };
    CONFIG.store(config.cast_mut(), Ordering::Relaxed);
    let fonts = unsafe { (peers.fonts)() };
    unsafe { log(peers, b"[FONT] catalog loaded=") };
    unsafe { (peers.decimal)(u64::from(fonts.registered)) };
    unsafe { log(peers, b" rejected=") };
    unsafe { (peers.decimal)(u64::from(fonts.rejected)) };
    unsafe { log(peers, b"\r\n") };
    super::performance::finish_filesystem_loader();
    LOADER_READY.store(true, Ordering::Release);
    let text = b"  Loader config [OK]\r\n";
    unsafe { (peers.status)(text.as_ptr(), text.len()) };
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_loader_boot_ready() -> bool {
    LOADER_READY.load(Ordering::Acquire)
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_loader_boot_config() -> *const c_void {
    if !LOADER_READY.load(Ordering::Acquire) {
        return core::ptr::null();
    }
    CONFIG.load(Ordering::Relaxed)
}

const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<ModulePeers>() == 16);
    assert!(offset_of!(ModulePeers, registry) == 0);
    assert!(offset_of!(ModulePeers, log) == 8);
    assert!(size_of::<Fonts>() == 8);
    assert!(offset_of!(Fonts, registered) == 0);
    assert!(offset_of!(Fonts, rejected) == 4);
    assert!(size_of::<LoaderPeers>() == 80);
    assert!(offset_of!(LoaderPeers, controllers) == 0);
    assert!(offset_of!(LoaderPeers, fail) == 8);
    assert!(offset_of!(LoaderPeers, libraries) == 16);
    assert!(offset_of!(LoaderPeers, drivers) == 24);
    assert!(offset_of!(LoaderPeers, summary) == 32);
    assert!(offset_of!(LoaderPeers, config) == 40);
    assert!(offset_of!(LoaderPeers, fonts) == 48);
    assert!(offset_of!(LoaderPeers, log) == 56);
    assert!(offset_of!(LoaderPeers, decimal) == 64);
    assert!(offset_of!(LoaderPeers, status) == 72);
};
