// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WebSocket echo: server accepts an upgrade, client says hello, both sides
//! complete the closing handshake — all over an in-memory duplex.

use tpt_async_io::TokioCompat;
use tpt_net_ws::prelude::*;

#[tokio::main]
async fn main() {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);

    let server = tokio::spawn(async move {
        let mut ws = WebSocketStream::accept(TokioCompat::new(server_io))
            .await
            .expect("accept");
        while let Some(msg) = ws.recv().await.expect("recv") {
            ws.send(msg).await.expect("echo send");
        }
    });

    let mut client = WebSocketStream::connect(TokioCompat::new(client_io), "/ws", "localhost")
        .await
        .expect("connect");

    client
        .send(Message::Text("hello over ws".into()))
        .await
        .expect("send");
    let reply = client.recv().await.expect("recv");
    println!("ws example: {reply:?}");

    client
        .send(Message::Close(Some((1000, "done".into()))))
        .await
        .expect("close");
    let _ = client.recv().await;
    server.await.expect("server task");
}
