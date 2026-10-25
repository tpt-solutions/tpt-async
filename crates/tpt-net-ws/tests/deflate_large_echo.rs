// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Regression for Autobahn 12.2.9 / 12.2.14: a large compressed message sent by
//! the server must reach the peer intact. The peer side decodes the raw frame
//! and decompresses it with an independent codec, so the check does not depend
//! on our own send path being symmetric with our receive path.

#![cfg(feature = "permessage-deflate")]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tpt_async_io::TokioCompat;
use tpt_net_ws::deflate::Codec;
use tpt_net_ws::frame::{decode, Opcode, Role};
use tpt_net_ws::{Message, WebSocketStream};

const HANDSHAKE: &[u8] = b"GET /ws HTTP/1.1\r\nhost: t\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\nsec-websocket-extensions: permessage-deflate\r\n\r\n";

/// Deterministic incompressible bytes, so compressed output stays large.
fn noise(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// Server echoes one message of `payload` and the peer checks the bytes it reads.
async fn server_sends_and_peer_reads(message: Message, payload: Vec<u8>) {
    let (mut peer, server_io) = tokio::io::duplex(1024 * 1024);
    peer.write_all(HANDSHAKE).await.expect("handshake");

    let mut server = WebSocketStream::accept(TokioCompat::new(server_io))
        .await
        .expect("server handshake");
    assert!(server.deflate_negotiated());
    server.send(message).await.expect("send");
    drop(server);

    // Skip the 101 response head.
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        peer.read_exact(&mut byte).await.expect("response byte");
        head.push(byte[0]);
    }

    let mut wire = Vec::new();
    peer.read_to_end(&mut wire).await.expect("read frames");

    let (frame, used) = decode(&wire, Role::Client)
        .expect("valid frame")
        .expect("complete frame");
    assert_eq!(used, wire.len(), "exactly one frame on the wire");
    assert!(frame.fin);
    assert!(frame.rsv1, "message must be marked compressed");
    assert!(matches!(frame.opcode, Opcode::Text | Opcode::Binary));

    // The server resets its context per message (server_default), so a fresh
    // decompressor is the correct independent reference.
    let decoded = Codec::new(true, true)
        .decompress(&frame.payload, 16 * 1024 * 1024)
        .expect("decompress");
    assert_eq!(decoded.len(), payload.len(), "length mismatch");
    assert!(decoded == payload, "payload mismatch");
}

#[tokio::test]
async fn binary_64k_noise_reaches_peer_intact() {
    let payload = noise(65536);
    server_sends_and_peer_reads(Message::Binary(payload.clone()), payload).await;
}

#[tokio::test]
async fn binary_128k_noise_reaches_peer_intact() {
    let payload = noise(131072);
    server_sends_and_peer_reads(Message::Binary(payload.clone()), payload).await;
}

/// Full echo as Autobahn drives it: a masked, fragmented 64 KiB compressed
/// message comes in, the server receives it and echoes it back.
#[tokio::test]
async fn fragmented_64k_echo_round_trips_on_the_wire() {
    use tpt_net_ws::frame::{encode, Frame};

    let payload = noise(65536);
    let compressed = Codec::new(true, true).compress(&payload).expect("compress");

    let (mut peer, server_io) = tokio::io::duplex(1024 * 1024);
    peer.write_all(HANDSHAKE).await.expect("handshake");

    let chunks: Vec<&[u8]> = compressed.chunks(1024).collect();
    let last = chunks.len() - 1;
    for (i, chunk) in chunks.iter().enumerate() {
        let mut out = Vec::new();
        encode(
            &Frame {
                fin: i == last,
                rsv1: i == 0,
                opcode: if i == 0 {
                    Opcode::Binary
                } else {
                    Opcode::Continuation
                },
                payload: chunk.to_vec(),
            },
            Some([9, 8, 7, 6]),
            &mut out,
        );
        peer.write_all(&out).await.expect("fragment");
    }

    let mut server = WebSocketStream::accept(TokioCompat::new(server_io))
        .await
        .expect("server handshake");
    let message = server.recv().await.expect("recv").expect("message");
    assert_eq!(message, Message::Binary(payload.clone()));
    server.send(message).await.expect("echo");
    drop(server);

    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        peer.read_exact(&mut byte).await.expect("response byte");
        head.push(byte[0]);
    }
    let mut wire = Vec::new();
    peer.read_to_end(&mut wire).await.expect("read echo");

    let (frame, used) = decode(&wire, Role::Client)
        .expect("valid frame")
        .expect("complete frame");
    assert_eq!(used, wire.len());
    let decoded = Codec::new(true, true)
        .decompress(&frame.payload, 16 * 1024 * 1024)
        .expect("decompress echo");
    assert!(
        decoded == payload,
        "echo payload mismatch: got {} bytes",
        decoded.len()
    );
}

#[tokio::test]
async fn text_64k_reaches_peer_intact() {
    let text: String = noise(65536)
        .iter()
        .map(|b| char::from(b'a' + b % 26))
        .collect();
    server_sends_and_peer_reads(Message::Text(text.clone()), text.into_bytes()).await;
}
