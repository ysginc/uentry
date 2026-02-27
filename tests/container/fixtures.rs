//! Test fixtures and utilities for container tests.

use std::path::PathBuf;
use tempfile::TempDir;
pub use testcontainers::core::WaitFor;
pub use testcontainers::runners::AsyncRunner;
pub use testcontainers::{ContainerAsync, GenericImage};
use tokio::process::Command as AsyncCommand;

pub const UENTRY_BINARY: &str = "target/x86_64-unknown-linux-musl/release/uentry";
pub const UENTRY_BINARY_LOCAL: &str = "target/release/uentry";

pub struct UentryTestContext {
    pub binary_path: PathBuf,
    pub temp_dir: TempDir,
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

        let binary_path = if PathBuf::from(UENTRY_BINARY).exists() {
            workspace_root.join(UENTRY_BINARY)
        } else {
            workspace_root.join(UENTRY_BINARY_LOCAL)
        };

        if !binary_path.exists() {
            panic!(
                "uentry binary not found at {:?}. Run: cargo build --release --target x86_64-unknown-linux-musl",
                binary_path
            );
        }

        Self {
            binary_path,
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
    let dockerfile_path = ctx.temp_dir.path().join("Dockerfile");
    std::fs::write(&dockerfile_path, dockerfile).expect("Failed to write Dockerfile");

    let binary_name = ctx.binary_path.file_name().unwrap().to_str().unwrap();
    std::fs::copy(&ctx.binary_path, ctx.temp_dir.path().join(binary_name))
        .expect("Failed to copy binary");

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
