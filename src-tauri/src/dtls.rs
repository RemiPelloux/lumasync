//! Minimal DTLS 1.2 PSK sender for the Philips Hue Entertainment protocol.
//!
//! It intentionally supports only TLS_PSK_WITH_AES_128_GCM_SHA256, the cipher
//! required by the Hue Bridge. The protocol framing was cross-checked against
//! the Philips Hue Entertainment documentation and music-assistant's
//! Apache-2.0 `hue-entertainment` implementation.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes128Gcm, Nonce,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::{
    net::UdpSocket,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::models::Rgb;

const HUE_PORT: u16 = 2100;
const DTLS_VERSION: u16 = 0xFEFD;
const CT_CHANGE_CIPHER_SPEC: u8 = 0x14;
const CT_HANDSHAKE: u8 = 0x16;
const CT_APPLICATION_DATA: u8 = 0x17;
const HT_CLIENT_HELLO: u8 = 0x01;
const HT_SERVER_HELLO: u8 = 0x02;
const HT_HELLO_VERIFY_REQUEST: u8 = 0x03;
const HT_SERVER_HELLO_DONE: u8 = 0x0E;
const HT_CLIENT_KEY_EXCHANGE: u8 = 0x10;
const HT_FINISHED: u8 = 0x14;
const CIPHER_SUITE: [u8; 2] = [0x00, 0xA8];

pub struct HueStream {
    connection: DtlsConnection,
    area_id: [u8; 36],
    sequence: u8,
}

impl HueStream {
    pub fn connect(
        host: &str,
        identity: &str,
        client_key_hex: &str,
        area_id: &str,
    ) -> Result<Self, String> {
        let parsed_area = uuid::Uuid::parse_str(area_id)
            .map_err(|_| "L’identifiant de la zone Entertainment est invalide.".to_owned())?;
        let area_text = parsed_area.hyphenated().to_string();
        let area_id: [u8; 36] = area_text
            .as_bytes()
            .try_into()
            .map_err(|_| "L’identifiant de la zone Entertainment est incomplet.".to_owned())?;
        let psk = hex::decode(client_key_hex)
            .map_err(|_| "La clé Entertainment enregistrée est invalide.".to_owned())?;
        let mut connection = DtlsConnection::new(host, HUE_PORT, identity.as_bytes(), &psk)?;
        connection.handshake()?;
        Ok(Self {
            connection,
            area_id,
            sequence: 0,
        })
    }

    pub fn send(&mut self, colors: &[(u8, Rgb)]) -> Result<(), String> {
        if colors.is_empty() {
            return Ok(());
        }
        let mut frame = Vec::with_capacity(52 + colors.len() * 7);
        frame.extend_from_slice(b"HueStream");
        frame.extend_from_slice(&[0x02, 0x00]);
        frame.push(self.sequence);
        frame.extend_from_slice(&[0x00, 0x00]);
        frame.push(0x00); // RGB color space
        frame.push(0x00);
        frame.extend_from_slice(&self.area_id);
        for (channel, color) in colors {
            frame.extend_from_slice(&[
                *channel, color.r, color.r, color.g, color.g, color.b, color.b,
            ]);
        }
        self.sequence = self.sequence.wrapping_add(1);
        self.connection.send_encrypted(CT_APPLICATION_DATA, &frame)
    }
}

struct DtlsConnection {
    socket: UdpSocket,
    identity: Vec<u8>,
    psk: Vec<u8>,
    client_random: [u8; 32],
    server_random: [u8; 32],
    master_secret: Vec<u8>,
    epoch: u16,
    send_sequence: u64,
    client_write_iv: [u8; 4],
    cipher: Option<Aes128Gcm>,
    handshake_messages: Vec<u8>,
    message_sequence: u16,
}

impl DtlsConnection {
    fn new(host: &str, port: u16, identity: &[u8], psk: &[u8]) -> Result<Self, String> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|error| format!("Impossible d’ouvrir le flux Hue : {error}"))?;
        socket.connect((host, port)).map_err(|error| {
            format!("Le Hue Bridge ne répond pas sur le port Entertainment : {error}")
        })?;
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| format!("Impossible de régler le délai réseau : {error}"))?;

        Ok(Self {
            socket,
            identity: identity.to_vec(),
            psk: psk.to_vec(),
            client_random: [0; 32],
            server_random: [0; 32],
            master_secret: Vec::new(),
            epoch: 0,
            send_sequence: 0,
            client_write_iv: [0; 4],
            cipher: None,
            handshake_messages: Vec::new(),
            message_sequence: 0,
        })
    }

    fn handshake(&mut self) -> Result<(), String> {
        self.client_random = tls_random()?;
        let hello = self.build_client_hello(&[])?;
        self.send_handshake(HT_CLIENT_HELLO, &hello)?;

        let verify = self.receive()?;
        let cookie = parse_hello_verify_request(&verify)?;
        self.send_cookie_client_hello(&cookie)?;

        let mut server_flight = None;
        for attempt in 0..3 {
            match self.receive() {
                Ok(data) => {
                    server_flight = Some(data);
                    break;
                }
                Err(error) if attempt < 2 => {
                    let _ = error;
                    self.send_cookie_client_hello(&cookie)?;
                }
                Err(error) => return Err(error),
            }
        }
        self.parse_server_flight(server_flight.as_deref().unwrap_or_default())?;
        self.derive_keys()?;

        let mut key_exchange = Vec::with_capacity(2 + self.identity.len());
        push_u16(&mut key_exchange, self.identity.len() as u16);
        key_exchange.extend_from_slice(&self.identity);
        self.send_handshake(HT_CLIENT_KEY_EXCHANGE, &key_exchange)?;
        self.send_record(CT_CHANGE_CIPHER_SPEC, &[0x01])?;
        self.epoch = 1;
        self.send_sequence = 0;

        let transcript_hash = Sha256::digest(&self.handshake_messages);
        let finished = prf(
            &self.master_secret,
            b"client finished",
            &transcript_hash,
            12,
        )?;
        let mut finished_message = Vec::with_capacity(24);
        push_handshake_header(
            &mut finished_message,
            HT_FINISHED,
            finished.len(),
            self.message_sequence,
        );
        finished_message.extend_from_slice(&finished);
        self.message_sequence = self.message_sequence.wrapping_add(1);
        self.send_encrypted(CT_HANDSHAKE, &finished_message)?;

        // The Hue Entertainment protocol accepts application frames as soon as
        // our encrypted Finished flight is sent. Waiting for optional firmware
        // acknowledgements here used to add up to 1.6 s to every start/reconnect.
        Ok(())
    }

    fn send_cookie_client_hello(&mut self, cookie: &[u8]) -> Result<(), String> {
        self.message_sequence = 0;
        self.send_sequence = 0;
        self.handshake_messages.clear();
        let hello = self.build_client_hello(cookie)?;
        self.send_handshake(HT_CLIENT_HELLO, &hello)
    }

    fn build_client_hello(&self, cookie: &[u8]) -> Result<Vec<u8>, String> {
        if cookie.len() > u8::MAX as usize {
            return Err("Le cookie DTLS du pont est trop long.".to_owned());
        }
        let mut body = Vec::with_capacity(40 + cookie.len());
        push_u16(&mut body, DTLS_VERSION);
        body.extend_from_slice(&self.client_random);
        body.push(0); // empty session ID
        body.push(cookie.len() as u8);
        body.extend_from_slice(cookie);
        push_u16(&mut body, 2);
        body.extend_from_slice(&CIPHER_SUITE);
        body.push(1);
        body.push(0); // null compression
        Ok(body)
    }

    fn send_handshake(&mut self, message_type: u8, body: &[u8]) -> Result<(), String> {
        let mut message = Vec::with_capacity(12 + body.len());
        push_handshake_header(
            &mut message,
            message_type,
            body.len(),
            self.message_sequence,
        );
        message.extend_from_slice(body);
        self.handshake_messages.extend_from_slice(&message);
        self.message_sequence = self.message_sequence.wrapping_add(1);
        self.send_record(CT_HANDSHAKE, &message)
    }

    fn send_record(&mut self, content_type: u8, fragment: &[u8]) -> Result<(), String> {
        let mut record = Vec::with_capacity(13 + fragment.len());
        record.push(content_type);
        push_u16(&mut record, DTLS_VERSION);
        push_u16(&mut record, self.epoch);
        push_sequence_48(&mut record, self.send_sequence);
        push_u16(&mut record, fragment.len() as u16);
        record.extend_from_slice(fragment);
        self.socket
            .send(&record)
            .map_err(|error| format!("Le paquet Hue n’a pas pu être envoyé : {error}"))?;
        self.send_sequence = self.send_sequence.wrapping_add(1);
        Ok(())
    }

    fn send_encrypted(&mut self, content_type: u8, plaintext: &[u8]) -> Result<(), String> {
        let cipher = self
            .cipher
            .as_ref()
            .ok_or_else(|| "Le chiffrement Hue n’est pas initialisé.".to_owned())?;
        let mut explicit_nonce = Vec::with_capacity(8);
        push_u16(&mut explicit_nonce, self.epoch);
        push_sequence_48(&mut explicit_nonce, self.send_sequence);

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..4].copy_from_slice(&self.client_write_iv);
        nonce_bytes[4..].copy_from_slice(&explicit_nonce);

        let mut aad = explicit_nonce.clone();
        aad.push(content_type);
        push_u16(&mut aad, DTLS_VERSION);
        push_u16(&mut aad, plaintext.len() as u16);

        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| "Le paquet Hue n’a pas pu être chiffré.".to_owned())?;
        let mut fragment = explicit_nonce;
        fragment.extend_from_slice(&encrypted);

        let mut record = Vec::with_capacity(13 + fragment.len());
        record.push(content_type);
        push_u16(&mut record, DTLS_VERSION);
        push_u16(&mut record, self.epoch);
        push_sequence_48(&mut record, self.send_sequence);
        push_u16(&mut record, fragment.len() as u16);
        record.extend_from_slice(&fragment);
        self.socket
            .send(&record)
            .map_err(|error| format!("Le flux chiffré Hue a été interrompu : {error}"))?;
        self.send_sequence = self.send_sequence.wrapping_add(1);
        Ok(())
    }

    fn receive(&self) -> Result<Vec<u8>, String> {
        let mut buffer = [0u8; 4096];
        let length = self.socket.recv(&mut buffer).map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.kind() == std::io::ErrorKind::TimedOut
            {
                "Le Hue Bridge n’a pas répondu au flux Entertainment à temps.".to_owned()
            } else {
                format!("La réponse Entertainment est illisible : {error}")
            }
        })?;
        Ok(buffer[..length].to_vec())
    }

    fn parse_server_flight(&mut self, first: &[u8]) -> Result<(), String> {
        let mut datagram = first.to_vec();
        let mut got_hello = false;
        let mut got_done = false;

        for _ in 0..3 {
            let mut record_offset = 0usize;
            while record_offset + 13 <= datagram.len() {
                let record_len = read_u16(&datagram, record_offset + 11)? as usize;
                let start = record_offset + 13;
                let end = start.saturating_add(record_len);
                if end > datagram.len() {
                    return Err("Le pont a envoyé un enregistrement DTLS incomplet.".to_owned());
                }
                let payload = &datagram[start..end];
                let mut message_offset = 0usize;
                while message_offset + 12 <= payload.len() {
                    let message_type = payload[message_offset];
                    let message_len = read_u24(payload, message_offset + 1)?;
                    let message_end = message_offset + 12 + message_len;
                    if message_end > payload.len() {
                        break;
                    }
                    let message = &payload[message_offset..message_end];
                    match message_type {
                        HT_SERVER_HELLO => {
                            if message.len() < 46 {
                                return Err(
                                    "La réponse ServerHello du pont est incomplète.".to_owned()
                                );
                            }
                            self.server_random.copy_from_slice(&message[14..46]);
                            self.handshake_messages.extend_from_slice(message);
                            got_hello = true;
                        }
                        HT_SERVER_HELLO_DONE => {
                            self.handshake_messages.extend_from_slice(message);
                            got_done = true;
                        }
                        _ => {}
                    }
                    message_offset = message_end;
                }
                record_offset = end;
            }
            if got_hello && got_done {
                return Ok(());
            }
            datagram = self.receive()?;
        }
        Err("La négociation DTLS avec le Hue Bridge est incomplète.".to_owned())
    }

    fn derive_keys(&mut self) -> Result<(), String> {
        let length = self.psk.len();
        if length == 0 || length > u16::MAX as usize {
            return Err("La clé Entertainment a une longueur invalide.".to_owned());
        }
        let mut pre_master = Vec::with_capacity(length * 2 + 4);
        push_u16(&mut pre_master, length as u16);
        pre_master.resize(pre_master.len() + length, 0);
        push_u16(&mut pre_master, length as u16);
        pre_master.extend_from_slice(&self.psk);

        let mut master_seed = Vec::with_capacity(64);
        master_seed.extend_from_slice(&self.client_random);
        master_seed.extend_from_slice(&self.server_random);
        self.master_secret = prf(&pre_master, b"master secret", &master_seed, 48)?;

        let mut key_seed = Vec::with_capacity(64);
        key_seed.extend_from_slice(&self.server_random);
        key_seed.extend_from_slice(&self.client_random);
        let key_block = prf(&self.master_secret, b"key expansion", &key_seed, 40)?;
        self.client_write_iv.copy_from_slice(&key_block[32..36]);
        self.cipher = Some(
            Aes128Gcm::new_from_slice(&key_block[..16])
                .map_err(|_| "La clé AES du flux Hue est invalide.".to_owned())?,
        );
        Ok(())
    }
}

fn parse_hello_verify_request(data: &[u8]) -> Result<Vec<u8>, String> {
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

fn tls_random() -> Result<[u8; 32], String> {
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

fn prf(secret: &[u8], label: &[u8], seed: &[u8], length: usize) -> Result<Vec<u8>, String> {
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

fn hmac_sha256(secret: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret)
        .map_err(|_| "La clé HMAC du flux Hue est invalide.".to_owned())?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn push_handshake_header(target: &mut Vec<u8>, message_type: u8, length: usize, sequence: u16) {
    target.push(message_type);
    push_u24(target, length);
    push_u16(target, sequence);
    push_u24(target, 0);
    push_u24(target, length);
}

fn push_u16(target: &mut Vec<u8>, value: u16) {
    target.extend_from_slice(&value.to_be_bytes());
}

fn push_u24(target: &mut Vec<u8>, value: usize) {
    target.extend_from_slice(&[
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    ]);
}

fn push_sequence_48(target: &mut Vec<u8>, value: u64) {
    let bytes = value.to_be_bytes();
    target.extend_from_slice(&bytes[2..]);
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, String> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .ok_or_else(|| "La réponse DTLS est tronquée.".to_owned())?
        .try_into()
        .map_err(|_| "La réponse DTLS est invalide.".to_owned())?;
    Ok(u16::from_be_bytes(bytes))
}

fn read_u24(data: &[u8], offset: usize) -> Result<usize, String> {
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
