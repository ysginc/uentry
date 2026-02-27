# Configuration Reference

uentry is configured via YAML files, environment variables, and CLI flags.

## Configuration Sources (Precedence)

1. **CLI flags** (highest priority)
2. **Environment variables** (`UENTRY_*`)
3. **Configuration file** (`/etc/uentry/config.yaml`)
4. **Profile defaults** (`--profile`)
5. **Built-in defaults** (lowest priority)

## Configuration File

Default location: `/etc/uentry/config.yaml`

Set via: `--config /path/to/config.yaml` or `UENTRY_CONFIG=/path/to/config.yaml`

## Full Schema

```yaml
runtime:
  # Enable strict fail-closed mode
  strict: false
  
  # User to run as (username or UID)
  user: null
  
  # Group to run as (groupname or GID)
  group: null
  
  # Supplementary groups
  supplementary_groups: []
  
  # Directories to create before starting
  ensure_dirs: []
  
  # Environment variables to set
  env: {}
  
  # Environment patterns to allow (globs)
  env_allow: []
  
  # Environment patterns to deny (globs)
  env_deny: []
  
  # Forward signals to child process
  signal_forward: true
  
  # Umask for created files
  umask: null
  
  # Working directory
  cwd: null

security:
  # Set PR_SET_NO_NEW_PRIVS
  no_new_privs: false
  
  # Allow running as root
  allow_root: false
  
  # Paths allowed to be writable in strict mode
  writable_paths: []

audit:
  # Enable runtime audit reporting
  enabled: false

  # Enable deep tracing (best-effort)
  deep_trace: false

  # Write audit JSON report to file (null = stderr)
  output: null

  # Write derived profile snippet to file
  profile_output: null

  # Deep trace backend: auto, strace, none
  backend: auto

secrets:
  # Load secrets from files into environment
  file_to_env: []
  
  # Write environment variables to files
  env_to_file: []

lifecycle:
  # Command to run before main process
  pre_start: null
  
  # Command to run after main process exits
  post_stop: null
  
  # Grace period for startup
  startup_grace_secs: 0
  
  # Timeout for graceful shutdown
  shutdown_timeout_secs: 30

app:
  # Application name (for logging)
  name: null
  
  # Health check configuration
  healthcheck: null
  
  # Readiness probe configuration
  readiness: null
```

## Runtime Configuration

### `runtime.strict`

Enable strict fail-closed security mode.

- **Type:** `bool`
- **Default:** `false`
- **CLI:** `--strict`
- **Env:** `UENTRY_STRICT=true`

When enabled, uentry will refuse to start if:
- Running as root without privilege drop configuration
- Root filesystem is writable (unless paths are allowlisted)
- Dangerous mounts are detected (proc, sys, dev without expected configs)
- Dangerous environment variables are present (LD_PRELOAD, etc.)

### `runtime.user` / `runtime.group`

Drop privileges to specified user/group before exec.

- **Type:** `string` (username, groupname, or numeric ID)
- **Default:** `null` (no privilege drop)
- **Env:** `UENTRY_USER`, `UENTRY_GROUP`

```yaml
runtime:
  user: "1000"
  group: "1000"
```

### `runtime.supplementary_groups`

Additional groups to set when dropping privileges.

- **Type:** `array of strings`
- **Default:** `[]`

```yaml
runtime:
  supplementary_groups:
    - docker
    - audio
```

### `runtime.ensure_dirs`

Create directories with specified permissions before starting.

- **Type:** `array of objects`
- **Default:** `[]`

```yaml
runtime:
  ensure_dirs:
    - path: /var/log/app
      mode: "0755"
      owner: "appuser"
      group: "appgroup"
    - path: /tmp/cache
      mode: "1777"
```

### `runtime.env`

Set environment variables for the child process.

- **Type:** `object (string -> string)`
- **Default:** `{}`

```yaml
runtime:
  env:
    PATH: "/app/bin:/usr/bin"
    LOG_LEVEL: "info"
    APP_ENV: "${ENVIRONMENT:-production}"
```

Environment variable expansion is supported:
- `$VAR` - Simple expansion
- `${VAR}` - Brace expansion
- `${VAR:-default}` - Default value if unset
- `${VAR:+alternate}` - Alternate if set

### `runtime.env_allow` / `runtime.env_deny`

Control which environment variables are inherited.

- **Type:** `array of glob patterns`
- **Default:** `[]` (all allowed, none denied)

```yaml
runtime:
  env_allow:
    - "PATH"
    - "HOME"
    - "KUBERNETES_*"
  env_deny:
    - "AWS_*"
    - "SECRET_*"
```

### `runtime.signal_forward`

Forward received signals to the child process.

- **Type:** `bool`
- **Default:** `true`
- **CLI:** `--no-signal-forward`

When running as PID 1, signals are forwarded to the child process.

### `runtime.cwd`

Set working directory for the child process.

- **Type:** `string` (path)
- **Default:** `null` (inherit from parent)

```yaml
runtime:
  cwd: /app
```

### `runtime.umask`

Set umask before exec.

- **Type:** `string` (octal mode)
- **Default:** `null` (inherit from parent)

```yaml
runtime:
  umask: "022"
```

## Security Configuration

### `security.no_new_privs`

Set the `no_new_privs` bit to prevent privilege escalation.

- **Type:** `bool`
- **Default:** `false`

When set, the process and its descendants cannot gain new privileges via exec.

### `security.allow_root`

Allow running as root in strict mode.

- **Type:** `bool`
- **Default:** `false`

Only use if you understand the security implications.

### `security.writable_paths`

Paths allowed to be writable in strict mode.

- **Type:** `array of paths`
- **Default:** `[]`

```yaml
security:
  writable_paths:
    - /tmp
    - /var/log
    - /var/run
```

## Audit Configuration

### `audit.enabled`

Enable runtime audit collection and final report generation.

- **Type:** `bool`
- **Default:** `false`

When disabled, no audit session is created and no report is emitted.

### `audit.deep_trace`

Request deep tracing of file/process syscalls.

- **Type:** `bool`
- **Default:** `false`

When enabled, uentry attempts deep tracing via `strace` unless disabled by backend policy.
If tracing is unavailable, uentry falls back to lightweight audit reporting.

### `audit.output`

Path for the audit JSON report.

- **Type:** `string` (path) or `null`
- **Default:** `null`

When `null`, the audit report is written to stderr.

### `audit.profile_output`

Path for the generated profile snippet (YAML).

- **Type:** `string` (path) or `null`
- **Default:** `null`

When set, uentry writes derived `runtime.env_allow` and `security.writable_paths` hints.

### `audit.backend`

Select deep trace backend behavior.

- **Type:** `enum` (`auto`, `strace`, `none`)
- **Default:** `auto`

Backend behavior:

- `auto`: use `strace` when available; otherwise continue without deep trace.
- `strace`: request `strace` explicitly; still downgrades if unavailable.
- `none`: disable deep trace even when `deep_trace: true`.

Environment overrides for audit settings:

- `UENTRY_AUDIT` -> `audit.enabled`
- `UENTRY_AUDIT_DEEP` -> `audit.deep_trace`
- `UENTRY_AUDIT_OUTPUT` -> `audit.output`
- `UENTRY_AUDIT_PROFILE_OUTPUT` -> `audit.profile_output`
- `UENTRY_AUDIT_BACKEND` -> `audit.backend`

Portable audit mode:

```yaml
audit:
  enabled: true
  deep_trace: false
  output: /var/log/uentry/audit.json
  profile_output: /var/log/uentry/profile.yaml
  backend: auto
```

Deep tracing with fallback (`backend: auto`):

```yaml
audit:
  enabled: true
  deep_trace: true
  output: /var/log/uentry/audit.json
  profile_output: /var/log/uentry/profile.yaml
  backend: auto
```

Force no deep trace (`backend: none`):

```yaml
audit:
  enabled: true
  deep_trace: true
  output: /var/log/uentry/audit.json
  profile_output: /var/log/uentry/profile.yaml
  backend: none
```

## Secrets Configuration

### `secrets.file_to_env`

Load secrets from files into environment variables.

- **Type:** `array of objects`
- **Default:** `[]`

```yaml
secrets:
  file_to_env:
    - file: /run/secrets/db_password
      env_var: DB_PASSWORD
      optional: false
```

### `secrets.env_to_file`

Write environment variables to files.

- **Type:** `array of objects`
- **Default:** `[]`

```yaml
secrets:
  env_to_file:
    - env_var: CONFIG_JSON
      file: /app/config.json
      mode: "0600"
```

## Lifecycle Configuration

### `lifecycle.pre_start`

Execute a command before the main process.

- **Type:** `object`
- **Default:** `null`

```yaml
lifecycle:
  pre_start:
    command: "/app/init.sh"
    args: ["--migrate"]
    timeout_secs: 30
    env:
      INIT_MODE: "true"
```

### `lifecycle.post_stop`

Execute a command after the main process exits.

- **Type:** `object`
- **Default:** `null`

```yaml
lifecycle:
  post_stop:
    command: "/app/cleanup.sh"
    timeout_secs: 10
```

### `lifecycle.startup_grace_secs`

Grace period before readiness probes begin.

- **Type:** `integer`
- **Default:** `0`

### `lifecycle.shutdown_timeout_secs`

Timeout for graceful shutdown (SIGTERM -> SIGKILL).

- **Type:** `integer`
- **Default:** `30`

## App Configuration

### `app.readiness`

Configure readiness probes.

- **Type:** `object`
- **Default:** `null`

HTTP probe:
```yaml
app:
  readiness:
    initial_delay_secs: 5
    interval_secs: 10
    timeout_secs: 5
    failure_threshold: 3
    http_get:
      path: /health
      port: 8080
      host: "127.0.0.1"
```

TCP probe:
```yaml
app:
  readiness:
    initial_delay_secs: 5
    interval_secs: 10
    tcp_socket:
      port: 8080
```

Exec probe:
```yaml
app:
  readiness:
    initial_delay_secs: 5
    interval_secs: 10
    exec:
      command: ["/app/health-check.sh"]
```

## Environment Variables

All configuration can be set via environment variables with `UENTRY_` prefix:

| Variable | Maps To |
|----------|---------|
| `UENTRY_STRICT` | `runtime.strict` |
| `UENTRY_USER` | `runtime.user` |
| `UENTRY_GROUP` | `runtime.group` |
| `UENTRY_CWD` | `runtime.cwd` |
| `UENTRY_PROFILE` | `--profile` |
| `UENTRY_CONFIG` | `--config` |
| `UENTRY_LOG_LEVEL` | Log level (trace, debug, info, warn, error) |
| `UENTRY_LOG_FORMAT` | Log format (text, json) |

## CLI Flags

```
uentry [OPTIONS] [--] <COMMAND>...

Arguments:
  <COMMAND>  Command to execute

Options:
  -c, --config <FILE>     Configuration file path
  -p, --profile <NAME>    Built-in profile to use
      --strict            Enable strict fail-closed mode
      --diagnose          Run diagnostics and exit
  -v, --verbose...        Increase verbosity
  -q, --quiet             Suppress output
      --log-format <FMT>  Log format: text, json
  -h, --help              Print help
  -V, --version           Print version
```
