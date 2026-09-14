// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP/1.1 request/response round-trip benchmark: the `tpt-async` stack
//! (server + client) against the same topology built on hyper 1.x, both over
//! tokio in-memory duplex pipes — so the comparison isolates the HTTP stack
//! from real networking.

use criterion::{criterion_group, criterion_main, Criterion};

use tpt_async_io::TokioCompat;
use tpt_net_http::prelude::*;

type Conn = TokioCompat<tokio::io::DuplexStream>;

const RESPONSE_BODY: &[u8] = b"benchmark response body";

// ── Our stack ────────────────────────────────────────────────────────────────

struct BenchHandler;

impl Handler<Conn> for BenchHandler {
    async fn handle(&mut self, _request: &mut ServerRequest<'_, Conn>) -> ResponseData {
        ResponseData::ok(RESPONSE_BODY.to_vec())
    }
}

fn bench_ours_roundtrip(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut group = c.benchmark_group("http1_roundtrip");
    group.throughput(criterion::Throughput::Bytes(RESPONSE_BODY.len() as u64));
    group.bench_function("tpt_net_http", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (client_io, server_io) = tokio::io::duplex(64 * 1024);
                let server = tokio::spawn(async move {
                    serve_connection(TokioCompat::new(server_io), &mut BenchHandler)
                        .await
                        .expect("server");
                });

                let mut conn = ClientConnection::new(TokioCompat::new(client_io));
                let mut response = conn
                    .send(&Request::new("GET", "/bench"), "bench")
                    .await
                    .expect("send");
                assert_eq!(response.status(), 200);
                let body = response.body_bytes(4096).await.expect("body");
                assert_eq!(body, RESPONSE_BODY);
                drop(conn);
                server.await.expect("server task");
            });
        })
    });
    group.finish();
}

// ── Hyper stack ──────────────────────────────────────────────────────────────

mod hyper_side {
    const RESPONSE_BODY: &[u8] = super::RESPONSE_BODY;

    async fn hyper_service(
        _req: hyper::Request<hyper::body::Incoming>,
    ) -> Result<hyper::Response<http_body_util::Full<&'static [u8]>>, hyper::Error> {
        Ok(hyper::Response::new(http_body_util::Full::new(
            RESPONSE_BODY,
        )))
    }

    pub async fn serve(socket: tokio::io::DuplexStream) {
        let io = hyper_util::rt::TokioIo::new(socket);
        let _ = hyper::server::conn::http1::Builder::new()
            .serve_connection(io, hyper::service::service_fn(hyper_service))
            .await;
    }

    pub async fn roundtrip(socket: tokio::io::DuplexStream) {
        let io = hyper_util::rt::TokioIo::new(socket);
        let (mut sender, connection) = hyper::client::conn::http1::handshake(io)
            .await
            .expect("hyper handshake");
        tokio::spawn(async move {
            let _ = connection.await;
        });

        let request = hyper::Request::builder()
            .method("GET")
            .uri("http://bench/bench")
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .expect("request");

        let response = sender.send_request(request).await.expect("send");
        assert_eq!(response.status(), 200);
        let body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("body")
            .to_bytes();
        assert_eq!(&body[..], RESPONSE_BODY);
    }
}

fn bench_hyper_roundtrip(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut group = c.benchmark_group("http1_roundtrip");
    group.throughput(criterion::Throughput::Bytes(RESPONSE_BODY.len() as u64));
    group.bench_function("hyper", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (client_io, server_io) = tokio::io::duplex(64 * 1024);
                let server = tokio::spawn(hyper_side::serve(server_io));
                hyper_side::roundtrip(client_io).await;
                server.await.expect("server task");
            });
        })
    });
    group.finish();
}

criterion_group!(benches, bench_ours_roundtrip, bench_hyper_roundtrip);
criterion_main!(benches);
