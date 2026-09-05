use super::pipeline::Pipeline;
use crate::models::SyncStatus;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
const PREVIEW_INTERVAL: Duration = Duration::from_millis(100);
const METRICS_INTERVAL: Duration = Duration::from_millis(500);

pub(super) struct Metrics {
    preview_at: Instant,
    measured_at: Instant,
    frames: u32,
    work_ms: f32,
    pub(super) dropped: u64,
}

impl Metrics {
    pub(super) fn new() -> Self {
        Self {
            preview_at: Instant::now(),
            measured_at: Instant::now(),
            frames: 0,
            work_ms: 0.0,
            dropped: 0,
        }
    }

    pub(super) fn record(&mut self, work: Duration) {
        self.frames += 1;
        self.work_ms += work.as_secs_f32() * 1000.0;
    }

    pub(super) fn publish(&mut self, status: &Mutex<SyncStatus>, pipeline: &Pipeline) {
        let preview = self.preview_at.elapsed() >= PREVIEW_INTERVAL;
        let metrics = self.measured_at.elapsed() >= METRICS_INTERVAL;
        if !preview && !metrics {
            return;
        }
        if let Ok(mut current) = status.lock() {
            if preview {
                current.colors.clear();
                for (channel, color) in &pipeline.outgoing {
                    current.colors.insert(channel.to_string(), color.hex());
                }
                current.black_bars_detected = pipeline.black_bars();
                self.preview_at = Instant::now();
            }
            if metrics {
                current.measured_fps =
                    self.frames as f32 / self.measured_at.elapsed().as_secs_f32();
                current.frame_time_ms = self.work_ms / self.frames.max(1) as f32;
                current.dropped_frames = self.dropped;
                self.measured_at = Instant::now();
                self.frames = 0;
                self.work_ms = 0.0;
            }
        }
    }
}
