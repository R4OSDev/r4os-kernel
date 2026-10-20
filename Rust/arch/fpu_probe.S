// Private diagnostic burst. Only the explicitly reviewed no-SIMD kernel
// boundary may be nonnull. Userland always passes null; its live RIP window
// contains no call. Ordinary SysV FP control environment is restored on return.
.text
.p2align 4
.global r4os_fp_matrix
.hidden r4os_fp_matrix
.type r4os_fp_matrix,@function
r4os_fp_matrix:
    push %rbx
    push %rbp
    push %r12
    push %r13
    push %r14
    push %r15
    sub $24, %rsp
    fnstcw 0(%rsp)
    stmxcsr 4(%rsp)
    mov %rdi, %r12
    mov %rsi, %r13
    mov %rdx, %r14
    mov %rcx, %r15
    mov %r8, %rbx
    mov %r9, %rbp
    fninit
    fldcw 0(%r14)
    ldmxcsr 4(%r14)
    cmpl $1, 12(%r14)
    je .Lr4os_fp_matrix_mmx
    cmpl $2, 12(%r14)
    je .Lr4os_fp_matrix_sparse
    fldt 512(%r12)
    fldt 528(%r12)
    fldt 544(%r12)
    fldt 560(%r12)
    fldt 576(%r12)
    fldt 592(%r12)
    fldt 608(%r12)
    fldt 624(%r12)
    fincstp
    fincstp
    jmp .Lr4os_fp_matrix_vectors
.Lr4os_fp_matrix_sparse:
    fldt 512(%r12)
    fldt 528(%r12)
    fldt 544(%r12)
    jmp .Lr4os_fp_matrix_vectors
.Lr4os_fp_matrix_mmx:
    movq 640(%r12), %mm0
    movq 648(%r12), %mm1
    movq 656(%r12), %mm2
    movq 664(%r12), %mm3
    movq 672(%r12), %mm4
    movq 680(%r12), %mm5
    movq 688(%r12), %mm6
    movq 696(%r12), %mm7
.Lr4os_fp_matrix_vectors:
    cmpl $0, 8(%r14)
    je .Lr4os_fp_matrix_sse
    vmovdqu 0(%r12), %ymm0
    vmovdqu 32(%r12), %ymm1
    vmovdqu 64(%r12), %ymm2
    vmovdqu 96(%r12), %ymm3
    vmovdqu 128(%r12), %ymm4
    vmovdqu 160(%r12), %ymm5
    vmovdqu 192(%r12), %ymm6
    vmovdqu 224(%r12), %ymm7
    vmovdqu 256(%r12), %ymm8
    vmovdqu 288(%r12), %ymm9
    vmovdqu 320(%r12), %ymm10
    vmovdqu 352(%r12), %ymm11
    vmovdqu 384(%r12), %ymm12
    vmovdqu 416(%r12), %ymm13
    vmovdqu 448(%r12), %ymm14
    vmovdqu 480(%r12), %ymm15
    jmp .Lr4os_fp_matrix_loaded
.Lr4os_fp_matrix_sse:
    movdqu 0(%r12), %xmm0
    movdqu 32(%r12), %xmm1
    movdqu 64(%r12), %xmm2
    movdqu 96(%r12), %xmm3
    movdqu 128(%r12), %xmm4
    movdqu 160(%r12), %xmm5
    movdqu 192(%r12), %xmm6
    movdqu 224(%r12), %xmm7
    movdqu 256(%r12), %xmm8
    movdqu 288(%r12), %xmm9
    movdqu 320(%r12), %xmm10
    movdqu 352(%r12), %xmm11
    movdqu 384(%r12), %xmm12
    movdqu 416(%r12), %xmm13
    movdqu 448(%r12), %xmm14
    movdqu 480(%r12), %xmm15
.Lr4os_fp_matrix_loaded:
    fxsave64 0(%r13)
    test %r15, %r15
    jz .Lr4os_fp_matrix_boundary_done
    mov %rbx, %rdi
    call *%r15
.Lr4os_fp_matrix_boundary_done:
    mov %rbp, %r10
    test %r10, %r10
    jz .Lr4os_fp_matrix_saved
.global r4os_fp_matrix_live_begin
.hidden r4os_fp_matrix_live_begin
r4os_fp_matrix_live_begin:
.Lr4os_fp_matrix_loop:
    pause
    dec %r10
    jnz .Lr4os_fp_matrix_loop
.global r4os_fp_matrix_live_end
.hidden r4os_fp_matrix_live_end
r4os_fp_matrix_live_end:
.Lr4os_fp_matrix_saved:
    fxsave64 512(%r13)
    cmpl $0, 8(%r14)
    je .Lr4os_fp_matrix_store_sse
    vmovdqu %ymm0, 1024(%r13)
    vmovdqu %ymm1, 1056(%r13)
    vmovdqu %ymm2, 1088(%r13)
    vmovdqu %ymm3, 1120(%r13)
    vmovdqu %ymm4, 1152(%r13)
    vmovdqu %ymm5, 1184(%r13)
    vmovdqu %ymm6, 1216(%r13)
    vmovdqu %ymm7, 1248(%r13)
    vmovdqu %ymm8, 1280(%r13)
    vmovdqu %ymm9, 1312(%r13)
    vmovdqu %ymm10, 1344(%r13)
    vmovdqu %ymm11, 1376(%r13)
    vmovdqu %ymm12, 1408(%r13)
    vmovdqu %ymm13, 1440(%r13)
    vmovdqu %ymm14, 1472(%r13)
    vmovdqu %ymm15, 1504(%r13)
    jmp .Lr4os_fp_matrix_round
.Lr4os_fp_matrix_store_sse:
    movdqu %xmm0, 1024(%r13)
    movdqu %xmm1, 1056(%r13)
    movdqu %xmm2, 1088(%r13)
    movdqu %xmm3, 1120(%r13)
    movdqu %xmm4, 1152(%r13)
    movdqu %xmm5, 1184(%r13)
    movdqu %xmm6, 1216(%r13)
    movdqu %xmm7, 1248(%r13)
    movdqu %xmm8, 1280(%r13)
    movdqu %xmm9, 1312(%r13)
    movdqu %xmm10, 1344(%r13)
    movdqu %xmm11, 1376(%r13)
    movdqu %xmm12, 1408(%r13)
    movdqu %xmm13, 1440(%r13)
    movdqu %xmm14, 1472(%r13)
    movdqu %xmm15, 1504(%r13)
.Lr4os_fp_matrix_round:
    emms
    fninit
    fldcw 0(%r14)
    fldl .Lr4os_fp_matrix_one_half(%rip)
    fistpll 1536(%r13)
    movsd .Lr4os_fp_matrix_one_half(%rip), %xmm0
    cvtsd2siq %xmm0, %rax
    mov %rax, 1544(%r13)
    emms
    fxsave64 1600(%r13)
    fninit
    ldmxcsr 4(%rsp)
    fldcw 0(%rsp)
    cmpl $0, 8(%r14)
    je .Lr4os_fp_matrix_return
    vzeroupper
.Lr4os_fp_matrix_return:
    add $24, %rsp
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbp
    pop %rbx
    ret
.size r4os_fp_matrix,.-r4os_fp_matrix
.section .rodata
.p2align 3
.Lr4os_fp_matrix_one_half:
    .quad 0x3ff8000000000000
.section .note.GNU-stack,"",@progbits
