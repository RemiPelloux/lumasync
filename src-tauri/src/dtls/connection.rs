use super::*;

impl DtlsConnection {
    pub(super) fn new(host: &str, port: u16, identity: &[u8], psk: &[u8]) -> Result<Self, String> {
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

    pub(super) fn send_handshake(&mut self, message_type: u8, body: &[u8]) -> Result<(), String> {
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

    pub(super) fn send_record(&mut self, content_type: u8, fragment: &[u8]) -> Result<(), String> {
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

    pub(super) fn send_encrypted(
        &mut self,
        content_type: u8,
        plaintext: &[u8],
    ) -> Result<(), String> {
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

    pub(super) fn receive(&self) -> Result<Vec<u8>, String> {
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
}
