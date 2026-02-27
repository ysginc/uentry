use crate::container::fixtures::*;
use std::os::unix::process::ExitStatusExt;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command as AsyncCommand;
use tokio::time::{timeout, Duration};

const RUN_TIMEOUT_SECS: u64 = 45;

pub static DOCKERGUARD: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();

pub async fn ensure_docker() -> bool {
    *DOCKERGUARD
        .get_or_init(|| async { check_docker_available().await })
        .await
}

pub async fn run_container(image: &str, args: &[String]) -> std::process::Output {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let container_name = format!("uentry-test-{}-{}", std::process::id(), nonce);

    let mut cmd = AsyncCommand::new("docker");
    cmd.args(["run", "--rm", "--name", &container_name]);
    cmd.args(args).arg(image);

    match timeout(Duration::from_secs(RUN_TIMEOUT_SECS), cmd.output()).await {
        Ok(result) => result.expect("Failed to run docker container"),
        Err(_) => {
            let cleanup = AsyncCommand::new("docker")
                .args(["rm", "-f", &container_name])
                .output()
                .await;

            let cleanup_details = match cleanup {
                Ok(output) => format!(
                    "cleanup stdout={} cleanup stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
                Err(e) => format!("cleanup error={}", e),
            };

            let stderr = format!(
                "Container run timed out after {}s for image {} (name {}). {}",
                RUN_TIMEOUT_SECS, image, container_name, cleanup_details
            );

            std::process::Output {
                status: std::process::ExitStatus::from_raw(124 << 8),
                stdout: Vec::new(),
                stderr: stderr.into_bytes(),
            }
        }
    }
}

pub fn combined_logs(output: &std::process::Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
