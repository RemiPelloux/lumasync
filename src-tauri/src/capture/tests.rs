use super::{
    analysis::Analyzer, bounds::*, color::*, cone::*, frame::ScreenFrame, temporal::ColorFilter,
    validate_assignments,
};
use crate::models::{ChannelAssignment, Zone};
use image::{Rgba, RgbaImage};
use std::time::Duration;

fn frame(image: RgbaImage) -> ScreenFrame {
    let mut frame = ScreenFrame::default();
    frame.set_image(image);
    frame
}

fn sample(image: &ScreenFrame, zone: Zone) -> [u8; 3] {
    let bounds = ContentBounds::full(image);
    let mut analyzer = Analyzer::new(bounds, [true; 8], 18.0);
    let lab = analyzer.analyze(image, bounds)[zone_index(zone)];
    let rgb = encode(gamut_map(lab), 1.0, 1.0);
    [rgb.r, rgb.g, rgb.b]
}

#[test]
fn uniform_colors_roundtrip_for_all_zones() {
    for rgb in [
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 255],
        [0, 0, 0],
        [80, 120, 190],
    ] {
        let image = frame(RgbaImage::from_pixel(
            160,
            90,
            Rgba([rgb[0], rgb[1], rgb[2], 255]),
        ));
        for zone in ZONES {
            let actual = sample(&image, zone);
            for i in 0..3 {
                assert!(
                    actual[i].abs_diff(rgb[i]) <= 2,
                    "{zone:?}: {actual:?} != {rgb:?}"
                );
            }
        }
    }
}

#[test]
fn cone_sees_color_away_from_corner() {
    let mut image = RgbaImage::from_pixel(160, 90, Rgba([0, 0, 0, 255]));
    for y in 12..40 {
        for x in 22..64 {
            image.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
    }
    let image = frame(image);
    let near = sample(&image, Zone::TopLeft);
    let far = sample(&image, Zone::BottomRight);
    assert!(near[0] > 80 && near[1] < 3 && near[2] < 3, "{near:?}");
    assert!(near[0] > far[0] + 50, "{near:?} / {far:?}");
}

#[test]
fn cone_weights_mirror_and_depth_expands_coverage() {
    let full = ContentBounds {
        left: 0,
        top: 0,
        right: 160,
        bottom: 90,
    };
    let narrow = ConePlan::new(full, 5.0);
    let wide = ConePlan::new(full, 30.0);
    for (a, b) in narrow.points.iter().zip(narrow.points.iter().rev()) {
        assert!((a.weights[4] - b.weights[7]).abs() < 0.00001);
    }
    assert!(
        wide.points.iter().map(|p| p.weights[4]).sum::<f32>()
            > narrow.points.iter().map(|p| p.weights[4]).sum::<f32>()
    );
}

#[test]
fn sample_budget_is_resolution_independent() {
    let plan = |w, h| {
        ConePlan::new(
            ContentBounds {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            },
            12.0,
        )
    };
    let hd = plan(1920, 1080);
    let uhd = plan(3840, 2160);
    assert_eq!(hd.points.len(), 3600);
    for (a, b) in hd.points.iter().zip(uhd.points.iter()) {
        assert_eq!(a.weights, b.weights);
        assert!(a.x.abs_diff(b.x / 2) <= 1);
    }
}

#[test]
fn linear_lab_roundtrip_and_gamut_preserves_hue() {
    for rgb in [
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [64, 128, 192],
    ] {
        let input = linear(rgb);
        assert!(distance(input, from_lab(to_lab(input))) < 0.000002);
    }
    let lab = [0.65, 0.45, 0.18];
    let mapped = to_lab(gamut_map(lab));
    assert!((mapped[0] - lab[0]).abs() < 0.001);
    assert!((mapped[2] / mapped[1] - lab[2] / lab[1]).abs() < 0.002);
}

#[test]
fn stable_target_keeps_converging_after_first_step() {
    let mut filter = ColorFilter::default();
    let dt = (Duration::from_millis(16), 80.0);
    filter.update([0.5, 0.0, 0.0], dt);
    let target = [0.65, 0.0, 0.0];
    let first = filter.update(target, dt);
    let second = filter.update(target, dt);
    assert!(
        second[0] > first[0],
        "regression: static target froze an intermediate color"
    );
    let mut result = second;
    for _ in 0..5 {
        result = filter.update(target, dt);
    }
    assert!(distance(result, target) < 0.005, "{result:?}");
}

#[test]
fn scene_cut_and_long_gap_reset_prediction() {
    let mut filter = ColorFilter::default();
    let dt = (Duration::from_millis(16), 90.0);
    filter.update([0.5, 0.1, 0.0], dt);
    filter.update([0.51, 0.1, 0.0], dt);
    let target = [0.2, -0.1, 0.0];
    assert_eq!(filter.update(target, dt), target);
    let after_gap = [0.25, 0.0, 0.0];
    assert_eq!(
        filter.update(after_gap, (Duration::from_secs(1), 90.0)),
        after_gap
    );
}

#[test]
fn temporal_ramp_is_bounded_and_settles() {
    let mut filter = ColorFilter::default();
    let dt = (Duration::from_millis(16), 100.0);
    filter.update([0.3, 0.0, 0.0], dt);
    for i in 1..30 {
        let target = [0.3 + i as f32 * 0.005, 0.0, 0.0];
        let output = filter.update(target, dt);
        assert!(distance(output, target) < 0.02);
    }
    let target = [0.445, 0.0, 0.0];
    for _ in 0..20 {
        filter.update(target, dt);
    }
    assert!(distance(filter.update(target, dt), target) < 0.001);
}

#[test]
fn black_bars_are_symmetric_and_temporally_stable() {
    let mut image = RgbaImage::from_pixel(160, 100, Rgba([0, 0, 0, 255]));
    for y in 12..88 {
        for x in 0..160 {
            image.put_pixel(x, y, Rgba([180, 40, 20, 255]));
        }
    }
    let image = frame(image);
    let crop = detect_content_bounds(&image);
    assert_eq!((crop.top, crop.bottom), (12, 88));
    let full = ContentBounds::full(&image);
    let mut tracker = BoundsTracker::new(full);
    assert_eq!(tracker.update(crop), full);
    assert_eq!(tracker.update(crop), full);
    assert_eq!(tracker.update(crop), crop);
    assert_eq!(tracker.update(full), crop);
}

#[test]
fn bgra_and_rgba_produce_identical_colors() {
    let rgba = frame(RgbaImage::from_pixel(160, 90, Rgba([200, 70, 30, 255])));
    let mut bgra = frame(RgbaImage::from_pixel(160, 90, Rgba([30, 70, 200, 255])));
    bgra.bgra = true;
    assert_eq!(sample(&rgba, Zone::Left), sample(&bgra, Zone::Left));
}

#[test]
fn brightness_and_ceiling_preserve_channel_ratios() {
    let rgb = encode(linear([240, 120, 60]), 0.5, 1.0);
    assert_eq!([rgb.r, rgb.g, rgb.b], [120, 60, 30]);
    let capped = encode(linear([240, 120, 60]), 1.0, 0.5);
    assert!(capped.r <= 128 && capped.g <= 64 && capped.b <= 32);
}

#[test]
fn channel_validation_rejects_duplicates_and_empty() {
    assert!(validate_assignments(&[]).is_err());
    let one = ChannelAssignment {
        channel_id: 1,
        zone: Zone::Left,
    };
    assert!(validate_assignments(&[one.clone(), one.clone()]).is_err());
    assert!(validate_assignments(&[one]).is_ok());
}

#[test]
fn small_and_black_frames_are_finite() {
    for size in [1, 2, 16] {
        let image = frame(RgbaImage::new(size, size));
        for zone in ZONES {
            assert_eq!(sample(&image, zone), [0, 0, 0]);
        }
    }
}

#[test]
fn every_cone_includes_the_center_even_on_a_single_pixel_frame() {
    let image = frame(RgbaImage::from_pixel(1, 1, Rgba([255, 255, 255, 255])));
    for zone in ZONES {
        assert_eq!(sample(&image, zone), [255, 255, 255]);
    }
}

#[test]
fn inactive_zones_do_not_change_active_colors_after_recropping() {
    let image = frame(RgbaImage::from_fn(160, 100, |x, y| {
        Rgba([(x * 5) as u8, (y * 3) as u8, (x + y) as u8, 255])
    }));
    let full = ContentBounds::full(&image);
    let crop = ContentBounds {
        top: 13,
        bottom: 87,
        ..full
    };
    let mut reference = Analyzer::new(full, [true; 8], 5.0);
    for zone in 0..8 {
        let active = std::array::from_fn(|i| i == zone);
        let mut analyzer = Analyzer::new(full, active, 5.0);
        for bounds in [full, crop, full] {
            let expected = reference.analyze(&image, bounds);
            let actual = analyzer.analyze(&image, bounds);
            assert_eq!(actual[zone], expected[zone]);
            assert!(actual
                .iter()
                .enumerate()
                .all(|(i, lab)| i == zone || *lab == [0.0; 3]));
        }
    }
}

#[test]
fn crop_change_requires_consecutive_confirmations() {
    let full = ContentBounds {
        left: 0,
        top: 0,
        right: 160,
        bottom: 100,
    };
    let first = ContentBounds {
        top: 12,
        bottom: 88,
        ..full
    };
    let next = ContentBounds {
        top: 20,
        bottom: 80,
        ..full
    };
    let mut tracker = BoundsTracker::new(full);
    for _ in 0..3 {
        tracker.update(first);
    }
    assert_eq!(tracker.update(next), first);
    assert_eq!(tracker.update(next), first);
    assert_eq!(tracker.update(full), first);
    assert_eq!(tracker.update(next), first);
    assert_eq!(tracker.update(next), first);
    assert_eq!(tracker.update(next), next);
    for _ in 0..11 {
        assert_eq!(tracker.update(full), next);
    }
    assert_eq!(tracker.update(full), full);
}

#[test]
fn bars_do_not_trim_content_between_coarse_scan_steps() {
    let image = frame(RgbaImage::from_fn(160, 100, |_, y| {
        Rgba(if (13..87).contains(&y) {
            [180, 40, 20, 255]
        } else {
            [0, 0, 0, 255]
        })
    }));
    let crop = detect_content_bounds(&image);
    assert_eq!((crop.top, crop.bottom), (13, 87));
}

#[test]
fn a_single_dark_edge_is_not_a_pair_of_black_bars() {
    let image = frame(RgbaImage::from_fn(160, 360, |_, y| {
        Rgba(if y < 3 {
            [0, 0, 0, 255]
        } else {
            [100, 100, 100, 255]
        })
    }));
    assert_eq!(detect_content_bounds(&image), ContentBounds::full(&image));
}

#[test]
fn percentile_includes_content_at_the_far_end_of_a_scanline() {
    let image = frame(RgbaImage::from_fn(160, 100, |x, y| {
        let value = if (12..88).contains(&y) {
            100
        } else if x >= 150 {
            24
        } else {
            0
        };
        Rgba([value, value, value, 255])
    }));
    assert_eq!(detect_content_bounds(&image), ContentBounds::full(&image));
}
