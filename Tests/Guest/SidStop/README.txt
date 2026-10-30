SID Stop: actual Kernel Core and shared output ownership
=======================================================
Explicit private native guest fixture, IMAGE_SCOPE=none. No product hook, normal
module catalogue entry, periodic observer or second model of PCM open counts.
Build.ps1 uses the current SDK ManifestProject on both hosts and writes projects,
caches and artifacts under Temp/SidStopFixtures by default.

Build a private SMP4 Test image containing protocol/P52C.R4P at
/R4OS/PROTOCOLS/P52C.R4P, driver/P52D.R4D at /R4OS/DRIVERS/P52D.R4D and
app/P52APP.R4X at /TEMP/P52APP.R4X. Use DRIVER=P52D and disable HDA, AC97,
SID, OPL3 and MIDI only in this private CONFIG. The driver registers actual
Core PCM and SID/OPL3 callbacks, not physical hardware. Keep AUDSVC active.
No kernel source injection or public API extension is needed.

Run C:\TEMP\P52APP.R4X followed by one mode per process:
 S: two actual Core/AUDSVC PCM streams remain open and continue while SID stops;
 P: failed private Stop retains one rendered pending frame for exact retry;
 I: last-source Idle Busy is propagated, private ACK retained, final/replay exact;
 E: same with real callback error -66 rather than Busy;
 N: invalid register preserves ACK, accepted register creates new private work;
 O: real output selection retires old output before a pending Stop retry;
 R: actual same-config SID provider replacement invalidates the private ACK;
 M: real MIDI-rendered PCM with zero ordinary streams continues after SID Stop.
Each process must exit0 and print one detailed callback observation and result=PASS.
The shared case opens both low-level R4AUDIO and materialized R4SYS/AUDSVC streams.
No fixture-maintained open count controls any assertion. Provider status performs
idempotent observer binding after the lazy R4P catalogue is actually available.

The private protocol borrows the driver's resident state until unregister; it
never retains app stack or transfer buffers. Controls inject real callback
returns. Counters are atomically copied by the explicit observer only. Output
selection records the controlled provider's own Stop ACK before changing index.
No controlled Stop or cursor counter proves physical DMA or audible completion.

0.88.52: identical frozen binaries on unchanged Kernel426 fail all eight detailed
checks. Kernel427 must pass them, alongside the separate real HDA/SID/SYNTHD and
parallel raw/AUDSVC PCM qualification. Preserve full source/binary/readback hashes,
original negative receipts and actual process exit. Normal product builds do not
compile or install these fixtures; test media must be shut down and removed.
