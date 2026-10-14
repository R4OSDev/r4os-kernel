// Host-only weak links: genuine Zig adapters override these traps.
// A pure host fixture must never execute privileged kernel operations.
.text
.weak r4os_read_cr0
r4os_read_cr0:
    jmp r4os_kernel_owner_test_panic
.weak r4os_write_cr0
r4os_write_cr0:
    jmp r4os_kernel_owner_test_panic
.weak r4os_read_cr4
r4os_read_cr4:
    jmp r4os_kernel_owner_test_panic
.weak r4os_write_cr4
r4os_write_cr4:
    jmp r4os_kernel_owner_test_panic
.weak r4os_read_cr3
r4os_read_cr3:
    jmp r4os_kernel_owner_test_panic
.weak r4os_invlpg
r4os_invlpg:
    jmp r4os_kernel_owner_test_panic
.weak r4os_xsetbv
r4os_xsetbv:
    jmp r4os_kernel_owner_test_panic
.weak r4os_fninit
r4os_fninit:
    jmp r4os_kernel_owner_test_panic
.weak r4os_init_simd
r4os_init_simd:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_fpu_log
r4os_zig_fpu_log:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_cpu_log
r4os_zig_cpu_log:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_kernel_write
r4os_zig_kernel_write:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_kernel_dec
r4os_zig_kernel_dec:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_heap_can_wait
r4os_zig_heap_can_wait:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_heap_wait_growth
r4os_zig_heap_wait_growth:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_heap_wake_growth
r4os_zig_heap_wake_growth:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_heap_commit
r4os_zig_heap_commit:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_heap_uncommit
r4os_zig_heap_uncommit:
    jmp r4os_kernel_owner_test_panic
.weak r4os_zig_heap_fault
r4os_zig_heap_fault:
    jmp r4os_kernel_owner_test_panic

.weak r4os_zig_scheduler_log
r4os_zig_scheduler_log:
    jmp r4os_kernel_owner_test_panic
