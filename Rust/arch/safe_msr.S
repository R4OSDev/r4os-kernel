.section .text

# Called only inside the exact CPU-local, IF=0 admission in safe_msr.rs.
# Recovery is valid only at the labelled single hardware instruction. No
# output is written on fault; ordinary exceptions retain the fatal path.
.global r4os_safe_msr_read
.type r4os_safe_msr_read, @function
r4os_safe_msr_read:
    movl %edi, %ecx
.global r4os_safe_msr_read_fault
r4os_safe_msr_read_fault:
    rdmsr
    shlq $32, %rdx
    orq %rdx, %rax
    movq %rax, (%rsi)
    xorl %eax, %eax
    ret
.global r4os_safe_msr_read_resume
r4os_safe_msr_read_resume:
    movl $1, %eax
    ret
.size r4os_safe_msr_read, .-r4os_safe_msr_read

.global r4os_safe_msr_write
.type r4os_safe_msr_write, @function
r4os_safe_msr_write:
    movl %edi, %ecx
    movl %esi, %eax
    movq %rsi, %rdx
    shrq $32, %rdx
.global r4os_safe_msr_write_fault
r4os_safe_msr_write_fault:
    wrmsr
    xorl %eax, %eax
    ret
.global r4os_safe_msr_write_resume
r4os_safe_msr_write_resume:
    movl $1, %eax
    ret
.size r4os_safe_msr_write, .-r4os_safe_msr_write
