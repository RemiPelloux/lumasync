//! Linear sRGB and Oklab (Björn Ottosson, D65 matrices).
use crate::models::Rgb;
use std::sync::OnceLock;

pub(super) type Vector = [f32; 3];
const LUMA: Vector = [0.2126, 0.7152, 0.0722];

pub(super) fn linear(pixel: [u8; 3]) -> Vector {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    let lut = LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let v = i as f32 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        })
    });
    pixel.map(|v| lut[v as usize])
}

pub(super) fn luminance(rgb: Vector) -> f32 {
    dot(rgb, LUMA)
}
pub(super) fn dot(a: Vector, b: Vector) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
pub(super) fn mix(a: Vector, b: Vector, t: f32) -> Vector {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
pub(super) fn distance(a: Vector, b: Vector) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

pub(super) fn to_lab(rgb: Vector) -> Vector {
    let l = dot(rgb, [0.41222146, 0.53633255, 0.051445995]).cbrt();
    let m = dot(rgb, [0.2119035, 0.6806995, 0.10739696]).cbrt();
    let s = dot(rgb, [0.08830246, 0.28171885, 0.6299787]).cbrt();
    [
        dot([l, m, s], [0.21045426, 0.7936178, -0.004072047]),
        dot([l, m, s], [1.9779985, -2.4285922, 0.4505937]),
        dot([l, m, s], [0.025904037, 0.78277177, -0.80867577]),
    ]
}

pub(super) fn from_lab(lab: Vector) -> Vector {
    let l = dot(lab, [1.0, 0.39633778, 0.21580376]).powi(3);
    let m = dot(lab, [1.0, -0.105561346, -0.06385417]).powi(3);
    let s = dot(lab, [1.0, -0.08948418, -1.2914855]).powi(3);
    [
        dot([l, m, s], [4.0767417, -3.3077116, 0.23096994]),
        dot([l, m, s], [-1.268438, 2.6097574, -0.34131938]),
        dot([l, m, s], [-0.0041960863, -0.7034186, 1.7076147]),
    ]
}

/// Reduce chroma at fixed perceptual lightness/hue instead of clipping RGB.
pub(super) fn gamut_map(lab: Vector) -> Vector {
    let rgb = from_lab(lab);
    if in_gamut(rgb) {
        return rgb.map(|v| v.clamp(0.0, 1.0));
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..12 {
        let scale = (low + high) * 0.5;
        if in_gamut(from_lab([lab[0], lab[1] * scale, lab[2] * scale])) {
            low = scale;
        } else {
            high = scale;
        }
    }
    from_lab([lab[0].clamp(0.0, 1.0), lab[1] * low, lab[2] * low]).map(|v| v.clamp(0.0, 1.0))
}

fn in_gamut(rgb: Vector) -> bool {
    rgb.iter().all(|v| (-0.00001..=1.00001).contains(v))
}

pub(super) fn encode(rgb: Vector, brightness: f32, ceiling: f32) -> Rgb {
    let mut encoded = rgb.map(|v| {
        let v = v.clamp(0.0, 1.0);
        let srgb = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        srgb * brightness.clamp(0.0, 1.0)
    });
    let peak = encoded.iter().copied().fold(0.0, f32::max);
    let scale = (ceiling / peak.max(0.00001)).min(1.0);
    encoded = encoded.map(|v| (v * scale * 255.0).round().clamp(0.0, 255.0));
    Rgb {
        r: encoded[0] as u8,
        g: encoded[1] as u8,
        b: encoded[2] as u8,
    }
}
