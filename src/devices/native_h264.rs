//! Private executable lease for the standalone native software encoder.
use super::*;
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
};
#[derive(Debug)]
pub(super) struct Program {
    pub max_kbps: u32,
    directory: PathBuf,
    executable: PathBuf,
}
impl Drop for Program {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.executable);
        let _ = fs::remove_dir(&self.directory);
    }
}
pub(super) fn prepare(max_kbps: u32) -> Result<EncoderSpec, String> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|e| format!("无法准备原生编码器：{e}"))?;
    let directory =
        std::env::temp_dir().join(format!("wingmankvm-h264-{}", URL_SAFE_NO_PAD.encode(nonce)));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|e| e.to_string())?;
    let program = Arc::new(Program {
        max_kbps,
        executable: directory.join("encoder"),
        directory,
    });
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&program.executable)
        .map_err(|e| e.to_string())?;
    file.write_all(wingman_h264_native::PROGRAM)
        .map_err(|e| e.to_string())?;
    file.set_permissions(fs::Permissions::from_mode(0o500))
        .map_err(|e| e.to_string())?;
    Ok(EncoderSpec {
        executable: program.executable.clone(),
        codec: "libx264",
        label: "x264 (native)",
        software: true,
        native: true,
        _program: Some(program),
    })
}
