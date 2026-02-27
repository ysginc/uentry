//! Privilege management.
//!
//! This module implements privilege dropping and security hardening:
//!
//! - setuid/setgid dropping (TASK-019)
//! - Supplementary groups handling (TASK-020)
//! - umask configuration (TASK-021)
//! - PR_SET_NO_NEW_PRIVS (TASK-022)

use crate::error::{Result, UentryError};
use nix::unistd::{Gid, Uid};
use std::fs;
use std::io::{BufRead, BufReader};
use tracing::{debug, info, warn};

/// Privilege configuration for dropping privileges.
#[derive(Debug, Clone, Default)]
pub struct PrivilegeConfig {
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub groups: Vec<u32>,
    pub umask: Option<u16>,
    pub no_new_privs: bool,
}

impl PrivilegeConfig {
    /// Create a new privilege configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the target UID.
    pub fn with_uid(mut self, uid: u32) -> Self {
        self.uid = Some(uid);
        self
    }

    /// Set the target GID.
    pub fn with_gid(mut self, gid: u32) -> Self {
        self.gid = Some(gid);
        self
    }

    /// Set supplementary groups.
    pub fn with_groups(mut self, groups: Vec<u32>) -> Self {
        self.groups = groups;
        self
    }

    /// Set the umask.
    pub fn with_umask(mut self, umask: u16) -> Self {
        self.umask = Some(umask);
        self
    }

    /// Enable NO_NEW_PRIVS.
    pub fn with_no_new_privs(mut self, enabled: bool) -> Self {
        self.no_new_privs = enabled;
        self
    }

    /// Resolve a username to UID.
    pub fn resolve_user(username: &str) -> Result<u32> {
        let file = fs::File::open("/etc/passwd")
            .map_err(|e| UentryError::Security(format!("Cannot open /etc/passwd: {}", e)))?;

        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line =
                line.map_err(|e| UentryError::Security(format!("Error reading passwd: {}", e)))?;
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 && parts[0] == username {
                return parts[2].parse().map_err(|_| {
                    UentryError::Security(format!("Invalid UID for user {}", username))
                });
            }
        }

        Err(UentryError::Security(format!(
            "User '{}' not found",
            username
        )))
    }

    /// Resolve a group name to GID.
    pub fn resolve_group(groupname: &str) -> Result<u32> {
        let file = fs::File::open("/etc/group")
            .map_err(|e| UentryError::Security(format!("Cannot open /etc/group: {}", e)))?;

        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line =
                line.map_err(|e| UentryError::Security(format!("Error reading group: {}", e)))?;
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 && parts[0] == groupname {
                return parts[2].parse().map_err(|_| {
                    UentryError::Security(format!("Invalid GID for group {}", groupname))
                });
            }
        }

        Err(UentryError::Security(format!(
            "Group '{}' not found",
            groupname
        )))
    }
}

/// Drop privileges according to the configuration.
pub fn drop_privileges(config: &PrivilegeConfig) -> Result<()> {
    let current_uid = unsafe { libc::getuid() };

    if current_uid != 0 {
        debug!("Not running as root, skipping privilege drop");
        return Ok(());
    }

    if config.uid.is_none() && config.gid.is_none() {
        warn!("Running as root but no privilege drop configured");
        return Ok(());
    }

    if let Some(gid) = config.gid {
        drop_gid(gid)?;
    }

    if !config.groups.is_empty() {
        set_supplementary_groups(&config.groups)?;
    }

    if let Some(uid) = config.uid {
        drop_uid(uid)?;
    }

    if let Some(mask) = config.umask {
        set_umask(mask)?;
    }

    if config.no_new_privs {
        set_no_new_privs()?;
    }

    info!("Privileges dropped successfully");
    Ok(())
}

/// TASK-019: Drop GID.
fn drop_gid(gid: u32) -> Result<()> {
    let gid = Gid::from_raw(gid);
    debug!("Dropping GID to {}", gid);

    nix::unistd::setgid(gid)
        .map_err(|e| UentryError::Security(format!("Failed to set GID to {}: {}", gid, e)))?;

    Ok(())
}

/// TASK-019: Drop UID (must be called after GID).
fn drop_uid(uid: u32) -> Result<()> {
    let uid = Uid::from_raw(uid);
    debug!("Dropping UID to {}", uid);

    nix::unistd::setuid(uid)
        .map_err(|e| UentryError::Security(format!("Failed to set UID to {}: {}", uid, e)))?;

    Ok(())
}

/// TASK-020: Set supplementary groups.
fn set_supplementary_groups(groups: &[u32]) -> Result<()> {
    debug!("Setting supplementary groups: {:?}", groups);

    let gids: Vec<Gid> = groups.iter().map(|&g| Gid::from_raw(g)).collect();

    nix::unistd::setgroups(&gids)
        .map_err(|e| UentryError::Security(format!("Failed to set supplementary groups: {}", e)))?;

    Ok(())
}

/// TASK-021: Set umask.
fn set_umask(mask: u16) -> Result<()> {
    debug!("Setting umask to {:03o}", mask);

    unsafe {
        libc::umask(mask as libc::mode_t);
    }

    Ok(())
}

/// TASK-022: Set NO_NEW_PRIVS to prevent privilege escalation.
fn set_no_new_privs() -> Result<()> {
    debug!("Setting NO_NEW_PRIVS");

    let result = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };

    if result != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(UentryError::Security(format!(
            "Failed to set NO_NEW_PRIVS: {}",
            errno
        )));
    }

    Ok(())
}

/// Set NO_NEW_PRIVS without dropping other privileges.
pub fn set_no_new_privs_only() -> Result<()> {
    set_no_new_privs()
}

/// Check if we can drop privileges (running as root with target config).
pub fn can_drop_privileges(config: &PrivilegeConfig) -> bool {
    let is_root = unsafe { libc::getuid() == 0 };
    is_root && (config.uid.is_some() || config.gid.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_privilege_config_new() {
        let config = PrivilegeConfig::new();
        assert!(config.uid.is_none());
        assert!(config.gid.is_none());
        assert!(config.groups.is_empty());
        assert!(config.umask.is_none());
        assert!(!config.no_new_privs);
    }

    #[test]
    fn test_privilege_config_builder() {
        let config = PrivilegeConfig::new()
            .with_uid(1000)
            .with_gid(1000)
            .with_groups(vec![100, 101])
            .with_umask(0o022)
            .with_no_new_privs(true);

        assert_eq!(config.uid, Some(1000));
        assert_eq!(config.gid, Some(1000));
        assert_eq!(config.groups, vec![100, 101]);
        assert_eq!(config.umask, Some(0o022));
        assert!(config.no_new_privs);
    }

    #[test]
    fn test_can_drop_privileges_not_root() {
        let config = PrivilegeConfig::new().with_uid(1000);
        if unsafe { libc::getuid() != 0 } {
            assert!(!can_drop_privileges(&config));
        }
    }

    #[test]
    fn test_can_drop_privileges_no_target() {
        let config = PrivilegeConfig::new();
        assert!(!can_drop_privileges(&config));
    }

    #[test]
    fn test_resolve_user_not_found() {
        let result = PrivilegeConfig::resolve_user("nonexistent_user_xyz_12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_group_not_found() {
        let result = PrivilegeConfig::resolve_group("nonexistent_group_xyz_12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_user_root() {
        if std::path::Path::new("/etc/passwd").exists() {
            let result = PrivilegeConfig::resolve_user("root");
            if let Ok(uid) = result {
                assert_eq!(uid, 0);
            }
        }
    }

    #[test]
    fn test_resolve_group_root() {
        if std::path::Path::new("/etc/group").exists() {
            let result = PrivilegeConfig::resolve_group("root");
            if let Ok(gid) = result {
                assert_eq!(gid, 0);
            }
        }
    }
}
