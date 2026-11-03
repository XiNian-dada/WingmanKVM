# Fork modifications

Baseline: jpeg-encoder 0.6.1, crates.io checksum
`b454d911ac55068f53495488d8ccd0646eaa540c033a28ee15b07838afafb01f`.
The original authors, MIT/Apache licenses, IJG notice and upstream README are retained.
This software is based in part on the work of the Independent JPEG Group.

Only three upstream source files changed:

- src/lib.rs: declare ARM64 modules, scoped unsafe availability for those modules, diagnostics exports and backend selection type.
- src/encoder.rs: backend selection, typed block loading and fused transform/quantization calls. Reference backend retains the original scalar formulas and entropy coding.
- src/quantization.rs: add compact ARM64 reciprocal/correction arrays derived from the unchanged original tables.

New code: src/neon_dct.rs (integer DCT, exact constant-block shortcut, checked SIMD sample loads), src/neon_quant.rs (exact Q15 quantization), src/diagnostics.rs (benchmark-only APIs), parity tests, offline benchmarks and mathematical/provenance checks.

Nine other upstream Rust source files remain byte-identical. UPSTREAM_SHA256.json and scripts/verify_upstream.py make this boundary reviewable. This is a research fork, not an unmodified upstream release or a newly invented DCT factorization. No component is deployed or published by these changes.
