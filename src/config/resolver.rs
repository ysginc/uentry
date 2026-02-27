//! Configuration resolution with precedence handling.
//!
//! This module provides the [`resolve`] function which loads configuration
//! from multiple sources with proper precedence:
//!
//! 1. Environment variables (highest priority)
//! 2. Configuration file
//! 3. Defaults (lowest priority)

use crate::config::env::merge_env_into_config;
use crate::config::file::{load_default, load_from_file};
use crate::config::schema::Config;
use crate::error::Result;
use std::path::Path;

/// Resolve configuration from all sources.
///
/// Configuration is loaded with the following precedence (highest to lowest):
/// 1. Environment variables (UENTRY_*)
/// 2. Specified config file (via `config_path`)
/// 3. Default config file (/etc/uentry/config.yaml)
/// 4. Default values
///
/// # Errors
///
/// Returns an error if a specified config file cannot be read or parsed.
pub fn resolve(config_path: Option<&Path>) -> Result<Config> {
    let mut config = if let Some(path) = config_path {
        load_from_file(path)?
    } else if let Some(result) = load_default() {
        result?
    } else {
        Config::default()
    };

    merge_env_into_config(&mut config);

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    fn clear_uentry_env() {
        for (key, _) in env::vars() {
            if key.starts_with("UENTRY_") {
                env::remove_var(&key);
            }
        }
    }

    #[test]
    #[serial]
    fn test_resolve_defaults() {
        clear_uentry_env();
        let config = resolve(None).unwrap();
        assert!(!config.runtime.strict);
        assert!(config.runtime.signal_forward);
    }

    #[test]
    #[serial]
    fn test_resolve_env_override() {
        clear_uentry_env();
        env::set_var("UENTRY_STRICT", "true");
        env::set_var("UENTRY_USER", "testuser");

        let config = resolve(None).unwrap();

        assert!(config.runtime.strict);
        assert_eq!(config.runtime.user, Some("testuser".to_string()));

        env::remove_var("UENTRY_STRICT");
        env::remove_var("UENTRY_USER");
    }

    #[test]
    fn test_resolve_missing_config_file() {
        let result = resolve(Some(Path::new("/nonexistent/config.yaml")));
        assert!(result.is_err());
    }
}
