use super::*;

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
    rooms.sort_by_key(|room| room.name.to_lowercase());
    Ok(rooms)
}
