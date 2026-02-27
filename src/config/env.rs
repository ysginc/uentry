//! Environment variable configuration loading.
//!
//! This module handles reading configuration from environment variables
//! prefixed with `UENTRY_`.

use crate::config::schema::Config;
use std::env;

/// Load configuration from environment variables.
///
/// Reads the following environment variables:
///
/// | Variable | Config Field |
/// |----------|-------------|
/// | `UENTRY_STRICT` | `runtime.strict` |
/// | `UENTRY_USER` | `runtime.user` |
/// | `UENTRY_GROUP` | `runtime.group` |
/// | `UENTRY_SUPPLEMENTARY_GROUPS` | `runtime.supplementary_groups` (colon-separated) |
/// | `UENTRY_ENSURE_DIR` | `runtime.ensure_dirs` (colon-separated) |
/// | `UENTRY_SIGNAL_FORWARD` | `runtime.signal_forward` |
/// | `UENTRY_UMASK` | `runtime.umask` |
/// | `UENTRY_NO_NEW_PRIVS` | `security.no_new_privs` |
/// | `UENTRY_ALLOW_ROOT` | `security.allow_root` |
/// | `UENTRY_WRITABLE_PATHS` | `security.writable_paths` (colon-separated) |
/// | `UENTRY_PROFILE` | `app.profile` |
/// | `UENTRY_APP_NAME` | `app.name` |
/// | `UENTRY_ENV_*` | `runtime.env.*` |
pub fn load_from_env() -> Config {
    let mut config = Config::default();

    let runtime = &mut config.runtime;
    let app = &mut config.app;
    let security = &mut config.security;

    if let Ok(val) = env::var("UENTRY_STRICT") {
        runtime.strict = val.parse().unwrap_or(false);
    }

    if let Ok(val) = env::var("UENTRY_USER") {
        runtime.user = Some(val);
    }

    if let Ok(val) = env::var("UENTRY_GROUP") {
        runtime.group = Some(val);
    }

    if let Ok(val) = env::var("UENTRY_SUPPLEMENTARY_GROUPS") {
        runtime.supplementary_groups = val
            .split(':')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
    }

    if let Ok(val) = env::var("UENTRY_ENSURE_DIR") {
        runtime.ensure_dirs = val
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|p| crate::config::schema::DirConfig::new(std::path::PathBuf::from(p)))
            .collect();
    }

    if let Ok(val) = env::var("UENTRY_SIGNAL_FORWARD") {
        runtime.signal_forward = val.parse().unwrap_or(true);
    }

    if let Ok(val) = env::var("UENTRY_UMASK") {
        runtime.umask = Some(val);
    }

    for (key, value) in env::vars() {
        if let Some(env_key) = key.strip_prefix("UENTRY_ENV_") {
            runtime.env.insert(env_key.to_string(), value);
        }
    }

    if let Ok(val) = env::var("UENTRY_PROFILE") {
        app.profile = Some(val);
    }

    if let Ok(val) = env::var("UENTRY_APP_NAME") {
        app.name = Some(val);
    }

    if let Ok(val) = env::var("UENTRY_NO_NEW_PRIVS") {
        security.no_new_privs = val.parse().unwrap_or(false);
    }

    if let Ok(val) = env::var("UENTRY_ALLOW_ROOT") {
        security.allow_root = val.parse().unwrap_or(false);
    }

    if let Ok(val) = env::var("UENTRY_WRITABLE_PATHS") {
        security.writable_paths = val
            .split(':')
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .collect();
    }

    config
}

/// Merge environment variables into an existing config.
///
/// Environment values override existing config values when set.
pub fn merge_env_into_config(config: &mut Config) {
    let env_config = load_from_env();

    if env_config.runtime.strict {
        config.runtime.strict = true;
    }
    if env_config.runtime.user.is_some() {
        config.runtime.user = env_config.runtime.user;
    }
    if env_config.runtime.group.is_some() {
        config.runtime.group = env_config.runtime.group;
    }
    if !env_config.runtime.supplementary_groups.is_empty() {
        config.runtime.supplementary_groups = env_config.runtime.supplementary_groups;
    }
    if !env_config.runtime.ensure_dirs.is_empty() {
        config.runtime.ensure_dirs = env_config.runtime.ensure_dirs;
    }
    if !env_config.runtime.signal_forward {
        config.runtime.signal_forward = false;
    }
    if env_config.runtime.umask.is_some() {
        config.runtime.umask = env_config.runtime.umask;
    }
    for (key, value) in env_config.runtime.env {
        config.runtime.env.insert(key, value);
    }
    if env_config.app.profile.is_some() {
        config.app.profile = env_config.app.profile;
    }
    if env_config.app.name.is_some() {
        config.app.name = env_config.app.name;
    }
    if env_config.security.no_new_privs {
        config.security.no_new_privs = true;
    }
    if env_config.security.allow_root {
        config.security.allow_root = true;
    }
    if !env_config.security.writable_paths.is_empty() {
        config.security.writable_paths = env_config.security.writable_paths;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::path::PathBuf;

    fn clear_uentry_env() {
        for (key, _) in env::vars() {
            if key.starts_with("UENTRY_") {
                env::remove_var(&key);
            }
        }
    }

    #[test]
    #[serial]
    fn test_load_from_env_empty() {
        clear_uentry_env();
        let config = load_from_env();
        assert!(!config.runtime.strict);
        assert!(config.runtime.user.is_none());
        assert!(config.runtime.group.is_none());
        assert!(config.runtime.supplementary_groups.is_empty());
        assert!(config.runtime.ensure_dirs.is_empty());
        assert!(config.runtime.signal_forward);
        assert!(config.runtime.umask.is_none());
        assert!(!config.security.no_new_privs);
        assert!(!config.security.allow_root);
        assert!(config.security.writable_paths.is_empty());
    }

    #[test]
    #[serial]
    fn test_load_from_env_strict() {
        clear_uentry_env();
        env::set_var("UENTRY_STRICT", "true");
        let config = load_from_env();
        assert!(config.runtime.strict);
        env::remove_var("UENTRY_STRICT");
    }

    #[test]
    #[serial]
    fn test_load_from_env_user() {
        clear_uentry_env();
        env::set_var("UENTRY_USER", "appuser");
        let config = load_from_env();
        assert_eq!(config.runtime.user, Some("appuser".to_string()));
        env::remove_var("UENTRY_USER");
    }

    #[test]
    #[serial]
    fn test_load_from_env_group() {
        clear_uentry_env();
        env::set_var("UENTRY_GROUP", "appgroup");
        let config = load_from_env();
        assert_eq!(config.runtime.group, Some("appgroup".to_string()));
        env::remove_var("UENTRY_GROUP");
    }

    #[test]
    #[serial]
    fn test_load_from_env_supplementary_groups() {
        clear_uentry_env();
        env::set_var("UENTRY_SUPPLEMENTARY_GROUPS", "docker:sudo");
        let config = load_from_env();
        assert_eq!(config.runtime.supplementary_groups, vec!["docker", "sudo"]);
        env::remove_var("UENTRY_SUPPLEMENTARY_GROUPS");
    }

    #[test]
    #[serial]
    fn test_load_from_env_umask() {
        clear_uentry_env();
        env::set_var("UENTRY_UMASK", "022");
        let config = load_from_env();
        assert_eq!(config.runtime.umask, Some("022".to_string()));
        env::remove_var("UENTRY_UMASK");
    }

    #[test]
    #[serial]
    fn test_load_from_env_no_new_privs() {
        clear_uentry_env();
        env::set_var("UENTRY_NO_NEW_PRIVS", "true");
        let config = load_from_env();
        assert!(config.security.no_new_privs);
        env::remove_var("UENTRY_NO_NEW_PRIVS");
    }

    #[test]
    #[serial]
    fn test_load_from_env_allow_root() {
        clear_uentry_env();
        env::set_var("UENTRY_ALLOW_ROOT", "true");
        let config = load_from_env();
        assert!(config.security.allow_root);
        env::remove_var("UENTRY_ALLOW_ROOT");
    }

    #[test]
    #[serial]
    fn test_load_from_env_writable_paths() {
        clear_uentry_env();
        env::set_var("UENTRY_WRITABLE_PATHS", "/tmp:/var/log");
        let config = load_from_env();
        assert_eq!(
            config.security.writable_paths,
            vec![PathBuf::from("/tmp"), PathBuf::from("/var/log")]
        );
        env::remove_var("UENTRY_WRITABLE_PATHS");
    }

    #[test]
    #[serial]
    fn test_merge_env_into_config_preserves_defaults() {
        clear_uentry_env();
        let mut config = Config::default();
        config.runtime.user = Some("original".to_string());

        merge_env_into_config(&mut config);

        assert_eq!(config.runtime.user, Some("original".to_string()));
    }
}
