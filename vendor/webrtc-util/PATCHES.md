# Temporary WebRTC utility dependency feature patch

Upstream: `webrtc-util` 0.17.2, https://crates.io/crates/webrtc-util/0.17.2

- Repository: https://github.com/webrtc-rs/webrtc
- Source commit: `1da3fe8ee84db8d266322cbcfcb357c745959a8e`, directory `util/`.
- Registry archive SHA-256: `1beae0b4f24969741c26ca282a257dc5823bd9cbe608f7e3ca3775102d323ad9`.
- Source, examples, benches, normalized Cargo.toml, README, licenses
  and VCS metadata were copied from that checksum-verified registry archive.
  Original manifests and CI/cache metadata are omitted. The independent
  lockfile is retained for reproducible upstream crate tests.
- `upstream-manifest.json` records each copied file's original SHA-256.

## Reason and local change

RUSTSEC-2026-0441 marks pin-utils as unmaintained. WebRTC 0.17.2 selects nix
0.26 with all default features, including the unused `aio` feature that alone
introduces pin-utils. Upstream webrtc-util uses nix only in
`src/ifaces/ffi/unix/mod.rs`, for `ifaddrs` and `sys::socket`.

The runtime patch is confined to `Cargo.toml`: set `default-features = false`
and `features = ["net"]` on nix. Its `net` feature enables `socket` transitively.
No upstream Rust code, network behavior, APIs or cryptography are changed.
`Cargo.lock` is regenerated for the crate's standalone development tests; the
application uses the repository root lockfile. All audit warnings remain denied;
no advisory is ignored.

## Validation and removal

Run `python3 scripts/verify_webrtc_util_vendor.py`,
`cargo test --locked --manifest-path vendor/webrtc-util/Cargo.toml`, project/DTLS regression tests and
`cargo audit --deny warnings`. `cargo tree --locked -i pin-utils` should find
no package. CI checks the reviewed patched hash and all unchanged source hashes.

Remove this patch and directory when a compatible upstream webrtc-util release
narrows its nix features or otherwise eliminates the unmaintained dependency.
See https://rustsec.org/advisories/RUSTSEC-2026-0441 for the upstream notice.
