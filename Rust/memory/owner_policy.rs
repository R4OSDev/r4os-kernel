pub fn order_allowed(previous: u8, requested: u8) -> bool {
    previous == 0 || previous < requested
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_owner_order(previous: u8, requested: u8) -> bool {
    order_allowed(previous, requested)
}
