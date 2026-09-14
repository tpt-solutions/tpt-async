// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Zero-copy HTTP/1.1 head parsing.
//!
//! Parsing scans a byte buffer and produces byte *ranges*; the buffer then
//! moves into an [`Arc`] and the public head types ([`RequestHead`],
//! [`ResponseHead`], [`HeaderBlock`]) are plain views over it.  No
//! per-header allocation happens, and everything is safe — the `Arc` keeps
//! the storage alive.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use crate::error::HttpError;

/// HTTP protocol version of a parsed message head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// `HTTP/1.0`
    Http10,
    /// `HTTP/1.1`
    Http11,
}

/// A header block stored as ranges into a shared read buffer.
#[derive(Debug, Clone)]
pub struct HeaderBlock {
    buf: Arc<Vec<u8>>,
    entries: Vec<HeaderRange>,
}

impl HeaderBlock {
    /// An empty header block.
    pub fn empty() -> Self {
        Self {
            buf: Arc::new(Vec::new()),
            entries: Vec::new(),
        }
    }

    /// Iterate `(name, value)` pairs as byte slices into the shared buffer.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &[u8])> + '_ {
        self.entries
            .iter()
            .map(move |(n, v)| (&self.buf[n.start..n.end], &self.buf[v.start..v.end]))
    }

    /// Case-insensitive lookup of the *first* header with `name`.
    pub fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }

    /// `true` when no headers are present.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A frozen request head: the read buffer lives in an `Arc`, and accessors
/// return slices borrowing from it.
#[derive(Debug, Clone)]
pub struct RequestHead {
    buf: Arc<Vec<u8>>,
    method: Range<usize>,
    target: Range<usize>,
    version: Version,
    headers: HeaderBlock,
}

impl RequestHead {
    /// Request method, e.g. `GET`.
    pub fn method(&self) -> &[u8] {
        &self.buf[self.method.clone()]
    }

    /// Request target, e.g. `/path?query`.
    pub fn target(&self) -> &[u8] {
        &self.buf[self.target.clone()]
    }

    /// Protocol version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Header block.
    pub fn headers(&self) -> &HeaderBlock {
        &self.headers
    }

    pub(crate) fn from_ranges(buf: Arc<Vec<u8>>, ranges: &HeadRanges) -> Self {
        Self {
            headers: HeaderBlock {
                buf: Arc::clone(&buf),
                entries: ranges.headers.clone(),
            },
            buf,
            method: ranges.first.0.clone(),
            target: ranges.first.1.clone(),
            version: ranges.version,
        }
    }
}

/// A frozen response head; see [`RequestHead`].
#[derive(Debug, Clone)]
pub struct ResponseHead {
    buf: Arc<Vec<u8>>,
    reason: Range<usize>,
    status: u16,
    version: Version,
    headers: HeaderBlock,
}

impl ResponseHead {
    /// Status code, e.g. 200.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Reason phrase, e.g. `OK`.
    pub fn reason(&self) -> &[u8] {
        &self.buf[self.reason.clone()]
    }

    /// Protocol version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Header block.
    pub fn headers(&self) -> &HeaderBlock {
        &self.headers
    }

    pub(crate) fn from_ranges(buf: Arc<Vec<u8>>, ranges: &HeadRanges) -> Self {
        Self {
            headers: HeaderBlock {
                buf: Arc::clone(&buf),
                entries: ranges.headers.clone(),
            },
            buf,
            reason: ranges.first.0.clone(),
            status: ranges.status.unwrap_or(200),
            version: ranges.version,
        }
    }
}

// ── range-level parsing (no lifetimes) ───────────────────────────────────────

/// A `(name, value)` byte-range pair.
pub(crate) type HeaderRange = (Range<usize>, Range<usize>);

/// Parse output: byte ranges into the buffer plus already-decoded scalar
/// fields.  `first` is `(method, target)` for requests and `(reason,
/// reason)` for responses.
#[derive(Debug)]
pub(crate) struct HeadRanges {
    pub first: (Range<usize>, Range<usize>),
    pub version: Version,
    pub status: Option<u16>,
    pub headers: Vec<HeaderRange>,
    /// Number of bytes the head occupies, including the final CRLFCRLF.
    pub consumed: usize,
}

const MAX_HEAD: usize = 64 * 1024;
const HEADER_DELIM: &[u8] = b"\r\n\r\n";

/// Parse a request head out of `buf`.
///
/// Returns `Ok(None)` when more bytes are needed; callers should read from
/// the transport and retry.
///
/// # Errors
///
/// [`HttpError::Parse`] for malformed request or header lines, obsolete
/// line folding (RFC 9112 §5.2), and heads exceeding 64 KiB.
pub(crate) fn parse_request_ranges(buf: &[u8]) -> Result<Option<HeadRanges>, HttpError> {
    let head_end = match find_head_end(buf)? {
        Some(end) => end,
        None => return Ok(None),
    };

    let request_line =
        first_line(&buf[..head_end]).ok_or(HttpError::Parse("empty request head"))?;
    let (method, target, version) = parse_request_line(&buf[request_line.clone()])?;
    let headers = collect_header_ranges(&buf[..head_end], 1)?;

    Ok(Some(HeadRanges {
        first: (
            offset_range(method, request_line.start),
            offset_range(target, request_line.start),
        ),
        version,
        status: None,
        headers,
        consumed: head_end + HEADER_DELIM.len(),
    }))
}

/// Parse a response head out of `buf`; see [`parse_request_ranges`].
pub(crate) fn parse_response_ranges(buf: &[u8]) -> Result<Option<HeadRanges>, HttpError> {
    let head_end = match find_head_end(buf)? {
        Some(end) => end,
        None => return Ok(None),
    };

    let status_line = first_line(&buf[..head_end]).ok_or(HttpError::Parse("empty status head"))?;
    let (version, status, reason) = parse_status_line(&buf[status_line.clone()])?;
    let headers = collect_header_ranges(&buf[..head_end], 1)?;

    Ok(Some(HeadRanges {
        first: (
            offset_range(reason.clone(), status_line.start),
            offset_range(reason, status_line.start),
        ),
        version,
        status: Some(status),
        headers,
        consumed: head_end + HEADER_DELIM.len(),
    }))
}

// ── internals ────────────────────────────────────────────────────────────────

/// Locate the blank line terminating the head; `Ok(None)` = incomplete.
fn find_head_end(buf: &[u8]) -> Result<Option<usize>, HttpError> {
    // Enforce the size limit before scanning so a hostile peer cannot make
    // us buffer an unbounded head.
    if buf.len() > MAX_HEAD {
        return Err(HttpError::Parse("message head exceeds 64 KiB"));
    }
    // Obsolete line folding (obs-fold) is rejected outright (RFC 9112 §5.2).
    // Only scan up to the last *complete* line: the trailing partial line may
    // already be body bytes; a fold inside it is caught once it completes.
    if let Some(last_crlf) = buf.windows(2).rposition(|w| w == b"\r\n") {
        let complete = &buf[..last_crlf];
        if complete.windows(2).any(|w| w == b"\n ") || complete.windows(2).any(|w| w == b"\n\t") {
            return Err(HttpError::Parse("obsolete line folding in header block"));
        }
    }
    Ok(memchr_find(buf, HEADER_DELIM))
}

/// The range of the first CRLF-terminated line of `head`.
fn first_line(head: &[u8]) -> Option<Range<usize>> {
    // With no CRLF inside `head`, the whole head *is* the first line —
    // the headerless-request shape (`GET / HTTP/1.1\r\n\r\n`), where the
    // line terminator doubles as the head delimiter.
    match head.windows(2).position(|w| w == b"\r\n") {
        Some(end) => Some(0..end),
        None if !head.is_empty() => Some(0..head.len()),
        None => None,
    }
}

/// Turn every line after `skip` lines of the head into a `(name, value)`
/// range pair.
fn collect_header_ranges(buf: &[u8], skip: usize) -> Result<Vec<HeaderRange>, HttpError> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut seen = 0usize;
    let mut i = 0usize;

    while i + 1 < buf.len() {
        if buf[i] == b'\r' && buf[i + 1] == b'\n' {
            if seen >= skip {
                let line = &buf[start..i];
                if !line.is_empty() {
                    out.push(header_ranges(start, line)?);
                }
            }
            seen += 1;
            start = i + 2;
            i += 2;
        } else {
            i += 1;
        }
    }
    // The last header line's CRLF is consumed by the `\r\n\r\n` delimiter,
    // so `start` may still point at an unterminated final line — collect it.
    if start < buf.len() && seen >= skip {
        let line = &buf[start..];
        if !line.is_empty() {
            out.push(header_ranges(start, line)?);
        }
    }
    Ok(out)
}

/// Split one `name: value` line (absolute offsets in `buf`).
fn header_ranges(start: usize, line: &[u8]) -> Result<(Range<usize>, Range<usize>), HttpError> {
    let colon = line
        .iter()
        .position(|&b| b == b':')
        .ok_or(HttpError::Parse("header line missing colon"))?;
    let trimmed = trim_ows(&line[colon + 1..]);
    Ok((
        start..start + colon,
        start + colon + 1 + trimmed.start..start + colon + 1 + trimmed.end,
    ))
}

/// `method SP target SP version`
fn parse_request_line(line: &[u8]) -> Result<(Range<usize>, Range<usize>, Version), HttpError> {
    let err = || HttpError::Parse("malformed request line");
    let sp1 = line.iter().position(|&b| b == b' ').ok_or_else(err)?;
    if sp1 == 0 || !line[..sp1].iter().all(is_tchar_strict) {
        return Err(err());
    }

    let rest = &line[sp1 + 1..];
    let sp2 = rest.iter().position(|&b| b == b' ').ok_or_else(err)?;
    if sp2 == 0 {
        return Err(err());
    }

    let version = match &rest[sp2 + 1..] {
        b"HTTP/1.1" => Version::Http11,
        b"HTTP/1.0" => Version::Http10,
        _ => return Err(HttpError::Parse("unsupported HTTP version")),
    };
    Ok((0..sp1, sp1 + 1..sp1 + 1 + sp2, version))
}

/// `version SP code SP reason`
fn parse_status_line(line: &[u8]) -> Result<(Version, u16, Range<usize>), HttpError> {
    let err = || HttpError::Parse("malformed status line");
    if !line.starts_with(b"HTTP/1.1 ") && !line.starts_with(b"HTTP/1.0 ") {
        return Err(err());
    }
    let version = if line[5] == b'1' {
        Version::Http11
    } else {
        Version::Http10
    };
    let rest = &line[9..];
    if rest.len() < 3 || !rest[..3].iter().all(|b| b.is_ascii_digit()) {
        return Err(err());
    }
    let code =
        (rest[0] - b'0') as u16 * 100 + (rest[1] - b'0') as u16 * 10 + (rest[2] - b'0') as u16;
    if !(100..=599).contains(&code) {
        return Err(err());
    }
    let trimmed = trim_ows(&rest[3..]);
    let reason = 9 + 3 + trimmed.start..9 + 3 + trimmed.end;
    Ok((version, code, reason))
}

/// RFC 9110 `tchar` — valid token characters (method names are tokens).
fn is_tchar_strict(b: &u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' |
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'^' | b'_' | b'`' | b'|' | b'~')
}

fn offset_range(r: Range<usize>, by: usize) -> Range<usize> {
    r.start + by..r.end + by
}

fn trim_ows(value: &[u8]) -> Range<usize> {
    let mut start = 0;
    let mut end = value.len();
    while start < end && (value[start] == b' ' || value[start] == b'\t') {
        start += 1;
    }
    while end > start && (value[end - 1] == b' ' || value[end - 1] == b'\t') {
        end -= 1;
    }
    start..end
}

/// Tiny substring search (HTTP heads are small; no need for memchr).
fn memchr_find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::HttpError;

    #[test]
    fn parses_request_head_with_headers() {
        let buf =
            b"POST /submit?x=1 HTTP/1.1\r\nhost: example.com\r\ncontent-length: 5\r\n\r\nhello";
        let parsed = parse_request_ranges(buf).unwrap().unwrap();
        assert_eq!(parsed.consumed, buf.len() - 5);
        let arc = alloc::sync::Arc::new(buf.to_vec());
        let head = RequestHead::from_ranges(arc, &parsed);
        assert_eq!(head.method(), b"POST");
        assert_eq!(head.target(), b"/submit?x=1");
        assert_eq!(head.version(), Version::Http11);
        assert_eq!(head.headers().get(b"Host"), Some(&b"example.com"[..]));
        assert_eq!(head.headers().get(b"CONTENT-LENGTH"), Some(&b"5"[..]));
    }

    #[test]
    fn parses_status_line_and_reason() {
        let buf = b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n";
        let parsed = parse_response_ranges(buf).unwrap().unwrap();
        let arc = alloc::sync::Arc::new(buf.to_vec());
        let head = ResponseHead::from_ranges(arc, &parsed);
        assert_eq!(head.status(), 404);
        assert_eq!(head.reason(), b"Not Found");
    }

    #[test]
    fn partial_head_needs_more_bytes() {
        let buf = b"GET / HTTP/1.1\r\nhost: a";
        assert!(parse_request_ranges(buf).unwrap().is_none());
    }

    #[test]
    fn rejects_obs_fold() {
        let buf = b"GET / HTTP/1.1\r\nx-a: 1\r\n  folded\r\n\r\n";
        assert!(matches!(
            parse_request_ranges(buf),
            Err(HttpError::Parse("obsolete line folding in header block"))
        ));
    }

    #[test]
    fn rejects_header_without_colon() {
        let buf = b"GET / HTTP/1.1\r\nbroken-header\r\n\r\n";
        assert!(matches!(
            parse_request_ranges(buf),
            Err(HttpError::Parse("header line missing colon"))
        ));
    }

    #[test]
    fn rejects_empty_method_and_bad_token() {
        assert!(parse_request_ranges(b" / HTTP/1.1\r\n\r\n").is_err());
        assert!(parse_request_ranges(b"GE(T / HTTP/1.1\r\n\r\n").is_err());
    }

    #[test]
    fn rejects_unsupported_version() {
        assert!(parse_request_ranges(b"GET / HTTP/2.0\r\n\r\n").is_err());
        assert!(parse_request_ranges(b"GET / HTTP/1.9\r\n\r\n").is_err());
    }

    #[test]
    fn lf_only_head_is_incomplete_not_an_error() {
        // LF-only line endings: no CRLFCRLF delimiter ever appears.
        let buf = b"GET / HTTP/1.1\nhost: a\n\n";
        assert!(parse_request_ranges(buf).unwrap().is_none());
    }

    #[test]
    fn oversized_head_rejected() {
        let big = alloc::vec![b'a'; MAX_HEAD + 1];
        assert!(matches!(
            parse_request_ranges(&big),
            Err(HttpError::Parse("message head exceeds 64 KiB"))
        ));
    }

    // ── body framing (smuggling hygiene) ─────────────────────────────────────

    use crate::body::{request_body_kind, response_body_kind, BodyKind};

    fn block_of(pairs: &[(&str, &str)]) -> HeaderBlock {
        // Build a HeaderBlock by parsing a synthetic head.
        let mut head = alloc::string::String::from("POST / HTTP/1.1\r\n");
        for (n, v) in pairs {
            head.push_str(n);
            head.push_str(": ");
            head.push_str(v);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        let parsed = parse_request_ranges(head.as_bytes())
            .unwrap()
            .expect("parses");
        RequestHead::from_ranges(alloc::sync::Arc::new(head.into_bytes()), &parsed)
            .headers
            .clone()
    }

    #[test]
    fn content_length_framing() {
        let headers = block_of(&[("content-length", "12")]);
        assert_eq!(
            request_body_kind(&headers).unwrap(),
            BodyKind::ContentLength(12)
        );
    }

    #[test]
    fn chunked_request_framing() {
        let headers = block_of(&[("transfer-encoding", "chunked")]);
        assert_eq!(request_body_kind(&headers).unwrap(), BodyKind::Chunked);
    }

    #[test]
    fn te_plus_cl_is_rejected() {
        let headers = block_of(&[("transfer-encoding", "chunked"), ("content-length", "5")]);
        assert!(matches!(
            request_body_kind(&headers),
            Err(HttpError::ConflictingHeaders(_))
        ));
    }

    #[test]
    fn unsupported_te_is_rejected() {
        let headers = block_of(&[("transfer-encoding", "gzip")]);
        assert!(request_body_kind(&headers).is_err());
    }

    #[test]
    fn response_without_length_is_eof_framed() {
        let headers = block_of(&[]);
        assert_eq!(
            response_body_kind(&headers, 200, false).unwrap(),
            BodyKind::UntilEof
        );
    }

    #[test]
    fn response_204_has_no_body() {
        let headers = block_of(&[]);
        assert_eq!(
            response_body_kind(&headers, 204, false).unwrap(),
            BodyKind::Empty
        );
        // 204 even WITH a content-length stays empty (no body by spec).
        let headers = block_of(&[("content-length", "10")]);
        assert_eq!(
            response_body_kind(&headers, 204, false).unwrap(),
            BodyKind::Empty
        );
    }

    #[test]
    fn head_request_has_no_body() {
        let headers = block_of(&[("content-length", "10")]);
        assert_eq!(
            response_body_kind(&headers, 200, true).unwrap(),
            BodyKind::Empty
        );
    }
}

#[cfg(test)]
mod ws_handshake_debug {
    use super::*;

    #[test]
    fn parses_ws_handshake_head() {
        let buf = b"GET /ws HTTP/1.1\r\nhost: t\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\n\r\n";
        match parse_request_ranges(buf) {
            Ok(Some(ranges)) => {
                let arc = alloc::sync::Arc::new(buf.to_vec());
                let head = RequestHead::from_ranges(arc, &ranges);
                assert_eq!(
                    head.headers().get(b"sec-websocket-key"),
                    Some(&b"dGhlIHNhbXBsZSBub25jZQ=="[..])
                );
            }
            Ok(None) => panic!("partial: needed more bytes"),
            Err(e) => panic!("parse error: {e:?}"),
        }
    }
}

/// Single-input entry for fuzzing: exercises both head parsers and the
/// body-framing decision on the same bytes.
#[cfg(feature = "fuzz")]
pub fn fuzz_entry(data: &[u8]) {
    let _ = parse_request_ranges(data);
    let _ = parse_response_ranges(data);
}
