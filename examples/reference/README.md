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
