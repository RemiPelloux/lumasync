mod capture;
mod diagnostics;
mod discovery;
mod dtls;
mod hue;
mod models;
mod sync;

use models::{
    BridgeInfo, Credentials, EntertainmentArea, HueRoom, MonitorInfo, StartSyncRequest, SyncStatus,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{Manager, State};
use xcap::Monitor;

#[derive(Clone, Default)]
struct AppState {
    credentials: Arc<Mutex<Option<Credentials>>>,
    sync: Arc<sync::Controller>,
    shutdown_complete: Arc<AtomicBool>,
}

#[tauri::command]
async fn discover_bridges() -> Result<Vec<BridgeInfo>, String> {
    tauri::async_runtime::spawn_blocking(discovery::discover)
        .await
        .map_err(|error| format!("La tâche de détection s’est interrompue : {error}"))?
}

#[tauri::command]
async fn restore_bridge(bridge: BridgeInfo, state: State<'_, AppState>) -> Result<bool, String> {
    let restored = tauri::async_runtime::spawn_blocking({
        let bridge = bridge.clone();
        move || hue::load_credentials(&bridge)
    })
    .await
    .map_err(|error| format!("La lecture des identifiants s’est interrompue : {error}"))??;
    let Some(credentials) = restored else {
        return Ok(false);
    };
    if hue::verify(&credentials).await.is_err() {
        return Ok(false);
    }
    *state
        .credentials
        .lock()
        .map_err(|_| "L’état du pont est indisponible.".to_owned())? = Some(credentials);
    Ok(true)
}

#[tauri::command]
async fn pair_bridge(bridge: BridgeInfo, state: State<'_, AppState>) -> Result<(), String> {
    let credentials = hue::pair(&bridge).await?;
    tauri::async_runtime::spawn_blocking({
        let bridge = bridge.clone();
        let credentials = credentials.clone();
        move || hue::save_credentials(&bridge, &credentials)
    })
    .await
    .map_err(|error| format!("L’enregistrement des identifiants s’est interrompu : {error}"))??;
    *state
        .credentials
        .lock()
        .map_err(|_| "L’état du pont est indisponible.".to_owned())? = Some(credentials);
    Ok(())
}

#[tauri::command]
async fn get_entertainment_areas(
    state: State<'_, AppState>,
) -> Result<Vec<EntertainmentArea>, String> {
    let credentials = state
        .credentials
        .lock()
        .map_err(|_| "L’état du pont est indisponible.".to_owned())?
        .clone()
        .ok_or_else(|| "Associez d’abord le Hue Bridge.".to_owned())?;
    hue::entertainment_areas(&credentials).await
}

fn current_credentials(state: &State<'_, AppState>) -> Result<Credentials, String> {
    state
        .credentials
        .lock()
        .map_err(|_| "L’état du pont est indisponible.".to_owned())?
        .clone()
        .ok_or_else(|| "Associez d’abord le Hue Bridge.".to_owned())
}

#[tauri::command]
async fn get_hue_rooms(state: State<'_, AppState>) -> Result<Vec<HueRoom>, String> {
    let credentials = current_credentials(&state)?;
    hue::rooms(&credentials).await
}

#[tauri::command]
async fn create_entertainment_from_room(
    room_id: String,
    state: State<'_, AppState>,
) -> Result<EntertainmentArea, String> {
    let credentials = current_credentials(&state)?;
    hue::create_entertainment_from_room(&credentials, &room_id).await
}

#[tauri::command]
async fn get_monitors() -> Result<Vec<MonitorInfo>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let monitors = Monitor::all()
            .map_err(|error| format!("Les écrans ne peuvent pas être listés : {error}"))?;
        monitors
            .into_iter()
            .enumerate()
            .map(|(index, monitor)| {
                Ok(MonitorInfo {
                    index,
                    name: monitor
                        .friendly_name()
                        .or_else(|_| monitor.name())
                        .unwrap_or_else(|_| format!("Écran {}", index + 1)),
                    width: monitor.width().map_err(|error| {
                        format!("La largeur de l’écran est indisponible : {error}")
                    })?,
                    height: monitor.height().map_err(|error| {
                        format!("La hauteur de l’écran est indisponible : {error}")
                    })?,
                    primary: monitor.is_primary().unwrap_or(index == 0),
                })
            })
            .collect::<Result<Vec<_>, String>>()
    })
    .await
    .map_err(|error| format!("La détection des écrans s’est interrompue : {error}"))?
}

#[tauri::command]
async fn start_sync(request: StartSyncRequest, state: State<'_, AppState>) -> Result<(), String> {
    state
        .sync
        .start(current_credentials(&state)?, request)
        .await
}

#[tauri::command]
async fn stop_sync(state: State<'_, AppState>) -> Result<(), String> {
    state.sync.stop().await
}

#[tauri::command]
fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    state
        .sync
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|_| "L’état de synchronisation est indisponible.".to_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            diagnostics::initialize(app.path().app_log_dir().map_err(|error| error.to_string()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            discover_bridges,
            restore_bridge,
            pair_bridge,
            get_entertainment_areas,
            get_hue_rooms,
            create_entertainment_from_room,
            get_monitors,
            start_sync,
            stop_sync,
            get_sync_status,
            diagnostics::get_diagnostics,
            diagnostics::clear_diagnostics,
            diagnostics::log_frontend_event
        ])
        .build(tauri::generate_context!())
        .expect("LumaSync could not start")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                if state.shutdown_complete.load(Ordering::Acquire) {
                    return;
                }
                api.prevent_exit();
                if state.sync.begin_shutdown() {
                    let (app, state) = (app.clone(), state.inner().clone());
                    tauri::async_runtime::spawn(async move {
                        let _ = state.sync.stop().await;
                        diagnostics::info("application.exit", "Application shutdown completed.");
                        let _ = tauri::async_runtime::spawn_blocking(diagnostics::flush).await;
                        state.shutdown_complete.store(true, Ordering::Release);
                        app.exit(0);
                    });
                }
            }
        });
}
