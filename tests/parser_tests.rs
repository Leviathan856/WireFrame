use wireframe::{
    format_debug, format_headers_only, format_json, parse_request,
    parse_request_with_config, HttpMethod, HttpVersion, ParseStatus, Parser,
    ParserConfig,
};

// =========================================================================
// Request-line parsing
// =========================================================================

#[test]
fn simple_get_request() {
    let raw = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.method, HttpMethod::GET);
    assert_eq!(req.uri, "/");
    assert_eq!(req.version, HttpVersion::Http11);
    assert_eq!(req.headers.len(), 1);
    assert_eq!(req.headers[0].name, "Host");
    assert_eq!(req.headers[0].value, "example.com");
    assert!(req.body.is_none());
}

#[test]
fn get_with_query_string() {
    let raw =
        b"GET /api/users?page=1&limit=10 HTTP/1.1\r\nHost: api.example.com\r\nAccept: application/json\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.method, HttpMethod::GET);
    assert_eq!(req.uri, "/api/users?page=1&limit=10");
    assert_eq!(
        req.header_value("Accept"),
        Some("application/json")
    );
}

#[test]
fn http_10_version() {
    let raw = b"GET /legacy HTTP/1.0\r\nHost: old.example.com\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.version, HttpVersion::Http10);
}

#[test]
fn all_standard_methods() {
    let methods = [
        ("GET", HttpMethod::GET),
        ("HEAD", HttpMethod::HEAD),
        ("POST", HttpMethod::POST),
        ("PUT", HttpMethod::PUT),
        ("DELETE", HttpMethod::DELETE),
        ("CONNECT", HttpMethod::CONNECT),
        ("OPTIONS", HttpMethod::OPTIONS),
        ("TRACE", HttpMethod::TRACE),
        ("PATCH", HttpMethod::PATCH),
    ];

    for (name, expected) in methods {
        let raw = format!("{name} / HTTP/1.1\r\nHost: h\r\n\r\n");
        let req = parse_request(raw.as_bytes())
            .unwrap_or_else(|e| panic!("method {name}: {e}"));
        assert_eq!(req.method, expected, "mismatch for method {name}");
    }
}

#[test]
fn options_asterisk_uri() {
    let raw = b"OPTIONS * HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.uri, "*");
}

// =========================================================================
// Header parsing
// =========================================================================

#[test]
fn multiple_headers() {
    let raw = b"GET / HTTP/1.1\r\n\
        Host: example.com\r\n\
        Accept: text/html\r\n\
        Accept-Language: en-US\r\n\
        User-Agent: WireFrame/1.0\r\n\
        Connection: keep-alive\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.headers.len(), 5);
    assert_eq!(req.header_value("Host"), Some("example.com"));
    assert_eq!(req.header_value("Accept"), Some("text/html"));
    assert_eq!(
        req.header_value("User-Agent"),
        Some("WireFrame/1.0")
    );
}

#[test]
fn header_value_ows_is_trimmed() {
    let raw = b"GET / HTTP/1.1\r\nHost:   example.com   \r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.header_value("Host"), Some("example.com"));
}

#[test]
fn header_value_with_interior_spaces() {
    let raw = b"GET / HTTP/1.1\r\nX-Custom: hello   world\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(
        req.header_value("X-Custom"),
        Some("hello   world")
    );
}

#[test]
fn empty_header_value() {
    let raw = b"GET / HTTP/1.1\r\nHost: example.com\r\nX-Empty:\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.header_value("X-Empty"), Some(""));
}

#[test]
fn case_insensitive_header_lookup() {
    let raw =
        b"GET / HTTP/1.1\r\nhost: example.com\r\ncontent-type: text/plain\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.header_value("Host"), Some("example.com"));
    assert_eq!(
        req.header_value("CONTENT-TYPE"),
        Some("text/plain")
    );
}

#[test]
fn duplicate_header_values() {
    let raw = b"GET / HTTP/1.1\r\nHost: h\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    let cookies = req.header_values("Set-Cookie");
    assert_eq!(cookies, vec!["a=1", "b=2"]);
}

// =========================================================================
// Body parsing (Content-Length)
// =========================================================================

#[test]
fn post_with_content_length_body() {
    let body = "name=John&age=30";
    let raw = format!(
        "POST /submit HTTP/1.1\r\n\
         Host: example.com\r\n\
         Content-Length: {}\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\r\n\
         {}",
        body.len(),
        body
    );
    let req = parse_request(raw.as_bytes()).expect("should parse");
    assert_eq!(req.method, HttpMethod::POST);
    assert_eq!(req.uri, "/submit");
    assert_eq!(req.body_as_str(), Some(body));
    assert_eq!(req.content_length(), Some(16));
}

#[test]
fn content_length_zero_yields_no_body() {
    let raw =
        b"POST /empty HTTP/1.1\r\nHost: h\r\nContent-Length: 0\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert!(req.body.is_none());
}

#[test]
fn put_with_json_body() {
    let body = r#"{"key":"value"}"#;
    let raw = format!(
        "PUT /resource HTTP/1.1\r\n\
         Host: api.example.com\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n\
         {}",
        body.len(),
        body
    );
    let req = parse_request(raw.as_bytes()).expect("should parse");
    assert_eq!(req.method, HttpMethod::PUT);
    assert_eq!(req.body_as_str(), Some(body));
}

#[test]
fn duplicate_identical_content_lengths_accepted() {
    let raw =
        b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\nabc";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.body_as_str(), Some("abc"));
}

// =========================================================================
// Chunked transfer encoding
// =========================================================================

#[test]
fn chunked_body_two_chunks() {
    let raw = b"POST /upload HTTP/1.1\r\n\
        Host: example.com\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.body_as_str(), Some("Hello World"));
    assert!(req.is_chunked());
}

#[test]
fn chunked_single_chunk() {
    let raw = b"POST /data HTTP/1.1\r\n\
        Host: h\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        4\r\nRust\r\n0\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.body_as_str(), Some("Rust"));
}

#[test]
fn chunked_with_extension() {
    let raw = b"POST /data HTTP/1.1\r\n\
        Host: h\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        5;ext=val\r\nHello\r\n0\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.body_as_str(), Some("Hello"));
}

#[test]
fn chunked_empty_body_zero_only() {
    let raw = b"POST / HTTP/1.1\r\n\
        Host: h\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        0\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    // zero-size only chunk yields empty body
    assert!(req.body.is_none() || req.body.as_deref() == Some(b""));
}

#[test]
fn chunked_hex_sizes() {
    // 0xA = 10 bytes, 0x5 = 5 bytes
    let raw = b"POST / HTTP/1.1\r\n\
        Host: h\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        A\r\n0123456789\r\n5\r\nabcde\r\n0\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.body_as_str(), Some("0123456789abcde"));
}

#[test]
fn chunked_with_trailer_fields() {
    let raw = b"POST / HTTP/1.1\r\n\
        Host: h\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        3\r\nabc\r\n0\r\n\
        Trailer-Field: value\r\n\r\n";
    let req = parse_request(raw).expect("should parse");
    assert_eq!(req.body_as_str(), Some("abc"));
}

// =========================================================================
// Incremental (streaming) parsing
// =========================================================================

#[test]
fn incremental_byte_by_byte() {
    let raw = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut parser = Parser::new();

    for &byte in &raw[..raw.len() - 1] {
        let status = parser
            .feed(&[byte])
            .expect("each byte should be ok");
        assert_eq!(status, ParseStatus::Incomplete);
    }

    let status = parser
        .feed(&[raw[raw.len() - 1]])
        .expect("last byte");
    assert!(matches!(status, ParseStatus::Complete(_)));

    let req = parser.finish().expect("should finish");
    assert_eq!(req.method, HttpMethod::GET);
    assert_eq!(req.uri, "/");
}

#[test]
fn incremental_multi_chunk_with_body() {
    let part1 = b"POST /path HTTP/1.1\r\n";
    let part2 = b"Host: example.com\r\n";
    let part3 = b"Content-Length: 5\r\n\r\n";
    let part4 = b"Hello";

    let mut parser = Parser::new();

    assert_eq!(parser.feed(part1).unwrap(), ParseStatus::Incomplete);
    assert_eq!(parser.feed(part2).unwrap(), ParseStatus::Incomplete);
    assert_eq!(parser.feed(part3).unwrap(), ParseStatus::Incomplete);
    assert!(matches!(
        parser.feed(part4).unwrap(),
        ParseStatus::Complete(_)
    ));

    let req = parser.finish().unwrap();
    assert_eq!(req.uri, "/path");
    assert_eq!(req.body_as_str(), Some("Hello"));
}

#[test]
fn incremental_chunked_body() {
    let mut parser = Parser::new();

    assert_eq!(
        parser
            .feed(b"POST / HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n")
            .unwrap(),
        ParseStatus::Incomplete
    );
    assert_eq!(
        parser.feed(b"3\r\nabc\r\n").unwrap(),
        ParseStatus::Incomplete
    );
    assert!(matches!(
        parser.feed(b"0\r\n\r\n").unwrap(),
        ParseStatus::Complete(_)
    ));

    let req = parser.finish().unwrap();
    assert_eq!(req.body_as_str(), Some("abc"));
}

// =========================================================================
// Bytes-consumed / pipelining
// =========================================================================

#[test]
fn bytes_consumed_with_trailing_data() {
    let raw = b"GET / HTTP/1.1\r\nHost: h\r\n\r\nGET /next HTTP/1.1\r\n";
    let mut parser = Parser::new();
    let status = parser.feed(raw).unwrap();

    if let ParseStatus::Complete(consumed) = status {
        // The bytes after the first request should start with "GET".
        assert_eq!(&raw[consumed..consumed + 3], b"GET");
    } else {
        panic!("expected Complete");
    }
}

// =========================================================================
// Parser reset & reuse
// =========================================================================

#[test]
fn parser_reset_and_reuse() {
    let raw1 = b"GET /a HTTP/1.1\r\nHost: h\r\n\r\n";
    let raw2 = b"POST /b HTTP/1.1\r\nHost: h\r\nContent-Length: 2\r\n\r\nOK";

    let mut parser = Parser::new();

    assert!(matches!(
        parser.feed(raw1).unwrap(),
        ParseStatus::Complete(_)
    ));

    parser.reset();

    assert!(matches!(
        parser.feed(raw2).unwrap(),
        ParseStatus::Complete(_)
    ));

    let req = parser.finish().unwrap();
    assert_eq!(req.method, HttpMethod::POST);
    assert_eq!(req.uri, "/b");
    assert_eq!(req.body_as_str(), Some("OK"));
}

// =========================================================================
// Error conditions
// =========================================================================

#[test]
fn error_invalid_method() {
    let raw = b"FOOBAR / HTTP/1.1\r\nHost: h\r\n\r\n";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_empty_method() {
    let raw = b" / HTTP/1.1\r\nHost: h\r\n\r\n";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_invalid_version() {
    let raw = b"GET / HTTP/2.0\r\nHost: h\r\n\r\n";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_missing_crlf_uses_bare_lf() {
    let raw = b"GET / HTTP/1.1\nHost: h\n\n";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_incomplete_request_no_end() {
    let raw = b"GET / HTTP/1.1\r\nHost: h\r\n";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_incomplete_body() {
    let raw =
        b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 100\r\n\r\nshort";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_differing_content_lengths() {
    let raw =
        b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 3\r\nContent-Length: 5\r\n\r\nabc";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_negative_content_length() {
    let raw =
        b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: -1\r\n\r\n";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_non_numeric_content_length() {
    let raw =
        b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: abc\r\n\r\n";
    assert!(parse_request(raw).is_err());
}

#[test]
fn error_empty_uri() {
    // Two spaces between method and version → empty URI.
    let raw = b"GET  HTTP/1.1\r\nHost: h\r\n\r\n";
    assert!(parse_request(raw).is_err());
}

// =========================================================================
// Configuration limits
// =========================================================================

#[test]
fn config_max_body_size_enforced() {
    let config = ParserConfig {
        max_body_size: 5,
        ..ParserConfig::default()
    };
    let raw = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 10\r\n\r\n0123456789";
    assert!(parse_request_with_config(raw, config).is_err());
}

#[test]
fn config_max_headers_count_enforced() {
    let config = ParserConfig {
        max_headers_count: 2,
        ..ParserConfig::default()
    };
    let raw =
        b"GET / HTTP/1.1\r\nH1: a\r\nH2: b\r\nH3: c\r\n\r\n";
    assert!(parse_request_with_config(raw, config).is_err());
}

#[test]
fn config_max_uri_len_enforced() {
    let config = ParserConfig {
        max_uri_len: 5,
        ..ParserConfig::default()
    };
    let raw = b"GET /very-long-uri HTTP/1.1\r\nHost: h\r\n\r\n";
    assert!(parse_request_with_config(raw, config).is_err());
}

#[test]
fn config_max_header_name_len_enforced() {
    let config = ParserConfig {
        max_header_name_len: 4,
        ..ParserConfig::default()
    };
    let raw =
        b"GET / HTTP/1.1\r\nVeryLongHeaderName: v\r\n\r\n";
    assert!(parse_request_with_config(raw, config).is_err());
}

#[test]
fn config_max_header_value_len_enforced() {
    let config = ParserConfig {
        max_header_value_len: 3,
        ..ParserConfig::default()
    };
    let raw =
        b"GET / HTTP/1.1\r\nHost: very-long-value\r\n\r\n";
    assert!(parse_request_with_config(raw, config).is_err());
}

#[test]
fn config_chunked_body_too_large() {
    let config = ParserConfig {
        max_body_size: 3,
        ..ParserConfig::default()
    };
    let raw = b"POST / HTTP/1.1\r\n\
        Host: h\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        5\r\nHello\r\n0\r\n\r\n";
    assert!(parse_request_with_config(raw, config).is_err());
}

// =========================================================================
// HttpRequest helper methods
// =========================================================================

#[test]
fn body_as_lossy_string() {
    let raw = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 3\r\n\r\nabc";
    let req = parse_request(raw).unwrap();
    assert_eq!(
        req.body_as_lossy_string(),
        Some("abc".to_string())
    );
}

#[test]
fn body_bytes_accessor() {
    let raw = b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 3\r\n\r\nXYZ";
    let req = parse_request(raw).unwrap();
    assert_eq!(req.body_bytes(), Some(b"XYZ".as_slice()));
}

#[test]
fn is_chunked_detection() {
    let raw = b"POST / HTTP/1.1\r\n\
        Host: h\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        0\r\n\r\n";
    let req = parse_request(raw).unwrap();
    assert!(req.is_chunked());
}

#[test]
fn is_not_chunked_without_header() {
    let raw = b"GET / HTTP/1.1\r\nHost: h\r\n\r\n";
    let req = parse_request(raw).unwrap();
    assert!(!req.is_chunked());
}

// =========================================================================
// Output formatting
// =========================================================================

#[test]
fn json_output_compact() {
    let raw = b"GET / HTTP/1.1\r\nHost: h\r\n\r\n";
    let req = parse_request(raw).unwrap();
    let json = format_json(&req, false);
    assert!(json.contains("\"method\":\"GET\""));
    assert!(json.contains("\"uri\":\"/\""));
    assert!(json.contains("\"version\":\"HTTP/1.1\""));
}

#[test]
fn json_output_pretty() {
    let raw = b"GET /pretty HTTP/1.1\r\nHost: h\r\n\r\n";
    let req = parse_request(raw).unwrap();
    let json = format_json(&req, true);
    // Pretty JSON has newlines and indentation.
    assert!(json.contains('\n'));
    assert!(json.contains("  "));
}

#[test]
fn json_output_with_body() {
    let raw =
        b"POST / HTTP/1.1\r\nHost: h\r\nContent-Length: 4\r\n\r\ndata";
    let req = parse_request(raw).unwrap();
    let json = format_json(&req, false);
    assert!(json.contains("\"body\":\"data\""));
}

#[test]
fn debug_output_contains_sections() {
    let raw = b"GET /test HTTP/1.1\r\nHost: h\r\n\r\n";
    let req = parse_request(raw).unwrap();
    let dbg = format_debug(&req);
    assert!(dbg.contains("=== HTTP Request ==="));
    assert!(dbg.contains("Method:  GET"));
    assert!(dbg.contains("URI:     /test"));
    assert!(dbg.contains("Version: HTTP/1.1"));
    assert!(dbg.contains("--- Headers"));
    assert!(dbg.contains("--- No Body ---"));
}

#[test]
fn headers_only_output() {
    let raw =
        b"GET /path HTTP/1.1\r\nHost: example.com\r\nAccept: */*\r\n\r\n";
    let req = parse_request(raw).unwrap();
    let out = format_headers_only(&req);
    assert!(out.starts_with("GET /path HTTP/1.1\n"));
    assert!(out.contains("Host: example.com\n"));
    assert!(out.contains("Accept: */*\n"));
}

// =========================================================================
// Edge cases
// =========================================================================

#[test]
fn large_body_content_length() {
    let body = "X".repeat(100_000);
    let raw = format!(
        "POST / HTTP/1.1\r\n\
         Host: h\r\n\
         Content-Length: {}\r\n\r\n\
         {}",
        body.len(),
        body
    );
    let req = parse_request(raw.as_bytes()).unwrap();
    assert_eq!(req.body_as_str(), Some(body.as_str()));
}

#[test]
fn many_headers_within_limit() {
    let mut raw = String::from("GET / HTTP/1.1\r\n");
    for i in 0..100 {
        raw.push_str(&format!("X-Header-{i}: value-{i}\r\n"));
    }
    raw.push_str("\r\n");

    let req = parse_request(raw.as_bytes()).unwrap();
    assert_eq!(req.headers.len(), 100);
}

#[test]
fn header_with_obs_text_bytes() {
    // obs-text (0x80-0xFF) is allowed in header values.
    let raw = b"GET / HTTP/1.1\r\nHost: h\r\nX-Custom: hello\x80world\r\n\r\n";
    let req = parse_request(raw).unwrap();
    let val = req.header_value("X-Custom").unwrap();
    // from_utf8_lossy replaces 0x80 with U+FFFD.
    assert!(val.contains('\u{FFFD}'));
}

#[test]
fn transfer_encoding_takes_precedence_over_content_length() {
    // RFC 9112 §6.1: if both are present, Transfer-Encoding wins.
    let raw = b"POST / HTTP/1.1\r\n\
        Host: h\r\n\
        Content-Length: 999\r\n\
        Transfer-Encoding: chunked\r\n\r\n\
        3\r\nabc\r\n0\r\n\r\n";
    let req = parse_request(raw).unwrap();
    assert_eq!(req.body_as_str(), Some("abc"));
}
