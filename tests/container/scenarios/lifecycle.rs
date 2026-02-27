//! Lifecycle and secrets tests.

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
async fn test_pre_start_hook() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let config = r#"
runtime:
  signal_forward: true

lifecycle:
  pre_start:
    command: /bin/sh
    args:
      - "-c"
      - "echo 'pre-start-hook' > /tmp/hook-ran.txt"
    timeout_secs: 10
"#;
    ctx.write_config("config.yaml", config);

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry /tmp
ENTRYPOINT ["/uentry"]
CMD ["/bin/sh", "-c", "cat /tmp/hook-ran.txt 2>/dev/null || echo 'no hook'; sleep 30"]
"#;

    let _image = build_test_image(&ctx, "lifecycle-prestart", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-lifecycle-prestart", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(3),
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
        stdout_str.contains("pre-start-hook"),
        "Expected pre-start-hook in logs, got: {}",
        stdout_str
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_secrets_file_to_env() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let config = r#"
runtime:
  signal_forward: true

secrets:
  file_to_env:
    - file: /run/secrets/test_secret
      env_var: MY_SECRET
"#;
    ctx.write_config("config.yaml", config);

    let dockerfile = r#"
FROM alpine:3.19
RUN mkdir -p /run/secrets /etc/uentry
RUN echo "super_secret_value" > /run/secrets/test_secret
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry
ENTRYPOINT ["/uentry"]
CMD ["/bin/sh", "-c", "echo $MY_SECRET && sleep 30"]
"#;

    let _image = build_test_image(&ctx, "secrets-file-env", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-secrets-file-env", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(3),
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
        stdout_str.contains("super_secret_value"),
        "Expected super_secret_value in logs, got: {}",
        stdout_str
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_secrets_env_to_file() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let config = r#"
runtime:
  signal_forward: true

secrets:
  env_to_file:
    - env_var: CONFIG_SECRET
      file: /app/config/secret.txt
      mode: "0600"
"#;
    ctx.write_config("config.yaml", config);

    let dockerfile = r#"
FROM alpine:3.19
RUN mkdir -p /app/config /etc/uentry /tmp
ENV CONFIG_SECRET="config_value_here"
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry
ENTRYPOINT ["/uentry"]
CMD ["/bin/sh", "-c", "cp /app/config/secret.txt /tmp/result.txt 2>/dev/null || echo 'no secret' > /tmp/result.txt; sleep 30"]
"#;

    let _image = build_test_image(&ctx, "secrets-env-file", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-secrets-env-file", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(3),
            })
            .start()
            .await
            .expect("Failed to start container");

    use testcontainers::core::ExecCommand;
    let mut exec = container
        .exec(ExecCommand::new(vec![
            "cat".to_string(),
            "/tmp/result.txt".to_string(),
        ]))
        .await
        .expect("Exec failed");

    let stdout = exec.stdout_to_vec().await.unwrap_or_default();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(
        stdout_str.contains("config_value_here"),
        "Expected config_value_here in /tmp/result.txt, got: {}",
        stdout_str
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_ensure_directories() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let config = r#"
runtime:
  signal_forward: true
  ensure_dirs:
    - path: /var/log/app
      mode: "0755"
    - path: /var/run/app
      mode: "0755"
"#;
    ctx.write_config("config.yaml", config);

    let dockerfile = r#"
FROM alpine:3.19
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry && mkdir -p /etc/uentry
ENTRYPOINT ["/uentry"]
CMD ["/bin/sh", "-c", "ls -la /var/log/app /var/run/app 2>&1; sleep 30"]
"#;

    let _image = build_test_image(&ctx, "ensure-dirs", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-ensure-dirs", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(3),
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
        stdout_str.contains("/var/log/app"),
        "Expected /var/log/app in logs, got: {}",
        stdout_str
    );
    assert!(
        stdout_str.contains("/var/run/app"),
        "Expected /var/run/app in logs, got: {}",
        stdout_str
    );
}
