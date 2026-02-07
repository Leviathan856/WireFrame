use std::fmt;

/// Errors that can occur during HTTP request parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The HTTP method is not a recognized standard method.
    InvalidMethod(String),
    /// The HTTP version string is not `HTTP/1.0` or `HTTP/1.1`.
    InvalidVersion(String),
    /// The request URI is malformed or empty.
    InvalidUri(String),
    /// The `Content-Length` header value is not a valid integer.
    InvalidContentLength(String),
    /// A chunk size in chunked transfer encoding is not valid hexadecimal.
    InvalidChunkSize(String),
    /// An unexpected byte was encountered during parsing.
    UnexpectedByte {
        /// Human-readable description of what was expected.
        expected: &'static str,
        /// The actual byte value found.
        found: u8,
    },
    /// A header name or value exceeds the configured maximum size.
    HeaderTooLarge,
    /// The request body exceeds the configured maximum size.
    BodyTooLarge,
    /// The number of headers exceeds the configured maximum.
    TooManyHeaders,
    /// The request data ended before a complete HTTP request was parsed.
    IncompleteRequest,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMethod(m) => write!(f, "invalid HTTP method: '{m}'"),
            Self::InvalidVersion(v) => write!(f, "invalid HTTP version: '{v}'"),
            Self::InvalidUri(u) => write!(f, "invalid request URI: '{u}'"),
            Self::InvalidContentLength(v) => write!(f, "invalid Content-Length: '{v}'"),
            Self::InvalidChunkSize(s) => write!(f, "invalid chunk size: '{s}'"),
            Self::UnexpectedByte { expected, found } => {
                write!(f, "unexpected byte 0x{found:02X} (expected {expected})")
            }
            Self::HeaderTooLarge => write!(f, "header exceeds maximum allowed size"),
            Self::BodyTooLarge => write!(f, "body exceeds maximum allowed size"),
            Self::TooManyHeaders => write!(f, "number of headers exceeds maximum"),
            Self::IncompleteRequest => write!(f, "incomplete HTTP request"),
        }
    }
}

impl std::error::Error for ParseError {}
