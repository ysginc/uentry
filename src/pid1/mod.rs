//! PID 1 functionality for containers.
//!
//! This module provides signal handling and process management
//! for when uentry runs as PID 1 in a container.

pub mod signal;

pub use signal::SignalHandler;
