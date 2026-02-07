use crate::types::HttpRequest;

/// Serialize an [`HttpRequest`] to a JSON string.
///
/// When `pretty` is `true` the output is indented for readability.
pub fn format_json(request: &HttpRequest, pretty: bool) -> String {
    if pretty {
        serde_json::to_string_pretty(request).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
    } else {
        serde_json::to_string(request).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
    }
}

/// Render an [`HttpRequest`] in a human-readable debug format.
pub fn format_debug(request: &HttpRequest) -> String {
    let mut out = String::with_capacity(256);

    out.push_str("=== HTTP Request ===\n");
    out.push_str(&format!("Method:  {}\n", request.method));
    out.push_str(&format!("URI:     {}\n", request.uri));
    out.push_str(&format!("Version: {}\n", request.version));

    out.push_str(&format!("\n--- Headers ({}) ---\n", request.headers.len()));
    for header in &request.headers {
        out.push_str(&format!("  {}: {}\n", header.name, header.value));
    }

    match &request.body {
        Some(body) => {
            out.push_str(&format!("\n--- Body ({} bytes) ---\n", body.len()));
            match std::str::from_utf8(body) {
                Ok(s) => out.push_str(s),
                Err(_) => {
                    out.push_str(&format!("<binary data: {} bytes>", body.len()));
                }
            }
            out.push('\n');
        }
        None => {
            out.push_str("\n--- No Body ---\n");
        }
    }

    out.push_str("====================\n");
    out
}

/// Render only the request line and headers (no body).
pub fn format_headers_only(request: &HttpRequest) -> String {
    let mut out = String::with_capacity(64 + request.headers.len() * 40);

    out.push_str(&format!(
        "{} {} {}\n",
        request.method, request.uri, request.version
    ));

    for header in &request.headers {
        out.push_str(&format!("{}: {}\n", header.name, header.value));
    }

    out
}
