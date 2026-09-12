use super::*;

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
    // Publication is eventually consistent. Poll the small configuration resource,
    // then resolve names and channels once after the id is visible.
    let mut configuration_id = None;
    for attempt in 0..8 {
        let configurations = request(
            credentials,
            Method::GET,
            "/clip/v2/resource/entertainment_configuration",
            None,
        )
        .await?;
        configuration_id = data(&configurations).iter().find_map(|configuration| {
            (configuration.get("id_v1").and_then(Value::as_str) == Some(id_v1.as_str()))
                .then(|| configuration.get("id").and_then(Value::as_str))
                .flatten()
                .map(str::to_owned)
        });
        if configuration_id.is_some() {
            break;
        }
        if attempt < 7 {
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    }
    if let Some(configuration_id) = configuration_id {
        let areas = entertainment_areas(credentials).await?;
        if let Some(area) = areas.into_iter().find(|area| area.id == configuration_id) {
            return Ok(area);
        }
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
    // Hue's screen plane is x/y; z is depth. First four slots are screen corners.
    const POSITIONS: [[f32; 3]; 10] = [
        [-1.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [-0.5, 1.0, 0.0],
        [0.5, 1.0, 0.0],
    ];
    POSITIONS.into_iter().take(count)
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
        assert_eq!(positions[0], [-1.0, 1.0, 0.0]);
        assert_eq!(positions[1], [1.0, 1.0, 0.0]);
        assert_eq!(positions[2], [-1.0, -1.0, 0.0]);
        assert_eq!(positions[3], [1.0, -1.0, 0.0]);
    }
}
