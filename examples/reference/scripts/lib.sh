#!/usr/bin/env bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REFERENCE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

STACKS=(
    nginx
    node
    python
    java
    postgres
    redis
    elasticsearch
    prometheus
    grafana
)

print_usage() {
    local script_name
    script_name="$(basename "$1")"

    cat <<EOF
Usage:
  ./${script_name}                # Run all reference stacks
  ./${script_name} <stack>        # Run one stack

Stacks:
  ${STACKS[*]}
EOF
}

fail() {
    echo "[ERROR] $*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "Missing required command: $1"
}

ensure_docker_available() {
    docker info >/dev/null 2>&1 || fail "Docker is not available. Start Docker and try again."
}

is_known_stack() {
    local candidate="$1"
    local stack

    for stack in "${STACKS[@]}"; do
        if [[ "$stack" == "$candidate" ]]; then
            return 0
        fi
    done

    return 1
}

resolve_stacks() {
    if [[ $# -eq 0 ]]; then
        printf '%s\n' "${STACKS[@]}"
        return 0
    fi

    if [[ $# -ne 1 ]]; then
        return 1
    fi

    if ! is_known_stack "$1"; then
        return 1
    fi

    printf '%s\n' "$1"
}

stack_dir() {
    printf '%s/%s\n' "$REFERENCE_DIR" "$1"
}

image_tag() {
    printf 'uentry-reference-%s-%s:demo\n' "$1" "$2"
}

is_transient_docker_failure() {
    local log_file="$1"

    grep -Eqi 'error getting credentials|pull access denied|unauthorized|toomanyrequests|tls handshake timeout|i/o timeout|connection reset|temporary failure|name resolution|service unavailable' "$log_file"
}