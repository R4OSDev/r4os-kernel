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
    // Fixed legacy controls only. Do not inherit optional BSP CR0 state.
    movl $0x33, %eax
    movl %eax, %cr0
    ljmpl *%cs:0x0850

.code32
r4os_ap_trampoline_protected_mode:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movb $9, 0x4e(%ebx)
    movl %ebx, %esi

    // CPUID clobbers EBX. ESI alone retains the private config pointer.
    // No optional CR4/EFER bits or FPU state are touched before these checks.
    xorl %eax, %eax
    xorl %ecx, %ecx
    cpuid
    movl %eax, %edi
    cmpl $1, %edi
    jb .Lap_config
    cmpl $0x1f, %edi
    jb .Lap_id_b
    movl $0x1f, %eax
    xorl %ecx, %ecx
    cpuid
    testl $0xffff, %ebx
    jz .Lap_id_b
    testl $0xff00, %ecx
    jnz .Lap_id_found
.Lap_id_b:
    cmpl $0x0b, %edi
    jb .Lap_id_extended
    movl $0x0b, %eax
    xorl %ecx, %ecx
    cpuid
    testl $0xffff, %ebx
    jz .Lap_id_extended
    testl $0xff00, %ecx
    jnz .Lap_id_found
.Lap_id_extended:
    movl $0x80000000, %eax
    xorl %ecx, %ecx
    cpuid
    cmpl $0x8000001e, %eax
    jb .Lap_id_legacy
    movl $0x80000001, %eax
    xorl %ecx, %ecx
    cpuid
    testl $(1 << 22), %ecx
    jz .Lap_id_legacy
    movl $0x8000001e, %eax
    xorl %ecx, %ecx
    cpuid
    movl %eax, %edx
    jmp .Lap_id_found
.Lap_id_legacy:
    movl $1, %eax
    xorl %ecx, %ecx
    cpuid
    movl %ebx, %edx
    shrl $24, %edx
.Lap_id_found:
    movl %edx, 0x68(%esi)
    cmpl 0x60(%esi), %edx
    jne .Lap_identity
    movl 0x58(%esi), %eax
    orl 0x5c(%esi), %eax
    jz .Lap_config

    movl $1, %eax
    xorl %ecx, %ecx
    cpuid
    testl $(1 << 5), %edx
    jz .Lap_msr
    testl $(1 << 6), %edx
    jz .Lap_pae
    movl $0x80000000, %eax
    xorl %ecx, %ecx
    cpuid
    cmpl $0x80000001, %eax
    jb .Lap_long_mode_missing
    movl $0x80000001, %eax
    xorl %ecx, %ecx
    cpuid
    testl $(1 << 29), %edx
    jz .Lap_long_mode_missing
    testl $1, 0x6c(%esi)
    jz .Lap_nx_check
    testl $(1 << 26), %edx
    jz .Lap_1g_missing
.Lap_nx_check:
    testl $(1 << 11), 0x20(%esi)
    jz .Lap_minimal
    testl $(1 << 20), %edx
    jz .Lap_nx
.Lap_minimal:
    // A checked four-level PAE root, LME and (only when required) NXE.
    // OSXSAVE/OSFXSR and every module requirement are checked in integer-only
    // Rust before local FPU controls, XSETBV or the first common XRSTOR.
    movl $0x20, %eax
    movl %eax, %cr4
    movl 0x08(%esi), %eax
    movl %eax, %cr3
    movb $10, 0x4e(%esi)

    movl $0xC0000080, %ecx
    movl 0x20(%esi), %eax
    xorl %edx, %edx
    wrmsr
    movb $11, 0x4e(%esi)

    movl $0x80010033, %eax
    movl %eax, %cr0
    movl %esi, %ebx
    movb $12, 0x4e(%ebx)
    ljmpl *0x40(%ebx)

.Lap_msr:
    movl $1, %eax
    jmp .Lap_reject
.Lap_pae:
    movl $2, %eax
    jmp .Lap_reject
.Lap_long_mode_missing:
    movl $3, %eax
    jmp .Lap_reject
.Lap_nx:
    movl $4, %eax
    jmp .Lap_reject
.Lap_identity:
    movl $5, %eax
    jmp .Lap_reject
.Lap_config:
    movl $6, %eax
    jmp .Lap_reject
.Lap_1g_missing:
    movl $7, %eax
.Lap_reject:
    movl %eax, 0x64(%esi)
    movb $0xfe, 0x4e(%esi)
.Lap_halt:
    cli
    hlt
    jmp .Lap_halt

.code64
r4os_ap_trampoline_long_mode:
    movl %ebx, %ebx
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
    movq 0x58(%rbx), %rsi
    movl 0x60(%rbx), %edx
    movq 0x28(%rbx), %rax
    movb $15, 0x4e(%rbx)
    call *%rax
1:
    cli
    hlt
    jmp 1b

r4os_ap_trampoline_end:
.size r4os_ap_trampoline_start, .-r4os_ap_trampoline_start
