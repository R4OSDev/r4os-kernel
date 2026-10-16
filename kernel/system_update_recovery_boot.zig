// Pre-runtime recovery belongs to the native update owner.
extern fn r4os_rust_update_boot_recover()bool;
pub fn recoverBeforeRuntime()bool{return r4os_rust_update_boot_recover();}
