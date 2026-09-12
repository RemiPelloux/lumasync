use super::*;
use crate::models::{ChannelAssignment, Zone};
use std::{sync::atomic::AtomicUsize, time::Duration};

fn credentials() -> Credentials {
    Credentials {
        bridge_id: String::new(),
        host: String::new(),
        app_key: String::new(),
        client_key: String::new(),
    }
}

fn request() -> StartSyncRequest {
    StartSyncRequest {
        area_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
        monitor_index: 0,
        assignments: vec![ChannelAssignment {
            channel_id: 1,
            zone: Zone::Left,
        }],
        brightness: 100.0,
        saturation: 100.0,
        reactivity: 80.0,
        max_luminosity: 100.0,
        edge_depth: 15.0,
        fps: 60,
        black_bar_detection: true,
    }
}

#[test]
fn validation_rejects_nan_and_invalid_area_before_activation() {
    let mut request = request();
    request.brightness = f32::NAN;
    assert!(validate(&request).is_err());
    request.brightness = 100.0;
    request.area_id = "../../lights".to_owned();
    assert!(validate(&request).is_err());
}

#[tokio::test]
async fn concurrent_stops_join_the_worker_once_and_reset_status() {
    let controller = Arc::new(Controller::default());
    let stopped = Arc::new(AtomicUsize::new(0));
    let mut session = Session::new(credentials(), request().area_id);
    let (stop, count) = (Arc::clone(&session.stop), Arc::clone(&stopped));
    session.handle = Some(std::thread::spawn(move || {
        while !stop.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        std::thread::sleep(Duration::from_millis(20));
        count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }));
    *controller.session.lock().await = Some(session);
    let (first, second) = tokio::join!(controller.stop(), controller.stop());
    assert!(first.is_ok() && second.is_ok());
    assert_eq!(stopped.load(Ordering::Relaxed), 1);
    assert!(controller.session.lock().await.is_none());
    let status = controller.status.lock().unwrap();
    assert!(!status.running && matches!(status.phase, SyncPhase::Idle));
}

#[tokio::test]
async fn existing_worker_rejects_a_second_start_without_network_access() {
    let controller = Controller::default();
    let mut session = Session::new(credentials(), request().area_id);
    let stop = Arc::clone(&session.stop);
    session.handle = Some(std::thread::spawn(move || {
        while !stop.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        Ok(())
    }));
    *controller.session.lock().await = Some(session);
    assert!(controller
        .start(credentials(), request())
        .await
        .unwrap_err()
        .contains("deja"));
    controller.stop().await.unwrap();
}

#[tokio::test]
async fn stopping_after_failed_start_acknowledges_error_without_a_worker() {
    let controller = Controller::default();
    publish(&controller.status, SyncPhase::Error, "Activation failed");
    controller.stop().await.unwrap();
    let status = controller.status.lock().unwrap();
    assert!(!status.running && matches!(status.phase, SyncPhase::Idle));
}

#[tokio::test]
async fn shutdown_prevents_all_later_starts() {
    let controller = Controller::default();
    assert!(controller.begin_shutdown());
    assert!(!controller.begin_shutdown());
    assert!(controller
        .start(credentials(), request())
        .await
        .unwrap_err()
        .contains("fermeture"));
}
