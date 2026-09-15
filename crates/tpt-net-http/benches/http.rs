// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP/1.1 request/response round-trip benchmark: the `tpt-async` stack
//! (server + client) against the same topology built on hyper 1.x, both over
//! tokio in-memory duplex pipes — so the comparison isolates the HTTP stack
//! from real networking.

use criterion::{criterion_group, criterion_main, Criterion};

use std::io::Read as _;
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

type TcpConn = TokioCompat<tokio::net::TcpStream>;

impl Handler<TcpConn> for BenchHandler {
    async fn handle(&mut self, _request: &mut ServerRequest<'_, TcpConn>) -> ResponseData {
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

criterion_group!(
    benches,
    bench_ours_roundtrip,
    bench_ours_tcp_roundtrip,
    bench_ureq_tcp_roundtrip,
    bench_hyper_roundtrip
);
criterion_main!(benches);

// ── ureq over real loopback TCP ──────────────────────────────────────────────
//
// ureq is a blocking std client, so the fair comparison runs both stacks
// through real loopback TCP sockets (our server via the tokio driver, ureq
// as a blocking caller on the criterion thread).

fn bench_ureq_tcp_roundtrip(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    // Bind a real loopback listener; serve accepted connections with our
    // HTTP/1.1 server for the lifetime of the benchmark.
    let listener = rt.block_on(async {
        tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind")
    });
    let addr = listener.local_addr().expect("addr");

    rt.spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            socket.set_nodelay(true).ok();
            tokio::spawn(async move {
                let _ = serve_connection(TokioCompat::new(socket), &mut BenchHandler).await;
            });
        }
    });

    let mut group = c.benchmark_group("http1_roundtrip");
    group.throughput(criterion::Throughput::Bytes(RESPONSE_BODY.len() as u64));
    group.bench_function("ureq_loopback_tcp", |b| {
        b.iter(|| {
            let response = ureq::get(&format!("http://{addr}/bench"))
                .call()
                .expect("ureq request");
            assert_eq!(response.status(), 200);
            let mut body = Vec::new();
            response
                .into_reader()
                .read_to_end(&mut body)
                .expect("ureq body");
            assert_eq!(body, RESPONSE_BODY);
        })
    });
    group.finish();
}

// ureq's round-trip includes a fresh TCP connection per request, so also
// measure our stack the same way (fresh duplex + fresh connection per iter
// is already the case in bench_ours_roundtrip; here fresh TCP for parity).
fn bench_ours_tcp_roundtrip(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let listener = rt.block_on(async {
        tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind")
    });
    let addr = listener.local_addr().expect("addr");

    rt.spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            socket.set_nodelay(true).ok();
            tokio::spawn(async move {
                let _ = serve_connection(TokioCompat::new(socket), &mut BenchHandler).await;
            });
        }
    });

    let mut group = c.benchmark_group("http1_roundtrip");
    group.throughput(criterion::Throughput::Bytes(RESPONSE_BODY.len() as u64));
    group.bench_function("tpt_net_http_loopback_tcp", |b| {
        b.iter(|| {
            rt.block_on(async {
                let socket = tokio::net::TcpStream::connect(addr).await.expect("connect");
                socket.set_nodelay(true).ok();
                let mut conn = ClientConnection::new(TokioCompat::new(socket));
                let mut response = conn
                    .send(&Request::new("GET", "/bench"), "bench")
                    .await
                    .expect("send");
                assert_eq!(response.status(), 200);
                let body = response.body_bytes(4096).await.expect("body");
                assert_eq!(body, RESPONSE_BODY);
                drop(conn);
            });
        })
    });
    group.finish();
}
