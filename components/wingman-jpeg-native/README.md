# Wingman JPEG native

Bounded production wrapper around pinned **libjpeg-turbo 3.2.1**. The static
library is compiled into the application; no system library is replaced.

Modes: standard accurate integer encoding, opt-in exact-flat encoding,
coefficient-domain coarsening, and source-table preservation with optimized
Huffman coding. Grayscale and optional entropy-table optimization are explicit
options. Preserve mode always optimizes entropy tables and ignores quality.

Pixel encoders produce 4:4:4 color. Coefficient modes retain source sampling;
grayscale retains luma only. Native grayscale uses JPEG luma, which can differ
slightly from the portable compatibility path's RGB grayscale definition.
Coefficient mode falls back to screen encoding for unsupported source tables or
color spaces. Preserve mode returns an error rather than silently requantizing
an unsupported format. It supports common 8-bit YCbCr and grayscale JPEGs.

Limits: 32 MiB input, 64 MiB output, 8192 pixels per edge, 33,177,600 total pixels,
64 input scans, 256 MiB virtual-array memory budget per JPEG object. Native
progress callbacks also enforce a processing deadline of approximately 3–4 s.
These are limits for a capture-frame processor, not a process-wide memory quota.

All callbacks and `setjmp`/`longjmp` remain in C; no jump crosses Rust frames.
Cleanup flags and buffers live in a heap context so they remain valid after a
jump. Invalid/corrupt frames return an error and can be skipped without exiting
the service. Each call owns its state; no mutable process-wide error handler or
private-ABI function-pointer hook is used. Successful C allocations are copied
into a fallibly reserved Rust vector and freed through their originating API.

The only upstream source modification is generated into Cargo's build directory
by `build.rs`: an opt-in branch in `src/jcdctmgr.c`, enabled only for this
wrapper's screen compressor. The original archive stays byte-identical. The
flat-block and NEON requantization kernels come from the separately tested
Wingman research code and retain its exact integer formulas.

```sh
cargo test --locked --release
```

Build prerequisites: C compiler, make, CMake >=3.15 and tar. Cargo's target
compiler is used for cross builds. ARM builds require the library's SIMD code;
other development targets can build without NASM. Build parallelism is capped
at two jobs. The main application pins all dependencies in its root lockfile.

Provenance and upstream licenses are in [THIRD_PARTY.md](THIRD_PARTY.md).
