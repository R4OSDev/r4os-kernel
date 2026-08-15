.section .text

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
