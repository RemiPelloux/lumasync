const MAX_MESSAGE_CHARS: usize = 1500;
const SECRET_FIELDS: [&str; 11] = [
    "app_key",
    "appkey",
    "client_key",
    "clientkey",
    "hue-application-key",
    "access_token",
    "authorization",
    "password",
    "username",
    "api_key",
    "token",
];

pub(super) fn sanitize(message: &str) -> String {
    let mut clean = message.to_owned();
    for field in SECRET_FIELDS {
        clean = redact_field(&clean, field);
    }
    let clean: String = clean
        .split_inclusive(char::is_whitespace)
        .map(redact_token)
        .collect();
    clean.chars().take(MAX_MESSAGE_CHARS).collect()
}

fn redact_field(message: &str, field: &str) -> String {
    let lower = message.to_ascii_lowercase();
    let mut result = String::new();
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find(field) {
        let end = cursor + offset + field.len();
        result.push_str(&message[cursor..end]);
        let suffix = &message[end..];
        let start =
            suffix.find(|c: char| !c.is_whitespace() && !matches!(c, ':' | '=' | '\"' | '\''));
        let Some(start) = start else {
            cursor = end;
            continue;
        };
        if !suffix[..start].contains([':', '=']) {
            cursor = end;
            continue;
        }
        result.push_str(&suffix[..start]);
        let value = &suffix[start..];
        let length = value
            .find(|c: char| {
                (c.is_whitespace() && field != "authorization")
                    || matches!(c, '\"' | '\'' | ',' | '}' | '&')
            })
            .unwrap_or(value.len());
        result.push_str("[redacted]");
        cursor = end + start + length;
    }
    result.push_str(&message[cursor..]);
    result
}

fn redact_token(token: &str) -> String {
    let whitespace = &token[token.trim_end().len()..];
    if token.contains("://") {
        return format!("[url]{whitespace}");
    }
    let mut result = String::new();
    let mut secret = String::new();
    for character in token.chars().chain(std::iter::once('\0')) {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
            secret.push(character);
        } else {
            result.push_str(if secret.len() >= 24 {
                "[redacted]"
            } else {
                &secret
            });
            secret.clear();
            if character != '\0' {
                result.push(character);
            }
        }
    }
    result
}
