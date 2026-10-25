// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration test: timer-driven keepalive over an in-memory duplex — a
//! live peer answers the server's `Ping`, a silent peer is timed out.

use std::time::Duration;

use tpt_async_io::TokioCompat;
use tpt_net_ws::prelude::*;

const INTERVAL: Duration = Duration::from_millis(20);
const TIMEOUT: Duration = Duration::from_millis(200);

#[tokio::test]
async fn live_peer_answers_keepalive_ping() {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);

    // Server: idle long enough to send a keepalive ping, sees the pong, closes.
    let server = tokio::spawn(async move {
        let mut ws = WebSocketStream::accept(TokioCompat::new(server_io))
            .await
            .expect("server handshake");
        ws.set_keepalive(Some(KeepAlive {
            interval: INTERVAL,
            timeout: TIMEOUT,
        }));
        let message = ws.recv().await.expect("server recv");
        assert_eq!(message, Some(Message::Pong(Vec::new())));
        ws.close(1000).await.expect("server close");
    });

    // Client: no keepalive of its own; `recv` answers the ping and then
    // receives the server's close.
    let mut client = WebSocketStream::connect(TokioCompat::new(client_io), "/ws", "localhost")
        .await
        .expect("client handshake");
    assert_eq!(client.recv().await.expect("client recv"), None);

    server.await.expect("server task");
}

#[tokio::test]
async fn silent_peer_is_timed_out() {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);

    let (server, client) = tokio::join!(
        WebSocketStream::accept(TokioCompat::new(server_io)),
        WebSocketStream::connect(TokioCompat::new(client_io), "/ws", "localhost"),
    );
    let mut server = server.expect("server handshake");
    // Held alive but never read, so it never answers the keepalive ping.
    let _client = client.expect("client handshake");

    server.set_keepalive(Some(KeepAlive {
        interval: INTERVAL,
        timeout: TIMEOUT,
    }));
    let err = server.recv().await.expect_err("silent peer must time out");
    assert!(
        matches!(err, WsError::TimedOut),
        "unexpected error: {err:?}"
    );
}
