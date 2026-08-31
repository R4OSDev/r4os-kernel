// Position-independent INIT/SIPI trampoline.  The BSP copies these bytes to
// one private low-memory page per AP and writes the fixed config block at
// offset 0x800.  No link-time address is dereferenced before long mode.

.section .text.ap_trampoline,"ax"
.code16
.balign 16
.global r4os_ap_trampoline_start
.global r4os_ap_trampoline_protected_mode
.global r4os_ap_trampoline_long_mode
.global r4os_ap_trampoline_end
.type r4os_ap_trampoline_start, @function
r4os_ap_trampoline_start:
    cli
    movb $1, %cs:0x084e
    cld

    movw %cs, %ax
    movw %ax, %ss
    movw $0x0700, %sp
    movb $2, %cs:0x084e
    movzwl %ax, %ebx
    shll $4, %ebx
    addl $0x0800, %ebx
    movb $3, %cs:0x084e

    xorw %ax, %ax
    movb $4, %cs:0x084e
    movw %ax, %ds
    movb $5, %cs:0x084e
    movw %ax, %es
    movb $6, %cs:0x084e
    // Before paging is active, a 32-bit offset through a real-mode data
    // segment is still checked against its 64 KiB limit.  Address the copied
    // page relative to CS instead; RBX remains the physical config address
    // needed after the long-mode jump.
    movb $7, %cs:0x084e

    lgdtl %cs:0x0848
    movb $8, %cs:0x084e
    // Establish a flat 32-bit protected-mode data segment before any access
    // through the physical config pointer.  Real-mode segment caches retain
    // a 64 KiB limit on KVM and hardware even for 32-bit offsets.
    movl %cs:0x0810, %eax
    andl $0x7fffffff, %eax
    orl $1, %eax
    movl %eax, %cr0
    ljmpl *%cs:0x0850

.code32
r4os_ap_trampoline_protected_mode:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movb $9, 0x4e(%ebx)

    movl 0x18(%ebx), %eax
    movl %eax, %cr4
    movl 0x08(%ebx), %eax
    movl %eax, %cr3
    movb $10, 0x4e(%ebx)

    movl $0xC0000080, %ecx
    movl 0x20(%ebx), %eax
    movl 0x24(%ebx), %edx
    wrmsr
    movb $11, 0x4e(%ebx)

    movl 0x10(%ebx), %eax
    movl %eax, %cr0
    movb $12, 0x4e(%ebx)
    ljmpl *0x40(%ebx)

.code64
r4os_ap_trampoline_long_mode:
    movb $13, 0x4e(%rbx)
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movb $14, 0x4e(%rbx)
    movq 0x30(%rbx), %rsp
    andq $-16, %rsp
    xorq %rbp, %rbp
    movq 0x38(%rbx), %rdi
    movq 0x28(%rbx), %rax
    movb $15, 0x4e(%rbx)
    call *%rax
1:
    cli
    hlt
    jmp 1b

r4os_ap_trampoline_end:
.size r4os_ap_trampoline_start, .-r4os_ap_trampoline_start
