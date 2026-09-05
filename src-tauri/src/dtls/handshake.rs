use super::*;

impl DtlsConnection {
    pub(super) fn handshake(&mut self) -> Result<(), String> {
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

    pub(super) fn send_cookie_client_hello(&mut self, cookie: &[u8]) -> Result<(), String> {
        self.message_sequence = 0;
        self.send_sequence = 0;
        self.handshake_messages.clear();
        let hello = self.build_client_hello(cookie)?;
        self.send_handshake(HT_CLIENT_HELLO, &hello)
    }

    pub(super) fn build_client_hello(&self, cookie: &[u8]) -> Result<Vec<u8>, String> {
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

    pub(super) fn parse_server_flight(&mut self, first: &[u8]) -> Result<(), String> {
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

    pub(super) fn derive_keys(&mut self) -> Result<(), String> {
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
