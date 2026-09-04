mod capture;
mod discovery;
mod dtls;
mod hue;
mod models;

use models::{
    BridgeInfo, Credentials, EntertainmentArea, HueRoom, MonitorInfo, StartSyncRequest, SyncPhase,
    SyncStatus,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use tauri::State;
use xcap::Monitor;

#[derive(Clone, Default)]
struct AppState {
    credentials: Arc<Mutex<Option<Credentials>>>,
    sync_status: Arc<Mutex<SyncStatus>>,
    active_sync: Arc<Mutex<Option<ActiveSync>>>,
}

struct ActiveSync {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
    credentials: Credentials,
    area_id: String,
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
    capture::validate_assignments(&request.assignments)?;
    if !(10.0..=100.0).contains(&request.brightness)
        || !(40.0..=150.0).contains(&request.saturation)
        || !(10.0..=100.0).contains(&request.reactivity)
        || !(20.0..=100.0).contains(&request.max_luminosity)
        || !(5.0..=30.0).contains(&request.edge_depth)
        || !(10..=60).contains(&request.fps)
    {
        return Err("Un réglage de capture est hors limites.".to_owned());
    }

    {
        let mut active = state
            .active_sync
            .lock()
            .map_err(|_| "L’état de synchronisation est indisponible.".to_owned())?;
        if let Some(current) = active.as_ref() {
            if !current.handle.is_finished() {
                return Err("L’éclairage est déjà en cours.".to_owned());
            }
        }
        *active = None;
    }

    let credentials = state
        .credentials
        .lock()
        .map_err(|_| "L’état du pont est indisponible.".to_owned())?
        .clone()
        .ok_or_else(|| "Associez d’abord le Hue Bridge.".to_owned())?;

    {
        let mut status = state
            .sync_status
            .lock()
            .map_err(|_| "L’état de synchronisation est indisponible.".to_owned())?;
        status.phase = SyncPhase::Starting;
        status.running = false;
        status.message = "Ouverture du flux Hue…".to_owned();
        status.measured_fps = 0.0;
        status.frame_time_ms = 0.0;
        status.dropped_frames = 0;
        status.black_bars_detected = false;
        status.colors.clear();
    }

    if let Err(error) = hue::start_entertainment(&credentials, &request.area_id).await {
        if let Ok(mut status) = state.sync_status.lock() {
            status.phase = SyncPhase::Error;
            status.message = error.clone();
        }
        return Err(error);
    }

    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = Arc::clone(&stop);
    let status = Arc::clone(&state.sync_status);
    let cleanup_credentials = credentials.clone();
    let area_id = request.area_id.clone();
    let cleanup_area = area_id.clone();
    let handle = std::thread::Builder::new()
        .name("lumasync-capture".to_owned())
        .spawn(move || {
            let result = capture::run_capture(
                cleanup_credentials.clone(),
                request,
                stop_for_thread.clone(),
                Arc::clone(&status),
            );
            if let Err(error) = result {
                if let Ok(mut current) = status.lock() {
                    current.running = false;
                    current.phase = SyncPhase::Error;
                    current.message = error;
                    current.measured_fps = 0.0;
                    current.frame_time_ms = 0.0;
                }
                let _ = tauri::async_runtime::block_on(hue::stop_entertainment(
                    &cleanup_credentials,
                    &cleanup_area,
                ));
            }
        })
        .map_err(|error| format!("La capture n’a pas pu démarrer : {error}"))?;

    *state
        .active_sync
        .lock()
        .map_err(|_| "L’état de synchronisation est indisponible.".to_owned())? =
        Some(ActiveSync {
            stop,
            handle,
            credentials,
            area_id,
        });
    Ok(())
}

#[tauri::command]
async fn stop_sync(state: State<'_, AppState>) -> Result<(), String> {
    let active = {
        let mut guard = state
            .active_sync
            .lock()
            .map_err(|_| "L’état de synchronisation est indisponible.".to_owned())?;
        guard.take()
    };
    let Some(active) = active else {
        return Ok(());
    };
    if let Ok(mut status) = state.sync_status.lock() {
        status.phase = SyncPhase::Stopping;
        status.message = "Arrêt du flux Hue…".to_owned();
    }
    active.stop.store(true, Ordering::Relaxed);
    let credentials = active.credentials.clone();
    let area_id = active.area_id.clone();
    tauri::async_runtime::spawn_blocking(move || active.handle.join())
        .await
        .map_err(|error| format!("L’arrêt de la capture s’est interrompu : {error}"))?
        .map_err(|_| "La capture s’est arrêtée de façon inattendue.".to_owned())?;
    let stop_result = hue::stop_entertainment(&credentials, &area_id).await;
    if let Ok(mut status) = state.sync_status.lock() {
        status.running = false;
        status.phase = if stop_result.is_ok() {
            SyncPhase::Idle
        } else {
            SyncPhase::Error
        };
        status.message = stop_result
            .as_ref()
            .map(|_| "Éclairage arrêté".to_owned())
            .unwrap_or_else(|error| error.clone());
        status.measured_fps = 0.0;
        status.frame_time_ms = 0.0;
        status.dropped_frames = 0;
        status.black_bars_detected = false;
        status.colors.clear();
    }
    stop_result
}

#[tauri::command]
fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    state
        .sync_status
        .lock()
        .map(|status| status.clone())
        .map_err(|_| "L’état de synchronisation est indisponible.".to_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
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
            get_sync_status
        ])
        .run(tauri::generate_context!())
        .expect("LumaSync could not start");
}
