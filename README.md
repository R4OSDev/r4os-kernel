# R4OS Kernel

Kernel 0.1.151 includes display drivers in the warm-reset handoff. It drains
presentation before driver shutdown, keeps output admission closed through
restoration, and requires released graphics ownership and stopped callbacks.
An unproven handoff enters poweroff (or halt if poweroff is unavailable); no
retained GPU firmware memory is handed to a warm-started kernel.

Kernel 0.1.147 uses the boot configuration owner's driver capacity for its
load plan. All twelve configured entries can reach normal driver admission;
the former independent eight-entry plan skipped a ninth driver even when
earlier optional entries had no matching hardware. Registry admission and
per-driver initialization remain responsible for actual activation.

This repository contains the x86_64 R4OS kernel, Limine boot integration,
required built-in facilities, and kernel-specific tests. The kernel consumes
the separate platform Contract and does not define optional Runtime-R4L APIs.

The early framebuffer boot screen reserves two bounded status rows below its
progress bar. Row one is updated before each potentially blocking boot step
and names every configured R4D before load and initialization. Row two stays
blank outside service autostart, where it transiently names the service being
started and can distinguish path validation, R4X loading, successful spawn and
the completed service plan. Bounded ServiceManager lifecycle markers then
cover foreground program return through handback to the boot launcher. The
configured shell launch also reports path, loader, stack, registry, task,
publication and R4XStart boundaries. The first recognized actionable error
replaces that detail and is preserved. Both rows are fully cleared on every
redraw; fatal reports and the crash screen retain ownership of the complete
failure diagnosis.

Shell path resolution distinguishes `FS-Sperre wartet` (another request owns
the boot-volume lane) from `Dateipfad suchen` (the VFS/NTFS lookup is active).
The immediate probe is observational; normal bounded gate acquisition and
launch behavior remain unchanged.

An occupied lane is attributed only after pinning its exact task generation.
The visible `MODULE: ACTIVITY` names the R4X owner and preferably its scheduler
wait reason; `FS-Halter fehlt` denotes an owner generation that cannot be
pinned. Numeric owner and request-kind evidence remains in the boot log.

## Build and validation

On Windows:

    Build.bat test

On Linux or macOS:

    ./Build.sh test

`Settings.R4S` maps the Contract, DevKit, and artifact paths. The kernel
build is ReleaseSafe and does not enable SIMD.

The platform exposes a continuous nanosecond clock independently from the
scheduler event source. A common clocksource uses an invariant TSC with an
exact CPUID frequency, or an independently HPET-calibrated and watched TSC.
Per-CPU HPET correlations compensate bounded TSC offsets; an unstable
frequency, excessive CPU skew, or a later discontinuity demotes every CPU to
the free-running HPET source. A global monotonic clamp protects concurrent
readers. Logical scheduler ticks use the same nanosecond epoch and a
precomputed multiply/shift conversion, so the normal TSC path performs no
HPET MMIO access or division. Periodic HPET/LAPIC delivery and their one-shot
idle modes remain event sources; PIT remains the explicitly degraded periodic
fallback. R4OS currently has no suspend/resume lifecycle; adding one requires
clocksource requalification before tasks resume.

PCI and PCIe devices are enumerated once through a canonical inventory.
Mapped segment-0 ECAM coverage is preferred; legacy CF8/CFC access is retained
only as a bounded fallback for missing or uncovered buses. Stored class fields
serve inventory searches without additional configuration-space reads.
The existing owner-bound `pci_enable_msi` operation prefers conventional MSI
and now falls back to exactly one MSI-X table entry when that is the only
message-signalled capability. Table geometry is bounded by the six PCI BARs
and the 16-MiB MMIO policy; disable and owner cleanup restore both the original
entry and endpoint control state. This does not introduce a public affinity or
multi-vector contract.

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

A USB boot deadline disables local interrupts only while sampling its clocks
and restores them before the block worker can park. Once the
task runtime is complete, the already-running `kernel-main` task explicitly
enables interrupts because it does not pass through the trampoline used by
new tasks. Timer-driven USB completions and watchdog wakes therefore continue
while the launcher is the only other runnable task.

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

File-backed R4M0 loads use one allocation-free reader with two 4-KiB metadata
windows. Header, tables, names, metadata and relocation records share this
fixed 8-KiB budget while section payloads stream directly into their final
image. Validated imports and exports are retained in the load plan, eliminating
duplicate table and string reads without changing record, error or publication
order. Installed disk R4P modules are still catalogued from header and metadata
only; their complete image, relocations, ABI checks, dependencies, and
initialization run on the first actual role use. Required USB boot protocols
retain their explicit eager preload path.

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
ordinary launches do not add serial traffic. Virtual-range IDs are monotonic
for the boot, so an ownership-preserving stack-release retry treats `NotFound`
as acknowledgement of an already completed release instead of requeueing the
same retirement forever.

The stable task registry is an ownership and inventory index, not a run queue.
Ready selection, timed wakeups, and deferred reaping use separate intrusive
projections, so their hot paths scale with the relevant work set. Finite waits
form a stable ordered deadline queue; one timer IRQ publishes at most 64 due
wakes and leaves a visible backlog for the next delivery. Equal deadlines keep
enrollment order, cancellation unlinks the exact waiter, and hardware horizons
are crossed through bounded one-shot checkpoints. A productive BSP restores
periodic delivery on task handoff, no-op yield, and a final idle one-shot IRQ,
so a suspended idle continuation cannot strand later timed waits. R4X tasks
also carry an immutable direct execution-owner binding; timer IRQ attribution does not scan
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

A naturally returned ProgramThread transfers Task storage to the scheduler
reaper through `exitCurrentAndRetire`. It keeps ownership of its kernel exit
epilogue until it has released the generation-checked execution pin and that
reaper has removed the exact Task generation. The program reaper waits instead
of killing or claiming the `exited` owner while either boundary remains;
hard-killed threads remain program-reaper-owned. This separates Task release
from ProgramThread and payload teardown across the terminal context switch.
The fixed stream-slot table publishes a generation-checked owner-to-volume
projection. Stream teardown therefore takes no filesystem gate for an owner
without leases and tries only each volume that actually contains one of its
slots. A busy relevant lane defers retirement without blocking the program
reaper; unrelated filesystem work no longer participates. The same projection
serializes fixed-slot reservation across concurrent volumes.

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

DriverApi28 binds external native display drivers through
`display/native_driver.zig`. Fixed boot geometry uses a retained common WB BO,
explicit CPU-write leases, sparse source-upload reservations and the existing
device-execution fence/worker. Driver-specific commands stay in R4D. A failed
upload follows the same acknowledged boot restoration path; unknown physical
completion retains ownership. An IRQ-safe, generation-bound notification
mailbox can wake an idle native backend without acquiring BO metadata locks.
MMIO UC mappings locate an actual PAT UC entry; the legacy boot-FB WC helper
cannot change native BO cache policy.

DriverApi30 adds an optional resident CPU heap for external R4Ds. Allocation
and release use the existing kernel heap outside the runtime section that
protects per-start metadata. Intrusive allocation records grow with actual
backing and provide expected logarithmic handle lookup. Worker callbacks can
use a cached table without joining the global driver lifecycle guard. Closing
rejects new allocations; generic cleanup reclaims leftovers only after device
callbacks, IRQ, work and DMA have quiesced. Failed release retains ownership
and vetoes unload. The CPU addresses carry no DMA or GPU mapping promise.

DriverApi31 also exposes the existing fixed MonotonicClockInfo snapshot to
drivers. kernel/monotonic_api.zig is the shared R4SYS/R4D mapping of the
canonical monotonic source. The optional entry preserves the 608-byte prefix;
no new clock, timer backend or scheduling policy is introduced. Reads do not
acquire a driver lifecycle guard, allocate or wait.

DriverApi32 appends a dedicated driver-task query to the preserved 616-byte
prefix (total 624). kernel/driver_threads.zig owns scheduler integration;
driver_thread_owner.zig owns the dynamic intrusive identity index. Tasks have
guarded stacks, full module FPU state and an execution/unwind owner installed
before publication. Default placement is BSP; audited module code may opt in
to SMP. The service never acquires the global R4D lifecycle guard. Finite joins
retain their target; cooperative stop wakes only service-owned waits.

Parent close rejects starts, wakes sleepers/joiners and requires returned
callbacks before generic backend/IRQ/work teardown. Completed Task generations
and stacks retire before record backing is freed, then DMA/CPU cleanup may
proceed. Failed construction/release retains ownership and vetoes unload.
Heap and task operations occur outside runtime metadata sections. The small
storage-callback identity projection also uses that boundary so parallel CPU
callers can safely enforce the existing storage exclusion. Driver logs publish
one bounded complete record with the actual callback owner. Legacy device and
filesystem APIs gain no new parallel admission.

DriverApi33 adds resident counting semaphores at offset 624 (total 632).
kernel/driver_semaphores.zig binds dynamic records to actual R4D starts and
uses the existing scheduler Semaphore/WaitQueue FIFO handoff. Allocation,
free and parking occur outside runtime metadata sections. Wait leases and
unwind lifetime guards retain backing across actual waits; failed free keeps
the same record. There is no fixed semaphore slot pool.

Zero timeout is an IRQ-safe try; release grants exactly one permit. Blocking
operations require a sleep-capable task. Parent close refuses creation but
never fabricates permits or cancels uninterruptible waits. Existing operations
remain available for ordered shutdown. Generic cleanup follows IRQ/work/task
quiescence and precedes DMA/CPU heap cleanup. Semaphore ownership is a counter,
not Task mutex ownership: drivers must quiesce every user before destruction.
The existing semaphore handoff guard is now also dropped under the runtime
owner. No legacy PCI/backend API gains parallel admission.

The semaphore contention guest exposed missing AP-to-BSP deadline notification:
AP waits could publish an earlier deadline while the BSP slept against a
later one-shot. scheduler.blockCurrent now sends the existing reschedule IPI
to the BSP only for an earlier finite deadline, under the same publication
owner. The existing STI/HLT-safe idle path rechecks the minimum and programs
its own timer; APs never write the BSP timer. The regression retains its
original five-second bound.

Dedicated driver Tasks may opt into synchronous self-abort with flag 2.
The version-1 DriverThreadApi has `abort_current` at offset 72 and
`current_request` at offset 80, size 88. Query accepts the full legacy
72-byte prefix, copies 72 for capacities 72..79, 80 for 80..87 and 88 from
88 upward. Old callers' tails are untouched.
This thread-service extension retains DriverApi33/632. Flag 4 reports an accepted abort and
is rejected as a start flag.

`arch/x86_64/callback_abort.zig` implements one ordinary SysV call boundary,
saving the caller stack, callee-saved GPRs and MXCSR/x87 control state. It
clears DF before return and uses the target ABI on either build host. The
live frame belongs to the actual Task, with no per-CPU identity guess or
shared frame pool. This is original R4OS assembly, not a setjmp translation.

`driver_threads.abortCurrent` admits only negative self-aborts on opted-in
Tasks with interrupts enabled, no held kernel lock/runtime section, no wait
lease and exactly their initial unwind guard. Runtime metadata publication
ends before transfer. The normal threadMain epilogue still owns completion,
waiter notification and retirement of the exact Task generation and stack.
Module defers are bypassed; resource recovery remains the native owner's
responsibility after peers quiesce. No arbitrary CPU exception, hung callback,
kernel critical section or GPU failure is caught or forcibly terminated.

Kernel 0.1.146 derives `current_request` from the actual executing dedicated
Task under the existing Runtime owner and verifies driver ownership. The
immutable original handler/context and creation flags are returned without
an allocation, wait or additional unwind guard. Status flag 8 is computed
under that same owner from the backing Task's blocked state and membership
in this service's sleep queue. It clears as the Task becomes runnable and
is rejected as a creation flag. All callbacks and pointer lifetimes remain
with their existing owners; no generic native invocation data enters the
kernel. The NVIDIA driver owns its absolute phase deadlines.

Kernel 0.1.148 provides DriverApi34 byte-range DMA synchronization. The
unchanged v33 prefix is followed by two callbacks at 632/640 (648 bytes total).
`kernel/dma_sync_range.zig` copies only the requested bounce-buffer bytes and
preserves the existing x86 ordering and direction rules; whole-map sync uses
the same helper. The driver API validates real owner, descriptor and retained
mapping bounds before dispatch. No allocation, wait or new parallel DMA
admission is introduced. Callers own affected bytes, publication order and
device quiescence. One owner test covers independent peer fields, invalid
extents, copy directions and direct coherent mappings; the normal EXAMPLE
fixture exercises the actual API in a short SMP4 boot.
