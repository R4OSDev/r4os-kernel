// Private C facade; masks and physical PIC programming have one Rust owner.
extern fn r4os_rust_pic_control(operation: u8, irq: u8) u8;
pub const MASTER_OFFSET: u8 = 0x20;
pub const SLAVE_OFFSET: u8 = 0x28;
pub fn init() void {
    _ = r4os_rust_pic_control(0, 0);
}
pub fn unmask(irq: u8) void {
    _ = r4os_rust_pic_control(1, irq);
}
pub fn mask(irq: u8) void {
    _ = r4os_rust_pic_control(2, irq);
}
pub fn maskAll() void {
    _ = r4os_rust_pic_control(3, 0);
}
pub fn endOfInterrupt(irq: u8) void {
    _ = r4os_rust_pic_control(4, irq);
}
pub fn masterMask() u8 {
    return r4os_rust_pic_control(5, 0);
}
pub fn slaveMask() u8 {
    return r4os_rust_pic_control(6, 0);
}
