//! Experimental JPEG pipeline. The upstream DCT, quantizer and entropy encoder
//! are unchanged; our RGB row conversion is byte-exact against jpeg-encoder.

mod color;
#[cfg(target_arch = "aarch64")]
mod neon;

use jpeg_encoder::{Encoder, ImageBuffer, JpegColorType, SamplingFactor};
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
use zune_jpeg::JpegDecoder;

/// Select the portable reference path for comparisons or automatic NEON on ARM64.
#[derive(Clone, Copy, Debug, Default)]
pub enum Backend {
    #[default]
    Auto,
    Scalar,
}

/// GrayRgb preserves image 0.25.10's grayscale definition. JpegLuma is a
/// faster, explicitly different brightness definition, never an implicit fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Color,
    GrayRgb,
    JpegLuma,
}

impl Backend {
    pub fn name(self) -> &'static str {
        if matches!(self, Self::Auto) && cfg!(target_arch = "aarch64") {
            "aarch64-neon"
        } else {
            "portable-scalar"
        }
    }
}

/// Errors include invalid input sizes and decoder/encoder failures.
#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

fn error(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}

struct RgbRows<'a> {
    pixels: &'a [u8],
    width: u16,
    height: u16,
    backend: Backend,
}

impl ImageBuffer for RgbRows<'_> {
    fn get_jpeg_color_type(&self) -> JpegColorType {
        JpegColorType::Ycbcr
    }
    fn width(&self) -> u16 {
        self.width
    }
    fn height(&self) -> u16 {
        self.height
    }

    fn fill_buffers(&self, row: u16, buffers: &mut [Vec<u8>; 4]) {
        let width = usize::from(self.width);
        let offset = usize::from(row) * width * 3;
        let rgb = &self.pixels[offset..offset + width * 3];
        let [ys, cbs, crs, _] = buffers;
        let (y_start, cb_start, cr_start) = (ys.len(), cbs.len(), crs.len());
        ys.resize(y_start + width, 0);
        cbs.resize(cb_start + width, 0);
        crs.resize(cr_start + width, 0);
        color::convert(
            rgb,
            &mut ys[y_start..],
            &mut cbs[cb_start..],
            &mut crs[cr_start..],
            self.backend,
        );
    }
}

/// Encode packed RGB8 to baseline 4:4:4 JPEG. Output is cleared on entry;
/// discard it on error. Existing capacity can be reused on the next call.
pub fn encode_rgb_into(
    rgb: &[u8],
    width: u16,
    height: u16,
    quality: u8,
    backend: Backend,
    output: &mut Vec<u8>,
) -> Result<(), Error> {
    output.clear();
    let expected = usize::from(width)
        .checked_mul(usize::from(height))
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| error("RGB dimensions overflow"))?;
    if width == 0 || height == 0 || rgb.len() != expected {
        return Err(error("RGB buffer must match nonzero width × height × 3"));
    }
    let mut encoder = Encoder::new(output, quality.clamp(1, 100));
    encoder.set_sampling_factor(SamplingFactor::F_1_1);
    encoder
        .encode_image(RgbRows {
            pixels: rgb,
            width,
            height,
            backend,
        })
        .map_err(error)
}

/// Reuses the decoded pixel allocation between frames. The JPEG decoder's
/// internal coefficient storage is still managed by zune-jpeg per image.
#[derive(Default)]
pub struct Transcoder {
    pixels: Vec<u8>,
    gray: Vec<u8>,
}

impl Transcoder {
    /// Transcode a JPEG. JpegLuma requests JPEG luminance directly; its BT.601
    /// coefficients differ from image's BT.709-style RGB grayscale. Color mode
    /// preserves 4:4:4 sampling. ICC/EXIF metadata are not copied.
    ///
    /// The stream-oriented limits are 32 MiB compressed, 8192 per dimension,
    /// and 33,177,600 pixels. Progressive input is limited to 64 scans.
    pub fn transcode_into(
        &mut self,
        source: &[u8],
        quality: u8,
        mode: ColorMode,
        backend: Backend,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        output.clear();
        if source.len() > 32 * 1024 * 1024 {
            return Err(error("compressed JPEG exceeds 32 MiB limit"));
        }
        let wanted = if mode == ColorMode::JpegLuma {
            ColorSpace::Luma
        } else {
            ColorSpace::RGB
        };
        let options = DecoderOptions::default()
            .set_strict_mode(false)
            .set_max_width(8192)
            .set_max_height(8192)
            .jpeg_set_max_scans(64)
            .jpeg_set_out_colorspace(wanted);
        let mut decoder = JpegDecoder::new_with_options(ZCursor::new(source), options);
        decoder.decode_headers().map_err(error)?;
        let (width, height) = decoder
            .dimensions()
            .ok_or_else(|| error("missing JPEG dimensions"))?;
        if width == 0 || height == 0 || width.checked_mul(height).is_none_or(|n| n > 33_177_600) {
            return Err(error("decoded JPEG exceeds pixel limit"));
        }
        // Avoid expanding an already monochrome source to RGB.
        if decoder.input_colorspace() == Some(ColorSpace::Luma) {
            decoder.set_options(options.jpeg_set_out_colorspace(ColorSpace::Luma));
        }
        let color = decoder
            .output_colorspace()
            .ok_or_else(|| error("missing JPEG color space"))?;
        if !matches!(color, ColorSpace::RGB | ColorSpace::Luma)
            || (mode == ColorMode::JpegLuma && color != ColorSpace::Luma)
        {
            return Err(error("unsupported JPEG output color space"));
        }
        let size = decoder
            .output_buffer_size()
            .ok_or_else(|| error("JPEG size overflow"))?;
        let expected = width * height * if color == ColorSpace::Luma { 1 } else { 3 };
        if size != expected {
            return Err(error("inconsistent JPEG output size"));
        }
        self.pixels.resize(size, 0);
        decoder.decode_into(&mut self.pixels).map_err(error)?;
        let width = u16::try_from(width).map_err(error)?;
        let height = u16::try_from(height).map_err(error)?;
        if color == ColorSpace::RGB && mode == ColorMode::GrayRgb {
            self.gray
                .resize(usize::from(width) * usize::from(height), 0);
            color::gray_rgb(&self.pixels, &mut self.gray);
            Encoder::new(output, quality.clamp(1, 100))
                .encode(&self.gray, width, height, jpeg_encoder::ColorType::Luma)
                .map_err(error)
        } else if color == ColorSpace::Luma {
            Encoder::new(output, quality.clamp(1, 100))
                .encode(&self.pixels, width, height, jpeg_encoder::ColorType::Luma)
                .map_err(error)
        } else {
            encode_rgb_into(&self.pixels, width, height, quality, backend, output)
        }
    }
}
