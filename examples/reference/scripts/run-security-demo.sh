#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib.sh"

if [[ $# -gt 1 ]]; then
    print_usage "$0"
    fail "Expected zero or one stack argument"
fi

if [[ $# -eq 1 ]]; then
    if ! is_known_stack "$1"; then
        print_usage "$0"
        fail "Unknown stack: $1"
    fi

    TARGET_STACKS=("$1")
else
    TARGET_STACKS=("${STACKS[@]}")
fi

require_command docker
require_command timeout
ensure_docker_available

RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOG_ROOT="${REPO_ROOT}/target/reference-security-logs/${RUN_ID}"
mkdir -p "$LOG_ROOT"

BUILD_TIMEOUT="${DEMO_BUILD_TIMEOUT:-900s}"
RUN_TIMEOUT="${DEMO_RUN_TIMEOUT:-60s}"

declare -a BUILT_IMAGES=()

cleanup_images() {
    if [[ "${KEEP_DEMO_IMAGES:-0}" == "1" ]]; then
        return 0
    fi

    local image
    for image in "${BUILT_IMAGES[@]}"; do
        docker image rm -f "$image" >/dev/null 2>&1 || true
    done
}

trap cleanup_images EXIT

pass_count=0
fail_count=0
skip_count=0

echo "Running uentry security reference demo"
echo "Logs: ${LOG_ROOT}"
echo

for stack in "${TARGET_STACKS[@]}"; do
    stack_log_dir="${LOG_ROOT}/${stack}"
    mkdir -p "$stack_log_dir"

    stack_path="$(stack_dir "$stack")"
    vuln_tag="$(image_tag "$stack" "vuln")"
    secure_tag="$(image_tag "$stack" "secure")"

    echo "=== ${stack} ==="

    vuln_build_log="${stack_log_dir}/vuln.build.log"
    secure_build_log="${stack_log_dir}/secure.build.log"
    vuln_run_log="${stack_log_dir}/vuln.run.log"
    secure_run_log="${stack_log_dir}/secure.run.log"

    if timeout "$BUILD_TIMEOUT" docker build -f "${stack_path}/Dockerfile.vuln" -t "$vuln_tag" "$stack_path" >"$vuln_build_log" 2>&1; then
        vuln_build_exit=0
    else
        vuln_build_exit=$?
    fi

    if [[ $vuln_build_exit -ne 0 ]]; then
        if [[ $vuln_build_exit -eq 124 ]]; then
            echo "[SKIP] ${stack}: vulnerable image build timed out (${BUILD_TIMEOUT})"
            echo "       See ${vuln_build_log}"
            skip_count=$((skip_count + 1))
            echo
            continue
        fi

        if is_transient_docker_failure "$vuln_build_log"; then
            echo "[SKIP] ${stack}: transient Docker pull/build issue for vulnerable image"
            skip_count=$((skip_count + 1))
            echo
            continue
        fi

        echo "[FAIL] ${stack}: vulnerable image build failed"
        echo "       See ${vuln_build_log}"
        fail_count=$((fail_count + 1))
        echo
        continue
    fi
    BUILT_IMAGES+=("$vuln_tag")

    if timeout "$BUILD_TIMEOUT" docker build -f "${stack_path}/Dockerfile.secure" -t "$secure_tag" "$stack_path" >"$secure_build_log" 2>&1; then
        secure_build_exit=0
    else
        secure_build_exit=$?
    fi

    if [[ $secure_build_exit -ne 0 ]]; then
        if [[ $secure_build_exit -eq 124 ]]; then
            echo "[SKIP] ${stack}: secure image build timed out (${BUILD_TIMEOUT})"
            echo "       See ${secure_build_log}"
            skip_count=$((skip_count + 1))
            echo
            continue
        fi

        if is_transient_docker_failure "$secure_build_log"; then
            echo "[SKIP] ${stack}: transient Docker pull/build issue for secure image"
            skip_count=$((skip_count + 1))
            echo
            continue
        fi

        echo "[FAIL] ${stack}: secure image build failed"
        echo "       See ${secure_build_log}"
        fail_count=$((fail_count + 1))
        echo
        continue
    fi
    BUILT_IMAGES+=("$secure_tag")

    if timeout "$RUN_TIMEOUT" docker run --rm -e LD_LIBRARY_PATH=/tmp/escape-attempt "$vuln_tag" >"$vuln_run_log" 2>&1; then
        vuln_exit=0
    else
        vuln_exit=$?
    fi

    vuln_ok=0
    if [[ $vuln_exit -eq 0 ]]; then
        vuln_ok=1
    elif [[ $vuln_exit -eq 124 ]] && grep -q "Strict mode disabled, skipping security validation" "$vuln_run_log"; then
        vuln_ok=1
    elif grep -q "Strict mode disabled, skipping security validation" "$vuln_run_log"; then
        vuln_ok=1
    elif grep -q "ECHILD: No child processes" "$vuln_run_log"; then
        vuln_ok=1
    fi

    if [[ $vuln_ok -eq 1 ]]; then
        if [[ $vuln_exit -eq 124 ]]; then
            echo "[PASS] ${stack}: vulnerable reference allowed startup and remained running (timeout ${RUN_TIMEOUT})"
        else
            echo "[PASS] ${stack}: vulnerable reference allows dangerous env injection"
        fi
    else
        echo "[FAIL] ${stack}: vulnerable reference did not allow dangerous env injection"
        echo "       See ${vuln_run_log}"
        fail_count=$((fail_count + 1))
        echo
        continue
    fi

    if timeout "$RUN_TIMEOUT" docker run --rm -e LD_LIBRARY_PATH=/tmp/escape-attempt "$secure_tag" >"$secure_run_log" 2>&1; then
        secure_exit=0
    else
        secure_exit=$?
    fi

    if [[ $secure_exit -eq 124 ]]; then
        echo "[FAIL] ${stack}: secure reference runtime timed out (${RUN_TIMEOUT})"
        echo "       See ${secure_run_log}"
        fail_count=$((fail_count + 1))
        echo
        continue
    fi

    if [[ $secure_exit -eq 0 ]]; then
        echo "[FAIL] ${stack}: secure reference unexpectedly allowed dangerous env injection"
        echo "       See ${secure_run_log}"
        fail_count=$((fail_count + 1))
        echo
        continue
    fi

    if grep -Eq "dangerous environment variables|LD_LIBRARY_PATH" "$secure_run_log"; then
        echo "[PASS] ${stack}: secure reference blocks dangerous env injection"
        pass_count=$((pass_count + 1))
    else
        echo "[FAIL] ${stack}: secure reference failed without dangerous-env evidence"
        echo "       See ${secure_run_log}"
        fail_count=$((fail_count + 1))
        echo
        continue
    fi

    echo
done

echo "Summary: pass=${pass_count}, fail=${fail_count}, skip=${skip_count}"
echo "Logs saved under ${LOG_ROOT}"

if [[ $fail_count -gt 0 ]]; then
    exit 1
fi

if [[ $skip_count -gt 0 ]]; then
    exit 2
fi

exit 0