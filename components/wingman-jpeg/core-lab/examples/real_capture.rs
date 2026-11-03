//! Offline real-frame comparison; does not attach to the running service.
use std::{fs::OpenOptions, io::Write, path::Path, time::Instant};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() >= 3,
        "INPUT.jpg OUTPUT_PREFIX [quality=81] [rounds=11]"
    );
    let quality: u8 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(81);
    let rounds: usize = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(11);
    assert!((1..=100).contains(&quality) && (1..=51).contains(&rounds));
    let source = std::fs::read(&args[1]).unwrap();
    assert!(source.len() <= 32 * 1024 * 1024);
    let names = [
        "image-live-model",
        "branch-jpeg-encoder",
        "component-neon",
        "rust-core-flat",
    ];
    let mut times: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::new());
    let mut outputs: [Vec<u8>; 4] = std::array::from_fn(|_| Vec::with_capacity(source.len() * 2));
    let mut component = wingman_jpeg::Transcoder::default();
    for round in 0..rounds + 2 {
        for step in 0..4 {
            let mode = (step + round) % 4;
            let start = Instant::now();
            let out = &mut outputs[mode];
            out.clear();
            if mode == 2 {
                component
                    .transcode_into(
                        &source,
                        quality,
                        wingman_jpeg::ColorMode::Color,
                        wingman_jpeg::Backend::Auto,
                        out,
                    )
                    .unwrap();
            } else {
                let image =
                    image::load_from_memory_with_format(&source, image::ImageFormat::Jpeg).unwrap();
                if mode == 0 {
                    image::codecs::jpeg::JpegEncoder::new_with_quality(out, quality)
                        .encode_image(&image)
                        .unwrap();
                } else {
                    let width = u16::try_from(image.width()).unwrap();
                    let height = u16::try_from(image.height()).unwrap();
                    let rgb = image.into_rgb8();
                    if mode == 1 {
                        let mut encoder = jpeg_encoder_upstream::Encoder::new(out, quality);
                        encoder.set_sampling_factor(jpeg_encoder_upstream::SamplingFactor::F_1_1);
                        encoder
                            .encode(
                                rgb.as_raw(),
                                width,
                                height,
                                jpeg_encoder_upstream::ColorType::Rgb,
                            )
                            .unwrap();
                    } else {
                        let mut encoder = jpeg_encoder::Encoder::new(out, quality);
                        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
                        encoder.set_dct_backend(jpeg_encoder::DctBackend::Auto);
                        encoder
                            .encode(rgb.as_raw(), width, height, jpeg_encoder::ColorType::Rgb)
                            .unwrap();
                    }
                }
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if round >= 2 {
                times[mode].push(elapsed);
            }
        }
        assert!(
            outputs[1] == outputs[2],
            "component/upstream JPEG bytes differ"
        );
        assert!(outputs[1] == outputs[3], "core/upstream JPEG bytes differ");
    }
    for mode in 0..4 {
        times[mode].sort_by(f64::total_cmp);
        println!(
            "mode={} q={} rounds={} median_ms={:.6} p95_ms={:.6} bytes={} mbps_24={:.6}",
            names[mode],
            quality,
            rounds,
            times[mode][rounds / 2],
            times[mode][(rounds * 95).div_ceil(100) - 1],
            outputs[mode].len(),
            outputs[mode].len() as f64 * 8.0 * 24.0 / 1_000_000.0
        );
        let file = format!("{}-{}.jpg", args[2], names[mode]);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(Path::new(&file))
            .unwrap()
            .write_all(&outputs[mode])
            .unwrap();
    }
    println!("byte_exact_modes=branch-jpeg-encoder,component-neon,rust-core-flat; 4:4:4 color; reused output buffers; modeled paths, not live service instrumentation");
}
