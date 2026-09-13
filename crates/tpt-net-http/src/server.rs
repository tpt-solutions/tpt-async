// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal HTTP/1.1 server: a keep-alive connection loop plus an
//! RPITIT-based handler trait (no `async-trait`, no heap dispatch cost).

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::future::Future;

use tpt_async_io::{AsyncRead, AsyncWrite};

use crate::body::BodyReader;
use crate::connection::HttpConnection;
use crate::error::HttpError;
use crate::parse::RequestHead;

/// A server-side view of one request.
///
/// Headers are zero-copy views into the connection's read buffer (kept alive
/// by an internal `Arc`), and the body streams off the connection with
/// keep-alive framing preserved.
pub struct ServerRequest<'c, IO> {
    head: RequestHead,
    /// Holds the exclusive borrow of the connection while the body streams.
    body: BodyReader<'c, IO>,
    keep_alive: bool,
}

impl<'c, IO: AsyncRead + Unpin> ServerRequest<'c, IO> {
    /// Request method, e.g. `GET`.
    pub fn method(&self) -> &[u8] {
        self.head.method()
    }

    /// Request target (path + query), e.g. `/health`.
    pub fn target(&self) -> &[u8] {
        self.head.target()
    }

    /// Header block (zero-copy views).
    pub fn headers(&self) -> &crate::parse::HeaderBlock {
        self.head.headers()
    }

    /// Value of a request header, if present.
    pub fn header(&self, name: &[u8]) -> Option<&[u8]> {
        self.head.headers().get(name)
    }

    /// Read the whole request body (bounded by `max` bytes).
    pub async fn body_bytes(&mut self, max: usize) -> Result<Vec<u8>, HttpError> {
        self.body.read_to_vec(max).await
    }

    /// Whether the connection may serve another request after this one.
    pub fn keep_alive(&self) -> bool {
        self.keep_alive
    }
}

/// A response built by a handler.
#[derive(Debug, Clone)]
pub struct ResponseData {
    /// Status code (e.g. 200).
    pub status: u16,
    /// Extra headers (`content-length` is added automatically).
    pub headers: Vec<(String, String)>,
    /// Body bytes.
    pub body: Vec<u8>,
}

impl ResponseData {
    /// A response with a body.
    pub fn new(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// `200 OK` with body bytes.
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self::new(200, Vec::new(), body.into())
    }

    /// `404 Not Found` with a plain-text body.
    pub fn not_found() -> Self {
        Self::new(404, Vec::new(), b"not found".to_vec())
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            _ => "",
        }
    }
}

/// An HTTP request handler.
///
/// Uses RPITIT (async fn in traits, stable since Rust 1.75) — zero heap
/// allocation from trait dispatch, per the workspace invariants.
pub trait Handler<IO> {
    /// Handle one request and produce a response.
    ///
    /// The returned future is `Send`, so connections can be served on any
    /// task of a multi-threaded runtime.
    fn handle(
        &mut self,
        request: &mut ServerRequest<'_, IO>,
    ) -> impl Future<Output = ResponseData> + Send
    where
        IO: AsyncRead + Send;
}

/// Serve requests on one connection until the peer closes.
///
/// The keep-alive loop: read head → build [`ServerRequest`] → dispatch →
/// write response → repeat.  Malformed requests receive a `400` and close
/// the connection.
pub async fn serve_connection<IO, H>(io: IO, handler: &mut H) -> Result<(), HttpError>
where
    IO: AsyncRead + AsyncWrite + Send + Unpin,
    H: Handler<IO>,
{
    let mut conn = HttpConnection::new(io);
    loop {
        let head = match conn.read_request_head().await? {
            Some(head) => head,
            None => return Ok(()), // clean close
        };

        let keep_alive = keep_alive_requested(&head);
        let body_kind = crate::body::request_body_kind(head.headers())?;
        let body = BodyReader::new(&mut conn, body_kind);

        let mut request = ServerRequest {
            head,
            body,
            keep_alive,
        };

        let response = handler.handle(&mut request).await;
        let keep_alive = request.keep_alive && request.body.is_finished();

        let head_str = crate::connection::render_head(
            response.status,
            response.reason(),
            &response.headers,
            !response.body.is_empty(),
            response.body.len(),
            String::new(),
        );

        // End the connection borrow held by the body reader (assign to `_`
        // so the lifetime is released before the response write), then write.
        let ServerRequest { body, .. } = request;
        let _released = body;
        conn.write_message(&head_str, &response.body).await?;

        if !keep_alive {
            return Ok(());
        }
    }
}

/// RFC 9112 §9.3: HTTP/1.1 defaults to keep-alive; `Connection: close` (or
/// HTTP/1.0 without `keep-alive`) ends the connection after one exchange.
fn keep_alive_requested(head: &RequestHead) -> bool {
    let connection = head.headers().get(b"connection");
    match head.version() {
        crate::parse::Version::Http11 => !connection
            .map(|v| v.eq_ignore_ascii_case(b"close"))
            .unwrap_or(false),
        crate::parse::Version::Http10 => connection
            .map(|v| v.eq_ignore_ascii_case(b"keep-alive"))
            .unwrap_or(false),
    }
}

/// Convenience: build a header vector from pairs.
pub fn headers<const N: usize>(pairs: [(&str, &str); N]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
        .collect()
}
