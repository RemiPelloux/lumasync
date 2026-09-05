use super::{
    analysis::Analyzer,
    bounds::{detect_content_bounds, BoundsTracker, ContentBounds},
    color::{self, Vector},
    cone::zone_index,
    frame::ScreenFrame,
    temporal::ColorFilter,
};
use crate::models::{Rgb, StartSyncRequest};
use std::time::{Duration, Instant};
const BOUNDS_INTERVAL: Duration = Duration::from_millis(50);

pub(super) struct Pipeline {
    request: StartSyncRequest,
    analyzer: Analyzer,
    bounds: BoundsTracker,
    bounds_at: Instant,
    targets: [Vector; 8],
    filters: [ColorFilter; 8],
    pub(super) outgoing: Vec<(u8, Rgb)>,
}

impl Pipeline {
    pub(super) fn new(request: StartSyncRequest, frame: &ScreenFrame) -> Self {
        let bounds = ContentBounds::full(frame);
        let mut active = [false; 8];
        for assignment in &request.assignments {
            active[zone_index(assignment.zone)] = true;
        }
        let analyzer = Analyzer::new(bounds, active, request.edge_depth);
        let capacity = request.assignments.len();
        Self {
            request,
            analyzer,
            bounds: BoundsTracker::new(bounds),
            bounds_at: Instant::now() - BOUNDS_INTERVAL,
            targets: [[0.0; 3]; 8],
            filters: std::array::from_fn(|_| ColorFilter::default()),
            outgoing: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn observe(&mut self, frame: &ScreenFrame) {
        let full = ContentBounds::full(frame);
        if self.bounds.full != full {
            self.bounds = BoundsTracker::new(full);
            self.bounds_at = Instant::now() - BOUNDS_INTERVAL;
        }
        if self.request.black_bar_detection && self.bounds_at.elapsed() >= BOUNDS_INTERVAL {
            self.bounds.update(detect_content_bounds(frame));
            self.bounds_at = Instant::now();
        }
        self.targets = self.analyzer.analyze(frame, self.bounds.stable);
        for target in &mut self.targets {
            target[1] *= self.request.saturation / 100.0;
            target[2] *= self.request.saturation / 100.0;
        }
    }

    pub(super) fn advance(&mut self, elapsed: Duration) {
        let colors: [Rgb; 8] = std::array::from_fn(|i| {
            let lab = self.filters[i].update(self.targets[i], (elapsed, self.request.reactivity));
            color::encode(
                color::gamut_map(lab),
                self.request.brightness / 100.0,
                self.request.max_luminosity / 100.0,
            )
        });
        self.outgoing.clear();
        for assignment in &self.request.assignments {
            self.outgoing
                .push((assignment.channel_id, colors[zone_index(assignment.zone)]));
        }
    }

    pub(super) fn black_bars(&self) -> bool {
        self.bounds.stable != self.bounds.full
    }
}
