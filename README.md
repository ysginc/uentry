# uentry

[![CI](https://github.com/ysginc/uentry/actions/workflows/ci.yml/badge.svg)](https://github.com/ysginc/uentry/actions/workflows/ci.yml)
[![Release](https://github.com/ysginc/uentry/actions/workflows/release.yml/badge.svg)](https://github.com/ysginc/uentry/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **⚠️ Work in Progress**
>
> This project is under active development. The API and configuration format may change before the 1.0 release. Not recommended for production use yet.

A universal container entrypoint with PID 1 supervision and fail-closed security posture.

## Why uentry?

Running containers as PID 1 is tricky. You need to handle signals, reap zombies, and manage the child process lifecycle. uentry does all of this while providing:

- **Security-first design** - Fail-closed by default in strict mode
- **Declarative configuration** - YAML configs, profiles, and environment variables
- **Zero runtime dependencies (musl builds)** - Static ~1.5MB binary, with glibc builds also published
- **Kubernetes-native** - Built-in profiles for common workloads

## Quick Start

### Installation

**Download from releases:**

```bash
# Binary (glibc Linux)
curl -sL https://github.com/ysginc/uentry/releases/latest/download/uentry-x86_64-gnu.tar.gz | tar xz
sudo mv uentry /usr/local/bin/

# Binary (Alpine/static musl)
curl -sL https://github.com/ysginc/uentry/releases/latest/download/uentry-x86_64-musl.tar.gz | tar xz
sudo mv uentry /usr/local/bin/
```

**System packages:**

```bash
# Debian/Ubuntu
curl -sL https://github.com/ysginc/uentry/releases/latest/download/uentry-x86_64-gnu.deb -o uentry.deb
sudo dpkg -i uentry.deb

# RHEL/CentOS/Fedora
curl -sL https://github.com/ysginc/uentry/releases/latest/download/uentry-x86_64-gnu.rpm -o uentry.rpm
sudo rpm -i uentry.rpm

# Alpine
curl -sL https://github.com/ysginc/uentry/releases/latest/download/uentry-x86_64-musl.apk -o uentry.apk
sudo apk add --allow-untrusted uentry.apk
```

**Container image:**

```dockerfile
FROM ghcr.io/ysginc/uentry:latest AS uentry

# Use in your Dockerfile
FROM alpine
COPY --from=uentry /uentry /uentry
ENTRYPOINT ["/uentry"]
CMD ["/app/server"]
```

**Build from source:**

```bash
# glibc build (Debian/Ubuntu/RHEL/Fedora)
cargo build --release --target x86_64-unknown-linux-gnu

# musl build (Alpine/static)
cargo build --release --target x86_64-unknown-linux-musl
```

### Basic Usage

```bash
# Run any command as PID 1
uentry -- nginx -g "daemon off;"

# Use a profile for common patterns
uentry --profile web -- /app/server

# Enable strict security mode
uentry --strict --user appuser --group appgroup -- /app/app
```

### Dockerfile Example

```dockerfile
# Build uentry
FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev
COPY . /src
WORKDIR /src
RUN cargo build --release --target x86_64-unknown-linux-musl

# Minimal runtime image
FROM scratch
COPY --from=builder /src/target/x86_64-unknown-linux-musl/release/uentry /uentry
COPY --from=builder /etc/ssl/certs /etc/ssl/certs

ENTRYPOINT ["/uentry"]
CMD ["/app/server"]
```

## Features

| Feature | Description |
|---------|-------------|
| **PID 1 Supervisor** | Signal forwarding, zombie reaping, exit code propagation |
| **Profiles** | Built-in presets for k8s, web, worker workloads |
| **Lifecycle Hooks** | Pre-start and post-stop commands with timeouts |
| **Secrets Management** | File↔env injection with automatic log redaction |
| **Readiness Probes** | HTTP, TCP, and exec-based health checks |
| **Strict Mode** | Fail-closed security posture with preflight checks |
| **Privilege Dropping** | UID/GID, supplementary groups, no_new_privs |

## Configuration

### Precedence

1. CLI flags (highest)
2. Environment variables (`UENTRY_*`)
3. Config file (`/etc/uentry/config.yaml`)
4. Profile defaults
5. Built-in defaults (lowest)

### Example Config

```yaml
# /etc/uentry/config.yaml
runtime:
  user: appuser
  group: appgroup
  env:
    LOG_LEVEL: info
  signal_forward: true

security:
  no_new_privs: true
  writable_paths:
    - /tmp
    - /var/log/app

secrets:
  file_to_env:
    - file: /run/secrets/db_password
      env_var: DB_PASSWORD

lifecycle:
  pre_start:
    command: /app/migrate.sh
    timeout_secs: 30
  shutdown_timeout_secs: 30

app:
  readiness:
    initial_delay_secs: 5
    http_get:
      path: /health
      port: 8080
```

### CLI Reference

```
uentry [OPTIONS] [--] <COMMAND>...

Arguments:
  <COMMAND>  Command to execute

Options:
  -c, --config <FILE>     Configuration file path
  -p, --profile <NAME>    Built-in profile (baseline, k8s, web, worker)
      --strict            Enable strict fail-closed mode
      --diagnose          Run diagnostics and exit
  -v, --verbose           Increase verbosity
  -q, --quiet             Suppress output
      --log-format <FMT>  Log format: text, json
  -h, --help              Print help
  -V, --version           Print version
```

## Profiles

| Profile | Use Case | Key Features |
|---------|----------|--------------|
| `baseline` | General purpose | Safe defaults, signal forwarding |
| `k8s` | Kubernetes | K8s env vars, service account token |
| `web` | HTTP servers | HTTP readiness probe, startup grace |
| `worker` | Background jobs | Extended timeouts for graceful shutdown |

```bash
# HTTP server with readiness probe
uentry --profile web -- /app/server

# Kubernetes workload
uentry --profile k8s -- /app/worker

# Background job processor
uentry --profile worker -- /app/jobs
```

## Security

### Strict Mode

Enable with `--strict` for fail-closed behavior:

```yaml
runtime:
  strict: true
  user: "1000"
  group: "1000"

security:
  no_new_privs: true
  writable_paths:
    - /tmp
```

Strict mode refuses to start if:
- Running as root without privilege drop config
- Root filesystem is writable (unless allowlisted)
- Dangerous mounts detected (docker.sock, etc.)
- Dangerous env vars present (LD_PRELOAD, etc.)

### Secrets

Secrets are automatically redacted from all log output:

```yaml
secrets:
  file_to_env:
    - file: /run/secrets/api_key
      env_var: API_KEY
  env_to_file:
    - env_var: CONFIG_JSON
      file: /app/config.json
      mode: "0600"
```

## Lifecycle

uentry runs a phased startup:

```
preflight → sanitize → fs_prep → secrets → security → hooks → exec
```

### Pre-start Hooks

```yaml
lifecycle:
  pre_start:
    command: /app/init.sh
    args: ["--migrate"]
    timeout_secs: 30
```

### Readiness Probes

```yaml
# HTTP probe
app:
  readiness:
    initial_delay_secs: 5
    interval_secs: 10
    http_get:
      path: /health
      port: 8080

# TCP probe
app:
  readiness:
    tcp_socket:
      port: 6379

# Exec probe
app:
  readiness:
    exec:
      command: ["/app/health-check.sh"]
```

## Derived Images

Overlay uentry on existing images:

```dockerfile
# Add uentry to nginx
FROM nginx:alpine
COPY uentry /uentry
RUN chmod +x /uentry
ENTRYPOINT ["/uentry"]
CMD ["nginx", "-g", "daemon off;"]
```

```dockerfile
# Add uentry to redis
FROM redis:alpine
COPY uentry /uentry
RUN chmod +x /uentry
ENTRYPOINT ["/uentry"]
CMD ["redis-server"]
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `UENTRY_STRICT` | Enable strict mode (`true`/`false`) |
| `UENTRY_USER` | User to run as |
| `UENTRY_GROUP` | Group to run as |
| `UENTRY_CWD` | Working directory |
| `UENTRY_PROFILE` | Profile name |
| `UENTRY_CONFIG` | Config file path |
| `UENTRY_LOG_FORMAT` | `json` or `text` |
| `UENTRY_LOG_LEVEL` | `trace`, `debug`, `info`, `warn`, `error` |

## Distribution

Each release includes:

| Format | Architecture | Use Case |
|--------|--------------|----------|
| `uentry-x86_64-gnu.tar.gz` | x86_64 | Debian/Ubuntu/RHEL/Fedora binary |
| `uentry-aarch64-gnu.tar.gz` | aarch64 | ARM64 Debian/Ubuntu/RHEL/Fedora binary |
| `uentry-x86_64-musl.tar.gz` | x86_64 | Alpine/static binary |
| `uentry-aarch64-musl.tar.gz` | aarch64 | ARM64 Alpine/static binary |
| `uentry-x86_64-gnu.deb` | x86_64 | Debian/Ubuntu |
| `uentry-aarch64-gnu.deb` | aarch64 | Debian/Ubuntu (ARM) |
| `uentry-x86_64-gnu.rpm` | x86_64 | RHEL/CentOS/Fedora |
| `uentry-aarch64-gnu.rpm` | aarch64 | RHEL/CentOS/Fedora (ARM) |
| `uentry-x86_64-musl.apk` | x86_64 | Alpine Linux |
| `uentry-aarch64-musl.apk` | aarch64 | Alpine Linux (ARM) |
| `ghcr.io/ysginc/uentry` | multi-arch | Container image |

## Development

```bash
# Build
cargo build

# Test
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt

# Release build (static)
cargo build --release --target x86_64-unknown-linux-musl

# Run container tests (requires Docker)
cargo test --test container_tests

# Build system packages (requires nfpm)
make packages
```

## Documentation

- `man uentry` - Command reference (installed with packages)
- `man uentry` (section 5) - Configuration file format
- [Configuration Reference](docs/reference/config.md) - Full config schema
- [Profile Catalog](docs/reference/profiles.md) - Built-in profiles
- [Security Model](docs/reference/security.md) - Threat model and mitigations
- [Roadmap](docs/ROADMAP.md) - Project status and future plans

## Comparison

| Feature | uentry | tini | s6 |
|---------|--------|------|-----|
| PID 1 supervision | ✅ | ✅ | ✅ |
| Signal forwarding | ✅ | ✅ | ✅ |
| Zombie reaping | ✅ | ✅ | ✅ |
| Config files | ✅ | ❌ | ✅ |
| Profiles | ✅ | ❌ | ❌ |
| Lifecycle hooks | ✅ | ❌ | ✅ |
| Secrets management | ✅ | ❌ | ❌ |
| Readiness probes | ✅ | ❌ | ❌ |
| Strict mode | ✅ | ❌ | ❌ |
| Binary size | ~1.5MB | ~20KB | ~500KB |

## License

MIT
