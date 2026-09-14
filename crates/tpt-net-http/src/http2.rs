// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP/2 support via the [`h2`] crate (feature **`http2`**).
//!
//! The server adapts HTTP/2 streams onto an owned-request handler; the
//! client speaks to any HTTP/2 server through [`Http2Connection`].  Both
//! sides run over any tokio-compatible transport (the h2 crate requires
//! tokio's I/O traits — on microcontrollers use HTTP/1.1 over our own
//! traits instead).
//!
//! # Example
//!
//! ```rust,no_run
//! use tpt_net_http::http2::{Http2Connection, Http2Handler, H2Request};
//! use tpt_net_http::ResponseData;
//! use tpt_async_io::TokioCompat;
//!
//! struct Hello;
//! impl Http2Handler for Hello {
//!     async fn handle(&mut self, req: H2Request) -> ResponseData {
//!         ResponseData::ok(b"hello h2".to_vec())
//!     }
//! }
//!
//! # async fn demo(io: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static) {
//! // Server side:
//! tpt_net_http::http2::serve_h2(io, Hello).await.expect("serve");
//!
//! // Client side (another transport):
//! // let mut conn = Http2Connection::connect(io2).await?;
//! // let resp = conn.request(&tpt_net_http::Request::new("GET", "/")).await?;
//! # }
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::future::Future;

use bytes::Bytes;
use http::Request;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::HttpError;
use crate::server::ResponseData;

/// An owned HTTP/2 request view: method, target, headers and fully-drained
/// body.
#[derive(Debug, Clone)]
pub struct H2Request {
    /// Method, e.g. `GET`.
    pub method: String,
    /// Path + query, e.g. `/health`.
    pub target: String,
    /// Header pairs as received.
    pub headers: Vec<(String, String)>,
    /// Fully-drained request body.
    pub body: Vec<u8>,
}

/// An HTTP/2 request handler.
pub trait Http2Handler: Send + 'static {
    /// Handle one request; the returned future must be `Send` so concurrent
    /// HTTP/2 streams can be serviced on a multi-threaded runtime.
    fn handle(&mut self, request: H2Request) -> impl Future<Output = ResponseData> + Send;
}

/// Serve HTTP/2 on `io` until the client disconnects.
///
/// Each stream is handled on its own tokio task; the handler is shared via
/// `Arc` + `Mutex` (the trait takes `&mut self`).
pub async fn serve_h2<IO, H>(io: IO, handler: H) -> Result<(), HttpError>
where
    IO: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    H: Http2Handler,
{
    let handler = alloc::sync::Arc::new(tokio::sync::Mutex::new(handler));
    let mut conn = h2::server::handshake(io).await.map_err(HttpError::from)?;

    while let Some((http_req, mut respond)) =
        conn.accept().await.transpose().map_err(HttpError::from)?
    {
        let handler = alloc::sync::Arc::clone(&handler);
        tokio::spawn(async move {
            let method = http_req.method().to_string();
            let target = http_req.uri().path().to_string();
            let headers: Vec<(String, String)> = http_req
                .headers()
                .iter()
                .map(|(n, v)| (n.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            let (parts, mut body) = http_req.into_parts();
            let _ = parts;
            let mut body_bytes = Vec::new();
            while let Some(chunk) = body.data().await {
                match chunk {
                    Ok(bytes) => {
                        let _ = body.flow_control().release_capacity(bytes.len());
                        body_bytes.extend_from_slice(&bytes);
                    }
                    Err(_) => break,
                }
            }

            let request = H2Request {
                method,
                target,
                headers,
                body: body_bytes,
            };

            let response = {
                let mut h = handler.lock().await;
                h.handle(request).await
            };

            let http_response = response_data_to_http(&response);
            let response_size = response.body.len();
            if let Ok(mut send) = respond.send_response(http_response, false) {
                if response_size > 0 {
                    let _ = send.send_data(Bytes::from(response.body), true);
                } else {
                    let _ = send.send_data(Bytes::new(), true);
                }
            }
        });
    }
    Ok(())
}

fn response_data_to_http(data: &ResponseData) -> http::Response<()> {
    let mut builder = http::Response::builder().status(data.status);
    for (name, value) in &data.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder.body(()).expect("valid response")
}

/// An HTTP/2 client connection over an established transport.
pub struct Http2Connection<IO> {
    send_request: h2::client::SendRequest<Bytes>,
    _io_guard: std::marker::PhantomData<fn() -> IO>,
}

impl<IO> Http2Connection<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Perform the HTTP/2 preface + settings handshake over `io`.
    ///
    /// The connection's read half is spawned onto the current tokio runtime;
    /// call this from within a tokio context.
    ///
    /// # Panics
    /// Panics when called outside a tokio runtime (h2 needs one to drive the
    /// connection).
    pub async fn connect(io: IO) -> Result<Self, HttpError> {
        let (send_request, connection) =
            h2::client::handshake(io).await.map_err(HttpError::from)?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(Self {
            send_request,
            _io_guard: std::marker::PhantomData,
        })
    }

    /// Send one request and collect the full response body (bounded by
    /// `max_body` bytes).
    pub async fn request(
        &mut self,
        method: &str,
        target: &str,
        headers: &[(String, String)],
        body: Vec<u8>,
        max_body: usize,
    ) -> Result<OwnedH2Response, HttpError> {
        futures_poll_ready(&mut self.send_request).await?;

        let uri = http::Uri::builder()
            .path_and_query(target)
            .build()
            .map_err(|_e| HttpError::Parse("invalid request target"))?;

        let mut builder = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let request = builder
            .body(())
            .map_err(|_e| HttpError::Parse("invalid request"))?;

        let (response, mut send_body) = self
            .send_request
            .send_request(request, false)
            .map_err(HttpError::from)?;

        if !body.is_empty() {
            send_body
                .send_data(Bytes::from(body), true)
                .map_err(HttpError::from)?;
        } else {
            send_body
                .send_data(Bytes::new(), true)
                .map_err(HttpError::from)?;
        }

        let response = response.await.map_err(HttpError::from)?;
        let status = response.status().as_u16();
        let response_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(n, v)| (n.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let mut h2_body = response.into_body();
        let mut out = Vec::new();
        while let Some(chunk) = h2_body.data().await {
            let chunk = chunk.map_err(HttpError::from)?;
            if out.len() + chunk.len() > max_body {
                return Err(HttpError::Parse("body exceeds limit"));
            }
            let _ = h2_body.flow_control().release_capacity(chunk.len());
            out.extend_from_slice(&chunk);
        }

        Ok(OwnedH2Response {
            status,
            headers: response_headers,
            body: out,
        })
    }
}

/// An owned HTTP/2 response.
#[derive(Debug, Clone)]
pub struct OwnedH2Response {
    /// Status code.
    pub status: u16,
    /// Header pairs.
    pub headers: Vec<(String, String)>,
    /// Fully-drained body bytes.
    pub body: Vec<u8>,
}

/// Poll `SendRequest::poll_ready` as an async fn.
async fn futures_poll_ready(sr: &mut h2::client::SendRequest<Bytes>) -> Result<(), HttpError> {
    std::future::poll_fn(|cx| h2::client::SendRequest::poll_ready(sr, cx))
        .await
        .map_err(HttpError::from)
}
