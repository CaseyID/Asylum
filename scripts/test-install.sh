#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/install.sh"

assert_eq() {
  local actual=$1
  local expected=$2
  local label=$3
  if [[ "$actual" != "$expected" ]]; then
    echo "FAIL: ${label} (expected=${expected}, actual=${actual})"
    return 1
  fi
  echo "PASS: ${label}"
}

assert_contains() {
  local text=$1
  local needle=$2
  local label=$3
  if [[ "$text" != *"$needle"* ]]; then
    echo "FAIL: ${label} (missing '${needle}')"
    return 1
  fi
  echo "PASS: ${label}"
}

assert_not_contains() {
  local text=$1
  local needle=$2
  local label=$3
  if [[ "$text" == *"$needle"* ]]; then
    echo "FAIL: ${label} (found forbidden '${needle}')"
    return 1
  fi
  echo "PASS: ${label}"
}

assert_zero() {
  local rc=$1
  local label=$2
  if [[ "$rc" -ne 0 ]]; then
    echo "FAIL: ${label} (expected exit 0, got ${rc})"
    return 1
  fi
  echo "PASS: ${label}"
}

assert_nonempty_file() {
  local path=$1
  local label=$2
  if [[ ! -s "$path" ]]; then
    echo "FAIL: ${label} (expected non-empty file)"
    return 1
  fi
  echo "PASS: ${label}"
}

assert_nonzero() {
  local rc=$1
  local label=$2
  if [[ "$rc" -eq 0 ]]; then
    echo "FAIL: ${label} (expected non-zero exit)"
    return 1
  fi
  echo "PASS: ${label}"
}

failures=0

check() {
  if ! "$@"; then
    failures=$((failures + 1))
  fi
}

check assert_eq "$(asylum_normalize_os darwin)" "darwin" "normalize_os darwin"
check assert_eq "$(asylum_normalize_os Linux)" "linux" "normalize_os linux"
check assert_eq "$(asylum_normalize_os Windows_NT)" "unsupported" "normalize_os unsupported"

check assert_eq "$(asylum_normalize_arch arm64)" "arm64" "normalize_arch arm64"
check assert_eq "$(asylum_normalize_arch aarch64)" "arm64" "normalize_arch aarch64"
check assert_eq "$(asylum_normalize_arch x86_64)" "x86_64" "normalize_arch x86_64"
check assert_eq "$(asylum_normalize_arch amd64)" "x86_64" "normalize_arch amd64"
check assert_eq "$(asylum_normalize_arch x86)" "unsupported" "normalize_arch unsupported"

check assert_eq "$(asylum_archive_name darwin arm64)" "asylum-darwin-arm64.tar.gz" "archive name"
check assert_eq "$(asylum_release_url v0.1.1 asylum-darwin-arm64.tar.gz)" "https://github.com/CaseyID/Asylum/releases/download/v0.1.1/asylum-darwin-arm64.tar.gz" "release URL construction"

if asylum_parse_args --help; then
  parse_rc=0
else
  parse_rc=$?
fi
check assert_eq "${parse_rc}" "0" "parse --help"
check assert_eq "${SHOW_HELP}" "1" "--help flag"

if asylum_parse_args --version v1.2.3 --install-dir /tmp/asylum/bin --asylum-home /tmp/asylum/home --yes --skip-setup --skip-doctor --no-color; then
  parse_rc=0
else
  parse_rc=$?
fi
check assert_zero "${parse_rc}" "parse supported options"
check assert_eq "${VERSION}" "v1.2.3" "parse version"
check assert_eq "${INSTALL_DIR}" "/tmp/asylum/bin" "parse install-dir"
check assert_eq "${ASYLUM_HOME}" "/tmp/asylum/home" "parse asylum-home"
check assert_eq "${ASSUME_YES}" "1" "parse yes"
check assert_eq "${SKIP_SETUP}" "1" "parse skip-setup"
check assert_eq "${SKIP_DOCTOR}" "1" "parse skip-doctor"
check assert_eq "${NO_COLOR}" "1" "parse no-color"

if asylum_parse_args --version; then
  parse_rc=0
else
  parse_rc=$?
fi
check assert_eq "${parse_rc}" "2" "parse missing --version arg"

if asylum_parse_args --mystery; then
  parse_rc=0
else
  parse_rc=$?
fi
check assert_eq "${parse_rc}" "2" "parse unknown option"

if parse_sete_output="$(bash -lc 'set -e; source "'"${SCRIPT_DIR}"'/install.sh"; asylum_main --version' 2>&1)"; then
  parse_sete_rc=0
else
  parse_sete_rc=$?
fi
check assert_eq "${parse_sete_rc}" "2" "main parse failure under set -e returns parse code"
check assert_contains "$parse_sete_output" "Usage:" "main parse failure prints usage"

if piped_help_output="$(bash -s -- --help < "${SCRIPT_DIR}/install.sh" 2>&1)"; then
  piped_help_rc=0
else
  piped_help_rc=$?
fi
check assert_eq "${piped_help_rc}" "0" "piped installer runs under bash -s"
check assert_contains "$piped_help_output" "Asylum binary installer" "piped installer prints help"

NO_COLOR=1
asylum_color_init
INSTALL_DIR="/tmp/asylum-test-home"
next_steps_output="$(asylum_next_steps)"
check assert_contains "$next_steps_output" "Next steps:" "next steps includes header"
check assert_contains "$next_steps_output" "  asylum setup" "next steps includes asylum setup"
check assert_contains "$next_steps_output" "  asylum doctor" "next steps includes asylum doctor"
check assert_contains "$next_steps_output" "  asylum" "next steps includes asylum"
check assert_contains "$next_steps_output" "export PATH=\"/tmp/asylum-test-home:\$PATH\"" "next steps includes PATH suggestion"
check assert_not_contains "$next_steps_output" "%s" "next steps output has no format artifacts"

ORIG_HOME="${HOME}"
ORIG_PATH="${PATH}"
path_test_home="$(mktemp -d)"
export HOME="$path_test_home"
PATH="/usr/local/bin:/usr/bin:/bin"
touch "${HOME}/.bashrc"
touch "${HOME}/.zshrc"
touch "${HOME}/.profile"

export SHELL="/bin/bash"
check assert_eq "$(asylum_detect_shell_rc)" "${HOME}/.bashrc" "detect shell rc prefers bashrc"
export SHELL="/bin/zsh"
check assert_eq "$(asylum_detect_shell_rc)" "${HOME}/.zshrc" "detect shell rc prefers zshrc"
export SHELL="/bin/fish"
check assert_eq "$(asylum_detect_shell_rc)" "${HOME}/.profile" "detect shell rc uses profile fallback"

path_dir="${HOME}/.local/bin"
path_out="$(asylum_print_path_instructions "${path_dir}")"
check assert_contains "$path_out" "Add this line to ${HOME}/.profile" "path instructions include shell rc file"

ASYLUM_TEST_RC="${HOME}/.zshrc"
SHELL="/bin/zsh"
asylum_apply_path_to_shell_rc "${path_dir}" >/dev/null
first_marker_count="$(grep -Fc "# Added by Asylum installer" "${ASYLUM_TEST_RC}" 2>/dev/null || echo 0)"
first_rc_content="$(cat "${ASYLUM_TEST_RC}")"
asylum_apply_path_to_shell_rc "${path_dir}" >/dev/null
second_marker_count="$(grep -Fc "# Added by Asylum installer" "${ASYLUM_TEST_RC}" 2>/dev/null || echo 0)"
second_rc_content="$(cat "${ASYLUM_TEST_RC}")"
check assert_eq "${first_marker_count}" "1" "path rc edit is idempotent"
check assert_eq "${second_marker_count}" "1" "path rc edit remains idempotent on repeat"
check assert_eq "${second_rc_content}" "${first_rc_content}" "path rc edit leaves file content unchanged on repeat"
check assert_contains "$(cat "${ASYLUM_TEST_RC}")" "export PATH=\"${path_dir}:" "path rc edit adds export"

PATH="/some/path:/bin"
if asylum_path_contains "/some/path"; then
  rc_in_path=0
else
  rc_in_path=1
fi
check assert_eq "${rc_in_path}" "0" "path contains exact dir"
if asylum_path_contains "/some"; then
  rc_in_false=0
else
  rc_in_false=1
fi
check assert_eq "${rc_in_false}" "1" "path does not treat partial segment as present"
PATH="${ORIG_PATH}"

old_dir_install="${HOME}/.local/bin-old"
new_dir_install="${HOME}/.local/bin-new"
PATH="${new_dir_install}:/usr/bin:/bin"
printf '# Added by Asylum installer\nif [[ ":$PATH:" != *"%s:"* ]]; then\n  export PATH="%s:$PATH"\nfi\n' "${old_dir_install}" "${old_dir_install}" > "${ASYLUM_TEST_RC}"
asylum_apply_path_to_shell_rc "${new_dir_install}" >/dev/null
updated_line_count="$(grep -Fc "# Added by Asylum installer" "${ASYLUM_TEST_RC}" 2>/dev/null || echo 0)"
updated_rc_content="$(cat "${ASYLUM_TEST_RC}")"
asylum_apply_path_to_shell_rc "${new_dir_install}" >/dev/null
updated_repeat_rc_content="$(cat "${ASYLUM_TEST_RC}")"
check assert_eq "${updated_line_count}" "1" "path rc updates existing managed block without duplicating marker when install dir is already on PATH"
check assert_eq "${updated_repeat_rc_content}" "${updated_rc_content}" "path rc update leaves file content unchanged on repeat"
check assert_contains "$(cat "${ASYLUM_TEST_RC}")" "export PATH=\"${new_dir_install}:\$PATH\"" "path rc updates managed block to new install dir already on PATH"
check assert_not_contains "$(cat "${ASYLUM_TEST_RC}")" "${old_dir_install}" "path rc replaces old managed install dir when new dir is already on PATH"

next_steps_old_dir="${HOME}/.local/bin-next-old"
next_steps_new_dir="${HOME}/.local/bin-next-new"
INSTALL_DIR="${next_steps_new_dir}"
PATH="${next_steps_new_dir}:/usr/bin:/bin"
printf '# Added by Asylum installer\nif [[ ":$PATH:" != *"%s:"* ]]; then\n  export PATH="%s:$PATH"\nfi\n' "${next_steps_old_dir}" "${next_steps_old_dir}" > "${ASYLUM_TEST_RC}"
next_steps_managed_output="$(asylum_next_steps)"
next_steps_marker_count="$(grep -Fc "# Added by Asylum installer" "${ASYLUM_TEST_RC}" 2>/dev/null || echo 0)"
check assert_contains "$next_steps_managed_output" "Next steps:" "next steps still renders when reconciling managed PATH block"
check assert_eq "${next_steps_marker_count}" "1" "next steps reconciles managed block without duplicating marker when install dir is already on PATH"
check assert_contains "$(cat "${ASYLUM_TEST_RC}")" "export PATH=\"${next_steps_new_dir}:\$PATH\"" "next steps updates managed block to selected install dir already on PATH"
check assert_not_contains "$(cat "${ASYLUM_TEST_RC}")" "${next_steps_old_dir}" "next steps removes stale managed install dir when selected dir is already on PATH"

export HOME="${ORIG_HOME}"
export PATH="${ORIG_PATH}"

tmp_dir="$(mktemp -d)"
asylum_set_tmpdir "$tmp_dir"
if ! asylum_cleanup_tmpdir; then
  cleanup_rc=$?
else
  cleanup_rc=0
fi
check assert_eq "$cleanup_rc" "0" "cleanup helper removes temp dir"
if [[ -d "$tmp_dir" ]]; then
  failures=$((failures + 1))
  echo "FAIL: cleanup helper did not remove directory"
fi

unset -v ASYLUM_INSTALLER_TMPDIR
if asylum_cleanup_tmpdir; then
  unset_cleanup_rc=0
else
  unset_cleanup_rc=$?
fi
check assert_eq "$unset_cleanup_rc" "0" "cleanup helper tolerates unset temp dir variable"

trap_test_tmpdir="$(mktemp -d)"
if bash -lc 'set -u; source "'"${SCRIPT_DIR}"'/install.sh"; ASYLUM_INSTALLER_TMPDIR="'"$trap_test_tmpdir"'"; trap '\''asylum_cleanup_tmpdir'\'' EXIT'; then
  trap_status=0
else
  trap_status=$?
fi
check assert_eq "$trap_status" "0" "trap path cleanup executes"
if [[ -d "$trap_test_tmpdir" ]]; then
  failures=$((failures + 1))
  echo "FAIL: trap cleanup function did not remove temporary directory"
fi

main_install_dir="$(mktemp -d)"
main_home="$(mktemp -d)"
verify_tmp_dir_record="$(mktemp)"

if bash -lc 'set -u
source "'"${SCRIPT_DIR}"'/install.sh"

asylum_download() {
  : > "$2"
}

asylum_verify_archive() {
  printf "%s" "$4" > "'"${verify_tmp_dir_record}"'"
  printf "skipped\n"
}

    asylum_extract_binary() {
      mkdir -p "$2"
      printf "#!/usr/bin/env bash\necho mock-asylum\n" > "$2/asylum"
      chmod +x "$2/asylum"
      printf "%s/asylum" "$2"
    }

asylum_install_binary() {
  mkdir -p "$2"
  cp "$1" "$2/asylum"
  chmod +x "$2/asylum"
}

asylum_run_setup_if_needed() {
  :
}

asylum_run_doctor_if_needed() {
  :
}

main_output="$(asylum_main --version v9.9.9 --install-dir '"${main_install_dir}"' --asylum-home '"${main_home}"' --skip-setup --skip-doctor --no-color 2>&1)"
printf "%s\n" "$main_output"
'; then
  main_flow_rc=0
else
  main_flow_rc=$?
fi
check assert_eq "$main_flow_rc" "0" "main flow executes without undefined vars"
if [[ $main_flow_rc -eq 0 ]]; then
  check assert_nonempty_file "${verify_tmp_dir_record}" "checksum verifier received temp dir arg"
fi

child_home_test_dir="$(mktemp -d)"
child_home_record="$(mktemp)"
child_home_binary="${child_home_test_dir}/asylum"
cat > "$child_home_binary" <<'EOF'
#!/usr/bin/env bash
printf '%s=%s\n' "$1" "${ASYLUM_HOME:-}" >> "$ASYLUM_CHILD_RECORD"
EOF
chmod +x "$child_home_binary"
export ASYLUM_CHILD_RECORD="$child_home_record"
ASYLUM_HOME="${child_home_test_dir}/home"
SKIP_SETUP=0
SKIP_DOCTOR=0
ASSUME_YES=1
asylum_run_setup_if_needed "$child_home_binary" 0 >/dev/null
ASYLUM_HOME="${child_home_test_dir}/doctor-home"
asylum_run_doctor_if_needed "$child_home_binary" >/dev/null
check assert_contains "$(cat "$child_home_record")" "setup=${child_home_test_dir}/home" "setup child command receives custom ASYLUM_HOME"
check assert_contains "$(cat "$child_home_record")" "doctor=${child_home_test_dir}/doctor-home" "doctor child command receives custom ASYLUM_HOME"
SKIP_SETUP=1
SKIP_DOCTOR=1
unset ASYLUM_CHILD_RECORD

binary_install_dir="$(mktemp -d)"
binary_install_source="${binary_install_dir}/asylum-source"
printf '#!/usr/bin/env bash\necho new-asylum\n' > "$binary_install_source"
chmod +x "$binary_install_source"
binary_install_target="${binary_install_dir}/bin"
mkdir -p "$binary_install_target"
printf '#!/usr/bin/env bash\necho old-asylum\n' > "${binary_install_target}/asylum"
chmod +x "${binary_install_target}/asylum"

binary_tmp_count_before=0
for path in "${binary_install_target}"/.asylum-install-*; do
  if [[ -e "$path" ]]; then
    binary_tmp_count_before=$((binary_tmp_count_before + 1))
  fi
done
check assert_eq "$binary_tmp_count_before" "0" "install temp files cleaned up before install"

if asylum_install_binary "$binary_install_source" "$binary_install_target"; then
  install_binary_rc=0
else
  install_binary_rc=$?
fi
check assert_eq "$install_binary_rc" "0" "asylum_install_binary returns success"

check assert_eq "$("${binary_install_target}/asylum")" "new-asylum" "asylum_install_binary installs executable file"

binary_tmp_count_after=0
for path in "${binary_install_target}"/.asylum-install-*; do
  if [[ -e "$path" ]]; then
    binary_tmp_count_after=$((binary_tmp_count_after + 1))
  fi
done
check assert_eq "$binary_tmp_count_after" "0" "asylum_install_binary cleans temporary file on success"

extract_dir="$(mktemp -d)"
printf '#!/usr/bin/env bash\necho top-level\n' > "${extract_dir}/asylum"
chmod +x "${extract_dir}/asylum"
tar -czf "${extract_dir}/top-level.tar.gz" -C "${extract_dir}" asylum
if extracted_binary="$(asylum_extract_binary "${extract_dir}/top-level.tar.gz" "${extract_dir}/top-level-out")"; then
  top_level_rc=0
else
  top_level_rc=$?
fi
check assert_eq "$top_level_rc" "0" "extract top-level binary archive"
check assert_contains "$extracted_binary" "/asylum" "extract top-level binary path"

mkdir -p "${extract_dir}/pkg"
printf '#!/usr/bin/env bash\necho nested\n' > "${extract_dir}/pkg/asylum"
chmod +x "${extract_dir}/pkg/asylum"
tar -czf "${extract_dir}/nested.tar.gz" -C "${extract_dir}" pkg
if extracted_nested_binary="$(asylum_extract_binary "${extract_dir}/nested.tar.gz" "${extract_dir}/nested-out")"; then
  nested_rc=0
else
  nested_rc=$?
fi
check assert_eq "$nested_rc" "0" "extract nested path archive"
check assert_contains "$extracted_nested_binary" "/pkg/asylum" "extract nested keeps path"

mkdir -p "${extract_dir}/multiple"
printf '#!/usr/bin/env bash\necho top\n' > "${extract_dir}/multiple/asylum"
mkdir -p "${extract_dir}/multiple2"
printf '#!/usr/bin/env bash\necho nested\n' > "${extract_dir}/multiple2/asylum"
chmod +x "${extract_dir}/multiple/asylum" "${extract_dir}/multiple2/asylum"
tar -czf "${extract_dir}/multiple.tar.gz" -C "${extract_dir}" multiple/asylum multiple2/asylum
if duplicate_output="$(asylum_extract_binary "${extract_dir}/multiple.tar.gz" "${extract_dir}/multiple-out" 2>&1)"; then
  duplicate_rc=0
else
  duplicate_rc=$?
fi
check assert_nonzero "$duplicate_rc" "extract duplicate binaries rejected"

mkdir -p "${extract_dir}/symlink-src"
ln -s /bin/echo "${extract_dir}/symlink-src/asylum"
tar -czf "${extract_dir}/symlink.tar.gz" -C "${extract_dir}/symlink-src" asylum
if symlink_output="$(asylum_extract_binary "${extract_dir}/symlink.tar.gz" "${extract_dir}/symlink-out" 2>&1)"; then
  symlink_rc=0
else
  symlink_rc=$?
fi
check assert_nonzero "$symlink_rc" "extract rejects symlink binary"

mkdir -p "${extract_dir}/hardlink-src"
printf '#!/usr/bin/env bash\necho hard\n' > "${extract_dir}/hardlink-src/asylum-real"
chmod +x "${extract_dir}/hardlink-src/asylum-real"
ln "${extract_dir}/hardlink-src/asylum-real" "${extract_dir}/hardlink-src/asylum"
tar -czf "${extract_dir}/hardlink.tar.gz" -C "${extract_dir}/hardlink-src" asylum-real asylum
if hardlink_output="$(asylum_extract_binary "${extract_dir}/hardlink.tar.gz" "${extract_dir}/hardlink-out" 2>&1)"; then
  hardlink_rc=0
else
  hardlink_rc=$?
fi
check assert_nonzero "$hardlink_rc" "extract rejects hardlink binary"

if absolute_output="$((
  tar() {
    if [[ "$1" == "-tzf" ]]; then
      printf '/abs/path/asylum\n'
      return 0
    fi
    if [[ "$1" == "-tvf" ]]; then
      printf '-rw-r--r--  0 0      0 Jan  1 00:00 /abs/path/asylum\n'
      return 0
    fi
    if [[ "$1" == "-xzf" ]]; then
      return 0
    fi
    command tar "$@"
  }
  absolute_test_tmp="$(mktemp -d)"
  : > "${absolute_test_tmp}/absolute.tar.gz"
  if asylum_extract_binary "${absolute_test_tmp}/absolute.tar.gz" "${absolute_test_tmp}/out" 2>&1; then
    absolute_result=0
  else
    absolute_result=$?
  fi
  exit "$absolute_result"
) 2>&1)"; then
  absolute_rc=0
else
  absolute_rc=$?
fi
check assert_nonzero "$absolute_rc" "extract rejects absolute-path archive entry"

mkdir -p "${extract_dir}/traversal"
printf '#!/usr/bin/env bash\necho trav\n' > "${extract_dir}/traversal/asylum"
chmod +x "${extract_dir}/traversal/asylum"
(
  cd "${extract_dir}/traversal"
  tar -czf "${extract_dir}/traversal.tar.gz" ../traversal/asylum
)
if traversal_output="$(asylum_extract_binary "${extract_dir}/traversal.tar.gz" "${extract_dir}/traversal-out" 2>&1)"; then
  traversal_rc=0
else
  traversal_rc=$?
fi
check assert_nonzero "$traversal_rc" "extract rejects traversal archive entry"

checksum_probe_dir="$(mktemp -d)"
checksum_probe_archive="${checksum_probe_dir}/archive.bin"
: > "$checksum_probe_archive"
if checksum_probe_output="$((
  asylum_download() {
    printf 'curl: (22) failed\\n' >&2
    return 22
  }
  asylum_download_quiet() {
    return 22
  }
  asylum_verify_archive v0.0.0 asylum-check.tar.gz "$checksum_probe_archive" "$checksum_probe_dir"
) 2>&1)"; then
  checksum_probe_rc=0
else
  checksum_probe_rc=$?
fi
check assert_eq "$checksum_probe_rc" "0" "checksum verification skipped when probe files missing"
check assert_contains "$checksum_probe_output" "skipped" "checksum probe returns skipped status"
check assert_not_contains "$checksum_probe_output" "curl" "checksum probe suppresses curl errors"

checksum_verified_dir="$(mktemp -d)"
checksum_verified_archive="${checksum_verified_dir}/archive.bin"
printf 'archive-payload' > "$checksum_verified_archive"
checksum_hash_cmd="$(asylum_hash_command)"
if [[ "$checksum_hash_cmd" != "none" ]]; then
  if [[ "$checksum_hash_cmd" == "sha256sum" ]]; then
    checksum_expected="$(sha256sum "$checksum_verified_archive" | awk '{print $1}')"
  else
    checksum_expected="$(shasum -a 256 "$checksum_verified_archive" | awk '{print $1}')"
  fi
  printf '%s  archive.bin\n' "$checksum_expected" > "${checksum_verified_dir}/checksums.txt"
  if checksum_verified_output="$((
    asylum_download_quiet() {
      cp "${checksum_verified_dir}/checksums.txt" "$2"
    }
    asylum_verify_archive v0.0.0 archive.bin "$checksum_verified_archive" "$checksum_verified_dir"
  ) 2>&1)"; then
    checksum_verified_rc=0
  else
    checksum_verified_rc=$?
  fi
  check assert_eq "$checksum_verified_rc" "0" "checksum verification succeeds for matching hash"
  check assert_contains "$checksum_verified_output" "verified" "checksum verification reports verified for matching hash"
fi

checksum_missing_tool_dir="$(mktemp -d)"
checksum_missing_tool_archive="${checksum_missing_tool_dir}/archive.bin"
: > "$checksum_missing_tool_archive"
printf 'abc  archive.bin\n' > "${checksum_missing_tool_dir}/checksums.txt"
if missing_tool_output="$((
  PATH="${checksum_missing_tool_dir}"
  if asylum_verify_with_checksum_file "${checksum_missing_tool_dir}/checksums.txt" "$checksum_missing_tool_archive" "archive.bin"; then
    missing_tool_rc=0
  else
    missing_tool_rc=$?
  fi
  exit "$missing_tool_rc"
) 2>&1)"; then
  missing_tool_rc=0
else
  missing_tool_rc=$?
fi
check assert_eq "$missing_tool_rc" "0" "checksum verification succeeds when hash tools are missing"
check assert_contains "$missing_tool_output" "skipped" "checksum verification reports skipped when hash tools are missing"

if [[ $failures -gt 0 ]]; then
  echo "FAILED: ${failures} checks failed"
  exit 1
fi

echo "All checks passed"
exit 0
