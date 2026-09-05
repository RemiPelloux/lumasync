use crate::{
    dtls::HueStream,
    models::{Credentials, Rgb, SyncPhase, SyncStatus},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

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
        for attempt in 1..=2 {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            if let Ok(mut current) = status.lock() {
                current.phase = SyncPhase::Reconnecting;
                current.message = format!("Reconnexion au pont… {attempt}/2");
            }
            std::thread::sleep(Duration::from_millis(150 * attempt));
            if let Ok(stream) = connect(&self.credentials, &self.area_id) {
                self.stream = stream;
                mark_running(status);
                return Ok(());
            }
        }
        Err("Le flux Hue a été interrompu et la reconnexion a échoué.".to_owned())
    }
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
        current.running = true;
        current.phase = SyncPhase::Running;
        current.message = "Éclairage synchronisé".to_owned();
    }
}
