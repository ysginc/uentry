//! Integration tests for uentry.
//!
//! These tests verify end-to-end functionality by executing the binary.

use std::process::Command;

fn uentry_bin() -> String {
    env!("CARGO_BIN_EXE_uentry").to_string()
}

#[test]
fn test_help_flag() {
    let output = Command::new(uentry_bin())
        .arg("--help")
        .output()
        .expect("Failed to execute uentry");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("A minimal init system for containers"));
    assert!(stdout.contains("--strict"));
    assert!(stdout.contains("--profile"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--diagnose"));
}

#[test]
fn test_version_flag() {
    let output = Command::new(uentry_bin())
        .arg("--version")
        .output()
        .expect("Failed to execute uentry");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("uentry"));
}

#[test]
fn test_diagnose_flag() {
    let output = Command::new(uentry_bin())
        .arg("--diagnose")
        .output()
        .expect("Failed to execute uentry");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("uentry diagnostics:"));
    assert!(stdout.contains("PID:"));
    assert!(stdout.contains("PID 1:"));
    assert!(stdout.contains("Working directory:"));
    assert!(stdout.contains("Config path:"));
}

#[test]
fn test_execute_echo() {
    let output = Command::new(uentry_bin())
        .args(["echo", "hello", "world"])
        .output()
        .expect("Failed to execute uentry");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello world"));
}

#[test]
fn test_execute_true() {
    let output = Command::new(uentry_bin())
        .arg("true")
        .output()
        .expect("Failed to execute uentry");

    assert!(output.status.success());
}

#[test]
fn test_execute_false() {
    let output = Command::new(uentry_bin())
        .arg("false")
        .output()
        .expect("Failed to execute uentry");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn test_execute_exit_code() {
    let output = Command::new(uentry_bin())
        .args(["sh", "-c", "exit 42"])
        .output()
        .expect("Failed to execute uentry");

    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn test_execute_nonexistent_command() {
    let output = Command::new(uentry_bin())
        .arg("nonexistent_command_xyz_12345")
        .output()
        .expect("Failed to execute uentry");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Execution error") || stderr.contains("error"));
}

#[test]
fn test_no_command_no_diagnose() {
    let output = Command::new(uentry_bin())
        .output()
        .expect("Failed to execute uentry");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("COMMAND is required"));
}

#[test]
fn test_strict_flag() {
    let output = Command::new(uentry_bin())
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_PRELOAD")
        .args(["--strict", "echo", "test"])
        .output()
        .expect("Failed to execute uentry");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test"));
}

#[test]
fn test_with_env_var() {
    let output = Command::new(uentry_bin())
        .env("UENTRY_ENV_TEST_VAR", "test_value")
        .args(["sh", "-c", "echo $TEST_VAR"])
        .output()
        .expect("Failed to execute uentry");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test_value"));
}
