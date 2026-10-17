/* Higher-half Limine image. Retain markers and requests in actual protocol
 * order, writable bootloader responses and separate RX/RO/RW load segments. */
OUTPUT_FORMAT(elf64-x86-64)
ENTRY(kmain)
PHDRS {
    requests PT_LOAD FLAGS(6);
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}
SECTIONS {
    . = 0xffffffff80000000;
    .limine_reqs_a_start : ALIGN(8) {
        KEEP(*(.limine_reqs_a_start))
    } :requests
    .limine_reqs_m_data : ALIGN(8) {
        KEEP(*(.limine_reqs_m_data))
    } :requests
    .limine_reqs_z_end : ALIGN(8) {
        KEEP(*(.limine_reqs_z_end))
    } :requests
    . = ALIGN(4096);
    .text : {
        __r4os_kernel_text_start = .;
        *(.text .text.*)
        __r4os_kernel_text_end = .;
    } :text
    . = ALIGN(4096);
    .rodata : { *(.rodata .rodata.*) *(.data.rel.ro .data.rel.ro.*) } :rodata
    .r4os.kernel.meta : ALIGN(4) { KEEP(*(.r4os.kernel.meta)) } :rodata
    . = ALIGN(4096);
    .data : { *(.data .data.*) *(.got .got.*) } :data
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) } :data
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr .comment .note.*) }
}
