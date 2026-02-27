//! Lifecycle management module.
//!
//! This module provides:
//! - Phase coordinator for ordered execution (TASK-052, TASK-053)
//! - Hook system with discovery and execution (TASK-054-057)
//! - Pre/post command execution (TASK-058-060)

pub mod coordinator;
pub mod hooks;

pub use coordinator::LifecycleCoordinator;
pub use hooks::{Hook, HookExecutor, HookFailurePolicy};
