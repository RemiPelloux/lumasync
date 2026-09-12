use super::{
    bounds::ContentBounds,
    color::{to_lab, Vector},
    cone::{ConePlan, MIN_WEIGHT},
    frame::ScreenFrame,
    spectrum::{Observation, Spectrum},
};

pub(super) struct Analyzer {
    plan: ConePlan,
    active: [bool; 8],
    depth: f32,
}

impl Analyzer {
    pub fn new(bounds: ContentBounds, active: [bool; 8], depth: f32) -> Self {
        let mut plan = ConePlan::new(bounds, depth);
        plan.retain_active(active);
        Self {
            plan,
            active,
            depth,
        }
    }

    pub fn analyze(&mut self, image: &ScreenFrame, bounds: ContentBounds) -> [Vector; 8] {
        if self.plan.bounds != bounds {
            self.plan = ConePlan::new(bounds, self.depth);
            self.plan.retain_active(self.active);
        }
        let mut spectra: [Spectrum; 8] = std::array::from_fn(|_| Spectrum::default());
        let mut previous = [0; 3];
        let mut observation = Observation::new([0.0; 3]);
        for point in &self.plan.points {
            let pixel = image.rgb(point.x, point.y);
            if pixel != previous {
                observation = Observation::new(super::color::linear(pixel));
                previous = pixel;
            }
            for (i, spectrum) in spectra.iter_mut().enumerate() {
                if self.active[i] && point.weights[i] > MIN_WEIGHT {
                    spectrum.add(&observation, point.weights[i]);
                }
            }
        }
        std::array::from_fn(|i| to_lab(spectra[i].resolve()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_colors_match_independent_per_pixel_accumulation() {
        let palette = [
            [0, 0, 0],
            [230, 15, 50],
            [0, 0, 0],
            [30, 180, 240],
            [255; 3],
        ];
        let mut image = ScreenFrame {
            width: 160,
            height: 100,
            ..Default::default()
        };
        for i in 0..16000 {
            image
                .pixels
                .extend_from_slice(&palette[(i / 23) % palette.len()]);
            image.pixels.push(255);
        }
        let full = ContentBounds::full(&image);
        let crop = ContentBounds {
            top: 13,
            bottom: 87,
            ..full
        };
        let active = [true, false, false, true, false, false, false, true];
        let mut analyzer = Analyzer::new(full, active, 18.0);
        for bgra in [false, true] {
            image.bgra = bgra;
            for bounds in [full, crop, full] {
                let mut reference: [Spectrum; 8] = std::array::from_fn(|_| Spectrum::default());
                for point in ConePlan::new(bounds, 18.0).points {
                    let observation =
                        Observation::new(super::super::color::linear(image.rgb(point.x, point.y)));
                    for (i, spectrum) in reference.iter_mut().enumerate() {
                        if active[i] && point.weights[i] > MIN_WEIGHT {
                            spectrum.add(&observation, point.weights[i]);
                        }
                    }
                }
                let expected = std::array::from_fn(|i| to_lab(reference[i].resolve()));
                assert_eq!(analyzer.analyze(&image, bounds), expected);
            }
        }
    }
}
