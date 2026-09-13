// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration test: RFC 6455 client ↔ server over an in-memory duplex —
//! opening handshake, text echo, ping/pong, and the closing handshake.

use tpt_async_io::TokioCompat;
use tpt_net_ws::prelude::*;

#[tokio::test]
async fn handshake_echo_and_close() {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);

    // Server: accept + echo until close.
    let server = tokio::spawn(async move {
        let mut ws = WebSocketStream::accept(TokioCompat::new(server_io))
            .await
            .expect("server handshake");
        while let Some(message) = ws.recv().await.expect("server recv") {
            match message {
                Message::Text(text) => {
                    ws.send(Message::Text(format!("echo: {text}")))
                        .await
                        .expect("server send");
                }
                Message::Close(_) => break,
                other => {
                    ws.send(other).await.expect("server send");
                }
            }
        }
    });

    // Client: handshake, exchange, close.
    let mut client = WebSocketStream::connect(TokioCompat::new(client_io), "/ws", "localhost")
        .await
        .expect("client handshake");

    client
        .send(Message::Text("hello tpt-net-ws".into()))
        .await
        .expect("client send");
    assert_eq!(
        client.recv().await.expect("client recv"),
        Some(Message::Text("echo: hello tpt-net-ws".into()))
    );

    // Ping/pong round-trip.
    client
        .send(Message::Ping(b"keepalive".to_vec()))
        .await
        .expect("ping");
    assert_eq!(
        client.recv().await.expect("pong"),
        Some(Message::Pong(b"keepalive".to_vec()))
    );

    // Binary round-trip.
    client
        .send(Message::Binary(vec![1, 2, 3]))
        .await
        .expect("binary send");
    assert_eq!(
        client.recv().await.expect("binary recv"),
        Some(Message::Binary(vec![1, 2, 3]))
    );

    // Closing handshake: client initiates, server confirms.
    client
        .send(Message::Close(Some((1000, "done".into()))))
        .await
        .expect("close");
    assert!(
        client.recv().await.expect("close recv").is_none(),
        "server must confirm close"
    );

    server.await.expect("server task");
}
