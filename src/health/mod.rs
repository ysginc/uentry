//! Health check module.
//!
//! This module provides:
//! - Readiness probes (TASK-061)
//! - Startup grace period (TASK-062)
//! - Health check before signaling ready (TASK-063)

pub mod readiness;

pub use readiness::{ProbeResult, ReadinessChecker};
