// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration test: exact-match routing over a real client/server pair.

use tpt_async_io::TokioCompat;
use tpt_net_http::prelude::*;

type Conn = TokioCompat<tokio::io::DuplexStream>;

fn app() -> Router<Conn> {
    Router::new()
        .get("/health", |_req| {
            Box::pin(async { ResponseData::ok(b"ok".to_vec()) })
        })
        .post("/echo", |req| {
            Box::pin(async move {
                let body = req.body_bytes(64 * 1024).await.unwrap();
                ResponseData::ok(body)
            })
        })
}

/// Serve `router` on one end of a duplex pipe and return a client for the other.
async fn serve(
    router: Router<Conn>,
) -> (
    ClientConnection<Conn>,
    tokio::task::JoinHandle<Result<(), HttpError>>,
) {
    let (client_side, server_side) = tokio::io::duplex(64 * 1024);
    let mut router = router;
    let server =
        tokio::spawn(
            async move { serve_connection(TokioCompat::new(server_side), &mut router).await },
        );
    (ClientConnection::new(TokioCompat::new(client_side)), server)
}

#[tokio::test]
async fn matches_method_and_path_and_ignores_query() {
    let (mut conn, server) = serve(app()).await;

    let mut response = conn
        .send(&Request::new("GET", "/health?verbose=1"), "localhost")
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.body_bytes(64).await.unwrap(), b"ok");

    let request = Request::new("POST", "/echo").body(b"routed".to_vec());
    let mut response = conn.send(&request, "localhost").await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.body_bytes(64).await.unwrap(), b"routed");

    drop(conn);
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn wrong_method_on_known_path_is_405() {
    let (mut conn, server) = serve(app()).await;

    let response = conn
        .send(&Request::new("POST", "/health"), "localhost")
        .await
        .unwrap();
    assert_eq!(response.status(), 405);

    drop(conn);
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn unknown_path_defaults_to_404() {
    let (mut conn, server) = serve(app()).await;

    let response = conn
        .send(&Request::new("GET", "/missing"), "localhost")
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    drop(conn);
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn custom_fallback_replaces_404() {
    let router = app()
        .fallback(|_req| Box::pin(async { ResponseData::new(410, Vec::new(), b"gone".to_vec()) }));
    let (mut conn, server) = serve(router).await;

    let mut response = conn
        .send(&Request::new("GET", "/missing"), "localhost")
        .await
        .unwrap();
    assert_eq!(response.status(), 410);
    assert_eq!(response.body_bytes(64).await.unwrap(), b"gone");

    drop(conn);
    server.await.unwrap().unwrap();
}
