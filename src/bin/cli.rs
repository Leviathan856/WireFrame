use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process;

use clap::{CommandFactory, Parser as ClapParser};

use wireframe::{
    format_debug, format_headers_only, format_json, parse_request_with_config,
    ParserConfig,
};

/// WireFrame CLI — strict HTTP/1.1 request parser.
///
/// Reads a raw HTTP request from a file, --raw string, or stdin and outputs
/// a structured representation in the chosen format.
///
/// Escape sequences (\r, \n, \t, \\) in the --raw value are interpreted so
/// you can pass a full HTTP request as a single shell argument.
#[derive(ClapParser)]
#[command(name = "wireframe-cli", version, about, long_about = None)]
struct Cli {
    /// Path to a file containing a raw HTTP request.
    /// Reads from stdin when neither FILE nor --raw is given.
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Raw HTTP request string (escape sequences \r \n \t \\ are expanded).
    #[arg(long)]
    raw: Option<String>,

    /// Output format.
    #[arg(short, long, default_value = "json", value_enum)]
    format: OutputFormat,

    /// Pretty-print JSON output (ignored for other formats).
    #[arg(short, long)]
    pretty: bool,

    /// Maximum allowed body size in bytes.
    #[arg(long, default_value = "10485760")]
    max_body_size: usize,

    /// Maximum number of headers allowed.
    #[arg(long, default_value = "128")]
    max_headers: usize,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum OutputFormat {
    /// JSON output
    Json,
    /// Human-readable debug output
    Debug,
    /// Request-line + headers only
    Headers,
}

fn main() {
    let cli = Cli::parse();

    // When no input source is provided and stdin is a terminal (not piped),
    // show help instead of blocking.
    if cli.file.is_none() && cli.raw.is_none() && std::io::stdin().is_terminal() {
        Cli::command().print_help().ok();
        println!();
        process::exit(0);
    }

    let data = match read_input(&cli) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error reading input: {e}");
            process::exit(1);
        }
    };

    if data.is_empty() {
        eprintln!("Error: empty input");
        process::exit(1);
    }

    let config = ParserConfig {
        max_body_size: cli.max_body_size,
        max_headers_count: cli.max_headers,
        ..ParserConfig::default()
    };

    let request = match parse_request_with_config(&data, config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Parse error: {e}");
            process::exit(2);
        }
    };

    let output = match cli.format {
        OutputFormat::Json => format_json(&request, cli.pretty),
        OutputFormat::Debug => format_debug(&request),
        OutputFormat::Headers => format_headers_only(&request),
    };

    print!("{output}");
}

/// Read raw HTTP bytes from --raw, a file, or stdin.
fn read_input(cli: &Cli) -> Result<Vec<u8>, std::io::Error> {
    if let Some(raw) = &cli.raw {
        return Ok(unescape(raw).into_bytes());
    }
    match &cli.file {
        Some(path) => std::fs::read(path),
        None => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            Ok(buf)
        }
    }
}

/// Expand C-style escape sequences (`\r`, `\n`, `\t`, `\\`) in a string.
///
/// Any other `\X` sequence is kept as-is (both the backslash and `X`).
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('r') => out.push('\r'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}
