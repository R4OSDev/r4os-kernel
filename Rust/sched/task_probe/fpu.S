// Explicit probe-only AVX operations. Normal compiled kernel code keeps the
// unchanged no-SIMD target. The real Task/IRQ FPU owner surrounds these calls.
.text
.p2align 4
.global r4os_task_probe_ymm_load
.type r4os_task_probe_ymm_load,@function
r4os_task_probe_ymm_load:
    vmovdqu (%rdi), %ymm0
    ret
.size r4os_task_probe_ymm_load,.-r4os_task_probe_ymm_load
.p2align 4
.global r4os_task_probe_ymm_store
.type r4os_task_probe_ymm_store,@function
r4os_task_probe_ymm_store:
    vmovdqu %ymm0, (%rdi)
    vzeroupper
    ret
.size r4os_task_probe_ymm_store,.-r4os_task_probe_ymm_store
.p2align 4
.global r4os_task_probe_initial_fpu
.type r4os_task_probe_initial_fpu,@function
r4os_task_probe_initial_fpu:
    vmovdqu %ymm0, (%rdi)
    stmxcsr (%rsi)
    ret
.size r4os_task_probe_initial_fpu,.-r4os_task_probe_initial_fpu
.section .note.GNU-stack,"",@progbits
