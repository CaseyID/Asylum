#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# REPO_ROOT can be overridden by the caller (used by tests) so the script
# can be exercised against a fixture repo. Defaults to the parent of SCRIPT_DIR.
REPO_ROOT="${ASYLUM_PUBLISH_REPO_ROOT:-$(cd "${SCRIPT_DIR}/.." && pwd)}"

VERSION=""
ARTIFACT_DIR=""
DRY_RUN=0
ALLOW_CLOBBER=0
ALLOW_DIRTY=0
TARGETS="darwin-arm64,darwin-x86_64,linux-arm64,linux-x86_64"

print_help() {
  cat <<'USAGE'
Usage: scripts/publish-release.sh [options]

Publish local Asylum release artifacts to GitHub Releases.

Options:
  --version <semver|tag>      Version/tag to publish. Defaults to Cargo.toml workspace version.
  --artifact-dir <path>       Artifact directory. Defaults to dist/release/v<version>.
  --dry-run                   Validate inputs and print the publish command without running it.
  --allow-clobber             Allow overwriting existing release assets via gh upload --clobber.
                              Without this flag, publishing aborts if the release already has assets.
  --allow-dirty               Allow publishing from a dirty working tree (not recommended).
  --targets <list>            Comma-separated platforms to require + upload.
                              Default: darwin-arm64,darwin-x86_64,linux-arm64,linux-x86_64
                              Use this to publish a partial-platform release (e.g. when
                              only some build hosts are available). Subsequent --allow-clobber
                              runs from another host can fill in the missing platforms.
  --help                      Show this help.

Safety checks (always enforced unless overridden):
  * Local annotated tag <tag> must exist.
  * <tag> must point at HEAD; otherwise checkout the tag before publishing.
  * Working tree must be clean unless --allow-dirty is passed.
  * Existing release assets are preserved unless --allow-clobber is passed.

Optional signing:
  * If ASYLUM_RELEASE_SIGNING_KEY is set and minisign is on PATH, this script
    produces checksums.txt.minisig alongside checksums.txt before upload.
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
    --allow-clobber)
      ALLOW_CLOBBER=1
      shift
      ;;
    --allow-dirty)
      ALLOW_DIRTY=1
      shift
      ;;
    --targets)
      TARGETS="${2:?missing value for --targets}"
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

# M13: Before publishing, recompute sha256 for each archive and verify it matches
# the entry in checksums.txt. Fails with a clear message if mismatched or missing.
verify_checksums_against_artifacts() {
  local artifact_dir=$1
  local checksum_file="${artifact_dir}/checksums.txt"
  local hash_cmd

  # Determine available hash command (mirrors install.sh logic)
  if command -v sha256sum >/dev/null 2>&1; then
    hash_cmd="sha256sum"
  elif command -v shasum >/dev/null 2>&1; then
    hash_cmd="shasum -a 256"
  else
    echo "Cannot verify checksums: sha256sum or shasum is required." >&2
    exit 1
  fi

  for archive in "${artifact_dir}"/asylum-*.tar.gz; do
    local archive_name
    archive_name="$(basename "$archive")"
    local expected
    expected="$(awk -v f="$archive_name" '$2 == f || $2 == ("*" f) { print $1; found=1 } END { if (!found) exit 1 }' "$checksum_file" 2>/dev/null)" || {
      echo "Checksum mismatch: ${archive_name} not listed in checksums.txt — checksums.txt may be stale. Rebuild artifacts before publishing." >&2
      exit 1
    }
    local actual
    actual="$(eval "$hash_cmd \"$archive\"" | awk '{print $1}')"
    if [[ "$expected" != "$actual" ]]; then
      printf 'Checksum mismatch for %s\n  expected: %s\n  actual:   %s\n' \
        "$archive_name" "$expected" "$actual" >&2
      echo "Rebuild artifacts (checksums.txt is stale) before publishing." >&2
      exit 1
    fi
    echo "Checksum verified: ${archive_name}"
  done
}

# Verify that an annotated tag exists locally and points at HEAD.
# Prints errors to stderr and returns non-zero on failure.
# Args: <tag> <repo_dir>
verify_tag_matches_head() {
  local tag=$1
  local repo_dir=$2

  # Tag must exist locally.
  if ! git -C "$repo_dir" rev-parse --verify --quiet "refs/tags/${tag}" >/dev/null; then
    echo "Local tag ${tag} does not exist; create it (git tag -a ${tag}) before publishing." >&2
    return 1
  fi

  # Prefer annotated tags. A non-annotated (lightweight) tag is allowed but
  # warned about: annotated tags carry signer/date metadata.
  local tag_object_type
  tag_object_type="$(git -C "$repo_dir" cat-file -t "refs/tags/${tag}" 2>/dev/null || true)"
  if [[ "$tag_object_type" != "tag" ]]; then
    echo "warning: tag ${tag} is lightweight, not annotated; prefer 'git tag -a ${tag}'." >&2
  fi

  local tag_sha head_sha
  tag_sha="$(git -C "$repo_dir" rev-parse "${tag}^{commit}")"
  head_sha="$(git -C "$repo_dir" rev-parse "HEAD^{commit}")"

  if [[ "$tag_sha" != "$head_sha" ]]; then
    echo "tag ${tag} points at ${tag_sha}; HEAD is ${head_sha}; check out the tag before publishing." >&2
    return 1
  fi
  return 0
}

# Refuse to publish if the working tree is dirty unless overridden.
verify_clean_worktree() {
  local repo_dir=$1
  local allow_dirty=$2
  if (( allow_dirty )); then
    return 0
  fi
  local porcelain
  porcelain="$(git -C "$repo_dir" status --porcelain)"
  if [[ -n "$porcelain" ]]; then
    echo "working tree is dirty; commit/stash changes or pass --allow-dirty:" >&2
    printf '%s\n' "$porcelain" >&2
    return 1
  fi
  return 0
}

# Optionally produce checksums.txt.minisig if a signing key is configured.
# Honors ASYLUM_RELEASE_SIGNING_KEY (path to minisign secret key).
# No-op when the env var is unset (current default behavior, no signing).
maybe_sign_checksums() {
  local artifact_dir=$1
  if [[ -z "${ASYLUM_RELEASE_SIGNING_KEY:-}" ]]; then
    return 0
  fi
  if ! command -v minisign >/dev/null 2>&1; then
    echo "ASYLUM_RELEASE_SIGNING_KEY is set but minisign is not on PATH; cannot sign checksums.txt." >&2
    return 1
  fi
  local checksum_file="${artifact_dir}/checksums.txt"
  local sig_file="${artifact_dir}/checksums.txt.minisig"
  echo "Signing checksums.txt with ASYLUM_RELEASE_SIGNING_KEY"
  # -S sign, -s secret key path, -m message, -x signature output
  if ! minisign -S -s "$ASYLUM_RELEASE_SIGNING_KEY" -m "$checksum_file" -x "$sig_file"; then
    echo "minisign failed to sign checksums.txt" >&2
    return 1
  fi
  return 0
}

# Returns 0 if the release on GitHub already has any assets attached.
release_has_assets() {
  local tag=$1
  # gh release view --json assets -q '.assets | length' returns count
  local count
  if ! count="$(gh release view "$tag" --json assets --jq '.assets | length' 2>/dev/null)"; then
    return 1
  fi
  [[ "$count" =~ ^[0-9]+$ ]] || return 1
  (( count > 0 ))
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

  local required=("checksums.txt")
  IFS=',' read -ra _targets <<< "$TARGETS"
  for t in "${_targets[@]}"; do
    case "$t" in
      darwin-arm64|darwin-x86_64|linux-arm64|linux-x86_64)
        required+=("asylum-${t}.tar.gz")
        ;;
      "") ;;
      *)
        echo "Unknown target: $t (expected darwin-arm64|darwin-x86_64|linux-arm64|linux-x86_64)" >&2
        exit 2
        ;;
    esac
  done
  for file in "${required[@]}"; do
    if [[ ! -s "${ARTIFACT_DIR}/${file}" ]]; then
      echo "Missing release artifact: ${ARTIFACT_DIR}/${file}" >&2
      exit 1
    fi
  done
  for archive in "${ARTIFACT_DIR}"/asylum-*.tar.gz; do
    validate_archive "$archive"
  done
  verify_checksums_against_artifacts "$ARTIFACT_DIR"

  # Tag/HEAD reconciliation. Done before gh check so we fail fast even when gh
  # is unavailable.
  if ! verify_tag_matches_head "$tag" "$REPO_ROOT"; then
    exit 1
  fi
  if ! verify_clean_worktree "$REPO_ROOT" "$ALLOW_DIRTY"; then
    exit 1
  fi

  if ! command -v gh >/dev/null 2>&1; then
    echo "gh is required to publish the GitHub Release." >&2
    exit 1
  fi

  # Best-effort signature production. Only acts when env var is set.
  if ! maybe_sign_checksums "$ARTIFACT_DIR"; then
    exit 1
  fi

  if (( DRY_RUN )); then
    printf 'Would publish %s from %s\n' "$tag" "$ARTIFACT_DIR"
    return 0
  fi

  if gh release view "$tag" >/dev/null 2>&1; then
    if release_has_assets "$tag" && (( ! ALLOW_CLOBBER )); then
      echo "Release ${tag} already has assets attached." >&2
      echo "Refusing to overwrite. Bump the tag, or pass --allow-clobber to deliberately overwrite." >&2
      exit 1
    fi
    if (( ALLOW_CLOBBER )); then
      gh release upload "$tag" "${ARTIFACT_DIR}"/* --clobber
    else
      gh release upload "$tag" "${ARTIFACT_DIR}"/*
    fi
  else
    # Use the tag name (not HEAD sha) as --target so GitHub binds the release
    # to the tag's commit, not whatever HEAD happens to be.
    gh release create "$tag" "${ARTIFACT_DIR}"/* \
      --target "$tag" \
      --title "$tag" \
      --notes "Asylum ${tag} binary release."
  fi
}

# Allow this script to be sourced for testing of helper functions without
# executing main. Detection mirrors install.sh.
PUBLISH_SCRIPT_SOURCE="${BASH_SOURCE[0]:-}"
if [[ -z "$PUBLISH_SCRIPT_SOURCE" || "$PUBLISH_SCRIPT_SOURCE" == "${0}" ]]; then
  main "$@"
fi
