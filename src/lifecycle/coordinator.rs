//! Lifecycle coordinator.
//!
//! This module implements:
//! - Phase coordination for ordered execution (TASK-052)
//! - Lifecycle phase execution: preflight → sanitize → fs_prep → secrets → security → exec (TASK-053)
//! - Pre/post command execution (TASK-058-060)

use crate::config::schema::{Config, HookFailurePolicy};
use crate::error::{Result, UentryError};
use crate::lifecycle::hooks::{
    HookContext, HookExecutor, HookFailurePolicy as HookPolicy, HookPhase,
};
use crate::security::filesystem::FilesystemPrep;
use crate::security::preflight::PreflightCheck;
use crate::security::privileges::{drop_privileges, PrivilegeConfig};
use crate::security::secrets::SecretsManager;
use crate::security::strict::StrictMode;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Lifecycle coordinator for managing startup phases.
pub struct LifecycleCoordinator {
    config: Config,
    hook_executor: HookExecutor,
    secrets_manager: SecretsManager,
}

impl LifecycleCoordinator {
    /// Create a new lifecycle coordinator.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            hook_executor: HookExecutor::new(),
            secrets_manager: SecretsManager::new(),
        }
    }

    /// Run all lifecycle phases.
    pub fn run(&mut self) -> Result<()> {
        info!("Starting lifecycle coordination");

        self.run_preflight()?;
        self.run_sanitize()?;
        self.run_fs_prep()?;
        self.run_secrets()?;
        self.run_security()?;

        info!("Lifecycle coordination complete");
        Ok(())
    }

    /// Run preflight checks.
    fn run_preflight(&self) -> Result<()> {
        debug!("Phase: preflight");

        let context = self.build_context("preflight");
        self.hook_executor
            .execute_phase(HookPhase::Preflight, &context, HookPolicy::Fail)?;

        let preflight = PreflightCheck::new();
        let report = preflight.run()?;

        let strict_mode = StrictMode::new(self.config.runtime.strict)
            .with_writable_paths(self.config.security.writable_paths.clone())
            .allow_root(self.config.security.allow_root);

        strict_mode.validate(&report, &self.config)?;

        Ok(())
    }

    /// Run sanitization phase.
    fn run_sanitize(&self) -> Result<()> {
        debug!("Phase: sanitize");

        let context = self.build_context("sanitize");
        self.hook_executor.execute_phase(
            HookPhase::Sanitize,
            &context,
            self.hook_failure_policy(),
        )?;

        self.sanitize_environment()?;

        Ok(())
    }

    /// Sanitize environment based on allow/deny lists.
    fn sanitize_environment(&self) -> Result<()> {
        if !self.config.runtime.env_allow.is_empty() {
            let allowed: Vec<String> = self
                .config
                .runtime
                .env_allow
                .iter()
                .filter(|pattern| pattern.contains('*') || std::env::var(pattern).is_ok())
                .cloned()
                .collect();

            debug!("Environment allow list: {:?}", allowed);
        }

        for pattern in &self.config.runtime.env_deny {
            if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1];
                let vars_to_remove: Vec<String> = std::env::vars()
                    .filter(|(k, _)| k.starts_with(prefix))
                    .map(|(k, _)| k)
                    .collect();

                for var in vars_to_remove {
                    debug!("Removing env var (deny pattern): {}", var);
                    std::env::remove_var(&var);
                }
            } else if std::env::var(pattern).is_ok() {
                debug!("Removing env var (deny list): {}", pattern);
                std::env::remove_var(pattern);
            }
        }

        Ok(())
    }

    /// Run filesystem preparation phase.
    fn run_fs_prep(&self) -> Result<()> {
        debug!("Phase: fs_prep");

        let context = self.build_context("fs_prep");
        self.hook_executor.execute_phase(
            HookPhase::FsPrep,
            &context,
            self.hook_failure_policy(),
        )?;

        let mut fs_prep =
            FilesystemPrep::new().with_writable_paths(self.config.security.writable_paths.clone());

        for dir in &self.config.runtime.ensure_dirs {
            let mut spec = crate::security::filesystem::DirSpec::new(dir.path.clone());
            if let Some(mode) = dir.mode_value() {
                spec = spec.with_mode(mode);
            }
            fs_prep = fs_prep.add_dir(spec);
        }

        fs_prep.run()?;

        Ok(())
    }

    /// Run secrets injection phase.
    fn run_secrets(&mut self) -> Result<()> {
        debug!("Phase: secrets");

        let context = self.build_context("secrets");
        self.hook_executor.execute_phase(
            HookPhase::Secrets,
            &context,
            self.hook_failure_policy(),
        )?;

        for fte in &self.config.secrets.file_to_env {
            let mapping =
                crate::security::secrets::FileToEnv::new(fte.file.clone(), fte.env_var.clone());
            if fte.optional {
                self.secrets_manager =
                    std::mem::take(&mut self.secrets_manager).add_file_to_env(mapping.optional());
            } else {
                self.secrets_manager =
                    std::mem::take(&mut self.secrets_manager).add_file_to_env(mapping);
            }
        }

        for etf in &self.config.secrets.env_to_file {
            let mapping =
                crate::security::secrets::EnvToFile::new(etf.env_var.clone(), etf.file.clone())
                    .with_mode(etf.mode_value());
            self.secrets_manager =
                std::mem::take(&mut self.secrets_manager).add_env_to_file(mapping);
        }

        self.secrets_manager.run()?;

        Ok(())
    }

    /// Run security/privilege phase.
    fn run_security(&self) -> Result<()> {
        debug!("Phase: security");

        let context = self.build_context("security");
        self.hook_executor.execute_phase(
            HookPhase::Security,
            &context,
            self.hook_failure_policy(),
        )?;

        let mut priv_config = PrivilegeConfig::new();

        if let Some(ref user) = self.config.runtime.user {
            let uid = PrivilegeConfig::resolve_user(user)?;
            priv_config = priv_config.with_uid(uid);
        }

        if let Some(ref group) = self.config.runtime.group {
            let gid = PrivilegeConfig::resolve_group(group)?;
            priv_config = priv_config.with_gid(gid);
        }

        if !self.config.runtime.supplementary_groups.is_empty() {
            let mut groups = Vec::new();
            for group in &self.config.runtime.supplementary_groups {
                let gid = PrivilegeConfig::resolve_group(group)?;
                groups.push(gid);
            }
            priv_config = priv_config.with_groups(groups);
        }

        if let Some(ref umask_str) = self.config.runtime.umask {
            let umask = u16::from_str_radix(umask_str, 8)
                .map_err(|_| UentryError::Config(format!("Invalid umask: {}", umask_str)))?;
            priv_config = priv_config.with_umask(umask);
        }

        if self.config.security.no_new_privs {
            priv_config = priv_config.with_no_new_privs(true);
        }

        if priv_config.uid.is_some() || priv_config.gid.is_some() {
            drop_privileges(&priv_config)?;
        } else if self.config.security.no_new_privs {
            crate::security::privileges::set_no_new_privs_only()?;
        }

        Ok(())
    }

    /// Run pre-start hook from config.
    pub fn run_pre_start(&self) -> Result<()> {
        let context = self.build_context("pre_start");

        self.hook_executor
            .execute_phase(HookPhase::PreStart, &context, HookPolicy::Fail)?;

        if let Some(ref hook) = self.config.lifecycle.pre_start {
            info!("Running pre-start command: {}", hook.command);

            let timeout = std::time::Duration::from_secs(hook.timeout_secs);
            let mut cmd = std::process::Command::new(&hook.command);
            cmd.args(&hook.args);

            let child = cmd
                .spawn()
                .map_err(|e| UentryError::Exec(format!("Failed to run pre-start: {}", e)))?;

            self.wait_with_timeout(child, timeout, "pre-start", &hook.on_failure)?;
        }

        Ok(())
    }

    /// Run post-stop hook from config.
    pub fn run_post_stop(&self) -> Result<()> {
        let context = self.build_context("post_stop");

        self.hook_executor
            .execute_phase(HookPhase::PostStop, &context, HookPolicy::Warn)?;

        if let Some(ref hook) = self.config.lifecycle.post_stop {
            info!("Running post-stop command: {}", hook.command);

            let timeout = std::time::Duration::from_secs(hook.timeout_secs);
            let mut cmd = std::process::Command::new(&hook.command);
            cmd.args(&hook.args);

            let child = cmd
                .spawn()
                .map_err(|e| UentryError::Exec(format!("Failed to run post-stop: {}", e)))?;

            self.wait_with_timeout(child, timeout, "post-stop", &hook.on_failure)?;
        }

        Ok(())
    }

    fn wait_with_timeout(
        &self,
        mut child: std::process::Child,
        timeout: std::time::Duration,
        name: &str,
        policy: &HookFailurePolicy,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        info!("{} completed successfully", name);
                        return Ok(());
                    } else {
                        let code = status.code().unwrap_or(-1);
                        let err = UentryError::Exec(format!("{} failed with code {}", name, code));
                        match policy {
                            HookFailurePolicy::Fail => return Err(err),
                            HookFailurePolicy::Warn => {
                                warn!("{}", err);
                                return Ok(());
                            }
                            HookFailurePolicy::Ignore => {
                                debug!("{}", err);
                                return Ok(());
                            }
                        }
                    }
                }
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        let err = UentryError::Exec(format!("{} timed out", name));
                        match policy {
                            HookFailurePolicy::Fail => return Err(err),
                            HookFailurePolicy::Warn => {
                                warn!("{}", err);
                                return Ok(());
                            }
                            HookFailurePolicy::Ignore => {
                                debug!("{}", err);
                                return Ok(());
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => {
                    return Err(UentryError::Exec(format!(
                        "Failed to wait for {}: {}",
                        name, e
                    )));
                }
            }
        }
    }

    fn build_context(&self, phase: &str) -> HookContext {
        let mut environment = HashMap::new();
        for (key, value) in &self.config.runtime.env {
            environment.insert(key.clone(), value.clone());
        }

        HookContext {
            phase: phase.to_string(),
            config: crate::lifecycle::hooks::HookConfigInfo {
                app_name: self.config.app.name.clone(),
                strict_mode: self.config.runtime.strict,
                user: self.config.runtime.user.clone(),
                group: self.config.runtime.group.clone(),
            },
            environment,
            metadata: HashMap::new(),
        }
    }

    fn hook_failure_policy(&self) -> HookPolicy {
        HookPolicy::Warn
    }

    /// Get the secrets manager for log redaction.
    pub fn secrets_manager(&self) -> &SecretsManager {
        &self.secrets_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_new() {
        let config = Config::default();
        let coordinator = LifecycleCoordinator::new(config);
        assert!(!coordinator.config.runtime.strict);
    }
}
