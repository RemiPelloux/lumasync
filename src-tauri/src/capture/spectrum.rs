//! Soft circular hue histogram and weighted perceptual variance.
use super::color::{luminance, mix, Vector};
const BINS: usize = 36;
const CHROMA_FLOOR: f32 = 0.025;
const EPSILON: f32 = 0.000001;
const DOMINANT_BLEND: f32 = 0.65;
const VARIANCE_SCALE: f32 = 0.008;
const DOMINANT_COVERAGE_FLOOR: f32 = 0.10;
const OVERLAY_SHARE_LIMIT: f32 = 0.25;
const OVERLAY_BLEND: f32 = 0.7;
const COLORED_COVERAGE_FLOOR: f32 = 0.18;

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

#[derive(Clone, Copy)]
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

    #[inline]
    pub fn from_pixel(pixel: [u8; 3]) -> Self {
        Self::new(super::color::linear(pixel))
    }
}

const OBSERVATION_CACHE_SIZE: usize = 64;

pub(super) struct ObservationCache {
    entries: [Option<(u32, Observation)>; OBSERVATION_CACHE_SIZE],
}

impl Default for ObservationCache {
    fn default() -> Self {
        Self {
            entries: [None; OBSERVATION_CACHE_SIZE],
        }
    }
}

impl ObservationCache {
    #[inline]
    pub fn get(&mut self, pixel: [u8; 3]) -> Observation {
        let key = u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]]);
        let index = (key.wrapping_mul(0x9e37_79b9) >> 26) as usize;
        if let Some((cached_key, observation)) = self.entries[index] {
            if cached_key == key {
                return observation;
            }
        }
        let observation = Observation::from_pixel(pixel);
        self.entries[index] = Some((key, observation));
        observation
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
        let coverage = self.colored.mass / self.all.mass.max(EPSILON);
        // When most of a cone is neutral, attenuate a small saturated patch
        // before dominant-hue enhancement. Full-screen dark colours still
        // have full coverage and retain their intended chroma.
        let colored_gate = (coverage / COLORED_COVERAGE_FLOOR).clamp(0.0, 1.0);
        average = mix(self.neutral.mean(), average, colored_gate);
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
        // A small colored accent must not repaint an otherwise dark/neutral
        // zone. Require colored coverage twice: once for confidence and again
        // as an area penalty. Full-screen saturated scenes keep their
        // dominant-hue enhancement while subtitles, progress bars and
        // isolated highlights stay local to the samples that contain them.
        // Ramp the dominant hue in only after a meaningful part of the cone
        // carries colour. This keeps a thin status bar or a protected-video
        // overlay from turning an otherwise neutral top zone blue.
        let coverage_gate = (coverage / DOMINANT_COVERAGE_FLOOR).clamp(0.0, 1.0);
        let blend = DOMINANT_BLEND * confidence * coverage * coverage * coverage_gate * variance
            / (variance + VARIANCE_SCALE);
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

#[cfg(test)]
mod tests {
    use super::ObservationCache;

    #[test]
    fn cache_collisions_never_change_observation_values() {
        let mut cache = ObservationCache::default();
        // Black and [0, 0, 34] intentionally share a direct-mapped slot.
        let colors = [[0, 0, 0], [0, 0, 34], [1, 2, 3], [255, 127, 63]];
        for pixel in colors {
            assert_eq!(cache.get(pixel).rgb, super::super::color::linear(pixel));
        }
        for pixel in colors.into_iter().rev() {
            assert_eq!(cache.get(pixel).rgb, super::super::color::linear(pixel));
        }
    }
}
