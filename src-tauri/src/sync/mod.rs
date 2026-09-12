#[cfg(test)]
mod tests;
mod worker;

use crate::{
    capture, diagnostics, hue,
    models::{Credentials, StartSyncRequest, SyncPhase, SyncStatus},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::async_runtime::Mutex as AsyncMutex;
use worker::Session;

#[derive(Default)]
pub struct Controller {
    pub status: Arc<Mutex<SyncStatus>>,
    session: AsyncMutex<Option<Session>>,
    shutting_down: AtomicBool,
}

impl Controller {
    pub fn begin_shutdown(&self) -> bool {
        !self.shutting_down.swap(true, Ordering::AcqRel)
    }

    pub async fn start(
        &self,
        credentials: Credentials,
        request: StartSyncRequest,
    ) -> Result<(), String> {
        validate(&request)?;
        // Hold the asynchronous gate through activation and cleanup so requests cannot overlap.
        let mut session = self.session.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err("L'application est en cours de fermeture.".to_owned());
        }
        if session.as_ref().is_some_and(Session::is_running) {
            return Err("L'eclairage est deja en cours.".to_owned());
        }
        retire(&mut session).await?;
        publish(
            &self.status,
            SyncPhase::Starting,
            "Ouverture du flux Hue...",
        );
        diagnostics::info(
            "sync.start",
            &format!(
                "Starting capture: monitor={}, fps={}, lights={}",
                request.monitor_index,
                request.fps,
                request.assignments.len()
            ),
        );
        if let Err(error) = hue::start_entertainment(&credentials, &request.area_id).await {
            publish(&self.status, SyncPhase::Error, &error);
            diagnostics::error("sync.start", &error);
            return Err(error);
        }
        *session = Some(Session::new(credentials, request.area_id.clone()));
        let spawn = session
            .as_mut()
            .expect("session was just created")
            .spawn(request, Arc::clone(&self.status));
        if let Err(error) = spawn {
            let _ = retire(&mut session).await;
            publish(&self.status, SyncPhase::Error, &error);
            diagnostics::error("sync.spawn", &error);
            return Err(error);
        }
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let mut session = self.session.lock().await;
        if session.is_none() {
            publish(&self.status, SyncPhase::Idle, "Eclairage arrete");
            return Ok(());
        }
        publish(&self.status, SyncPhase::Stopping, "Arret du flux Hue...");
        let result = retire(&mut session).await;
        match &result {
            Ok(()) => {
                publish(&self.status, SyncPhase::Idle, "Eclairage arrete");
                diagnostics::info("sync.stop", "Capture and Hue stream stopped.");
            }
            Err(error) => {
                publish(&self.status, SyncPhase::Error, error);
                diagnostics::error("sync.stop", error);
            }
        }
        result
    }
}

async fn retire(session: &mut Option<Session>) -> Result<(), String> {
    let Some(active) = session.as_mut() else {
        return Ok(());
    };
    active.stop.store(true, Ordering::Release);
    let cleanup = match active.handle.take() {
        Some(handle) => tauri::async_runtime::spawn_blocking(move || handle.join())
            .await
            .map_err(|_| "L'arret de la capture a ete interrompu.".to_owned())
            .and_then(|result| {
                result.map_err(|_| "La capture s'est arretee de facon inattendue.".to_owned())
            })
            .and_then(|result| result),
        None => Err("La capture n'a pas demarre; nettoyage du pont requis.".to_owned()),
    };
    if let Err(error) = cleanup {
        diagnostics::warn("sync.cleanup.retry", &error);
        hue::stop_entertainment(&active.credentials, &active.area_id).await?;
    }
    *session = None;
    Ok(())
}

fn validate(request: &StartSyncRequest) -> Result<(), String> {
    capture::validate_assignments(&request.assignments)?;
    if uuid::Uuid::parse_str(&request.area_id).is_err() {
        return Err("La zone Entertainment selectionnee est invalide.".to_owned());
    }
    if !(10.0..=100.0).contains(&request.brightness)
        || !(40.0..=150.0).contains(&request.saturation)
        || !(10.0..=100.0).contains(&request.reactivity)
        || !(20.0..=100.0).contains(&request.max_luminosity)
        || !(5.0..=30.0).contains(&request.edge_depth)
        || !(10..=60).contains(&request.fps)
    {
        return Err("Un reglage de capture est hors limites.".to_owned());
    }
    Ok(())
}

fn publish(status: &Mutex<SyncStatus>, phase: SyncPhase, message: &str) {
    let mut current = status.lock().unwrap_or_else(|error| error.into_inner());
    *current = SyncStatus {
        phase,
        message: message.to_owned(),
        ..SyncStatus::default()
    };
}
