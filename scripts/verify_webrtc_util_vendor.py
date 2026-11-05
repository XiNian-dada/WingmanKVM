#!/usr/bin/env python3
"""Apply the existing strict vendored-crate verifier to webrtc-util."""
import verify_dtls_vendor as verifier

# This entry point runs in its own process; share the path, provenance and hash
# checks while keeping each crate's reviewed patch allowlist separate.
if __name__ == "__main__":
    verifier.VENDOR = ("vendor", "webrtc-util")
    verifier.PATCHED = {
        "Cargo.toml": "6e75bc9c33fb07518395216824abe2a99b4f3dcb7f5a2fdf322b57602d77ff36",
        "Cargo.lock": "3da6dcd73bf305590fd95314de66d64531a43033a9f71dd75c6029d3c8460dd4",
    }
    raise SystemExit(verifier.main())
