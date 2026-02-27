//! Runtime metadata and variable expansion.
//!
//! This module provides:
//! - K8s downward API file reading (TASK-050)
//! - Environment variable expansion in config (TASK-051)

use std::collections::HashMap;
use std::path::Path;

/// Read Kubernetes downward API files and return as a map.
///
/// Common downward API files:
/// - /var/run/secrets/kubernetes.io/serviceaccount/namespace
/// - /var/run/secrets/kubernetes.io/serviceaccount/token
/// - /etc/podinfo/labels
/// - /etc/podinfo/annotations
pub fn read_k8s_downward_api() -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    let namespace_path = Path::new("/var/run/secrets/kubernetes.io/serviceaccount/namespace");
    if namespace_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(namespace_path) {
            metadata.insert(
                "KUBERNETES_NAMESPACE".to_string(),
                contents.trim().to_string(),
            );
        }
    }

    let pod_name_paths = ["/etc/podinfo/name", "/var/run/podinfo/name"];
    for path in &pod_name_paths {
        let path = Path::new(path);
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(path) {
                metadata.insert(
                    "KUBERNETES_POD_NAME".to_string(),
                    contents.trim().to_string(),
                );
                break;
            }
        }
    }

    let pod_uid_paths = ["/etc/podinfo/uid", "/var/run/podinfo/uid"];
    for path in &pod_uid_paths {
        let path = Path::new(path);
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(path) {
                metadata.insert(
                    "KUBERNETES_POD_UID".to_string(),
                    contents.trim().to_string(),
                );
                break;
            }
        }
    }

    let labels_path = Path::new("/etc/podinfo/labels");
    if labels_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(labels_path) {
            for line in contents.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    let safe_key = key.replace(['.', '-'], "_").to_uppercase();
                    metadata.insert(format!("KUBERNETES_LABEL_{}", safe_key), value.to_string());
                }
            }
        }
    }

    metadata
}

/// Expand environment variables in a string.
///
/// Supports:
/// - `$VAR` - Simple variable expansion
/// - `${VAR}` - Brace expansion
/// - `${VAR:-default}` - Default value if not set
/// - `${VAR:+alternate}` - Alternate value if set
pub fn expand_env_vars(input: &str, env: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            if let Some(&next) = chars.peek() {
                if next == '{' {
                    chars.next(); // consume '{'
                    let expanded = expand_braced_var(&mut chars, env);
                    result.push_str(&expanded);
                } else if next.is_alphanumeric() || next == '_' {
                    let expanded = expand_simple_var(&mut chars, env);
                    result.push_str(&expanded);
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn expand_simple_var(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    env: &HashMap<String, String>,
) -> String {
    let mut var_name = String::new();

    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' {
            var_name.push(c);
            chars.next();
        } else {
            break;
        }
    }

    env.get(&var_name).cloned().unwrap_or_default()
}

fn expand_braced_var(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    env: &HashMap<String, String>,
) -> String {
    let mut content = String::new();
    let mut depth = 1;

    for c in chars.by_ref() {
        if c == '}' {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        if c == '{' {
            depth += 1;
        }
        content.push(c);
    }

    if content.is_empty() {
        return String::new();
    }

    // Handle special operators: :- :+ := :?
    if let Some((var_name, default)) = content.split_once(":-") {
        return env
            .get(var_name)
            .cloned()
            .unwrap_or_else(|| expand_env_vars(default, env));
    }

    if let Some((var_name, alternate)) = content.split_once(":+") {
        if env.contains_key(var_name) {
            return expand_env_vars(alternate, env);
        } else {
            return String::new();
        }
    }

    if let Some((var_name, default)) = content.split_once(":=") {
        let value = env
            .get(var_name)
            .cloned()
            .unwrap_or_else(|| expand_env_vars(default, env));
        return value;
    }

    if let Some((var_name, message)) = content.split_once(":?") {
        if env.contains_key(var_name) {
            return env.get(var_name).cloned().unwrap_or_default();
        } else {
            return format!("ERROR: {} is unset - {}", var_name, message);
        }
    }

    // Simple variable reference
    env.get(&content).cloned().unwrap_or_default()
}

/// Expand environment variables in all string values in the config.
pub fn expand_config_env(config: &mut crate::config::schema::Config) {
    let mut env: HashMap<String, String> = std::env::vars().collect();

    let k8s_meta = read_k8s_downward_api();
    env.extend(k8s_meta);

    for value in config.runtime.env.values_mut() {
        *value = expand_env_vars(value, &env);
    }

    if let Some(ref mut cwd) = config.runtime.cwd {
        let expanded = expand_env_vars(&cwd.to_string_lossy(), &env);
        *cwd = std::path::PathBuf::from(expanded);
    }

    for dir in &mut config.runtime.ensure_dirs {
        let expanded = expand_env_vars(&dir.path.to_string_lossy(), &env);
        dir.path = std::path::PathBuf::from(expanded);
    }

    for fte in &mut config.secrets.file_to_env {
        let expanded = expand_env_vars(&fte.file.to_string_lossy(), &env);
        fte.file = std::path::PathBuf::from(expanded);
    }

    for etf in &mut config.secrets.env_to_file {
        let expanded = expand_env_vars(&etf.file.to_string_lossy(), &env);
        etf.file = std::path::PathBuf::from(expanded);
    }

    for path in &mut config.security.writable_paths {
        let expanded = expand_env_vars(&path.to_string_lossy(), &env);
        *path = std::path::PathBuf::from(expanded);
    }

    if let Some(ref mut hook) = config.lifecycle.pre_start {
        hook.command = expand_env_vars(&hook.command, &env);
        for arg in &mut hook.args {
            *arg = expand_env_vars(arg, &env);
        }
    }

    if let Some(ref mut hook) = config.lifecycle.post_stop {
        hook.command = expand_env_vars(&hook.command, &env);
        for arg in &mut hook.args {
            *arg = expand_env_vars(arg, &env);
        }
    }

    if let Some(ref mut hc) = config.app.healthcheck {
        hc.command = expand_env_vars(&hc.command, &env);
    }

    if let Some(ref mut readiness) = config.app.readiness {
        if let crate::config::schema::ProbeConfig::Exec { ref mut exec } = readiness.probe {
            for cmd in &mut exec.command {
                *cmd = expand_env_vars(cmd, &env);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env(vars: &[(&str, &str)]) -> HashMap<String, String> {
        vars.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_expand_simple_var() {
        let env = make_env(&[("FOO", "bar")]);
        assert_eq!(expand_env_vars("$FOO", &env), "bar");
    }

    #[test]
    fn test_expand_simple_var_not_found() {
        let env = HashMap::new();
        assert_eq!(expand_env_vars("$FOO", &env), "");
    }

    #[test]
    fn test_expand_braced_var() {
        let env = make_env(&[("FOO", "bar")]);
        assert_eq!(expand_env_vars("${FOO}", &env), "bar");
    }

    #[test]
    fn test_expand_braced_var_not_found() {
        let env = HashMap::new();
        assert_eq!(expand_env_vars("${FOO}", &env), "");
    }

    #[test]
    fn test_expand_braced_var_default() {
        let env = HashMap::new();
        assert_eq!(expand_env_vars("${FOO:-default}", &env), "default");
    }

    #[test]
    fn test_expand_braced_var_default_not_used() {
        let env = make_env(&[("FOO", "actual")]);
        assert_eq!(expand_env_vars("${FOO:-default}", &env), "actual");
    }

    #[test]
    fn test_expand_braced_var_alternate() {
        let env = make_env(&[("FOO", "bar")]);
        assert_eq!(expand_env_vars("${FOO:+alternate}", &env), "alternate");
    }

    #[test]
    fn test_expand_braced_var_alternate_unset() {
        let env = HashMap::new();
        assert_eq!(expand_env_vars("${FOO:+alternate}", &env), "");
    }

    #[test]
    fn test_expand_multiple_vars() {
        let env = make_env(&[("FOO", "hello"), ("BAR", "world")]);
        assert_eq!(expand_env_vars("$FOO ${BAR}!", &env), "hello world!");
    }

    #[test]
    fn test_expand_nested_value() {
        let env = make_env(&[("FOO", "bar"), ("BAR", "baz")]);
        assert_eq!(expand_env_vars("${FOO:-${BAR}}", &env), "bar");
    }

    #[test]
    fn test_expand_in_path() {
        let env = make_env(&[("APP", "myapp")]);
        assert_eq!(expand_env_vars("/var/run/${APP}", &env), "/var/run/myapp");
    }

    #[test]
    fn test_read_k8s_downward_api_empty() {
        std::env::remove_var("KUBERNETES_NAMESPACE");
        let meta = read_k8s_downward_api();
        assert!(!meta.contains_key("KUBERNETES_NAMESPACE"));
    }

    #[test]
    fn test_expand_config_env() {
        let mut config = crate::config::schema::Config::default();
        config
            .runtime
            .env
            .insert("PATH_EXPANDED".to_string(), "$HOME/bin".to_string());

        std::env::set_var("HOME", "/home/user");
        expand_config_env(&mut config);
        std::env::remove_var("HOME");

        assert_eq!(
            config.runtime.env.get("PATH_EXPANDED"),
            Some(&"/home/user/bin".to_string())
        );
    }
}
