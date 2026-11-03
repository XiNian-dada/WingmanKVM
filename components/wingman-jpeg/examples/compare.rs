//! Offline, round-robin benchmark; no capture devices or network are opened.
use std::{hint::black_box, time::Instant};
use wingman_jpeg::{Backend, ColorMode, Transcoder, encode_rgb_into};

fn upstream(rgb: &[u8], w: u16, h: u16, out: &mut Vec<u8>) {
    out.clear();
    let mut encoder = jpeg_encoder::Encoder::new(out, 81);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    encoder
        .encode(rgb, w, h, jpeg_encoder::ColorType::Rgb)
        .unwrap();
}

fn baseline(source: &[u8], old: bool, gray: bool, out: &mut Vec<u8>) {
    out.clear();
    let image = image::load_from_memory_with_format(source, image::ImageFormat::Jpeg).unwrap();
    let image = if gray {
        image::DynamicImage::ImageLuma8(image.into_luma8())
    } else {
        image
    };
    if old {
        image::codecs::jpeg::JpegEncoder::new_with_quality(out, 81)
            .encode_image(&image)
            .unwrap();
    } else if gray {
        jpeg_encoder::Encoder::new(out, 81)
            .encode(
                image.as_luma8().unwrap().as_raw(),
                image.width() as u16,
                image.height() as u16,
                jpeg_encoder::ColorType::Luma,
            )
            .unwrap();
    } else {
        upstream(
            image.as_rgb8().unwrap().as_raw(),
            image.width() as u16,
            image.height() as u16,
            out,
        );
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let pattern = args.get(1).map(String::as_str).unwrap_or("desktop");
    let iterations: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(7);
    let w: u16 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(1920);
    let h: u16 = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(1080);
    assert!(iterations >= 3 && w > 0 && h > 0);
    let mut seed = 0xa135ce42_u32;
    let rgb = image::RgbImage::from_fn(u32::from(w), u32::from(h), |x, y| match pattern {
        "desktop" => {
            let glyph = x % 11 < 7 && y % 22 < 14 && ((x / 11 + y / 22) % 3 != 0);
            if glyph {
                image::Rgb([210, (80 + y / 8 % 170) as u8, 100])
            } else {
                image::Rgb([18, 22, 28])
            }
        }
        "gradient" => image::Rgb([
            (x * 255 / u32::from(w)) as u8,
            (y * 255 / u32::from(h)) as u8,
            ((x + y) % 256) as u8,
        ]),
        "noise" => {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            image::Rgb([seed as u8, (seed >> 8) as u8, (seed >> 16) as u8])
        }
        _ => panic!("patterns: desktop, gradient, noise"),
    });
    let mut source = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut source, 95);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_1);
    encoder
        .encode(rgb.as_raw(), w, h, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    let decoded = image::load_from_memory(&source).unwrap().into_rgb8();
    let names = [
        "encode_upstream",
        "encode_bulk_scalar",
        "encode_neon_auto",
        "pipeline_original_image",
        "pipeline_current_color",
        "pipeline_optimized_color",
        "pipeline_current_gray",
        "pipeline_optimized_gray_exact",
        "pipeline_direct_luma",
    ];
    let mut times = vec![Vec::new(); names.len()];
    let mut outputs = vec![Vec::new(); names.len()];
    let mut transcoder = Transcoder::default();
    for round in 0..=iterations {
        for step in 0..names.len() {
            let mode = (step + round) % names.len();
            let start = Instant::now();
            let out = &mut outputs[mode];
            match mode {
                0 => upstream(black_box(decoded.as_raw()), w, h, out),
                1 => encode_rgb_into(black_box(decoded.as_raw()), w, h, 81, Backend::Scalar, out)
                    .unwrap(),
                2 => encode_rgb_into(black_box(decoded.as_raw()), w, h, 81, Backend::Auto, out)
                    .unwrap(),
                3 => baseline(black_box(&source), true, false, out),
                4 => baseline(black_box(&source), false, false, out),
                5 => transcoder
                    .transcode_into(black_box(&source), 81, ColorMode::Color, Backend::Auto, out)
                    .unwrap(),
                6 => baseline(black_box(&source), false, true, out),
                7 => transcoder
                    .transcode_into(
                        black_box(&source),
                        81,
                        ColorMode::GrayRgb,
                        Backend::Auto,
                        out,
                    )
                    .unwrap(),
                8 => transcoder
                    .transcode_into(
                        black_box(&source),
                        81,
                        ColorMode::JpegLuma,
                        Backend::Auto,
                        out,
                    )
                    .unwrap(),
                _ => unreachable!(),
            }
            black_box(out.len());
            if round != 0 {
                times[mode].push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        assert_eq!(outputs[0], outputs[1], "scalar output changed");
        assert_eq!(outputs[0], outputs[2], "SIMD output changed");
        assert_eq!(
            outputs[4], outputs[5],
            "optimized color pipeline output changed"
        );
        assert_eq!(
            outputs[6], outputs[7],
            "optimized RGB grayscale output changed"
        );
    }
    println!(
        "pattern={pattern} width={w} height={h} iterations={iterations} backend={}",
        Backend::Auto.name()
    );
    for (i, name) in names.iter().enumerate() {
        times[i].sort_by(f64::total_cmp);
        let median = times[i][iterations / 2];
        let p95 = times[i][(iterations * 95).div_ceil(100).saturating_sub(1)];
        println!(
            "{name},median_ms={median:.3},p95_ms={p95:.3},min_ms={:.3},bytes={}",
            times[i][0],
            outputs[i].len()
        );
    }
    let reference = image::load_from_memory(&outputs[6]).unwrap().into_luma8();
    let direct = image::load_from_memory(&outputs[8]).unwrap().into_luma8();
    let mse = reference
        .as_raw()
        .iter()
        .zip(direct.as_raw())
        .map(|(&a, &b)| (f64::from(a) - f64::from(b)).powi(2))
        .sum::<f64>()
        / reference.len() as f64;
    let max = reference
        .as_raw()
        .iter()
        .zip(direct.as_raw())
        .map(|(&a, &b)| a.abs_diff(b))
        .max()
        .unwrap();
    println!(
        "gray_difference,mse={mse:.5},psnr_db={:.3},max_abs={max}",
        10.0 * (255.0_f64.powi(2) / mse).log10()
    );
}
