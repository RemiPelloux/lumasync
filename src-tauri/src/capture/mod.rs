mod analysis;
#[cfg(test)]
mod benchmarks;
mod bounds;
mod color;
mod cone;
#[cfg(windows)]
mod desktop;
mod frame;
mod metrics;
mod pacing;
mod pipeline;
mod source;
mod spectrum;
#[cfg(test)]
mod statistics_tests;
mod temporal;
#[cfg(test)]
mod tests;
mod transport;

use crate::models::{ChannelAssignment, Credentials, StartSyncRequest, SyncStatus};
use metrics::Metrics;
use pipeline::Pipeline;
use source::ScreenCapture;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use transport::Transport;

const CAPTURE_ERROR_LOG_INTERVAL: Duration = Duration::from_secs(2);

// Compatibility boundary for the existing Tauri worker.
pub fn run_capture(
    credentials: Credentials,
    request: StartSyncRequest,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<SyncStatus>>,
) -> Result<(), String> {
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let monitor = xcap::Monitor::all()
        .map_err(|error| {
            let message = format!("Écrans indisponibles : {error}");
            crate::diagnostics::error("capture.monitor", &message);
            message
        })?
        .into_iter()
        .nth(request.monitor_index)
        .ok_or_else(|| {
            let message = "Le moniteur choisi n’est plus disponible.".to_owned();
            crate::diagnostics::error("capture.monitor", &message);
            message
        })?;
    let mut capture = ScreenCapture::new(monitor);
    capture.update().inspect_err(|error| {
        crate::diagnostics::error("capture.initial_frame", error);
    })?;
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let period = Duration::from_secs_f64(1.0 / request.fps.clamp(10, 60) as f64);
    let transport = Transport::new(credentials, request.area_id.clone())?;
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut pipeline = Pipeline::new(request, &capture.frame);
    let has_valid_frame = can_analyze_frame(capture.frame.protected_content);
    if has_valid_frame {
        pipeline.observe(&capture.frame);
    }
    transport::mark_running(&status);
    crate::diagnostics::info(
        "capture.running",
        "Screen analysis and Hue streaming started.",
    );
    run_loop(
        &mut Worker {
            capture,
            pipeline,
            transport,
            period,
            capture_errors: 0,
            capture_error_logged_at: Instant::now() - CAPTURE_ERROR_LOG_INTERVAL,
            has_valid_frame,
            protected_frames: 0,
            protected_logged_at: Instant::now() - CAPTURE_ERROR_LOG_INTERVAL,
        },
        &stop,
        &status,
    )
}

struct Worker {
    capture: ScreenCapture,
    pipeline: Pipeline,
    transport: Transport,
    period: Duration,
    capture_errors: u32,
    capture_error_logged_at: Instant,
    has_valid_frame: bool,
    protected_frames: u32,
    protected_logged_at: Instant,
}

impl Worker {
    fn tick(&mut self, elapsed: Duration) -> Result<(), String> {
        let fresh = match self.capture.update() {
            Ok(fresh) => {
                self.capture_errors = 0;
                if fresh && !can_analyze_frame(self.capture.frame.protected_content) {
                    self.protected_frames = self.protected_frames.saturating_add(1);
                    let now = Instant::now();
                    if should_log_capture_error(self.protected_logged_at, now) {
                        crate::diagnostics::warn(
                            "capture.protected",
                            &format!(
                                "Contenu protégé masqué par Windows; dernière image conservée ({})",
                                self.protected_frames
                            ),
                        );
                        self.protected_logged_at = now;
                    }
                    false
                } else {
                    self.protected_frames = 0;
                    self.has_valid_frame = self.has_valid_frame || fresh;
                    fresh
                }
            }
            Err(error) => {
                self.capture_errors = self.capture_errors.saturating_add(1);
                let now = Instant::now();
                if should_log_capture_error(self.capture_error_logged_at, now) {
                    crate::diagnostics::warn(
                        "capture.frame.retry",
                        &format!(
                            "Capture indisponible; conservation de la dernière image ({}) : {error}",
                            self.capture_errors
                        ),
                    );
                    self.capture_error_logged_at = now;
                }
                false
            }
        };
        if fresh && self.has_valid_frame {
            self.pipeline.observe(&self.capture.frame);
        }
        // Even without a new desktop frame, converge toward the latest target.
        if self.has_valid_frame {
            self.pipeline.advance(elapsed);
        }
        Ok(())
    }
}

#[inline]
fn should_log_capture_error(last_logged: Instant, now: Instant) -> bool {
    now.duration_since(last_logged) >= CAPTURE_ERROR_LOG_INTERVAL
}

#[inline]
fn can_analyze_frame(protected_content: bool) -> bool {
    !protected_content
}

fn run_loop(
    worker: &mut Worker,
    stop: &AtomicBool,
    status: &Mutex<SyncStatus>,
) -> Result<(), String> {
    let mut metrics = Metrics::new();
    let mut previous = Instant::now() - worker.period;
    let mut deadline = Instant::now();
    let mut reconnect_failures = 0_u32;
    let mut reconnect_after = None;
    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        worker.tick(started.duration_since(previous))?;
        if stop.load(Ordering::Acquire) {
            break;
        }
        previous = started;
        let now = Instant::now();
        // Do not emit an empty Hue frame while the first desktop image is
        // still protected/unavailable. Hue treats an empty update as a request
        // to clear the entertainment channels, which turns every lamp off.
        if worker.has_valid_frame && reconnect_after.is_none() {
            if let Err(error) = worker.transport.send(&worker.pipeline.outgoing) {
                crate::diagnostics::warn("transport.send", &error);
                reconnect_after = Some(now);
            }
        }
        if worker.has_valid_frame && reconnect_after.is_some_and(|retry_at| now >= retry_at) {
            match worker.transport.reconnect(stop, status) {
                Ok(()) => {
                    reconnect_failures = 0;
                    reconnect_after = None;
                    // Discard the old colors after reconnect: capture again next tick.
                }
                Err(error) => {
                    reconnect_failures = reconnect_failures.saturating_add(1);
                    let backoff = transport::reconnect_backoff(reconnect_failures);
                    reconnect_after = Some(Instant::now() + backoff);
                    crate::diagnostics::warn(
                        "transport.reconnect.exhausted",
                        &format!(
                            "Hue reconnect failed after {} attempts; retrying in {:.1}s: {error}",
                            transport::RECONNECT_ATTEMPTS,
                            backoff.as_secs_f32()
                        ),
                    );
                }
            }
        }
        metrics.record(started.elapsed());
        metrics.publish(status, &worker.pipeline);
        deadline = pacing::wait(deadline, worker.period, &mut metrics.dropped);
    }
    Ok(())
}

pub fn validate_assignments(assignments: &[ChannelAssignment]) -> Result<(), String> {
    if assignments.is_empty() {
        return Err("Aucune lumière n’est affectée à l’écran.".to_owned());
    }
    let mut channels = std::collections::HashSet::new();
    for assignment in assignments {
        if !channels.insert(assignment.channel_id) {
            return Err("Une lumière apparaît plusieurs fois dans la configuration.".to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod resilience_tests {
    use super::*;

    #[test]
    fn capture_error_logging_is_rate_limited() {
        let now = Instant::now();
        assert!(should_log_capture_error(
            now - CAPTURE_ERROR_LOG_INTERVAL,
            now
        ));
        assert!(!should_log_capture_error(
            now - CAPTURE_ERROR_LOG_INTERVAL + Duration::from_millis(1),
            now
        ));
    }

    #[test]
    fn protected_video_frames_are_not_used_as_black_analysis_frames() {
        assert!(!can_analyze_frame(true));
        assert!(can_analyze_frame(false));
    }
}
