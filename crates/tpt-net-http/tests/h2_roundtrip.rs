// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration test: HTTP/2 client ↔ HTTP/2 server over an in-memory
//! duplex, exercising the `h2`-backed server loop and client connection.

use tpt_net_http::http2::{H2Request, Http2Connection, Http2Handler};
use tpt_net_http::ResponseData;

struct Health;

impl Http2Handler for Health {
    async fn handle(&mut self, request: H2Request) -> ResponseData {
        assert_eq!(request.method, "GET");
        assert_eq!(request.target, "/health");
        ResponseData::ok(br#"{"status":"ok"}"#.to_vec())
    }
}

struct Echo;

impl Http2Handler for Echo {
    async fn handle(&mut self, request: H2Request) -> ResponseData {
        ResponseData::ok(request.body)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn h2_get_roundtrip() {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);

    let server = tokio::spawn(async move {
        tpt_net_http::http2::serve_h2(server_io, Health)
            .await
            .expect("serve_h2");
    });

    let mut conn = Http2Connection::connect(client_io)
        .await
        .expect("h2 connect");
    let response = conn
        .request("GET", "/health", &[], Vec::new(), 4096)
        .await
        .expect("h2 request");

    assert_eq!(response.status, 200);
    assert_eq!(response.body, br#"{"status":"ok"}"#.to_vec());

    drop(conn); // GOAWAY: the server's serve_h2 returns
    server.await.expect("server task");
}

#[tokio::test(flavor = "multi_thread")]
async fn h2_post_body_roundtrip() {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);

    let server = tokio::spawn(async move {
        tpt_net_http::http2::serve_h2(server_io, Echo)
            .await
            .expect("serve_h2");
    });

    let mut conn = Http2Connection::connect(client_io)
        .await
        .expect("h2 connect");
    let payload = b"h2 body round trip".to_vec();
    let response = conn
        .request("POST", "/echo", &[], payload.clone(), 4096)
        .await
        .expect("h2 request");

    assert_eq!(response.status, 200);
    assert_eq!(response.body, payload);

    drop(conn); // GOAWAY: the server's serve_h2 returns
    server.await.expect("server task");
}
