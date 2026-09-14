// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal HTTP/1.1 client.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::future::Future;

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

    /// Unwrap the underlying transport (after a completed exchange).
    pub fn into_inner(self) -> IO {
        self.conn.into_inner()
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

// ── Pooling + redirects ───────────────────────────────────────────────────────

/// Establishes transports for a [`Pool`].
///
/// Implement for your transport (tokio TCP + TLS, embassy, in-memory test
/// doubles…).  `authority` is the `host[:port]` string the request targets.
pub trait Connector {
    /// The transport this connector produces.
    type Conn: AsyncRead + AsyncWrite + Unpin;

    /// Connect to `authority` ("host" or "host:port").
    fn connect(
        &self,
        authority: &str,
    ) -> impl Future<Output = Result<Self::Conn, HttpError>> + Send;
}

/// A bounded, per-host connection pool.
///
/// Connections come back to the pool when a request/response exchange
/// finished with the body fully drained and neither side signalled
/// `Connection: close`.  Idle connections beyond `max_idle_per_host` are
/// dropped (the transport close happens on drop).
pub struct Pool<C: Connector> {
    connector: C,
    idle: std::sync::Mutex<std::collections::HashMap<String, Vec<C::Conn>>>,
    max_idle_per_host: usize,
}

impl<C: Connector> Pool<C> {
    /// Create a pool using `connector`, keeping at most `max_idle_per_host`
    /// idle connections per host.
    pub fn new(connector: C, max_idle_per_host: usize) -> Self {
        Self {
            connector,
            idle: std::sync::Mutex::new(std::collections::HashMap::new()),
            max_idle_per_host: max_idle_per_host.max(1),
        }
    }

    fn take(&self, authority: &str) -> Option<C::Conn> {
        let mut idle = self.idle.lock().expect("pool poisoned");
        idle.get_mut(authority)?.pop()
    }

    fn put(&self, authority: &str, conn: C::Conn) {
        let mut idle = self.idle.lock().expect("pool poisoned");
        let slot = idle.entry(authority.to_string()).or_default();
        if slot.len() < self.max_idle_per_host {
            slot.push(conn);
        }
        // else: dropped → transport closed
    }
}

/// An owned response for pooled/redirected requests: everything is cloned
/// out of the zero-copy head so the connection can go back to the pool.
#[derive(Debug, Clone)]
pub struct OwnedResponse {
    /// Status code.
    pub status: u16,
    /// Response headers (name, value), lowercased name on the wire.
    pub headers: Vec<(String, String)>,
    /// Fully-drained body bytes.
    pub body: Vec<u8>,
}

impl HttpClient {
    /// Send `request` through `pool` to `authority`, following up to
    /// `max_redirects` 3xx responses whose `location` header points at a
    /// new target (relative paths reuse the current authority).
    ///
    /// Returns [`OwnedResponse`]; the connection returns to the pool when
    /// the exchange was keep-alive clean.
    pub async fn request<C: Connector>(
        &self,
        pool: &Pool<C>,
        request: &Request,
        max_redirects: usize,
    ) -> Result<OwnedResponse, HttpError> {
        let mut authority = String::new();
        let mut req = request.clone();
        let mut hops = 0usize;

        loop {
            if authority.is_empty() {
                authority = default_authority_for(&req)?;
            }

            let conn = match pool.take(&authority) {
                Some(conn) => conn,
                None => pool.connector.connect(&authority).await?,
            };

            let mut client = ClientConnection::new(conn);
            let mut response = client.send(&req, host_of(&authority)).await?;
            let status = response.status();

            // Redirect?
            if (300..400).contains(&status) && status != 304 {
                if hops >= max_redirects {
                    return Err(HttpError::Parse("too many redirects"));
                }
                let location = response
                    .header(b"location")
                    .map(|v| String::from_utf8_lossy(v).into_owned())
                    .ok_or(HttpError::Parse("redirect without location"))?;
                // Drain whatever body accompanies the redirect so the
                // connection stays framed (best effort for pooled reuse).
                let _ = response.body_bytes(1024 * 1024).await;
                hops += 1;
                let (new_authority, new_target) =
                    resolve_redirect(&authority, &req.target, &location);
                authority = new_authority;
                req.target = new_target;
                req.body.clear();
                continue;
            }

            let body = response.body_bytes(MAX_POOLED_BODY).await?;
            let keep_alive = keep_alive_from(response.headers());
            let headers = response
                .headers()
                .iter()
                .map(|(n, v)| {
                    (
                        String::from_utf8_lossy(n).into_owned(),
                        String::from_utf8_lossy(v).into_owned(),
                    )
                })
                .collect();

            drop(response);

            let conn = client.into_inner();
            if keep_alive {
                pool.put(&authority, conn);
            }

            return Ok(OwnedResponse {
                status,
                headers,
                body,
            });
        }
    }
}

/// Upper bound for pooled-request bodies (64 MiB).
const MAX_POOLED_BODY: usize = 64 * 1024 * 1024;

fn default_authority_for(request: &Request) -> Result<String, HttpError> {
    Ok(request
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("host"))
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "localhost".to_string()))
}

fn host_of(authority: &str) -> &str {
    authority.split(':').next().unwrap_or(authority)
}

fn keep_alive_from(headers: &crate::parse::HeaderBlock) -> bool {
    match headers.get(b"connection") {
        Some(v) => !v.eq_ignore_ascii_case(b"close"),
        None => true, // HTTP/1.1 default
    }
}

/// Resolve a `Location` header to (authority, target).
fn resolve_redirect(
    current_authority: &str,
    current_target: &str,
    location: &str,
) -> (String, String) {
    if let Some(rest) = location.strip_prefix("http://") {
        let (auth, path) = split_authority_path(rest);
        return (auth.to_string(), path.to_string());
    }
    if location.starts_with("https://") {
        // TLS pooling needs a TLS-aware connector; treat as an authority the
        // connector must understand (the Connector decides the scheme).
        let rest = location.strip_prefix("https://").unwrap();
        let (auth, path) = split_authority_path(rest);
        return (
            format!("{}:443", auth.split(':').next().unwrap_or(auth)),
            path.to_string(),
        );
    }
    if location.starts_with('/') {
        return (current_authority.to_string(), location.to_string());
    }
    // Relative path: resolve against the current target's directory.
    let base = current_target
        .rsplit_once('/')
        .map(|(d, _)| d)
        .unwrap_or("");
    (
        current_authority.to_string(),
        alloc::format!("{base}/{location}"),
    )
}

fn split_authority_path(rest: &str) -> (&str, &str) {
    match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, "/"),
    }
}
