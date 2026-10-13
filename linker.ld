/* R4OS kernel linker script: higher-half x86_64 for Limine
 *
 * Retain the existing Limine request section names:
 *   .limine_reqs_a_start, .limine_reqs_m_data, .limine_reqs_z_end
 * Names alone do not establish physical ELF order. The frozen Zig ELF and
 * current mixed ELF have the same orphan order; validate actual protocol
 * handoff/retention instead of inferring it from alphabetical names.
 */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(kmain)
