#!/usr/bin/env bash

set -euo pipefail

REPO_SLUG="CaseyID/Asylum"
GITHUB_API_URL="https://api.github.com/repos/${REPO_SLUG}/releases"
GITHUB_RELEASE_URL="https://github.com/${REPO_SLUG}/releases"
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
  --yes                 Run setup and doctor automatically
  --skip-setup          Do not run or prompt for setup
  --skip-doctor         Do not run asylum doctor
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

asylum_path_contains() {
  local candidate=$1
  case ":${PATH}:" in
    *":${candidate%/}:"*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
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
  ASSUME_YES=0
  SKIP_SETUP=0
  SKIP_DOCTOR=0
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
      --yes)
        ASSUME_YES=1
        shift
        ;;
      --skip-setup)
        SKIP_SETUP=1
        shift
        ;;
      --skip-doctor)
        SKIP_DOCTOR=1
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
  local api_payload
  api_payload="$(curl --fail --silent --location -H "Accept: application/vnd.github+json" "$GITHUB_API_URL/latest")"
  local tag
  tag="$(printf '%s\n' "$api_payload" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
  if [[ -z "$tag" ]]; then
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

asylum_verify_with_checksum_file() {
  local checksum_file=$1
  local archive_path=$2
  local archive_name=$3
  local hash_cmd
  hash_cmd="$(asylum_hash_command)"

  if [[ "$hash_cmd" == "none" ]]; then
    printf 'skipped\n'
    return 0
  fi

  local expected
  expected="$(awk -v f="$archive_name" '$2 == f || $2 == "*" f { print $1 }' "$checksum_file" | head -n1)"
  if [[ -z "$expected" ]]; then
    expected="$(awk '{print $1; exit}' "$checksum_file")"
  fi
  if [[ -z "$expected" ]]; then
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
  local sha_url="${GITHUB_RELEASE_URL}/download/${version}/${asset_name}.sha256"
  local checksum_file="${tmpdir}/checksums.txt"
  local direct_sha_file="${tmpdir}/${asset_name}.sha256"

  if asylum_download_quiet "$checksum_url" "$checksum_file"; then
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

  printf 'skipped\n'
  return 0
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

asylum_prompt_yes_no() {
  local question=$1
  local response
  printf '%s [Y/n] ' "$question"
  read -r response
  response="$(printf '%s' "$response" | tr '[:upper:]' '[:lower:]')"
  case "$response" in
    n|no)
      return 1
      ;;
    *)
      return 0
      ;;
  esac
}

asylum_run_setup_if_needed() {
  local binary_path=$1
  local interactive=$2

  if (( SKIP_SETUP )); then
    return 0
  fi

  if (( ASSUME_YES )); then
    asylum_step "Running asylum setup"
    ASYLUM_HOME="$ASYLUM_HOME" "$binary_path" setup
    return $?
  fi

  if (( interactive )); then
    if asylum_prompt_yes_no "Run asylum setup now?"; then
      asylum_step "Running asylum setup"
      ASYLUM_HOME="$ASYLUM_HOME" "$binary_path" setup
      return $?
    fi
    printf "Setup skipped. Run %sasylum setup%s later if needed.\n" "$COLOR_OK" "$COLOR_RESET"
  else
    printf "Interactive mode is unavailable. Run %sasylum setup%s when ready.\n" "$COLOR_OK" "$COLOR_RESET"
  fi
}

asylum_run_doctor_if_needed() {
  local binary_path=$1

  if (( SKIP_DOCTOR )); then
    return 0
  fi

  asylum_step "Running asylum doctor"
  ASYLUM_HOME="$ASYLUM_HOME" "$binary_path" doctor
}

asylum_detect_shell_rc() {
  local shell_name
  shell_name="${SHELL:-/bin/sh}"
  shell_name="${shell_name##*/}"
  case "$shell_name" in
    zsh)
      echo "${HOME}/.zshrc"
      return 0
      ;;
    bash)
      if [[ -f "${HOME}/.bashrc" ]]; then
        echo "${HOME}/.bashrc"
      elif [[ -f "${HOME}/.bash_profile" ]]; then
        echo "${HOME}/.bash_profile"
      elif [[ -f "${HOME}/.profile" ]]; then
        echo "${HOME}/.profile"
      else
        echo "${HOME}/.bashrc"
      fi
      return 0
      ;;
    *)
      if [[ -f "${HOME}/.profile" ]]; then
        echo "${HOME}/.profile"
      else
        echo "${HOME}/.bash_profile"
      fi
      return 0
      ;;
  esac
}

asylum_render_managed_path_block() {
  local install_dir=$1
  local marker="# Added by Asylum installer"

  cat <<EOF
$marker
if [[ ":\$PATH:" != *":${install_dir}:"* ]]; then
  export PATH="${install_dir}:\$PATH"
fi
EOF
}

asylum_apply_path_to_shell_rc() {
  local install_dir=$1
  local rc_file
  local marker="# Added by Asylum installer"
  local has_managed_block=0
  rc_file="$(asylum_detect_shell_rc)"

  if [[ -f "$rc_file" ]] && grep -Fxq "$marker" "$rc_file" 2>/dev/null; then
    has_managed_block=1
  fi

  if (( ! has_managed_block )) && asylum_path_contains "$install_dir"; then
    return 0
  fi

  if [[ ! -f "$rc_file" ]]; then
    touch "$rc_file"
  fi

  if (( has_managed_block )); then
    local tmp_rc
    local clean_rc
    tmp_rc="$(mktemp)"
    clean_rc="$(mktemp)"

    awk -v marker="$marker" '
      $0 == marker { in_block=1; next }
      in_block {
        if ($0 == "fi") {
          in_block=0
        }
        next
      }
      { print }
    ' "$rc_file" > "$tmp_rc"

    awk '
      { lines[NR]=$0 }
      END {
        end=NR
        while (end > 0 && lines[end] == "") {
          end--
        }
        for (i = 1; i <= end; i++) {
          print lines[i]
        }
      }
    ' "$tmp_rc" > "$clean_rc"

    cat "$clean_rc" > "$rc_file"
    if [[ -s "$rc_file" ]]; then
      printf '\n' >> "$rc_file"
    fi
    asylum_render_managed_path_block "$install_dir" >> "$rc_file"
    rm -f "$tmp_rc" "$clean_rc"
    return 0
  fi

  local tmp_rc
  tmp_rc="$(mktemp)"
  awk '
    { lines[NR]=$0 }
    END {
      end=NR
      while (end > 0 && lines[end] == "") {
        end--
      }
      for (i = 1; i <= end; i++) {
        print lines[i]
      }
    }
  ' "$rc_file" > "$tmp_rc"

  cat "$tmp_rc" > "$rc_file"
  if [[ -s "$rc_file" ]]; then
    printf '\n' >> "$rc_file"
  fi
  asylum_render_managed_path_block "$install_dir" >> "$rc_file"
  rm -f "$tmp_rc"
}

asylum_shell_rc_has_managed_path_block() {
  local rc_file
  local marker="# Added by Asylum installer"
  rc_file="$(asylum_detect_shell_rc)"
  [[ -f "$rc_file" ]] && grep -Fxq "$marker" "$rc_file" 2>/dev/null
}

asylum_print_path_instructions() {
  local install_dir=$1
  local rc_file
  rc_file="$(asylum_detect_shell_rc)"
  printf 'Asylum installed but not yet on PATH for this shell.\n'
  printf 'Add this line to %s and restart your shell:\n' "$rc_file"
  printf '  export PATH="%s:$PATH"\n' "$install_dir"
}

asylum_next_steps() {
  printf '\n'
  asylum_colorize "$COLOR_LABEL" "Next steps:"
  printf '\n'
  printf '  asylum setup\n'
  printf '  asylum doctor\n'
  printf '  asylum\n'
  printf '\n'
  if asylum_shell_rc_has_managed_path_block; then
    if ! asylum_apply_path_to_shell_rc "${INSTALL_DIR}"; then
      asylum_print_path_instructions "${INSTALL_DIR}"
    fi
    return 0
  fi
  if ! asylum_path_contains "${INSTALL_DIR}"; then
    if [[ -t 0 && "${ASSUME_YES}" -eq 1 ]]; then
      if ! asylum_apply_path_to_shell_rc "${INSTALL_DIR}"; then
        asylum_print_path_instructions "${INSTALL_DIR}"
      fi
    else
      asylum_print_path_instructions "${INSTALL_DIR}"
    fi
  fi
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

  asylum_step "Creating Asylum home at ${ASYLUM_HOME}"
  mkdir -p "${ASYLUM_HOME}" "${ASYLUM_HOME}/logs" "${ASYLUM_HOME}/run"

  if (( SKIP_SETUP )); then
    printf "Setup skipped by flag: %s--skip-setup%s\n" "$COLOR_WARN" "$COLOR_RESET"
  fi

  local interactive=0
  if [[ -t 0 ]]; then
    interactive=1
  fi

  asylum_run_setup_if_needed "${INSTALL_DIR}/asylum" "$interactive"
  asylum_run_doctor_if_needed "${INSTALL_DIR}/asylum"
  asylum_next_steps
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  asylum_main "$@"
fi
