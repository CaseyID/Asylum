#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

VERSION=""
OUTPUT_DIR=""
TARGETS="darwin-arm64,darwin-x86_64,linux-arm64,linux-x86_64"
SKIP_NPM_CI=0
SKIP_COCKPIT_BUILD=0
DOCKER_IMAGE="${ASYLUM_RELEASE_DOCKER_IMAGE:-rust:1-bookworm}"

print_help() {
  cat <<'USAGE'
Usage: scripts/build-release-artifacts.sh [options]

Build local release archives for Asylum.

Options:
  --version <semver|tag>        Version to package. Defaults to Cargo.toml workspace version.
  --output-dir <path>           Output directory. Defaults to dist/release/v<version>.
  --targets <list>              Comma-separated assets to build.
                               Default: darwin-arm64,darwin-x86_64,linux-arm64,linux-x86_64
  --skip-npm-ci                 Do not run npm ci before building Cockpit.
  --skip-cockpit-build          Reuse existing cockpit/dist.
  --docker-image <image>        Rust Docker image for Linux builds. Default: rust:1-bookworm.
  --help                        Show this help.

Linux artifacts are built locally with Docker, not GitHub Actions. The
linux-x86_64 artifact is cross-compiled from a native arm64 Linux container on
Apple Silicon Macs so the compiler does not run under amd64 emulation.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:?missing value for --version}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:?missing value for --output-dir}"
      shift 2
      ;;
    --targets)
      TARGETS="${2:?missing value for --targets}"
      shift 2
      ;;
    --skip-npm-ci)
      SKIP_NPM_CI=1
      shift
      ;;
    --skip-cockpit-build)
      SKIP_COCKPIT_BUILD=1
      shift
      ;;
    --docker-image)
      DOCKER_IMAGE="${2:?missing value for --docker-image}"
      shift 2
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

workspace_version() {
  sed -n 's/^version = "\([^"]*\)"/\1/p' "${REPO_ROOT}/Cargo.toml" | head -n1
}

normalize_version() {
  local version=$1
  printf '%s\n' "${version#v}"
}

tag_for_version() {
  local version=$1
  printf 'v%s\n' "$(normalize_version "$version")"
}

hash_file() {
  local path=$1
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path"
  else
    shasum -a 256 "$path"
  fi
}

contains_target() {
  local target=$1
  case ",${TARGETS}," in
    *",${target},"*) return 0 ;;
    *) return 1 ;;
  esac
}

package_binary() {
  local binary=$1
  local asset_name=$2
  local output_dir=$3
  local tmpdir
  tmpdir="$(mktemp -d)"
  cp "$binary" "${tmpdir}/asylum"
  chmod 0755 "${tmpdir}/asylum"
  tar -czf "${output_dir}/${asset_name}" -C "$tmpdir" asylum
  rm -rf "$tmpdir"

  local archive_contents
  archive_contents="$(tar -tzf "${output_dir}/${asset_name}")"
  if [[ "$archive_contents" != "asylum" ]]; then
    echo "Archive ${asset_name} does not contain exactly one top-level asylum binary" >&2
    printf '%s\n' "$archive_contents" >&2
    exit 1
  fi
}

build_macos() {
  local release_name=$1
  local rust_target=$2
  local output_dir=$3
  rustup target add "$rust_target"
  cargo build --release -p asylum --target "$rust_target"
  package_binary "${REPO_ROOT}/target/${rust_target}/release/asylum" "asylum-${release_name}.tar.gz" "$output_dir"
}

require_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    echo "Docker is required for local Linux release builds on macOS." >&2
    exit 1
  fi
}

build_linux_native() {
  local release_name=$1
  local platform=$2
  local output_dir=$3
  local scratch_binary="${output_dir}/.asylum-${release_name}"

  require_docker

  # M17: run as the host user so bind-mounted repo files are not modified as root.
  docker run --rm \
    --platform "$platform" \
    --user "$(id -u):$(id -g)" \
    -e HOME=/tmp/cargo-home \
    -e OUTPUT_BIN="/out/.asylum-${release_name}" \
    -e PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    -v "${REPO_ROOT}:/work" \
    -v "${output_dir}:/out" \
    -w /work \
    "$DOCKER_IMAGE" \
    bash -c 'set -euo pipefail
mkdir -p /tmp/cargo-home
CARGO_TARGET_DIR=/tmp/asylum-target cargo build --release -p asylum
cp /tmp/asylum-target/release/asylum "$OUTPUT_BIN"'

  package_binary "$scratch_binary" "asylum-${release_name}.tar.gz" "$output_dir"
  rm -f "$scratch_binary"
}

build_linux_x86_64() {
  local release_name=$1
  local output_dir=$2
  local scratch_binary="${output_dir}/.asylum-${release_name}"

  require_docker

  # M17: run as the host user so bind-mounted repo files are not modified as root.
  docker run --rm \
    --platform linux/arm64 \
    --user "$(id -u):$(id -g)" \
    -e HOME=/tmp/cargo-home \
    -e OUTPUT_BIN="/out/.asylum-${release_name}" \
    -e PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    -v "${REPO_ROOT}:/work" \
    -v "${output_dir}:/out" \
    -w /work \
    "$DOCKER_IMAGE" \
    bash -c 'set -euo pipefail
mkdir -p /tmp/cargo-home
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu libc6-dev-amd64-cross
rustup target add x86_64-unknown-linux-gnu
export CARGO_TARGET_DIR=/tmp/asylum-target
export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
export PKG_CONFIG_ALLOW_CROSS=1
cargo build --release -p asylum --target x86_64-unknown-linux-gnu
cp /tmp/asylum-target/x86_64-unknown-linux-gnu/release/asylum "$OUTPUT_BIN"'

  package_binary "$scratch_binary" "asylum-${release_name}.tar.gz" "$output_dir"
  rm -f "$scratch_binary"
}

main() {
  cd "$REPO_ROOT"

  if [[ -z "$VERSION" ]]; then
    VERSION="$(workspace_version)"
  fi
  VERSION="$(normalize_version "$VERSION")"
  local tag
  tag="$(tag_for_version "$VERSION")"

  if [[ -z "$OUTPUT_DIR" ]]; then
    OUTPUT_DIR="${REPO_ROOT}/dist/release/${tag}"
  fi
  mkdir -p "$OUTPUT_DIR"

  if (( ! SKIP_NPM_CI )); then
    npm --prefix cockpit ci
  fi
  if (( ! SKIP_COCKPIT_BUILD )); then
    npm --prefix cockpit run build
  fi

  if [[ ! -f "${REPO_ROOT}/cockpit/dist/index.html" ]]; then
    echo "cockpit/dist/index.html is missing; run npm --prefix cockpit run build" >&2
    exit 1
  fi

  if contains_target "darwin-arm64"; then
    build_macos "darwin-arm64" "aarch64-apple-darwin" "$OUTPUT_DIR"
  fi
  if contains_target "darwin-x86_64"; then
    build_macos "darwin-x86_64" "x86_64-apple-darwin" "$OUTPUT_DIR"
  fi
  if contains_target "linux-arm64"; then
    build_linux_native "linux-arm64" "linux/arm64" "$OUTPUT_DIR"
  fi
  if contains_target "linux-x86_64"; then
    build_linux_x86_64 "linux-x86_64" "$OUTPUT_DIR"
  fi

  (
    cd "$OUTPUT_DIR"
    rm -f checksums.txt
    for archive in asylum-*.tar.gz; do
      hash_file "$archive" >> checksums.txt
    done
  )

  printf 'Release artifacts written to %s\n' "$OUTPUT_DIR"
}

main "$@"
