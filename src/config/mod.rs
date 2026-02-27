//! Configuration management.
//!
//! This module handles loading and merging configuration from:
//! - YAML files
//! - Environment variables
//! - Profiles
//! - Default values

pub mod env;
pub mod file;
pub mod metadata;
pub mod profile;
pub mod resolver;
pub mod schema;

pub use schema::Config;
