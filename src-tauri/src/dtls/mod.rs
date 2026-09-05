//! Minimal DTLS 1.2 PSK sender for the Philips Hue Entertainment protocol.
//!
//! It intentionally supports only TLS_PSK_WITH_AES_128_GCM_SHA256, the cipher
//! required by the Hue Bridge. The protocol framing was cross-checked against
//! the Philips Hue Entertainment documentation and music-assistant's
//! Apache-2.0 `hue-entertainment` implementation.

mod connection;
mod handshake;
mod wire;
use wire::*;

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
