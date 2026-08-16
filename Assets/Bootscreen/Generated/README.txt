Generated Bootscreen Artifacts
==============================

This directory is the output location for generated bootscreen artifacts.
The build converts Assets/Bootscreen/BOOTSCREEN.BMP into BOOTSCREEN.R4B.
The R4B file is a build artifact and is not versioned.

A normal build must generate the artifact. Missing or invalid BMP input is a
hard build failure; no stale bootscreen fallback is used.

The build then copies BOOTSCREEN.R4B into kernel/generated because Zig
requires embedded files to be inside the kernel package path. That copy is
also an unversioned build artifact.
