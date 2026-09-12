use super::bounds::ContentBounds;
use crate::models::Zone;

pub(super) const ZONES: [Zone; 8] = [
    Zone::Left,
    Zone::Right,
    Zone::Top,
    Zone::Bottom,
    Zone::TopLeft,
    Zone::TopRight,
    Zone::BottomLeft,
    Zone::BottomRight,
];
const GRID_WIDTH: u32 = 80;
const GRID_HEIGHT: u32 = 45;
const EDGE_SIGMA: f32 = 0.30;
const CORNER_SIGMA: f32 = 0.22;
const BASE_REACH: f32 = 0.72;
const DEPTH_SCALE: f32 = 75.0;
pub(super) const MIN_WEIGHT: f32 = 0.0001;

pub(super) fn zone_index(zone: Zone) -> usize {
    ZONES.iter().position(|v| *v == zone).unwrap_or(0)
}

pub(super) struct SamplePoint {
    pub x: u32,
    pub y: u32,
    pub weights: [f32; 8],
}

pub(super) struct ConePlan {
    pub bounds: ContentBounds,
    pub points: Vec<SamplePoint>,
}

impl ConePlan {
    pub fn retain_active(&mut self, active: [bool; 8]) {
        self.points.retain(|point| {
            point
                .weights
                .iter()
                .zip(active)
                .any(|(weight, enabled)| enabled && *weight > MIN_WEIGHT)
        });
    }

    pub fn new(bounds: ContentBounds, depth: f32) -> Self {
        let nx = GRID_WIDTH.min(bounds.width());
        let ny = GRID_HEIGHT.min(bounds.height());
        let mut points = Vec::with_capacity((nx * ny) as usize);
        for row in 0..ny {
            for col in 0..nx {
                let u = (col as f32 + 0.5) / nx as f32;
                let v = (row as f32 + 0.5) / ny as f32;
                points.push(SamplePoint {
                    x: bounds.left + (u * bounds.width() as f32) as u32,
                    y: bounds.top + (v * bounds.height() as f32) as u32,
                    weights: ZONES.map(|zone| cone_weight(zone, [u, v], depth)),
                });
            }
        }
        Self { bounds, points }
    }
}

/// Smooth overlapping cones point from each lamp's screen anchor toward the
/// center. Depth expands their reach, with no pixel cap tied to resolution.
fn cone_weight(zone: Zone, point: [f32; 2], depth: f32) -> f32 {
    let anchor: [f32; 2] = match zone {
        Zone::Left => [0.0, 0.5],
        Zone::Right => [1.0, 0.5],
        Zone::Top => [0.5, 0.0],
        Zone::Bottom => [0.5, 1.0],
        Zone::TopLeft => [0.0, 0.0],
        Zone::TopRight => [1.0, 0.0],
        Zone::BottomLeft => [0.0, 1.0],
        Zone::BottomRight => [1.0, 1.0],
    };
    let direction = [0.5 - anchor[0], 0.5 - anchor[1]];
    let length = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
    let direction = direction.map(|v| v / length);
    let delta = [point[0] - anchor[0], point[1] - anchor[1]];
    let inward = delta[0] * direction[0] + delta[1] * direction[1];
    let lateral = delta[0] * direction[1] - delta[1] * direction[0];
    let reach = BASE_REACH + depth.clamp(5.0, 30.0) / DEPTH_SCALE;
    if inward < 0.0 || inward >= reach {
        return 0.0;
    }
    let corner = anchor[0] != 0.5 && anchor[1] != 0.5;
    let sigma = if corner { CORNER_SIGMA } else { EDGE_SIGMA } + inward * 0.25;
    let angular = (-0.5 * (lateral / sigma).powi(2)).exp();
    angular * (1.0 - inward / reach).powi(2)
}
