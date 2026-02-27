//! Audit session and report generation.
//!
//! This module provides lightweight runtime auditing with optional deep tracing.

use crate::config::schema::{AuditBackend, AuditConfig};
use crate::error::{Result, UentryError};
use crate::security::preflight::PreflightReport;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
struct AuditEvent {
    timestamp_ms: u128,
    category: String,
    detail: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct PreflightSummary {
    is_root: bool,
    uid: u32,
    gid: u32,
    rootfs_readonly: bool,
    capabilities: Vec<String>,
    forbidden_mounts: Vec<String>,
    dangerous_env_vars: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AuditReport {
    started_at_ms: u128,
    finished_at_ms: u128,
    backend: String,
    deep_trace_requested: bool,
    deep_trace_active: bool,
    command: Vec<String>,
    linked_libraries: Vec<String>,
    observed_env_keys: Vec<String>,
    ensured_directories: Vec<PathBuf>,
    observed_read_paths: Vec<PathBuf>,
    observed_write_paths: Vec<PathBuf>,
    observed_syscalls: Vec<String>,
    preflight: Option<PreflightSummary>,
    events: Vec<AuditEvent>,
}

#[derive(Debug, Serialize)]
struct ProfileSnippet {
    runtime: ProfileRuntime,
    security: ProfileSecurity,
}

#[derive(Debug, Serialize)]
struct ProfileRuntime {
    env_allow: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProfileSecurity {
    writable_paths: Vec<PathBuf>,
}

/// Runtime audit session.
#[derive(Debug, Clone)]
pub struct AuditSession {
    output: Option<PathBuf>,
    profile_output: Option<PathBuf>,
    backend: AuditBackend,
    deep_trace_requested: bool,
    deep_trace_active: bool,
    trace_prefix: Option<PathBuf>,
    started_at_ms: u128,
    command: Vec<String>,
    linked_libraries: BTreeSet<String>,
    observed_env_keys: BTreeSet<String>,
    ensured_directories: BTreeSet<PathBuf>,
    observed_read_paths: BTreeSet<PathBuf>,
    observed_write_paths: BTreeSet<PathBuf>,
    observed_syscalls: BTreeSet<String>,
    preflight: Option<PreflightSummary>,
    events: Vec<AuditEvent>,
}

impl AuditSession {
    /// Create an audit session from configuration when auditing is enabled.
    pub fn from_config(config: &AuditConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }

        let mut session = Self {
            output: config.output.clone(),
            profile_output: config.profile_output.clone(),
            backend: config.backend.clone(),
            deep_trace_requested: config.deep_trace,
            deep_trace_active: false,
            trace_prefix: None,
            started_at_ms: now_millis(),
            command: Vec::new(),
            linked_libraries: BTreeSet::new(),
            observed_env_keys: BTreeSet::new(),
            ensured_directories: BTreeSet::new(),
            observed_read_paths: BTreeSet::new(),
            observed_write_paths: BTreeSet::new(),
            observed_syscalls: BTreeSet::new(),
            preflight: None,
            events: Vec::new(),
        };

        session.record_event(
            "audit_started",
            BTreeMap::from([
                (
                    "deep_trace_requested".to_string(),
                    config.deep_trace.to_string(),
                ),
                (
                    "backend".to_string(),
                    audit_backend_name(&session.backend).to_string(),
                ),
            ]),
        );

        Some(session)
    }

    /// Record a generic audit event.
    pub fn record_event(&mut self, category: &str, detail: BTreeMap<String, String>) {
        self.events.push(AuditEvent {
            timestamp_ms: now_millis(),
            category: category.to_string(),
            detail,
        });
    }

    /// Record command details.
    pub fn record_command(&mut self, command: &[String]) {
        self.command = command.to_vec();
        let mut detail = BTreeMap::new();
        detail.insert("argc".to_string(), command.len().to_string());
        if let Some(cmd) = command.first() {
            detail.insert("command".to_string(), cmd.clone());
        }
        self.record_event("command", detail);
    }

    /// Record preflight report summary.
    pub fn record_preflight_report(&mut self, report: &PreflightReport) {
        self.preflight = Some(PreflightSummary {
            is_root: report.is_root,
            uid: report.uid,
            gid: report.gid,
            rootfs_readonly: report.rootfs_readonly,
            capabilities: report.capabilities.clone(),
            forbidden_mounts: report.forbidden_mounts.clone(),
            dangerous_env_vars: report.dangerous_env_vars.clone(),
            warnings: report.warnings.clone(),
        });

        let mut detail = BTreeMap::new();
        detail.insert("is_root".to_string(), report.is_root.to_string());
        detail.insert(
            "capability_count".to_string(),
            report.capabilities.len().to_string(),
        );
        detail.insert(
            "forbidden_mount_count".to_string(),
            report.forbidden_mounts.len().to_string(),
        );
        detail.insert(
            "dangerous_env_count".to_string(),
            report.dangerous_env_vars.len().to_string(),
        );
        self.record_event("preflight_summary", detail);
    }

    /// Record lifecycle phase outcome.
    pub fn record_lifecycle_outcome(&mut self, phase: &str, success: bool, message: Option<&str>) {
        let mut detail = BTreeMap::new();
        detail.insert("phase".to_string(), phase.to_string());
        detail.insert("success".to_string(), success.to_string());
        if let Some(msg) = message {
            detail.insert("message".to_string(), msg.to_string());
        }
        self.record_event("lifecycle", detail);
    }

    /// Record execution outcome.
    pub fn record_exec_outcome(&mut self, exit_code: Option<i32>, error: Option<&str>) {
        let mut detail = BTreeMap::new();
        if let Some(code) = exit_code {
            detail.insert("exit_code".to_string(), code.to_string());
        }
        if let Some(err) = error {
            detail.insert("error".to_string(), err.to_string());
        }
        self.record_event("exec", detail);
    }

    /// Record observed environment keys.
    pub fn record_env_keys<I>(&mut self, keys: I)
    where
        I: IntoIterator<Item = String>,
    {
        let mut added = 0usize;
        for key in keys {
            if self.observed_env_keys.insert(key) {
                added += 1;
            }
        }

        let mut detail = BTreeMap::new();
        detail.insert("new_key_count".to_string(), added.to_string());
        detail.insert(
            "total_key_count".to_string(),
            self.observed_env_keys.len().to_string(),
        );
        self.record_event("env_keys", detail);
    }

    /// Record an ensured directory.
    pub fn record_ensured_directory(&mut self, path: &Path, created: bool) {
        self.ensured_directories.insert(path.to_path_buf());

        let mut detail = BTreeMap::new();
        detail.insert("path".to_string(), path.display().to_string());
        detail.insert("created".to_string(), created.to_string());
        self.record_event("ensure_dir", detail);
    }

    /// Best-effort linked library detection via `ldd`.
    pub fn detect_linked_libraries(&mut self, command: &[String]) {
        if command.is_empty() {
            return;
        }

        if !command_exists("ldd") {
            self.record_event(
                "ldd_unavailable",
                BTreeMap::from([("reason".to_string(), "ldd not found".to_string())]),
            );
            return;
        }

        let Some(executable) = resolve_executable(&command[0]) else {
            self.record_event(
                "ldd_skipped",
                BTreeMap::from([(
                    "reason".to_string(),
                    "unable to resolve executable".to_string(),
                )]),
            );
            return;
        };

        match Command::new("ldd").arg(&executable).output() {
            Ok(output) => {
                if !output.status.success() {
                    self.record_event(
                        "ldd_failed",
                        BTreeMap::from([(
                            "status".to_string(),
                            output
                                .status
                                .code()
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "signal".to_string()),
                        )]),
                    );
                    return;
                }

                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut detected = 0usize;
                for line in stdout.lines() {
                    if let Some(path) = parse_ldd_path(line) {
                        if self.linked_libraries.insert(path) {
                            detected += 1;
                        }
                    }
                }

                self.record_event(
                    "ldd_detected",
                    BTreeMap::from([("library_count".to_string(), detected.to_string())]),
                );
            }
            Err(e) => {
                self.record_event(
                    "ldd_error",
                    BTreeMap::from([("error".to_string(), e.to_string())]),
                );
            }
        }
    }

    /// Wrap a command with deep tracing when configured and available.
    pub fn prepare_command_for_exec(&mut self, command: &[String]) -> Vec<String> {
        if !self.deep_trace_requested {
            return command.to_vec();
        }

        if matches!(self.backend, AuditBackend::None) {
            self.record_event(
                "deep_trace_downgrade",
                BTreeMap::from([(
                    "reason".to_string(),
                    "backend configured as none".to_string(),
                )]),
            );
            return command.to_vec();
        }

        if !command_exists("strace") {
            self.record_event(
                "deep_trace_downgrade",
                BTreeMap::from([("reason".to_string(), "strace not available".to_string())]),
            );
            return command.to_vec();
        }

        let trace_prefix = self.default_trace_prefix();
        let mut wrapped = vec![
            "strace".to_string(),
            "-ff".to_string(),
            "-tt".to_string(),
            "-s".to_string(),
            "256".to_string(),
            "-e".to_string(),
            "trace=%file,%process".to_string(),
            "-o".to_string(),
            trace_prefix.display().to_string(),
            "--".to_string(),
        ];
        wrapped.extend(command.iter().cloned());

        self.deep_trace_active = true;
        self.trace_prefix = Some(trace_prefix.clone());
        self.record_event(
            "deep_trace_enabled",
            BTreeMap::from([
                (
                    "backend".to_string(),
                    audit_backend_name(&self.backend).to_string(),
                ),
                (
                    "trace_prefix".to_string(),
                    trace_prefix.display().to_string(),
                ),
            ]),
        );

        wrapped
    }

    /// Ingest strace output and derive syscall/read/write observations.
    pub fn ingest_trace_output(&mut self) {
        if !self.deep_trace_active {
            return;
        }

        let Some(prefix) = self.trace_prefix.clone() else {
            self.record_event(
                "trace_ingest_skipped",
                BTreeMap::from([("reason".to_string(), "missing trace prefix".to_string())]),
            );
            return;
        };

        let files = trace_files_for_prefix(&prefix);
        if files.is_empty() {
            self.record_event(
                "trace_ingest_skipped",
                BTreeMap::from([("reason".to_string(), "no trace files found".to_string())]),
            );
            return;
        }

        let mut parsed_files = 0usize;
        for file in files {
            let contents = match fs::read_to_string(&file) {
                Ok(c) => c,
                Err(e) => {
                    self.record_event(
                        "trace_read_error",
                        BTreeMap::from([
                            ("path".to_string(), file.display().to_string()),
                            ("error".to_string(), e.to_string()),
                        ]),
                    );
                    continue;
                }
            };

            parsed_files += 1;
            for line in contents.lines() {
                self.ingest_trace_line(line);
            }
        }

        self.record_event(
            "trace_ingested",
            BTreeMap::from([
                ("file_count".to_string(), parsed_files.to_string()),
                (
                    "syscall_count".to_string(),
                    self.observed_syscalls.len().to_string(),
                ),
            ]),
        );
    }

    /// Emit audit report to configured output or stderr.
    pub fn finalize(&mut self) -> Result<()> {
        self.record_event(
            "audit_finalize",
            BTreeMap::from([(
                "destination".to_string(),
                self.output
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "stderr".to_string()),
            )]),
        );

        let finished_at_ms = now_millis();
        let report = AuditReport {
            started_at_ms: self.started_at_ms,
            finished_at_ms,
            backend: audit_backend_name(&self.backend).to_string(),
            deep_trace_requested: self.deep_trace_requested,
            deep_trace_active: self.deep_trace_active,
            command: self.command.clone(),
            linked_libraries: self.linked_libraries.iter().cloned().collect(),
            observed_env_keys: self.observed_env_keys.iter().cloned().collect(),
            ensured_directories: self.ensured_directories.iter().cloned().collect(),
            observed_read_paths: self.observed_read_paths.iter().cloned().collect(),
            observed_write_paths: self.observed_write_paths.iter().cloned().collect(),
            observed_syscalls: self.observed_syscalls.iter().cloned().collect(),
            preflight: self.preflight.clone(),
            events: self.events.clone(),
        };

        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| UentryError::Config(format!("Failed to serialize audit report: {}", e)))?;

        if let Some(path) = &self.output {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, json)?;
        } else {
            eprintln!("{}", json);
        }

        if let Some(path) = &self.profile_output {
            let snippet = ProfileSnippet {
                runtime: ProfileRuntime {
                    env_allow: self.observed_env_keys.iter().cloned().collect(),
                },
                security: ProfileSecurity {
                    writable_paths: self.observed_write_paths.iter().cloned().collect(),
                },
            };

            let yaml = serde_yaml::to_string(&snippet).map_err(|e| {
                UentryError::Config(format!("Failed to serialize profile snippet: {}", e))
            })?;

            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, yaml)?;
        }

        Ok(())
    }

    fn ingest_trace_line(&mut self, line: &str) {
        let Some(syscall) = parse_syscall_name(line) else {
            return;
        };

        self.observed_syscalls.insert(syscall.clone());

        let paths = parse_quoted_paths(line);
        if paths.is_empty() {
            return;
        }

        for path in paths {
            if !path.starts_with('/') {
                continue;
            }

            let path_buf = PathBuf::from(path);
            if path_is_write_for_syscall(&syscall, line) {
                self.observed_write_paths.insert(path_buf);
            } else {
                self.observed_read_paths.insert(path_buf);
            }
        }
    }

    fn default_trace_prefix(&self) -> PathBuf {
        if let Some(output) = &self.output {
            let mut trace = output.clone();
            let mut file_name = output
                .file_name()
                .map(|n| n.to_os_string())
                .unwrap_or_else(|| "audit.json".into());
            file_name.push(".strace");
            trace.set_file_name(file_name);
            return trace;
        }

        std::env::temp_dir().join(format!(
            "uentry-audit-{}-{}",
            std::process::id(),
            now_millis()
        ))
    }
}

fn audit_backend_name(backend: &AuditBackend) -> &'static str {
    match backend {
        AuditBackend::Auto => "auto",
        AuditBackend::Strace => "strace",
        AuditBackend::None => "none",
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        return Path::new(command).exists();
    }

    let Ok(path_var) = std::env::var("PATH") else {
        return false;
    };

    path_var
        .split(':')
        .filter(|p| !p.is_empty())
        .map(Path::new)
        .map(|dir| dir.join(command))
        .any(|candidate| candidate.exists())
}

fn resolve_executable(command: &str) -> Option<PathBuf> {
    if command.contains('/') {
        let path = PathBuf::from(command);
        if path.exists() {
            return Some(path);
        }
        return None;
    }

    let path_var = std::env::var("PATH").ok()?;
    path_var
        .split(':')
        .filter(|p| !p.is_empty())
        .map(Path::new)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.exists())
}

fn parse_ldd_path(line: &str) -> Option<String> {
    if let Some((_, rhs)) = line.split_once("=>") {
        let path = rhs.split_whitespace().next().unwrap_or_default();
        if path.starts_with('/') {
            return Some(path.to_string());
        }
    }

    let candidate = line.split_whitespace().next().unwrap_or_default();
    if candidate.starts_with('/') {
        return Some(candidate.to_string());
    }

    None
}

fn trace_files_for_prefix(prefix: &Path) -> Vec<PathBuf> {
    let parent = prefix.parent().unwrap_or_else(|| Path::new("."));
    let Some(base) = prefix.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with(base) {
                    files.push(path);
                }
            }
        }
    }

    if files.is_empty() && prefix.exists() {
        files.push(prefix.to_path_buf());
    }

    files
}

fn parse_syscall_name(line: &str) -> Option<String> {
    let open_paren = line.find('(')?;
    let left = line[..open_paren].trim_end();
    let syscall = left.split_whitespace().last()?;
    if syscall
        .chars()
        .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
    {
        Some(syscall.to_string())
    } else {
        None
    }
}

fn parse_quoted_paths(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut previous_was_escape = false;

    for ch in line.chars() {
        if in_quotes {
            if ch == '"' && !previous_was_escape {
                in_quotes = false;
                if current.starts_with('/') {
                    out.push(current.clone());
                }
                current.clear();
                previous_was_escape = false;
                continue;
            }

            if ch == '\\' && !previous_was_escape {
                previous_was_escape = true;
                continue;
            }

            previous_was_escape = false;
            current.push(ch);
        } else if ch == '"' {
            in_quotes = true;
            current.clear();
            previous_was_escape = false;
        }
    }

    out
}

fn path_is_write_for_syscall(syscall: &str, line: &str) -> bool {
    match syscall {
        "open" | "openat" => {
            line.contains("O_WRONLY")
                || line.contains("O_RDWR")
                || line.contains("O_CREAT")
                || line.contains("O_TRUNC")
                || line.contains("O_APPEND")
        }
        "creat" | "write" | "pwrite64" | "rename" | "renameat" | "renameat2" | "unlink"
        | "unlinkat" | "mkdir" | "mkdirat" | "rmdir" | "chmod" | "fchmod" | "fchmodat"
        | "chown" | "fchown" | "fchownat" | "symlink" | "symlinkat" | "link" | "linkat"
        | "truncate" | "ftruncate" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_syscall_name() {
        assert_eq!(
            parse_syscall_name("12345 openat(AT_FDCWD, \"/etc/hosts\", O_RDONLY) = 3"),
            Some("openat".to_string())
        );
    }

    #[test]
    fn test_parse_quoted_paths() {
        let paths = parse_quoted_paths("openat(AT_FDCWD, \"/etc/hosts\", O_RDONLY) = 3");
        assert_eq!(paths, vec!["/etc/hosts".to_string()]);
    }

    #[test]
    fn test_path_is_write_for_syscall() {
        assert!(path_is_write_for_syscall(
            "openat",
            "openat(AT_FDCWD, \"/tmp/x\", O_WRONLY|O_CREAT, 0666) = 3"
        ));
        assert!(!path_is_write_for_syscall(
            "openat",
            "openat(AT_FDCWD, \"/etc/hosts\", O_RDONLY) = 3"
        ));
    }
}
