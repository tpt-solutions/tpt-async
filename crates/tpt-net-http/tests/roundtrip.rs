// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration test: full HTTP/1.1 round-trip — our client against our
//! server over an in-memory duplex.  Covers head parsing, keep-alive,
//! request bodies, and zero-copy header access.

use tpt_async_io::TokioCompat;
use tpt_net_http::prelude::*;

// ── Test handler ─────────────────────────────────────────────────────────────

type Conn = TokioCompat<tokio::io::DuplexStream>;

struct EchoHandler;

impl Handler<Conn> for EchoHandler {
    async fn handle(&mut self, request: &mut ServerRequest<'_, Conn>) -> ResponseData {
        assert_eq!(request.method(), b"POST");
        assert_eq!(request.target(), b"/echo");

        // Zero-copy header access into the frozen read buffer.
        let marker = request
            .header(b"x-marker")
            .map(|v| v.to_vec())
            .unwrap_or_default();

        let body = request.body_bytes(64 * 1024).await.unwrap();

        ResponseData::ok(body).header("x-marker-echo", &String::from_utf8_lossy(&marker))
    }
}

trait WithHeader {
    fn header(self, name: &str, value: &str) -> Self;
}

impl WithHeader for ResponseData {
    fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn full_roundtrip_with_keepalive_and_body() {
    let (client_side, server_side) = tokio::io::duplex(64 * 1024);

    let server = tokio::spawn(async move {
        serve_connection(TokioCompat::new(server_side), &mut EchoHandler).await
    });

    let mut conn = ClientConnection::new(TokioCompat::new(client_side));

    // ── Request 1: POST with a body, echoed back. ────────────────────────
    let request = Request::new("POST", "/echo")
        .header("x-marker", "round-trip-1")
        .body(b"hello tpt-net-http".to_vec());
    let mut response = conn.send(&request, "localhost").await.unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response.header(b"x-marker-echo"),
        Some(&b"round-trip-1"[..]),
        "zero-copy header read failed"
    );
    let body = response.body_bytes(64 * 1024).await.unwrap();
    assert_eq!(body, b"hello tpt-net-http");

    // ── Request 2: same connection (keep-alive), different body. ─────────
    let request = Request::new("POST", "/echo")
        .header("x-marker", "round-trip-2")
        .body(b"second".to_vec());
    let mut response = conn.send(&request, "localhost").await.unwrap();

    assert_eq!(response.status(), 200);
    let body = response.body_bytes(64 * 1024).await.unwrap();
    assert_eq!(body, b"second");

    drop(conn);
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn get_without_body_roundtrip() {
    struct Simple;

    impl Handler<Conn> for Simple {
        async fn handle(&mut self, request: &mut ServerRequest<'_, Conn>) -> ResponseData {
            assert_eq!(request.method(), b"GET");
            assert_eq!(request.target(), b"/health");
            assert!(request.header(b"host").is_some(), "client must send host");
            ResponseData::ok(b"ok".to_vec())
        }
    }

    let (client_side, server_side) = tokio::io::duplex(64 * 1024);
    let server =
        tokio::spawn(
            async move { serve_connection(TokioCompat::new(server_side), &mut Simple).await },
        );

    let mut conn = ClientConnection::new(TokioCompat::new(client_side));
    let mut response = conn
        .send(&Request::new("GET", "/health"), "example.com")
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response.header(b"content-length"), Some(&b"2"[..]));
    let body = response.body_bytes(64 * 1024).await.unwrap();
    assert_eq!(body, b"ok");

    drop(conn);
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn oversized_body_is_rejected() {
    struct Rejecting;

    impl Handler<Conn> for Rejecting {
        async fn handle(&mut self, request: &mut ServerRequest<'_, Conn>) -> ResponseData {
            match request.body_bytes(8).await {
                Ok(body) => ResponseData::ok(body),
                Err(_) => ResponseData::new(400, Vec::new(), b"too large".to_vec()),
            }
        }
    }

    let (client_side, server_side) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        serve_connection(TokioCompat::new(server_side), &mut Rejecting).await
    });

    let mut conn = ClientConnection::new(TokioCompat::new(client_side));
    let request = Request::new("POST", "/echo").body(vec![7u8; 4096]);
    let mut response = conn.send(&request, "localhost").await.unwrap();

    assert_eq!(response.status(), 400);
    let body = response.body_bytes(64).await.unwrap();
    assert_eq!(body, b"too large");

    drop(conn);
    server.await.unwrap().unwrap();
}
