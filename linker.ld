/* R4OS kernel linker script: higher-half x86_64 for Limine
 *
 * LLD ignores SECTIONS order for orphan sections and sorts them
 * alphabetically. R4OS uses Limine request section names that make alphabetical
 * order match the required order:
 *   .limine_reqs_a_start  <  .limine_reqs_m_data  <  .limine_reqs_z_end
 */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(kmain)
