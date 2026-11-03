use crate::Backend;

pub(crate) fn gray_rgb(rgb: &[u8], gray: &mut [u8]) {
    static COEFFICIENTS: std::sync::OnceLock<[f32; 3]> = std::sync::OnceLock::new();
    let coefficients = *COEFFICIENTS.get_or_init(|| {
        moxcms::ColorPrimaries::BT_709
            .transform_to_xyz(moxcms::Chromaticity::D65)
            .v[1]
    });
    for (pixel, gray) in rgb.as_chunks::<3>().0.iter().zip(gray) {
        let mut value = 0.0;
        for (&channel, coefficient) in pixel.iter().zip(coefficients) {
            let channel = f32::from(channel) * (1.0 / 255.0);
            // Match image 0.25.10's float expansion, accumulation and rounding.
            // In particular its AArch64 path uses FMA, not separate mul/add.
            #[cfg(any(
                all(
                    any(target_arch = "x86", target_arch = "x86_64"),
                    target_feature = "fma"
                ),
                all(target_arch = "aarch64", target_feature = "neon")
            ))]
            {
                value = channel.mul_add(coefficient, value);
            }
            #[cfg(not(any(
                all(
                    any(target_arch = "x86", target_arch = "x86_64"),
                    target_feature = "fma"
                ),
                all(target_arch = "aarch64", target_feature = "neon")
            )))]
            {
                value += channel * coefficient;
            }
        }
        *gray = (value * 255.0).round() as u8;
    }
}

pub(crate) fn convert(rgb: &[u8], y: &mut [u8], cb: &mut [u8], cr: &mut [u8], backend: Backend) {
    #[cfg(target_arch = "aarch64")]
    if matches!(backend, Backend::Auto) {
        // AArch64's Rust baseline includes NEON. All slice lengths are checked
        // by the safe row construction before the intrinsic kernel is called.
        return crate::neon::convert(rgb, y, cb, cr);
    }
    let _ = backend;
    scalar(rgb, y, cb, cr);
}

pub(crate) fn scalar(rgb: &[u8], y: &mut [u8], cb: &mut [u8], cr: &mut [u8]) {
    for (((pixel, y), cb), cr) in rgb.as_chunks::<3>().0.iter().zip(y).zip(cb).zip(cr) {
        (*y, *cb, *cr) = jpeg_encoder::rgb_to_ycbcr(pixel[0], pixel[1], pixel[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_grayscale_matches_image_for_every_rgb_value() {
        let mut pixels = vec![0; 256 * 256 * 3];
        let mut gray = vec![0; 256 * 256];
        for r in 0..=255_u8 {
            for (i, pixel) in pixels.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                pixel.copy_from_slice(&[r, (i / 256) as u8, i as u8]);
            }
            gray_rgb(&pixels, &mut gray);
            let rgb = image::RgbImage::from_raw(256, 256, pixels.clone()).unwrap();
            let reference = image::DynamicImage::ImageRgb8(rgb).into_luma8();
            assert!(
                gray == *reference.as_raw(),
                "RGB grayscale mismatch at R={r}"
            );
        }
    }

    #[test]
    fn every_rgb_value_matches_upstream() {
        let mut rgb = [0; 256 * 3];
        let (mut y, mut cb, mut cr) = ([0; 256], [0; 256], [0; 256]);
        for r in 0..=255_u8 {
            for g in 0..=255_u8 {
                for (b, pixel) in rgb.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                    pixel.copy_from_slice(&[r, g, b as u8]);
                }
                convert(&rgb, &mut y, &mut cb, &mut cr, Backend::Auto);
                for b in 0..256 {
                    assert_eq!(
                        (y[b], cb[b], cr[b]),
                        jpeg_encoder::rgb_to_ycbcr(r, g, b as u8)
                    );
                }
            }
        }
    }

    #[test]
    fn arbitrary_alignment_tails_and_empty_rows_match_scalar() {
        let rgb: Vec<u8> = (0..300).map(|n| (n * 73 % 256) as u8).collect();
        for offset in 0..16 {
            for width in 0..32 {
                let row = &rgb[offset..offset + width * 3];
                let (mut y, mut cb, mut cr) = (vec![0; width], vec![0; width], vec![0; width]);
                convert(row, &mut y, &mut cb, &mut cr, Backend::Auto);
                for (i, pixel) in row.as_chunks::<3>().0.iter().enumerate() {
                    assert_eq!(
                        (y[i], cb[i], cr[i]),
                        jpeg_encoder::rgb_to_ycbcr(pixel[0], pixel[1], pixel[2])
                    );
                }
            }
        }
    }
}
