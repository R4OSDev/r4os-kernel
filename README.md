# R4OS Kernel

This repository contains the x86_64 R4OS kernel, Limine boot integration,
required built-in facilities, and kernel-specific tests. The kernel consumes
the separate platform Contract and does not define optional Runtime-R4L APIs.

## Build and validation

On Windows:

    Build.bat test

On Linux or macOS:

    ./Build.sh test

`Settings.R4S` maps the Contract, DevKit, and artifact paths. The kernel
build is ReleaseSafe and does not enable SIMD.

The platform exposes a continuous nanosecond clock independently from the
scheduler event source. Exact invariant-TSC and free-running HPET sources are
preferred. On the modern timer path, logical scheduler ticks continue from the
HPET main counter while active work uses periodic HPET or LAPIC delivery. True
idle programs only the earliest finite wait as an HPET or calibrated-LAPIC
one-shot; no pending deadline disarms the event source. PIT remains the
explicitly degraded periodic fallback.

PCI and PCIe devices are enumerated once through a canonical inventory.
Mapped segment-0 ECAM coverage is preferred; legacy CF8/CFC access is retained
only as a bounded fallback for missing or uncovered buses. Stored class fields
serve inventory searches without additional configuration-space reads.

Normal kernel artifacts perform only the non-mutating heap structure check
and the required kernel-space page-table dry run. The invasive heap,
page-table, synchronization, and scheduler probes are available only in an
explicit diagnostic artifact:

    Build.bat -Dboot-selftests=true

Once the shell task has been admitted, the one-shot kernel boot task exits and
is reaped. The shell's first `boot_ready` call independently freezes the boot
measurement and retires the boot renderer without repainting the ready shell
surface.

The stable task registry is an ownership and inventory index, not a run queue.
Ready selection, timed wakeups, and deferred reaping use separate intrusive
projections, so their hot paths scale with the relevant work set. Finite waits
form a stable ordered deadline queue; one timer IRQ publishes at most 64 due
wakes and leaves a visible backlog for the next delivery. Equal deadlines keep
enrollment order, cancellation unlinks the exact waiter, and hardware horizons
are crossed through bounded one-shot checkpoints. R4X tasks also carry an
immutable direct execution-owner binding; timer IRQ attribution does not scan
ProgramThread or asynchronous-I/O registries. A wake of a higher priority task
requests rescheduling, but the switch is consumed only at a safe IRQ exit after
wait-queue critical sections have been released.

Detailed German migration notes are preserved in
`DOCUMENTATION.de.txt`.

## License

Original R4OS material is licensed under Apache License 2.0. See `LICENSE`,
`NOTICE`, and `THIRD_PARTY_NOTICES.md`.
