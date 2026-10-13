# Third-Party Notices

The kernel implements the public Limine boot protocol and uses protocol
constants and layouts maintained independently by R4OS. This repository does
not bundle the Limine bootloader binary or upstream Limine source code.

The Limine project is available at
https://github.com/limine-bootloader/limine under BSD-2-Clause. Bootloader
binaries used by a complete R4OS image are supplied by the DevKit and their
license is included by the Distribution repository.

During the Rust migration the kernel links the `core` and compiler runtime
provided by the pinned Rust 1.99.0 `x86_64-unknown-none` toolchain. These retain
their upstream Rust/LLVM and per-file licenses; they are not relicensed as
original R4OS code. The complete upstream library copyright/license report
is distributed as `RUST-1.99-LIBRARY-LICENSES.html` under `R4OS/LICENSES`.
Source/toolchain provenance: `https://github.com/rust-lang/rust/tree/1.99.0`
and the DevKit's checksum-pinned Rust toolchain. Host-only standard-library
code used by `build.rs` is not part of the kernel.
