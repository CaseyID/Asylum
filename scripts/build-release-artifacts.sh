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
# rust-cross image bundles cargo-zigbuild + zig + the macOS SDK. Used on Linux
# hosts to cross-build darwin-arm64, darwin-x86_64, and linux-arm64 without
# QEMU, osxcross, or any apt installs beyond Docker itself.
# Pinned to a digest (not :latest) for release reproducibility. To update,
# pull `ghcr.io/rust-cross/cargo-zigbuild:latest`, grab the new digest from
# `docker images --digests`, and replace below. The semver-tagged images
# (0.16.x, 0.17.1) lag behind and ship Rust too old for Cargo.lock v4.
ZIGBUILD_IMAGE="${ASYLUM_RELEASE_ZIGBUILD_IMAGE:-ghcr.io/rust-cross/cargo-zigbuild@sha256:b66e2a5063921aca74fc53248d75d187b7499fe1e076d78eb7d87ab1dbc52f6a}"

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

Releases are built locally (no GitHub Actions). Both Apple Silicon Macs and
x86_64 Linux hosts can produce all four archives — same matrix, different
mechanisms:

  macOS Apple Silicon:
    - darwin-arm64: native cargo
    - darwin-x86_64: rustup cross
    - linux-arm64: native Docker (--platform linux/arm64)
    - linux-x86_64: cross-compiled inside an arm64 Linux container

  Linux x86_64:
    - linux-x86_64: native cargo (no Docker)
    - darwin-arm64, darwin-x86_64, linux-arm64: cross-compiled in the
      ghcr.io/rust-cross/cargo-zigbuild Docker image (zig as linker; macOS
      SDK baked into the image). No QEMU, no osxcross, no extra apt installs.
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
  # Ensure tmpdir is removed on exit, error, or signal (L17). The :- guard
  # prevents the EXIT trap from referencing an unset local after the function
  # returns under `set -u`.
  trap 'rm -rf "${tmpdir:-}"' EXIT
  cp "$binary" "${tmpdir}/asylum"
  chmod 0755 "${tmpdir}/asylum"
  tar -czf "${output_dir}/${asset_name}" -C "$tmpdir" asylum
  rm -rf "$tmpdir"
  trap - EXIT

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

# Cross-compile via rust-cross/cargo-zigbuild Docker image. Handles
# darwin-arm64, darwin-x86_64, and linux-arm64 from any Linux/x86_64 host.
# The image bundles zig (the linker), cargo-zigbuild, the Rust toolchain,
# and the macOS SDK — no host-side install beyond Docker.
build_via_zigbuild() {
  local release_name=$1
  local rust_target=$2
  local output_dir=$3
  local scratch_binary="${output_dir}/.asylum-${release_name}"

  require_docker

  docker run --rm \
    --user "$(id -u):$(id -g)" \
    -e HOME=/tmp/cargo-home \
    -e OUTPUT_BIN="/out/.asylum-${release_name}" \
    -e RUST_TARGET="$rust_target" \
    -v "${REPO_ROOT}:/work" \
    -v "${output_dir}:/out" \
    -w /work \
    "$ZIGBUILD_IMAGE" \
    bash -c 'set -euo pipefail
mkdir -p /tmp/cargo-home
rustup target add "$RUST_TARGET"
CARGO_TARGET_DIR=/tmp/asylum-target cargo zigbuild --release -p asylum --target "$RUST_TARGET"
cp "/tmp/asylum-target/${RUST_TARGET}/release/asylum" "$OUTPUT_BIN"'

  package_binary "$scratch_binary" "asylum-${release_name}.tar.gz" "$output_dir"
  rm -f "$scratch_binary"
}

require_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    echo "Docker is required for local Linux release builds." >&2
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

  local host="$(uname -s):$(uname -m)"

  if contains_target "darwin-arm64"; then
    case "$host" in
      Darwin:*) build_macos "darwin-arm64" "aarch64-apple-darwin" "$OUTPUT_DIR" ;;
      *)        build_via_zigbuild "darwin-arm64" "aarch64-apple-darwin" "$OUTPUT_DIR" ;;
    esac
  fi
  if contains_target "darwin-x86_64"; then
    case "$host" in
      Darwin:*) build_macos "darwin-x86_64" "x86_64-apple-darwin" "$OUTPUT_DIR" ;;
      *)        build_via_zigbuild "darwin-x86_64" "x86_64-apple-darwin" "$OUTPUT_DIR" ;;
    esac
  fi
  if contains_target "linux-arm64"; then
    case "$host" in
      # Apple Silicon: native arm64 in Docker, no emulation.
      Darwin:arm64|Linux:aarch64) build_linux_native "linux-arm64" "linux/arm64" "$OUTPUT_DIR" ;;
      # Linux x86_64: cross-compile via zigbuild image (no QEMU needed).
      *)                          build_via_zigbuild "linux-arm64" "aarch64-unknown-linux-gnu" "$OUTPUT_DIR" ;;
    esac
  fi
  if contains_target "linux-x86_64"; then
    case "$host" in
      Linux:x86_64)
        build_linux_native "linux-x86_64" "linux/amd64" "$OUTPUT_DIR"
        ;;
      *)
        build_linux_x86_64 "linux-x86_64" "$OUTPUT_DIR"
        ;;
    esac
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
