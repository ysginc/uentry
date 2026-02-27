//! Security module for uentry.
//!
//! This module provides security features including:
//! - Preflight security checks
//! - Privilege dropping
//! - Strict mode enforcement
//! - Filesystem preparation
//! - Secrets management

pub mod filesystem;
pub mod preflight;
pub mod privileges;
pub mod secrets;
pub mod strict;

pub use filesystem::{DirSpec, FilesystemPrep};
pub use preflight::{PreflightCheck, PreflightReport};
pub use privileges::{drop_privileges, PrivilegeConfig};
pub use secrets::{EnvToFile, FileToEnv, SecretsManager};
pub use strict::StrictMode;
