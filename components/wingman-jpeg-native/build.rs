use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, process::Command};

fn replace(text: &mut String, old: &str, new: &str) {
    assert_eq!(
        text.matches(old).count(),
        1,
        "unexpected pinned JPEG source"
    );
    *text = text.replacen(old, new, 1);
}

fn main() {
    for file in [
        "build.rs",
        "native.c",
        "flat_core.h",
        "requant_core.h",
        "vendor/libjpeg-turbo-3.2.1.tar.gz",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let archive = root.join("vendor/libjpeg-turbo-3.2.1.tar.gz");
    let hash = format!("{:x}", Sha256::digest(fs::read(&archive).unwrap()));
    assert_eq!(
        hash,
        "6f5819c51649bce2571a23b1b0cd41eae2f57fc3f8908c7ec9e7047efb65ca13"
    );
    let source = out.join("jpeg-source");
    fs::create_dir_all(&source).unwrap();
    assert!(Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("--strip-components=1")
        .arg("-C")
        .arg(&source)
        .status()
        .unwrap()
        .success());
    fs::copy(root.join("flat_core.h"), source.join("src/flat_core.h")).unwrap();
    let path = source.join("src/jcdctmgr.c");
    let mut text = fs::read_to_string(&path).unwrap();
    replace(&mut text, "#define JPEG_INTERNALS", "/* Modified by WingmanKVM contributors, 2026: opt-in exact flat blocks. */\n#define JPEG_INTERNALS");
    replace(&mut text, "/* Private subobject for this module */", "#if BITS_IN_JSAMPLE == 8\n#include \"flat_core.h\"\nextern const unsigned char wk_jpeg_flat_marker;\n#endif\n\n/* Private subobject for this module */");
    replace(&mut text, "  struct jpeg_forward_dct pub;  /* public fields */", "  struct jpeg_forward_dct pub;  /* public fields */\n#if BITS_IN_JSAMPLE == 8\n  wk_flat_core wk_flat;\n  int wk_flat_enabled;\n#endif");
    replace(&mut text, "  quantize_method_ptr do_quantize = fdct->quantize;\n  workspace = fdct->workspace;\n\n  sample_data += start_row;     /* fold in the vertical offset once */", r#"  quantize_method_ptr do_quantize = fdct->quantize;
  workspace = fdct->workspace;
  sample_data += start_row;
#if BITS_IN_JSAMPLE == 8
  if (fdct->wk_flat_enabled && cinfo->dct_method == JDCT_ISLOW &&
      wk_flat_begin(&fdct->wk_flat, compptr->component_index,
                    compptr->quant_tbl_no, num_blocks)) {
    for (bi = 0; bi < num_blocks; bi++, start_col += DCTSIZE) {
      JCOEF dc;
      if (wk_flat_try(&fdct->wk_flat, compptr->component_index,
                      compptr->quant_tbl_no, sample_data, 0, start_col, &dc)) {
        memset(coef_blocks[bi], 0, sizeof(JBLOCK));
        coef_blocks[bi][0] = dc;
      } else {
        (*do_convsamp)(sample_data, start_col, workspace);
        (*do_dct)(workspace);
        (*do_quantize)(coef_blocks[bi], divisors, workspace);
      }
    }
    return;
  }
#endif"#);
    replace(&mut text, "  fdct->pub.start_pass = start_pass_fdctmgr;", "  fdct->pub.start_pass = start_pass_fdctmgr;\n#if BITS_IN_JSAMPLE == 8\n  fdct->wk_flat_enabled = cinfo->client_data == &wk_jpeg_flat_marker;\n  if (fdct->wk_flat_enabled) wk_flat_init(&fdct->wk_flat, cinfo, 1);\n#endif");
    fs::write(path, text).unwrap();
    let target = env::var("TARGET").unwrap();
    let simd = target.starts_with("aarch64") || target.starts_with("arm");
    // ARM release builds require NEON/SIMD. Other development targets can
    // build without a separately installed NASM assembler.
    let built = cmake::Config::new(&source)
        .profile("Release")
        .define("ENABLE_SHARED", "OFF")
        .define("ENABLE_STATIC", "ON")
        .define("WITH_TURBOJPEG", "OFF")
        .define("WITH_TOOLS", "OFF")
        .define("WITH_TESTS", "OFF")
        .define("WITH_JPEG7", "OFF")
        .define("WITH_JPEG8", "OFF")
        .define("WITH_SIMD", if simd { "ON" } else { "OFF" })
        .define("REQUIRE_SIMD", if simd { "ON" } else { "OFF" })
        .build_target("jpeg-static")
        .build_arg("-j2")
        .build();
    cc::Build::new()
        .file(root.join("native.c"))
        .include(built.join("build"))
        .include(source.join("src"))
        .include(&root)
        .warnings(true)
        .compile("wingman_jpeg_bridge");
    println!(
        "cargo:rustc-link-search=native={}",
        built.join("build").display()
    );
    println!("cargo:rustc-link-lib=static=jpeg");
    println!("cargo:include={}", source.join("src").display());
    println!("cargo:config={}", built.join("build").display());
    println!("cargo:libdir={}", built.join("build").display());
}
