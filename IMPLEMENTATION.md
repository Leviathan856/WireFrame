# WireFrame — Implementation Guide

## What was implemented

WireFrame is a **strict, streaming HTTP/1.1 request parser** implemented as a
finite state machine in pure Rust. It ships as both a **reusable library crate**
and a **CLI binary**.

### Core features

| Feature | Details |
|---|---|
| **State-machine parser** | Byte-by-byte processing with bulk-copy optimisation for body data. Processes data incrementally — suitable for both sync and async callers. |
| **RFC 9112 compliance** | Strict CRLF enforcement, `tchar` validation for method/header names, OWS trimming, obs-text support in header values. |
| **Chunked transfer encoding** | Full support for `Transfer-Encoding: chunked` including chunk extensions (ignored) and trailer sections (skipped). |
| **Content-Length body** | Fixed-length body reading with duplicate Content-Length detection (RFC 9112 §6.3). |
| **Transfer-Encoding precedence** | When both `Content-Length` and `Transfer-Encoding: chunked` are present, Transfer-Encoding wins per RFC 9112 §6.1. |
| **Configurable limits** | Max method length, URI length, header name/value sizes, header count, and body size — all configurable via `ParserConfig`. |
| **Zero unsafe code** | No `unwrap()` calls on fallible operations — all error paths use `Result` propagation, `unwrap_or_else`, or safe defaults. |
| **Three output formats** | JSON (compact/pretty), human-readable debug, and headers-only. |
| **CLI tool** | Reads raw HTTP from a file or stdin, outputs structured parsed data. |
| **62 tests** | 4 unit tests, 55 integration tests, 3 doc-tests covering happy paths, error cases, edge cases, incremental parsing, and config limits. |

### Project structure

```
WireFrame/
├── Cargo.toml                  # Package manifest (lib + bin targets)
├── README.md                   # Project concept
├── IMPLEMENTATION.md           # This file
├── src/
│   ├── lib.rs                  # Public API & re-exports
│   ├── error.rs                # ParseError enum
│   ├── types.rs                # HttpMethod, HttpVersion, Header, HttpRequest
│   ├── parser.rs               # State-machine parser, ParserConfig, ParseStatus
│   ├── output.rs               # JSON / debug / headers-only formatting
│   └── bin/
│       └── cli.rs              # wireframe-cli binary
└── tests/
    └── parser_tests.rs         # 55 integration tests
```

### Architecture decisions

- **State machine** — The parser maintains an explicit `State` enum and
  advances one transition per byte (except body states which bulk-copy).
  This makes the parser trivially resumable for incremental / async use.
- **No `unsafe`** — The entire crate is safe Rust. No `unwrap()` is used on
  user-supplied data; every fallible operation returns a proper `Result`.
- **`serde` for serialization** — `HttpRequest` derives `Serialize` so it
  can be directly serialized to JSON (or any other serde-supported format).
- **Minimal dependencies** — Only `serde`, `serde_json` and `clap` (CLI only).
  The parser itself has **zero** runtime dependencies beyond `serde`.

---

## How to compile

### Prerequisites

- **Rust toolchain ≥ 1.85** (for `edition = "2024"`)
- Install via [rustup](https://rustup.rs/):
  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustup update
  ```

### Build the library + CLI

```sh
# Debug build
cargo build

# Optimised release build
cargo build --release
```

The CLI binary is placed at:
- `target/debug/wireframe-cli` (debug)
- `target/release/wireframe-cli` (release)

### Run the tests

```sh
cargo test
```

### Run clippy (linter)

```sh
cargo clippy --all-targets
```

---

## Using the CLI tool

### From stdin

```sh
printf 'GET /hello HTTP/1.1\r\nHost: example.com\r\n\r\n' | wireframe-cli --pretty
```

Output:
```json
{
  "method": "GET",
  "uri": "/hello",
  "version": "HTTP/1.1",
  "headers": [
    { "name": "Host", "value": "example.com" }
  ],
  "body": null
}
```

### From a file

```sh
wireframe-cli request.raw --pretty
```

### Output formats

| Flag | Format |
|---|---|
| `-f json` (default) | JSON |
| `-f json --pretty` | Pretty-printed JSON |
| `-f debug` | Human-readable debug view |
| `-f headers` | Request-line + headers only |

### Additional options

```
wireframe-cli --help

Options:
  -f, --format <FORMAT>      Output format [default: json] [json|debug|headers]
  -p, --pretty               Pretty-print JSON output
      --max-body-size <N>    Maximum allowed body size in bytes [default: 10485760]
      --max-headers <N>      Maximum number of headers [default: 128]
  -h, --help                 Print help
  -V, --version              Print version
```

---

## Using WireFrame as a library

### Add the dependency

In your project's `Cargo.toml`, add a path or git dependency:

```toml
[dependencies]
wireframe = { path = "../WireFrame" }
# or from a git repo:
# wireframe = { git = "https://github.com/user/WireFrame.git" }
```

### One-shot parsing

```rust
use wireframe::{parse_request, HttpMethod};

fn main() {
    let raw = b"GET /api/data HTTP/1.1\r\nHost: localhost\r\n\r\n";

    match parse_request(raw) {
        Ok(request) => {
            println!("Method: {}", request.method);
            println!("URI:    {}", request.uri);

            if let Some(host) = request.header_value("Host") {
                println!("Host:   {host}");
            }

            if let Some(body) = request.body_as_str() {
                println!("Body:   {body}");
            }
        }
        Err(e) => eprintln!("Parse error: {e}"),
    }
}
```

### Incremental / streaming parsing

```rust
use wireframe::{Parser, ParseStatus};

fn main() {
    let mut parser = Parser::new();

    // Simulate data arriving in chunks (e.g. from a TCP socket).
    let chunks: &[&[u8]] = &[
        b"POST /upload HTTP/1.1\r\n",
        b"Host: example.com\r\n",
        b"Content-Length: 5\r\n\r\n",
        b"Hello",
    ];

    for chunk in chunks {
        match parser.feed(chunk) {
            Ok(ParseStatus::Complete(bytes)) => {
                println!("Request complete after {bytes} bytes");
                let request = parser.finish().expect("complete request");
                println!("{:?}", request);
                return;
            }
            Ok(ParseStatus::Incomplete) => {
                // Need more data, keep going.
            }
            Err(e) => {
                eprintln!("Parse error: {e}");
                return;
            }
        }
    }
}
```

### Custom parser limits

```rust
use wireframe::{parse_request_with_config, ParserConfig};

let config = ParserConfig {
    max_body_size: 1024,        // 1 KiB max body
    max_headers_count: 32,      // at most 32 headers
    max_uri_len: 2048,          // 2 KiB max URI
    ..ParserConfig::default()   // keep other defaults
};

let raw = b"GET / HTTP/1.1\r\nHost: h\r\n\r\n";
let request = parse_request_with_config(raw, config).expect("valid");
```

### Formatting parsed requests

```rust
use wireframe::{parse_request, format_json, format_debug, format_headers_only};

let raw = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
let request = parse_request(raw).expect("valid");

// JSON (compact or pretty)
println!("{}", format_json(&request, true));

// Human-readable debug
println!("{}", format_debug(&request));

// Headers only
println!("{}", format_headers_only(&request));
```

### Public API summary

| Item | Description |
|---|---|
| `parse_request(data)` | One-shot parse from `&[u8]` |
| `parse_request_with_config(data, config)` | One-shot with custom limits |
| `Parser::new()` / `Parser::with_config(c)` | Create an incremental parser |
| `parser.feed(data)` | Feed bytes, returns `Complete` or `Incomplete` |
| `parser.finish()` | Consume parser → `HttpRequest` |
| `parser.reset()` | Reuse parser for another request |
| `parser.is_complete()` | Check completion status |
| `parser.bytes_consumed()` | Total bytes consumed (for pipelining) |
| `HttpRequest` | Parsed request with method, URI, version, headers, body |
| `request.header_value(name)` | Case-insensitive single header lookup |
| `request.header_values(name)` | All values for a header name |
| `request.body_as_str()` | Body as `&str` (if valid UTF-8) |
| `request.content_length()` | Parsed `Content-Length` value |
| `request.is_chunked()` | Whether Transfer-Encoding is chunked |
| `format_json(&req, pretty)` | Serialize to JSON string |
| `format_debug(&req)` | Human-readable debug string |
| `format_headers_only(&req)` | Request-line + headers string |
| `ParserConfig` | Configurable limits (body size, header count, etc.) |
| `ParseError` | Detailed error enum for all failure modes |
