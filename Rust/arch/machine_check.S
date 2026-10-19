.section .text
# Only the claimed terminal #MC owner, on its dedicated IST3. Exact nested GP
# fixup returns to this caller; no write/clear instruction exists in this file.
.global r4os_mce_read
.type r4os_mce_read, @function
r4os_mce_read:
    movl %edi, %ecx
.global r4os_mce_read_fault
r4os_mce_read_fault:
    rdmsr
    shlq $32, %rdx
    orq %rdx, %rax
    movq %rax, (%rsi)
    xorl %eax, %eax
    ret
.global r4os_mce_read_resume
r4os_mce_read_resume:
    movl $1, %eax
    ret
.size r4os_mce_read, .-r4os_mce_read
