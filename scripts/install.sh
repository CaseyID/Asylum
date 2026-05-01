#!/usr/bin/env bash

set -euo pipefail

REPO_SLUG="${ASYLUM_REPO_SLUG:-CaseyID/Asylum}"
GITHUB_API_URL="https://api.github.com/repos/${REPO_SLUG}/releases"
GITHUB_RELEASE_URL="https://github.com/${REPO_SLUG}/releases"

# ASYLUM_RELEASE_PUBKEY: minisign public key (single line, "RWxxxx..." format).
# When non-empty (either the embedded constant below or the env var override),
# the installer requires a valid checksums.txt.minisig signature alongside the
# checksum file. Until the maintainer publishes a release-signing key, the
# constant is empty: the installer prints a "warning: checksum file is
# unsigned" line and proceeds with checksum-only verification. Once a key is
# published, paste it into ASYLUM_RELEASE_PUBKEY_DEFAULT below and every
# existing installer download immediately upgrades to verified-mode.
ASYLUM_RELEASE_PUBKEY_DEFAULT="RWSEo4O44NHaVBVl1XI5eUk7JhvJBxyevCVtAwld6t5J0M5cIi4pS+xu"
COLOR_LABEL=""
COLOR_OK=""
COLOR_WARN=""
COLOR_ERR=""
COLOR_RESET=""
ASYLUM_INSTALLER_TMPDIR=""

print_help() {
  cat <<'EOF'
Asylum binary installer

Usage:
  bash scripts/install.sh [options]

Options:
  --help                Show this help and exit
  --version <tag>       Install a specific release tag
  --install-dir <path>  Directory to install asylum binary (default: ~/.local/bin)
  --asylum-home <path>  Asylum home path (default: ~/.asylum)
  --skip-setup          Do not run `asylum setup` after install
  --no-color            Disable color output

Examples:
  curl -fsSL https://raw.githubusercontent.com/CaseyID/Asylum/main/scripts/install.sh | bash
  curl -fsSL https://raw.githubusercontent.com/CaseyID/Asylum/main/scripts/install.sh | bash -- --version v0.1.1
EOF
}

asylum_color_init() {
  if (( NO_COLOR )); then
    COLOR_LABEL=""
    COLOR_OK=""
    COLOR_WARN=""
    COLOR_ERR=""
    COLOR_RESET=""
    return
  fi

  COLOR_LABEL=$'\033[36m'
  COLOR_OK=$'\033[32m'
  COLOR_WARN=$'\033[33m'
  COLOR_ERR=$'\033[31m'
  COLOR_RESET=$'\033[0m'
}

asylum_colorize() {
  local color=$1
  shift
  printf '%s' "${color}${*}${COLOR_RESET}"
}

asylum_banner() {
  printf '\n'
  cat <<'EOF'
 _____                     _                 
/ ____|                   | |                
(___   ___  ___ _ __ ___| |__   ___ _ __   
 \___ \ / _ \/ _ \ __/ | | / __| "_ \  
  ____) |  __/  __/ |  | | |_| (__| | | |
 |_____/ \___|\___|_|  |_|\__,_|\___|_| |_|
EOF
  printf '%s' "$COLOR_RESET"
  printf '%s\n' "Asylum installer"
}

asylum_step() {
  local msg=$1
  asylum_colorize "$COLOR_OK" "=> "
  printf '%s\n' "$msg"
}

asylum_warn() {
  local msg=$1
  asylum_colorize "$COLOR_WARN" "!! "
  printf '%s\n' "$msg"
}

asylum_error() {
  local msg=$1
  printf '%s\n' "${COLOR_ERR}!! ${msg}${COLOR_RESET}" >&2
}

asylum_normalize_os() {
  local raw_os=${1:-$(uname -s)}
  raw_os="$(printf '%s' "$raw_os" | tr '[:upper:]' '[:lower:]')"
  case "$raw_os" in
    darwin|macos|mac* )
      printf "darwin"
      ;;
    linux* )
      printf "linux"
      ;;
    * )
      printf "unsupported"
      ;;
  esac
}

asylum_normalize_arch() {
  local raw_arch=${1:-$(uname -m)}
  raw_arch="$(printf '%s' "$raw_arch" | tr '[:upper:]' '[:lower:]')"
  case "$raw_arch" in
    arm64|aarch64 )
      printf "arm64"
      ;;
    x86_64|amd64|x64|x86-64 )
      printf "x86_64"
      ;;
    * )
      printf "unsupported"
      ;;
  esac
}

asylum_archive_name() {
  local os=$1
  local arch=$2
  printf 'asylum-%s-%s.tar.gz' "$os" "$arch"
}

asylum_release_url() {
  local version=$1
  local asset_name=$2
  printf '%s/download/%s/%s' "$GITHUB_RELEASE_URL" "$version" "$asset_name"
}

asylum_parse_args() {
  VERSION=""
  INSTALL_DIR="${HOME}/.local/bin"
  ASYLUM_HOME="${HOME}/.asylum"
  SKIP_SETUP=0
  NO_COLOR=0
  SHOW_HELP=0

  if [[ -t 1 ]]; then
    NO_COLOR=0
  else
    NO_COLOR=1
  fi

  while [[ $# -gt 0 ]]; do
    case $1 in
      --help)
        SHOW_HELP=1
        return 0
        ;;
      --version)
        if [[ $# -lt 2 ]]; then
          asylum_error "Missing value for --version"
          return 2
        fi
        VERSION=$2
        shift 2
        ;;
      --install-dir)
        if [[ $# -lt 2 ]]; then
          asylum_error "Missing value for --install-dir"
          return 2
        fi
        INSTALL_DIR=$2
        shift 2
        ;;
      --asylum-home)
        if [[ $# -lt 2 ]]; then
          asylum_error "Missing value for --asylum-home"
          return 2
        fi
        ASYLUM_HOME=$2
        shift 2
        ;;
      --skip-setup)
        SKIP_SETUP=1
        shift
        ;;
      --skip-doctor|--yes)
        # Accepted-but-no-op for back-compat with `asylum update` from
        # v0.1.2 and earlier. The installer no longer runs doctor or
        # prompts, so these flags have nothing to do.
        shift
        ;;
      --no-color)
        NO_COLOR=1
        shift
        ;;
      *)
        asylum_error "Unknown option: $1"
        return 2
        ;;
    esac
  done
  return 0
}

asylum_set_tmpdir() {
  ASYLUM_INSTALLER_TMPDIR=$1
}

asylum_cleanup_tmpdir() {
  if [[ -n "${ASYLUM_INSTALLER_TMPDIR:-}" && -d "$ASYLUM_INSTALLER_TMPDIR" ]]; then
    rm -rf -- "$ASYLUM_INSTALLER_TMPDIR"
  fi
  ASYLUM_INSTALLER_TMPDIR=""
}

asylum_is_supported() {
  local os=$1
  local arch=$2
  [[ "$os" != "unsupported" && "$arch" != "unsupported" ]]
}

asylum_download() {
  local url=$1
  local output=$2
  curl --fail --silent --location --show-error "$url" -o "$output"
}

asylum_download_quiet() {
  local url=$1
  local output=$2
  curl --fail --silent --location "$url" -o "$output"
}

asylum_fetch_latest_release() {
  # Resolve via the /releases/latest redirect rather than the authenticated API
  # so rate-limit quota is not consumed and sed JSON parsing is not needed (L18).
  local resolved_url
  resolved_url="$(curl --fail --silent --head --location \
    -w '%{url_effective}' -o /dev/null \
    "${GITHUB_RELEASE_URL}/latest" 2>/dev/null)" || true
  local tag
  tag="$(printf '%s\n' "$resolved_url" | sed 's|.*/tag/||')"
  if [[ -z "$tag" ]] || [[ "$tag" == "$resolved_url" ]]; then
    asylum_error "Could not resolve latest release tag from GitHub"
    return 1
  fi
  printf '%s\n' "$tag"
}

asylum_hash_command() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf "sha256sum"
  elif command -v shasum >/dev/null 2>&1; then
    printf "shasum"
  else
    printf "none"
  fi
}

asylum_release_pubkey() {
  # Env override wins over the embedded constant.
  if [[ -n "${ASYLUM_RELEASE_PUBKEY:-}" ]]; then
    printf '%s' "$ASYLUM_RELEASE_PUBKEY"
    return
  fi
  printf '%s' "$ASYLUM_RELEASE_PUBKEY_DEFAULT"
}

# Verify checksums.txt with minisign if we can; emit a warning otherwise.
# Returns 0 on (signature verified) OR (unsigned, proceeding with hash-only).
# Returns 1 only on an explicit signature failure.
asylum_verify_checksum_signature() {
  local checksum_file=$1
  local sig_file=$2
  local pubkey
  pubkey="$(asylum_release_pubkey)"

  if [[ ! -f "$sig_file" ]]; then
    asylum_warn "warning: checksum file is unsigned (no checksums.txt.minisig found)"
    return 0
  fi

  if ! command -v minisign >/dev/null 2>&1; then
    asylum_warn "warning: checksum file is unsigned (minisign not on PATH; install minisign to enable signature verification)"
    return 0
  fi

  if [[ -z "$pubkey" ]]; then
    asylum_warn "warning: checksum file is unsigned (no Asylum release pubkey configured; set ASYLUM_RELEASE_PUBKEY or wait for a signed release)"
    return 0
  fi

  if minisign -V -P "$pubkey" -m "$checksum_file" -x "$sig_file" >/dev/null 2>&1; then
    printf 'Signature: %s\n' "$(asylum_colorize "$COLOR_OK" "verified")"
    return 0
  fi

  asylum_error "checksums.txt signature failed verification with configured pubkey"
  return 1
}

asylum_verify_with_checksum_file() {
  local checksum_file=$1
  local archive_path=$2
  local archive_name=$3
  local hash_cmd
  hash_cmd="$(asylum_hash_command)"

  if [[ "$hash_cmd" == "none" ]]; then
    if [[ "${ASYLUM_SKIP_CHECKSUM:-0}" == "1" ]]; then
      asylum_warn "ASYLUM_SKIP_CHECKSUM=1 set; downloading without integrity verification."
      asylum_warn "This is unsafe: the binary you install may be tampered with."
      printf 'skipped\n'
      return 0
    fi
    asylum_error "Cannot verify download integrity: install one of sha256sum or shasum and re-run."
    asylum_error "To bypass (NOT RECOMMENDED), re-run with ASYLUM_SKIP_CHECKSUM=1."
    return 1
  fi

  # M14: hard-fail when the archive isn't listed — never fall back to another entry.
  local expected
  expected="$(awk -v f="$archive_name" '$2 == f || $2 == ("*" f) { print $1 }' "$checksum_file" | head -n1)"
  if [[ -z "$expected" ]]; then
    asylum_error "Checksum entry for ${archive_name} not found in checksums.txt — aborting."
    return 1
  fi

  local actual
  if [[ "$hash_cmd" == "sha256sum" ]]; then
    actual="$(sha256sum "$archive_path" | awk '{print $1}')"
  else
    actual="$(shasum -a 256 "$archive_path" | awk '{print $1}')"
  fi

  if [[ "$expected" == "$actual" ]]; then
    printf 'verified\n'
    return 0
  fi

  asylum_error "Checksum mismatch for ${archive_name}"
  printf 'Expected: %s\nFound:    %s\n' "$expected" "$actual" >&2
  return 1
}

asylum_verify_archive() {
  local version=$1
  local asset_name=$2
  local archive_path=$3
  local tmpdir=$4
  local checksum_url="${GITHUB_RELEASE_URL}/download/${version}/checksums.txt"
  local sig_url="${GITHUB_RELEASE_URL}/download/${version}/checksums.txt.minisig"
  local sha_url="${GITHUB_RELEASE_URL}/download/${version}/${asset_name}.sha256"
  local checksum_file="${tmpdir}/checksums.txt"
  local sig_file="${tmpdir}/checksums.txt.minisig"
  local direct_sha_file="${tmpdir}/${asset_name}.sha256"

  if asylum_download_quiet "$checksum_url" "$checksum_file"; then
    # Best-effort signature fetch. Absent signature is acceptable today;
    # mandatory once a pubkey is published. Errors here are silent — the
    # signature verifier handles "no sig file" with a warning.
    asylum_download_quiet "$sig_url" "$sig_file" || rm -f "$sig_file"
    if ! asylum_verify_checksum_signature "$checksum_file" "$sig_file" >&2; then
      return 1
    fi
    local checksum_status
    if ! checksum_status="$(asylum_verify_with_checksum_file "$checksum_file" "$archive_path" "$asset_name")"; then
      return 1
    fi
    printf '%s\n' "$checksum_status"
    if [[ "$checksum_status" == "verified" || "$checksum_status" == "skipped" ]]; then
      return 0
    fi
    return 1
  fi

  if asylum_download_quiet "$sha_url" "$direct_sha_file"; then
    local direct_checksum_status
    if ! direct_checksum_status="$(asylum_verify_with_checksum_file "$direct_sha_file" "$archive_path" "$asset_name")"; then
      return 1
    fi
    printf '%s\n' "$direct_checksum_status"
    if [[ "$direct_checksum_status" == "verified" || "$direct_checksum_status" == "skipped" ]]; then
      return 0
    fi
    return 1
  fi

  # Neither checksum file nor per-archive sha256 was reachable. This used to
  # be a silent skip. Treat as the same "no integrity tool / no integrity
  # data" condition: hard fail unless ASYLUM_SKIP_CHECKSUM=1.
  if [[ "${ASYLUM_SKIP_CHECKSUM:-0}" == "1" ]]; then
    asylum_warn "ASYLUM_SKIP_CHECKSUM=1 set; no checksum data was available, proceeding without verification."
    printf 'skipped\n'
    return 0
  fi
  asylum_error "Cannot verify download integrity: no checksum data could be fetched from the release."
  asylum_error "To bypass (NOT RECOMMENDED), re-run with ASYLUM_SKIP_CHECKSUM=1."
  return 1
}

asylum_extract_binary() {
  local archive_path=$1
  local extract_dir=$2
  local archive_basename archive_contents entry sanitized_entry entry_count archive_entry entry_info entry_type
  local -a matched=()
  archive_basename="$(basename "$archive_path")"

  mkdir -p "$extract_dir"

  if ! archive_contents="$(tar -tzf "$archive_path")"; then
    asylum_error "Unable to read archive contents: ${archive_basename}"
    return 1
  fi

  while IFS= read -r entry; do
    if [[ -z "$entry" ]]; then
      continue
    fi
    sanitized_entry="${entry#./}"
    if [[ -z "$sanitized_entry" ]]; then
      continue
    fi

    if [[ "$sanitized_entry" == /* ]] || [[ "$sanitized_entry" == "../"* ]] || [[ "$sanitized_entry" == *"/../"* ]] || [[ "$sanitized_entry" == *"/.." ]] ; then
      asylum_error "Archive contains unsafe path: ${entry}"
      return 1
    fi

    case "$sanitized_entry" in
      asylum|*/asylum)
        ;;
      *)
        continue
        ;;
    esac
    if [[ "$sanitized_entry" == */*/asylum ]]; then
      continue
    fi

    entry_info="$(tar -tvf "$archive_path" -- "$sanitized_entry" 2>/dev/null | head -n1)"
    if [[ -z "$entry_info" ]]; then
      continue
    fi
    entry_type="${entry_info:0:1}"
    # M15: reject hardlinks (type 'h') and any non-regular-file type.
    # Note: GNU tar reports the first instance of a hard-linked inode as '-';
    # subsequent hard links appear as 'h'. We also reject after extraction via
    # nlink check so that first-instance hardlinks are caught.
    if [[ "$entry_type" == "h" ]]; then
      asylum_error "Archive contains a hardlink for asylum binary — refusing to install."
      return 1
    fi
    if [[ "$entry_type" != "-" ]]; then
      continue
    fi
    matched+=("$sanitized_entry")
  done < <(printf '%s\n' "$archive_contents")

  entry_count="${#matched[@]}"
  if [[ "$entry_count" -eq 0 ]]; then
    asylum_error "Archive is missing valid asylum binary: ${archive_basename}"
    return 1
  fi
  if [[ "$entry_count" -ne 1 ]]; then
    asylum_error "Archive contains multiple asylum binaries: ${archive_basename}"
    return 1
  fi

  archive_entry="${matched[0]}"

  tar -xzf "$archive_path" -C "$extract_dir" "$archive_entry"
  if [[ ! -f "${extract_dir}/${archive_entry}" ]]; then
    asylum_error "Extracted asylum binary not found: ${archive_entry}"
    return 1
  fi
  if [[ ! -x "${extract_dir}/${archive_entry}" ]]; then
    asylum_error "Extracted asylum is not executable: ${archive_entry}"
    return 1
  fi
  # M15: post-extraction hardlink check — catches first-instance hardlinks that
  # GNU tar reports as '-' in the listing but which share an inode with another path.
  local nlink
  nlink="$(stat -c '%h' "${extract_dir}/${archive_entry}" 2>/dev/null || stat -f '%l' "${extract_dir}/${archive_entry}" 2>/dev/null || echo 1)"
  if [[ "$nlink" -gt 1 ]] 2>/dev/null; then
    asylum_error "Extracted asylum binary is a hardlink (nlink=${nlink}) — refusing to install."
    return 1
  fi

  printf '%s\n' "${extract_dir}/${archive_entry}"
}

asylum_install_binary() {
  local source_path=$1
  local target_dir=$2
  local target_binary temp_binary

  mkdir -p "$target_dir"
  target_binary="${target_dir}/asylum"
  temp_binary="$(mktemp "${target_dir}/.asylum-install-XXXXXX")"

  cp -- "$source_path" "$temp_binary" || {
    rm -f "$temp_binary"
    return 1
  }
  chmod 0755 "$temp_binary" || {
    rm -f "$temp_binary"
    return 1
  }
  mv -f "$temp_binary" "$target_binary" || {
    rm -f "$temp_binary"
    return 1
  }
}


asylum_main() {
  local parse_result=0
  set +e
  asylum_parse_args "$@"
  parse_result=$?
  set -e
  if (( parse_result != 0 )); then
    print_help
    return "$parse_result"
  fi

  if (( SHOW_HELP )); then
    print_help
    return 0
  fi

  asylum_color_init
  asylum_banner
  asylum_step "Preparing Asylum binary installer"

  if [[ -z "${VERSION}" ]]; then
    asylum_step "Resolving latest release"
    VERSION="$(asylum_fetch_latest_release)"
    asylum_step "Latest release: ${VERSION}"
  fi

  local os arch archive asset_url extracted_dir archive_path checksum_status
  os="$(asylum_normalize_os "$(uname -s)")"
  arch="$(asylum_normalize_arch "$(uname -m)")"
  if ! asylum_is_supported "$os" "$arch"; then
    asylum_error "Unsupported platform: $(uname -s) / $(uname -m)"
    return 1
  fi
  archive="$(asylum_archive_name "$os" "$arch")"
  asset_url="$(asylum_release_url "$VERSION" "$archive")"

  asylum_set_tmpdir "$(mktemp -d)"
  trap 'asylum_cleanup_tmpdir' EXIT
  archive_path="${ASYLUM_INSTALLER_TMPDIR}/${archive}"
  extracted_dir="${ASYLUM_INSTALLER_TMPDIR}/extracted"

  asylum_step "Downloading ${archive} from ${VERSION}"
  asylum_download "$asset_url" "$archive_path"

  if ! checksum_status="$(asylum_verify_archive "$VERSION" "$archive" "$archive_path" "$ASYLUM_INSTALLER_TMPDIR")"; then
    return 1
  fi
  if [[ "$checksum_status" == "verified" ]]; then
    printf 'Checksum: %s\n' "$(asylum_colorize "$COLOR_OK" "verified")"
  elif [[ "$checksum_status" == "skipped" ]]; then
    asylum_warn "Checksum verification skipped (checksum file not available)."
  else
    asylum_warn "Checksum verification status unknown."
  fi

  asylum_step "Extracting archive"
  local extracted_binary
  extracted_binary="$(asylum_extract_binary "$archive_path" "$extracted_dir")"

  asylum_step "Installing asylum to ${INSTALL_DIR}"
  asylum_install_binary "$extracted_binary" "$INSTALL_DIR"

  if (( SKIP_SETUP )); then
    printf "Setup skipped by flag: %s--skip-setup%s\n" "$COLOR_WARN" "$COLOR_RESET"
    printf "Run %sasylum setup%s when ready.\n" "$COLOR_OK" "$COLOR_RESET"
    return 0
  fi

  asylum_step "Handing off to asylum setup"
  export ASYLUM_HOME
  exec "${INSTALL_DIR}/asylum" setup
}

INSTALLER_SCRIPT_SOURCE="${BASH_SOURCE[0]:-}"
if [[ -z "$INSTALLER_SCRIPT_SOURCE" || "$INSTALLER_SCRIPT_SOURCE" == "${0}" ]]; then
  asylum_main "$@"
fi
