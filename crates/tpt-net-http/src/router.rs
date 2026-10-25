// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Exact-match `(method, path)` routing for the HTTP/1.1 server.
//!
//! A [`Router`] is itself a [`Handler`], so it plugs straight into
//! [`serve_connection`](crate::server::serve_connection). Routes are stored as
//! boxed closures: the RPITIT [`Handler`] trait is not object-safe, so the
//! router boxes each route's future instead. The cost is one allocation per
//! request, paid only by users who choose the router.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;

use tpt_async_io::AsyncRead;

use crate::server::{Handler, ResponseData, ServerRequest};

/// A boxed, `Send` future borrowing from one request.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Type-erased route handler.
type RouteFn<IO> = Box<
    dyn for<'a, 'c> Fn(&'a mut ServerRequest<'c, IO>) -> BoxFuture<'a, ResponseData> + Send + Sync,
>;

struct Route<IO> {
    method: &'static [u8],
    path: String,
    handler: RouteFn<IO>,
}

/// Routes requests by exact method and path.
///
/// The query string is ignored when matching. A path that matches a route
/// under a different method gets `405 Method Not Allowed`; any other unmatched
/// request goes to the fallback, which is `404 Not Found` unless replaced with
/// [`Router::fallback`].
///
/// # Example
///
/// ```rust,no_run
/// use tpt_net_http::router::Router;
/// use tpt_net_http::ResponseData;
///
/// let router = Router::<tpt_async_io::TokioCompat<tokio::net::TcpStream>>::new()
///     .get("/health", |_req| Box::pin(async { ResponseData::ok("ok") }));
/// # drop(router);
/// ```
pub struct Router<IO> {
    routes: Vec<Route<IO>>,
    fallback: RouteFn<IO>,
}

impl<IO: AsyncRead + Send> Default for Router<IO> {
    fn default() -> Self {
        Self::new()
    }
}

impl<IO: AsyncRead + Send> Router<IO> {
    /// An empty router. Unmatched requests get `404 Not Found`.
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            fallback: Box::new(|_| Box::pin(async { ResponseData::not_found() })),
        }
    }

    /// Register a handler for `method` and an exact `path`.
    pub fn route<F>(mut self, method: &'static str, path: &str, handler: F) -> Self
    where
        F: for<'a, 'c> Fn(&'a mut ServerRequest<'c, IO>) -> BoxFuture<'a, ResponseData>
            + Send
            + Sync
            + 'static,
    {
        self.routes.push(Route {
            method: method.as_bytes(),
            path: String::from(path),
            handler: Box::new(handler),
        });
        self
    }

    /// Register a `GET` handler for `path`.
    pub fn get<F>(self, path: &str, handler: F) -> Self
    where
        F: for<'a, 'c> Fn(&'a mut ServerRequest<'c, IO>) -> BoxFuture<'a, ResponseData>
            + Send
            + Sync
            + 'static,
    {
        self.route("GET", path, handler)
    }

    /// Register a `POST` handler for `path`.
    pub fn post<F>(self, path: &str, handler: F) -> Self
    where
        F: for<'a, 'c> Fn(&'a mut ServerRequest<'c, IO>) -> BoxFuture<'a, ResponseData>
            + Send
            + Sync
            + 'static,
    {
        self.route("POST", path, handler)
    }

    /// Replace the handler used when no route matches.
    pub fn fallback<F>(mut self, handler: F) -> Self
    where
        F: for<'a, 'c> Fn(&'a mut ServerRequest<'c, IO>) -> BoxFuture<'a, ResponseData>
            + Send
            + Sync
            + 'static,
    {
        self.fallback = Box::new(handler);
        self
    }
}

/// Path portion of a request target, without the query string.
fn path_only(target: &[u8]) -> &[u8] {
    match target.iter().position(|&b| b == b'?') {
        Some(end) => &target[..end],
        None => target,
    }
}

impl<IO: AsyncRead + Send + Unpin> Handler<IO> for Router<IO> {
    fn handle(
        &mut self,
        request: &mut ServerRequest<'_, IO>,
    ) -> impl Future<Output = ResponseData> + Send
    where
        IO: AsyncRead + Send,
    {
        let path = path_only(request.target()).to_vec();
        let method = request.method().to_vec();

        let mut path_matched = false;
        let mut selected = None;
        for route in &self.routes {
            if route.path.as_bytes() != path.as_slice() {
                continue;
            }
            path_matched = true;
            if route.method == method.as_slice() {
                selected = Some(route);
                break;
            }
        }

        let fallback = &self.fallback;
        async move {
            match selected {
                Some(route) => (route.handler)(request).await,
                None if path_matched => {
                    ResponseData::new(405, Vec::new(), b"method not allowed".to_vec())
                }
                None => (fallback)(request).await,
            }
        }
    }
}
