# WireFrame
MicroGate server core: WireFrame – strict streaming HTTP parser and CLI tool

## Global idea

A lightweight asynchronous (Tokio-based) HTTP server written from scratch in Rust, along with an HTTP parser implemented as a separate library, designed in a framework-style format for reuse in other projects. It provides a minimally sufficient feature set with straightforward extensibility and is intended to run on a wide range of devices, including mini-PCs running embedded Linux, enabling deployment for IoT use cases.

The HTTP parser itself is implemented as a standalone library that can also be used as a CLI tool for parsing raw HTTP requests. The parser performs all operations required by the standard on the parsing side and was developed using a TDD approach to ensure strict compliance with the specification.

Technical details:
	•	The server implements the HTTP/1.1 protocol in accordance with RFC 9112, maintaining a balance between modern standards and implementation complexity.
	•	TLS support is intentionally out of scope, as it does not affect HTTP protocol logic and is typically handled at a different system layer.
	•	Simple routing: method + path
	•	Static file storage
	•	Support for chunked request bodies, without streaming responses
	•	No WebSocket support
	•	HTTP parser: incremental parser, state machine–based, zero-copy where possible, strict RFC-compliant parsing, CLI support for converting raw HTTP into structured output (JSON / debug).

## What should be done

Implement the HTTP parser as described in project idea, creating whole project structure which should result in a library that can be integrated in Rust environment (be reused as package in the web server project), CLI tool to use the library without code and test coverage for the parser