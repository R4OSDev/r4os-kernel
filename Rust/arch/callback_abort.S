.text
.p2align 4
.globl r4os_rust_callback_invoke
r4os_rust_callback_invoke:
    movq %rsp, 0(%rdx)
    movq %rbx, 8(%rdx)
    movq %rbp, 16(%rdx)
    movq %r12, 24(%rdx)
    movq %r13, 32(%rdx)
    movq %r14, 40(%rdx)
    movq %r15, 48(%rdx)
    stmxcsr 56(%rdx)
    fnstcw 60(%rdx)
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
.globl r4os_rust_callback_resume
r4os_rust_callback_resume:
    movl %esi, %eax
    movq %rdi, %rdx
    movq 0(%rdx), %rsp
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
