mod creation;
mod names;
mod resources;
pub use creation::create_entertainment_from_room;
use names::resolve_service_names;
pub use resources::{entertainment_areas, rooms};

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
    static CLIENT: std::sync::OnceLock<Result<Client, String>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .danger_accept_invalid_certs(true)
                .connect_timeout(Duration::from_secs(4))
                .timeout(Duration::from_secs(7))
                .build()
                .map_err(|error| format!("Le client réseau Hue n’a pas pu démarrer : {error}"))
        })
        .clone()
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
