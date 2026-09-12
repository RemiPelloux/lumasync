use crate::{
    capture, diagnostics, hue,
    models::{Credentials, StartSyncRequest, SyncStatus},
};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread::JoinHandle,
};

pub(super) struct Session {
    pub stop: Arc<AtomicBool>,
    pub handle: Option<JoinHandle<Result<(), String>>>,
    pub credentials: Credentials,
    pub area_id: String,
}

impl Session {
    pub fn new(credentials: Credentials, area_id: String) -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
            credentials,
            area_id,
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    pub fn spawn(
        &mut self,
        request: StartSyncRequest,
        status: Arc<Mutex<SyncStatus>>,
    ) -> Result<(), String> {
        let credentials = self.credentials.clone();
        let area_id = self.area_id.clone();
        let stop = Arc::clone(&self.stop);
        self.handle = Some(
            std::thread::Builder::new()
                .name("lumasync-capture".to_owned())
                .spawn(move || {
                    let capture = catch_unwind(AssertUnwindSafe(|| {
                        capture::run_capture(
                            credentials.clone(),
                            request,
                            stop,
                            Arc::clone(&status),
                        )
                    }))
                    .unwrap_or_else(|_| {
                        Err("La capture s'est arretee de facon inattendue.".to_owned())
                    });
                    let cleanup = tauri::async_runtime::block_on(hue::stop_entertainment(
                        &credentials,
                        &area_id,
                    ));
                    finish_worker(&status, capture, &cleanup);
                    cleanup
                })
                .map_err(|error| format!("La capture n'a pas pu demarrer : {error}"))?,
        );
        Ok(())
    }
}

fn finish_worker(
    status: &Mutex<SyncStatus>,
    capture: Result<(), String>,
    cleanup: &Result<(), String>,
) {
    if let Err(error) = &capture {
        diagnostics::error("capture.failed", error);
    }
    if let Err(error) = cleanup {
        diagnostics::error("sync.cleanup", error);
    }
    if let Some(error) = capture.err().or_else(|| cleanup.as_ref().err().cloned()) {
        super::publish(status, crate::models::SyncPhase::Error, &error);
    }
    diagnostics::info("capture.stopped", "Capture worker stopped.");
}
