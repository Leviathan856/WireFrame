use serde::{Serialize, Serializer};
use std::fmt;

use crate::error::ParseError;

// ---------------------------------------------------------------------------
// HttpMethod
// ---------------------------------------------------------------------------

/// Standard HTTP request methods as defined in RFC 9110.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum HttpMethod {
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    CONNECT,
    OPTIONS,
    TRACE,
    PATCH,
}

impl HttpMethod {
    /// Parse an HTTP method from a byte slice.
    ///
    /// Returns an error if the bytes do not match a known method.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        match bytes {
            b"GET" => Ok(Self::GET),
            b"HEAD" => Ok(Self::HEAD),
            b"POST" => Ok(Self::POST),
            b"PUT" => Ok(Self::PUT),
            b"DELETE" => Ok(Self::DELETE),
            b"CONNECT" => Ok(Self::CONNECT),
            b"OPTIONS" => Ok(Self::OPTIONS),
            b"TRACE" => Ok(Self::TRACE),
            b"PATCH" => Ok(Self::PATCH),
            _ => Err(ParseError::InvalidMethod(
                String::from_utf8_lossy(bytes).into_owned(),
            )),
        }
    }

    /// Return the method as a static string slice.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GET => "GET",
            Self::HEAD => "HEAD",
            Self::POST => "POST",
            Self::PUT => "PUT",
            Self::DELETE => "DELETE",
            Self::CONNECT => "CONNECT",
            Self::OPTIONS => "OPTIONS",
            Self::TRACE => "TRACE",
            Self::PATCH => "PATCH",
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// HttpVersion
// ---------------------------------------------------------------------------

/// HTTP protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpVersion {
    /// HTTP/1.0
    Http10,
    /// HTTP/1.1
    Http11,
}

impl HttpVersion {
    /// Parse an HTTP version from a byte slice (e.g. `b"HTTP/1.1"`).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        match bytes {
            b"HTTP/1.0" => Ok(Self::Http10),
            b"HTTP/1.1" => Ok(Self::Http11),
            _ => Err(ParseError::InvalidVersion(
                String::from_utf8_lossy(bytes).into_owned(),
            )),
        }
    }

    /// Return the version as a static string slice.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http10 => "HTTP/1.0",
            Self::Http11 => "HTTP/1.1",
        }
    }
}

impl fmt::Display for HttpVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for HttpVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

/// A single HTTP header field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Header {
    /// Header field name (original casing preserved).
    pub name: String,
    /// Header field value (leading/trailing OWS trimmed).
    pub value: String,
}

// ---------------------------------------------------------------------------
// HttpRequest
// ---------------------------------------------------------------------------

/// A fully parsed HTTP request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HttpRequest {
    /// The request method.
    pub method: HttpMethod,
    /// The request target (URI / path).
    pub uri: String,
    /// The HTTP version.
    pub version: HttpVersion,
    /// The list of header fields.
    pub headers: Vec<Header>,
    /// The optional request body.
    #[serde(serialize_with = "serialize_body")]
    pub body: Option<Vec<u8>>,
}

/// Serialize body bytes as a UTF-8 string (lossy) for JSON output.
fn serialize_body<S: Serializer>(body: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
    match body {
        None => s.serialize_none(),
        Some(bytes) => s.serialize_str(&String::from_utf8_lossy(bytes)),
    }
}

impl HttpRequest {
    /// Return the body as a UTF-8 `&str` if it is valid UTF-8.
    pub fn body_as_str(&self) -> Option<&str> {
        self.body.as_ref().and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Return the body as a lossy UTF-8 string (always succeeds).
    pub fn body_as_lossy_string(&self) -> Option<String> {
        self.body
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// Return the raw body bytes.
    pub fn body_bytes(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }

    /// Look up the first header value by name (case-insensitive).
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }

    /// Return all values for headers matching `name` (case-insensitive).
    pub fn header_values(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
            .collect()
    }

    /// Parse the `Content-Length` header, if present and valid.
    pub fn content_length(&self) -> Option<usize> {
        self.header_value("content-length")
            .and_then(|v| v.trim().parse().ok())
    }

    /// Return `true` if the `Transfer-Encoding` header contains `chunked`.
    pub fn is_chunked(&self) -> bool {
        self.header_value("transfer-encoding")
            .map(|v| v.to_ascii_lowercase().contains("chunked"))
            .unwrap_or(false)
    }
}
