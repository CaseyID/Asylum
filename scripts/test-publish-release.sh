#!/usr/bin/env bash
#
# Tests for scripts/publish-release.sh safety checks (H8).
#
# Drives publish-release.sh against a temporary git repository so we can
# manipulate tag/HEAD/working-tree state and assert the script aborts (or
# proceeds) with the right exit code and error message.
#
# We never reach the actual `gh release` calls here:
#   * for the dry-run case we stop at the dry-run path (success).
#   * for the failure cases we abort earlier than gh.
# A `gh` shim is placed on PATH that records calls but always succeeds, so
# even non-dry-run invocations would not contact GitHub.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PUBLISH_SCRIPT="${SCRIPT_DIR}/publish-release.sh"

failures=0
pass() {
  printf 'PASS: %s\n' "$1"
}
fail() {
  printf 'FAIL: %s\n' "$1"
  failures=$((failures + 1))
}

assert_contains() {
  local haystack=$1
  local needle=$2
  local label=$3
  if [[ "$haystack" == *"$needle"* ]]; then
    pass "$label"
  else
    fail "$label (missing '${needle}')"
    printf '  haystack was: %s\n' "$haystack" >&2
  fi
}

assert_eq() {
  local actual=$1
  local expected=$2
  local label=$3
  if [[ "$actual" == "$expected" ]]; then
    pass "$label"
  else
    fail "$label (expected=${expected}, actual=${actual})"
  fi
}

# Build a fixture repo with:
#   * a Cargo.toml workspace version
#   * an artifact dir containing all required archives + checksums.txt
make_fixture() {
  local fixture_root=$1
  local version=$2

  mkdir -p "$fixture_root"
  cd "$fixture_root"
  git init -q -b main
  git config user.email tester@example.com
  git config user.name "Tester"
  cat > Cargo.toml <<EOF
[workspace]
version = "${version}"
EOF
  cat > .gitignore <<'EOF'
dist/
shim/
gh.log
NOTES.md
EOF
  git add Cargo.toml .gitignore
  git commit -q -m "initial"

  local tag="v${version}"
  local artifact_dir="${fixture_root}/dist/release/${tag}"
  mkdir -p "$artifact_dir"

  # Build a minimal valid asylum tar archive (single top-level `asylum`).
  local tmpbin
  tmpbin="$(mktemp -d)"
  printf '#!/usr/bin/env bash\n:\n' > "${tmpbin}/asylum"
  chmod +x "${tmpbin}/asylum"
  for asset in darwin-arm64 darwin-x86_64 linux-arm64 linux-x86_64; do
    tar -czf "${artifact_dir}/asylum-${asset}.tar.gz" -C "$tmpbin" asylum
  done
  rm -rf "$tmpbin"

  (
    cd "$artifact_dir"
    : > checksums.txt
    for f in asylum-*.tar.gz; do
      if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$f" >> checksums.txt
      else
        shasum -a 256 "$f" >> checksums.txt
      fi
    done
  )
  printf '%s\n' "$artifact_dir"
}

# Place a `gh` shim on PATH that records invocations to a log file and
# always succeeds. Returns the path to the log file.
install_gh_shim() {
  local shim_root=$1
  mkdir -p "${shim_root}/bin"
  local log="${shim_root}/gh.log"
  : > "$log"
  cat > "${shim_root}/bin/gh" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${log}"
# Default: pretend release does not exist so 'view' fails.
case "\$1" in
  release)
    case "\$2" in
      view)
        if [[ -n "\${ASYLUM_TEST_RELEASE_EXISTS:-}" ]]; then
          if [[ "\$*" == *"--json assets"* ]]; then
            printf '%s\n' "\${ASYLUM_TEST_ASSETS_COUNT:-0}"
          fi
          exit 0
        fi
        exit 1
        ;;
    esac
    ;;
esac
exit 0
EOF
  chmod +x "${shim_root}/bin/gh"
  printf '%s\n' "$log"
}

# ---- Test 1: missing tag ----------------------------------------------------
test_missing_tag() {
  local root
  root="$(mktemp -d)"
  local artifact_dir
  artifact_dir="$(make_fixture "$root" "9.9.0")"
  local shim_root="${root}/shim"
  install_gh_shim "$shim_root" >/dev/null
  cd "$root"
  local out rc
  set +e
  out="$(ASYLUM_PUBLISH_REPO_ROOT="$root" PATH="${shim_root}/bin:${PATH}" bash "$PUBLISH_SCRIPT" --version v9.9.0 --artifact-dir "$artifact_dir" --dry-run 2>&1)"
  rc=$?
  set -e
  assert_eq "$rc" "1" "missing tag aborts (exit code)"
  assert_contains "$out" "Local tag v9.9.0 does not exist" "missing tag error message"
  rm -rf "$root"
}

# ---- Test 2: tag exists but does not point at HEAD --------------------------
test_tag_mismatch_head() {
  local root
  root="$(mktemp -d)"
  local artifact_dir
  artifact_dir="$(make_fixture "$root" "9.9.1")"
  cd "$root"
  # tag the current commit
  git tag -a v9.9.1 -m "release"
  # then move HEAD forward
  date > extra.txt
  git add extra.txt
  git commit -q -m "after tag"
  local shim_root="${root}/shim"
  install_gh_shim "$shim_root" >/dev/null
  local out rc
  set +e
  out="$(ASYLUM_PUBLISH_REPO_ROOT="$root" PATH="${shim_root}/bin:${PATH}" bash "$PUBLISH_SCRIPT" --version v9.9.1 --artifact-dir "$artifact_dir" --dry-run 2>&1)"
  rc=$?
  set -e
  assert_eq "$rc" "1" "tag/HEAD mismatch aborts (exit code)"
  assert_contains "$out" "check out the tag before publishing" "tag mismatch error message"
  rm -rf "$root"
}

# ---- Test 3: dirty working tree --------------------------------------------
test_dirty_worktree() {
  local root
  root="$(mktemp -d)"
  local artifact_dir
  artifact_dir="$(make_fixture "$root" "9.9.2")"
  cd "$root"
  git tag -a v9.9.2 -m "release"
  echo "modified" > Cargo.toml
  cat >> Cargo.toml <<EOF

# extra noise
EOF
  local shim_root="${root}/shim"
  install_gh_shim "$shim_root" >/dev/null
  local out rc
  set +e
  out="$(ASYLUM_PUBLISH_REPO_ROOT="$root" PATH="${shim_root}/bin:${PATH}" bash "$PUBLISH_SCRIPT" --version v9.9.2 --artifact-dir "$artifact_dir" --dry-run 2>&1)"
  rc=$?
  set -e
  # Note: editing Cargo.toml may change the workspace version. Reset before
  # re-checking. The dirty error is still expected because we never reach
  # the version-derived steps; verify_clean_worktree runs second.
  assert_eq "$rc" "1" "dirty worktree aborts (exit code)"
  assert_contains "$out" "working tree is dirty" "dirty worktree error message"
  rm -rf "$root"
}

# ---- Test 4: dirty worktree allowed when --allow-dirty is passed -----------
test_dirty_worktree_allow_dirty() {
  local root
  root="$(mktemp -d)"
  local artifact_dir
  artifact_dir="$(make_fixture "$root" "9.9.3")"
  cd "$root"
  git tag -a v9.9.3 -m "release"
  # Create a dirty change that does not affect Cargo.toml version.
  echo "noise" > NOTES.md
  local shim_root="${root}/shim"
  install_gh_shim "$shim_root" >/dev/null
  local out rc
  set +e
  out="$(ASYLUM_PUBLISH_REPO_ROOT="$root" PATH="${shim_root}/bin:${PATH}" bash "$PUBLISH_SCRIPT" --version v9.9.3 --artifact-dir "$artifact_dir" --dry-run --allow-dirty 2>&1)"
  rc=$?
  set -e
  assert_eq "$rc" "0" "dirty worktree with --allow-dirty succeeds (exit code)"
  assert_contains "$out" "Would publish v9.9.3" "dry-run summary printed"
  rm -rf "$root"
}

# ---- Test 5: valid tag at HEAD, clean tree, dry-run succeeds ---------------
test_happy_path_dry_run() {
  local root
  root="$(mktemp -d)"
  local artifact_dir
  artifact_dir="$(make_fixture "$root" "9.9.4")"
  cd "$root"
  git tag -a v9.9.4 -m "release"
  local shim_root="${root}/shim"
  install_gh_shim "$shim_root" >/dev/null
  local out rc
  set +e
  out="$(ASYLUM_PUBLISH_REPO_ROOT="$root" PATH="${shim_root}/bin:${PATH}" bash "$PUBLISH_SCRIPT" --version v9.9.4 --artifact-dir "$artifact_dir" --dry-run 2>&1)"
  rc=$?
  set -e
  assert_eq "$rc" "0" "happy path dry-run succeeds"
  assert_contains "$out" "Would publish v9.9.4" "happy path prints publish summary"
  rm -rf "$root"
}

# ---- Test 6: refuse to clobber existing assets without --allow-clobber -----
test_refuse_clobber_default() {
  local root
  root="$(mktemp -d)"
  local artifact_dir
  artifact_dir="$(make_fixture "$root" "9.9.5")"
  cd "$root"
  git tag -a v9.9.5 -m "release"
  local shim_root="${root}/shim"
  install_gh_shim "$shim_root" >/dev/null
  local out rc
  set +e
  # Tell shim: release exists with 4 assets
  out="$(ASYLUM_TEST_RELEASE_EXISTS=1 ASYLUM_TEST_ASSETS_COUNT=4 \
    ASYLUM_PUBLISH_REPO_ROOT="$root" PATH="${shim_root}/bin:${PATH}" bash "$PUBLISH_SCRIPT" --version v9.9.5 --artifact-dir "$artifact_dir" 2>&1)"
  rc=$?
  set -e
  assert_eq "$rc" "1" "default refuses to clobber existing assets"
  assert_contains "$out" "Refusing to overwrite" "clobber refusal error message"
  assert_contains "$out" "--allow-clobber" "clobber refusal mentions opt-in flag"
  rm -rf "$root"
}

# ---- Test 7: --allow-clobber proceeds to gh upload -------------------------
test_allow_clobber_proceeds() {
  local root
  root="$(mktemp -d)"
  local artifact_dir
  artifact_dir="$(make_fixture "$root" "9.9.6")"
  cd "$root"
  git tag -a v9.9.6 -m "release"
  local shim_root="${root}/shim"
  local log
  log="$(install_gh_shim "$shim_root")"
  local out rc
  set +e
  out="$(ASYLUM_TEST_RELEASE_EXISTS=1 ASYLUM_TEST_ASSETS_COUNT=4 \
    ASYLUM_PUBLISH_REPO_ROOT="$root" PATH="${shim_root}/bin:${PATH}" bash "$PUBLISH_SCRIPT" --version v9.9.6 --artifact-dir "$artifact_dir" --allow-clobber 2>&1)"
  rc=$?
  set -e
  assert_eq "$rc" "0" "--allow-clobber succeeds"
  if [[ -s "$log" ]]; then
    assert_contains "$(cat "$log")" "release upload v9.9.6" "gh upload was invoked"
    assert_contains "$(cat "$log")" "--clobber" "gh upload --clobber was used"
  else
    fail "gh log was empty"
  fi
  rm -rf "$root"
}

# ---- Test 8: new release uses tag (not HEAD sha) as --target ---------------
test_release_create_uses_tag_target() {
  local root
  root="$(mktemp -d)"
  local artifact_dir
  artifact_dir="$(make_fixture "$root" "9.9.7")"
  cd "$root"
  git tag -a v9.9.7 -m "release"
  local shim_root="${root}/shim"
  local log
  log="$(install_gh_shim "$shim_root")"
  local out rc
  set +e
  # release does NOT exist (default shim behavior)
  out="$(ASYLUM_PUBLISH_REPO_ROOT="$root" PATH="${shim_root}/bin:${PATH}" bash "$PUBLISH_SCRIPT" --version v9.9.7 --artifact-dir "$artifact_dir" 2>&1)"
  rc=$?
  set -e
  assert_eq "$rc" "0" "create-new-release succeeds"
  if [[ -s "$log" ]]; then
    assert_contains "$(cat "$log")" "release create v9.9.7" "gh release create was invoked"
    assert_contains "$(cat "$log")" "--target v9.9.7" "release create uses tag as --target"
  else
    fail "gh log was empty"
  fi
  rm -rf "$root"
}

test_missing_tag
test_tag_mismatch_head
test_dirty_worktree
test_dirty_worktree_allow_dirty
test_happy_path_dry_run
test_refuse_clobber_default
test_allow_clobber_proceeds
test_release_create_uses_tag_target

if (( failures > 0 )); then
  printf '\nFAILED: %d checks failed\n' "$failures"
  exit 1
fi

printf '\nAll publish-release safety checks passed\n'
