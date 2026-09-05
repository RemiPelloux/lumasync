use super::{
    color::*,
    spectrum::{Observation, Spectrum},
    temporal::{alpha, ColorFilter},
};
use std::time::Duration;

fn mixture(samples: &[(Vector, f32)]) -> Vector {
    let mut spectrum = Spectrum::default();
    for (rgb, weight) in samples {
        spectrum.add(&Observation::new(*rgb), *weight);
    }
    spectrum.resolve()
}

#[test]
fn equal_distinct_hues_do_not_choose_an_arbitrary_winner() {
    let red = linear([255, 0, 0]);
    let blue = linear([0, 0, 255]);
    let actual = mixture(&[(red, 0.5), (blue, 0.5)]);
    assert!(distance(actual, [0.5, 0.0, 0.5]) < 0.0001, "{actual:?}");
}

#[test]
fn dominant_hue_gains_chroma_without_brightness_bias() {
    let red = linear([255, 0, 0]);
    let blue = linear([0, 0, 255]);
    let actual = mixture(&[(red, 0.2), (blue, 0.8)]);
    assert!(actual[2] > 0.8 && actual[0] < 0.2, "{actual:?}");
    assert!(luminance(actual) <= luminance([0.2, 0.0, 0.8]) + 0.0001);
}

#[test]
fn hue_wrap_is_continuous() {
    let a = gamut_map([0.6, 0.1, -0.0001]);
    let b = gamut_map([0.6, 0.1, 0.0001]);
    let mixed = mixture(&[(a, 0.5), (b, 0.5)]);
    assert!(distance(to_lab(mixed), [0.6, 0.1, 0.0]) < 0.0001);
}

#[test]
fn small_white_overlays_are_suppressed_but_white_scenes_remain_white() {
    let blue = linear([10, 40, 180]);
    let white = [1.0; 3];
    let actual = mixture(&[(blue, 0.9), (white, 0.1)]);
    assert!(actual[0] < mix(blue, white, 0.1)[0]);
    assert!(distance(mixture(&[(white, 1.0)]), white) < 0.0001);
}

#[test]
fn overlay_gate_has_no_threshold_jump() {
    let blue = linear([10, 40, 180]);
    let a = mixture(&[(blue, 0.801), ([1.0; 3], 0.199)]);
    let b = mixture(&[(blue, 0.799), ([1.0; 3], 0.201)]);
    assert!(distance(to_lab(a), to_lab(b)) < 0.005);
}

#[test]
fn exponential_response_is_time_based() {
    let a = alpha(8.0, 0.016);
    assert!((1.0 - (1.0 - a).powi(2) - alpha(8.0, 0.032)).abs() < 0.00001);
}

#[test]
fn quiet_noise_is_attenuated_without_stopping_convergence() {
    let mut filter = ColorFilter::default();
    let dt = (Duration::from_millis(16), 80.0);
    filter.update([0.5, 0.0, 0.0], dt);
    let mut square = 0.0;
    for i in 0..120 {
        let target = [0.5 + if i % 2 == 0 { 0.0015 } else { -0.0015 }, 0.0, 0.0];
        square += (filter.update(target, dt)[0] - 0.5).powi(2);
    }
    assert!((square / 120.0).sqrt() < 0.0008);
}
