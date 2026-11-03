use wingman_jpeg::{Backend, ColorMode, Transcoder, encode_rgb_into};

fn upstream(rgb: &[u8], width: u16, height: u16, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, quality);
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    enc.encode(rgb, width, height, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    out
}

#[test]
fn same_jpeg_bytes_as_upstream_including_tails_edges_and_quality_extremes() {
    for (width, height) in [(1, 1), (7, 9), (8, 8), (9, 7), (31, 17), (640, 48)] {
        let mut seed = 0x435f1234_u32;
        let rgb: Vec<u8> = (0..usize::from(width) * usize::from(height) * 3)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        for quality in [1, 50, 81, 100] {
            let reference = upstream(&rgb, width, height, quality);
            for backend in [Backend::Scalar, Backend::Auto] {
                let mut out = vec![123; 100];
                encode_rgb_into(&rgb, width, height, quality, backend, &mut out).unwrap();
                assert_eq!(reference, out, "{width}x{height}, q={quality}, {backend:?}");
            }
        }
    }
}

#[test]
fn handles_reused_buffers_color_grayscale_progressive_and_source_size_changes() {
    let mut transcoder = Transcoder::default();
    let mut out = Vec::new();
    for (width, height) in [(64, 32), (7, 9), (128, 64), (1, 1)] {
        let rgb: Vec<u8> = (0..usize::from(width) * usize::from(height) * 3)
            .map(|i| ((i * 71 + i / 7) % 256) as u8)
            .collect();
        let source = upstream(&rgb, width, height, 95);
        for grayscale in [false, true, false] {
            transcoder
                .transcode_into(
                    &source,
                    81,
                    if grayscale {
                        ColorMode::JpegLuma
                    } else {
                        ColorMode::Color
                    },
                    Backend::Auto,
                    &mut out,
                )
                .unwrap();
            let decoded = image::load_from_memory(&out).unwrap();
            assert_eq!(
                (decoded.width(), decoded.height()),
                (u32::from(width), u32::from(height))
            );
            assert_eq!(
                decoded.color(),
                if grayscale {
                    image::ColorType::L8
                } else {
                    image::ColorType::Rgb8
                }
            );
        }
        let gray = image::load_from_memory(&source).unwrap().into_luma8();
        let mut reference = Vec::new();
        jpeg_encoder::Encoder::new(&mut reference, 81)
            .encode(gray.as_raw(), width, height, jpeg_encoder::ColorType::Luma)
            .unwrap();
        transcoder
            .transcode_into(&source, 81, ColorMode::GrayRgb, Backend::Auto, &mut out)
            .unwrap();
        assert_eq!(reference, out, "RGB grayscale must remain byte-exact");
        let mut progressive = Vec::new();
        let mut enc = jpeg_encoder::Encoder::new(&mut progressive, 81);
        enc.set_progressive(true);
        enc.encode(&rgb, width, height, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        transcoder
            .transcode_into(
                &progressive,
                81,
                ColorMode::JpegLuma,
                Backend::Auto,
                &mut out,
            )
            .unwrap();
        let gray_source = out.clone();
        transcoder
            .transcode_into(&gray_source, 81, ColorMode::Color, Backend::Auto, &mut out)
            .unwrap();
        assert_eq!(
            image::load_from_memory(&out).unwrap().color(),
            image::ColorType::L8
        );
    }
}

#[test]
fn rejects_invalid_input_and_oversized_headers_without_returning_an_old_frame() {
    let mut out = vec![1, 2, 3];
    assert!(encode_rgb_into(&[0; 2], 1, 1, 81, Backend::Auto, &mut out).is_err());
    assert!(out.is_empty());
    let mut transcoder = Transcoder::default();
    for source in [&[][..], &[0xff, 0xd8, 0, 1, 0xff, 0xd9][..]] {
        assert!(
            transcoder
                .transcode_into(source, 81, ColorMode::Color, Backend::Auto, &mut out)
                .is_err()
        );
        assert!(out.is_empty());
    }
    let mut source = upstream(&[0; 3], 1, 1, 81);
    let sof = source.windows(2).position(|s| s == [0xff, 0xc0]).unwrap();
    source[sof + 5..sof + 7].copy_from_slice(&8192_u16.to_be_bytes());
    source[sof + 7..sof + 9].copy_from_slice(&8192_u16.to_be_bytes());
    let error = transcoder
        .transcode_into(&source, 81, ColorMode::Color, Backend::Auto, &mut out)
        .unwrap_err();
    assert!(error.to_string().contains("pixel limit"), "{error}");
}
