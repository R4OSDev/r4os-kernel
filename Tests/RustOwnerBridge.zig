// Host fixture panic reaches the existing Zig test runtime. Product kernels
// never import this module and keep the resident Rust fatal handler.
export fn r4os_kernel_owner_test_panic() callconv(.c) noreturn {
    @panic("Rust kernel owner fixture panic");
}
