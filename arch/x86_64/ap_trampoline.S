// Position-independent INIT/SIPI trampoline.  The BSP copies these bytes to
// one private low-memory page per AP and writes the fixed config block at
// offset 0x800.  No link-time address is dereferenced before long mode.

.section .text.ap_trampoline,"ax"
.code16
.balign 16
.global r4os_ap_trampoline_start
.global r4os_ap_trampoline_long_mode
.global r4os_ap_trampoline_end
.type r4os_ap_trampoline_start, @function
r4os_ap_trampoline_start:
    cli
    cld

    movw %cs, %ax
    movw %ax, %ss
    movw $0x0700, %sp
    movzwl %ax, %ebx
    shll $4, %ebx
    addl $0x0800, %ebx

    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es

    lgdtl 0x48(%ebx)
    movl 0x18(%ebx), %eax
    movl %eax, %cr4
    movl 0x08(%ebx), %eax
    movl %eax, %cr3

    movl $0xC0000080, %ecx
    movl 0x20(%ebx), %eax
    movl 0x24(%ebx), %edx
    wrmsr

    movl 0x10(%ebx), %eax
    movl %eax, %cr0
    ljmpl *0x40(%ebx)

.code64
r4os_ap_trampoline_long_mode:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movq 0x30(%rbx), %rsp
    andq $-16, %rsp
    xorq %rbp, %rbp
    movq 0x38(%rbx), %rdi
    movq 0x28(%rbx), %rax
    call *%rax
1:
    cli
    hlt
    jmp 1b

r4os_ap_trampoline_end:
.size r4os_ap_trampoline_start, .-r4os_ap_trampoline_start
