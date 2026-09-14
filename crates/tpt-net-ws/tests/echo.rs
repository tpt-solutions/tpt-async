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

#[tokio::test]
async fn fragmented_message_is_assembled() {
    use tokio::io::AsyncWriteExt as _;

    use tpt_net_ws::frame::{encode, Frame, Opcode};

    let (mut client_io, server_io) = tokio::io::duplex(64 * 1024);

    let server = tokio::spawn(async move {
        WebSocketStream::accept(TokioCompat::new(server_io))
            .await
            .expect("handshake")
    });

    // Handshake head built from explicit CRLF pieces (no literals that
    // editors/converters may normalize).
    let mut handshake = b"GET /ws HTTP/1.1".to_vec();
    handshake.extend_from_slice(b"\r\n");
    let headers: &[&[u8]] = &[
        b"host: t",
        b"upgrade: websocket",
        b"connection: Upgrade",
        b"sec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==",
        b"sec-websocket-version: 13",
    ];
    for header in headers {
        handshake.extend_from_slice(header);
        handshake.extend_from_slice(b"\r\n");
    }
    handshake.extend_from_slice(b"\r\n");

    // Feed a handcrafted fragmented text message: "frag" (no FIN) +
    // "ment" (FIN, continuation), then an unrelated "next" message.
    let mut raw = Vec::new();
    encode(
        &Frame {
            fin: false,
            rsv1: false,
            opcode: Opcode::Text,
            payload: b"frag".to_vec(),
        },
        Some([1, 2, 3, 4]),
        &mut raw,
    );
    encode(
        &Frame {
            fin: true,
            rsv1: false,
            opcode: Opcode::Continuation,
            payload: b"ment".to_vec(),
        },
        Some([5, 6, 7, 8]),
        &mut raw,
    );
    encode(
        &Frame {
            fin: true,
            rsv1: false,
            opcode: Opcode::Text,
            payload: b"next".to_vec(),
        },
        Some([9, 10, 11, 12]),
        &mut raw,
    );
    use tokio::io::AsyncWriteExt as _;
    client_io
        .write_all(&handshake)
        .await
        .expect("handshake write");
    client_io.write_all(&raw).await.expect("frame write");

    let mut server = server.await.expect("server task");

    let first = server.recv().await.expect("recv 1").expect("message 1");
    assert_eq!(
        first,
        Message::Text("fragment".into()),
        "fragments must assemble"
    );
    let second = server.recv().await.expect("recv 2").expect("message 2");
    assert_eq!(
        second,
        Message::Text("next".into()),
        "messages must stay separated"
    );
    drop(client_io); // keep-alive while the server wrote its 101 + reads
}
