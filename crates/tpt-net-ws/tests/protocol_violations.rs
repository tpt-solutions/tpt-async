// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Protocol violations: each one must fail `recv` with a protocol error and
//! answer with a Close frame carrying the RFC 6455 status code.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tpt_async_io::TokioCompat;
use tpt_net_ws::frame::{decode, encode, Frame, Opcode, Role};
use tpt_net_ws::{WebSocketStream, WsError};

const HANDSHAKE: &[u8] = b"GET /ws HTTP/1.1\r\nhost: t\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\n\r\n";

/// A client-masked frame, as the server expects to receive it.
fn frame(fin: bool, opcode: Opcode, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    encode(
        &Frame {
            fin,
            rsv1: false,
            opcode,
            payload: payload.to_vec(),
        },
        Some([1, 2, 3, 4]),
        &mut out,
    );
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Accept a connection, feed it `frames`, and return the error from the
/// server's `recv` together with the status code of the Close it sent back.
async fn violate(frames: Vec<Vec<u8>>) -> (WsError, u16) {
    let (mut client_io, server_io) = tokio::io::duplex(64 * 1024);
    client_io
        .write_all(HANDSHAKE)
        .await
        .expect("handshake write");
    for bytes in &frames {
        client_io.write_all(bytes).await.expect("frame write");
    }

    let mut server = WebSocketStream::accept(TokioCompat::new(server_io))
        .await
        .expect("server handshake");
    let err = server
        .recv()
        .await
        .expect_err("protocol violation must fail recv");

    // Skip the 101 response head, then decode the Close frame the server wrote.
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 256];
    loop {
        let n = client_io.read(&mut chunk).await.expect("read");
        assert!(n > 0, "server closed without sending a Close frame");
        bytes.extend_from_slice(&chunk[..n]);
        let Some(head_end) = find(&bytes, b"\r\n\r\n") else {
            continue;
        };
        if let Some((frame, _)) = decode(&bytes[head_end + 4..], Role::Client).expect("decode") {
            assert!(
                matches!(frame.opcode, Opcode::Close),
                "expected a Close frame"
            );
            let code = u16::from_be_bytes([frame.payload[0], frame.payload[1]]);
            return (err, code);
        }
    }
}

#[tokio::test]
async fn fragmented_ping_closes_with_1002() {
    let (err, code) = violate(vec![frame(false, Opcode::Ping, b"x")]).await;
    assert!(matches!(err, WsError::Protocol(_)));
    assert_eq!(code, 1002);
}

#[tokio::test]
async fn fragmented_close_closes_with_1002() {
    let (_, code) = violate(vec![frame(false, Opcode::Close, &[0x03, 0xE8])]).await;
    assert_eq!(code, 1002);
}

#[tokio::test]
async fn one_byte_close_payload_closes_with_1002() {
    let (_, code) = violate(vec![frame(true, Opcode::Close, &[0x03])]).await;
    assert_eq!(code, 1002);
}

#[tokio::test]
async fn reserved_close_code_1005_closes_with_1002() {
    // 1005 may not appear on the wire.
    let (_, code) = violate(vec![frame(true, Opcode::Close, &[0x03, 0xED])]).await;
    assert_eq!(code, 1002);
}

#[tokio::test]
async fn close_code_zero_closes_with_1002() {
    let (_, code) = violate(vec![frame(true, Opcode::Close, &[0x00, 0x00])]).await;
    assert_eq!(code, 1002);
}

#[tokio::test]
async fn invalid_utf8_close_reason_closes_with_1007() {
    let (_, code) = violate(vec![frame(true, Opcode::Close, &[0x03, 0xE8, 0xFF])]).await;
    assert_eq!(code, 1007);
}

#[tokio::test]
async fn invalid_utf8_text_message_closes_with_1007() {
    let (err, code) = violate(vec![frame(true, Opcode::Text, &[0xFF])]).await;
    assert!(matches!(err, WsError::Protocol(_)));
    assert_eq!(code, 1007);
}
