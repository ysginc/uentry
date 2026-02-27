# Profile Catalog

Profiles provide preset configurations for common use cases. Use `--profile <name>` to select a profile.

## Built-in Profiles

### baseline

Safe defaults for any container.

```yaml
runtime:
  signal_forward: true

security:
  no_new_privs: false
  allow_root: false
```

**Use when:** No special requirements, want sensible defaults.

---

### k8s

Optimized for Kubernetes workloads.

```yaml
runtime:
  signal_forward: true
  env_allow:
    - KUBERNETES_*
    - POD_*
    - CONTAINER_*

secrets:
  file_to_env:
    - file: /var/run/secrets/kubernetes.io/serviceaccount/token
      env_var: KUBERNETES_SERVICE_ACCOUNT_TOKEN
      optional: true

lifecycle:
  shutdown_timeout_secs: 30
```

**Use when:** Running in Kubernetes, need access to pod metadata.

**Features:**
- Allows Kubernetes environment variables through
- Loads service account token if available
- Extended shutdown timeout for graceful pod termination

---

### web

Optimized for HTTP servers and web applications.

```yaml
runtime:
  signal_forward: true
  env_allow:
    - PORT
    - HOST
    - BIND

app:
  readiness:
    initial_delay_secs: 5
    interval_secs: 10
    http_get:
      path: /health
      port: 8080

lifecycle:
  startup_grace_secs: 10
  shutdown_timeout_secs: 30
```

**Use when:** Running HTTP servers, APIs, or web applications.

**Features:**
- Allows PORT/HOST/BIND environment variables
- Configures HTTP readiness probe on /health:8080
- Startup grace period for slow initializers

---

### worker

Optimized for background job processors and workers.

```yaml
runtime:
  signal_forward: true

lifecycle:
  startup_grace_secs: 30
  shutdown_timeout_secs: 60
```

**Use when:** Running background workers, job processors, or long-running tasks.

**Features:**
- Extended startup grace period (30s) for initialization
- Long shutdown timeout (60s) for completing in-flight work

## Custom Profiles

Place custom profiles in `/etc/uentry/profiles/<name>.yaml`:

```yaml
# /etc/uentry/profiles/myapp.yaml
runtime:
  user: "appuser"
  env:
    APP_ENV: "production"
    
security:
  no_new_privs: true
  writable_paths:
    - /var/log/myapp
    - /tmp

lifecycle:
  pre_start:
    command: "/app/migrate.sh"
    timeout_secs: 60
```

Then use with: `uentry --profile myapp /app/myapp`

## Profile Merging

Profiles provide defaults; your config overrides profile values.

**Example:**

Profile (`web`):
```yaml
app:
  readiness:
    http_get:
      path: /health
      port: 8080
```

Your config:
```yaml
app:
  readiness:
    http_get:
      port: 3000  # Override port only
```

Result: `http_get.path: /health` (from profile), `http_get.port: 3000` (from config)

## Combining Profiles

Only one profile can be active, but you can chain configs:

```bash
# Use web profile + custom overrides
uentry --profile web --config /etc/uentry/overrides.yaml /app/server
```

## Profile Discovery

uentry discovers profiles from:
1. Built-in profiles (baseline, k8s, web, worker)
2. `/etc/uentry/profiles/*.yaml`

List available profiles:
```bash
uentry --diagnose 2>&1 | grep -A20 "Profiles:"
```
