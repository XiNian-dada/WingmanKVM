use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, process::Command};
fn run(command: &mut Command) {
    assert!(
        command
            .status()
            .expect("start native encoder build")
            .success(),
        "native encoder build failed"
    );
}
fn main() {
    println!("cargo:rerun-if-changed=encoder.c");
    println!("cargo:rerun-if-changed=vendor/x264-31e19f92.tar.gz");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let archive = root.join("vendor/x264-31e19f92.tar.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&archive).unwrap())),
        "d053c9d86988d6bc78237ca5205865c5ddf99c98ef4cd9927eec8f6d388f6dd9"
    );
    let source = out.join("x264");
    fs::create_dir_all(&source).unwrap();
    run(Command::new("tar")
        .args(["-xzf"])
        .arg(&archive)
        .args(["--strip-components=1", "-C"])
        .arg(&source));
    let tool = cc::Build::new().get_compiler();
    let cflags = tool
        .args()
        .iter()
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let target = env::var("TARGET").unwrap();
    let mut configure = Command::new("sh");
    configure
        .current_dir(&source)
        .arg("configure")
        .args([
            "--disable-cli",
            "--enable-static",
            "--enable-pic",
            "--disable-opencl",
            "--disable-lavf",
            "--disable-swscale",
            "--bit-depth=8",
            "--chroma-format=420",
        ])
        .arg(format!("--host={target}"))
        .env("CC", tool.path())
        .env("CFLAGS", cflags);
    if !target.starts_with("aarch64") && !target.starts_with("arm") {
        configure.arg("--disable-asm"); // Development builds need no NASM. ARM keeps NEON.
    }
    run(&mut configure);
    run(Command::new("make")
        .current_dir(&source)
        .args(["-j2", "libx264.a"]));
    let jpeg_include = env::var_os("DEP_WINGMAN_JPEG_INCLUDE").unwrap();
    let jpeg_config = env::var_os("DEP_WINGMAN_JPEG_CONFIG").unwrap();
    let jpeg_libdir = PathBuf::from(env::var_os("DEP_WINGMAN_JPEG_LIBDIR").unwrap());
    let mut compile = tool.to_command();
    compile
        .args([
            "-O3",
            "-std=c11",
            "-D_POSIX_C_SOURCE=200809L",
            "-Wall",
            "-Wextra",
        ])
        .arg("-I")
        .arg(&source)
        .arg("-I")
        .arg(jpeg_include)
        .arg("-I")
        .arg(jpeg_config)
        .arg(root.join("encoder.c"))
        .arg(source.join("libx264.a"))
        .arg(jpeg_libdir.join("libjpeg.a"))
        .args(["-lm", "-lpthread"]);
    if target.contains("musl") {
        compile.arg("-static");
    }
    compile.arg("-o").arg(out.join("wingman-h264-encoder"));
    run(&mut compile);
}
