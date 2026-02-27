//! Profile system tests - validates built-in profiles.

use crate::container::fixtures::*;
use std::time::Duration;
use testcontainers::core::ExecCommand;

static DOCKERGUARD: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();

async fn ensure_docker() -> bool {
    *DOCKERGUARD
        .get_or_init(|| async { check_docker_available().await })
        .await
}

fn profile_dockerfile(profile: &str) -> String {
    format!(
        r#"
FROM alpine:3.19
COPY uentry /uentry
RUN chmod +x /uentry
ENTRYPOINT ["/uentry", "--profile", "{}"]
CMD ["/bin/sh", "-c", "echo 'profile loaded' > /tmp/loaded.txt; sleep 30"]
"#,
        profile
    )
}

async fn test_profile_loads(profile_name: &str) {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    // Skip web profile as it has a readiness probe that requires port 8080
    if profile_name == "web" {
        eprintln!("Skipping web profile (requires HTTP server on port 8080)");
        return;
    }

    let ctx = UentryTestContext::new().await;
    let test_name = format!("profile-{}", profile_name);
    let _image = build_test_image(&ctx, &test_name, &profile_dockerfile(profile_name)).await;

    let image_name = format!("uentry-test-{}", test_name);
    let container: ContainerAsync<GenericImage> = GenericImage::new(image_name.as_str(), "latest")
        .with_wait_for(WaitFor::Duration {
            length: Duration::from_secs(3),
        })
        .start()
        .await
        .expect(&format!(
            "Failed to start container for profile {}",
            profile_name
        ));
    let _cleanup = ForcedContainerCleanup::new(&container);

    let mut exec = container
        .exec(ExecCommand::new(vec![
            "cat".to_string(),
            "/tmp/loaded.txt".to_string(),
        ]))
        .await
        .expect("Exec failed");

    let stdout = exec.stdout_to_vec().await.unwrap_or_default();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(
        stdout_str.contains("profile loaded"),
        "Profile {} failed to load, got: {}",
        profile_name,
        stdout_str
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_baseline_profile() {
    test_profile_loads("baseline").await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_web_profile() {
    test_profile_loads("web").await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_worker_profile() {
    test_profile_loads("worker").await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_k8s_profile() {
    test_profile_loads("k8s").await;
}

#[tokio::test]
#[serial_test::serial]
async fn test_profile_with_custom_config() {
    if !ensure_docker().await {
        eprintln!("Skipping: Docker not available");
        return;
    }

    let ctx = UentryTestContext::new().await;

    let config = r#"
runtime:
  signal_forward: true
  env:
    CUSTOM_VAR: "custom_value"
"#;
    ctx.write_config("config.yaml", config);

    let dockerfile = r#"
FROM alpine:3.19
RUN mkdir -p /etc/uentry /tmp
COPY uentry /uentry
COPY config.yaml /etc/uentry/config.yaml
RUN chmod +x /uentry
ENTRYPOINT ["/uentry", "--profile", "baseline"]
CMD ["/bin/sh", "-c", "echo CUSTOM_VAR=$CUSTOM_VAR > /tmp/env.txt; sleep 30"]
"#;

    let _image = build_test_image(&ctx, "profile-custom", dockerfile).await;

    let container: ContainerAsync<GenericImage> =
        GenericImage::new("uentry-test-profile-custom", "latest")
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(3),
            })
            .start()
            .await
            .expect("Failed to start container");
    let _cleanup = ForcedContainerCleanup::new(&container);

    let mut exec = container
        .exec(ExecCommand::new(vec![
            "cat".to_string(),
            "/tmp/env.txt".to_string(),
        ]))
        .await
        .expect("Exec failed");

    let stdout = exec.stdout_to_vec().await.unwrap_or_default();
    let stdout_str = String::from_utf8_lossy(&stdout);
    assert!(
        stdout_str.contains("CUSTOM_VAR=custom_value"),
        "Expected CUSTOM_VAR=custom_value in /tmp/env.txt, got: {}",
        stdout_str
    );
}
