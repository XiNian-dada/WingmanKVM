//! Core ablations use identical bulk RGB rows, quality and Huffman settings.
use jpeg_encoder::{diagnostics, DctBackend};
use std::{hint::black_box, time::Instant};

struct Rows<'a>(&'a [u8], u16, u16);
impl Rows<'_> {
    fn fill(&self, row: u16, buffers: &mut [Vec<u8>; 4]) {
        let width = usize::from(self.1);
        let offset = usize::from(row) * width * 3;
        let [y, cb, cr, _] = buffers;
        let starts = [y.len(), cb.len(), cr.len()];
        y.resize(starts[0] + width, 0);
        cb.resize(starts[1] + width, 0);
        cr.resize(starts[2] + width, 0);
        for (((pixel, y), cb), cr) in self.0[offset..offset + width * 3]
            .as_chunks::<3>()
            .0
            .iter()
            .zip(&mut y[starts[0]..])
            .zip(&mut cb[starts[1]..])
            .zip(&mut cr[starts[2]..])
        {
            (*y, *cb, *cr) = jpeg_encoder_upstream::rgb_to_ycbcr(pixel[0], pixel[1], pixel[2]);
        }
    }
}
macro_rules! rows {
    ($lib:ident) => {
        impl $lib::ImageBuffer for Rows<'_> {
            fn width(&self) -> u16 {
                self.1
            }
            fn height(&self) -> u16 {
                self.2
            }
            fn get_jpeg_color_type(&self) -> $lib::JpegColorType {
                $lib::JpegColorType::Ycbcr
            }
            fn fill_buffers(&self, row: u16, buffers: &mut [Vec<u8>; 4]) {
                self.fill(row, buffers);
            }
        }
    };
}
rows!(jpeg_encoder);
rows!(jpeg_encoder_upstream);

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "micro") {
        micro();
        return;
    }
    let pattern = args.get(1).map(String::as_str).unwrap_or("desktop");
    let rounds: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(15);
    let (width, height) = (1920_u16, 1080_u16);
    let mut seed = 0xa135ce42_u32;
    let rgb = image::RgbImage::from_fn(u32::from(width), u32::from(height), |x, y| match pattern {
        "flat" => image::Rgb([18, 22, 28]),
        "desktop" | "sparse" => {
            let glyph = x % 11 < 7
                && y % 22 < 14
                && (x / 11 + y / 22) % 3 != 0
                && (pattern != "sparse" || (x < 900 && y < 300));
            if glyph {
                image::Rgb([210, (80 + y / 8 % 170) as u8, 100])
            } else {
                image::Rgb([18, 22, 28])
            }
        }
        "gradient" => image::Rgb([
            (x * 255 / u32::from(width)) as u8,
            (y * 255 / u32::from(height)) as u8,
            ((x + y) % 256) as u8,
        ]),
        "noise" => {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            image::Rgb([seed as u8, (seed >> 8) as u8, (seed >> 16) as u8])
        }
        _ => panic!("flat/desktop/sparse/gradient/noise"),
    });
    let input_hash = rgb
        .as_raw()
        .iter()
        .fold(14695981039346656037_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
        });
    println!("input_fnv64={input_hash:016x}");
    let names = [
        "upstream_bulk",
        "phase1_component",
        "reference_core",
        "neon_dct",
        "neon_flat_dct",
        "neon_dct_quant",
        "neon_flat_dct_quant",
        "neon_flat_fused_quant",
        "neon_flat_fused_quant_load",
    ];
    let backends = [
        DctBackend::Reference,
        DctBackend::Neon,
        DctBackend::NeonFlat,
        DctBackend::NeonQuant,
        DctBackend::NeonFlatQuant,
        DctBackend::FusedFlatQuant,
        DctBackend::FusedLoad,
    ];
    let mut outputs = vec![Vec::new(); names.len()];
    let mut times = vec![Vec::new(); names.len()];
    for round in 0..=rounds {
        for step in 0..names.len() {
            let mode = (step + round) % names.len();
            let out = &mut outputs[mode];
            out.clear();
            let start = Instant::now();
            if mode == 0 {
                let mut enc = jpeg_encoder_upstream::Encoder::new(&mut *out, 81);
                enc.set_sampling_factor(jpeg_encoder_upstream::SamplingFactor::F_1_1);
                enc.encode_image(Rows(black_box(rgb.as_raw()), width, height))
                    .unwrap();
            } else if mode == 1 {
                wingman_jpeg::encode_rgb_into(
                    black_box(rgb.as_raw()),
                    width,
                    height,
                    81,
                    wingman_jpeg::Backend::Auto,
                    out,
                )
                .unwrap();
            } else {
                let mut enc = jpeg_encoder::Encoder::new(&mut *out, 81);
                enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
                enc.set_dct_backend(backends[mode - 2]);
                enc.encode_image(Rows(black_box(rgb.as_raw()), width, height))
                    .unwrap();
            }
            black_box(out.len());
            if round > 0 {
                times[mode].push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        for out in &outputs[1..] {
            assert!(out == &outputs[0], "JPEG bytes changed");
        }
    }
    if let Some(directory) = args.get(3) {
        std::fs::write(
            std::path::Path::new(directory).join(format!("rust-{pattern}.jpg")),
            &outputs[0],
        )
        .unwrap();
    }
    println!(
        "pattern={pattern} width={width} height={height} rounds={rounds} arch={}",
        std::env::consts::ARCH
    );
    for (i, name) in names.iter().enumerate() {
        times[i].sort_by(f64::total_cmp);
        println!(
            "{name},median_ms={:.3},p95_ms={:.3},bytes={}",
            times[i][rounds / 2],
            times[i][(rounds * 95).div_ceil(100) - 1],
            outputs[i].len()
        );
    }
}

fn micro() {
    const COUNT: usize = 65536;
    let mut seed = 0x6789abcd_u32;
    for flat in [false, true] {
        let blocks: Vec<[i16; 64]> = (0..2048)
            .map(|_| {
                core::array::from_fn(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    if flat {
                        17
                    } else {
                        (seed as u8) as i16 - 128
                    }
                })
            })
            .collect();
        for backend in [
            DctBackend::Reference,
            DctBackend::Neon,
            DctBackend::NeonFlat,
        ] {
            let mut times = Vec::new();
            for _ in 0..7 {
                let start = Instant::now();
                for n in 0..COUNT {
                    let mut block = *black_box(&blocks[n % blocks.len()]);
                    diagnostics::dct(&mut block, backend);
                    black_box(&block);
                }
                times.push(start.elapsed().as_secs_f64() * 1e9 / COUNT as f64);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "dct flat={flat} backend={backend:?} median_ns={:.3}",
                times[3]
            );
        }
        let mut coefficients = blocks;
        for block in &mut coefficients {
            diagnostics::dct(block, DctBackend::Reference);
        }
        let quantizer = diagnostics::Quantizer::new(81);
        for vector in [false, true] {
            let mut times = Vec::new();
            for _ in 0..7 {
                let start = Instant::now();
                for n in 0..COUNT {
                    let mut output = [0; 64];
                    quantizer.apply(
                        black_box(&coefficients[n % coefficients.len()]),
                        &mut output,
                        vector,
                    );
                    black_box(&output);
                }
                times.push(start.elapsed().as_secs_f64() * 1e9 / COUNT as f64);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "quant flat={flat} vector={vector} median_ns={:.3}",
                times[3]
            );
        }
    }
}
