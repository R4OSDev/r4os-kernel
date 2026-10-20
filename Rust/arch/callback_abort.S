.text
.macro save_frame base
    movq %rsp, 0(\base)
    movq %rbx, 8(\base)
    movq %rbp, 16(\base)
    movq %r12, 24(\base)
    movq %r13, 32(\base)
    movq %r14, 40(\base)
    movq %r15, 48(\base)
    stmxcsr 56(\base)
    fnstcw 60(\base)
.endm
.p2align 4
.globl r4os_rust_callback_invoke
r4os_rust_callback_invoke:
    save_frame %rdx
    movq %rdi, %rax
    movq %rsi, %rdi
    pushq %rdx
    call *%rax
    popq %rdx
    ldmxcsr 56(%rdx)
    fldcw 60(%rdx)
    movq 8(%rdx), %rbx
    movq 16(%rdx), %rbp
    movq 24(%rdx), %r12
    movq 32(%rdx), %r13
    movq 40(%rdx), %r14
    movq 48(%rdx), %r15
    cld
    ret
.p2align 4
.globl r4os_rust_program_invoke
r4os_rust_program_invoke:
    save_frame %rcx
    movq %rdi, %rax
    movq %rcx, %r15
    movq %rdx, %rsp
    andq $-16, %rsp
    subq $16, %rsp
    movq %rsi, %rdi
    call *%rax
    movl %eax, %esi
    movq %r15, %rdi
    jmp r4os_rust_callback_resume
.p2align 4
.globl r4os_rust_callback_resume
r4os_rust_callback_resume:
    movl %esi, %eax
    movq %rdi, %rdx
    movq 0(%rdx), %rsp
    # Nonlocal exit skips EMMS and module epilogues. Data/status are caller-
    # saved here; return in x87 mode before restoring the callee-saved controls.
    # FNINIT is nonwaiting, including a discarded pending x87 condition.
    fninit
    ldmxcsr 56(%rdx)
    fldcw 60(%rdx)
    movq 8(%rdx), %rbx
    movq 16(%rdx), %rbp
    movq 24(%rdx), %r12
    movq 32(%rdx), %r13
    movq 40(%rdx), %r14
    movq 48(%rdx), %r15
    cld
    ret
