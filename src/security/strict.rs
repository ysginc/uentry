//! Strict mode enforcement.
//!
//! This module implements strict fail-closed security enforcement:
//!
//! - --strict flag behavior (TASK-023)
//! - Refuse to run as root without drop config (TASK-024)
//! - Refuse writable rootfs unless allowlisted (TASK-025)
//! - Refuse forbidden mounts (TASK-026)
//! - Emit posture report on startup (TASK-027)

use crate::config::Config;
use crate::error::{Result, UentryError};
use crate::security::preflight::PreflightReport;
use std::path::PathBuf;
use tracing::{error, info, warn};

/// Strict mode enforcer.
#[derive(Debug, Clone)]
pub struct StrictMode {
    enabled: bool,
    writable_paths: Vec<PathBuf>,
    allow_root: bool,
}

impl StrictMode {
    /// Create a new strict mode enforcer.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            writable_paths: Vec::new(),
            allow_root: false,
        }
    }

    /// Add a path to the writable allowlist.
    pub fn allow_writable_path(mut self, path: PathBuf) -> Self {
        self.writable_paths.push(path);
        self
    }

    /// Set multiple writable paths.
    pub fn with_writable_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.writable_paths = paths;
        self
    }

    /// Allow running as root (use with caution).
    pub fn allow_root(mut self, allow: bool) -> Self {
        self.allow_root = allow;
        self
    }

    /// Check if strict mode is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// TASK-023 to TASK-027: Validate security posture.
    ///
    /// Returns Ok(()) if all checks pass, Err otherwise.
    pub fn validate(&self, report: &PreflightReport, config: &Config) -> Result<()> {
        self.emit_posture_report(report);

        if !self.enabled {
            info!("Strict mode disabled, skipping security validation");
            return Ok(());
        }

        self.check_root(report, config)?;
        self.check_rootfs(report)?;
        self.check_forbidden_mounts(report)?;
        self.check_dangerous_env_vars(report)?;

        info!("Strict mode validation passed");
        Ok(())
    }

    /// TASK-024: Check if running as root without privilege drop config.
    fn check_root(&self, report: &PreflightReport, config: &Config) -> Result<()> {
        if !report.is_root {
            return Ok(());
        }

        if self.allow_root {
            warn!("Running as root with explicit allow_root=true");
            return Ok(());
        }

        let has_drop_config = config.runtime.user.is_some();

        if !has_drop_config {
            error!("Strict mode: Running as root without privilege drop configuration");
            return Err(UentryError::Security(
                "Refusing to run as root without privilege drop configuration. \
                 Set runtime.user in config or UENTRY_USER environment variable."
                    .to_string(),
            ));
        }

        warn!("Running as root with privilege drop configured");
        Ok(())
    }

    /// TASK-025: Check if rootfs is writable (unless allowlisted).
    fn check_rootfs(&self, report: &PreflightReport) -> Result<()> {
        if report.rootfs_readonly {
            info!("Root filesystem is read-only");
            return Ok(());
        }

        if !self.writable_paths.is_empty() {
            warn!("Root filesystem is writable, but writable_paths allowlist is configured");
            return Ok(());
        }

        error!("Strict mode: Root filesystem is writable");
        Err(UentryError::Security(
            "Refusing to run with writable root filesystem in strict mode. \
             Configure writable_paths allowlist or use read-only rootfs."
                .to_string(),
        ))
    }

    /// TASK-026: Check for forbidden mounts.
    fn check_forbidden_mounts(&self, report: &PreflightReport) -> Result<()> {
        if report.forbidden_mounts.is_empty() {
            return Ok(());
        }

        for mount in &report.forbidden_mounts {
            error!("Strict mode: Forbidden mount detected: {}", mount);
        }

        Err(UentryError::Security(format!(
            "Refusing to run with forbidden mounts: {}",
            report.forbidden_mounts.join(", ")
        )))
    }

    /// TASK-018 (strict): Check for dangerous environment variables.
    fn check_dangerous_env_vars(&self, report: &PreflightReport) -> Result<()> {
        if report.dangerous_env_vars.is_empty() {
            return Ok(());
        }

        for var in &report.dangerous_env_vars {
            error!(
                "Strict mode: Dangerous environment variable detected: {}",
                var
            );
        }

        Err(UentryError::Security(format!(
            "Refusing to run with dangerous environment variables: {}",
            report.dangerous_env_vars.join(", ")
        )))
    }

    /// TASK-027: Emit posture report on startup.
    fn emit_posture_report(&self, report: &PreflightReport) {
        info!("=== Security Posture Report ===");
        info!("  Running as root: {}", report.is_root);
        info!("  UID: {}, GID: {}", report.uid, report.gid);
        info!("  Root filesystem read-only: {}", report.rootfs_readonly);

        if report.capabilities.is_empty() {
            info!("  Capabilities: none");
        } else {
            warn!("  Capabilities: {}", report.capabilities.join(", "));
        }

        if report.forbidden_mounts.is_empty() {
            info!("  Forbidden mounts: none");
        } else {
            warn!("  Forbidden mounts: {}", report.forbidden_mounts.join(", "));
        }

        if report.dangerous_env_vars.is_empty() {
            info!("  Dangerous env vars: none");
        } else {
            warn!(
                "  Dangerous env vars: {}",
                report.dangerous_env_vars.join(", ")
            );
        }

        info!("  Strict mode: {}", self.enabled);
        info!("================================");
    }
}

impl Default for StrictMode {
    fn default() -> Self {
        Self::new(false)
    }
}

/// Create a StrictMode from the configuration.
pub fn from_config(config: &Config) -> StrictMode {
    StrictMode::new(config.runtime.strict)
        .with_writable_paths(config.security.writable_paths.clone())
        .allow_root(config.security.allow_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::preflight::PreflightReport;

    fn create_test_report() -> PreflightReport {
        let mut report = PreflightReport::new();
        report.uid = 1000;
        report.gid = 1000;
        report.is_root = false;
        report.rootfs_readonly = true;
        report
    }

    #[test]
    fn test_strict_mode_new_disabled() {
        let mode = StrictMode::new(false);
        assert!(!mode.is_enabled());
    }

    #[test]
    fn test_strict_mode_new_enabled() {
        let mode = StrictMode::new(true);
        assert!(mode.is_enabled());
    }

    #[test]
    fn test_strict_mode_default() {
        let mode = StrictMode::default();
        assert!(!mode.is_enabled());
    }

    #[test]
    fn test_strict_mode_validate_disabled() {
        let mode = StrictMode::new(false);
        let report = create_test_report();
        let config = Config::default();

        let result = mode.validate(&report, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_mode_validate_non_root() {
        let mode = StrictMode::new(true);
        let report = create_test_report();
        let config = Config::default();

        let result = mode.validate(&report, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_mode_validate_root_without_drop_config() {
        let mode = StrictMode::new(true);
        let mut report = create_test_report();
        report.is_root = true;
        report.uid = 0;
        report.gid = 0;
        let config = Config::default();

        let result = mode.validate(&report, &config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, UentryError::Security(_)));
    }

    #[test]
    fn test_strict_mode_validate_root_with_drop_config() {
        let mode = StrictMode::new(true);
        let mut report = create_test_report();
        report.is_root = true;
        report.uid = 0;
        report.gid = 0;
        let mut config = Config::default();
        config.runtime.user = Some("appuser".to_string());

        let result = mode.validate(&report, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_mode_validate_root_with_allow_root() {
        let mode = StrictMode::new(true).allow_root(true);
        let mut report = create_test_report();
        report.is_root = true;
        report.uid = 0;
        report.gid = 0;
        let config = Config::default();

        let result = mode.validate(&report, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_mode_validate_writable_rootfs() {
        let mode = StrictMode::new(true);
        let mut report = create_test_report();
        report.rootfs_readonly = false;
        let config = Config::default();

        let result = mode.validate(&report, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_strict_mode_validate_writable_rootfs_with_allowlist() {
        let mode = StrictMode::new(true).allow_writable_path(PathBuf::from("/tmp"));
        let mut report = create_test_report();
        report.rootfs_readonly = false;
        let config = Config::default();

        let result = mode.validate(&report, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_mode_validate_forbidden_mounts() {
        let mode = StrictMode::new(true);
        let mut report = create_test_report();
        report
            .forbidden_mounts
            .push("/var/run/docker.sock".to_string());
        let config = Config::default();

        let result = mode.validate(&report, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_strict_mode_validate_dangerous_env_vars() {
        let mode = StrictMode::new(true);
        let mut report = create_test_report();
        report.dangerous_env_vars.push("LD_PRELOAD".to_string());
        let config = Config::default();

        let result = mode.validate(&report, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_config() {
        let mut config = Config::default();
        config.runtime.strict = true;

        let mode = from_config(&config);
        assert!(mode.is_enabled());
    }
}
