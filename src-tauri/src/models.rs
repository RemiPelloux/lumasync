use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeInfo {
    pub id: String,
    pub host: String,
    pub name: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub bridge_id: String,
    pub host: String,
    pub app_key: String,
    pub client_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightChannel {
    pub channel_id: u8,
    pub service_id: String,
    pub name: String,
    pub position: (f32, f32, f32),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntertainmentArea {
    pub id: String,
    pub name: String,
    pub channels: Vec<LightChannel>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HueRoom {
    pub id: String,
    pub name: String,
    pub light_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Zone {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAssignment {
    pub channel_id: u8,
    pub zone: Zone,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSyncRequest {
    pub area_id: String,
    pub monitor_index: usize,
    pub assignments: Vec<ChannelAssignment>,
    pub brightness: f32,
    pub saturation: f32,
    pub reactivity: f32,
    pub max_luminosity: f32,
    pub edge_depth: f32,
    pub fps: u32,
    pub black_bar_detection: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncPhase {
    Idle,
    Starting,
    Running,
    Reconnecting,
    Stopping,
    Error,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub running: bool,
    pub phase: SyncPhase,
    pub message: String,
    pub measured_fps: f32,
    pub frame_time_ms: f32,
    pub dropped_frames: u64,
    pub black_bars_detected: bool,
    pub colors: HashMap<String, String>,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            running: false,
            phase: SyncPhase::Idle,
            message: "Prêt à configurer".to_owned(),
            measured_fps: 0.0,
            frame_time_ms: 0.0,
            dropped_frames: 0,
            black_bars_detected: false,
            colors: HashMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}
