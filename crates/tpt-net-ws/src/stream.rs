// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`WebSocketStream`] — a full-duplex RFC 6455 connection.

use alloc::string::String;
use alloc::vec::Vec;

use tpt_async_io::{AsyncRead, AsyncWrite};
use tpt_net_http::connection::HttpConnection;

use crate::error::WsError;
use crate::frame::{self, Frame, Opcode, Role};
use crate::handshake;

/// Trim ASCII whitespace around a byte slice.
fn trim_ows(value: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = value.len();
    while start < end && (value[start] == b' ' || value[start] == b'\t') {
        start += 1;
    }
    while end > start && (value[end - 1] == b' ' || value[end - 1] == b'\t') {
        end -= 1;
    }
    &value[start..end]
}

/// A WebSocket message (post-assembly: fragmentation is handled internally).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// UTF-8 text.
    Text(String),
    /// Binary bytes.
    Binary(Vec<u8>),
    /// Ping control frame payload.
    Ping(Vec<u8>),
    /// Pong control frame payload.
    Pong(Vec<u8>),
    /// Close frame: optional `(code, reason)`.
    Close(Option<(u16, String)>),
}

/// A WebSocket connection over any async transport.
///
/// Received `Ping` frames are answered automatically; fragmented messages are
/// assembled; the closing handshake is completed on [`recv`](Self::recv) returning
/// `None`.
pub struct WebSocketStream<IO> {
    conn: HttpConnection<IO>,
    role: Role,
    /// Bytes read from the transport but not yet consumed by the codec.
    frame_buf: Vec<u8>,
    close_sent: bool,
    close_received: bool,
}

impl<IO: AsyncRead + AsyncWrite + Unpin> WebSocketStream<IO> {
    pub(crate) fn from_conn(conn: HttpConnection<IO>, role: Role) -> Self {
        Self {
            conn,
            role,
            frame_buf: Vec::new(),
            close_sent: false,
            close_received: false,
        }
    }

    /// Complete the *server-side* opening handshake on an established
    /// transport: read the HTTP upgrade request, validate it, and answer
    /// `101 Switching Protocols`.
    pub async fn accept(io: IO) -> Result<Self, WsError> {
        let mut conn = HttpConnection::new(io);
        let head = conn
            .read_request_head()
            .await?
            .ok_or(WsError::Handshake("connection closed before handshake"))?;

        if !head.method().eq_ignore_ascii_case(b"GET") {
            return Err(WsError::Handshake("upgrade requires a GET request"));
        }
        let headers = head.headers();
        if !headers
            .get(b"upgrade")
            .map(|v| v.eq_ignore_ascii_case(b"websocket"))
            .unwrap_or(false)
        {
            return Err(WsError::Handshake("upgrade header missing"));
        }
        if !headers
            .get(b"connection")
            .map(|v| {
                v.split(|&b| b == b',')
                    .any(|p| trim_ows(p).eq_ignore_ascii_case(b"upgrade"))
            })
            .unwrap_or(false)
        {
            return Err(WsError::Handshake(
                "connection header missing Upgrade token",
            ));
        }
        let key = headers
            .get(b"sec-websocket-key")
            .ok_or(WsError::Handshake("Sec-WebSocket-Key missing"))?;
        if headers
            .get(b"sec-websocket-version")
            .map(|v| v != b"13")
            .unwrap_or(true)
        {
            return Err(WsError::Handshake("unsupported Sec-WebSocket-Version"));
        }

        let accept = handshake::validate_and_accept(key)?;
        conn.write_message(
            core::str::from_utf8(&handshake::render_accept_response(&accept))
                .expect("accept response is ASCII"),
            &[],
        )
        .await?;

        Ok(Self::from_conn(conn, Role::Server))
    }

    /// Complete the *client-side* opening handshake: send the upgrade
    /// request for `path` on `host`, validate the `101` response and its
    /// `Sec-WebSocket-Accept`.
    pub async fn connect(io: IO, path: &str, host: &str) -> Result<Self, WsError> {
        let key_raw = handshake::generate_client_key();
        let key_b64 = handshake::base64_encode(&key_raw);
        let expected = handshake::accept_key(key_b64.as_bytes());

        let mut conn = HttpConnection::new(io);
        let request = handshake::render_client_request(path, host, &key_b64);
        conn.write_message(
            core::str::from_utf8(&request).expect("client request is ASCII"),
            &[],
        )
        .await?;

        let head = conn.read_response_head().await.map_err(|e| match e {
            tpt_net_http::HttpError::ConnectionClosed => {
                WsError::Handshake("connection closed mid-handshake")
            }
            tpt_net_http::HttpError::Io(io) => WsError::Io(io),
            other => WsError::Handshake(Box::leak(other.to_string().into_boxed_str())),
        })?;

        if head.status() != 101 {
            return Err(WsError::Handshake("server did not switch protocols"));
        }
        if head.headers().get(b"sec-websocket-accept") != Some(expected.as_bytes()) {
            return Err(WsError::Handshake("Sec-WebSocket-Accept mismatch"));
        }

        Ok(Self::from_conn(conn, Role::Client))
    }

    /// Send a message.  Text payloads must be UTF-8; control payloads are
    /// capped at 125 bytes by the RFC.
    pub async fn send(&mut self, message: Message) -> Result<(), WsError> {
        let (opcode, payload, fin): (Opcode, Vec<u8>, bool) = match message {
            Message::Text(text) => (Opcode::Text, text.into_bytes(), true),
            Message::Binary(bytes) => (Opcode::Binary, bytes, true),
            Message::Ping(payload) => {
                Self::check_control(&payload)?;
                (Opcode::Ping, payload, true)
            }
            Message::Pong(payload) => {
                Self::check_control(&payload)?;
                (Opcode::Pong, payload, true)
            }
            Message::Close(info) => {
                let mut payload = Vec::new();
                if let Some((code, reason)) = info {
                    payload.extend_from_slice(&code.to_be_bytes());
                    payload.extend_from_slice(reason.as_bytes());
                }
                Self::check_control(&payload)?;
                self.close_sent = true;
                (Opcode::Close, payload, true)
            }
        };

        let mut out = Vec::new();
        frame::encode(
            &Frame {
                fin,
                opcode,
                payload,
            },
            // Clients MUST mask their frames (RFC 6455 §5.1).
            if self.role == Role::Client {
                Some(Self::random_mask())
            } else {
                None
            },
            &mut out,
        );
        self.conn.write_raw(&out).await?;
        Ok(())
    }

    /// Receive the next message, transparently handling `Ping` (auto-`Pong`),
    /// fragmentation, and the closing handshake.  `Ok(None)` = the peer
    /// closed.
    pub async fn recv(&mut self) -> Result<Option<Message>, WsError> {
        let mut message_opcode: Option<Opcode> = None;
        let mut assembled = Vec::new();

        loop {
            // Decode from buffered bytes, topping up from the transport.
            let decoded = loop {
                if let Some((frame, consumed)) = frame::decode(&self.frame_buf, self.role)? {
                    self.frame_buf.drain(..consumed);
                    break frame;
                }
                let mut chunk = [0u8; 8192];
                let n = self.conn.read_raw(&mut chunk).await?;
                if n == 0 {
                    return if self.close_received {
                        Ok(None)
                    } else {
                        Err(WsError::ConnectionClosed)
                    };
                }
                self.frame_buf.extend_from_slice(&chunk[..n]);
            };

            match decoded.opcode {
                Opcode::Ping => {
                    self.send(Message::Pong(decoded.payload.clone())).await?;
                    continue; // control frames never participate in message assembly
                }
                Opcode::Pong => {
                    return Ok(Some(Message::Pong(decoded.payload)));
                }
                Opcode::Close => {
                    if !self.close_sent {
                        // Echo the close, preserving the status code.
                        let info = if decoded.payload.len() >= 2 {
                            Some((
                                u16::from_be_bytes([decoded.payload[0], decoded.payload[1]]),
                                String::from_utf8_lossy(&decoded.payload[2..]).into_owned(),
                            ))
                        } else {
                            None
                        };
                        self.send(Message::Close(info)).await?;
                    }
                    self.close_received = true;
                    return Ok(None);
                }
                Opcode::Text | Opcode::Binary => {
                    if message_opcode.is_some() {
                        return Err(WsError::Protocol(
                            "new data frame during fragmented message",
                        ));
                    }
                    message_opcode = Some(decoded.opcode);
                    assembled.extend_from_slice(&decoded.payload);
                    Self::check_size(&assembled)?;
                }
                Opcode::Continuation => {
                    if message_opcode.is_none() {
                        return Err(WsError::Protocol("continuation without initial frame"));
                    }
                    assembled.extend_from_slice(&decoded.payload);
                    Self::check_size(&assembled)?;
                }
            }

            if decoded.fin {
                return Ok(match message_opcode.take() {
                    Some(Opcode::Text) => Some(Message::Text(
                        String::from_utf8(assembled)
                            .map_err(|_| WsError::Protocol("invalid UTF-8 in text message"))?,
                    )),
                    Some(Opcode::Binary) => Some(Message::Binary(assembled)),
                    _ => return Err(WsError::Protocol("final frame without a message")),
                });
            }
        }
    }

    /// Begin (or echo) the closing handshake and drain until the peer
    /// confirms.  Idempotent.
    pub async fn close(&mut self, code: u16) -> Result<(), WsError> {
        if !self.close_sent {
            self.send(Message::Close(Some((code, String::new()))))
                .await?;
            self.close_sent = true;
        }
        while self.recv().await?.is_some() {}
        Ok(())
    }

    /// Whether the peer has completed the closing handshake.
    pub fn is_closed(&self) -> bool {
        self.close_sent && self.close_received
    }

    fn check_control(payload: &[u8]) -> Result<(), WsError> {
        if payload.len() > 125 {
            return Err(WsError::Protocol("control frame payload exceeds 125 bytes"));
        }
        Ok(())
    }

    fn check_size(assembled: &[u8]) -> Result<(), WsError> {
        if assembled.len() > frame::MAX_PAYLOAD {
            return Err(WsError::Protocol("message exceeds 16 MiB limit"));
        }
        Ok(())
    }

    fn random_mask() -> [u8; 4] {
        let mut key = [0u8; 4];
        getrandom::getrandom(&mut key).expect("system RNG unavailable");
        key
    }
}
