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
