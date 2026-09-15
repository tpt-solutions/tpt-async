// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal HTTP/1.1 server + client over an in-memory duplex: the exact
//! code shape you would use with real sockets (swap the duplex for a
//! `TcpStream` and nothing else changes).

use tpt_async_io::TokioCompat;
use tpt_net_http::prelude::*;

struct Health;

impl Handler<TokioCompat<tokio::io::DuplexStream>> for Health {
    async fn handle(
        &mut self,
        _req: &mut ServerRequest<'_, TokioCompat<tokio::io::DuplexStream>>,
    ) -> ResponseData {
        ResponseData::ok(br#"{"status":"ok"}"#.to_vec())
    }
}

#[tokio::main]
async fn main() {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);

    let server = tokio::spawn(async move {
        serve_connection(TokioCompat::new(server_io), &mut Health)
            .await
            .expect("server");
    });

    let mut conn = ClientConnection::new(TokioCompat::new(client_io));
    let mut response = conn
        .send(&Request::new("GET", "/health"), "localhost")
        .await
        .expect("request");

    assert_eq!(response.status(), 200);
    let body = response.body_bytes(4096).await.expect("body");
    println!(
        "http example: {} {}",
        response.status(),
        String::from_utf8_lossy(&body)
    );

    drop(conn);
    server.await.expect("server task");
}
