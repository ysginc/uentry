//! Profile system for configuration presets.
//!
//! This module provides:
//! - Profile discovery from /etc/uentry/profiles/*.yaml (TASK-043)
//! - Built-in profiles: baseline, k8s, web, worker (TASK-044-047)
//! - Profile merging with user config (TASK-048)

use crate::config::schema::Config;
use crate::error::{Result, UentryError};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const PROFILES_DIR: &str = "/etc/uentry/profiles";

/// Built-in profile definitions.
const BASELINE_PROFILE: &str = r#"
runtime:
  signal_forward: true

security:
  no_new_privs: false
  allow_root: false
"#;

const K8S_PROFILE: &str = r#"
runtime:
  signal_forward: true
  env_allow:
    - KUBERNETES_*
    - POD_*
    - CONTAINER_*

secrets:
  file_to_env:
    - file: /var/run/secrets/kubernetes.io/serviceaccount/token
      env_var: KUBERNETES_SERVICE_ACCOUNT_TOKEN
      optional: true

lifecycle:
  shutdown_timeout_secs: 30
"#;

const WEB_PROFILE: &str = r#"
runtime:
  signal_forward: true
  env_allow:
    - PORT
    - HOST
    - BIND

app:
  readiness:
    initial_delay_secs: 5
    interval_secs: 10
    http_get:
      path: /health
      port: 8080

lifecycle:
  startup_grace_secs: 10
  shutdown_timeout_secs: 30
"#;

const WORKER_PROFILE: &str = r#"
runtime:
  signal_forward: true

lifecycle:
  startup_grace_secs: 30
  shutdown_timeout_secs: 60
"#;

/// Profile manager for loading and merging profiles.
#[derive(Debug, Clone)]
pub struct ProfileManager {
    profiles_dir: PathBuf,
    cache: std::collections::HashMap<String, Config>,
}

impl ProfileManager {
    /// Create a new profile manager with default profiles directory.
    pub fn new() -> Self {
        Self {
            profiles_dir: PathBuf::from(PROFILES_DIR),
            cache: std::collections::HashMap::new(),
        }
    }

    /// Create a profile manager with a custom profiles directory.
    pub fn with_dir(path: PathBuf) -> Self {
        Self {
            profiles_dir: path,
            cache: std::collections::HashMap::new(),
        }
    }

    /// Load a profile by name.
    ///
    /// Built-in profiles (baseline, k8s, web, worker) are always available.
    /// Custom profiles are loaded from the profiles directory.
    pub fn load(&mut self, name: &str) -> Result<Config> {
        if let Some(cached) = self.cache.get(name) {
            debug!("Using cached profile: {}", name);
            return Ok(cached.clone());
        }

        let config = if let Some(builtin) = Self::load_builtin(name)? {
            info!("Loaded built-in profile: {}", name);
            builtin
        } else {
            let path = self.profiles_dir.join(format!("{}.yaml", name));
            self.load_from_file(&path)?
        };

        self.cache.insert(name.to_string(), config.clone());
        Ok(config)
    }

    /// Load a built-in profile.
    fn load_builtin(name: &str) -> Result<Option<Config>> {
        let yaml = match name {
            "baseline" => BASELINE_PROFILE,
            "k8s" => K8S_PROFILE,
            "web" => WEB_PROFILE,
            "worker" => WORKER_PROFILE,
            _ => return Ok(None),
        };

        let config: Config = serde_yaml::from_str(yaml).map_err(|e| {
            UentryError::Config(format!(
                "Failed to parse built-in profile '{}': {}",
                name, e
            ))
        })?;

        Ok(Some(config))
    }

    /// Load a profile from a file.
    fn load_from_file(&self, path: &Path) -> Result<Config> {
        if !path.exists() {
            return Err(UentryError::Config(format!(
                "Profile not found: {:?}",
                path
            )));
        }

        debug!("Loading profile from: {:?}", path);
        let contents = std::fs::read_to_string(path).map_err(|e| {
            UentryError::Config(format!("Failed to read profile {:?}: {}", path, e))
        })?;

        let config: Config = serde_yaml::from_str(&contents).map_err(|e| {
            UentryError::Config(format!("Failed to parse profile {:?}: {}", path, e))
        })?;

        Ok(config)
    }

    /// Merge a profile into a base configuration.
    ///
    /// Profile values are used as defaults; base config values take precedence.
    pub fn merge_into(base: &mut Config, profile: &Config) {
        if base.runtime.user.is_none() && profile.runtime.user.is_some() {
            base.runtime.user = profile.runtime.user.clone();
        }
        if base.runtime.group.is_none() && profile.runtime.group.is_some() {
            base.runtime.group = profile.runtime.group.clone();
        }
        if base.runtime.supplementary_groups.is_empty()
            && !profile.runtime.supplementary_groups.is_empty()
        {
            base.runtime.supplementary_groups = profile.runtime.supplementary_groups.clone();
        }
        if base.runtime.ensure_dirs.is_empty() && !profile.runtime.ensure_dirs.is_empty() {
            base.runtime.ensure_dirs = profile.runtime.ensure_dirs.clone();
        }
        if base.runtime.cwd.is_none() && profile.runtime.cwd.is_some() {
            base.runtime.cwd = profile.runtime.cwd.clone();
        }
        if base.runtime.umask.is_none() && profile.runtime.umask.is_some() {
            base.runtime.umask = profile.runtime.umask.clone();
        }

        for (key, value) in &profile.runtime.env {
            if !base.runtime.env.contains_key(key) {
                base.runtime.env.insert(key.clone(), value.clone());
            }
        }

        for key in &profile.runtime.env_allow {
            if !base.runtime.env_allow.contains(key) {
                base.runtime.env_allow.push(key.clone());
            }
        }

        for key in &profile.runtime.env_deny {
            if !base.runtime.env_deny.contains(key) {
                base.runtime.env_deny.push(key.clone());
            }
        }

        if base.security.writable_paths.is_empty() && !profile.security.writable_paths.is_empty() {
            base.security.writable_paths = profile.security.writable_paths.clone();
        }

        for fte in &profile.secrets.file_to_env {
            if !base
                .secrets
                .file_to_env
                .iter()
                .any(|f| f.env_var == fte.env_var)
            {
                base.secrets.file_to_env.push(fte.clone());
            }
        }

        for etf in &profile.secrets.env_to_file {
            if !base
                .secrets
                .env_to_file
                .iter()
                .any(|f| f.env_var == etf.env_var)
            {
                base.secrets.env_to_file.push(etf.clone());
            }
        }

        if base.lifecycle.pre_start.is_none() && profile.lifecycle.pre_start.is_some() {
            base.lifecycle.pre_start = profile.lifecycle.pre_start.clone();
        }
        if base.lifecycle.post_stop.is_none() && profile.lifecycle.post_stop.is_some() {
            base.lifecycle.post_stop = profile.lifecycle.post_stop.clone();
        }
        if base.lifecycle.shutdown_timeout_secs == 30
            && profile.lifecycle.shutdown_timeout_secs != 30
        {
            base.lifecycle.shutdown_timeout_secs = profile.lifecycle.shutdown_timeout_secs;
        }
        if base.lifecycle.startup_grace_secs == 0 && profile.lifecycle.startup_grace_secs > 0 {
            base.lifecycle.startup_grace_secs = profile.lifecycle.startup_grace_secs;
        }

        if base.app.name.is_none() && profile.app.name.is_some() {
            base.app.name = profile.app.name.clone();
        }
        if base.app.healthcheck.is_none() && profile.app.healthcheck.is_some() {
            base.app.healthcheck = profile.app.healthcheck.clone();
        }
        if base.app.readiness.is_none() && profile.app.readiness.is_some() {
            base.app.readiness = profile.app.readiness.clone();
        }
    }

    /// Discover available profiles (built-in + filesystem).
    pub fn discover(&self) -> Vec<String> {
        let mut profiles = vec![
            "baseline".to_string(),
            "k8s".to_string(),
            "web".to_string(),
            "worker".to_string(),
        ];

        if let Ok(entries) = std::fs::read_dir(&self.profiles_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Some(profile_name) = name.strip_suffix(".yaml") {
                        if !profiles.contains(&profile_name.to_string()) {
                            profiles.push(profile_name.to_string());
                        }
                    }
                }
            }
        }

        profiles
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Load and merge a profile into the configuration.
pub fn apply_profile(config: &mut Config, profile_name: &str) -> Result<()> {
    let mut manager = ProfileManager::new();
    let profile = manager.load(profile_name)?;
    info!("Applying profile: {}", profile_name);
    ProfileManager::merge_into(config, &profile);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_manager_new() {
        let manager = ProfileManager::new();
        assert_eq!(manager.profiles_dir, PathBuf::from(PROFILES_DIR));
        assert!(manager.cache.is_empty());
    }

    #[test]
    fn test_load_builtin_baseline() {
        let mut manager = ProfileManager::new();
        let config = manager.load("baseline").unwrap();
        assert!(config.runtime.signal_forward);
    }

    #[test]
    fn test_load_builtin_k8s() {
        let mut manager = ProfileManager::new();
        let config = manager.load("k8s").unwrap();
        assert!(config
            .runtime
            .env_allow
            .contains(&"KUBERNETES_*".to_string()));
        assert!(!config.secrets.file_to_env.is_empty());
    }

    #[test]
    fn test_load_builtin_web() {
        let mut manager = ProfileManager::new();
        let config = manager.load("web").unwrap();
        assert!(config.app.readiness.is_some());
        assert_eq!(config.lifecycle.startup_grace_secs, 10);
    }

    #[test]
    fn test_load_builtin_worker() {
        let mut manager = ProfileManager::new();
        let config = manager.load("worker").unwrap();
        assert_eq!(config.lifecycle.startup_grace_secs, 30);
        assert_eq!(config.lifecycle.shutdown_timeout_secs, 60);
    }

    #[test]
    fn test_load_builtin_not_found() {
        let mut manager = ProfileManager::new();
        let result = manager.load("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_builtin_cached() {
        let mut manager = ProfileManager::new();
        let _ = manager.load("baseline").unwrap();
        assert!(manager.cache.contains_key("baseline"));

        let config = manager.load("baseline").unwrap();
        assert!(config.runtime.signal_forward);
    }

    #[test]
    fn test_merge_into_preserves_base_values() {
        let mut base = Config::default();
        base.runtime.strict = true;
        base.runtime.user = Some("customuser".to_string());

        let profile = ProfileManager::new().load("baseline").unwrap();
        ProfileManager::merge_into(&mut base, &profile);

        assert!(base.runtime.strict);
        assert_eq!(base.runtime.user, Some("customuser".to_string()));
    }

    #[test]
    fn test_merge_into_applies_profile_defaults() {
        let mut base = Config::default();
        base.runtime.signal_forward = true;

        let mut profile = Config::default();
        profile.runtime.user = Some("profileuser".to_string());
        profile.runtime.umask = Some("022".to_string());

        ProfileManager::merge_into(&mut base, &profile);

        assert_eq!(base.runtime.user, Some("profileuser".to_string()));
        assert_eq!(base.runtime.umask, Some("022".to_string()));
    }

    #[test]
    fn test_merge_into_env() {
        let mut base = Config::default();
        base.runtime
            .env
            .insert("CUSTOM".to_string(), "value".to_string());

        let mut profile = Config::default();
        profile
            .runtime
            .env
            .insert("PROFILE_VAR".to_string(), "profile".to_string());
        profile
            .runtime
            .env
            .insert("CUSTOM".to_string(), "should_not_override".to_string());

        ProfileManager::merge_into(&mut base, &profile);

        assert_eq!(base.runtime.env.get("CUSTOM"), Some(&"value".to_string()));
        assert_eq!(
            base.runtime.env.get("PROFILE_VAR"),
            Some(&"profile".to_string())
        );
    }

    #[test]
    fn test_discover_includes_builtins() {
        let manager = ProfileManager::new();
        let profiles = manager.discover();

        assert!(profiles.contains(&"baseline".to_string()));
        assert!(profiles.contains(&"k8s".to_string()));
        assert!(profiles.contains(&"web".to_string()));
        assert!(profiles.contains(&"worker".to_string()));
    }

    #[test]
    fn test_apply_profile() {
        let mut config = Config::default();
        apply_profile(&mut config, "web").unwrap();

        assert!(config.app.readiness.is_some());
        assert_eq!(config.lifecycle.startup_grace_secs, 10);
    }
}
