// Terminal reset belongs to the native architecture owner.
extern fn r4os_rust_terminal_reset() noreturn;
pub fn reboot() noreturn {
    r4os_rust_terminal_reset();
}
