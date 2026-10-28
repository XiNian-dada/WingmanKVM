# Temporary DTLS security compatibility patch

Upstream: `dtls` 0.17.2, https://crates.io/crates/dtls/0.17.2

- Source repository: https://github.com/webrtc-rs/webrtc
- Source commit: `1da3fe8ee84db8d266322cbcfcb357c745959a8e`, directory `dtls/`.
- Registry archive SHA-256: `01a431e87fc386bd5e02deb554a013f97bc406e47a7d8e97efb7c6366b980e9c`.
- Obtained with `cargo info dtls@0.17.2`. Copy `src/`, normalized `Cargo.toml`,
  README, both licenses and `.cargo_vcs_info.json` from the Cargo-verified archive.
- `upstream-manifest.json` records the original SHA-256 of each copied file.
  Cache markers, the dependency's independent lockfile and CI files are omitted.

## Why this patch exists

WebRTC 0.17.2 preserves the existing UDP-mux API and removes the unmaintained
bincode dependency. Its DTLS crate still pins P-256/P-384 0.13, which brings
crypto-bigint 0.5.5 affected by RUSTSEC-2026-0387. A direct move to WebRTC 0.21
would require a separate transport/API migration. This temporary patch updates
the public RustCrypto adapters to the current 0.14 releases without changing
protocol negotiation, curves, wire encoding or implementing cryptographic math.

## Local changes

- `Cargo.toml`: P-256/P-384 0.14; explicitly enable P-384 ECDH; import `subtle`
  directly for the existing constant-time CBC MAC comparison.
- `src/curve/named_curve.rs`: use `Generate::try_generate()` (OS cryptographic
  randomness with propagated failure) and the renamed `Sec1Point` type.
- `src/prf/mod.rs`: parse the SEC1 point directly through `PublicKey` instead
  of an intermediate encoded-point wrapper; invalid points remain errors.
- `src/crypto/crypto_cbc.rs`: import the unchanged constant-time comparison
  trait from `subtle`, independent of RustCrypto's re-export.
- `src/prf/prf_test.rs`: add P-256/P-384 agreement, invalid-point and mismatched
  private-key regression checks. All other upstream files are unchanged.

## Validation and removal

Run `cargo test --locked -p dtls`, project tests, the two-viewer UDP-mux
ICE/DTLS/SRTP loopback test and `cargo audit --deny warnings`. The source and
licenses must remain traceable when updating this directory. No audit advisories
are ignored. Remove `[patch.crates-io]` and this directory once a compatible
upstream DTLS release uses fixed RustCrypto dependencies, or after a separately
validated migration to the new WebRTC transport API.
