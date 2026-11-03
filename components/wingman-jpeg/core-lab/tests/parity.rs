use jpeg_encoder::{DctBackend, Encoder};

#[test]
fn full_jpeg_bytes_match_unmodified_upstream_across_formats_and_modes() {
    let mut seed = 0x1234abcd_u32;
    for (width, height) in [(1, 1), (7, 9), (31, 17), (128, 64)] {
        for channels in [1, 3, 4] {
            let pixels: Vec<u8> = (0..usize::from(width) * usize::from(height) * channels)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect();
            for quality in [1, 20, 50, 81, 95, 100] {
                for (progressive, optimize, subsample) in [
                    (false, false, false),
                    (false, true, true),
                    (true, false, true),
                ] {
                    let mut reference = Vec::new();
                    let mut encoder = jpeg_encoder_upstream::Encoder::new(&mut reference, quality);
                    encoder.set_progressive(progressive);
                    encoder.set_optimized_huffman_tables(optimize);
                    encoder.set_restart_interval(3);
                    if !subsample {
                        encoder.set_sampling_factor(jpeg_encoder_upstream::SamplingFactor::F_1_1);
                    }
                    let color = match channels {
                        1 => jpeg_encoder_upstream::ColorType::Luma,
                        3 => jpeg_encoder_upstream::ColorType::Rgb,
                        _ => jpeg_encoder_upstream::ColorType::Rgba,
                    };
                    encoder.encode(&pixels, width, height, color).unwrap();
                    for backend in [
                        DctBackend::Auto,
                        DctBackend::FusedLoad,
                        DctBackend::Reference,
                        DctBackend::Neon,
                        DctBackend::NeonFlat,
                        DctBackend::NeonQuant,
                        DctBackend::NeonFlatQuant,
                        DctBackend::FusedFlatQuant,
                    ] {
                        let mut out = Vec::new();
                        let mut encoder = Encoder::new(&mut out, quality);
                        encoder.set_dct_backend(backend);
                        encoder.set_progressive(progressive);
                        encoder.set_optimized_huffman_tables(optimize);
                        encoder.set_restart_interval(3);
                        if !subsample {
                            encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
                        }
                        let color = match channels {
                            1 => jpeg_encoder::ColorType::Luma,
                            3 => jpeg_encoder::ColorType::Rgb,
                            _ => jpeg_encoder::ColorType::Rgba,
                        };
                        encoder.encode(&pixels, width, height, color).unwrap();
                        assert!(out==reference,"{width}x{height} channels={channels} quality={quality} backend={backend:?} progressive={progressive} optimize={optimize}");
                    }
                }
            }
        }
    }
}
