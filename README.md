# R4OS Kernel

The production x86_64 kernel is implemented in Rust and the required assembly
routines. It boots through Limine in long mode. External R4M0 applications,
libraries, drivers and protocols, and the separate Recovery kernel retain
their existing languages and public interfaces.

Kernel412 adds generation-bound audio playback attribution under the existing
stream/output owners. `Rust/audio/playback.rs` stores bounded scalar source
intervals; `Rust/audio/core/playback_progress.rs` maps backend/DMA/consumed and
unconfirmed progress, source age/deadlines and final Close snapshots. R4AUDIO3
extends its unchanged200-byte prefix to224 bytes. No extra worker thread or
global scheduler admission is added. Contract, ownership and hardware evidence
are documented in the workspace `Docs/Audio/PlaybackProgress.txt`.

Kernel427 separates a SID source's acknowledged Stop from the shared PCM
output's Idle completion. A failed private Stop retains pending PCM; an
acknowledged source stays quiet across Busy/error retries. Actual other PCM
or MIDI producers keep their output. The last foreign PCM Close may finish
the output even while the quiet SID handle remains acquired. Private functional
output revisions prevent an old Stop retry from idling a successor output.
There is no new public API, periodic sampler, worker or hardware owner.
`Tests/Guest/SidStop` builds explicit native callback/stream regressions outside
normal images; their frozen binaries fail eight checks on426 and pass on427.

Kernel428 binds exclusive SID/MIDI sessions to full process generations and
exact synth registration generations. Reaper action9 finishes only its own
session before PCM cleanup. Separate try-only SID and MIDI/OPL3 execution owners
retain provider callbacks; active sessions veto driver closing/unregister/unload.
Stop, common Idle and private Release errors retain their actual obligations.
MIDI Close preserves a still-producing SID source. Public ABI layouts, the
explicit internal KERNEL owner and global OPL3 register use remain unchanged.
`Tests/Guest/SynthSessions` reproduces real process exit/kill/stack-guard recovery,
threads, callback failures and a separately scoped private R4D lifecycle bridge.
The normal product contains no bridge include/call or extra periodic observer.
Physical HDA/queued-PCM/last-Close evidence and its limits are in the workspace
`Docs/Audio/README.txt` release0.88.52 entry.

Task/thread inventory pages stage native values and preserve their final
epoch checks under the original owner, then release that owner before writing
caller rows, cursor or page replies. Demand-committed caller memory can fault
on first write and requires ordinary pager admission. Native410 reproduced a
hang with a fresh resident-zero caller buffer. Current412 passes the original
KVM4 kernel/subsystem gates and real task/thread demand-fault cases; native16
also passes those fresh buffers and both old failing concurrent TASKS+SLEEP
and TASKS+DESKTOP pairs with complete replies and subsequent SSH liveness.
The bounded regression is part of Spectra79 CHECK VM /INVENTORY and default VM.

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
