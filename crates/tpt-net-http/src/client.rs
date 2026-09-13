// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal HTTP/1.1 client.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tpt_async_io::{AsyncRead, AsyncWrite};

use crate::body::BodyReader;
use crate::connection::HttpConnection;
use crate::error::HttpError;
use crate::parse::ResponseHead;

/// An outgoing request.
#[derive(Debug, Clone)]
pub struct Request {
    /// Method, e.g. `GET`.
    pub method: String,
    /// Path + query, e.g. `/health?verbose`.
    pub target: String,
    /// Extra headers (`host` and `content-length` are added automatically).
    pub headers: Vec<(String, String)>,
    /// Body bytes (empty for most GETs).
    pub body: Vec<u8>,
}

impl Request {
    /// Build a request.
    pub fn new(method: &str, target: &str) -> Self {
        Self {
            method: method.to_string(),
            target: target.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Add a header (keeps insertion order; duplicates allowed).
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Set the body (adds `content-length` automatically when sent).
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

/// A response: frozen head (zero-copy) plus a streaming body reader.
pub struct Response<'c, IO> {
    head: ResponseHead,
    body: BodyReader<'c, IO>,
}

impl<'c, IO: AsyncRead + Unpin> Response<'c, IO> {
    /// Status code.
    pub fn status(&self) -> u16 {
        self.head.status()
    }

    /// Reason phrase.
    pub fn reason(&self) -> &[u8] {
        self.head.reason()
    }

    /// Header block (zero-copy views into the connection buffer).
    pub fn headers(&self) -> &crate::parse::HeaderBlock {
        self.head.headers()
    }

    /// Value of a response header, if present.
    pub fn header(&self, name: &[u8]) -> Option<&[u8]> {
        self.head.headers().get(name)
    }

    /// Read the whole body (bounded by `max` bytes).
    pub async fn body_bytes(&mut self, max: usize) -> Result<Vec<u8>, HttpError> {
        self.body.read_to_vec(max).await
    }
}

/// A client-side HTTP/1.1 connection over an established transport.
pub struct ClientConnection<IO> {
    conn: HttpConnection<IO>,
}

impl<IO: AsyncRead + AsyncWrite + Unpin> ClientConnection<IO> {
    /// Wrap an already-connected transport (TCP, TLS, in-memory, …).
    pub fn new(io: IO) -> Self {
        Self {
            conn: HttpConnection::new(io),
        }
    }

    /// Send one request and read the response head.
    ///
    /// The body streams lazily through [`Response`]; drain it (or drop the
    /// response) before sending the next request on this connection.
    pub async fn send(
        &mut self,
        request: &Request,
        host: &str,
    ) -> Result<Response<'_, IO>, HttpError> {
        let mut head = format!("{} {} HTTP/1.1\r\n", request.method, request.target);
        let mut has_host = false;
        for (name, value) in &request.headers {
            if name.eq_ignore_ascii_case("host") {
                has_host = true;
            }
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        if !has_host {
            head.push_str("host: ");
            head.push_str(host);
            head.push_str("\r\n");
        }
        head.push_str("content-length: ");
        head.push_str(&request.body.len().to_string());
        head.push_str("\r\n\r\n");

        self.conn.write_message(&head, &request.body).await?;

        let response_head = self.conn.read_response_head().await?;
        let is_head = request.method.eq_ignore_ascii_case("HEAD");
        let body = self.conn.response_body(&response_head, is_head)?;
        Ok(Response {
            head: response_head,
            body,
        })
    }
}

/// An HTTP client with optional TLS.
///
/// Transports are supplied by the caller (the `tpt-async-io` traits are
/// runtime-agnostic on purpose): wrap your TCP socket / TLS stream /
/// in-memory pipe in [`ClientConnection`] and send requests.  For TLS,
/// [`HttpClient::connect_tls`] upgrades any transport using the bundled
/// `tpt-net-tls` connector.
#[derive(Clone)]
pub struct HttpClient {
    tls: Option<tpt_net_tls::TlsConnector>,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    /// A client that speaks plain HTTP over whatever transport it is given.
    pub fn new() -> Self {
        Self { tls: None }
    }

    /// A client that upgrades outgoing connections with TLS (rustls, the
    /// default feature-backed root store of `tpt-net-tls`).
    pub fn with_tls(tls: tpt_net_tls::TlsConnector) -> Self {
        Self { tls: Some(tls) }
    }

    /// Upgrade an established transport to TLS, verifying `server_name`.
    ///
    /// # Panics
    /// Panics if the client was not built with
    /// [`HttpClient::with_tls`].
    pub async fn connect_tls<IO>(
        &self,
        io: IO,
        server_name: rustls::pki_types::ServerName<'static>,
    ) -> Result<ClientConnection<tpt_net_tls::TlsStream<IO>>, HttpError>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        let tls = self
            .tls
            .as_ref()
            .expect("connect_tls: client built without with_tls()");
        let stream = tls.connect(server_name, io).await.map_err(|e| {
            HttpError::Io(tpt_async_io::IoError::from(std::io::Error::other(
                e.to_string(),
            )))
        })?;
        Ok(ClientConnection::new(stream))
    }
}
