//! Secrets management module.
//!
//! This module handles secrets securely:
//!
//! - Read secrets from files → inject to env (TASK-032)
//! - Read env vars → write to files (TASK-033)
//! - Minimal templating support (TASK-034)
//! - Strict secret redaction in logs (TASK-035)
//! - Permission enforcement on written files (TASK-036)

use crate::error::{Result, UentryError};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Secret file to environment variable mapping.
#[derive(Debug, Clone)]
pub struct FileToEnv {
    pub file: PathBuf,
    pub env_var: String,
    pub optional: bool,
}

impl FileToEnv {
    /// Create a new file-to-env mapping.
    pub fn new(file: PathBuf, env_var: String) -> Self {
        Self {
            file,
            env_var,
            optional: false,
        }
    }

    /// Mark this mapping as optional (skip if file doesn't exist).
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

/// Environment variable to file mapping.
#[derive(Debug, Clone)]
pub struct EnvToFile {
    pub env_var: String,
    pub file: PathBuf,
    pub mode: u32,
}

impl EnvToFile {
    /// Create a new env-to-file mapping with secure default permissions.
    pub fn new(env_var: String, file: PathBuf) -> Self {
        Self {
            env_var,
            file,
            mode: 0o600,
        }
    }

    /// Set custom file permissions.
    pub fn with_mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }
}

/// Secrets manager.
#[derive(Debug, Clone, Default)]
pub struct SecretsManager {
    file_to_env: Vec<FileToEnv>,
    env_to_file: Vec<EnvToFile>,
    secret_values: HashSet<String>,
}

impl SecretsManager {
    /// Create a new secrets manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file-to-env mapping.
    pub fn add_file_to_env(mut self, mapping: FileToEnv) -> Self {
        self.file_to_env.push(mapping);
        self
    }

    /// Add an env-to-file mapping.
    pub fn add_env_to_file(mut self, mapping: EnvToFile) -> Self {
        self.env_to_file.push(mapping);
        self
    }

    /// Get the set of known secret values for redaction.
    pub fn secret_values(&self) -> &HashSet<String> {
        &self.secret_values
    }

    /// TASK-032: Read secrets from files and inject into environment.
    pub fn inject_secrets_from_files(&mut self) -> Result<()> {
        for mapping in &self.file_to_env {
            match fs::read_to_string(&mapping.file) {
                Ok(content) => {
                    let value = content.trim().to_string();
                    self.secret_values.insert(value.clone());
                    std::env::set_var(&mapping.env_var, &value);
                    info!(
                        "Injected secret from {:?} into {}",
                        mapping.file, mapping.env_var
                    );
                }
                Err(e) if mapping.optional => {
                    debug!(
                        "Optional secret file {:?} not found, skipping",
                        mapping.file
                    );
                }
                Err(e) => {
                    return Err(UentryError::Io(std::io::Error::other(format!(
                        "Failed to read secret file {:?}: {}",
                        mapping.file, e
                    ))));
                }
            }
        }
        Ok(())
    }

    /// TASK-033: Read env vars and write to files.
    pub fn write_secrets_to_files(&mut self) -> Result<()> {
        for mapping in &self.env_to_file {
            match std::env::var(&mapping.env_var) {
                Ok(value) => {
                    self.secret_values.insert(value.clone());
                    self.write_secret_file(&mapping.file, &value, mapping.mode)?;

                    if mapping.env_var != "PATH" && !mapping.env_var.ends_with("_PATH") {
                        std::env::remove_var(&mapping.env_var);
                        debug!("Removed env var {} after writing to file", mapping.env_var);
                    }
                }
                Err(_) => {
                    warn!("Environment variable {} not set, skipping", mapping.env_var);
                }
            }
        }
        Ok(())
    }

    /// TASK-036: Write a secret file with secure permissions.
    fn write_secret_file(&self, path: &PathBuf, content: &str, mode: u32) -> Result<()> {
        debug!("Writing secret to {:?} with mode {:o}", path, mode);

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    UentryError::Io(std::io::Error::other(format!(
                        "Failed to create parent directory {:?}: {}",
                        parent, e
                    )))
                })?;
            }
        }

        let mut file = fs::File::create(path).map_err(|e| {
            UentryError::Io(std::io::Error::other(format!(
                "Failed to create secret file {:?}: {}",
                path, e
            )))
        })?;

        file.write_all(content.as_bytes()).map_err(|e| {
            UentryError::Io(std::io::Error::other(format!(
                "Failed to write secret file {:?}: {}",
                path, e
            )))
        })?;

        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| {
            UentryError::Io(std::io::Error::other(format!(
                "Failed to set permissions on {:?}: {}",
                path, e
            )))
        })?;

        info!("Wrote secret to {:?}", path);
        Ok(())
    }

    /// TASK-035: Redact secret values from a string.
    pub fn redact(&self, message: &str) -> String {
        let mut result = message.to_string();
        for secret in &self.secret_values {
            if !secret.is_empty() && secret.len() > 3 {
                let redacted = "*".repeat(secret.len());
                result = result.replace(secret, &redacted);
            }
        }
        result
    }

    /// Run all secrets processing tasks.
    pub fn run(&mut self) -> Result<()> {
        self.inject_secrets_from_files()?;
        self.write_secrets_to_files()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;

    fn create_temp_secret_file(content: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir().join("uentry_test_secrets");
        fs::create_dir_all(&temp_dir).ok();
        let file_path = temp_dir.join(format!("secret_{}", std::process::id()));
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file_path
    }

    #[test]
    fn test_file_to_env_new() {
        let mapping = FileToEnv::new(PathBuf::from("/tmp/secret"), "MY_SECRET".to_string());
        assert_eq!(mapping.file, PathBuf::from("/tmp/secret"));
        assert_eq!(mapping.env_var, "MY_SECRET");
        assert!(!mapping.optional);
    }

    #[test]
    fn test_file_to_env_optional() {
        let mapping =
            FileToEnv::new(PathBuf::from("/tmp/secret"), "MY_SECRET".to_string()).optional();
        assert!(mapping.optional);
    }

    #[test]
    fn test_env_to_file_new() {
        let mapping = EnvToFile::new("MY_SECRET".to_string(), PathBuf::from("/tmp/secret"));
        assert_eq!(mapping.env_var, "MY_SECRET");
        assert_eq!(mapping.file, PathBuf::from("/tmp/secret"));
        assert_eq!(mapping.mode, 0o600);
    }

    #[test]
    fn test_env_to_file_with_mode() {
        let mapping =
            EnvToFile::new("MY_SECRET".to_string(), PathBuf::from("/tmp/secret")).with_mode(0o400);
        assert_eq!(mapping.mode, 0o400);
    }

    #[test]
    fn test_secrets_manager_new() {
        let manager = SecretsManager::new();
        assert!(manager.file_to_env.is_empty());
        assert!(manager.env_to_file.is_empty());
        assert!(manager.secret_values.is_empty());
    }

    #[test]
    fn test_inject_secrets_from_files() {
        let file_path = create_temp_secret_file("my_secret_value\n");

        let mut manager = SecretsManager::new().add_file_to_env(FileToEnv::new(
            file_path.clone(),
            "TEST_SECRET_VAR".to_string(),
        ));

        manager.inject_secrets_from_files().unwrap();

        assert_eq!(std::env::var("TEST_SECRET_VAR").unwrap(), "my_secret_value");
        assert!(manager.secret_values.contains("my_secret_value"));

        std::env::remove_var("TEST_SECRET_VAR");
        let _ = fs::remove_file(&file_path);
    }

    #[test]
    fn test_inject_secrets_optional_missing() {
        let mut manager = SecretsManager::new().add_file_to_env(
            FileToEnv::new(PathBuf::from("/nonexistent/secret"), "TEST_VAR".to_string()).optional(),
        );

        let result = manager.inject_secrets_from_files();
        assert!(result.is_ok());
    }

    #[test]
    fn test_inject_secrets_required_missing() {
        let mut manager = SecretsManager::new().add_file_to_env(FileToEnv::new(
            PathBuf::from("/nonexistent/secret"),
            "TEST_VAR".to_string(),
        ));

        let result = manager.inject_secrets_from_files();
        assert!(result.is_err());
    }

    #[test]
    fn test_write_secrets_to_files() {
        let temp_dir = std::env::temp_dir().join("uentry_test_secrets_write");
        fs::create_dir_all(&temp_dir).ok();
        let file_path = temp_dir.join(format!("output_{}", std::process::id()));

        std::env::set_var("TEST_WRITE_SECRET", "write_secret_value");

        let mut manager = SecretsManager::new().add_env_to_file(EnvToFile::new(
            "TEST_WRITE_SECRET".to_string(),
            file_path.clone(),
        ));

        manager.write_secrets_to_files().unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "write_secret_value");

        let metadata = fs::metadata(&file_path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        std::env::remove_var("TEST_WRITE_SECRET");
        let _ = fs::remove_file(&file_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_redact() {
        let mut manager = SecretsManager::new();
        manager.secret_values.insert("super_secret".to_string());

        let message = "The password is super_secret and should be hidden";
        let redacted = manager.redact(message);

        assert!(redacted.contains("***********"));
        assert!(!redacted.contains("super_secret"));
    }

    #[test]
    fn test_redact_empty_secret() {
        let mut manager = SecretsManager::new();
        manager.secret_values.insert("".to_string());

        let message = "Hello world";
        let redacted = manager.redact(message);

        assert_eq!(redacted, message);
    }

    #[test]
    fn test_redact_short_secret() {
        let mut manager = SecretsManager::new();
        manager.secret_values.insert("ab".to_string());

        let message = "Hello ab world";
        let redacted = manager.redact(message);

        assert_eq!(redacted, message);
    }
}
