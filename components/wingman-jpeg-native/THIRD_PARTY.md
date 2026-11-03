# Sources and licenses

This software is based in part on the work of the Independent JPEG Group.

- Upstream: https://github.com/libjpeg-turbo/libjpeg-turbo/releases/tag/3.2.1
- Official release date: 2026-10-27.
- Archive: https://github.com/libjpeg-turbo/libjpeg-turbo/archive/refs/tags/3.2.1.tar.gz
- SHA-256: `6f5819c51649bce2571a23b1b0cd41eae2f57fc3f8908c7ec9e7047efb65ca13`.
- The complete original source, notices and licenses are retained inside
  `vendor/libjpeg-turbo-3.2.1.tar.gz`; extracted copies of the main notices
  accompany it for convenience. Upstream retains its IJG, BSD and zlib terms.
- Changes: `build.rs` marks and patches only `src/jcdctmgr.c` in a private build
  tree, adding an exact constant-block path gated per compressor. It also adds
  the MIT-licensed `flat_core.h`. The original source archive is unmodified.
- The wrapper, build script, exact flat-block detection and requantization
  kernels are MIT, Copyright (c) 2026 WingmanKVM contributors.

The new binding does not use the research private-ABI hook or depend on the
board's installed libjpeg 2.1.5. Source hash verification runs before extraction.
