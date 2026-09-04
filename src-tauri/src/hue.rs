use keyring::Entry;
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::IpAddr,
    time::{Duration, Instant},
};

use crate::models::{BridgeInfo, Credentials, EntertainmentArea, HueRoom, LightChannel};

const CREDENTIAL_SERVICE: &str = "LumaSync Hue Bridge";

fn validate_host(host: &str) -> Result<IpAddr, String> {
    host.parse::<IpAddr>()
        .map_err(|_| "L’adresse du Hue Bridge n’est pas une adresse IP valide.".to_owned())
}

fn url_host(host: &str) -> Result<String, String> {
    Ok(match validate_host(host)? {
        IpAddr::V4(address) => address.to_string(),
        IpAddr::V6(address) => format!("[{address}]"),
    })
}

fn credential_account(bridge: &BridgeInfo) -> String {
    if bridge.id.is_empty() {
        bridge.host.clone()
    } else {
        bridge.id.clone()
    }
}

pub fn save_credentials(bridge: &BridgeInfo, credentials: &Credentials) -> Result<(), String> {
    let entry = Entry::new(CREDENTIAL_SERVICE, &credential_account(bridge)).map_err(|error| {
        format!("Le gestionnaire d’identifiants Windows est indisponible : {error}")
    })?;
    let encoded = serde_json::to_string(credentials)
        .map_err(|error| format!("Les identifiants Hue sont invalides : {error}"))?;
    entry
        .set_password(&encoded)
        .map_err(|error| format!("Les identifiants Hue n’ont pas pu être protégés : {error}"))
}

pub fn load_credentials(bridge: &BridgeInfo) -> Result<Option<Credentials>, String> {
    let entry = Entry::new(CREDENTIAL_SERVICE, &credential_account(bridge)).map_err(|error| {
        format!("Le gestionnaire d’identifiants Windows est indisponible : {error}")
    })?;
    match entry.get_password() {
        Ok(encoded) => {
            let mut credentials: Credentials = serde_json::from_str(&encoded)
                .map_err(|_| "Les identifiants Hue enregistrés sont illisibles. Une nouvelle association est nécessaire.".to_owned())?;
            credentials.host = bridge.host.clone();
            Ok(Some(credentials))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!(
            "Les identifiants Hue n’ont pas pu être lus : {error}"
        )),
    }
}

fn client() -> Result<Client, String> {
    Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(7))
        .build()
        .map_err(|error| format!("Le client réseau Hue n’a pas pu démarrer : {error}"))
}

pub async fn pair(bridge: &BridgeInfo) -> Result<Credentials, String> {
    let client = client()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let url = format!("https://{}/api", url_host(&bridge.host)?);

    while Instant::now() < deadline {
        let response = client
            .post(&url)
            .json(&json!({
                "devicetype": "lumasync#windows",
                "generateclientkey": true
            }))
            .send()
            .await;

        let value: Value = match response {
            Ok(response) => response.json().await.map_err(|error| {
                format!("La réponse d’association du pont est illisible : {error}")
            })?,
            Err(error) => {
                if Instant::now() + Duration::from_secs(2) < deadline {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                return Err(format!("Le Hue Bridge est injoignable : {error}"));
            }
        };

        let first = value.as_array().and_then(|items| items.first());
        if let Some(success) = first.and_then(|entry| entry.get("success")) {
            let app_key = success
                .get("username")
                .and_then(Value::as_str)
                .ok_or_else(|| "Le pont n’a pas renvoyé de clé d’application.".to_owned())?;
            let client_key = success
                .get("clientkey")
                .and_then(Value::as_str)
                .ok_or_else(|| "Le pont n’a pas renvoyé de clé Entertainment.".to_owned())?;
            return Ok(Credentials {
                bridge_id: bridge.id.clone(),
                host: bridge.host.clone(),
                app_key: app_key.to_owned(),
                client_key: client_key.to_owned(),
            });
        }

        if first
            .and_then(|entry| entry.get("error"))
            .and_then(|error| error.get("type"))
            .and_then(Value::as_u64)
            == Some(101)
        {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }

        let description = first
            .and_then(|entry| entry.get("error"))
            .and_then(|error| error.get("description"))
            .and_then(Value::as_str)
            .unwrap_or("réponse inattendue du pont");
        return Err(format!(
            "Le Hue Bridge a refusé l’association : {description}"
        ));
    }

    Err(
        "Le délai d’association est écoulé. Appuyez de nouveau sur le bouton central du pont."
            .to_owned(),
    )
}

async fn request(
    credentials: &Credentials,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let client = client()?;
    let url = format!("https://{}{}", url_host(&credentials.host)?, path);
    let mut request = client
        .request(method, url)
        .header("hue-application-key", &credentials.app_key);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("Le Hue Bridge est injoignable : {error}"))?;
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|error| format!("La réponse du Hue Bridge est illisible : {error}"))?;
    if !status.is_success() {
        return Err(format!("Le Hue Bridge a répondu avec l’état {status}."));
    }
    if let Some(errors) = value.get("errors").and_then(Value::as_array) {
        if !errors.is_empty() {
            return Err("Le Hue Bridge a refusé la commande Entertainment.".to_owned());
        }
    }
    Ok(value)
}

async fn request_v1(
    credentials: &Credentials,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let client = client()?;
    let url = format!(
        "https://{}/api/{}{}",
        url_host(&credentials.host)?,
        credentials.app_key,
        path
    );
    let mut request = client.request(method, url);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("Le Hue Bridge est injoignable : {error}"))?;
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|error| format!("La réponse du Hue Bridge est illisible : {error}"))?;
    if !status.is_success() {
        return Err(format!("Le Hue Bridge a répondu avec l’état {status}."));
    }
    if let Some(description) = value.as_array().into_iter().flatten().find_map(|item| {
        item.get("error")
            .and_then(|error| error.get("description"))
            .and_then(Value::as_str)
    }) {
        return Err(format!(
            "Le Hue Bridge a refusé la création : {description}"
        ));
    }
    Ok(value)
}

pub async fn verify(credentials: &Credentials) -> Result<(), String> {
    request(credentials, Method::GET, "/clip/v2/resource/bridge", None)
        .await
        .map(|_| ())
}

pub async fn entertainment_areas(
    credentials: &Credentials,
) -> Result<Vec<EntertainmentArea>, String> {
    // These resources are independent. Fetch them concurrently so bridge
    // round-trips don't accumulate before the setup screen can be displayed.
    let (configurations, names) = tokio::try_join!(
        request(
            credentials,
            Method::GET,
            "/clip/v2/resource/entertainment_configuration",
            None,
        ),
        resolve_service_names(credentials),
    )?;
    let mut areas = Vec::new();

    for configuration in data(&configurations) {
        let id = configuration
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let name = configuration
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or("Zone Entertainment");
        let mut channels = Vec::new();
        if let Some(items) = configuration.get("channels").and_then(Value::as_array) {
            for channel in items {
                let channel_id = channel
                    .get("channel_id")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u8;
                let service_id = channel
                    .pointer("/members/0/service/rid")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let position = (
                    channel
                        .pointer("/position/x")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0) as f32,
                    channel
                        .pointer("/position/y")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0) as f32,
                    channel
                        .pointer("/position/z")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0) as f32,
                );
                channels.push(LightChannel {
                    channel_id,
                    name: names
                        .get(&service_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Lumière {}", channel_id + 1)),
                    service_id,
                    position,
                });
            }
        }
        areas.push(EntertainmentArea {
            id: id.to_owned(),
            name: name.to_owned(),
            channels,
        });
    }
    Ok(areas)
}

pub async fn rooms(credentials: &Credentials) -> Result<Vec<HueRoom>, String> {
    let groups = request_v1(credentials, Method::GET, "/groups", None).await?;
    let mut rooms = groups
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, group)| {
            let kind = group.get("type").and_then(Value::as_str)?;
            if kind != "Room" {
                return None;
            }
            let name = group.get("name").and_then(Value::as_str)?;
            let light_count = group
                .get("lights")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            (light_count > 0).then(|| HueRoom {
                id: id.clone(),
                name: name.to_owned(),
                light_count,
            })
        })
        .collect::<Vec<_>>();
    rooms.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(rooms)
}

pub async fn create_entertainment_from_room(
    credentials: &Credentials,
    room_id: &str,
) -> Result<EntertainmentArea, String> {
    if room_id.is_empty() || !room_id.chars().all(|character| character.is_ascii_digit()) {
        return Err("La pièce Hue sélectionnée est invalide.".to_owned());
    }

    let groups = request_v1(credentials, Method::GET, "/groups", None).await?;
    let room = groups
        .get(room_id)
        .filter(|group| group.get("type").and_then(Value::as_str) == Some("Room"))
        .ok_or_else(|| "La pièce Hue sélectionnée n’existe plus.".to_owned())?;
    let room_name = room.get("name").and_then(Value::as_str).unwrap_or("Pièce");
    let lights = room
        .get("lights")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if lights.is_empty() {
        return Err("Cette pièce ne contient aucune lampe Hue.".to_owned());
    }
    if lights.len() > 10 {
        return Err("Une zone Entertainment accepte au maximum 10 lampes.".to_owned());
    }

    let name = entertainment_name(room_name);
    let mut expected_lights = lights.clone();
    expected_lights.sort();
    let mut existing_group_id = groups.as_object().and_then(|all_groups| {
        all_groups.iter().find_map(|(id, group)| {
            let same_type = group.get("type").and_then(Value::as_str) == Some("Entertainment");
            let same_name = group.get("name").and_then(Value::as_str) == Some(name.as_str());
            let mut group_lights = group
                .get("lights")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            group_lights.sort();
            (same_type && same_name && group_lights == expected_lights).then(|| id.clone())
        })
    });

    if existing_group_id.is_none() {
        let locations = lights
            .iter()
            .zip(positions_for_lights(lights.len()))
            .map(|(light_id, position)| (light_id.clone(), json!(position)))
            .collect::<serde_json::Map<String, Value>>();
        let response = request_v1(
            credentials,
            Method::POST,
            "/groups",
            Some(json!({
                "name": name,
                "type": "Entertainment",
                "class": "TV",
                "lights": lights,
                "locations": locations
            })),
        )
        .await?;
        existing_group_id = response
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.pointer("/success/id"))
            .and_then(Value::as_str)
            .map(|id| id.trim_start_matches("/groups/").to_owned());
    }

    let group_id = existing_group_id.ok_or_else(|| {
        "Le pont n’a pas confirmé la création de la zone Entertainment.".to_owned()
    })?;
    let id_v1 = format!("/groups/{group_id}");
    for _ in 0..8 {
        let areas = entertainment_areas(credentials).await?;
        let configurations = request(
            credentials,
            Method::GET,
            "/clip/v2/resource/entertainment_configuration",
            None,
        )
        .await?;
        if let Some(configuration_id) = data(&configurations).iter().find_map(|configuration| {
            (configuration.get("id_v1").and_then(Value::as_str) == Some(id_v1.as_str()))
                .then(|| configuration.get("id").and_then(Value::as_str))
                .flatten()
        }) {
            if let Some(area) = areas.into_iter().find(|area| area.id == configuration_id) {
                return Ok(area);
            }
        }
        tokio::time::sleep(Duration::from_millis(350)).await;
    }

    Err("La zone a été créée, mais le pont ne l’a pas encore publiée. Rechargez dans quelques secondes.".to_owned())
}

fn entertainment_name(room_name: &str) -> String {
    let suffix = " Ambilight";
    let max_room_chars = 32usize.saturating_sub(suffix.chars().count());
    let room = room_name.chars().take(max_room_chars).collect::<String>();
    format!("{room}{suffix}")
}

fn positions_for_lights(count: usize) -> impl Iterator<Item = [f32; 3]> {
    // First four slots are screen corners (top-left, top-right, bottom-left, bottom-right).
    const POSITIONS: [[f32; 3]; 10] = [
        [-1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
        [0.0, 1.0, -1.0],
        [-0.5, 1.0, 1.0],
        [0.5, 1.0, 1.0],
    ];
    POSITIONS.into_iter().take(count)
}

async fn resolve_service_names(
    credentials: &Credentials,
) -> Result<HashMap<String, String>, String> {
    let (devices, lights, entertainments) = tokio::try_join!(
        request(credentials, Method::GET, "/clip/v2/resource/device", None),
        request(credentials, Method::GET, "/clip/v2/resource/light", None),
        request(
            credentials,
            Method::GET,
            "/clip/v2/resource/entertainment",
            None,
        ),
    )?;
    let mut device_names = HashMap::new();
    let mut names = HashMap::new();

    for device in data(&devices) {
        if let (Some(id), Some(name)) = (
            device.get("id").and_then(Value::as_str),
            device.pointer("/metadata/name").and_then(Value::as_str),
        ) {
            device_names.insert(id.to_owned(), name.to_owned());
        }
    }
    for light in data(&lights) {
        if let Some(id) = light.get("id").and_then(Value::as_str) {
            let direct = light.pointer("/metadata/name").and_then(Value::as_str);
            let owner = light.pointer("/owner/rid").and_then(Value::as_str);
            if let Some(name) = direct
                .or_else(|| owner.and_then(|value| device_names.get(value).map(String::as_str)))
            {
                names.insert(id.to_owned(), name.to_owned());
            }
        }
    }
    for entertainment in data(&entertainments) {
        if let (Some(id), Some(owner)) = (
            entertainment.get("id").and_then(Value::as_str),
            entertainment.pointer("/owner/rid").and_then(Value::as_str),
        ) {
            if let Some(name) = device_names.get(owner) {
                names.insert(id.to_owned(), name.clone());
            }
        }
    }
    Ok(names)
}

pub async fn start_entertainment(credentials: &Credentials, area_id: &str) -> Result<(), String> {
    request(
        credentials,
        Method::PUT,
        &format!("/clip/v2/resource/entertainment_configuration/{area_id}"),
        Some(json!({ "action": "start" })),
    )
    .await
    .map(|_| ())
}

pub async fn stop_entertainment(credentials: &Credentials, area_id: &str) -> Result<(), String> {
    request(
        credentials,
        Method::PUT,
        &format!("/clip/v2/resource/entertainment_configuration/{area_id}"),
        Some(json!({ "action": "stop" })),
    )
    .await
    .map(|_| ())
}

fn data(value: &Value) -> &[Value] {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::{entertainment_name, positions_for_lights};

    #[test]
    fn entertainment_name_stays_within_hue_limit() {
        let name = entertainment_name("Une chambre avec un nom beaucoup trop long");
        assert!(name.chars().count() <= 32);
        assert!(name.ends_with(" Ambilight"));
    }

    #[test]
    fn four_light_positions_cover_screen_corners() {
        let positions = positions_for_lights(4).collect::<Vec<_>>();
        assert_eq!(positions.len(), 4);
        assert_eq!(positions[0], [-1.0, 1.0, 1.0]);
        assert_eq!(positions[1], [1.0, 1.0, 1.0]);
        assert_eq!(positions[2], [-1.0, 1.0, -1.0]);
        assert_eq!(positions[3], [1.0, 1.0, -1.0]);
    }
}
