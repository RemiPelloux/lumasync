use super::{analysis::Analyzer, bounds::ContentBounds, frame::ScreenFrame};
use std::{hint::black_box, time::Instant};

#[test]
#[ignore = "Release-mode synthetic CPU benchmark; no Hue traffic"]
fn analysis_benchmark() {
    let mut frame = ScreenFrame {
        width: 3840,
        height: 2160,
        pixels: vec![0; 3840 * 2160 * 4],
        bgra: false,
    };
    for (i, pixel) in frame.pixels.chunks_exact_mut(4).enumerate() {
        pixel.copy_from_slice(&[
            (i % 251) as u8,
            ((i / 7) % 241) as u8,
            ((i / 13) % 239) as u8,
            255,
        ]);
    }
    let bounds = ContentBounds::full(&frame);
    let mut analyzer = Analyzer::new(bounds, [true; 8], 18.0);
    for _ in 0..20 {
        black_box(analyzer.analyze(&frame, bounds));
    }
    let mut times = Vec::with_capacity(500);
    for _ in 0..500 {
        let start = Instant::now();
        black_box(analyzer.analyze(black_box(&frame), bounds));
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "4K / 8 zones / 3600 samples: p50={:.3}ms p95={:.3}ms p99={:.3}ms",
        times[250], times[475], times[495]
    );
}
