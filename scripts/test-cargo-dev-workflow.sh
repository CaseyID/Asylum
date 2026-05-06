#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config="${repo_root}/.cargo/config.toml"
vite_config="${repo_root}/cockpit/vite.config.ts"
cli_source="${repo_root}/crates/asylum-cli/src/cli.rs"
workspace_manifest="${repo_root}/Cargo.toml"
xtask_manifest="${repo_root}/xtask/Cargo.toml"
xtask_source="${repo_root}/xtask/src/main.rs"

fail() {
  echo "fail: $*" >&2
  exit 1
}

[[ -f "$config" ]] || fail "missing .cargo/config.toml"
[[ -f "$xtask_manifest" ]] || fail "missing xtask/Cargo.toml"
[[ -f "$xtask_source" ]] || fail "missing xtask/src/main.rs"
grep -q '"xtask"' "$workspace_manifest" || fail "xtask must be a workspace member"

for alias in dev dev-daemon dev-cockpit build-stack test-stack run-stack start-stack stop-stack restart-stack status-stack doctor-stack logs-stack; do
  grep -Eq "^${alias}[[:space:]]*=[[:space:]]*\"run -p xtask --" "$config" \
    || fail "cargo alias must route through xtask: $alias"
done

grep -Eq "Command::Dev|enum DevCommand|run_dev_command|source_checkout_dev_script" "$cli_source" \
  && fail "developer workflow must not be wired into the asylum product CLI"
grep -R "asylum-dev" "$repo_root/crates" >/dev/null 2>&1 \
  && fail "developer workflow launcher must not be attached to product crates"

grep -q '"/api"' "$vite_config" || fail "vite dev config must proxy /api"
grep -q 'ws: true' "$vite_config" || fail "vite dev config must proxy websockets"
grep -q '127.0.0.1:7788' "$vite_config" || fail "vite dev proxy must default away from the installed daemon port"
grep -q 'ASYLUM_COCKPIT_DEV_PORT' "$xtask_source" || fail "xtask must allow overriding Vite port"
grep -q 'DEFAULT_DEV_BIND: &str = "127.0.0.1:7788"' "$xtask_source" || fail "xtask must default source daemon away from the installed daemon port"
grep -q 'DEV_HOME_DIR: &str = ".asylum-dev"' "$xtask_source" || fail "xtask must default source runtime into repo-local state"
grep -q 'cargo_target_dir' "$xtask_source" || fail "xtask must respect Cargo target directory"
grep -q -- '--strictPort' "$xtask_source" || fail "xtask must not silently move Vite ports"
grep -q '^.asylum-dev/' "${repo_root}/.gitignore" || fail "repo-local source runtime must be ignored"

help_output="$(cargo run -p xtask -- help)"
for command in dev dev-daemon dev-cockpit build-stack test-stack run-stack start-stack stop-stack restart-stack status-stack doctor-stack logs-stack help; do
  grep -Eq "(^|[[:space:]])${command}([[:space:]]|$)" <<<"$help_output" \
    || fail "help output does not mention $command"
done

echo "cargo dev workflow checks passed"
