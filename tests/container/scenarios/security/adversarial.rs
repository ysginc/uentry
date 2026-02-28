use super::helpers::{combined_logs, ensure_docker, run_container};
use crate::container::fixtures::*;

fn strict_adversarial_config() -> &'static str {
    r#"
runtime:
  strict: true

security:
  allow_root: true
  no_new_privs: true
  writable_paths:
    - /tmp
"#
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_blocks_root_without_privilege_drop() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let config = r#"
runtime:
  strict: true
"#;
    ctx.write_config("config.yaml", config);

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "echo strict-root-block"]
"#;

    let image = build_test_image(&ctx, "strict-root-block", dockerfile).await;

    let output = run_container(&image, &[]).await;
    assert!(
        !output.status.success(),
        "Container should fail in strict root mode"
    );

    let logs = combined_logs(&output);
    assert!(
        logs.contains("Refusing to run as root"),
        "Expected root refusal message in logs, got: {}",
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_cannot_be_disabled_by_env_override() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let config = r#"
runtime:
  strict: true
"#;
    ctx.write_config("config.yaml", config);

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "echo strict-override-attempt"]
"#;

    let image = build_test_image(&ctx, "strict-override-env", dockerfile).await;

    let args = vec!["-e".to_string(), "UENTRY_STRICT=false".to_string()];
    let output = run_container(&image, &args).await;

    assert!(
        !output.status.success(),
        "Container should still fail in strict mode despite env override"
    );

    let logs = combined_logs(&output);
    assert!(
        logs.contains("Refusing to run as root") || logs.contains("Strict mode"),
        "Expected strict-mode rejection in logs, got: {}",
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_blocks_dangerous_env_injection() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    ctx.write_config("config.yaml", strict_adversarial_config());

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "echo env-injection-attempt"]
"#;

    let image = build_test_image(&ctx, "strict-env-injection", dockerfile).await;

    let args = vec![
        "-e".to_string(),
        "LD_LIBRARY_PATH=/tmp/escape-attempt".to_string(),
    ];
    let output = run_container(&image, &args).await;
    assert!(
        !output.status.success(),
        "Container should fail when dangerous env vars are injected"
    );

    let logs = combined_logs(&output);
    assert!(
        logs.contains("dangerous environment variables") || logs.contains("LD_LIBRARY_PATH"),
        "Expected dangerous env var rejection in logs, got: {}",
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_blocks_ld_preload_injection() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    ctx.write_config("config.yaml", strict_adversarial_config());

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "echo ld-preload-injection-attempt"]
"#;

    let image = build_test_image(&ctx, "strict-ld-preload", dockerfile).await;

    let args = vec!["-e".to_string(), "LD_PRELOAD=/tmp/evil.so".to_string()];
    let output = run_container(&image, &args).await;
    assert!(
        !output.status.success(),
        "Container should fail when LD_PRELOAD is injected"
    );

    let logs = combined_logs(&output);
    assert!(
        logs.contains("dangerous environment variables") || logs.contains("LD_PRELOAD"),
        "Expected LD_PRELOAD rejection in logs, got: {}",
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_blocks_docker_socket_mount() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    ctx.write_config("config.yaml", strict_adversarial_config());

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "echo docker-socket-mount-attempt"]
"#;

    let image = build_test_image(&ctx, "strict-docker-socket", dockerfile).await;

    let fake_socket = ctx.temp_dir.path().join("fake-docker.sock");
    std::fs::write(&fake_socket, b"not-a-socket").expect("Failed to create fake docker socket");

    let args = vec![
        "-v".to_string(),
        format!("{}:/var/run/docker.sock", fake_socket.display()),
    ];
    let output = run_container(&image, &args).await;
    assert!(
        !output.status.success(),
        "Container should fail when docker socket is mounted"
    );

    let logs = combined_logs(&output);
    assert!(
        logs.contains("forbidden mounts") || logs.contains("/var/run/docker.sock"),
        "Expected forbidden mount rejection in logs, got: {}",
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_blocks_run_docker_socket_mount() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    ctx.write_config("config.yaml", strict_adversarial_config());

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "echo run-docker-socket-mount-attempt"]
"#;

    let image = build_test_image(&ctx, "strict-run-docker-socket", dockerfile).await;

    let fake_socket = ctx.temp_dir.path().join("fake-docker-2.sock");
    std::fs::write(&fake_socket, b"not-a-socket").expect("Failed to create fake docker socket");

    let args = vec![
        "-v".to_string(),
        format!("{}:/run/docker.sock", fake_socket.display()),
    ];
    let output = run_container(&image, &args).await;
    assert!(
        !output.status.success(),
        "Container should fail when /run/docker.sock is mounted"
    );

    let logs = combined_logs(&output);
    assert!(
        logs.contains("forbidden mounts") || logs.contains("/run/docker.sock"),
        "Expected forbidden mount rejection in logs, got: {}",
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_contains_mount_syscall_attempt() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    ctx.write_config("config.yaml", strict_adversarial_config());

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "mkdir -p /tmp/escape-mnt && mount -t tmpfs tmpfs /tmp/escape-mnt >/tmp/mount.log 2>&1 && echo MOUNT_ALLOWED || echo MOUNT_BLOCKED"]
"#;

    let image = build_test_image(&ctx, "strict-mount-syscall", dockerfile).await;

    let output = run_container(&image, &[]).await;
    let logs = combined_logs(&output);
    let mount_allowed = logs.lines().any(|line| line.trim() == "MOUNT_ALLOWED");
    let mount_blocked = logs.lines().any(|line| line.trim() == "MOUNT_BLOCKED");

    assert!(
        !mount_allowed,
        "Mount-based breakout attempt should not be allowed, got logs: {}",
        logs
    );
    assert!(
        mount_blocked || logs.contains("Strict mode"),
        "Expected mount syscall attempt to be blocked, got logs: {}",
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_contains_proc_sys_reconfigure_attempt() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    ctx.write_config("config.yaml", strict_adversarial_config());

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "echo hacked >/proc/sys/kernel/hostname 2>/tmp/proc-sys.log && echo PROC_SYS_ALLOWED || echo PROC_SYS_BLOCKED"]
"#;

    let image = build_test_image(&ctx, "strict-proc-sys-reconfigure", dockerfile).await;

    let output = run_container(&image, &[]).await;
    let logs = combined_logs(&output);
    let sysctl_allowed = logs.lines().any(|line| line.trim() == "PROC_SYS_ALLOWED");
    let sysctl_blocked = logs.lines().any(|line| line.trim() == "PROC_SYS_BLOCKED");

    assert!(
        !sysctl_allowed,
        "Kernel reconfiguration attempt should not be allowed, got logs: {}",
        logs
    );
    assert!(
        sysctl_blocked || logs.contains("Strict mode"),
        "Expected /proc/sys reconfiguration attempt to be blocked, got logs: {}",
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_strict_mode_contains_ipc_socket_probe_attempt() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    ctx.write_config("config.yaml", strict_adversarial_config());

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
USER root
ENTRYPOINT ["/uentry", "--strict"]
CMD ["/bin/sh", "-c", "if [ -S /var/run/docker.sock ] || [ -S /run/docker.sock ] || [ -S /proc/1/root/var/run/docker.sock ]; then echo IPC_SOCKET_PRESENT; else echo IPC_SOCKET_BLOCKED; fi"]
"#;

    let image = build_test_image(&ctx, "strict-ipc-probe", dockerfile).await;

    let output = run_container(&image, &[]).await;
    let logs = combined_logs(&output);
    let ipc_present = logs.lines().any(|line| line.trim() == "IPC_SOCKET_PRESENT");
    let ipc_blocked = logs.lines().any(|line| line.trim() == "IPC_SOCKET_BLOCKED");

    assert!(
        !ipc_present,
        "IPC socket probe unexpectedly found privileged channel, logs: {}",
        logs
    );
    assert!(
        ipc_blocked || logs.contains("forbidden mounts"),
        "Expected IPC probe attempt to be blocked, got logs: {}",
        logs
    );
}
