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

const RECONNECT_ATTEMPTS: u64 = 2;
const RECONNECT_DELAY: Duration = Duration::from_millis(150);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) struct Transport {
    stream: HueStream,
    credentials: Credentials,
    area_id: String,
}

impl Transport {
    pub(super) fn new(credentials: Credentials, area_id: String) -> Result<Self, String> {
        let stream = connect(&credentials, &area_id)?;
        Ok(Self {
            stream,
            credentials,
            area_id,
        })
    }

    pub(super) fn send(&mut self, colors: &[(u8, Rgb)]) -> Result<(), String> {
        self.stream.send(colors)
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
                current.message = format!("Reconnexion au pont… {attempt}/{RECONNECT_ATTEMPTS}");
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
}
