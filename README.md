# R4OS Kernel

The production x86_64 kernel is implemented in Rust and the required assembly
routines. It boots through Limine in long mode. External R4M0 applications,
libraries, drivers and protocols, and the separate Recovery kernel retain
their existing languages and public interfaces.

In the R4OS workspace, build with:

    ./Repositories/Kernel/Build.sh

On Windows use `Repositories\Kernel\Build.bat`. Both run the same portable
PowerShell 7 builder. The isolated DevKit starter selects Rust 1.99.0 and
`x86_64-unknown-none`. Cargo uses the locked canonical Contract dependency;
the builder rejects drift from the local Contract schema and Rust bindings.
No Zig compiler, source facade or link stage belongs to this kernel build.

The fixed release profile uses optimization level 3 with overflow checks,
debug assertions and panic=abort. Normal kernel code uses no SIMD or red
zone. Existing assembly and task/IRQ guards preserve SIMD-capable module
calls. The linker preserves the higher-half layout, Limine requests and the
44-byte version metadata. `VERSION.R4S` is the product version.

`BuildOptions.json` owns supported options, including explicit detailed
metrics and fault probes. Defaults keep intrusive probes disabled.
Since0.87.27, -Dheap-poison=true explicitly enables the full0xdd heap release
fill, independently of metrics. Default=false preserves functional boundary
tags, size/owner checks, retiring identities and release acknowledgments.
`Build.sh fmt`, `check` and `clippy` provide current source/compiler checks.
Runtime acceptance uses targeted existing diagnostics and QEMU SMP4.
Retired Zig host selectors and temporary fixtures remain in Git and the
migration evidence; they are not a second supported build.

`Rust/main.rs` enters `Rust/boot/entry.rs`. Subsystem owners live under
`Rust/arch`, `boot`, `memory`, `sched`, `driver`, `storage`, `fs`, `loader`,
`program`, `service`, `usb`, `net`, `input`, `font`, `display`, `gfx`, `audio`
and `update`. Assembly remains in `arch/x86_64` and the bounded native task/
module helpers. Each mutable state has one owner; existing generation,
unwind, lock rank and explicit completion rules remain binding.

The artifact contract stays `zig-out/bin/r4os.elf`; `r4os.build.json` binds
its exact bytes to compiler, profile and options. Cargo intermediates live
under `Artifacts/Kernel/Rust` in the workspace. A new image must embed that
exact ELF before guest tests. Spectra STANDARD is the sole standard
performance benchmark; it retains the fixed reference and unchanged loads.

Workspace references: `Agents/KernelStruktur.txt`, `Agents/Konventionen.txt`,
`Docs/Core/KernelRustBuild.txt`, `KernelRustMigration.txt`, `KernelRustFinal.txt`,
`ProgramLifecycle.txt` and `Speicherverwaltung.txt`. Detailed German overview:
[DOCUMENTATION.de.txt](DOCUMENTATION.de.txt). History and original provenance
remain in Git and [PROVENANCE.txt](PROVENANCE.txt).
