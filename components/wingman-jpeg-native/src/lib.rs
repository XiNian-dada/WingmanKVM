//! Bounded, per-call JPEG processing. Native errors return to C before Rust.
use std::{
    ffi::{c_char, c_int, c_uchar, CStr},
    fmt, ptr,
};

pub const LIBRARY_VERSION: &str = "3.2.1";
pub const MAX_INPUT: usize = 32 * 1024 * 1024;
pub const MAX_OUTPUT: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum Mode {
    Standard = 0,
    Screen = 1,
    Coefficient = 2,
    Preserve = 3,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub quality: u8,
    pub grayscale: bool,
    pub mode: Mode,
    pub optimize_huffman: bool,
}

#[derive(Debug)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}

unsafe extern "C" {
    fn wk_jpeg_process(
        input: *const c_uchar,
        input_len: usize,
        quality: c_int,
        gray: c_int,
        mode: c_int,
        optimize: c_int,
        output: *mut *mut c_uchar,
        output_len: *mut usize,
        error: *mut c_char,
        error_size: usize,
    ) -> c_int;
    fn wk_jpeg_free(output: *mut c_uchar);
}

struct NativeBuffer(*mut c_uchar);
impl Drop for NativeBuffer {
    fn drop(&mut self) {
        // SAFETY: the C function returns either NULL or one exclusively owned
        // malloc buffer. Only this owner releases it, using the same allocator.
        unsafe { wk_jpeg_free(self.0) };
    }
}

/// Output is cleared on entry and on failure. No references escape this call.
pub fn transcode_into(input: &[u8], options: Options, output: &mut Vec<u8>) -> Result<(), Error> {
    output.clear();
    if input.len() < 4 || input.len() > MAX_INPUT || !(1..=100).contains(&options.quality) {
        return Err(Error("JPEG input or quality exceeds limits".into()));
    }
    let mut buffer = NativeBuffer(ptr::null_mut());
    let mut length = 0;
    let mut message = [0 as c_char; 256];
    // SAFETY: all slices/pointers remain valid for this synchronous call. C
    // only reads input; returns a bounded allocation and terminated error text.
    // setjmp and every error callback are wholly within C, with no Rust callbacks.
    let status = unsafe {
        wk_jpeg_process(
            input.as_ptr(),
            input.len(),
            options.quality.into(),
            options.grayscale.into(),
            options.mode as c_int,
            options.optimize_huffman.into(),
            &mut buffer.0,
            &mut length,
            message.as_mut_ptr(),
            message.len(),
        )
    };
    if status != 0 {
        // SAFETY: the C contract terminates the fixed-size message buffer.
        let text = unsafe { CStr::from_ptr(message.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        return Err(Error(text));
    }
    if buffer.0.is_null() || length == 0 || length > MAX_OUTPUT {
        return Err(Error(
            "Native JPEG returned an invalid output length".into(),
        ));
    }
    output
        .try_reserve(length)
        .map_err(|_| Error("JPEG output allocation failed".into()))?;
    // SAFETY: on success C transfers ownership of length initialized bytes.
    // The buffer lives through this copy and is freed once by NativeBuffer.
    output.extend_from_slice(unsafe { std::slice::from_raw_parts(buffer.0, length) });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageEncoder, ImageFormat};

    fn fixture(width: u32, height: u32, gray: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        if gray {
            let pixels = image::GrayImage::from_fn(width, height, |x, y| {
                image::Luma([((x * 13 + y * 7) % 256) as u8])
            });
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95)
                .write_image(pixels.as_raw(), width, height, image::ExtendedColorType::L8)
                .unwrap();
        } else {
            let pixels = image::RgbImage::from_fn(width, height, |x, y| {
                if x < width / 2 {
                    image::Rgb([20, 20, 20])
                } else {
                    image::Rgb([(x * 7) as u8, (y * 11) as u8, 90])
                }
            });
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95)
                .write_image(
                    pixels.as_raw(),
                    width,
                    height,
                    image::ExtendedColorType::Rgb8,
                )
                .unwrap();
        }
        bytes
    }
    fn encode(
        input: &[u8],
        mode: Mode,
        quality: u8,
        grayscale: bool,
        optimize_huffman: bool,
    ) -> Vec<u8> {
        let mut output = vec![1, 2, 3];
        transcode_into(
            input,
            Options {
                mode,
                quality,
                grayscale,
                optimize_huffman,
            },
            &mut output,
        )
        .unwrap();
        output
    }
    #[test]
    fn screen_bytes_equal_standard_in_color_gray_and_odd_dimensions() {
        for (w, h) in [(1, 1), (7, 9), (31, 17), (128, 65)] {
            for source_gray in [false, true] {
                let source = fixture(w, h, source_gray);
                for q in [1, 20, 50, 81, 95, 100] {
                    for gray in [false, true] {
                        for huffman in [false, true] {
                            let standard = encode(&source, Mode::Standard, q, gray, huffman);
                            assert_eq!(standard, encode(&source, Mode::Screen, q, gray, huffman));
                            let decoded =
                                image::load_from_memory_with_format(&standard, ImageFormat::Jpeg)
                                    .unwrap();
                            assert_eq!((decoded.width(), decoded.height()), (w, h));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn coefficient_modes_decode_and_preserve_is_pixel_exact() {
        for (w, h) in [(1, 1), (7, 9), (31, 17), (127, 65)] {
            for source_gray in [false, true] {
                let source = fixture(w, h, source_gray);
                let reference = image::load_from_memory(&source).unwrap().into_rgb8();
                for mode in [Mode::Coefficient, Mode::Preserve] {
                    for gray in [false, true] {
                        let output = encode(&source, mode, 50, gray, false);
                        let decoded = image::load_from_memory(&output).unwrap().into_rgb8();
                        assert_eq!(decoded.dimensions(), (w, h));
                        if matches!(mode, Mode::Preserve) && !gray {
                            assert_eq!(decoded, reference);
                        }
                        if gray {
                            assert!(decoded.pixels().all(|p| p[0] == p[1] && p[1] == p[2]));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn truncated_invalid_and_oversized_frames_return_errors_then_recover() {
        let source = fixture(64, 33, false);
        let mut invalids = vec![
            vec![],
            vec![255, 216, 255, 217],
            vec![3; 200],
            source[..source.len() / 2].to_vec(),
        ];
        let mut oversized = source.clone();
        let sof = oversized.windows(2).position(|b| b == [255, 192]).unwrap();
        oversized[sof + 5..sof + 9].copy_from_slice(&[0x7f, 0xff, 0x7f, 0xff]);
        invalids.push(oversized);
        for bad in invalids {
            for mode in [
                Mode::Standard,
                Mode::Screen,
                Mode::Coefficient,
                Mode::Preserve,
            ] {
                let mut output = vec![7; 20];
                assert!(transcode_into(
                    &bad,
                    Options {
                        quality: 81,
                        grayscale: false,
                        mode,
                        optimize_huffman: false
                    },
                    &mut output
                )
                .is_err());
                assert!(output.is_empty());
                assert!(!encode(&source, mode, 81, false, false).is_empty());
            }
        }
    }
    #[test]
    fn independent_calls_can_run_concurrently() {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    let source = fixture(31, 17, false);
                    for _ in 0..20 {
                        encode(&source, Mode::Screen, 81, true, false);
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn every_gray_sample_and_quality_has_identical_screen_bytes() {
        for sample in 0..=255 {
            let pixels = [sample; 64];
            let mut source = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut source, 100)
                .write_image(&pixels, 8, 8, image::ExtendedColorType::L8)
                .unwrap();
            for quality in 1..=100 {
                assert_eq!(
                    encode(&source, Mode::Standard, quality, true, false),
                    encode(&source, Mode::Screen, quality, true, false)
                );
            }
        }
    }

    #[test]
    fn preserve_keeps_exif_and_icc_but_drops_color_profile_for_grayscale() {
        fn append_marker(bytes: &mut Vec<u8>, marker: u8, payload: &[u8]) {
            bytes.extend_from_slice(&[255, marker]);
            bytes.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
            bytes.extend_from_slice(payload);
        }
        let plain = fixture(31, 17, false);
        let mut source = plain[..2].to_vec();
        let exif = b"Exif\0\0II*\0\x08\0\0\0\0\0\0\0\0\0";
        let profile = b"ICC_PROFILE\0\x01\x01opaque-profile-test";
        append_marker(&mut source, 0xe1, exif);
        append_marker(&mut source, 0xe2, profile);
        source.extend_from_slice(&plain[2..]);
        let color = encode(&source, Mode::Preserve, 81, false, false);
        assert!(color.windows(exif.len()).any(|w| w == exif));
        assert!(color.windows(profile.len()).any(|w| w == profile));
        let gray = encode(&source, Mode::Preserve, 81, true, false);
        assert!(gray.windows(exif.len()).any(|w| w == exif));
        assert!(!gray.windows(profile.len()).any(|w| w == profile));
    }
}
