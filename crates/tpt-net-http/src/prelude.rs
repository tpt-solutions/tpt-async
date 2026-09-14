//! Convenience re-exports for the most common types in `tpt-net-http`.

pub use crate::client::{
    ClientConnection, Connector, HttpClient, OwnedResponse, Pool, Request, Response,
};
pub use crate::error::HttpError;
pub use crate::parse::HeaderBlock;
pub use crate::server::{headers, serve_connection, Handler, ResponseData, ServerRequest};
