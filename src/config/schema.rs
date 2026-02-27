//! Configuration schema definitions.
//!
//! This module defines the structure of uentry's configuration,
//! including runtime settings, app metadata, security settings, health checks,
//! and lifecycle hooks.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Root configuration structure.
///
/// Contains all configuration sections for uentry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub app: AppConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub secrets: SecretsConfig,
    #[serde(default)]
    pub lifecycle: LifecycleConfig,
}

/// Runtime configuration for process execution.
///
/// Controls how uentry manages the child process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub supplementary_groups: Vec<String>,
    #[serde(default)]
    pub ensure_dirs: Vec<DirConfig>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub env_allow: Vec<String>,
    #[serde(default)]
    pub env_deny: Vec<String>,
    #[serde(default)]
    pub signal_forward: bool,
    #[serde(default)]
    pub umask: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            strict: false,
            user: None,
            group: None,
            supplementary_groups: Vec::new(),
            ensure_dirs: Vec::new(),
            env: HashMap::new(),
            env_allow: Vec::new(),
            env_deny: Vec::new(),
            signal_forward: true,
            umask: None,
            cwd: None,
        }
    }
}

/// Directory configuration for filesystem preparation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

impl DirConfig {
    /// Create a new directory configuration.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            mode: None,
            owner: None,
            group: None,
        }
    }

    /// Get the mode as a u32 (parsed from octal string).
    pub fn mode_value(&self) -> Option<u32> {
        self.mode
            .as_ref()
            .and_then(|m| u32::from_str_radix(m, 8).ok())
    }
}

impl From<PathBuf> for DirConfig {
    fn from(path: PathBuf) -> Self {
        Self::new(path)
    }
}

/// Security configuration for strict mode and privilege management.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub no_new_privs: bool,
    #[serde(default)]
    pub allow_root: bool,
    #[serde(default)]
    pub writable_paths: Vec<PathBuf>,
}

/// Secrets configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretsConfig {
    #[serde(default)]
    pub file_to_env: Vec<FileToEnvConfig>,
    #[serde(default)]
    pub env_to_file: Vec<EnvToFileConfig>,
}

/// File to environment variable mapping configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileToEnvConfig {
    pub file: PathBuf,
    pub env_var: String,
    #[serde(default)]
    pub optional: bool,
}

/// Environment variable to file mapping configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvToFileConfig {
    pub env_var: String,
    pub file: PathBuf,
    #[serde(default = "default_secret_file_mode")]
    pub mode: String,
}

fn default_secret_file_mode() -> String {
    "600".to_string()
}

impl EnvToFileConfig {
    /// Get the mode as a u32 (parsed from octal string).
    pub fn mode_value(&self) -> u32 {
        u32::from_str_radix(&self.mode, 8).unwrap_or(0o600)
    }
}

/// Lifecycle configuration for hooks and startup behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    #[serde(default)]
    pub pre_start: Option<HookConfig>,
    #[serde(default)]
    pub post_stop: Option<HookConfig>,
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_secs: u64,
    #[serde(default = "default_startup_grace")]
    pub startup_grace_secs: u64,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            pre_start: None,
            post_stop: None,
            shutdown_timeout_secs: default_shutdown_timeout(),
            startup_grace_secs: default_startup_grace(),
        }
    }
}

fn default_shutdown_timeout() -> u64 {
    30
}

fn default_startup_grace() -> u64 {
    0
}

/// Hook configuration for lifecycle events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub on_failure: HookFailurePolicy,
}

fn default_hook_timeout() -> u64 {
    60
}

/// Hook failure policy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookFailurePolicy {
    #[default]
    Fail,
    Warn,
    Ignore,
}

/// Application metadata configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub healthcheck: Option<HealthcheckConfig>,
    #[serde(default)]
    pub readiness: Option<ReadinessConfig>,
}

/// Health check configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthcheckConfig {
    #[serde(default = "default_healthcheck_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_healthcheck_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_healthcheck_retries")]
    pub retries: u32,
    pub command: String,
}

fn default_healthcheck_interval() -> u64 {
    30
}

fn default_healthcheck_timeout() -> u64 {
    5
}

fn default_healthcheck_retries() -> u32 {
    3
}

/// Readiness probe configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessConfig {
    #[serde(default = "default_readiness_initial_delay")]
    pub initial_delay_secs: u64,
    #[serde(default = "default_readiness_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_readiness_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_readiness_retries")]
    pub retries: u32,
    #[serde(flatten)]
    pub probe: ProbeConfig,
}

fn default_readiness_initial_delay() -> u64 {
    0
}

fn default_readiness_interval() -> u64 {
    10
}

fn default_readiness_timeout() -> u64 {
    5
}

fn default_readiness_retries() -> u32 {
    3
}

/// Probe configuration for health/readiness checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProbeConfig {
    Http { http_get: HttpProbeConfig },
    Tcp { tcp_socket: TcpProbeConfig },
    Exec { exec: ExecProbeConfig },
}

/// HTTP probe configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpProbeConfig {
    pub path: String,
    pub port: u16,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub scheme: Option<String>,
}

/// TCP probe configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpProbeConfig {
    pub port: u16,
    #[serde(default)]
    pub host: Option<String>,
}

/// Exec probe configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecProbeConfig {
    pub command: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(!config.runtime.strict);
        assert!(config.runtime.user.is_none());
        assert!(config.runtime.cwd.is_none());
        assert!(config.runtime.env_allow.is_empty());
        assert!(config.runtime.env_deny.is_empty());
        assert!(config.lifecycle.pre_start.is_none());
        assert_eq!(config.lifecycle.shutdown_timeout_secs, 30);
        assert!(config.app.readiness.is_none());
    }

    #[test]
    fn test_lifecycle_config_default() {
        let lifecycle = LifecycleConfig::default();
        assert!(lifecycle.pre_start.is_none());
        assert!(lifecycle.post_stop.is_none());
        assert_eq!(lifecycle.shutdown_timeout_secs, 30);
        assert_eq!(lifecycle.startup_grace_secs, 0);
    }

    #[test]
    fn test_hook_config_default() {
        let yaml = r#"
pre_start:
  command: /bin/init
"#;
        let config: LifecycleConfig = serde_yaml::from_str(yaml).unwrap();
        let hook = config.pre_start.unwrap();
        assert_eq!(hook.command, "/bin/init");
        assert!(hook.args.is_empty());
        assert_eq!(hook.timeout_secs, 60);
        assert!(matches!(hook.on_failure, HookFailurePolicy::Fail));
    }

    #[test]
    fn test_lifecycle_deserialize() {
        let yaml = r#"
pre_start:
  command: /bin/migrate
  args:
    - --up
  timeout_secs: 120
  on_failure: warn
shutdown_timeout_secs: 60
startup_grace_secs: 10
"#;
        let lifecycle: LifecycleConfig = serde_yaml::from_str(yaml).unwrap();
        let hook = lifecycle.pre_start.unwrap();
        assert_eq!(hook.command, "/bin/migrate");
        assert_eq!(hook.args, vec!["--up"]);
        assert_eq!(hook.timeout_secs, 120);
        assert!(matches!(hook.on_failure, HookFailurePolicy::Warn));
        assert_eq!(lifecycle.shutdown_timeout_secs, 60);
        assert_eq!(lifecycle.startup_grace_secs, 10);
    }

    #[test]
    fn test_readiness_http_probe() {
        let yaml = r#"
initial_delay_secs: 5
interval_secs: 15
http_get:
  path: /health
  port: 8080
  host: localhost
"#;
        let readiness: ReadinessConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(readiness.initial_delay_secs, 5);
        assert_eq!(readiness.interval_secs, 15);
        match readiness.probe {
            ProbeConfig::Http { http_get } => {
                assert_eq!(http_get.path, "/health");
                assert_eq!(http_get.port, 8080);
                assert_eq!(http_get.host, Some("localhost".to_string()));
            }
            _ => panic!("Expected HTTP probe"),
        }
    }

    #[test]
    fn test_readiness_exec_probe() {
        let yaml = r#"
exec:
  command:
    - /bin/check-ready
"#;
        let readiness: ReadinessConfig = serde_yaml::from_str(yaml).unwrap();
        match readiness.probe {
            ProbeConfig::Exec { exec } => {
                assert_eq!(exec.command, vec!["/bin/check-ready"]);
            }
            _ => panic!("Expected exec probe"),
        }
    }

    #[test]
    fn test_runtime_env_allow_deny() {
        let yaml = r#"
runtime:
  env_allow:
    - PATH
    - HOME
  env_deny:
    - LD_*
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.runtime.env_allow, vec!["PATH", "HOME"]);
        assert_eq!(config.runtime.env_deny, vec!["LD_*"]);
    }

    #[test]
    fn test_runtime_cwd() {
        let yaml = r#"
runtime:
  cwd: /app
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.runtime.cwd, Some(PathBuf::from("/app")));
    }

    #[test]
    fn test_config_clone() {
        let mut config = Config::default();
        config.runtime.strict = true;
        config.app.name = Some("test".to_string());
        let cloned = config.clone();
        assert!(cloned.runtime.strict);
        assert_eq!(cloned.app.name, Some("test".to_string()));
    }
}
