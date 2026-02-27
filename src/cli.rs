//! CLI argument parsing.
//!
//! This module defines the command-line interface using clap.

use clap::Parser;

/// Command-line arguments for uentry.
#[derive(Debug, Parser)]
#[command(name = "uentry")]
#[command(about = "A minimal init system for containers", long_about = None)]
#[command(version)]
pub struct Cli {
    #[arg(long, env = "UENTRY_STRICT", help = "Enable strict mode")]
    pub strict: bool,

    #[arg(long, env = "UENTRY_PROFILE", help = "Configuration profile to use")]
    pub profile: Option<String>,

    #[arg(long, env = "UENTRY_CONFIG", help = "Path to config file")]
    pub config: Option<std::path::PathBuf>,

    #[arg(long, help = "Run diagnostics and exit")]
    pub diagnose: bool,

    #[arg(num_args = 1.., trailing_var_arg = true)]
    pub command: Vec<String>,
}

/// Parse command-line arguments.
pub fn parse() -> Cli {
    Cli::parse()
}

impl Cli {
    /// Validate the CLI arguments.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - No command is provided and `--diagnose` is not set
    pub fn validate(&self) -> Result<(), String> {
        if !self.diagnose && self.command.is_empty() {
            return Err("COMMAND is required unless --diagnose is specified".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_validate_with_command() {
        let cli = Cli {
            strict: false,
            profile: None,
            config: None,
            diagnose: false,
            command: vec!["echo".to_string(), "hello".to_string()],
        };
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_cli_validate_with_diagnose() {
        let cli = Cli {
            strict: false,
            profile: None,
            config: None,
            diagnose: true,
            command: vec![],
        };
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_cli_validate_no_command_no_diagnose() {
        let cli = Cli {
            strict: false,
            profile: None,
            config: None,
            diagnose: false,
            command: vec![],
        };
        assert!(cli.validate().is_err());
        assert!(cli.validate().unwrap_err().contains("COMMAND is required"));
    }

    #[test]
    fn test_cli_debug() {
        let cli = Cli {
            strict: true,
            profile: Some("prod".to_string()),
            config: Some(std::path::PathBuf::from("/etc/config.yaml")),
            diagnose: false,
            command: vec!["app".to_string()],
        };
        let debug_str = format!("{:?}", cli);
        assert!(debug_str.contains("strict: true"));
        assert!(debug_str.contains("prod"));
    }

    #[test]
    fn test_cli_parse_from_args() {
        let cli = Cli::try_parse_from(["uentry", "--strict", "echo", "hello"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.strict);
        assert_eq!(cli.command, vec!["echo", "hello"]);
    }

    #[test]
    fn test_cli_parse_with_config() {
        let cli = Cli::try_parse_from(["uentry", "--config", "/etc/uentry/config.yaml", "app"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(
            cli.config,
            Some(std::path::PathBuf::from("/etc/uentry/config.yaml"))
        );
    }

    #[test]
    fn test_cli_parse_with_profile() {
        let cli = Cli::try_parse_from(["uentry", "--profile", "production", "app"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.profile, Some("production".to_string()));
    }

    #[test]
    fn test_cli_parse_diagnose() {
        let cli = Cli::try_parse_from(["uentry", "--diagnose"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.diagnose);
    }
}
