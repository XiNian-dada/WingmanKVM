//! Native ARM smoke/parity checks on a private capture; no device access.
use std::time::Instant;
use wingman_jpeg_native::{transcode_into, Mode, Options};

fn main() {
    let input = std::fs::read(std::env::args().nth(1).expect("INPUT.jpg")).unwrap();
    let reference = image::load_from_memory(&input).unwrap().into_rgb8();
    for gray in [false, true] {
        for huffman in [false, true] {
            for quality in [35, 81, 100] {
                let mut standard = Vec::new();
                let mut screen = Vec::new();
                for (mode, output) in [(Mode::Standard, &mut standard), (Mode::Screen, &mut screen)]
                {
                    transcode_into(
                        &input,
                        Options {
                            quality,
                            grayscale: gray,
                            mode,
                            optimize_huffman: huffman,
                        },
                        output,
                    )
                    .unwrap();
                }
                assert!(standard == screen, "screen output differs from standard");
                for mode in [Mode::Coefficient, Mode::Preserve] {
                    let mut output = Vec::new();
                    let start = Instant::now();
                    transcode_into(
                        &input,
                        Options {
                            quality,
                            grayscale: gray,
                            mode,
                            optimize_huffman: huffman,
                        },
                        &mut output,
                    )
                    .unwrap();
                    let ms = start.elapsed().as_secs_f64() * 1000.;
                    let decoded = image::load_from_memory(&output).unwrap().into_rgb8();
                    assert_eq!(decoded.dimensions(), reference.dimensions());
                    if matches!(mode, Mode::Preserve) && !gray {
                        assert!(decoded == reference, "preserve changed source pixels");
                    }
                    if gray {
                        assert!(decoded.pixels().all(|p| p[0] == p[1] && p[1] == p[2]));
                    }
                    if quality == 81 && !huffman {
                        println!(
                            "mode={mode:?} gray={gray} bytes={} native_ms={ms:.3}",
                            output.len()
                        );
                    }
                }
            }
        }
    }
    println!(
        "all 48 native operations passed; source {}x{}",
        reference.width(),
        reference.height()
    );
}
