//! # WireFrame
//!
//! A **strict, streaming HTTP/1.1 request parser** implemented as a
//! state machine, designed for use both as a Rust library and as a CLI tool.
//!
//! WireFrame processes HTTP requests incrementally (byte-by-byte or in
//! arbitrarily-sized chunks), making it suitable for both synchronous and
//! asynchronous contexts. The parser follows **RFC 9112** strictly and
//! supports **chunked transfer encoding**.
//!
//! ## Quick start — one-shot parsing
//!
//! ```rust
//! use wireframe::parse_request;
//!
//! let raw = b"GET /hello HTTP/1.1\r\nHost: example.com\r\n\r\n";
//! let request = parse_request(raw).expect("valid request");
//! assert_eq!(request.method.as_str(), "GET");
//! assert_eq!(request.uri, "/hello");
//! ```
//!
//! ## Quick start — incremental parsing
//!
//! ```rust
//! use wireframe::{Parser, ParseStatus};
//!
//! let mut parser = Parser::new();
//!
//! let status = parser.feed(b"GET / HTTP/1.1\r\n").unwrap();
//! assert_eq!(status, ParseStatus::Incomplete);
//!
//! let status = parser.feed(b"Host: example.com\r\n\r\n").unwrap();
//! assert!(matches!(status, ParseStatus::Complete(_)));
//!
//! let request = parser.finish().unwrap();
//! assert_eq!(request.uri, "/");
//! ```

mod error;
mod output;
mod parser;
mod types;

// Re-export public API.
pub use error::ParseError;
pub use output::{format_debug, format_headers_only, format_json};
pub use parser::{ParseStatus, Parser, ParserConfig};
pub use types::{Header, HttpMethod, HttpRequest, HttpVersion};

/// Parse a **complete** HTTP request from a byte slice in one call.
///
/// This is a convenience wrapper around [`Parser`]. For incremental /
/// streaming use-cases, create a `Parser` directly.
///
/// # Errors
///
/// Returns [`ParseError`] if the data is malformed or incomplete.
pub fn parse_request(data: &[u8]) -> Result<HttpRequest, ParseError> {
    let mut parser = Parser::new();
    match parser.feed(data)? {
        ParseStatus::Complete(_) => parser.finish(),
        ParseStatus::Incomplete => Err(ParseError::IncompleteRequest),
    }
}

/// Parse a **complete** HTTP request using custom [`ParserConfig`] limits.
///
/// # Errors
///
/// Returns [`ParseError`] if the data is malformed, incomplete, or
/// exceeds the configured limits.
pub fn parse_request_with_config(
    data: &[u8],
    config: ParserConfig,
) -> Result<HttpRequest, ParseError> {
    let mut parser = Parser::with_config(config);
    match parser.feed(data)? {
        ParseStatus::Complete(_) => parser.finish(),
        ParseStatus::Incomplete => Err(ParseError::IncompleteRequest),
    }
}
