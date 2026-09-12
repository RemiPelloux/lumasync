//! Soft circular hue histogram and weighted perceptual variance.
use super::color::{luminance, mix, Vector};
const BINS: usize = 36;
const CHROMA_FLOOR: f32 = 0.025;
const EPSILON: f32 = 0.000001;
const DOMINANT_BLEND: f32 = 0.65;
const VARIANCE_SCALE: f32 = 0.008;
const OVERLAY_SHARE_LIMIT: f32 = 0.25;
const OVERLAY_BLEND: f32 = 0.7;

#[derive(Clone, Copy, Default)]
struct Moment {
    mass: f32,
    sum: Vector,
}

impl Moment {
    fn add(&mut self, rgb: Vector, weight: f32) {
        self.mass += weight;
        for (sum, value) in self.sum.iter_mut().zip(rgb) {
            *sum += value * weight;
        }
    }
    fn mean(self) -> Vector {
        self.sum.map(|v| v / self.mass.max(EPSILON))
    }
}

pub(super) struct Observation {
    pub rgb: Vector,
    pub lab: Vector,
    lab_square: f32,
    hue_bin: usize,
    hue_fraction: f32,
}

impl Observation {
    #[inline]
    pub fn new(rgb: Vector) -> Self {
        let lab = super::color::to_lab(rgb);
        let chroma_square = lab[1] * lab[1] + lab[2] * lab[2];
        let (hue_bin, hue_fraction) = if chroma_square >= CHROMA_FLOOR * CHROMA_FLOOR {
            let hue = (lab[2].atan2(lab[1]) / std::f32::consts::TAU).rem_euclid(1.0);
            let position = hue * BINS as f32;
            (position as usize % BINS, position.fract())
        } else {
            (BINS, 0.0)
        };
        Self {
            rgb,
            lab,
            lab_square: super::color::dot(lab, lab),
            hue_bin,
            hue_fraction,
        }
    }
}

pub(super) struct Spectrum {
    all: Moment,
    neutral: Moment,
    colored: Moment,
    lab_sum: Vector,
    lab_square: f32,
    bins: [Moment; BINS],
}

impl Default for Spectrum {
    fn default() -> Self {
        Self {
            all: Moment::default(),
            neutral: Moment::default(),
            colored: Moment::default(),
            lab_sum: [0.0; 3],
            lab_square: 0.0,
            bins: [Moment::default(); BINS],
        }
    }
}

impl Spectrum {
    #[inline]
    pub fn add(&mut self, sample: &Observation, weight: f32) {
        self.all.add(sample.rgb, weight);
        for (sum, v) in self.lab_sum.iter_mut().zip(sample.lab) {
            *sum += v * weight;
        }
        self.lab_square += sample.lab_square * weight;
        let bin = sample.hue_bin;
        if bin >= BINS {
            self.neutral.add(sample.rgb, weight);
            return;
        }
        let fraction = sample.hue_fraction;
        self.colored.add(sample.rgb, weight);
        self.bins[bin].add(sample.rgb, weight * (1.0 - fraction));
        self.bins[(bin + 1) % BINS].add(sample.rgb, weight * fraction);
    }

    pub fn resolve(&self) -> Vector {
        let mut average = self.all.mean();
        let neutral_share = self.neutral.mass / self.all.mass.max(EPSILON);
        // Only suppress small bright neutral overlays in predominantly colored
        // content; neutral/white scenes retain their actual white point.
        let share_gate = ((OVERLAY_SHARE_LIMIT - neutral_share) / 0.20).clamp(0.0, 1.0);
        let brightness_gate = ((luminance(self.neutral.mean()) - 0.45) / 0.40).clamp(0.0, 1.0);
        average = mix(
            average,
            self.colored.mean(),
            OVERLAY_BLEND * share_gate * brightness_gate,
        );
        let (dominant, confidence) = self.dominant();
        let mean_lab = self.lab_sum.map(|v| v / self.all.mass.max(EPSILON));
        let variance = (self.lab_square / self.all.mass.max(EPSILON)
            - super::color::dot(mean_lab, mean_lab))
        .max(0.0);
        let coverage = self.colored.mass / self.all.mass.max(EPSILON);
        let blend = DOMINANT_BLEND * confidence * coverage * variance / (variance + VARIANCE_SCALE);
        let matched = match_luminance(dominant, luminance(average));
        mix(average, matched, blend)
    }

    fn dominant(&self) -> (Vector, f32) {
        let clusters: [Moment; BINS] = std::array::from_fn(|i| self.cluster(i));
        let peak = (0..BINS)
            .max_by(|a, b| clusters[*a].mass.total_cmp(&clusters[*b].mass))
            .unwrap_or(0);
        let second = (0..BINS)
            .filter(|i| circular_distance(*i, peak) > 2)
            .map(|i| clusters[i].mass)
            .fold(0.0, f32::max);
        let confidence =
            ((clusters[peak].mass - second) / clusters[peak].mass.max(EPSILON)).clamp(0.0, 1.0);
        (clusters[peak].mean(), confidence)
    }

    fn cluster(&self, index: usize) -> Moment {
        let mut result = self.bins[index];
        for neighbor in [(index + BINS - 1) % BINS, (index + 1) % BINS] {
            result.mass += self.bins[neighbor].mass;
            for i in 0..3 {
                result.sum[i] += self.bins[neighbor].sum[i];
            }
        }
        result
    }
}

fn circular_distance(a: usize, b: usize) -> usize {
    let d = a.abs_diff(b);
    d.min(BINS - d)
}

fn match_luminance(rgb: Vector, target: f32) -> Vector {
    let scale = target / luminance(rgb).max(EPSILON);
    let peak = rgb.iter().copied().fold(EPSILON, f32::max);
    rgb.map(|v| v * scale.min(1.0 / peak))
}
