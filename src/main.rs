use std::process::ExitCode;

use uentry::{
    audit::AuditSession,
    cli,
    config::{self, metadata::expand_config_env, profile::apply_profile},
    exec,
    health::readiness::ReadinessChecker,
    lifecycle::coordinator::LifecycleCoordinator,
    logging,
    pid1::SignalHandler,
};

fn main() -> ExitCode {
    logging::init();

    let args = cli::parse();

    if let Err(e) = args.validate() {
        eprintln!("Error: {}", e);
        return ExitCode::from(1);
    }

    if args.diagnose {
        run_diagnostics();
        return ExitCode::SUCCESS;
    }

    let mut config = match config::resolver::resolve(args.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            return ExitCode::from(1);
        }
    };

    merge_cli_into_config(&args, &mut config);

    if let Some(profile_name) = config.app.profile.clone() {
        if let Err(e) = apply_profile(&mut config, &profile_name) {
            eprintln!("Failed to load profile '{}': {}", profile_name, e);
            return ExitCode::from(1);
        }
    }

    expand_config_env(&mut config);

    let mut audit = AuditSession::from_config(&config.audit);

    if let Some(session) = audit.as_mut() {
        session.record_command(&args.command);
        match uentry::security::preflight::PreflightCheck::new().run() {
            Ok(report) => {
                session.record_preflight_report(&report);
                session.record_lifecycle_outcome("preflight_probe", true, None);
            }
            Err(e) => {
                session.record_lifecycle_outcome("preflight_probe", false, Some(&e.to_string()));
            }
        }
    }

    let mut lifecycle = LifecycleCoordinator::new(config.clone());

    if let Err(e) = lifecycle.run() {
        if let Some(session) = audit.as_mut() {
            session.record_lifecycle_outcome("lifecycle_run", false, Some(&e.to_string()));
        }
        eprintln!("Lifecycle error: {}", e);
        return exit_with_audit(1, &mut audit);
    } else if let Some(session) = audit.as_mut() {
        session.record_lifecycle_outcome("lifecycle_run", true, None);
    }

    if let Err(e) = lifecycle.run_pre_start() {
        if let Some(session) = audit.as_mut() {
            session.record_lifecycle_outcome("pre_start", false, Some(&e.to_string()));
        }
        eprintln!("Pre-start error: {}", e);
        return exit_with_audit(1, &mut audit);
    } else if let Some(session) = audit.as_mut() {
        session.record_lifecycle_outcome("pre_start", true, None);
    }

    if let Some(ref readiness_config) = config.app.readiness {
        let mut checker = ReadinessChecker::new(readiness_config.clone());
        match checker.wait_for_ready() {
            Ok(uentry::health::readiness::ProbeResult::Ready) => {
                if let Some(session) = audit.as_mut() {
                    session.record_lifecycle_outcome("readiness", true, None);
                }
            }
            Ok(_) => {
                if let Some(session) = audit.as_mut() {
                    session.record_lifecycle_outcome(
                        "readiness",
                        false,
                        Some("probe did not become ready"),
                    );
                }
                eprintln!("Readiness check failed");
                return exit_with_audit(1, &mut audit);
            }
            Err(e) => {
                if let Some(session) = audit.as_mut() {
                    session.record_lifecycle_outcome("readiness", false, Some(&e.to_string()));
                }
                eprintln!("Readiness check error: {}", e);
                return exit_with_audit(1, &mut audit);
            }
        }
    }

    let mut signal_handler = SignalHandler::new();
    if uentry::pid1::signal::is_pid1() {
        if let Err(e) = signal_handler.install_handlers() {
            eprintln!("Failed to install signal handlers: {}", e);
        }
    }

    let exit_code = match exec::execute(&args.command, &config, &mut signal_handler, audit.as_mut())
    {
        Ok(code) => {
            if let Some(session) = audit.as_mut() {
                session.record_exec_outcome(Some(code), None);
            }
            code
        }
        Err(e) => {
            if let Some(session) = audit.as_mut() {
                session.record_exec_outcome(None, Some(&e.to_string()));
            }
            eprintln!("Execution error: {}", e);
            1
        }
    };

    match lifecycle.run_post_stop() {
        Ok(()) => {
            if let Some(session) = audit.as_mut() {
                session.record_lifecycle_outcome("post_stop", true, None);
            }
        }
        Err(e) => {
            if let Some(session) = audit.as_mut() {
                session.record_lifecycle_outcome("post_stop", false, Some(&e.to_string()));
            }
        }
    }

    finalize_audit(&mut audit);

    ExitCode::from(exit_code as u8)
}

fn merge_cli_into_config(args: &cli::Cli, config: &mut config::Config) {
    if args.strict {
        config.runtime.strict = true;
    }
    if args.profile.is_some() {
        config.app.profile = args.profile.clone();
    }

    if args.audit {
        config.audit.enabled = true;
    }
    if args.audit_deep {
        config.audit.enabled = true;
        config.audit.deep_trace = true;
    }
    if let Some(path) = &args.audit_output {
        config.audit.enabled = true;
        config.audit.output = Some(path.clone());
    }
    if let Some(path) = &args.audit_profile_output {
        config.audit.enabled = true;
        config.audit.profile_output = Some(path.clone());
    }
}

fn finalize_audit(audit: &mut Option<AuditSession>) {
    if let Some(session) = audit.as_mut() {
        if let Err(e) = session.finalize() {
            eprintln!("Audit finalize error: {}", e);
        }
    }
}

fn exit_with_audit(code: u8, audit: &mut Option<AuditSession>) -> ExitCode {
    finalize_audit(audit);
    ExitCode::from(code)
}

fn run_diagnostics() {
    println!("uentry diagnostics:");
    println!("  PID: {}", std::process::id());
    println!("  PID 1: {}", uentry::pid1::signal::is_pid1());
    println!("  Working directory: {:?}", std::env::current_dir());
    println!("  Config path: {:?}", config::file::default_config_path());

    println!("\nSecurity diagnostics:");
    let preflight = uentry::security::preflight::PreflightCheck::new();
    match preflight.run() {
        Ok(report) => {
            println!("  Running as root: {}", report.is_root);
            println!("  UID: {}, GID: {}", report.uid, report.gid);
            println!("  Root filesystem read-only: {}", report.rootfs_readonly);

            let caps = if report.capabilities.is_empty() {
                "none".to_string()
            } else {
                report.capabilities.join(", ")
            };
            println!("  Capabilities: {}", caps);

            let mounts = if report.forbidden_mounts.is_empty() {
                "none".to_string()
            } else {
                report.forbidden_mounts.join(", ")
            };
            println!("  Forbidden mounts: {}", mounts);

            let env_vars = if report.dangerous_env_vars.is_empty() {
                "none".to_string()
            } else {
                report.dangerous_env_vars.join(", ")
            };
            println!("  Dangerous env vars: {}", env_vars);
        }
        Err(e) => {
            println!("  Error running security checks: {}", e);
        }
    }

    println!("\nProfile discovery:");
    let manager = uentry::config::profile::ProfileManager::new();
    let profiles = manager.discover();
    println!("  Available profiles: {}", profiles.join(", "));

    println!("\nKubernetes metadata:");
    let k8s_meta = uentry::config::metadata::read_k8s_downward_api();
    if k8s_meta.is_empty() {
        println!("  No Kubernetes metadata found");
    } else {
        for (key, value) in &k8s_meta {
            println!("  {}: {}", key, value);
        }
    }
}
