//! YAML configuration file loading.
//!
//! This module handles loading configuration from YAML files.

use crate::config::schema::Config;
use crate::error::{Result, UentryError};
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_PATH: &str = "/etc/uentry/config.yaml";

/// Load configuration from a YAML file.
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be read
/// - The YAML content is invalid
/// - The configuration is malformed
pub fn load_from_file(path: &Path) -> Result<Config> {
    let contents = std::fs::read_to_string(path).map_err(|source| UentryError::ConfigFileRead {
        path: path.to_path_buf(),
        source,
    })?;

    parse_yaml(&contents, path)
}

/// Parse YAML configuration content.
///
/// # Errors
///
/// Returns an error if the YAML is invalid or doesn't match the config schema.
pub fn parse_yaml(contents: &str, path: &Path) -> Result<Config> {
    serde_yaml::from_str(contents).map_err(|e| UentryError::ConfigFileParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Get the default configuration file path.
pub fn default_config_path() -> PathBuf {
    PathBuf::from(DEFAULT_CONFIG_PATH)
}

/// Load configuration from the default path if it exists.
///
/// Returns `None` if the default config file doesn't exist.
pub fn load_default() -> Option<Result<Config>> {
    let path = default_config_path();
    if path.exists() {
        Some(load_from_file(&path))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_path() {
        assert_eq!(
            default_config_path(),
            PathBuf::from("/etc/uentry/config.yaml")
        );
    }

    #[test]
    fn test_parse_yaml_empty() {
        let yaml = "";
        let config = parse_yaml(yaml, Path::new("test.yaml")).unwrap();
        assert!(!config.runtime.strict);
    }

    #[test]
    fn test_parse_yaml_valid() {
        let yaml = r#"
runtime:
  strict: true
  user: appuser
"#;
        let config = parse_yaml(yaml, Path::new("test.yaml")).unwrap();
        assert!(config.runtime.strict);
        assert_eq!(config.runtime.user, Some("appuser".to_string()));
    }

    #[test]
    fn test_parse_yaml_invalid() {
        let yaml = "not: valid: yaml: :::";
        let result = parse_yaml(yaml, Path::new("test.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, UentryError::ConfigFileParse { .. }));
    }

    #[test]
    fn test_parse_yaml_full_config() {
        let yaml = r#"
runtime:
  strict: true
  user: appuser
  ensure_dirs:
    - path: /var/run/app
    - path: /var/log/app
  env:
    LOG_LEVEL: info
    DB_HOST: localhost
  signal_forward: false

app:
  name: myapp
  profile: production
  healthcheck:
    command: /bin/health
    interval_secs: 60
    timeout_secs: 10
    retries: 5
"#;
        let config = parse_yaml(yaml, Path::new("test.yaml")).unwrap();

        assert!(config.runtime.strict);
        assert_eq!(config.runtime.user, Some("appuser".to_string()));
        assert_eq!(config.runtime.ensure_dirs.len(), 2);
        assert_eq!(
            config.runtime.ensure_dirs[0].path,
            std::path::PathBuf::from("/var/run/app")
        );
        assert_eq!(
            config.runtime.ensure_dirs[1].path,
            std::path::PathBuf::from("/var/log/app")
        );
        assert_eq!(
            config.runtime.env.get("LOG_LEVEL"),
            Some(&"info".to_string())
        );
        assert!(!config.runtime.signal_forward);

        assert_eq!(config.app.name, Some("myapp".to_string()));
        assert_eq!(config.app.profile, Some("production".to_string()));

        let hc = config.app.healthcheck.unwrap();
        assert_eq!(hc.command, "/bin/health");
        assert_eq!(hc.interval_secs, 60);
        assert_eq!(hc.timeout_secs, 10);
        assert_eq!(hc.retries, 5);
    }

    #[test]
    fn test_load_from_file_not_found() {
        let result = load_from_file(Path::new("/nonexistent/path/config.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, UentryError::ConfigFileRead { .. }));
    }

    #[test]
    fn test_load_default_missing() {
        let result = load_default();
        assert!(result.is_none() || default_config_path().exists());
    }
}
