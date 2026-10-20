.text
.p2align 4
.global r4os_fp_scribble
.type r4os_fp_scribble,@function
r4os_fp_scribble:
    sub $16,%rsp
    movw $0x0b7f,0(%rsp)
    movl $0x5f80,4(%rsp)
    fninit
    fldcw 0(%rsp)
    ldmxcsr 4(%rsp)
    test %edi,%edi
    jz .Lscribble_sse
    vpcmpeqd %ymm0,%ymm0,%ymm0
    vpcmpeqd %ymm1,%ymm1,%ymm1
    vpcmpeqd %ymm2,%ymm2,%ymm2
    vpcmpeqd %ymm3,%ymm3,%ymm3
    vpcmpeqd %ymm4,%ymm4,%ymm4
    vpcmpeqd %ymm5,%ymm5,%ymm5
    vpcmpeqd %ymm6,%ymm6,%ymm6
    vpcmpeqd %ymm7,%ymm7,%ymm7
    vpcmpeqd %ymm8,%ymm8,%ymm8
    vpcmpeqd %ymm9,%ymm9,%ymm9
    vpcmpeqd %ymm10,%ymm10,%ymm10
    vpcmpeqd %ymm11,%ymm11,%ymm11
    vpcmpeqd %ymm12,%ymm12,%ymm12
    vpcmpeqd %ymm13,%ymm13,%ymm13
    vpcmpeqd %ymm14,%ymm14,%ymm14
    vpcmpeqd %ymm15,%ymm15,%ymm15
    jmp .Lscribble_mmx
.Lscribble_sse:
    pcmpeqd %xmm0,%xmm0
    pcmpeqd %xmm1,%xmm1
    pcmpeqd %xmm2,%xmm2
    pcmpeqd %xmm3,%xmm3
    pcmpeqd %xmm4,%xmm4
    pcmpeqd %xmm5,%xmm5
    pcmpeqd %xmm6,%xmm6
    pcmpeqd %xmm7,%xmm7
    pcmpeqd %xmm8,%xmm8
    pcmpeqd %xmm9,%xmm9
    pcmpeqd %xmm10,%xmm10
    pcmpeqd %xmm11,%xmm11
    pcmpeqd %xmm12,%xmm12
    pcmpeqd %xmm13,%xmm13
    pcmpeqd %xmm14,%xmm14
    pcmpeqd %xmm15,%xmm15
.Lscribble_mmx:
    pcmpeqd %mm0,%mm0
    pcmpeqd %mm1,%mm1
    pcmpeqd %mm2,%mm2
    pcmpeqd %mm3,%mm3
    pcmpeqd %mm4,%mm4
    pcmpeqd %mm5,%mm5
    pcmpeqd %mm6,%mm6
    pcmpeqd %mm7,%mm7
    emms
    add $16,%rsp
    ret
.size r4os_fp_scribble,.-r4os_fp_scribble
.p2align 4
.global r4os_fp_vectors
.type r4os_fp_vectors,@function
r4os_fp_vectors:
    test %esi,%esi
    jz .Lvectors_sse
    vmovdqu %ymm0,0(%rdi)
    vmovdqu %ymm1,32(%rdi)
    vmovdqu %ymm2,64(%rdi)
    vmovdqu %ymm3,96(%rdi)
    vmovdqu %ymm4,128(%rdi)
    vmovdqu %ymm5,160(%rdi)
    vmovdqu %ymm6,192(%rdi)
    vmovdqu %ymm7,224(%rdi)
    vmovdqu %ymm8,256(%rdi)
    vmovdqu %ymm9,288(%rdi)
    vmovdqu %ymm10,320(%rdi)
    vmovdqu %ymm11,352(%rdi)
    vmovdqu %ymm12,384(%rdi)
    vmovdqu %ymm13,416(%rdi)
    vmovdqu %ymm14,448(%rdi)
    vmovdqu %ymm15,480(%rdi)
    ret
.Lvectors_sse:
    movdqu %xmm0,0(%rdi)
    movdqu %xmm1,32(%rdi)
    movdqu %xmm2,64(%rdi)
    movdqu %xmm3,96(%rdi)
    movdqu %xmm4,128(%rdi)
    movdqu %xmm5,160(%rdi)
    movdqu %xmm6,192(%rdi)
    movdqu %xmm7,224(%rdi)
    movdqu %xmm8,256(%rdi)
    movdqu %xmm9,288(%rdi)
    movdqu %xmm10,320(%rdi)
    movdqu %xmm11,352(%rdi)
    movdqu %xmm12,384(%rdi)
    movdqu %xmm13,416(%rdi)
    movdqu %xmm14,448(%rdi)
    movdqu %xmm15,480(%rdi)
    ret
.size r4os_fp_vectors,.-r4os_fp_vectors
.p2align 4
.global r4os_fp_abort_body
.type r4os_fp_abort_body,@function
r4os_fp_abort_body:
    sub $16,%rsp
    movw $0x0b7f,0(%rsp)
    movl $0x5f80,4(%rsp)
    fldcw 0(%rsp)
    ldmxcsr 4(%rsp)
    pcmpeqd %mm0,%mm0
    mov $46,%esi
    jmp r4os_rust_callback_resume
.size r4os_fp_abort_body,.-r4os_fp_abort_body
.section .note.GNU-stack,"",@progbits
