# Security Model

uentry implements a **fail-closed** security posture: unsafe configurations are rejected rather than silently degraded.

## Threat Model

uentry protects against:

| Threat | Mitigation |
|--------|------------|
| Running as root | Refuse startup without drop config in strict mode |
| Privilege escalation | `no_new_privs` bit, drop capabilities |
| Secret leakage | Redaction in logs, secure file permissions |
| Compromised environment | Environment variable sanitization |
| Mutable container | Read-only rootfs enforcement |
| Signal spoofing | Signal forwarding from PID 1 |

## Strict Mode

Enable with `--strict` or `runtime.strict: true`.

### Preflight Checks

When strict mode is enabled, uentry performs these checks before exec:

#### 1. Root User Check

```
FAIL if UID == 0 AND no privilege drop configured
```

Mitigation: Configure `runtime.user` or use `--allow-root` (not recommended).

#### 2. Writable Rootfs Check

```
FAIL if / is writable AND no writable_paths configured
```

Mitigation: Use read-only container filesystem, or allowlist specific writable paths.

#### 3. Mount Check

```
FAIL if unexpected mounts detected
```

Checks for dangerous mount configurations:
- `/proc` without expected masks
- `/sys` without expected masks
- `/dev` with full device access

#### 4. Environment Check

```
FAIL if dangerous environment variables present
```

Blocked variables:
- `LD_PRELOAD` - Library injection
- `LD_LIBRARY_PATH` - Library path hijacking
- `LD_AUDIT` - Audit library injection
- `LD_BIND_NOT` - Symbol binding manipulation

### Configuration

```yaml
runtime:
  strict: true

security:
  # Prevent exec-based privilege escalation
  no_new_privs: true
  
  # Allow running as root (dangerous!)
  allow_root: false
  
  # Allowed writable paths in strict mode
  writable_paths:
    - /tmp
    - /var/log/app
```

## Privilege Dropping

uentry can drop privileges before executing the child process:

```yaml
runtime:
  user: "1000"        # Drop to UID 1000
  group: "1000"       # Drop to GID 1000
  supplementary_groups:
    - "audio"         # Additional groups
  umask: "022"        # Set umask
```

### Process

1. Parse user/group (name or numeric ID)
2. Set supplementary groups via `setgroups(2)`
3. Set GID via `setgid(2)`
4. Set UID via `setuid(2)`
5. Set umask via `umask(2)`
6. Set `PR_SET_NO_NEW_PRIVS` if configured

### Verification

After dropping, uentry verifies:
- Effective UID matches target
- Effective GID matches target
- Supplementary groups are set correctly

## Secret Handling

### File to Environment

```yaml
secrets:
  file_to_env:
    - file: /run/secrets/db_password
      env_var: DB_PASSWORD
      optional: false
```

**Security properties:**
- File read with minimal permissions
- Content stored in memory only
- Redacted from all log output
- File permissions unchanged (not chmod'd)

### Environment to File

```yaml
secrets:
  env_to_file:
    - env_var: API_KEY
      file: /app/config/api.key
      mode: "0600"
```

**Security properties:**
- Written with specified permissions (default 0600)
- Parent directory must exist
- No world-readable defaults

### Redaction

All secret values are redacted from:
- Log output (tracing)
- Diagnostics output
- Error messages

Pattern: `[REDACTED]` replaces secret values.

## Signal Handling

When running as PID 1:

1. uentry registers handlers for SIGTERM, SIGINT, SIGCHLD
2. On SIGTERM/SIGINT: forward to child, wait for exit
3. On timeout: send SIGKILL to child
4. On SIGCHLD: reap zombie processes

### Signal Forwarding

```yaml
runtime:
  signal_forward: true  # Default
```

When enabled, signals are forwarded to the child process:
- Child can implement graceful shutdown
- Container orchestrators get expected behavior

## Filesystem Preparation

### Directory Creation

```yaml
runtime:
  ensure_dirs:
    - path: /var/log/app
      mode: "0755"
      owner: "1000"
      group: "1000"
```

**Security properties:**
- No recursive operations (no `-r` equivalent)
- Explicit permissions required
- Ownership optional, defaults to current

### Writable Path Enforcement

In strict mode, only allowlisted paths can be written:

```yaml
security:
  writable_paths:
    - /tmp
    - /var/log/app
    - /var/run/app
```

Verification: test write access to `/` and compare against allowlist.

## Best Practices

### Container Configuration

```dockerfile
# Run as non-root
USER 1000:1000

# Read-only filesystem
VOLUME ["/tmp"]
# docker run --read-only ...

# Drop all capabilities
# docker run --cap-drop=ALL ...

# No new privileges
# docker run --security-opt=no-new-privileges ...
```

### Kubernetes Pod Spec

```yaml
spec:
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
    readOnlyRootFilesystem: true
    allowPrivilegeEscalation: false
    capabilities:
      drop: ["ALL"]
```

### uentry Config

```yaml
runtime:
  strict: true
  user: "1000"
  group: "1000"

security:
  no_new_privs: true
  writable_paths:
    - /tmp
    - /var/log

secrets:
  file_to_env:
    - file: /run/secrets/db_password
      env_var: DB_PASSWORD
```

## Audit Checklist

Before deploying with strict mode:

- [ ] User/group configured for privilege drop
- [ ] Writable paths explicitly allowlisted
- [ ] Secrets loaded from files, not environment
- [ ] No hardcoded credentials in config
- [ ] Health checks configured for availability
- [ ] Shutdown timeout appropriate for workload
- [ ] Pre-start hooks validated and bounded

## Limitations

uentry does NOT provide:
- Container isolation (use container runtime)
- Network segmentation (use network policies)
- Mandatory access control (use SELinux/AppArmor)
- Resource limits (use cgroups)
- Process supervision (use s6/tini)

These are orthogonal concerns handled by the container runtime and orchestrator.
