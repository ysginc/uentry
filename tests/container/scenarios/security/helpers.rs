use crate::container::fixtures::*;
use tokio::process::Command as AsyncCommand;

pub static DOCKERGUARD: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();

pub async fn ensure_docker() -> bool {
    *DOCKERGUARD
        .get_or_init(|| async { check_docker_available().await })
        .await
}

pub async fn run_container(image: &str, args: &[String]) -> std::process::Output {
    AsyncCommand::new("docker")
        .args(["run", "--rm"])
        .args(args)
        .arg(image)
        .output()
        .await
        .expect("Failed to run docker container")
}

pub fn combined_logs(output: &std::process::Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
