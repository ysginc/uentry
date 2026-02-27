//! # uentry - A minimal init system for containers
//!
//! Universal container entrypoint with PID 1 awareness, signal forwarding,
//! and declarative configuration.
//!
//! ## Features
//!
//! - **PID 1 aware**: Signal forwarding (SIGTERM, SIGINT) and zombie reaping
//! - **Declarative config**: YAML files + environment variables + CLI flags
//! - **Structured logging**: JSON or plain text via `UENTRY_LOG_FORMAT`
//! - **Minimal binary**: ~1.3MB with release optimizations
//! - **Security**: Strict mode, privilege dropping, preflight checks
//! - **Secrets**: File-to-env and env-to-file injection with redaction
//! - **Lifecycle**: Hooks, pre/post commands, readiness probes
//! - **Profiles**: Built-in and custom configuration profiles

pub mod cli;
pub mod config;
pub mod error;
pub mod exec;
pub mod health;
pub mod lifecycle;
pub mod logging;
pub mod pid1;
pub mod security;

pub use cli::Cli;
pub use config::Config;
pub use error::{Result, UentryError};
