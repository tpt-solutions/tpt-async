// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `permessage-deflate` message handling over an in-memory duplex: a fragmented
//! compressed message is decompressed as one stream, and a peer that keeps
//! compression context across messages is decoded correctly.

#![cfg(feature = "permessage-deflate")]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tpt_async_io::TokioCompat;
use tpt_net_ws::deflate::Codec;
use tpt_net_ws::frame::{encode, Frame, Opcode};
use tpt_net_ws::handshake::accept_key;
use tpt_net_ws::{Message, WebSocketStream};

const HANDSHAKE: &[u8] = b"GET /ws HTTP/1.1\r\nhost: t\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\nsec-websocket-extensions: permessage-deflate\r\n\r\n";

/// A server-side frame: unmasked, as RFC 6455 requires.
fn server_frame(fin: bool, rsv1: bool, opcode: Opcode, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    encode(
        &Frame {
            fin,
            rsv1,
            opcode,
            payload: payload.to_vec(),
        },
        None,
        &mut out,
    );
    out
}

#[tokio::test]
async fn fragmented_compressed_message_is_decompressed_once() {
    let text = b"fragmented compressed payload ".repeat(20);
    // The server codec resets per message, so a fresh compressor matches it.
    let compressed = Codec::new(true, false).compress(&text).unwrap();
    let (head, tail) = compressed.split_at(compressed.len() / 2);

    let (mut client_io, server_io) = tokio::io::duplex(64 * 1024);
    client_io.write_all(HANDSHAKE).await.expect("handshake");
    client_io
        .write_all(&server_frame_as_client(false, true, Opcode::Text, head))
        .await
        .expect("first fragment");
    client_io
        .write_all(&server_frame_as_client(
            true,
            false,
            Opcode::Continuation,
            tail,
        ))
        .await
        .expect("second fragment");

    let mut server = WebSocketStream::accept(TokioCompat::new(server_io))
        .await
        .expect("server handshake");
    assert!(server.deflate_negotiated());
    let message = server.recv().await.expect("recv").expect("message");
    assert_eq!(message, Message::Text(String::from_utf8(text).unwrap()));
}

/// Client-masked variant of [`server_frame`] for feeding the server.
fn server_frame_as_client(fin: bool, rsv1: bool, opcode: Opcode, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    encode(
        &Frame {
            fin,
            rsv1,
            opcode,
            payload: payload.to_vec(),
        },
        Some([1, 2, 3, 4]),
        &mut out,
    );
    out
}

#[tokio::test]
async fn client_decodes_messages_compressed_with_shared_context() {
    let text = b"the quick brown fox jumps over the lazy dog ".repeat(40);

    let (client_io, mut peer) = tokio::io::duplex(64 * 1024);

    // Peer: answer the handshake, then send two messages that share context.
    let payload = text.clone();
    let peer_task = tokio::spawn(async move {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            peer.read_exact(&mut byte).await.expect("request byte");
            head.push(byte[0]);
        }
        let request = String::from_utf8(head).expect("ascii request");
        let key = request
            .lines()
            .find_map(|line| line.strip_prefix("sec-websocket-key: "))
            .expect("client sent a key")
            .trim();
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             upgrade: websocket\r\n\
             connection: Upgrade\r\n\
             sec-websocket-accept: {}\r\n\
             sec-websocket-extensions: permessage-deflate\r\n\r\n",
            accept_key(key.as_bytes()),
        );
        peer.write_all(response.as_bytes())
            .await
            .expect("response write");

        let mut compressor = Codec::new(false, false);
        for _ in 0..2 {
            let compressed = compressor.compress(&payload).expect("compress");
            peer.write_all(&server_frame(true, true, Opcode::Text, &compressed))
                .await
                .expect("message write");
        }
    });

    let mut client = WebSocketStream::connect(TokioCompat::new(client_io), "/ws", "localhost")
        .await
        .expect("client handshake");
    assert!(client.deflate_negotiated());

    let expected = Message::Text(String::from_utf8(text).unwrap());
    assert_eq!(
        client.recv().await.expect("first").expect("message"),
        expected
    );
    assert_eq!(
        client.recv().await.expect("second").expect("message"),
        expected
    );
    peer_task.await.expect("peer task");
}
