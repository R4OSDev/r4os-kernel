Public filename-provider fixture
===============================

Build this module with the normal SDK manifest-project builder. Its
image_scope is none: it is an explicit C guest fixture, not an installed app
or periodic product test. It requires the current R4SYS filename-limit tail.

Run NAMES27A.R4X only in a disposable private Test image with its normal
NTFS C:/D:, FAT32 E: and hidden C:/BOOT routing. The fixture owns transient
N27* names there; do not use a volume containing user-owned N27* files.
Mount slot/generation are observed, not hard-coded. No physical device access
or original external Exchange-provider identity is inferred from this image.

The public SDK supplies actual CREATE/write/finish/readback, independent
byte/UTF16-unit bounds, untouched query outputs on failure, short-stage
atomic replacement and document saving to long targets. Rejected long
atomic stages must preserve target, stage and backup sentinel bytes.
C/Rust SDK path-syntax outcomes are printed separately from raw provider
results. A zero process result plus the explicit failures=0 marker is required.

0.88.27: Kernel420/SDK109/Contract57, private KVM4:40 name vectors,
12 atomic cases,4 real document saves,215 checks; no failures.
Windows and physical storage hardware were not exercised by this fixture.
