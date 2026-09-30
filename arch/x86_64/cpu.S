.section .text

.global r4os_current_cpu_index
.type r4os_current_cpu_index, @function
r4os_current_cpu_index:
    movl %gs:0, %eax
    ret

.global r4os_load_gdt
.type r4os_load_gdt, @function
r4os_load_gdt:
    lgdt (%rdi)

    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss
    mov %ax, %fs
    mov %ax, %gs

    pushq $0x08
    leaq 1f(%rip), %rax
    pushq %rax
    lretq
1:
    ret

.global r4os_load_idt
.type r4os_load_idt, @function
r4os_load_idt:
    lidt (%rdi)
    ret

.global r4os_load_tss
.type r4os_load_tss, @function
r4os_load_tss:
    ltr %di
    ret

.global r4os_read_cr3
.type r4os_read_cr3, @function
r4os_read_cr3:
    mov %cr3, %rax
    ret

.global r4os_read_cr0
.type r4os_read_cr0, @function
r4os_read_cr0:
    mov %cr0, %rax
    ret

.global r4os_write_cr0
.type r4os_write_cr0, @function
r4os_write_cr0:
    mov %rdi, %cr0
    ret

.global r4os_read_cr4
.type r4os_read_cr4, @function
r4os_read_cr4:
    mov %cr4, %rax
    ret

.global r4os_write_cr4
.type r4os_write_cr4, @function
r4os_write_cr4:
    mov %rdi, %cr4
    ret

.global r4os_write_cr3
.type r4os_write_cr3, @function
r4os_write_cr3:
    mov %rdi, %cr3
    ret

.global r4os_invlpg
.type r4os_invlpg, @function
r4os_invlpg:
    invlpg (%rdi)
    ret

.global r4os_context_switch
.type r4os_context_switch, @function
r4os_context_switch:
    pushq %rbp
    pushq %rbx
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    movq %rsp, (%rdi)
    movq %rsi, %rsp

    # The outgoing task becomes selectable only after its complete switch RSP
    # is published above.  Run the global scheduler-boundary release on the
    # incoming stack, below its saved context, so another CPU can never load a
    # stale outgoing RSP.  r15 is callee-saved and temporarily anchors the
    # incoming context while the call stack is aligned for SysV.
    movq %rsp, %r15
    andq $-16, %rsp
    call r4os_finish_context_switch
    movq %r15, %rsp

    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %rbx
    popq %rbp
    ret

.global r4os_xsetbv
.type r4os_xsetbv, @function
r4os_xsetbv:
    mov %edi, %ecx
    mov %esi, %eax
    mov %rsi, %rdx
    shr $32, %rdx
    xsetbv
    ret

.global r4os_fninit
.type r4os_fninit, @function
r4os_fninit:
    fninit
    ret

.global r4os_init_simd
.type r4os_init_simd, @function
r4os_init_simd:
    # FNINIT affects x87 only. Record the inherited SIMD baseline for the
    # bounded boot diagnostic, then initialize SSE control and every lane.
    movdqu %xmm0, (%rsi)
    stmxcsr 16(%rsi)
    sub $8, %rsp
    movl $0x1f80, (%rsp)
    ldmxcsr (%rsp)
    add $8, %rsp
    test %edi, %edi
    jz .Linit_simd_sse
    # The caller has already enabled OSXSAVE and XCR0.AVX on this CPU.
    vzeroall
    ret
.Linit_simd_sse:
    pxor %xmm0, %xmm0
    pxor %xmm1, %xmm1
    pxor %xmm2, %xmm2
    pxor %xmm3, %xmm3
    pxor %xmm4, %xmm4
    pxor %xmm5, %xmm5
    pxor %xmm6, %xmm6
    pxor %xmm7, %xmm7
    pxor %xmm8, %xmm8
    pxor %xmm9, %xmm9
    pxor %xmm10, %xmm10
    pxor %xmm11, %xmm11
    pxor %xmm12, %xmm12
    pxor %xmm13, %xmm13
    pxor %xmm14, %xmm14
    pxor %xmm15, %xmm15
    ret

.global r4os_fxsave
.type r4os_fxsave, @function
r4os_fxsave:
    fxsave64 (%rdi)
    ret

.global r4os_fxrstor
.type r4os_fxrstor, @function
r4os_fxrstor:
    fxrstor64 (%rdi)
    ret

.global r4os_xsave
.type r4os_xsave, @function
r4os_xsave:
    mov %esi, %eax
    mov %rsi, %rdx
    shr $32, %rdx
    xsave64 (%rdi)
    ret

.global r4os_xrstor
.type r4os_xrstor, @function
r4os_xrstor:
    mov %esi, %eax
    mov %rsi, %rdx
    shr $32, %rdx
    xrstor64 (%rdi)
    ret

.global r4os_call_program
.type r4os_call_program, @function
r4os_call_program:
    pushq %rbp
    pushq %rbx
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    movq %rsp, %r15
    movq %rdi, %rax
    movq %rdx, %rsp
    andq $-16, %rsp
    subq $16, %rsp
    movq %rsi, %rdi
    call *%rax
    movq %r15, %rsp

    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %rbx
    popq %rbp
    ret
