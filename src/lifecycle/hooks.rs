//! Hook execution system.
//!
//! This module implements:
//! - Hook discovery from /usr/lib/uentry/hooks/{phase}.d/* (TASK-054)
//! - JSON context on stdin for hooks (TASK-055)
//! - Hook timeout enforcement (TASK-056)
//! - Failure policy per hook (TASK-057)

use crate::error::{Result, UentryError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

const HOOKS_BASE_DIR: &str = "/usr/lib/uentry/hooks";

/// Hook phases in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPhase {
    Preflight,
    Sanitize,
    FsPrep,
    Secrets,
    Security,
    PreStart,
    PostStop,
}

impl HookPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookPhase::Preflight => "preflight",
            HookPhase::Sanitize => "sanitize",
            HookPhase::FsPrep => "fs_prep",
            HookPhase::Secrets => "secrets",
            HookPhase::Security => "security",
            HookPhase::PreStart => "pre_start",
            HookPhase::PostStop => "post_stop",
        }
    }

    pub fn dir_name(&self) -> &'static str {
        match self {
            HookPhase::Preflight => "preflight.d",
            HookPhase::Sanitize => "sanitize.d",
            HookPhase::FsPrep => "fs_prep.d",
            HookPhase::Secrets => "secrets.d",
            HookPhase::Security => "security.d",
            HookPhase::PreStart => "pre_start.d",
            HookPhase::PostStop => "post_stop.d",
        }
    }
}

/// Hook failure policy.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookFailurePolicy {
    #[default]
    Fail,
    Warn,
    Ignore,
}

/// Hook context passed to hooks via stdin as JSON.
#[derive(Debug, Clone, Serialize)]
pub struct HookContext {
    pub phase: String,
    pub config: HookConfigInfo,
    pub environment: HashMap<String, String>,
    pub metadata: HashMap<String, String>,
}

/// Simplified config info for hooks.
#[derive(Debug, Clone, Serialize)]
pub struct HookConfigInfo {
    pub app_name: Option<String>,
    pub strict_mode: bool,
    pub user: Option<String>,
    pub group: Option<String>,
}

/// Discovered hook.
#[derive(Debug, Clone)]
pub struct Hook {
    pub path: PathBuf,
    pub name: String,
    pub phase: HookPhase,
}

impl Hook {
    pub fn new(path: PathBuf, phase: HookPhase) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Self { path, name, phase }
    }
}

/// Hook executor.
#[derive(Debug, Clone)]
pub struct HookExecutor {
    hooks_dir: PathBuf,
    timeout: Duration,
}

impl HookExecutor {
    /// Create a new hook executor with default settings.
    pub fn new() -> Self {
        Self {
            hooks_dir: PathBuf::from(HOOKS_BASE_DIR),
            timeout: Duration::from_secs(60),
        }
    }

    /// Set custom hooks directory.
    pub fn with_hooks_dir(mut self, path: PathBuf) -> Self {
        self.hooks_dir = path;
        self
    }

    /// Set default timeout for hooks.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Discover hooks for a specific phase.
    pub fn discover(&self, phase: HookPhase) -> Vec<Hook> {
        let phase_dir = self.hooks_dir.join(phase.dir_name());

        if !phase_dir.exists() {
            debug!("Hook directory does not exist: {:?}", phase_dir);
            return Vec::new();
        }

        let mut hooks = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&phase_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() || path.is_symlink() {
                    hooks.push(Hook::new(path, phase));
                }
            }
        }

        hooks.sort_by(|a, b| a.name.cmp(&b.name));
        hooks
    }

    /// Execute a single hook.
    pub fn execute(&self, hook: &Hook, context: &HookContext) -> Result<()> {
        info!("Executing hook: {} ({:?})", hook.name, hook.phase);
        debug!("Hook path: {:?}", hook.path);

        let context_json = serde_json::to_string(context)
            .map_err(|e| UentryError::Exec(format!("Failed to serialize hook context: {}", e)))?;

        let start = Instant::now();

        let mut child = Command::new(&hook.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                UentryError::Exec(format!("Failed to spawn hook {:?}: {}", hook.path, e))
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(context_json.as_bytes())
                .map_err(|e| UentryError::Exec(format!("Failed to write to hook stdin: {}", e)))?;
        }

        let result = child.wait_timeout(self.timeout);

        match result {
            Ok(status) => {
                let elapsed = start.elapsed();
                if status.success() {
                    info!("Hook {} completed successfully in {:?}", hook.name, elapsed);
                    Ok(())
                } else {
                    let code = status.code().unwrap_or(-1);
                    Err(UentryError::Exec(format!(
                        "Hook {} failed with exit code {}",
                        hook.name, code
                    )))
                }
            }
            Err(e) => {
                error!("Hook {} failed: {}", hook.name, e);
                let _ = child.kill();
                Err(UentryError::Exec(format!(
                    "Hook {} timed out or failed: {}",
                    hook.name, e
                )))
            }
        }
    }

    /// Execute all hooks for a phase with failure policy.
    pub fn execute_phase(
        &self,
        phase: HookPhase,
        context: &HookContext,
        policy: HookFailurePolicy,
    ) -> Result<()> {
        let hooks = self.discover(phase);

        if hooks.is_empty() {
            debug!("No hooks found for phase {:?}", phase);
            return Ok(());
        }

        info!("Executing {} hooks for phase {:?}", hooks.len(), phase);

        for hook in &hooks {
            match self.execute(hook, context) {
                Ok(()) => {}
                Err(e) => match policy {
                    HookFailurePolicy::Fail => {
                        error!("Hook {} failed, aborting: {}", hook.name, e);
                        return Err(e);
                    }
                    HookFailurePolicy::Warn => {
                        warn!("Hook {} failed (warn policy): {}", hook.name, e);
                    }
                    HookFailurePolicy::Ignore => {
                        debug!("Hook {} failed (ignore policy): {}", hook.name, e);
                    }
                },
            }
        }

        Ok(())
    }
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

trait ChildExt {
    fn wait_timeout(&mut self, timeout: Duration) -> std::io::Result<std::process::ExitStatus>;
}

impl ChildExt for std::process::Child {
    fn wait_timeout(&mut self, timeout: Duration) -> std::io::Result<std::process::ExitStatus> {
        let start = Instant::now();
        loop {
            match self.try_wait()? {
                Some(status) => return Ok(status),
                None => {
                    if start.elapsed() >= timeout {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "Process timed out",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_phase_as_str() {
        assert_eq!(HookPhase::Preflight.as_str(), "preflight");
        assert_eq!(HookPhase::PreStart.as_str(), "pre_start");
    }

    #[test]
    fn test_hook_context_serialize() {
        let context = HookContext {
            phase: "preflight".to_string(),
            config: HookConfigInfo {
                app_name: Some("test".to_string()),
                strict_mode: true,
                user: Some("appuser".to_string()),
                group: None,
            },
            environment: HashMap::new(),
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&context).unwrap();
        assert!(json.contains("preflight"));
        assert!(json.contains("test"));
    }

    #[test]
    fn test_hook_executor_new() {
        let executor = HookExecutor::new();
        assert_eq!(executor.hooks_dir, PathBuf::from(HOOKS_BASE_DIR));
        assert_eq!(executor.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_hook_executor_with_timeout() {
        let executor = HookExecutor::new().with_timeout(Duration::from_secs(30));
        assert_eq!(executor.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_discover_empty_dir() {
        let temp_dir = std::env::temp_dir().join("uentry_test_hooks_empty");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).ok();

        let executor = HookExecutor::new().with_hooks_dir(temp_dir.clone());
        let hooks = executor.discover(HookPhase::Preflight);
        assert!(hooks.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_hook_new() {
        let hook = Hook::new(PathBuf::from("/path/to/hook.sh"), HookPhase::PreStart);
        assert_eq!(hook.name, "hook.sh");
        assert_eq!(hook.phase, HookPhase::PreStart);
    }
}
