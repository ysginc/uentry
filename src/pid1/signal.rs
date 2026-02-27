//! Signal handling for PID 1.
//!
//! This module provides signal handling functionality for when uentry
//! runs as PID 1 in a container:
//!
//! - Signal forwarding: Forward SIGTERM/SIGINT to the child process
//! - Zombie reaping: Reap child processes to prevent zombies

use nix::sys::signal::{self, SigHandler, Signal};
use nix::unistd::Pid;
use std::sync::atomic::{AtomicI32, Ordering};
use tracing::{debug, error, info};

static PENDING_SIGNAL: AtomicI32 = AtomicI32::new(0);

/// Signal handler for PID 1 operations.
///
/// Manages signal forwarding to child processes and zombie reaping.
pub struct SignalHandler {
    child_pid: Option<Pid>,
}

impl SignalHandler {
    /// Create a new signal handler.
    pub fn new() -> Self {
        Self { child_pid: None }
    }

    /// Set the child PID for signal forwarding.
    pub fn set_child(&mut self, pid: Pid) {
        self.child_pid = Some(pid);
    }

    /// Install signal handlers for SIGTERM, SIGINT, and SIGCHLD.
    ///
    /// # Errors
    ///
    /// Returns an error string if signal handler installation fails.
    pub fn install_handlers(&self) -> Result<(), String> {
        unsafe {
            signal::sigaction(
                Signal::SIGTERM,
                &signal::SigAction::new(
                    SigHandler::Handler(handle_sigterm),
                    signal::SaFlags::empty(),
                    signal::SigSet::empty(),
                ),
            )
            .map_err(|e| format!("Failed to install SIGTERM handler: {}", e))?;

            signal::sigaction(
                Signal::SIGINT,
                &signal::SigAction::new(
                    SigHandler::Handler(handle_sigint),
                    signal::SaFlags::empty(),
                    signal::SigSet::empty(),
                ),
            )
            .map_err(|e| format!("Failed to install SIGINT handler: {}", e))?;

            signal::sigaction(
                Signal::SIGCHLD,
                &signal::SigAction::new(
                    SigHandler::Handler(handle_sigchld),
                    signal::SaFlags::empty(),
                    signal::SigSet::empty(),
                ),
            )
            .map_err(|e| format!("Failed to install SIGCHLD handler: {}", e))?;
        }

        info!("Signal handlers installed");
        Ok(())
    }

    /// Forward any pending signal to the child process.
    pub fn forward_to_child(&self) {
        if let Some(child_pid) = self.child_pid {
            let sig = PENDING_SIGNAL.load(Ordering::SeqCst);
            if sig != 0 {
                let signal = match sig {
                    15 => Signal::SIGTERM,
                    2 => Signal::SIGINT,
                    _ => return,
                };

                debug!("Forwarding signal {:?} to child {}", signal, child_pid);
                if let Err(e) = signal::kill(child_pid, signal) {
                    error!("Failed to forward signal to child: {}", e);
                }
                PENDING_SIGNAL.store(0, Ordering::SeqCst);
            }
        }
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

extern "C" fn handle_sigterm(_sig: i32) {
    PENDING_SIGNAL.store(15, Ordering::SeqCst);
}

extern "C" fn handle_sigint(_sig: i32) {
    PENDING_SIGNAL.store(2, Ordering::SeqCst);
}

extern "C" fn handle_sigchld(_sig: i32) {
    reap_zombies();
}

/// Reap zombie child processes.
fn reap_zombies() {
    use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};

    loop {
        match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(pid, status)) => {
                debug!("Reaped zombie process {} with exit status {}", pid, status);
            }
            Ok(WaitStatus::Signaled(pid, sig, _core)) => {
                debug!("Reaped zombie process {} killed by signal {:?}", pid, sig);
            }
            Ok(WaitStatus::StillAlive) | Err(_) => break,
            Ok(_) => continue,
        }
    }
}

/// Check if the current process is running as PID 1.
pub fn is_pid1() -> bool {
    nix::unistd::getpid() == Pid::from_raw(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_handler_new() {
        let handler = SignalHandler::new();
        assert!(handler.child_pid.is_none());
    }

    #[test]
    fn test_signal_handler_default() {
        let handler = SignalHandler::default();
        assert!(handler.child_pid.is_none());
    }

    #[test]
    fn test_signal_handler_set_child() {
        let mut handler = SignalHandler::new();
        handler.set_child(Pid::from_raw(1234));
        assert_eq!(handler.child_pid, Some(Pid::from_raw(1234)));
    }

    #[test]
    fn test_is_pid1() {
        let result = is_pid1();
        assert!(!result);
    }

    #[test]
    fn test_pending_signal_initial() {
        assert_eq!(PENDING_SIGNAL.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_forward_to_child_no_child() {
        let handler = SignalHandler::new();
        handler.forward_to_child();
    }

    #[test]
    fn test_forward_to_child_with_child_no_signal() {
        let mut handler = SignalHandler::new();
        handler.set_child(Pid::from_raw(9999));
        handler.forward_to_child();
    }
}
