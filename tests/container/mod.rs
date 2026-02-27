//! Container integration tests for uentry.
//!
//! These tests validate uentry behavior in real container environments.
//!
//! ## Running Container Tests
//!
//! Container tests require Docker to be available. They build and run
//! actual containers with uentry as the entrypoint.
//!
//! ```bash
//! # Build uentry first (static binary required)
//! cargo build --release --target x86_64-unknown-linux-musl
//!
//! # Or use local dynamic binary
//! cargo build --release
//!
//! # Run container tests
//! cargo test --test container_tests
//! ```

pub mod fixtures;

pub mod scenarios {
    pub mod basic;
    pub mod derived;
    pub mod lifecycle;
    pub mod profiles;
    pub mod security;
}
