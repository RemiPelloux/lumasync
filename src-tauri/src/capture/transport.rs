use crate::{
    diagnostics,
    dtls::HueStream,
    models::{Credentials, Rgb, SyncPhase, SyncStatus},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

pub(super) const RECONNECT_ATTEMPTS: u64 = 5;
const RECONNECT_DELAY: Duration = Duration::from_millis(150);
const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_millis(500);
const RECONNECT_BACKOFF_MAX: Duration = Duration::from_secs(8);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(25);
const MIN_SEND_INTERVAL: Duration = Duration::from_millis(24);
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(100);
const MATERIAL_COLOR_DELTA: u8 = 2;

pub(super) struct Transport {
    stream: HueStream,
    credentials: Credentials,
    area_id: String,
    last_sent: Vec<(u8, Rgb)>,
    last_send: Instant,
}

impl Transport {
    pub(super) fn new(credentials: Credentials, area_id: String) -> Result<Self, String> {
        let stream = connect(&credentials, &area_id)?;
        Ok(Self {
            stream,
            credentials,
            area_id,
            last_sent: Vec::new(),
            last_send: Instant::now() - HEARTBEAT_INTERVAL,
        })
    }

    pub(super) fn send(&mut self, colors: &[(u8, Rgb)]) -> Result<(), String> {
        let now = Instant::now();
        let since_last = now.duration_since(self.last_send);
        if !should_emit(&self.last_sent, colors, since_last) {
            return Ok(());
        }
        self.stream.send(colors)?;
        self.last_sent.clear();
        self.last_sent.extend_from_slice(colors);
        self.last_send = now;
        Ok(())
    }

    pub(super) fn reconnect(
        &mut self,
        stop: &AtomicBool,
        status: &Mutex<SyncStatus>,
    ) -> Result<(), String> {
        for attempt in 1..=RECONNECT_ATTEMPTS {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            if let Ok(mut current) = status.lock() {
                current.phase = SyncPhase::Reconnecting;
                current.message =
                    format!("Reconnexion au pont… tentative {attempt}/{RECONNECT_ATTEMPTS}");
            }
            diagnostics::warn(
                "transport.reconnect",
                &format!("Hue reconnect attempt {attempt}/{RECONNECT_ATTEMPTS}."),
            );
            if !wait_for_retry(stop, RECONNECT_DELAY * attempt as u32) {
                return Ok(());
            }
            match connect(&self.credentials, &self.area_id) {
                Ok(stream) => {
                    if stop.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    self.stream = stream;
                    self.last_sent.clear();
                    self.last_send = Instant::now() - HEARTBEAT_INTERVAL;
                    diagnostics::info("transport.reconnected", "Hue connection restored.");
                    mark_running(status);
                    return Ok(());
                }
                Err(error) => diagnostics::warn("transport.reconnect.failed", &error),
            }
        }
        Err("Le flux Hue a été interrompu et la reconnexion a échoué.".to_owned())
    }
}

fn materially_changed(previous: &[(u8, Rgb)], current: &[(u8, Rgb)]) -> bool {
    previous.len() != current.len()
        || previous
            .iter()
            .zip(current)
            .any(|((old_channel, old), (channel, new))| {
                old_channel != channel
                    || old.r.abs_diff(new.r) >= MATERIAL_COLOR_DELTA
                    || old.g.abs_diff(new.g) >= MATERIAL_COLOR_DELTA
                    || old.b.abs_diff(new.b) >= MATERIAL_COLOR_DELTA
            })
}

#[inline]
fn should_emit(previous: &[(u8, Rgb)], current: &[(u8, Rgb)], since_last: Duration) -> bool {
    since_last >= MIN_SEND_INTERVAL
        && (materially_changed(previous, current) || since_last >= HEARTBEAT_INTERVAL)
}

fn wait_for_retry(stop: &AtomicBool, delay: Duration) -> bool {
    let deadline = Instant::now() + delay;
    while !stop.load(Ordering::Acquire) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        std::thread::sleep(remaining.min(STOP_POLL_INTERVAL));
    }
    false
}

/// Delay before retrying a connection after all attempts in one reconnect
/// burst failed. The cap keeps a long bridge outage from spinning the worker
/// while still allowing recovery without restarting capture.
pub(super) fn reconnect_backoff(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(4);
    let multiplier = 1_u32 << exponent;
    RECONNECT_BACKOFF_INITIAL
        .saturating_mul(multiplier)
        .min(RECONNECT_BACKOFF_MAX)
}

fn connect(credentials: &Credentials, area: &str) -> Result<HueStream, String> {
    HueStream::connect(
        &credentials.host,
        &credentials.app_key,
        &credentials.client_key,
        area,
    )
}

pub(super) fn mark_running(status: &Mutex<SyncStatus>) {
    if let Ok(mut current) = status.lock() {
        if !matches!(current.phase, SyncPhase::Starting | SyncPhase::Reconnecting) {
            return;
        }
        current.running = true;
        current.phase = SyncPhase::Running;
        current.message = "Éclairage synchronisé".to_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_interrupts_retry_wait_immediately() {
        let stop = AtomicBool::new(true);
        assert!(!wait_for_retry(&stop, Duration::from_secs(1)));
    }

    #[test]
    fn connection_completion_cannot_overwrite_a_stop_request() {
        let status = Mutex::new(SyncStatus {
            phase: SyncPhase::Stopping,
            ..SyncStatus::default()
        });
        mark_running(&status);
        let status = status.lock().unwrap();
        assert!(matches!(status.phase, SyncPhase::Stopping));
        assert!(!status.running);
    }

    #[test]
    fn tiny_color_deltas_do_not_trigger_network_bursts() {
        let previous = vec![(
            1,
            Rgb {
                r: 100,
                g: 120,
                b: 140,
            },
        )];
        let current = vec![(
            1,
            Rgb {
                r: 101,
                g: 121,
                b: 141,
            },
        )];
        assert!(!materially_changed(&previous, &current));
        let changed = vec![(
            1,
            Rgb {
                r: 103,
                g: 120,
                b: 140,
            },
        )];
        assert!(materially_changed(&previous, &changed));
    }

    #[test]
    fn channel_changes_are_detected_even_when_lengths_match() {
        let previous = vec![(
            1,
            Rgb {
                r: 10,
                g: 10,
                b: 10,
            },
        )];
        let current = vec![(
            2,
            Rgb {
                r: 10,
                g: 10,
                b: 10,
            },
        )];
        assert!(materially_changed(&previous, &current));
    }

    #[test]
    fn emission_is_rate_limited_but_heartbeats_keep_static_streams_alive() {
        let frame = vec![(
            1,
            Rgb {
                r: 20,
                g: 30,
                b: 40,
            },
        )];
        assert!(!should_emit(&[], &frame, Duration::from_millis(1)));
        assert!(should_emit(&[], &frame, MIN_SEND_INTERVAL));
        assert!(!should_emit(&frame, &frame, Duration::from_millis(50)));
        assert!(should_emit(&frame, &frame, HEARTBEAT_INTERVAL));
    }

    #[test]
    fn reconnect_backoff_grows_then_stays_bounded() {
        assert_eq!(reconnect_backoff(0), RECONNECT_BACKOFF_INITIAL);
        assert_eq!(reconnect_backoff(1), Duration::from_millis(500));
        assert_eq!(reconnect_backoff(2), Duration::from_secs(1));
        assert_eq!(reconnect_backoff(3), Duration::from_secs(2));
        assert_eq!(reconnect_backoff(4), Duration::from_secs(4));
        assert_eq!(reconnect_backoff(5), RECONNECT_BACKOFF_MAX);
        assert_eq!(reconnect_backoff(u32::MAX), RECONNECT_BACKOFF_MAX);
    }
}
