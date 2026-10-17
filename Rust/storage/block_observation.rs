//! Optional exact-request observation. Only explicit native diagnosis supplies
//! a resident immutable subscription; no caller output pointer is retained.
use crate::contract as c;
#[repr(C)]
pub(crate) struct Subscription {
    pub cookie: u64,
    pub observe: c::StorageRequestObserve,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct ReadReceipt {
    pub flags: u32,
    pub reserved0: u32,
    pub request_id: u64,
    pub backend_handle: u64,
    pub bytes: u64,
}
const _: () = {
    assert!(core::mem::size_of::<ReadReceipt>() == 32);
};
