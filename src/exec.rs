//! Process execution module.
//!
//! This module handles forking, executing, and waiting for child processes.

use crate::audit::AuditSession;
use crate::config::Config;
use crate::error::{Result, UentryError};
use crate::pid1::SignalHandler;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{execvp, fork, ForkResult};
use std::ffi::CString;
use tracing::{debug, error, info};

/// Execute a command with the given configuration.
///
/// This function:
/// 1. Creates any configured directories
/// 2. Sets environment variables from config
/// 3. Forks and executes the command
/// 4. Waits for the child process to exit
///
/// # Returns
///
/// The exit code of the child process, or 128+signal for signal termination.
pub fn execute(
    command: &[String],
    config: &Config,
    signal_handler: &mut SignalHandler,
    mut audit: Option<&mut AuditSession>,
) -> Result<i32> {
    let audit_for_dirs = audit.as_deref_mut();
    ensure_directories(config, audit_for_dirs)?;

    if let Some(session) = audit.as_mut() {
        session.record_env_keys(config.runtime.env.keys().cloned());
    }

    for (key, value) in &config.runtime.env {
        std::env::set_var(key, value);
    }

    if let Some(user) = &config.runtime.user {
        debug!("Dropping privileges to user: {}", user);
    }

    let mut command_to_execute = command.to_vec();
    if let Some(session) = audit.as_mut() {
        session.record_command(command);
        session.detect_linked_libraries(command);
        command_to_execute = session.prepare_command_for_exec(command);
    }

    let (cmd, args) = parse_command(&command_to_execute)?;

    info!("Executing: {} {:?}", cmd, args);

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            info!("Spawned child process: {}", child);
            signal_handler.set_child(child);
            let result = wait_for_child(signal_handler);
            if let Some(session) = audit.as_mut() {
                session.ingest_trace_output();
            }
            result
        }
        Ok(ForkResult::Child) => {
            exec_child(&cmd, &args)?;
            unreachable!()
        }
        Err(e) => {
            error!("Fork failed: {}", e);
            Err(UentryError::ExecFailed {
                command: cmd.clone(),
                source: e.into(),
            })
        }
    }
}

/// Parse a command into the executable and arguments.
fn parse_command(command: &[String]) -> Result<(String, Vec<CString>)> {
    if command.is_empty() {
        return Err(UentryError::InvalidArgument(
            "No command provided".to_string(),
        ));
    }

    let cmd = command[0].clone();
    let args: Vec<CString> = command
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap_or_default())
        .collect();

    Ok((cmd, args))
}

/// Execute the child process (called in child after fork).
fn exec_child(cmd: &str, args: &[CString]) -> Result<()> {
    let c_cmd = CString::new(cmd)
        .map_err(|e| UentryError::Exec(format!("Invalid command string: {}", e)))?;

    execvp(&c_cmd, args).map_err(|e| {
        error!("execvp failed: {}", e);
        UentryError::ExecFailed {
            command: cmd.to_string(),
            source: e.into(),
        }
    })?;

    Ok(())
}

/// Create directories specified in the configuration.
fn ensure_directories(config: &Config, mut audit: Option<&mut AuditSession>) -> Result<()> {
    for dir in &config.runtime.ensure_dirs {
        let existed = dir.path.exists();
        if !existed {
            std::fs::create_dir_all(&dir.path).map_err(|e| {
                UentryError::Io(std::io::Error::other(format!(
                    "Failed to create directory {:?}: {}",
                    dir.path, e
                )))
            })?;
            debug!("Created directory: {:?}", dir.path);
        }

        if let Some(session) = audit.as_deref_mut() {
            session.record_ensured_directory(&dir.path, !existed);
        }
    }
    Ok(())
}

/// Wait for the child process to exit.
///
/// Returns the exit code (0-255 for normal exit, 128+signal for signals).
pub fn wait_for_child(signal_handler: &SignalHandler) -> Result<i32> {
    loop {
        signal_handler.forward_to_child();

        match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_pid, status)) => {
                info!("Child exited with status: {}", status);
                return Ok(status);
            }
            Ok(WaitStatus::Signaled(_pid, sig, _core)) => {
                let exit_code = 128 + sig as i32;
                info!("Child killed by signal {:?}, exit code: {}", sig, exit_code);
                return Ok(exit_code);
            }
            Ok(WaitStatus::StillAlive) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => {
                error!("waitpid error: {}", e);
                return Err(UentryError::Exec(e.to_string()));
            }
            Ok(_) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_simple() {
        let cmd = vec!["echo".to_string(), "hello".to_string()];
        let (name, args) = parse_command(&cmd).unwrap();
        assert_eq!(name, "echo");
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn test_parse_command_empty() {
        let cmd: Vec<String> = vec![];
        let result = parse_command(&cmd);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, UentryError::InvalidArgument(_)));
    }

    #[test]
    fn test_parse_command_single() {
        let cmd = vec!["ls".to_string()];
        let (name, args) = parse_command(&cmd).unwrap();
        assert_eq!(name, "ls");
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_ensure_directories_empty() {
        let config = Config::default();
        let result = ensure_directories(&config, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ensure_directories_creates_dirs() {
        let temp_dir = std::env::temp_dir().join("uentry_test_ensure_dirs");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let mut config = Config::default();
        config.runtime.ensure_dirs = vec![crate::config::schema::DirConfig::new(temp_dir.clone())];

        let result = ensure_directories(&config, None);
        assert!(result.is_ok());
        assert!(temp_dir.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_directories_existing_dir() {
        let temp_dir = std::env::temp_dir().join("uentry_test_existing");
        std::fs::create_dir_all(&temp_dir).ok();

        let mut config = Config::default();
        config.runtime.ensure_dirs = vec![crate::config::schema::DirConfig::new(temp_dir.clone())];

        let result = ensure_directories(&config, None);
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_exec_child_invalid_command() {
        let args = vec![CString::new("nonexistent_command_xyz").unwrap()];
        let result = exec_child("nonexistent_command_xyz", &args);
        assert!(result.is_err());
    }
}
