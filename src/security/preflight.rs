//! Preflight security checks.
//!
//! This module implements security checks that run before executing
//! the child process:
//!
//! - Root detection (TASK-014)
//! - Read-only rootfs verification (TASK-015)
//! - Capability inspection (TASK-016)
//! - Forbidden mount detection (TASK-017)
//! - Dangerous environment variable detection (TASK-018)

use crate::error::{Result, UentryError};
use std::fs;
use std::path::Path;

/// Dangerous environment variables that could compromise security.
const DANGEROUS_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "LD_DEBUG",
    "LD_TRACE_LOADED_OBJECTS",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "__CF_USER_TEXT_ENCODING",
];

/// Mount paths considered dangerous in containers.
const FORBIDDEN_MOUNT_PATTERNS: &[&str] = &[
    "/var/run/docker.sock",
    "/run/docker.sock",
    "/.dockerenv",
    "/proc/sys/kernel",
];

/// Preflight check report containing security posture information.
#[derive(Debug, Clone, Default)]
pub struct PreflightReport {
    pub is_root: bool,
    pub uid: u32,
    pub gid: u32,
    pub rootfs_readonly: bool,
    pub capabilities: Vec<String>,
    pub forbidden_mounts: Vec<String>,
    pub dangerous_env_vars: Vec<String>,
    pub warnings: Vec<String>,
}

impl PreflightReport {
    /// Create a new empty preflight report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if any security issues were detected.
    pub fn has_issues(&self) -> bool {
        !self.forbidden_mounts.is_empty() || !self.dangerous_env_vars.is_empty()
    }

    /// Check if the process is running as root.
    pub fn is_privileged(&self) -> bool {
        self.is_root
    }
}

/// Individual preflight check.
#[derive(Debug, Clone)]
pub struct PreflightCheck {
    report: PreflightReport,
}

impl PreflightCheck {
    /// Create a new preflight check instance.
    pub fn new() -> Self {
        Self {
            report: PreflightReport::new(),
        }
    }

    /// Run all preflight checks and return the report.
    pub fn run(mut self) -> Result<PreflightReport> {
        self.check_uid_gid()?;
        self.check_rootfs()?;
        self.check_capabilities()?;
        self.check_forbidden_mounts()?;
        self.check_dangerous_env_vars()?;
        Ok(self.report)
    }

    /// TASK-014: Check if running as root.
    fn check_uid_gid(&mut self) -> Result<()> {
        self.report.uid = unsafe { libc::getuid() };
        self.report.gid = unsafe { libc::getgid() };
        self.report.is_root = self.report.uid == 0;

        if self.report.is_root {
            self.report
                .warnings
                .push("Running as root (UID 0)".to_string());
        }

        Ok(())
    }

    /// TASK-015: Check if root filesystem is read-only.
    fn check_rootfs(&mut self) -> Result<()> {
        let test_file = Path::new("/.uentry_write_test");

        match fs::write(test_file, b"test") {
            Ok(_) => {
                self.report.rootfs_readonly = false;
                let _ = fs::remove_file(test_file);
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    || e.kind() == std::io::ErrorKind::ReadOnlyFilesystem
                {
                    self.report.rootfs_readonly = true;
                } else {
                    self.report.rootfs_readonly = false;
                    self.report
                        .warnings
                        .push(format!("Could not determine rootfs status: {}", e));
                }
            }
        }

        Ok(())
    }

    /// TASK-016: Inspect /proc/self/status for capabilities.
    fn check_capabilities(&mut self) -> Result<()> {
        let status_path = Path::new("/proc/self/status");

        if !status_path.exists() {
            self.report
                .warnings
                .push("Cannot read /proc/self/status".to_string());
            return Ok(());
        }

        let contents = fs::read_to_string(status_path).map_err(|e| {
            UentryError::Security(format!("Failed to read /proc/self/status: {}", e))
        })?;

        for line in contents.lines() {
            if line.starts_with("CapEff:") {
                let caps = line.split(':').nth(1).unwrap_or("").trim();
                if !caps.is_empty() && caps != "0000000000000000" {
                    self.report.capabilities.push(caps.to_string());
                }
                break;
            }
        }

        if !self.report.capabilities.is_empty() {
            self.report
                .warnings
                .push("Process has elevated capabilities".to_string());
        }

        Ok(())
    }

    /// TASK-017: Check for forbidden mounts.
    fn check_forbidden_mounts(&mut self) -> Result<()> {
        let mounts_path = Path::new("/proc/mounts");

        if !mounts_path.exists() {
            self.report
                .warnings
                .push("Cannot read /proc/mounts".to_string());
            return Ok(());
        }

        let contents = fs::read_to_string(mounts_path)
            .map_err(|e| UentryError::Security(format!("Failed to read /proc/mounts: {}", e)))?;

        for line in contents.lines() {
            let mount_point = line.split_whitespace().nth(1).unwrap_or("");
            for forbidden in FORBIDDEN_MOUNT_PATTERNS {
                if mount_point.contains(forbidden) || mount_point == *forbidden {
                    self.report.forbidden_mounts.push(mount_point.to_string());
                }
            }
        }

        Ok(())
    }

    /// TASK-018: Check for dangerous environment variables.
    fn check_dangerous_env_vars(&mut self) -> Result<()> {
        for var in DANGEROUS_ENV_VARS {
            if std::env::var(var).is_ok() {
                self.report.dangerous_env_vars.push(var.to_string());
            }
        }

        Ok(())
    }
}

impl Default for PreflightCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if running as root (convenience function).
pub fn is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

/// Get current UID.
pub fn get_uid() -> u32 {
    unsafe { libc::getuid() }
}

/// Get current GID.
pub fn get_gid() -> u32 {
    unsafe { libc::getgid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preflight_report_new() {
        let report = PreflightReport::new();
        assert!(!report.is_root);
        assert!(report.capabilities.is_empty());
        assert!(report.forbidden_mounts.is_empty());
        assert!(report.dangerous_env_vars.is_empty());
    }

    #[test]
    fn test_preflight_report_has_issues_false() {
        let report = PreflightReport::new();
        assert!(!report.has_issues());
    }

    #[test]
    fn test_preflight_report_has_issues_true() {
        let mut report = PreflightReport::new();
        report.dangerous_env_vars.push("LD_PRELOAD".to_string());
        assert!(report.has_issues());
    }

    #[test]
    fn test_preflight_check_new() {
        let check = PreflightCheck::new();
        assert!(!check.report.is_root);
    }

    #[test]
    fn test_preflight_check_run() {
        let check = PreflightCheck::new();
        let report = check.run().unwrap();
        assert!(report.uid == unsafe { libc::getuid() });
        assert!(report.gid == unsafe { libc::getgid() });
    }

    #[test]
    fn test_is_root() {
        let result = is_root();
        assert!(result == (unsafe { libc::getuid() == 0 }));
    }

    #[test]
    fn test_get_uid() {
        let uid = get_uid();
        assert_eq!(uid, unsafe { libc::getuid() });
    }

    #[test]
    fn test_get_gid() {
        let gid = get_gid();
        assert_eq!(gid, unsafe { libc::getgid() });
    }

    #[test]
    fn test_dangerous_env_vars_detection() {
        let mut check = PreflightCheck::new();
        check.check_dangerous_env_vars().unwrap();

        std::env::set_var("LD_PRELOAD", "/tmp/test.so");
        let mut check2 = PreflightCheck::new();
        check2.check_dangerous_env_vars().unwrap();
        assert!(check2
            .report
            .dangerous_env_vars
            .contains(&"LD_PRELOAD".to_string()));

        std::env::remove_var("LD_PRELOAD");
    }
}
