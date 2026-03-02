#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ $# -ne 1 ]]; then
    cat <<EOF
Usage:
  ./run-one.sh <stack>

Examples:
  ./run-one.sh java
  ./run-one.sh nginx
EOF
    exit 1
fi

exec "${SCRIPT_DIR}/run-security-demo.sh" "$1"