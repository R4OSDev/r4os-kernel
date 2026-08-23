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
periodic PIT, HPET, or LAPIC scheduler event source. Exact invariant-TSC and
free-running HPET sources are preferred; hardware without either capability
uses an explicitly degraded periodic fallback.

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

Detailed German migration notes are preserved in
`DOCUMENTATION.de.txt`.

## License

Original R4OS material is licensed under Apache License 2.0. See `LICENSE`,
`NOTICE`, and `THIRD_PARTY_NOTICES.md`.
