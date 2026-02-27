//! Error types for uentry.
//!
//! This module defines the [`UentryError`] enum for all error conditions
//! and a [`Result`] type alias for convenience.

use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// The main error type for uentry operations.
///
/// # Error Categories
///
/// - **Config**: Configuration parsing and loading errors
/// - **Exec**: Process execution errors
/// - **Security**: Security policy violations
/// - **Io**: General I/O errors
/// - **Signal**: Signal handling errors
#[derive(Debug, Error)]
pub enum UentryError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Failed to read config file '{path}': {source}")]
    ConfigFileRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Failed to parse config file '{path}': {message}")]
    ConfigFileParse { path: PathBuf, message: String },

    #[error("Execution error: {0}")]
    Exec(String),

    #[error("Failed to execute '{command}': {source}")]
    ExecFailed {
        command: String,
        #[source]
        source: io::Error,
    },

    #[error("Security error: {0}")]
    Security(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Missing required configuration: {0}")]
    MissingConfig(String),

    #[error("Signal handling error: {0}")]
    Signal(String),
}

/// A specialized `Result` type for uentry operations.
pub type Result<T> = std::result::Result<T, UentryError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_config() {
        let err = UentryError::Config("test error".to_string());
        assert_eq!(err.to_string(), "Configuration error: test error");
    }

    #[test]
    fn test_error_display_exec() {
        let err = UentryError::Exec("failed to run".to_string());
        assert_eq!(err.to_string(), "Execution error: failed to run");
    }

    #[test]
    fn test_error_display_security() {
        let err = UentryError::Security("root forbidden".to_string());
        assert_eq!(err.to_string(), "Security error: root forbidden");
    }

    #[test]
    fn test_error_display_invalid_argument() {
        let err = UentryError::InvalidArgument("bad value".to_string());
        assert_eq!(err.to_string(), "Invalid argument: bad value");
    }

    #[test]
    fn test_error_display_missing_config() {
        let err = UentryError::MissingConfig("user field".to_string());
        assert_eq!(
            err.to_string(),
            "Missing required configuration: user field"
        );
    }

    #[test]
    fn test_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err: UentryError = io_err.into();
        assert!(matches!(err, UentryError::Io(_)));
    }

    #[test]
    fn test_config_file_read_error() {
        let path = PathBuf::from("/etc/uentry/config.yaml");
        let source = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let err = UentryError::ConfigFileRead {
            path: path.clone(),
            source,
        };
        let msg = err.to_string();
        assert!(msg.contains("/etc/uentry/config.yaml"));
        assert!(msg.contains("access denied"));
    }

    #[test]
    fn test_config_file_parse_error() {
        let err = UentryError::ConfigFileParse {
            path: PathBuf::from("/etc/uentry/config.yaml"),
            message: "invalid YAML at line 5".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid YAML at line 5"));
    }

    #[test]
    fn test_exec_failed_error() {
        let err = UentryError::ExecFailed {
            command: "/bin/bash".to_string(),
            source: io::Error::new(io::ErrorKind::NotFound, "no such file"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/bin/bash"));
        assert!(msg.contains("no such file"));
    }
}
