//! Derived container tests - overlaying uentry on existing images.

use crate::container::fixtures::*;
use std::time::Duration;
use testcontainers::core::ExecCommand;

static DOCKERGUARD: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();

async fn ensure_docker() -> bool {
    *DOCKERGUARD
        .get_or_init(|| async { check_docker_available().await })
        .await
}

#[tokio::test]
#[serial_test::serial]
async fn test_derive_from_alpine() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
RUN chmod +x /uentry
ENTRYPOINT ["/uentry"]
CMD ["/bin/sh", "-c", "echo 'hello from derived alpine' && sleep 30"]
"#;

    let _image = build_test_image(&ctx, "derived-alpine", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-derived-alpine", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(2),
            })
            .start()
            .await
            .expect("Failed to start container");

    let mut exec = container
        .exec(ExecCommand::new(vec![
            "cat".to_string(),
            "/etc/alpine-release".to_string(),
        ]))
        .await
        .expect("Exec failed");

    let stdout = exec.stdout_to_vec().await.unwrap_or_default();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(!stdout_str.trim().is_empty());
}

#[tokio::test]
#[serial_test::serial]
async fn test_derive_preserves_original_files() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let dockerfile = r#"
FROM alpine:3.19
RUN mkdir -p /app
RUN echo "original content" > /app/original.txt
COPY uentry /uentry
RUN chmod +x /uentry
ENTRYPOINT ["/uentry"]
CMD ["/bin/sh", "-c", "cat /app/original.txt && sleep 30"]
"#;

    let _image = build_test_image(&ctx, "derived-preserve", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-derived-preserve", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(2),
            })
            .start()
            .await
            .expect("Failed to start container");

    let mut exec = container
        .exec(ExecCommand::new(vec![
            "cat".to_string(),
            "/app/original.txt".to_string(),
        ]))
        .await
        .expect("Exec failed");

    let stdout = exec.stdout_to_vec().await.unwrap_or_default();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(stdout_str.contains("original content"));
}

#[tokio::test]
#[serial_test::serial]
async fn test_derive_with_config() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let config = r#"
runtime:
  signal_forward: true
  env:
    FROM_CONFIG: "config_value"
"#;
    ctx.write_config("config.yaml", config);

    let dockerfile = r#"
FROM alpine:3.19
RUN mkdir -p /etc/uentry
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry
ENTRYPOINT ["/uentry"]
CMD ["/bin/sh", "-c", "echo $FROM_CONFIG && sleep 30"]
"#;

    let _image = build_test_image(&ctx, "derived-config", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-derived-config", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(2),
            })
            .start()
            .await
            .expect("Failed to start container");

    let stdout = container
        .stdout_to_vec()
        .await
        .expect("Failed to read container logs");
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(
        stdout_str.contains("config_value"),
        "Expected config_value in logs, got: {}",
        stdout_str
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_derive_multistage_build() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let dockerfile = r#"
FROM alpine:3.19 AS builder
RUN mkdir -p /app
RUN echo '#!/bin/sh' > /app/start.sh && \
    echo 'echo "App starting..."' >> /app/start.sh && \
    echo 'sleep 30' >> /app/start.sh && \
    chmod +x /app/start.sh

FROM alpine:3.19
COPY --from=builder /app /app
COPY uentry /uentry
RUN chmod +x /uentry
WORKDIR /app
ENTRYPOINT ["/uentry"]
CMD ["/app/start.sh"]
"#;

    let _image = build_test_image(&ctx, "derived-multistage", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-derived-multistage", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(3),
            })
            .start()
            .await
            .expect("Failed to start container");

    let mut exec = container
        .exec(ExecCommand::new(vec![
            "ls".to_string(),
            "-la".to_string(),
            "/app".to_string(),
        ]))
        .await
        .expect("Exec failed");

    let stdout = exec.stdout_to_vec().await.unwrap_or_default();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(stdout_str.contains("start.sh"));
}

#[tokio::test]
#[serial_test::serial]
async fn test_derive_from_busybox() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let dockerfile = r#"
FROM busybox:1.36-musl
COPY uentry /uentry
RUN chmod +x /uentry
ENTRYPOINT ["/uentry"]
CMD ["/bin/sh", "-c", "busybox | head -1 && sleep 30"]
"#;

    let _image = build_test_image(&ctx, "derived-busybox", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-derived-busybox", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(3),
            })
            .start()
            .await
            .expect("Failed to start container");

    let mut exec = container
        .exec(ExecCommand::new(vec![
            "busybox".to_string(),
            "--help".to_string(),
        ]))
        .await
        .expect("Exec failed");

    let stdout = exec.stdout_to_vec().await.unwrap_or_default();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(stdout_str.contains("BusyBox"));
}
