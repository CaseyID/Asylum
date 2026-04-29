#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

INSTALLER="https://raw.githubusercontent.com/CaseyID/Asylum/main/scripts/install.sh"
VERSION=""
EXPECTED_VERSION=""
INSTALL_DIR=""
ASYLUM_HOME=""
KEEP=0
RUN_UPDATE=1
WORK_DIR=""

print_help() {
  cat <<'USAGE'
Usage: scripts/test-release-install.sh [options]

Exercise the real release installer against an isolated install directory.

Options:
  --version <tag>              Release tag to install, for example v0.1.1.
  --expected-version <semver>  Expected `asylum --version` semver.
                               Defaults to --version without the leading v.
  --installer <path|url>       Installer script path or URL.
                               Defaults to the public installer on main.
  --install-dir <path>         Install directory. Defaults to a temp dir.
  --asylum-home <path>         ASYLUM_HOME. Defaults to a temp dir.
  --skip-update                Do not exercise `asylum update`.
  --keep                       Keep temporary directories after the test.
  --help                       Show this help.
USAGE
}

workspace_version() {
  sed -n 's/^version = "\([^"]*\)"/\1/p' "${REPO_ROOT}/Cargo.toml" | head -n1
}

normalize_version() {
  local version=$1
  printf '%s\n' "${version#v}"
}

cleanup() {
  if (( KEEP )); then
    if [[ -n "$WORK_DIR" ]]; then
      printf 'Kept test workspace: %s\n' "$WORK_DIR"
    fi
    return
  fi
  if [[ -n "$WORK_DIR" && -d "$WORK_DIR" ]]; then
    rm -rf "$WORK_DIR"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:?missing value for --version}"
      shift 2
      ;;
    --expected-version)
      EXPECTED_VERSION="${2:?missing value for --expected-version}"
      shift 2
      ;;
    --installer)
      INSTALLER="${2:?missing value for --installer}"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="${2:?missing value for --install-dir}"
      shift 2
      ;;
    --asylum-home)
      ASYLUM_HOME="${2:?missing value for --asylum-home}"
      shift 2
      ;;
    --skip-update)
      RUN_UPDATE=0
      shift
      ;;
    --keep)
      KEEP=1
      shift
      ;;
    --help)
      print_help
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      print_help >&2
      exit 2
      ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  VERSION="v$(workspace_version)"
fi
if [[ -z "$EXPECTED_VERSION" ]]; then
  EXPECTED_VERSION="$(normalize_version "$VERSION")"
fi

WORK_DIR="$(mktemp -d)"
trap cleanup EXIT

if [[ -z "$INSTALL_DIR" ]]; then
  INSTALL_DIR="${WORK_DIR}/bin"
fi
if [[ -z "$ASYLUM_HOME" ]]; then
  ASYLUM_HOME="${WORK_DIR}/home"
fi

run_installer() {
  local -a args=(
    --version "$VERSION"
    --install-dir "$INSTALL_DIR"
    --asylum-home "$ASYLUM_HOME"
    --skip-setup
    --skip-doctor
    --no-color
  )

  if [[ -f "$INSTALLER" ]]; then
    bash "$INSTALLER" "${args[@]}"
  else
    curl -fsSL "$INSTALLER" | bash -s -- "${args[@]}"
  fi
}

assert_version() {
  local actual expected
  actual="$("${INSTALL_DIR}/asylum" --version)"
  expected="asylum ${EXPECTED_VERSION}"
  if [[ "$actual" != "$expected" ]]; then
    printf 'Unexpected version: expected "%s", got "%s"\n' "$expected" "$actual" >&2
    exit 1
  fi
}

printf 'Installing %s with %s\n' "$VERSION" "$INSTALLER"
run_installer

if [[ ! -x "${INSTALL_DIR}/asylum" ]]; then
  printf 'Installed binary is missing or not executable: %s\n' "${INSTALL_DIR}/asylum" >&2
  exit 1
fi

export PATH="${INSTALL_DIR}:${PATH}"
assert_version

ASYLUM_HOME="$ASYLUM_HOME" "${INSTALL_DIR}/asylum" setup
ASYLUM_HOME="$ASYLUM_HOME" "${INSTALL_DIR}/asylum" doctor --verbose

if (( RUN_UPDATE )); then
  ASYLUM_HOME="$ASYLUM_HOME" "${INSTALL_DIR}/asylum" update --version "$VERSION"
  assert_version
fi

printf 'Release install smoke passed for %s\n' "$VERSION"
