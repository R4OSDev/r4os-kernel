//! Explicit private-image call site for the otherwise private native R4D
//! lifecycle. A bounded host qualification may include/call this after the
//! fixture plan. It is absent from every normal kernel build.
use crate::{
    boot::log_route as log,
    contract as a,
    driver::{api_admission, registry, resource_state, resources},
    fs::vfs_types::Bytes,
    loader::r4d,
};
use core::{mem::MaybeUninit, ptr};
fn bytes(name: &[u8]) -> Bytes {
    Bytes {
        data: name.as_ptr(),
        len: name.len(),
    }
}
fn capture(name: &[u8]) -> (u32, a::DriverModuleInfo, u64) {
    let index = unsafe { registry::r4os_rust_driver_registry_find(name.as_ptr(), name.len()) };
    assert_ne!(index, usize::MAX);
    let owner = u32::try_from(index + 1).unwrap();
    let mut info = a::DriverModuleInfo::default();
    assert_eq!(unsafe { r4d::r4os_rust_r4d_info(owner, &mut info) }, 2);
    assert!(api_admission::r4os_rust_driver_api_enter(owner, 1000));
    let mut binding = ptr::null();
    assert_eq!(
        unsafe {
            resource_state::r4os_rust_resource_current(
                resources::r4os_rust_driver_resource_state(),
                owner,
                &mut binding,
            )
        },
        0
    );
    let epoch = unsafe { (*binding).epoch };
    api_admission::r4os_rust_driver_api_leave();
    (owner, info, epoch)
}
pub(super) fn run() {
    let mut completed = 0;
    for n in 0..129u32 {
        let name = [
            b'D',
            b'C',
            b'2',
            b'9',
            b'0' + (n / 100) as u8,
            b'0' + (n / 10 % 10) as u8,
            b'0' + (n % 10) as u8,
        ];
        let (owner, before, epoch) = capture(&name);
        assert!(
            unsafe { r4d::r4os_rust_r4d_unload(bytes(&name)) },
            "real R4D shutdown/cleanup refused"
        );
        let mut absent = MaybeUninit::uninit();
        assert_eq!(
            unsafe { r4d::r4os_rust_r4d_info(owner, absent.as_mut_ptr()) },
            0
        );
        assert_eq!(
            crate::driver::heap::r4os_rust_driver_heap_query(owner),
            a::driver_heap_error_stale
        );
        let result = unsafe { r4d::r4os_rust_r4d_load(bytes(&name)) };
        assert!(r4d::r4os_rust_r4d_succeeded(result));
        let (new_owner, after, new_epoch) = capture(&name);
        assert!(after.generation > before.generation);
        assert_ne!(new_epoch, epoch);
        log::puts(b"D29CYCLE owner=");
        log::dec(owner.into());
        log::puts(b"->");
        log::dec(new_owner.into());
        log::puts(b" generation=");
        log::dec(before.generation);
        log::puts(b"->");
        log::dec(after.generation);
        log::puts(b" epoch=");
        log::dec(epoch);
        log::puts(b"->");
        log::dec(new_epoch);
        log::puts(b" unload-ack+stale-heap+renewed-init PASS\r\n");
        completed += 1;
    }
    log::puts(b"D29CYCLE complete=");
    log::dec(completed);
    log::puts(b" actual-shutdown-cleanup-reuse PASS\r\n");
}
