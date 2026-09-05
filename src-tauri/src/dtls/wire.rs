use super::*;

pub(super) fn parse_hello_verify_request(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 28 || data[13] != HT_HELLO_VERIFY_REQUEST {
        return Err("Le Hue Bridge n’a pas accepté l’ouverture du flux DTLS.".to_owned());
    }
    let cookie_length = data[27] as usize;
    let start = 28usize;
    let end = start.saturating_add(cookie_length);
    if end > data.len() {
        return Err("Le cookie DTLS reçu est incomplet.".to_owned());
    }
    Ok(data[start..end].to_vec())
}

pub(super) fn tls_random() -> Result<[u8; 32], String> {
    let mut random = [0u8; 32];
    let unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "L’horloge système est invalide.".to_owned())?
        .as_secs() as u32;
    random[..4].copy_from_slice(&unix_seconds.to_be_bytes());
    getrandom::fill(&mut random[4..])
        .map_err(|error| format!("Impossible de générer l’aléa DTLS : {error}"))?;
    Ok(random)
}

pub(super) fn prf(
    secret: &[u8],
    label: &[u8],
    seed: &[u8],
    length: usize,
) -> Result<Vec<u8>, String> {
    let mut label_seed = Vec::with_capacity(label.len() + seed.len());
    label_seed.extend_from_slice(label);
    label_seed.extend_from_slice(seed);
    let mut a = hmac_sha256(secret, &label_seed)?;
    let mut output = Vec::with_capacity(length);
    while output.len() < length {
        let mut block_input = Vec::with_capacity(a.len() + label_seed.len());
        block_input.extend_from_slice(&a);
        block_input.extend_from_slice(&label_seed);
        output.extend_from_slice(&hmac_sha256(secret, &block_input)?);
        a = hmac_sha256(secret, &a)?;
    }
    output.truncate(length);
    Ok(output)
}

pub(super) fn hmac_sha256(secret: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret)
        .map_err(|_| "La clé HMAC du flux Hue est invalide.".to_owned())?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

pub(super) fn push_handshake_header(
    target: &mut Vec<u8>,
    message_type: u8,
    length: usize,
    sequence: u16,
) {
    target.push(message_type);
    push_u24(target, length);
    push_u16(target, sequence);
    push_u24(target, 0);
    push_u24(target, length);
}

pub(super) fn push_u16(target: &mut Vec<u8>, value: u16) {
    target.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn push_u24(target: &mut Vec<u8>, value: usize) {
    target.extend_from_slice(&[
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    ]);
}

pub(super) fn push_sequence_48(target: &mut Vec<u8>, value: u64) {
    let bytes = value.to_be_bytes();
    target.extend_from_slice(&bytes[2..]);
}

pub(super) fn read_u16(data: &[u8], offset: usize) -> Result<u16, String> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .ok_or_else(|| "La réponse DTLS est tronquée.".to_owned())?
        .try_into()
        .map_err(|_| "La réponse DTLS est invalide.".to_owned())?;
    Ok(u16::from_be_bytes(bytes))
}

pub(super) fn read_u24(data: &[u8], offset: usize) -> Result<usize, String> {
    let bytes = data
        .get(offset..offset + 3)
        .ok_or_else(|| "La réponse DTLS est tronquée.".to_owned())?;
    Ok(((bytes[0] as usize) << 16) | ((bytes[1] as usize) << 8) | bytes[2] as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_prf_is_deterministic_and_sized() {
        let first = prf(b"secret", b"label", b"seed", 48).unwrap();
        let second = prf(b"secret", b"label", b"seed", 48).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 48);
        assert_ne!(first, vec![0; 48]);
    }

    #[test]
    fn sequence_writer_keeps_low_six_bytes() {
        let mut output = Vec::new();
        push_sequence_48(&mut output, 0x1122_3344_5566_7788);
        assert_eq!(output, vec![0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
    }
}
