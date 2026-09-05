use super::{
    bounds::ContentBounds,
    color::{to_lab, Vector},
    cone::ConePlan,
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
        Self {
            plan: ConePlan::new(bounds, depth),
            active,
            depth,
        }
    }

    pub fn analyze(&mut self, image: &ScreenFrame, bounds: ContentBounds) -> [Vector; 8] {
        if self.plan.bounds != bounds {
            self.plan = ConePlan::new(bounds, self.depth);
        }
        let mut spectra: [Spectrum; 8] = std::array::from_fn(|_| Spectrum::default());
        for point in &self.plan.points {
            let rgb = super::color::linear(image.rgb(point.x, point.y));
            let observation = Observation::new(rgb);
            for (i, spectrum) in spectra.iter_mut().enumerate() {
                if self.active[i] && point.weights[i] > 0.0001 {
                    spectrum.add(&observation, point.weights[i]);
                }
            }
        }
        std::array::from_fn(|i| to_lab(spectra[i].resolve()))
    }
}
