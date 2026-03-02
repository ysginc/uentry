# Mainstream Container Reference Matrix

This directory provides mainstream container reference implementations with paired
secure and intentionally vulnerable overlays for `uentry`.

- Secure variants enable strict mode and audit reporting with restrictive defaults.
- Vulnerable variants intentionally disable strict protections and include dangerous
  configuration examples for testing and demonstration only.

| Technology | Base Image | Command | Secure Reference | Vulnerable Reference |
| ---------- | ---------- | ------- | ---------------- | -------------------- |
| nginx | `nginx:1.27-alpine` | `nginx -v` | [examples/reference/nginx](./nginx) | [examples/reference/nginx](./nginx) |
| node | `node:20-alpine` | `node --version` | [examples/reference/node](./node) | [examples/reference/node](./node) |
| python | `python:3.12-alpine` | `python --version` | [examples/reference/python](./python) | [examples/reference/python](./python) |
| java | `eclipse-temurin:21-jre` | `java -version` | [examples/reference/java](./java) | [examples/reference/java](./java) |
| postgres | `postgres:16-alpine` | `postgres --version` | [examples/reference/postgres](./postgres) | [examples/reference/postgres](./postgres) |
| redis | `redis:7.2-alpine` | `redis-server --version` | [examples/reference/redis](./redis) | [examples/reference/redis](./redis) |
| rabbitmq | `rabbitmq:3.13-alpine` | `rabbitmq-server --version` | [examples/reference/rabbitmq](./rabbitmq) | [examples/reference/rabbitmq](./rabbitmq) |
| elasticsearch | `docker.elastic.co/elasticsearch/elasticsearch:8.15.0` | `elasticsearch --version` | [examples/reference/elasticsearch](./elasticsearch) | [examples/reference/elasticsearch](./elasticsearch) |
| prometheus | `prom/prometheus:v2.55.1` | `prometheus --version` | [examples/reference/prometheus](./prometheus) | [examples/reference/prometheus](./prometheus) |
| grafana | `grafana/grafana:11.2.0` | `grafana server -v` | [examples/reference/grafana](./grafana) | [examples/reference/grafana](./grafana) |

Each stack folder contains:

- `Dockerfile.secure`
- `Dockerfile.vuln`
- `uentry.secure.yaml`
- `uentry.vuln.yaml`

## Security Demo Test Scripts

Use `examples/reference/scripts/run-security-demo.sh` to demonstrate the security
difference between each vulnerable and secure reference image.

### Prerequisites

- Docker daemon is running (`docker info` succeeds)
- Network access to pull base images and install `uentry` from package repositories

### Run all references

- `./examples/reference/scripts/run-security-demo.sh`

### Run one reference

- `./examples/reference/scripts/run-security-demo.sh java`
- `./examples/reference/scripts/run-one.sh java`

Supported stack values:

- `nginx`
- `node`
- `python`
- `java`
- `postgres`
- `redis`
- `elasticsearch`
- `prometheus`
- `grafana`

`rabbitmq` is currently disabled in the demo scripts.

### Expected behavior

For each stack, the script:

1. Builds `Dockerfile.vuln` and `Dockerfile.secure`
2. Runs each image with `LD_LIBRARY_PATH=/tmp/escape-attempt`
3. Verifies that vulnerable starts while secure blocks dangerous env injection

The scripts print `[PASS]`, `[FAIL]`, and `[SKIP]` per stack and write logs to
`target/reference-security-logs/<timestamp>/...`.

Exit codes:

- `0` = all selected stacks demonstrated successfully
- `1` = at least one stack failed a required assertion
- `2` = assertion failures were absent, but one or more stacks were skipped due
  to transient Docker/pull issues

Set `KEEP_DEMO_IMAGES=1` to keep built demo images instead of removing them on exit.
