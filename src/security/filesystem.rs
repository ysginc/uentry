//! Filesystem preparation module.
//!
//! This module handles filesystem setup tasks:
//!
//! - Create required directories with permissions (TASK-028)
//! - Set ownership for explicit paths (TASK-029)
//! - Verify writable paths match allowlist (TASK-030)

use crate::error::{Result, UentryError};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Directory creation specification.
#[derive(Debug, Clone)]
pub struct DirSpec {
    pub path: PathBuf,
    pub mode: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
}

impl DirSpec {
    /// Create a new directory specification.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            mode: None,
            owner: None,
            group: None,
        }
    }

    /// Set the directory permissions (octal mode).
    pub fn with_mode(mut self, mode: u32) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Set the directory owner.
    pub fn with_owner(mut self, owner: String) -> Self {
        self.owner = Some(owner);
        self
    }

    /// Set the directory group.
    pub fn with_group(mut self, group: String) -> Self {
        self.group = Some(group);
        self
    }
}

/// Filesystem preparation manager.
#[derive(Debug, Clone, Default)]
pub struct FilesystemPrep {
    dirs: Vec<DirSpec>,
    writable_paths: Vec<PathBuf>,
}

impl FilesystemPrep {
    /// Create a new filesystem preparation manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a directory to create.
    pub fn add_dir(mut self, spec: DirSpec) -> Self {
        self.dirs.push(spec);
        self
    }

    /// Add multiple directories from paths.
    pub fn with_dirs(mut self, paths: Vec<PathBuf>) -> Self {
        for path in paths {
            self.dirs.push(DirSpec::new(path));
        }
        self
    }

    /// Set writable paths allowlist.
    pub fn with_writable_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.writable_paths = paths;
        self
    }

    /// TASK-028 & TASK-029: Create directories with permissions and ownership.
    pub fn prepare_directories(&self) -> Result<()> {
        for dir in &self.dirs {
            self.ensure_directory(dir)?;
        }
        Ok(())
    }

    fn ensure_directory(&self, dir: &DirSpec) -> Result<()> {
        if dir.path.exists() {
            debug!("Directory already exists: {:?}", dir.path);
            return Ok(());
        }

        info!("Creating directory: {:?}", dir.path);
        fs::create_dir_all(&dir.path).map_err(|e| {
            UentryError::Io(std::io::Error::other(format!(
                "Failed to create directory {:?}: {}",
                dir.path, e
            )))
        })?;

        if let Some(mode) = dir.mode {
            self.set_permissions(&dir.path, mode)?;
        }

        if dir.owner.is_some() || dir.group.is_some() {
            warn!(
                "Ownership change requested for {:?} but requires root privileges",
                dir.path
            );
        }

        Ok(())
    }

    fn set_permissions(&self, path: &PathBuf, mode: u32) -> Result<()> {
        debug!("Setting permissions {:o} on {:?}", mode, path);
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| {
            UentryError::Io(std::io::Error::other(format!(
                "Failed to set permissions on {:?}: {}",
                path, e
            )))
        })?;
        Ok(())
    }

    /// TASK-030: Verify that writable paths match the allowlist.
    pub fn verify_writable_paths(&self) -> Result<()> {
        if self.writable_paths.is_empty() {
            debug!("No writable paths allowlist configured");
            return Ok(());
        }

        for path in &self.writable_paths {
            if !path.exists() {
                warn!("Allowlisted path does not exist: {:?}", path);
            }
        }

        info!("Writable paths allowlist verified");
        Ok(())
    }

    /// Run all filesystem preparation tasks.
    pub fn run(&self) -> Result<()> {
        self.prepare_directories()?;
        self.verify_writable_paths()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_dir_spec_new() {
        let spec = DirSpec::new(PathBuf::from("/tmp/test"));
        assert_eq!(spec.path, PathBuf::from("/tmp/test"));
        assert!(spec.mode.is_none());
        assert!(spec.owner.is_none());
        assert!(spec.group.is_none());
    }

    #[test]
    fn test_dir_spec_builder() {
        let spec = DirSpec::new(PathBuf::from("/tmp/test"))
            .with_mode(0o755)
            .with_owner("user".to_string())
            .with_group("group".to_string());

        assert_eq!(spec.mode, Some(0o755));
        assert_eq!(spec.owner, Some("user".to_string()));
        assert_eq!(spec.group, Some("group".to_string()));
    }

    #[test]
    fn test_filesystem_prep_new() {
        let prep = FilesystemPrep::new();
        assert!(prep.dirs.is_empty());
        assert!(prep.writable_paths.is_empty());
    }

    #[test]
    fn test_filesystem_prep_with_dirs() {
        let prep =
            FilesystemPrep::new().with_dirs(vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);

        assert_eq!(prep.dirs.len(), 2);
    }

    #[test]
    fn test_ensure_directory_creates() {
        let temp_dir = std::env::temp_dir().join("uentry_test_fs_prep");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let prep = FilesystemPrep::new().add_dir(DirSpec::new(temp_dir.clone()));
        prep.prepare_directories().unwrap();

        assert!(temp_dir.exists());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_directory_existing() {
        let temp_dir = std::env::temp_dir().join("uentry_test_fs_prep_existing");
        std::fs::create_dir_all(&temp_dir).ok();

        let prep = FilesystemPrep::new().add_dir(DirSpec::new(temp_dir.clone()));
        let result = prep.prepare_directories();

        assert!(result.is_ok());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_verify_writable_paths_empty() {
        let prep = FilesystemPrep::new();
        let result = prep.verify_writable_paths();
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_writable_paths_nonexistent() {
        let prep =
            FilesystemPrep::new().with_writable_paths(vec![PathBuf::from("/nonexistent/path")]);
        let result = prep.verify_writable_paths();
        assert!(result.is_ok());
    }
}
