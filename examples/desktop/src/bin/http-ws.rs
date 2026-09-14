//! `http-ws` — a fully runnable end-to-end demonstration: our HTTP client
//! against our HTTP server, then a WebSocket echo, all over in-memory
//! duplex pipes (no sockets needed).
//!
//! Run with: `cargo run -p desktop-examples --bin http-ws`

use std::time::Duration;

use tpt_async::prelude::*;
use tpt_async_io::TokioCompat;
use tpt_net_http::prelude::*;
use tpt_net_ws::prelude::*;

type Conn = TokioCompat<tokio::io::DuplexStream>;

// ── HTTP server ──────────────────────────────────────────────────────────────

struct Health;

impl Handler<Conn> for Health {
    async fn handle(&mut self, request: &mut ServerRequest<'_, Conn>) -> ResponseData {
        match request.target() {
            b"/health" => ResponseData::ok(br#"{"status":"ok"}"#.to_vec()),
            _ => ResponseData::not_found(),
        }
    }
}

#[tokio::main]
async fn main() {
    // ── HTTP: server task + client through a connected pipe pair ────────────
    let (client_side, server_side) = tokio::io::duplex(64 * 1024);
    let http_server =
        tokio::spawn(
            async move { serve_connection(TokioCompat::new(server_side), &mut Health).await },
        );

    let mut conn = ClientConnection::new(TokioCompat::new(client_side));
    let mut response = conn
        .send(&Request::new("GET", "/health"), "localhost")
        .await
        .expect("http send");
    let body = response.body_bytes(4096).await.expect("http body");
    println!("http-ws: GET /health -> {} {body:?}", response.status());
    assert_eq!(response.status(), 200);
    drop(conn);
    http_server.await.expect("http server").expect("http serve");

    // ── WebSocket: server accepts, client echoes, both on our stack ────────
    let (ws_client_io, ws_server_io) = tokio::io::duplex(64 * 1024);
    let ws_server = tokio::spawn(async move {
        let mut ws = WebSocketStream::accept(TokioCompat::new(ws_server_io))
            .await
            .expect("ws accept");
        while let Some(message) = ws.recv().await.expect("ws recv") {
            match message {
                Message::Text(text) => {
                    ws.send(Message::Text(format!("echo: {text}")))
                        .await
                        .expect("ws send");
                }
                Message::Close(_) => break,
                other => ws.send(other).await.expect("ws send"),
            }
        }
    });

    let mut ws = WebSocketStream::connect(TokioCompat::new(ws_client_io), "/ws", "localhost")
        .await
        .expect("ws handshake");

    // A timeout-wrapped exchange, showing the timer integration:
    let reply = timeout(Duration::from_secs(5), async {
        ws.send(Message::Text("hello over ws".into()))
            .await
            .expect("ws send");
        ws.recv().await.expect("ws recv")
    })
    .await
    .expect("ws exchange timed out");
    assert_eq!(reply, Some(Message::Text("echo: hello over ws".into())));
    println!("http-ws: websocket echo OK");

    ws.send(Message::Close(Some((1000, "bye".into()))))
        .await
        .expect("ws close");
    ws_server.await.expect("ws server");
}
