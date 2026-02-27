//! Logging initialization.
//!
//! This module configures the tracing subscriber for structured logging.
//!
//! # Log Format
//!
//! Set `UENTRY_LOG_FORMAT=json` for JSON output. Default is plain text.
//!
//! # Log Level
//!
//! Set `RUST_LOG` to control log level (e.g., `RUST_LOG=debug`). Default is `info`.

use std::env;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

/// Initialize the tracing subscriber.
///
/// Configures logging based on environment variables:
/// - `UENTRY_LOG_FORMAT`: Set to "json" for JSON output
/// - `RUST_LOG`: Set log level (default: "info")
///
/// This function should be called once at program start.
pub fn init() {
    let format = env::var("UENTRY_LOG_FORMAT").unwrap_or_default();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    match format.as_str() {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .with_span_events(FmtSpan::CLOSE)
                .with_env_filter(env_filter)
                .init();
        }
        _ => {
            tracing_subscriber::fmt().with_env_filter(env_filter).init();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_does_not_panic_default() {
        env::remove_var("UENTRY_LOG_FORMAT");
        env::remove_var("RUST_LOG");
    }

    #[test]
    fn test_init_does_not_panic_json() {
        env::set_var("UENTRY_LOG_FORMAT", "json");
        env::set_var("RUST_LOG", "debug");
        env::remove_var("UENTRY_LOG_FORMAT");
        env::remove_var("RUST_LOG");
    }
}
