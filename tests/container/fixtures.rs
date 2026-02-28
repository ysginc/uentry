//! Test fixtures and utilities for container tests.

use std::path::{Path, PathBuf};
use tempfile::TempDir;
pub use testcontainers::core::WaitFor;
pub use testcontainers::runners::AsyncRunner;
pub use testcontainers::{ContainerAsync, GenericImage};
use tokio::process::Command as AsyncCommand;

pub const UENTRY_BINARY: &str = "target/x86_64-unknown-linux-musl/release/uentry";
pub const UENTRY_BINARY_LOCAL: &str = "target/release/uentry";
pub const UENTRY_TEST_PROBE_BINARY: &str =
    "target/x86_64-unknown-linux-musl/release/uentry-test-probe";
pub const UENTRY_TEST_PROBE_BINARY_LOCAL: &str = "target/release/uentry-test-probe";

pub struct UentryTestContext {
    pub binary_path: PathBuf,
    pub test_probe_binary_path: PathBuf,
    pub temp_dir: TempDir,
}

fn resolve_binary_path(
    workspace_root: &Path,
    primary: &str,
    fallback: &str,
    binary_name: &str,
    build_hint: &str,
) -> PathBuf {
    let primary_path = workspace_root.join(primary);
    if primary_path.exists() {
        return primary_path;
    }

    let fallback_path = workspace_root.join(fallback);
    if fallback_path.exists() {
        return fallback_path;
    }

    panic!(
        "{} binary not found at {:?} or {:?}. Run: {}",
        binary_name, primary_path, fallback_path, build_hint
    );
}

fn copy_named_binary(source_path: &Path, destination_dir: &Path, destination_name: &str) {
    std::fs::copy(source_path, destination_dir.join(destination_name)).unwrap_or_else(|error| {
        panic!(
            "Failed to copy binary {:?} to {:?}: {}",
            source_path, destination_name, error
        )
    });
}

pub struct ForcedContainerCleanup {
    container_id: String,
}

impl ForcedContainerCleanup {
    pub fn new<I: testcontainers::Image>(container: &ContainerAsync<I>) -> Self {
        Self {
            container_id: container.id().to_string(),
        }
    }
}

impl Drop for ForcedContainerCleanup {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.container_id])
            .output();
    }
}

impl UentryTestContext {
    pub async fn new() -> Self {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        let binary_path = resolve_binary_path(
            &workspace_root,
            UENTRY_BINARY,
            UENTRY_BINARY_LOCAL,
            "uentry",
            "cargo build --release --target x86_64-unknown-linux-musl --bin uentry",
        );

        let test_probe_binary_path = resolve_binary_path(
            &workspace_root,
            UENTRY_TEST_PROBE_BINARY,
            UENTRY_TEST_PROBE_BINARY_LOCAL,
            "uentry-test-probe",
            "cargo build --release --target x86_64-unknown-linux-musl --bin uentry-test-probe",
        );

        Self {
            binary_path,
            test_probe_binary_path,
            temp_dir,
        }
    }

    pub fn binary_size(&self) -> u64 {
        std::fs::metadata(&self.binary_path)
            .expect("Failed to read binary metadata")
            .len()
    }

    pub fn write_config(&self, name: &str, content: &str) -> PathBuf {
        let path = self.temp_dir.path().join(name);
        std::fs::write(&path, content).expect("Failed to write config");
        path
    }
}

pub async fn check_docker_available() -> bool {
    AsyncCommand::new("docker")
        .arg("info")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub async fn build_test_image(ctx: &UentryTestContext, name: &str, dockerfile: &str) -> String {
    build_test_image_with_extra_binaries(ctx, name, dockerfile, &[]).await
}

pub async fn build_test_image_with_extra_binaries(
    ctx: &UentryTestContext,
    name: &str,
    dockerfile: &str,
    extra_binaries: &[(&str, &Path)],
) -> String {
    let dockerfile_path = ctx.temp_dir.path().join("Dockerfile");
    std::fs::write(&dockerfile_path, dockerfile).expect("Failed to write Dockerfile");

    copy_named_binary(&ctx.binary_path, ctx.temp_dir.path(), "uentry");
    copy_named_binary(
        &ctx.test_probe_binary_path,
        ctx.temp_dir.path(),
        "uentry-test-probe",
    );

    for (destination_name, source_path) in extra_binaries {
        copy_named_binary(source_path, ctx.temp_dir.path(), destination_name);
    }

    let image_tag = format!("uentry-test-{}:latest", name);

    let output = AsyncCommand::new("docker")
        .args(["build", "-t", &image_tag, "."])
        .current_dir(ctx.temp_dir.path())
        .output()
        .await
        .expect("Failed to run docker build");

    if !output.status.success() {
        panic!(
            "Docker build failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    image_tag
}
