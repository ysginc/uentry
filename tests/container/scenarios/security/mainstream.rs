use super::helpers::{combined_logs, ensure_docker, run_container};
use crate::container::fixtures::*;
use tokio::process::Command as AsyncCommand;

struct MainstreamStack {
    name: &'static str,
    base_image: &'static str,
    command: &'static str,
    success_marker: &'static str,
}

fn vulnerable_config() -> &'static str {
    r#"
# INSECURE EXAMPLE: intentionally vulnerable configuration for testing.
runtime:
  strict: false
  env:
    LD_LIBRARY_PATH: /tmp/injected-lib

security:
  allow_root: true
  no_new_privs: false
  writable_paths:
    - /

audit:
  enabled: false
"#
}

fn strict_config() -> &'static str {
    r#"
runtime:
  strict: true

security:
  allow_root: true
  no_new_privs: true
  writable_paths:
    - /tmp

audit:
  enabled: true
  deep_trace: true
  output: /tmp/uentry-audit.json
  profile_output: /tmp/uentry-audit-profile.yaml
  backend: auto
"#
}

fn dockerfile_for_stack(stack: &MainstreamStack, strict: bool) -> String {
    let entrypoint = if strict {
        "[\"/uentry\", \"--strict\"]"
    } else {
        "[\"/uentry\"]"
    };

    format!(
        r#"
FROM {base_image}
USER root
COPY --chmod=0755 uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN mkdir -p /etc/uentry /tmp
ENTRYPOINT {entrypoint}
CMD {command}
"#,
        base_image = stack.base_image,
        entrypoint = entrypoint,
        command = stack.command,
    )
}

fn should_skip_build_failure(logs: &str) -> bool {
    let lowered = logs.to_ascii_lowercase();

    lowered.contains("error getting credentials")
        || lowered.contains("pull access denied")
        || lowered.contains("unauthorized")
        || lowered.contains("toomanyrequests")
        || lowered.contains("tls handshake timeout")
        || lowered.contains("i/o timeout")
        || lowered.contains("connection reset")
        || lowered.contains("temporary failure")
}

async fn try_build_test_image(
    ctx: &UentryTestContext,
    name: &str,
    dockerfile: &str,
) -> Result<String, String> {
    let dockerfile_path = ctx.temp_dir.path().join("Dockerfile");
    std::fs::write(&dockerfile_path, dockerfile)
        .map_err(|e| format!("failed to write Dockerfile: {}", e))?;

    let binary_name = ctx
        .binary_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "failed to resolve binary name".to_string())?;

    std::fs::copy(&ctx.binary_path, ctx.temp_dir.path().join(binary_name))
        .map_err(|e| format!("failed to copy binary: {}", e))?;

    let image_tag = format!("uentry-test-{}:latest", name);

    let output = AsyncCommand::new("docker")
        .args(["build", "-t", &image_tag, "."])
        .current_dir(ctx.temp_dir.path())
        .output()
        .await
        .map_err(|e| format!("failed to run docker build: {}", e))?;

    if output.status.success() {
        Ok(image_tag)
    } else {
        Err(format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

async fn assert_vulnerable_allows_dangerous_env(stack: &MainstreamStack) {
    let ctx = UentryTestContext::new().await;
    ctx.write_config("config.yaml", vulnerable_config());

    let test_name = format!("mainstream-{}-vuln", stack.name);
    let image =
        match try_build_test_image(&ctx, &test_name, &dockerfile_for_stack(stack, false)).await {
            Ok(image) => image,
            Err(build_logs) => {
                if should_skip_build_failure(&build_logs) {
                    eprintln!(
                        "Skipping {} vulnerable test due to transient build/pull issue: {}",
                        stack.name, build_logs
                    );
                    return;
                }

                panic!(
                    "Docker build failed for {} vulnerable test:\n{}",
                    stack.name, build_logs
                );
            }
        };

    let args = vec![
        "-e".to_string(),
        "LD_LIBRARY_PATH=/tmp/escape-attempt".to_string(),
    ];
    let output = run_container(&image, &args).await;
    let logs = combined_logs(&output);

    let tolerated_echild =
        logs.contains("ECHILD: No child processes") && logs.contains(stack.success_marker);

    assert!(
        output.status.success() || tolerated_echild,
        "Vulnerable-style config should allow startup for {} with dangerous env, logs: {}",
        stack.name,
        logs
    );
}

async fn assert_strict_blocks_dangerous_env(stack: &MainstreamStack) {
    let ctx = UentryTestContext::new().await;
    ctx.write_config("config.yaml", strict_config());

    let test_name = format!("mainstream-{}-strict", stack.name);
    let image =
        match try_build_test_image(&ctx, &test_name, &dockerfile_for_stack(stack, true)).await {
            Ok(image) => image,
            Err(build_logs) => {
                if should_skip_build_failure(&build_logs) {
                    eprintln!(
                        "Skipping {} strict test due to transient build/pull issue: {}",
                        stack.name, build_logs
                    );
                    return;
                }

                panic!(
                    "Docker build failed for {} strict test:\n{}",
                    stack.name, build_logs
                );
            }
        };

    let args = vec![
        "-e".to_string(),
        "LD_LIBRARY_PATH=/tmp/escape-attempt".to_string(),
    ];
    let output = run_container(&image, &args).await;
    let logs = combined_logs(&output);

    assert!(
        !output.status.success(),
        "Strict-style config should reject dangerous env for {}, logs: {}",
        stack.name,
        logs
    );
    assert!(
        logs.contains("dangerous environment variables") || logs.contains("LD_LIBRARY_PATH"),
        "Expected dangerous env rejection in logs for {}, got: {}",
        stack.name,
        logs
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_nginx_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "nginx",
        base_image: "nginx:1.27-alpine",
        command: "[\"nginx\", \"-v\"]",
        success_marker: "nginx version:",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_node_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "node",
        base_image: "node:20-alpine",
        command: "[\"node\", \"--version\"]",
        success_marker: "v20",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_python_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "python",
        base_image: "python:3.12-alpine",
        command: "[\"python\", \"--version\"]",
        success_marker: "Python 3",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_redis_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "redis",
        base_image: "redis:7.2-alpine",
        command: "[\"redis-server\", \"--version\"]",
        success_marker: "Redis server v=",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_java_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "java",
        base_image: "eclipse-temurin:21-jre",
        command: "[\"java\", \"-version\"]",
        success_marker: "openjdk version",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_postgres_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "postgres",
        base_image: "postgres:16-alpine",
        command: "[\"postgres\", \"--version\"]",
        success_marker: "postgres (PostgreSQL)",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_rabbitmq_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "rabbitmq",
        base_image: "rabbitmq:3.13-alpine",
        command: "[\"/bin/sh\", \"-c\", \"echo RabbitMQ\"]",
        success_marker: "RabbitMQ",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_elasticsearch_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "elasticsearch",
        base_image: "docker.elastic.co/elasticsearch/elasticsearch:8.15.0",
        command: "[\"elasticsearch\", \"--version\"]",
        success_marker: "Version:",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_prometheus_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "prometheus",
        base_image: "prom/prometheus:v2.55.1",
        command: "[\"prometheus\", \"--version\"]",
        success_marker: "prometheus, version",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_grafana_mainstream_env_controls() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let stack = MainstreamStack {
        name: "grafana",
        base_image: "grafana/grafana:11.2.0",
        command: "[\"grafana\", \"server\", \"-v\"]",
        success_marker: "Version",
    };

    assert_vulnerable_allows_dangerous_env(&stack).await;
    assert_strict_blocks_dangerous_env(&stack).await;
}
