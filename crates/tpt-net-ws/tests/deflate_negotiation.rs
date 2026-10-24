// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Client-side `permessage-deflate` negotiation: the handshake is accepted
//! only when the server commits to `server_no_context_takeover`.

#![cfg(feature = "permessage-deflate")]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tpt_async_io::TokioCompat;
use tpt_net_ws::handshake::accept_key;
use tpt_net_ws::{WebSocketStream, WsError};

/// Answer the client's upgrade request with `extensions` and return the
/// result of `connect`.
async fn connect_with_extensions(extensions: String) -> Result<(), WsError> {
    let (client_io, mut server_io) = tokio::io::duplex(64 * 1024);

    let server = tokio::spawn(async move {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            server_io.read_exact(&mut byte).await.expect("request byte");
            head.push(byte[0]);
        }
        let text = String::from_utf8(head).expect("ascii request");
        let key = text
            .lines()
            .find_map(|line| line.strip_prefix("sec-websocket-key: "))
            .expect("client sent a key")
            .trim();
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             upgrade: websocket\r\n\
             connection: Upgrade\r\n\
             sec-websocket-accept: {}\r\n\
             sec-websocket-extensions: {extensions}\r\n\r\n",
            accept_key(key.as_bytes()),
        );
        server_io
            .write_all(response.as_bytes())
            .await
            .expect("response write");
    });

    let result = WebSocketStream::connect(TokioCompat::new(client_io), "/ws", "localhost")
        .await
        .map(|_| ());
    server.await.expect("server task");
    result
}

#[tokio::test]
async fn accepts_deflate_with_context_takeover() {
    connect_with_extensions("permessage-deflate".into())
        .await
        .expect("context takeover is supported in both directions");
}

#[tokio::test]
async fn refuses_smaller_compressor_window() {
    let err = connect_with_extensions("permessage-deflate; client_max_window_bits=10".into())
        .await
        .expect_err("a window below 15 cannot be honoured");
    assert!(matches!(err, WsError::Handshake(_)));
}

#[tokio::test]
async fn accepts_deflate_with_server_no_context_takeover() {
    connect_with_extensions(
        "permessage-deflate; server_no_context_takeover; client_no_context_takeover".into(),
    )
    .await
    .expect("server committed to no context takeover");
}
