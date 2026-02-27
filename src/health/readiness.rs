//! Readiness probe implementation.
//!
//! This module implements readiness checks with support for:
//! - HTTP probes
//! - TCP probes
//! - Exec probes

use crate::config::schema::{
    ExecProbeConfig, HttpProbeConfig, ProbeConfig, ReadinessConfig, TcpProbeConfig,
};
use crate::error::{Result, UentryError};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Result of a readiness probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeResult {
    Ready,
    NotReady,
    Failed,
}

/// Readiness checker.
#[derive(Debug, Clone)]
pub struct ReadinessChecker {
    config: ReadinessConfig,
    attempts: u32,
}

impl ReadinessChecker {
    /// Create a new readiness checker.
    pub fn new(config: ReadinessConfig) -> Self {
        Self {
            config,
            attempts: 0,
        }
    }

    /// Wait for initial delay before starting probes.
    pub fn wait_initial_delay(&self) {
        if self.config.initial_delay_secs > 0 {
            info!(
                "Waiting {}s before readiness probe",
                self.config.initial_delay_secs
            );
            std::thread::sleep(Duration::from_secs(self.config.initial_delay_secs));
        }
    }

    /// Run readiness probe until ready or max retries exceeded.
    pub fn wait_for_ready(&mut self) -> Result<ProbeResult> {
        self.wait_initial_delay();

        loop {
            self.attempts += 1;

            match self.probe() {
                Ok(ProbeResult::Ready) => {
                    info!("Readiness probe passed after {} attempts", self.attempts);
                    return Ok(ProbeResult::Ready);
                }
                Ok(ProbeResult::NotReady) => {
                    if self.attempts >= self.config.retries {
                        warn!("Readiness probe failed after {} attempts", self.attempts);
                        return Ok(ProbeResult::NotReady);
                    }
                    debug!(
                        "Readiness probe not ready, waiting {}s",
                        self.config.interval_secs
                    );
                    std::thread::sleep(Duration::from_secs(self.config.interval_secs));
                }
                Ok(ProbeResult::Failed) => {
                    if self.attempts >= self.config.retries {
                        error!("Readiness probe failed after {} attempts", self.attempts);
                        return Ok(ProbeResult::Failed);
                    }
                    std::thread::sleep(Duration::from_secs(self.config.interval_secs));
                }
                Err(e) => {
                    error!("Readiness probe error: {}", e);
                    if self.attempts >= self.config.retries {
                        return Ok(ProbeResult::Failed);
                    }
                    std::thread::sleep(Duration::from_secs(self.config.interval_secs));
                }
            }
        }
    }

    /// Execute a single probe.
    pub fn probe(&self) -> Result<ProbeResult> {
        match &self.config.probe {
            ProbeConfig::Http { http_get } => self.http_probe(http_get),
            ProbeConfig::Tcp { tcp_socket } => self.tcp_probe(tcp_socket),
            ProbeConfig::Exec { exec } => self.exec_probe(exec),
        }
    }

    /// HTTP probe implementation.
    fn http_probe(&self, config: &HttpProbeConfig) -> Result<ProbeResult> {
        let host = config.host.as_deref().unwrap_or("localhost");
        let scheme = config.scheme.as_deref().unwrap_or("http");
        let url = format!("{}://{}:{}{}", scheme, host, config.port, config.path);

        debug!("HTTP probe: GET {}", url);

        let timeout = Duration::from_secs(self.config.timeout_secs);
        let addr = format!("{}:{}", host, config.port);

        let start = Instant::now();

        let stream = TcpStream::connect(&addr);
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                debug!("HTTP probe connection failed: {}", e);
                return Ok(ProbeResult::NotReady);
            }
        };

        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
            config.path, host, config.port
        );

        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.write_all(request.as_bytes())?;

        let mut response = String::new();
        stream.read_to_string(&mut response)?;

        let elapsed = start.elapsed();

        if response.starts_with("HTTP/1.1 2") || response.starts_with("HTTP/1.0 2") {
            debug!("HTTP probe successful in {:?}", elapsed);
            Ok(ProbeResult::Ready)
        } else {
            let status = response.lines().next().unwrap_or("unknown");
            debug!("HTTP probe returned non-2xx: {}", status);
            Ok(ProbeResult::NotReady)
        }
    }

    /// TCP probe implementation.
    fn tcp_probe(&self, config: &TcpProbeConfig) -> Result<ProbeResult> {
        let host = config.host.as_deref().unwrap_or("localhost");
        let addr = format!("{}:{}", host, config.port);

        debug!("TCP probe: {}", addr);

        match TcpStream::connect(&addr) {
            Ok(_) => {
                debug!("TCP probe successful");
                Ok(ProbeResult::Ready)
            }
            Err(e) => {
                debug!("TCP probe failed: {}", e);
                Ok(ProbeResult::NotReady)
            }
        }
    }

    /// Exec probe implementation.
    fn exec_probe(&self, config: &ExecProbeConfig) -> Result<ProbeResult> {
        if config.command.is_empty() {
            return Err(UentryError::Config(
                "Exec probe command is empty".to_string(),
            ));
        }

        debug!("Exec probe: {:?}", config.command);

        let timeout = Duration::from_secs(self.config.timeout_secs);
        let start = Instant::now();

        let mut child = Command::new(&config.command[0])
            .args(&config.command[1..])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| UentryError::Exec(format!("Failed to spawn exec probe: {}", e)))?;

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let elapsed = start.elapsed();
                    if status.success() {
                        debug!("Exec probe successful in {:?}", elapsed);
                        return Ok(ProbeResult::Ready);
                    } else {
                        debug!("Exec probe failed with status: {:?}", status.code());
                        return Ok(ProbeResult::NotReady);
                    }
                }
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        warn!("Exec probe timed out");
                        return Ok(ProbeResult::Failed);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    return Err(UentryError::Exec(format!(
                        "Failed to wait for exec probe: {}",
                        e
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_http_config(port: u16, path: &str) -> ReadinessConfig {
        ReadinessConfig {
            initial_delay_secs: 0,
            interval_secs: 1,
            timeout_secs: 5,
            retries: 3,
            probe: ProbeConfig::Http {
                http_get: HttpProbeConfig {
                    path: path.to_string(),
                    port,
                    host: Some("localhost".to_string()),
                    scheme: Some("http".to_string()),
                },
            },
        }
    }

    fn make_exec_config(command: Vec<String>) -> ReadinessConfig {
        ReadinessConfig {
            initial_delay_secs: 0,
            interval_secs: 1,
            timeout_secs: 5,
            retries: 3,
            probe: ProbeConfig::Exec {
                exec: ExecProbeConfig { command },
            },
        }
    }

    fn make_tcp_config(port: u16) -> ReadinessConfig {
        ReadinessConfig {
            initial_delay_secs: 0,
            interval_secs: 1,
            timeout_secs: 5,
            retries: 3,
            probe: ProbeConfig::Tcp {
                tcp_socket: TcpProbeConfig {
                    port,
                    host: Some("localhost".to_string()),
                },
            },
        }
    }

    #[test]
    fn test_checker_new() {
        let config = make_http_config(8080, "/health");
        let checker = ReadinessChecker::new(config);
        assert_eq!(checker.attempts, 0);
    }

    #[test]
    fn test_exec_probe_success() {
        let config = make_exec_config(vec!["true".to_string()]);
        let checker = ReadinessChecker::new(config);
        let result = checker.probe().unwrap();
        assert_eq!(result, ProbeResult::Ready);
    }

    #[test]
    fn test_exec_probe_failure() {
        let config = make_exec_config(vec!["false".to_string()]);
        let checker = ReadinessChecker::new(config);
        let result = checker.probe().unwrap();
        assert_eq!(result, ProbeResult::NotReady);
    }

    #[test]
    fn test_tcp_probe_not_ready() {
        let config = make_tcp_config(59999);
        let checker = ReadinessChecker::new(config);
        let result = checker.probe().unwrap();
        assert_eq!(result, ProbeResult::NotReady);
    }

    #[test]
    fn test_probe_result_equality() {
        assert_eq!(ProbeResult::Ready, ProbeResult::Ready);
        assert_ne!(ProbeResult::Ready, ProbeResult::NotReady);
    }
}
