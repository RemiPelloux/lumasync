use super::*;

pub(super) async fn resolve_service_names(
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
