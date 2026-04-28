#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

VERSION=""
ARTIFACT_DIR=""
DRY_RUN=0

print_help() {
  cat <<'USAGE'
Usage: scripts/publish-release.sh [options]

Publish local Asylum release artifacts to GitHub Releases.

Options:
  --version <semver|tag>      Version/tag to publish. Defaults to Cargo.toml workspace version.
  --artifact-dir <path>       Artifact directory. Defaults to dist/release/v<version>.
  --dry-run                   Validate inputs and print the publish command without running it.
  --help                      Show this help.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:?missing value for --version}"
      shift 2
      ;;
    --artifact-dir)
      ARTIFACT_DIR="${2:?missing value for --artifact-dir}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
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

validate_archive() {
  local archive=$1
  local archive_contents
  archive_contents="$(tar -tzf "$archive")"
  if [[ "$archive_contents" != "asylum" ]]; then
    echo "Archive ${archive} does not contain exactly one top-level asylum binary" >&2
    printf '%s\n' "$archive_contents" >&2
    exit 1
  fi
}

main() {
  cd "$REPO_ROOT"

  if [[ -z "$VERSION" ]]; then
    VERSION="$(workspace_version)"
  fi
  local tag
  tag="$(tag_for_version "$VERSION")"

  if [[ -z "$ARTIFACT_DIR" ]]; then
    ARTIFACT_DIR="${REPO_ROOT}/dist/release/${tag}"
  fi

  local required=(
    "asylum-darwin-arm64.tar.gz"
    "asylum-darwin-x86_64.tar.gz"
    "asylum-linux-arm64.tar.gz"
    "asylum-linux-x86_64.tar.gz"
    "checksums.txt"
  )
  for file in "${required[@]}"; do
    if [[ ! -s "${ARTIFACT_DIR}/${file}" ]]; then
      echo "Missing release artifact: ${ARTIFACT_DIR}/${file}" >&2
      exit 1
    fi
  done
  for archive in "${ARTIFACT_DIR}"/asylum-*.tar.gz; do
    validate_archive "$archive"
  done

  if ! command -v gh >/dev/null 2>&1; then
    echo "gh is required to publish the GitHub Release." >&2
    exit 1
  fi

  if (( DRY_RUN )); then
    printf 'Would publish %s from %s\n' "$tag" "$ARTIFACT_DIR"
    return 0
  fi

  if gh release view "$tag" >/dev/null 2>&1; then
    gh release upload "$tag" "${ARTIFACT_DIR}"/* --clobber
  else
    gh release create "$tag" "${ARTIFACT_DIR}"/* \
      --target "$(git rev-parse HEAD)" \
      --title "$tag" \
      --notes "Asylum ${tag} binary release."
  fi
}

main "$@"
