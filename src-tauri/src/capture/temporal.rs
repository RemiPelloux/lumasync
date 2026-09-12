//! Speed-adaptive low-pass filtering with a short, bounded color extrapolation.
//! Derivatives come from observations, never from the filter's own error.
use super::color::{distance, dot, mix, Vector};
use std::time::Duration;

const SCENE_CUT: f32 = 0.24;
const MAX_GAP: f32 = 0.25;
const DERIVATIVE_CUTOFF: f32 = 12.0;
const PREDICTION_SECONDS: f32 = 0.012;
const MAX_PREDICTION: f32 = 0.015;
const NOISE_FLOOR: f32 = 0.004;
const SETTLED_ERROR: f32 = 0.00001;
const BASE_CUTOFF: f32 = 2.0;
const RESPONSE_CUTOFF: f32 = 8.0;
const SPEED_GAIN: f32 = 6.0;

#[derive(Default)]
pub(super) struct ColorFilter {
    value: Vector,
    previous: Option<Vector>,
    velocity: Vector,
}

impl ColorFilter {
    pub fn update(&mut self, target: Vector, timing: (Duration, f32)) -> Vector {
        let (elapsed, reactivity) = timing;
        let Some(previous) = self.previous.replace(target) else {
            self.value = target;
            return target;
        };
        let dt = elapsed.as_secs_f32().clamp(0.001, MAX_GAP);
        if distance(previous, target) > SCENE_CUT || elapsed.as_secs_f32() > MAX_GAP {
            self.value = target;
            self.velocity = [0.0; 3];
            return target;
        }
        self.advance(previous, target, (dt, reactivity))
    }

    fn advance(&mut self, previous: Vector, target: Vector, timing: (f32, f32)) -> Vector {
        let (dt, reactivity) = timing;
        let observation = if distance(previous, target) < NOISE_FLOOR {
            [0.0; 3]
        } else {
            std::array::from_fn(|i| (target[i] - previous[i]) / dt)
        };
        let coherent = dot(observation, self.velocity) > 0.0;
        self.velocity = mix(self.velocity, observation, alpha(DERIVATIVE_CUTOFF, dt));
        let speed = dot(self.velocity, self.velocity).sqrt();
        let response = (reactivity / 100.0).clamp(0.1, 1.0);
        // Keep the reactivity control linear so the middle of the range is
        // responsive enough for video without making the low end unstable.
        // The previous squared curve kept 50% reactivity close to the base
        // cutoff, making transitions feel noticeably delayed.
        let cutoff = BASE_CUTOFF + RESPONSE_CUTOFF * response + speed * SPEED_GAIN;
        let predicted = predict(target, self.velocity, coherent);
        self.value = mix(self.value, predicted, alpha(cutoff, dt));
        // Stable targets still converge. The old held-target early return froze them.
        if distance(target, self.value) < SETTLED_ERROR {
            self.value = target;
        }
        self.value
    }
}

fn predict(target: Vector, velocity: Vector, coherent: bool) -> Vector {
    if !coherent {
        return target;
    }
    let speed = dot(velocity, velocity).sqrt();
    let horizon = PREDICTION_SECONDS.min(MAX_PREDICTION / speed.max(0.00001));
    std::array::from_fn(|i| target[i] + velocity[i] * horizon)
}

pub(super) fn alpha(cutoff: f32, dt: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * cutoff * dt).exp()
}
