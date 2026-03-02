#!/usr/bin/env bash
set -euo pipefail

overwrite_existing=false
configure_pages=true
target_repo=""

gpg_name="uentry packages repository"
gpg_email="community@ysginc.io"
gpg_expire="2y"
gpg_passphrase=""

created_secrets=()
skipped_secrets=()

print_usage() {
  cat <<'USAGE'
Usage: setup-repo-keys.sh [options]

Options:
  --repo OWNER/REPO         Target repository (default: current repository)
  --overwrite               Overwrite existing repository secrets
  --configure-pages         Ensure Pages build_type is workflow (default)
  --no-configure-pages      Skip Pages configuration
  --gpg-name NAME           GPG key real name
  --gpg-email EMAIL         GPG key email
  --gpg-expire DURATION     GPG expiry (default: 2y)
  --gpg-passphrase VALUE    Optional GPG passphrase
  -h, --help                Show this help
USAGE
}

fail() {
  echo "Error: $*" >&2
  exit 1
}

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    fail "Required command not found: $command_name"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      [[ $# -ge 2 ]] || fail "Missing value for --repo"
      target_repo="$2"
      shift 2
      ;;
    --overwrite)
      overwrite_existing=true
      shift
      ;;
    --configure-pages)
      configure_pages=true
      shift
      ;;
    --no-configure-pages)
      configure_pages=false
      shift
      ;;
    --gpg-name)
      [[ $# -ge 2 ]] || fail "Missing value for --gpg-name"
      gpg_name="$2"
      shift 2
      ;;
    --gpg-email)
      [[ $# -ge 2 ]] || fail "Missing value for --gpg-email"
      gpg_email="$2"
      shift 2
      ;;
    --gpg-expire)
      [[ $# -ge 2 ]] || fail "Missing value for --gpg-expire"
      gpg_expire="$2"
      shift 2
      ;;
    --gpg-passphrase)
      [[ $# -ge 2 ]] || fail "Missing value for --gpg-passphrase"
      gpg_passphrase="$2"
      shift 2
      ;;
    -h|--help)
      print_usage
      exit 0
      ;;
    *)
      fail "Unknown option: $1"
      ;;
  esac
done

require_command gh
require_command gpg
require_command openssl
require_command base64

if ! gh auth status >/dev/null 2>&1; then
  fail "GitHub CLI is not authenticated. Run: gh auth login"
fi

if [[ -z "$target_repo" ]]; then
  target_repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner 2>/dev/null || true)"
fi

if [[ -z "$target_repo" ]]; then
  fail "Unable to determine repository. Pass --repo OWNER/REPO"
fi

tmp_dir="$(mktemp -d)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

export GNUPGHOME="$tmp_dir/gnupg"
mkdir -p "$GNUPGHOME"
chmod 700 "$GNUPGHOME"

gpg_batch_file="$tmp_dir/gpg-batch.txt"

{
  echo "Key-Type: RSA"
  echo "Key-Length: 4096"
  echo "Subkey-Type: RSA"
  echo "Subkey-Length: 4096"
  echo "Name-Real: $gpg_name"
  echo "Name-Email: $gpg_email"
  echo "Expire-Date: $gpg_expire"
  if [[ -n "$gpg_passphrase" ]]; then
    echo "Passphrase: $gpg_passphrase"
  else
    echo "%no-protection"
  fi
  echo "%commit"
} > "$gpg_batch_file"

gpg --batch --generate-key "$gpg_batch_file" >/dev/null 2>&1

gpg_uid="$gpg_name <$gpg_email>"
gpg_key_id="$(gpg --batch --with-colons --list-secret-keys "$gpg_uid" | awk -F: '/^fpr:/ {print $10; exit}')"
[[ -n "$gpg_key_id" ]] || fail "Failed to resolve generated GPG key fingerprint"

gpg_private_key_file="$tmp_dir/private.asc"
if [[ -n "$gpg_passphrase" ]]; then
  gpg --batch --yes --pinentry-mode loopback --passphrase "$gpg_passphrase" --armor \
    --export-secret-keys "$gpg_key_id" > "$gpg_private_key_file"
else
  gpg --batch --yes --armor --export-secret-keys "$gpg_key_id" > "$gpg_private_key_file"
fi
gpg_private_key="$(cat "$gpg_private_key_file")"

apk_private_key_file="$tmp_dir/uentry-packages.rsa"
apk_public_key_file="$tmp_dir/uentry-packages.rsa.pub"

openssl genrsa -out "$apk_private_key_file" 4096 >/dev/null 2>&1
openssl rsa -in "$apk_private_key_file" -pubout -out "$apk_public_key_file" >/dev/null 2>&1

apk_private_key_b64="$(base64 < "$apk_private_key_file" | tr -d '\n')"
apk_public_key="$(cat "$apk_public_key_file")"

secret_exists() {
  local secret_name="$1"
  gh api "repos/${target_repo}/actions/secrets/${secret_name}" >/dev/null 2>&1
}

set_repo_secret() {
  local secret_name="$1"
  local secret_value="$2"

  if ! $overwrite_existing && secret_exists "$secret_name"; then
    skipped_secrets+=("$secret_name")
    echo "Skipped existing secret: $secret_name"
    return
  fi

  printf '%s' "$secret_value" | gh secret set "$secret_name" --repo "$target_repo" --app actions >/dev/null
  created_secrets+=("$secret_name")
  echo "Set secret: $secret_name"
}

set_repo_secret "PACKAGE_REPO_GPG_PRIVATE_KEY" "$gpg_private_key"
set_repo_secret "PACKAGE_REPO_GPG_KEY_ID" "$gpg_key_id"

if [[ -n "$gpg_passphrase" ]]; then
  set_repo_secret "PACKAGE_REPO_GPG_PASSPHRASE" "$gpg_passphrase"
fi

set_repo_secret "PACKAGE_REPO_APK_PRIVATE_KEY_B64" "$apk_private_key_b64"
set_repo_secret "PACKAGE_REPO_APK_PUBLIC_KEY" "$apk_public_key"

pages_status="skipped"

configure_pages_workflow_build() {
  local current_build_type
  if current_build_type="$(gh api "repos/${target_repo}/pages" --jq '.build_type' 2>/dev/null)"; then
    if [[ "$current_build_type" != "workflow" ]]; then
      gh api -X PUT "repos/${target_repo}/pages" -f build_type='workflow' >/dev/null
      pages_status="updated"
      return
    fi
    pages_status="already-workflow"
    return
  fi

  gh api -X POST "repos/${target_repo}/pages" -f build_type='workflow' >/dev/null
  pages_status="created"
}

if $configure_pages; then
  configure_pages_workflow_build
fi

echo
echo "Repository: $target_repo"
echo "GPG key ID: $gpg_key_id"
echo "Pages status: $pages_status"
echo "Secrets created or updated: ${#created_secrets[@]}"
for secret_name in "${created_secrets[@]}"; do
  echo "  - $secret_name"
done
echo "Secrets skipped: ${#skipped_secrets[@]}"
for secret_name in "${skipped_secrets[@]}"; do
  echo "  - $secret_name"
done