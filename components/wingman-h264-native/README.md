# Persistent software H.264 encoder

The standalone `wingman-h264-encoder` program is GPL-2.0-or-later (see
ENCODER-LICENSE), separate from the MIT Rust application. It statically links
x264 stable commit 31e19f92f00c7003fa115047ce50978bc98c3a0d and the existing
libjpeg-turbo 3.2.1 build. The complete pinned x264 source is in vendor/; the
build verifies its SHA-256. ARM builds retain NEON; x86 development builds need
no NASM. This crate embeds the compiled executable, which the application
extracts into a private, temporary directory and invokes without a shell.

Input: `WKH1`, big-endian bitrate kbps, FPS, JPEG length (four bytes each), JPEG.
Output: big-endian access-unit length, applied bitrate and JPEG+encode time in
microseconds (four bytes each),
Annex B access unit. Each input has exactly one output; no delimiter lookahead.
JPEGs must be baseline, even dimensions, at most 32 MiB and 33,177,600 pixels.
Output access units are bounded to 4 MiB. Unsupported/corrupt input fails the
process, preserving the application's existing video fallback behavior.

libjpeg reads raw component rows for grayscale and common 4:4:4, 4:2:2 and
4:2:0 JPEGs. Luma is copied directly; chroma is copied or downsampled into
I420 without upsampling, interleaving or an RGB intermediate. x264 uses
ultrafast/zerolatency/baseline with in-band SPS/PPS and full-range samples.
The executable receives the configured bitrate ceiling as its sole argument,
so SPS level selection covers later raises even from a low starting target.
Changing bitrate and VBV settings calls x264_encoder_reconfig before encoding
the next frame, preserving the instance, reference frames and timestamps.

## Distribution and corresponding source

Release archives include this document, `ENCODER-LICENSE`, the upstream x264
`AUTHORS` list and `SOURCE.txt` under `licenses/h264-native/`. `SOURCE.txt`
links to the complete source archive for the exact WingmanKVM commit used to
build that release. It includes the pinned x264 and libjpeg-turbo archives,
the encoder program, JPEG modifications and both native build scripts.
The x264 sources retain their upstream copyright and license notices.
The standalone encoder wrapper is Copyright (c) 2026 WingmanKVM contributors,
GPL-2.0-or-later; the Rust embedding/build wrapper retains its MIT license.
The linked JPEG library's IJG, BSD and zlib notices accompany the binary under
`licenses/jpeg-native/`.

To rebuild from that complete source archive, follow `docs/DEPLOYMENT.md` for
the platform dependencies and run `cargo build --locked --release` at its
root. The native build also requires a C compiler, make, tar and CMake.
`components/wingman-h264-native/build.rs` compiles the standalone executable
in Cargo's build output directory before embedding it in the application;
`components/wingman-jpeg-native/build.rs` applies the documented JPEG changes.
The release workflow records the compiler and target configuration in
`.github/workflows/release.yml`.
