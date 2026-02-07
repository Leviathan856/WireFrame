use crate::error::ParseError;
use crate::types::{Header, HttpMethod, HttpRequest, HttpVersion};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configurable limits for the HTTP parser.
///
/// All sizes are in bytes unless stated otherwise.
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Maximum length of the HTTP method token (default: 16).
    pub max_method_len: usize,
    /// Maximum length of the request URI (default: 8 192).
    pub max_uri_len: usize,
    /// Maximum length of a single header field name (default: 256).
    pub max_header_name_len: usize,
    /// Maximum length of a single header field value (default: 8 192).
    pub max_header_value_len: usize,
    /// Maximum number of header fields (default: 128).
    pub max_headers_count: usize,
    /// Maximum body size (default: 10 MiB).
    pub max_body_size: usize,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            max_method_len: 16,
            max_uri_len: 8_192,
            max_header_name_len: 256,
            max_header_value_len: 8_192,
            max_headers_count: 128,
            max_body_size: 10 * 1024 * 1024,
        }
    }
}

// ---------------------------------------------------------------------------
// Parse status
// ---------------------------------------------------------------------------

/// Outcome of a [`Parser::feed`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseStatus {
    /// The parser has consumed a complete HTTP request.
    /// The contained value is the **total** number of bytes consumed so far
    /// (across all `feed` calls). Any bytes past this offset belong to the
    /// next request (HTTP pipelining).
    Complete(usize),
    /// The parser needs more data before the request is complete.
    Incomplete,
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    // ---- Request line ----
    Method,
    Uri,
    Version,
    VersionLf,

    // ---- Header section ----
    HeaderStart,
    HeaderName,
    HeaderValueOws,
    HeaderValue,
    HeaderValueLf,

    // ---- Transition to body ----
    EndHeadersLf,

    // ---- Fixed-length body ----
    Body,

    // ---- Chunked transfer encoding ----
    ChunkSize,
    ChunkExt,
    ChunkSizeLf,
    ChunkData,
    ChunkDataCr,
    ChunkDataLf,

    // ---- Chunked trailers ----
    TrailerStart,
    TrailerField,
    TrailerFieldLf,
    TrailerEndLf,

    // ---- Done ----
    Complete,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// An incremental, state-machine-based HTTP/1.1 request parser.
///
/// # Usage
///
/// ```rust
/// use wireframe::{Parser, ParseStatus};
///
/// let mut parser = Parser::new();
///
/// // Feed data (possibly in multiple calls).
/// let status = parser.feed(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
///
/// if let ParseStatus::Complete(_bytes_consumed) = status {
///     let request = parser.finish().unwrap();
///     assert_eq!(request.uri, "/");
/// }
/// ```
pub struct Parser {
    state: State,
    config: ParserConfig,
    bytes_consumed: usize,

    // Accumulation buffers
    method_buf: Vec<u8>,
    uri_buf: Vec<u8>,
    version_buf: Vec<u8>,
    header_name_buf: Vec<u8>,
    header_value_buf: Vec<u8>,
    body_buf: Vec<u8>,
    chunk_size_buf: Vec<u8>,

    // Parsed components
    method: Option<HttpMethod>,
    uri: Option<String>,
    version: Option<HttpVersion>,
    headers: Vec<Header>,

    // Body bookkeeping
    body_remaining: usize,
    chunk_remaining: usize,
}

impl Parser {
    /// Create a new parser with default configuration.
    pub fn new() -> Self {
        Self::with_config(ParserConfig::default())
    }

    /// Create a new parser with custom limits.
    pub fn with_config(config: ParserConfig) -> Self {
        Self {
            state: State::Method,
            config,
            bytes_consumed: 0,
            method_buf: Vec::with_capacity(8),
            uri_buf: Vec::with_capacity(256),
            version_buf: Vec::with_capacity(8),
            header_name_buf: Vec::with_capacity(32),
            header_value_buf: Vec::with_capacity(128),
            body_buf: Vec::new(),
            chunk_size_buf: Vec::with_capacity(16),
            method: None,
            uri: None,
            version: None,
            headers: Vec::new(),
            body_remaining: 0,
            chunk_remaining: 0,
        }
    }

    /// Reset the parser so it can be reused for another request.
    pub fn reset(&mut self) {
        self.state = State::Method;
        self.bytes_consumed = 0;
        self.method_buf.clear();
        self.uri_buf.clear();
        self.version_buf.clear();
        self.header_name_buf.clear();
        self.header_value_buf.clear();
        self.body_buf.clear();
        self.chunk_size_buf.clear();
        self.method = None;
        self.uri = None;
        self.version = None;
        self.headers.clear();
        self.body_remaining = 0;
        self.chunk_remaining = 0;
    }

    /// Feed a slice of bytes into the parser.
    ///
    /// Returns [`ParseStatus::Complete`] once a full HTTP request has been
    /// consumed, or [`ParseStatus::Incomplete`] if more data is required.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] on any protocol violation or limit breach.
    pub fn feed(&mut self, data: &[u8]) -> Result<ParseStatus, ParseError> {
        let mut i = 0;

        while i < data.len() {
            // Fast exit when already done (supports trailing data / pipelining).
            if self.state == State::Complete {
                return Ok(ParseStatus::Complete(self.bytes_consumed));
            }

            // ----- Bulk-copy paths for body states -----
            match self.state {
                State::Body => {
                    let available = data.len() - i;
                    let to_copy = available.min(self.body_remaining);

                    if self.body_buf.len() + to_copy > self.config.max_body_size {
                        return Err(ParseError::BodyTooLarge);
                    }

                    self.body_buf.extend_from_slice(&data[i..i + to_copy]);
                    self.body_remaining -= to_copy;
                    self.bytes_consumed += to_copy;
                    i += to_copy;

                    if self.body_remaining == 0 {
                        self.state = State::Complete;
                    }
                    continue;
                }
                State::ChunkData => {
                    let available = data.len() - i;
                    let to_copy = available.min(self.chunk_remaining);

                    if self.body_buf.len() + to_copy > self.config.max_body_size {
                        return Err(ParseError::BodyTooLarge);
                    }

                    self.body_buf.extend_from_slice(&data[i..i + to_copy]);
                    self.chunk_remaining -= to_copy;
                    self.bytes_consumed += to_copy;
                    i += to_copy;

                    if self.chunk_remaining == 0 {
                        self.state = State::ChunkDataCr;
                    }
                    continue;
                }
                _ => {}
            }

            // ----- Byte-by-byte path -----
            let byte = data[i];
            self.bytes_consumed += 1;
            i += 1;

            match self.state {
                // ===================== REQUEST LINE =====================
                State::Method => {
                    if byte == b' ' {
                        self.method = Some(HttpMethod::from_bytes(&self.method_buf)?);
                        self.state = State::Uri;
                    } else if is_tchar(byte) {
                        if self.method_buf.len() >= self.config.max_method_len {
                            return Err(ParseError::InvalidMethod("method too long".into()));
                        }
                        self.method_buf.push(byte);
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "token character or SP in request method",
                            found: byte,
                        });
                    }
                }

                State::Uri => {
                    if byte == b' ' {
                        if self.uri_buf.is_empty() {
                            return Err(ParseError::InvalidUri("empty URI".into()));
                        }
                        self.uri = Some(String::from_utf8_lossy(&self.uri_buf).into_owned());
                        self.state = State::Version;
                    } else if byte > b' ' && byte != 0x7F {
                        if self.uri_buf.len() >= self.config.max_uri_len {
                            return Err(ParseError::InvalidUri("URI too long".into()));
                        }
                        self.uri_buf.push(byte);
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "visible character or SP in request URI",
                            found: byte,
                        });
                    }
                }

                State::Version => {
                    if byte == b'\r' {
                        self.version = Some(HttpVersion::from_bytes(&self.version_buf)?);
                        self.state = State::VersionLf;
                    } else if byte >= b' ' && byte != 0x7F {
                        if self.version_buf.len() >= 16 {
                            return Err(ParseError::InvalidVersion(
                                "version string too long".into(),
                            ));
                        }
                        self.version_buf.push(byte);
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "version character or CR",
                            found: byte,
                        });
                    }
                }

                State::VersionLf => {
                    if byte == b'\n' {
                        self.state = State::HeaderStart;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "LF after version CR",
                            found: byte,
                        });
                    }
                }

                // ===================== HEADERS =====================
                State::HeaderStart => {
                    if byte == b'\r' {
                        // End of header section.
                        self.state = State::EndHeadersLf;
                    } else if is_tchar(byte) {
                        if self.headers.len() >= self.config.max_headers_count {
                            return Err(ParseError::TooManyHeaders);
                        }
                        self.header_name_buf.clear();
                        self.header_name_buf.push(byte);
                        self.state = State::HeaderName;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "header name character or CR",
                            found: byte,
                        });
                    }
                }

                State::HeaderName => {
                    if byte == b':' {
                        self.header_value_buf.clear();
                        self.state = State::HeaderValueOws;
                    } else if is_tchar(byte) {
                        if self.header_name_buf.len() >= self.config.max_header_name_len {
                            return Err(ParseError::HeaderTooLarge);
                        }
                        self.header_name_buf.push(byte);
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "header name character or ':'",
                            found: byte,
                        });
                    }
                }

                State::HeaderValueOws => {
                    if byte == b' ' || byte == b'\t' {
                        // Skip optional whitespace before the value.
                    } else if byte == b'\r' {
                        // Empty header value.
                        self.store_current_header();
                        self.state = State::HeaderValueLf;
                    } else if is_field_content_byte(byte) {
                        self.header_value_buf.push(byte);
                        self.state = State::HeaderValue;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "header value character, OWS, or CR",
                            found: byte,
                        });
                    }
                }

                State::HeaderValue => {
                    if byte == b'\r' {
                        // Trim trailing OWS from the value.
                        while self
                            .header_value_buf
                            .last()
                            .is_some_and(|&b| b == b' ' || b == b'\t')
                        {
                            self.header_value_buf.pop();
                        }
                        self.store_current_header();
                        self.state = State::HeaderValueLf;
                    } else if is_field_content_byte(byte) {
                        if self.header_value_buf.len() >= self.config.max_header_value_len {
                            return Err(ParseError::HeaderTooLarge);
                        }
                        self.header_value_buf.push(byte);
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "header value character or CR",
                            found: byte,
                        });
                    }
                }

                State::HeaderValueLf => {
                    if byte == b'\n' {
                        self.state = State::HeaderStart;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "LF after header value CR",
                            found: byte,
                        });
                    }
                }

                // ===================== END OF HEADERS =====================
                State::EndHeadersLf => {
                    if byte == b'\n' {
                        self.determine_body_handling()?;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "LF after end-of-headers CR",
                            found: byte,
                        });
                    }
                }

                // ===================== CHUNKED ENCODING =====================
                State::ChunkSize => {
                    if byte == b'\r' {
                        self.apply_chunk_size()?;
                        self.state = State::ChunkSizeLf;
                    } else if byte == b';' {
                        self.apply_chunk_size()?;
                        self.state = State::ChunkExt;
                    } else if byte.is_ascii_hexdigit() {
                        self.chunk_size_buf.push(byte);
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "hex digit, ';', or CR in chunk size",
                            found: byte,
                        });
                    }
                }

                State::ChunkExt => {
                    // RFC 9112 §7.1.1: ignore chunk extensions.
                    if byte == b'\r' {
                        self.state = State::ChunkSizeLf;
                    }
                }

                State::ChunkSizeLf => {
                    if byte == b'\n' {
                        if self.chunk_remaining == 0 {
                            // Last chunk → enter trailer section.
                            self.state = State::TrailerStart;
                        } else {
                            self.state = State::ChunkData;
                        }
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "LF after chunk size CR",
                            found: byte,
                        });
                    }
                }

                // ChunkData is handled by the bulk-copy path above.
                State::ChunkDataCr => {
                    if byte == b'\r' {
                        self.state = State::ChunkDataLf;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "CR after chunk data",
                            found: byte,
                        });
                    }
                }

                State::ChunkDataLf => {
                    if byte == b'\n' {
                        self.chunk_size_buf.clear();
                        self.state = State::ChunkSize;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "LF after chunk data CR",
                            found: byte,
                        });
                    }
                }

                // ===================== TRAILER SECTION =====================
                State::TrailerStart => {
                    if byte == b'\r' {
                        self.state = State::TrailerEndLf;
                    } else {
                        // Beginning of a trailer field – skip its content.
                        self.state = State::TrailerField;
                    }
                }

                State::TrailerField => {
                    if byte == b'\r' {
                        self.state = State::TrailerFieldLf;
                    }
                    // Otherwise keep skipping.
                }

                State::TrailerFieldLf => {
                    if byte == b'\n' {
                        self.state = State::TrailerStart;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "LF after trailer field CR",
                            found: byte,
                        });
                    }
                }

                State::TrailerEndLf => {
                    if byte == b'\n' {
                        self.state = State::Complete;
                    } else {
                        return Err(ParseError::UnexpectedByte {
                            expected: "LF after trailer-section end CR",
                            found: byte,
                        });
                    }
                }

                // Body & ChunkData handled above; Complete checked at loop top.
                State::Body | State::ChunkData | State::Complete => {
                    unreachable!("handled by bulk-copy or early-return paths");
                }
            }
        }

        if self.state == State::Complete {
            Ok(ParseStatus::Complete(self.bytes_consumed))
        } else {
            Ok(ParseStatus::Incomplete)
        }
    }

    // ----- helpers --------------------------------------------------------

    /// Move accumulated header name/value buffers into `self.headers`.
    fn store_current_header(&mut self) {
        let name = String::from_utf8_lossy(&self.header_name_buf).into_owned();
        let value = String::from_utf8_lossy(&self.header_value_buf).into_owned();
        self.headers.push(Header { name, value });
    }

    /// Inspect parsed headers to decide how to read the body.
    fn determine_body_handling(&mut self) -> Result<(), ParseError> {
        // Transfer-Encoding takes precedence over Content-Length (RFC 9112 §6.1).
        let has_chunked = self.headers.iter().any(|h| {
            h.name.eq_ignore_ascii_case("transfer-encoding")
                && h.value.to_ascii_lowercase().contains("chunked")
        });

        if has_chunked {
            self.chunk_size_buf.clear();
            self.state = State::ChunkSize;
            return Ok(());
        }

        // Collect Content-Length values.
        let cl_values: Vec<&str> = self
            .headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case("content-length"))
            .map(|h| h.value.as_str())
            .collect();

        // RFC 9112 §6.3: multiple differing Content-Length values are an error.
        if cl_values.len() > 1 {
            let first = cl_values[0].trim();
            if !cl_values.iter().all(|v| v.trim() == first) {
                return Err(ParseError::InvalidContentLength(
                    "multiple differing Content-Length values".into(),
                ));
            }
        }

        if let Some(cl_str) = cl_values.first() {
            let length: usize = cl_str
                .trim()
                .parse()
                .map_err(|_| ParseError::InvalidContentLength(cl_str.trim().to_string()))?;

            if length > self.config.max_body_size {
                return Err(ParseError::BodyTooLarge);
            }

            if length == 0 {
                self.state = State::Complete;
            } else {
                self.body_remaining = length;
                // Pre-allocate up to 64 KiB to avoid frequent reallocations.
                self.body_buf.reserve(length.min(65_536));
                self.state = State::Body;
            }
        } else {
            // No body indication → request is complete.
            self.state = State::Complete;
        }

        Ok(())
    }

    /// Parse the hex chunk-size that was accumulated in `chunk_size_buf`.
    fn apply_chunk_size(&mut self) -> Result<(), ParseError> {
        if self.chunk_size_buf.is_empty() {
            return Err(ParseError::InvalidChunkSize("empty chunk size".into()));
        }

        let size_str = String::from_utf8_lossy(&self.chunk_size_buf);
        let size = usize::from_str_radix(size_str.trim(), 16)
            .map_err(|_| ParseError::InvalidChunkSize(size_str.into_owned()))?;

        if self.body_buf.len() + size > self.config.max_body_size {
            return Err(ParseError::BodyTooLarge);
        }

        self.chunk_remaining = size;
        Ok(())
    }

    // ----- public query / finalization ------------------------------------

    /// Consume the parser and return the fully-parsed [`HttpRequest`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::IncompleteRequest`] if the parser has not yet
    /// reached the `Complete` state.
    pub fn finish(self) -> Result<HttpRequest, ParseError> {
        if self.state != State::Complete {
            return Err(ParseError::IncompleteRequest);
        }

        let body = if self.body_buf.is_empty() {
            None
        } else {
            Some(self.body_buf)
        };

        Ok(HttpRequest {
            method: self.method.ok_or(ParseError::IncompleteRequest)?,
            uri: self.uri.ok_or(ParseError::IncompleteRequest)?,
            version: self.version.ok_or(ParseError::IncompleteRequest)?,
            headers: self.headers,
            body,
        })
    }

    /// Returns `true` when a complete HTTP request has been parsed.
    pub fn is_complete(&self) -> bool {
        self.state == State::Complete
    }

    /// Total number of bytes consumed across all `feed` calls.
    pub fn bytes_consumed(&self) -> usize {
        self.bytes_consumed
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Character classification helpers (RFC 9110 / RFC 9112)
// ---------------------------------------------------------------------------

/// `tchar` – characters allowed in HTTP tokens (method, header names).
///
/// ```text
/// tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." /
///         "^" / "_" / "`" / "|" / "~" / DIGIT / ALPHA
/// ```
#[inline]
fn is_tchar(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
            | b'0'..=b'9'
            | b'a'..=b'z'
            | b'A'..=b'Z'
    )
}

/// Bytes permitted inside a header field value:
/// `SP / HTAB / VCHAR / obs-text`.
///
/// VCHAR = 0x21..=0x7E, obs-text = 0x80..=0xFF.
#[inline]
fn is_field_content_byte(b: u8) -> bool {
    b == b' ' || b == b'\t' || (0x21..=0x7E).contains(&b) || b >= 0x80
}

// ---------------------------------------------------------------------------
// Tests (unit)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tchar_accepts_valid_bytes() {
        for &b in b"abcXYZ019!#$%&'*+-.^_`|~" {
            assert!(is_tchar(b), "expected tchar for byte 0x{b:02X}");
        }
    }

    #[test]
    fn tchar_rejects_invalid_bytes() {
        for &b in b" \t\r\n@[]{}" {
            assert!(!is_tchar(b), "expected non-tchar for byte 0x{b:02X}");
        }
    }

    #[test]
    fn field_content_byte_accepts_sp_htab_vchar_obstext() {
        assert!(is_field_content_byte(b' '));
        assert!(is_field_content_byte(b'\t'));
        assert!(is_field_content_byte(b'A'));
        assert!(is_field_content_byte(0x80));
        assert!(is_field_content_byte(0xFF));
    }

    #[test]
    fn field_content_byte_rejects_ctl() {
        assert!(!is_field_content_byte(0x00));
        assert!(!is_field_content_byte(0x1F));
        assert!(!is_field_content_byte(0x7F)); // DEL
    }
}
