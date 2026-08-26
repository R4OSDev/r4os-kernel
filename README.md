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

The controller-parallel block-dispatch and resident direct-buffer runtime
acceptance also covers the asynchronous depth-two request contract, flush
ordering, cancellation/reset, unload/kill vetoes, and stale completion. It can
be enabled independently of the other invasive probes:

    Build.bat -Dblock-dispatch-selftest=true

On Linux or macOS use `./Build.sh -Dblock-dispatch-selftest=true`.

DriverApi v19 adds owner-bound pin/map/sync/unmap segment DMA for existing
resident buffers. Version 20 adds bounded audio-refill requests with an
absolute tick deadline, device serialization key, and a separate EDF queue
served by one budgeted short-completion worker. Normal IRQ/task Driver Work
keeps its fair FIFO and reserved progress. StorageBackend v2 separates
nonblocking submit and exact completion while retaining the version-1
synchronous depth-one adapter. The canonical lifetime rules are in the
Contract repository's `ABI/R4DDriver.txt`.

DriverApi v21 makes XHCI.R4D the activation owner of one kernel-resident xHCI
backend instead of a second hardware implementation. UsbHostController v2
dispatches port, control, bulk, interrupt, recovery and poll operations and
reports capabilities and activity. Endpoint-bound generation handles allow a
pending HID transfer and storage transfer to coexist; the event ring wakes by
INTx when routed and retains bounded polling as fallback. Bulk TDs span up to
64 KiB in page-bounded TRBs, including a chained ring wrap. Failed controller
halt vetoes unload.

DriverApi v22 admits one owner-bound synchronous display-blit backend.
R4DRAW normalizes complete XRGB32 generations with at most eight regions
before calling it. The backend borrows every address only for that callback;
an absent, incompatible or failed backend causes one complete boot-framebuffer
CPU copy, while DisplayManager alone owns present statistics and fences.

DriverApi v23 adds an IRQ-safe adapter RX-work notification. Network IRQ
handlers acknowledge and classify bounded device causes only. The `net-rx`
task polls the published adapter, copies frames into a fixed 64-slot queue and
runs protocol work in batches of at most 32. Queue backpressure leaves the
device entry owned by its driver. Event wakeups remove normal 10-ms poll
latency; the timer remains a routing watchdog. Accepted, processed, cancelled
and occupied ownership plus queue, batch and tail-latency counters are exposed
through the NETRX diagnostic snapshot.

NetBackend v2 and DriverApi v24 negotiate queue count, ownership, segments,
checksum, VLAN/segmentation, moderation and completion metadata without
changing the v1 prefix. The BSP implementation selects one queue and only
validated RX TCP/UDP checksums. Every metadata packet still carries canonical
flat bytes; unknown or rejected fields take the byte-identical software path.

File-backed R4M0 relocation tables are streamed in record-aligned 4,080-byte
windows while preserving record and error order. Installed disk R4P modules
are catalogued from header and metadata only; their complete image,
relocations, ABI checks, dependencies, and initialization run on the first
actual role use. Required USB boot protocols retain their explicit eager
preload path.

R4SYS retains a validated R4R1 hive view for its complete file generation.
Ordinary Registry reads use the resident immutable bytes without heap churn,
file reload, or repeated full validation. A separate transaction gate builds
the next generation in an inactive fixed slot, verifies the staged bytes, and
uses the filesystem's atomic target/backup ownership transfer. Readers remain
on the previous complete generation until the installed target is verified;
definite failures publish nothing and ambiguous completions must reconcile
before later Registry work proceeds.

Once the shell task has been admitted, the one-shot kernel boot task exits and
is reaped. The shell's first `boot_ready` call independently freezes the boot
measurement and retires the boot renderer without repainting the ready shell
surface.

Synthetic kernel-thread contexts preserve the SysV x86_64 call boundary: after
the context switch restores six registers and enters the common trampoline by
`ret`, its stack pointer is 8 modulo 16. A reserved word below the aligned
stack top supplies the call-shaped entry layout, and a build-time layout test
keeps the assembly restore frame and Zig stack construction in agreement.

Kernel-task and R4X program stacks now carry one-lifetime canary high-water
telemetry plus TSC creation/release costs. The Test guest measured at most
39,800 of the 65,536 committed kernel-stack bytes, so the kernel size, eight
cached stacks and four critical reserves stay unchanged. Program profiles use
the measured reserves: normal/service/desktop reserve 4 MiB and
large-service/build-tool 8 MiB, with 64- or 128-KiB initial commits. Tiny stays
at 2 MiB and unmeasured browser/workstation profiles stay at 32 MiB. All retain
the moving guard and 64-KiB commit growth. Profile/role aggregates update
atomically on SMP. The instrumented R4BASIC acceptance emits bounded
`[R4XSTACK]` records with owner, module, profile, reserve, commit, high-water,
cycles, cache and critical occupancy at normal return and common teardown;
ordinary launches do not add serial traffic.

The stable task registry is an ownership and inventory index, not a run queue.
Ready selection, timed wakeups, and deferred reaping use separate intrusive
projections, so their hot paths scale with the relevant work set. Finite waits
form a stable ordered deadline queue; one timer IRQ publishes at most 64 due
wakes and leaves a visible backlog for the next delivery. Equal deadlines keep
enrollment order, cancellation unlinks the exact waiter, and hardware horizons
are crossed through bounded one-shot checkpoints. R4X tasks also carry an
immutable direct execution-owner binding; timer IRQ attribution does not scan
ProgramThread or asynchronous-I/O registries. Kernel owners assign the internal
roles input, short completion, interactive, and batch; applications cannot
select scheduler policy. Input and completion boosts have per-activation tick
and dispatch budgets and are demoted to interactive rank after exhaustion.
Directed single wakes prefer the most urgent role while preserving FIFO order
inside that role; drain and cancellation paths remain FIFO. A more urgent wake
requests rescheduling, but a switch is consumed only after queue and owner
state is published, either at a lock-safe synchronous return point or at the
existing post-handler/EOI IRQ boundary. Bounded mutex role donation covers
short inversions without creating an unbounded high-priority lane.

Console input can use an optional generation-bound wait while legacy key polls
and bulk reads remain available. Console output is retained in sealed source
blocks referenced by both the visible host transcript and an owned completion;
each transcript keeps its own 16-KiB boundary, while revisions and desktop
signals are published once per complete write batch.

Detailed German migration notes are preserved in
`DOCUMENTATION.de.txt`.

## License

Original R4OS material is licensed under Apache License 2.0. See `LICENSE`,
`NOTICE`, and `THIRD_PARTY_NOTICES.md`.
