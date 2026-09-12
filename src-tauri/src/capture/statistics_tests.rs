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
fn gamut_mapping_clamps_extrapolated_lightness() {
    for lightness in [-0.2, 1.2] {
        let rgb = gamut_map([lightness, 0.2, -0.1]);
        assert!(rgb.iter().all(|value| (0.0..=1.0).contains(value)));
        assert!((0.0..=1.0).contains(&to_lab(rgb)[0]));
    }
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
fn higher_reactivity_reduces_transition_lag() {
    let dt = (Duration::from_millis(16), 50.0);
    let mut low = ColorFilter::default();
    let mut high = ColorFilter::default();
    low.update([0.3, 0.0, 0.0], (dt.0, 10.0));
    high.update([0.3, 0.0, 0.0], (dt.0, 10.0));
    let (mut low_output, mut high_output) = ([0.2, 0.0, 0.0], [0.2, 0.0, 0.0]);
    for _ in 0..3 {
        low_output = low.update([0.4, 0.0, 0.0], dt);
        high_output = high.update([0.4, 0.0, 0.0], (dt.0, 90.0));
    }
    let low_error = distance(low_output, [0.4, 0.0, 0.0]);
    let high_error = distance(high_output, [0.4, 0.0, 0.0]);
    assert!(
        high_error < low_error * 0.8,
        "low={low_error} high={high_error}"
    );
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

#[test]
fn quiet_noise_stays_stable_at_maximum_reactivity() {
    let mut filter = ColorFilter::default();
    let dt = (Duration::from_millis(16), 100.0);
    filter.update([0.5, 0.0, 0.0], dt);
    let mut square = 0.0;
    for i in 0..120 {
        let target = [0.5 + if i % 2 == 0 { 0.0018 } else { -0.0018 }, 0.0, 0.0];
        square += (filter.update(target, dt)[0] - 0.5).powi(2);
    }
    assert!(
        (square / 120.0).sqrt() < 0.001,
        "rms={}",
        (square / 120.0).sqrt()
    );
}

#[test]
fn reversing_small_changes_do_not_trigger_speed_boost() {
    let dt = (Duration::from_millis(16), 80.0);
    let mut filter = ColorFilter::default();
    filter.update([0.45, 0.02, 0.0], dt);
    let mut outputs = Vec::new();
    for i in 0..24 {
        let target = if i % 2 == 0 {
            [0.452, 0.021, 0.0]
        } else {
            [0.448, 0.019, 0.0]
        };
        outputs.push(filter.update(target, dt));
    }
    let spread = outputs
        .iter()
        .map(|value| distance(*value, [0.45, 0.02, 0.0]))
        .fold(0.0, f32::max);
    assert!(spread < 0.002, "oscillation spread={spread}");
}

#[test]
fn prediction_never_overshoots_observed_components() {
    let dt = (Duration::from_millis(16), 100.0);
    let mut filter = ColorFilter::default();
    filter.update([0.25, -0.12, 0.08], dt);
    let mut previous: [f32; 3] = [0.25, -0.12, 0.08];
    for target in [
        [0.38, -0.04, 0.02],
        [0.52, 0.06, -0.03],
        [0.30, 0.14, -0.08],
        [0.44, 0.02, 0.04],
    ] {
        let output = filter.update(target, dt);
        for i in 0..3 {
            let low = previous[i].min(target[i]);
            let high = previous[i].max(target[i]);
            assert!(
                (low..=high).contains(&output[i]),
                "component {i} overshot: previous={} target={} output={}",
                previous[i],
                target[i],
                output[i]
            );
        }
        previous = output;
    }
}
