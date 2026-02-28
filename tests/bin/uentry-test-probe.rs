use std::env;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("status") => report_status(),
        Some("print-env") => print_env(args.collect()),
        Some("proc-sys-write") => probe_proc_sys_write(),
        Some("probe-sockets") => probe_privileged_sockets(),
        Some("mount-attempt") => probe_mount_attempt(),
        _ => {
            eprintln!(
                "Usage: uentry-test-probe <status|print-env|proc-sys-write|probe-sockets|mount-attempt> [args]"
            );
            ExitCode::from(2)
        }
    }
}

fn report_status() -> ExitCode {
    let status = match fs::read_to_string("/proc/self/status") {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("failed to read /proc/self/status: {}", error);
            return ExitCode::from(1);
        }
    };

    if let Some(no_new_privs) = status
        .lines()
        .find_map(|line| line.strip_prefix("NoNewPrivs:"))
        .map(str::trim)
    {
        println!("NoNewPrivs={}", no_new_privs);
    }

    if let Some(uid_line) = status.lines().find(|line| line.starts_with("Uid:")) {
        println!("{}", uid_line.replace('\t', " "));
    }

    ExitCode::SUCCESS
}

fn print_env(keys: Vec<String>) -> ExitCode {
    for key in keys {
        let value = env::var(&key).unwrap_or_else(|_| "<unset>".to_string());
        println!("{}={}", key, value);
    }

    ExitCode::SUCCESS
}

fn probe_proc_sys_write() -> ExitCode {
    match fs::write("/proc/sys/kernel/hostname", "uentry-test\n") {
        Ok(_) => println!("PROC_SYS_ALLOWED"),
        Err(error) => {
            println!("PROC_SYS_BLOCKED");
            eprintln!("proc sys write blocked: {}", error);
        }
    }

    ExitCode::SUCCESS
}

fn probe_privileged_sockets() -> ExitCode {
    let socket_paths = [
        "/var/run/docker.sock",
        "/run/docker.sock",
        "/proc/1/root/var/run/docker.sock",
    ];

    for socket_path in socket_paths {
        if let Ok(metadata) = fs::metadata(socket_path) {
            if metadata.file_type().is_socket() {
                println!("IPC_SOCKET_PRESENT");
                println!("IPC_SOCKET_PATH={}", socket_path);
                return ExitCode::SUCCESS;
            }
        }
    }

    println!("IPC_SOCKET_BLOCKED");
    ExitCode::SUCCESS
}

fn probe_mount_attempt() -> ExitCode {
    if let Err(error) = fs::create_dir_all("/tmp/escape-mnt") {
        eprintln!("failed to prepare mount path: {}", error);
        println!("MOUNT_BLOCKED");
        return ExitCode::SUCCESS;
    }

    let output = std::process::Command::new("mount")
        .args(["-t", "tmpfs", "tmpfs", "/tmp/escape-mnt"])
        .output();

    match output {
        Ok(command_output) if command_output.status.success() => println!("MOUNT_ALLOWED"),
        Ok(command_output) => {
            println!("MOUNT_BLOCKED");
            eprintln!(
                "mount blocked: {}",
                String::from_utf8_lossy(&command_output.stderr)
            );
        }
        Err(error) => {
            println!("MOUNT_BLOCKED");
            eprintln!("mount command failed: {}", error);
        }
    }

    ExitCode::SUCCESS
}
