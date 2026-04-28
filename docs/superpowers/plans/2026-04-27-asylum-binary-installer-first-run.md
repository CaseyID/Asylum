# Asylum Binary Installer And First-Run UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the approved binary installer and first-run UX spec into a usable product path: `curl ... | bash`, then `asylum`.

**Architecture:** Keep the shell installer responsible for GitHub release asset discovery, download, install, PATH guidance, and post-install checks. Keep Rust responsible for runtime home/config paths, first-run setup, daemon lifecycle, diagnostics, Cockpit launch, update orchestration, and release asset serving. The friendly commands wrap existing power-user commands rather than deleting them.

**Tech Stack:** Bash installer, Rust CLI with clap/tokio/reqwest, axum/tower-http daemon serving Cockpit, npm/Vite Cockpit build.

---

## Task 1: Binary Release Installer

**Files:**
- Create: `scripts/install.sh`
- Create: `scripts/test-install.sh`

**Requirements:**
- Implement the public installer command target at `scripts/install.sh`.
- Do not clone the repository or build Cargo in the normal flow.
- Support flags: `--help`, `--version <tag>`, `--install-dir <path>`, `--asylum-home <path>`, `--yes`, `--skip-setup`, `--skip-doctor`, `--no-color`.
- Detect `darwin|linux` and `arm64|x86_64` and map to assets named `asylum-<os>-<arch>.tar.gz`.
- Resolve latest GitHub release when no version is supplied.
- Download the release archive, verify checksum when `checksums.txt` or `<archive>.sha256` is available, and clearly report when checksum verification is skipped.
- Install `asylum` into `~/.local/bin` by default.
- Create `~/.asylum`, `~/.asylum/logs`, and `~/.asylum/run` by default or equivalent paths under `--asylum-home`.
- Run or offer `asylum setup` in interactive mode unless skipped.
- Run `asylum doctor` unless skipped.
- Print concise next steps centered on `asylum`.
- Add shell tests for platform/arch mapping, asset URL construction, and option parsing without making network calls.

## Task 2: First-Run CLI And Service Manager

**Files:**
- Modify: `crates/asylum/src/cli.rs`
- Modify: `crates/asylum/src/client.rs`
- Create: `crates/asylum/src/runtime.rs`
- Create: `crates/asylum/src/service.rs`
- Modify: `crates/asylum/src/main.rs`
- Modify as needed: `crates/asylum/Cargo.toml`

**Requirements:**
- Make bare `asylum` behave like `asylum cockpit` plus first-run setup.
- Keep existing advanced commands: `serve`, `config`, `install`, `node`, `graph`, `attach`, `token`, `notify`, `mcp`.
- Add friendly commands: `setup`, `cockpit`, `start`, `stop`, `restart`, `status`, `doctor`, `logs`, `update`.
- Normalize product paths around `ASYLUM_HOME` or `~/.asylum`: `config.toml`, `asylum.sqlite3`, `logs/asylum.log`, `run/asylum.pid`.
- Keep optional `ASYLUM_CONFIG`, `ASYLUM_DATABASE`, `ASYLUM_BIND`, `ASYLUM_BASE_URL`, and CLI overrides for development/operator use.
- `setup` must create missing directories and config, choose localhost defaults, choose database under Asylum home, detect Codex and Claude commands on PATH, leave Loon and ntfy optional, and end with `asylum`.
- `start` must start the local control plane if it is not healthy, using a small service manager abstraction for launchd, systemd user services, and background PID fallback.
- `stop` and `restart` must route through the same service manager abstraction.
- `status` must print a friendly health summary.
- `doctor` must print concise checks for binary/version, PATH, home/config/database writability, health, Cockpit assets, harness command availability, optional Loon/ntfy state, and service state. Add `--verbose`.
- `logs` must print or tail the control-plane log path.
- `update` must invoke the installer with `--version` when supplied and rerun `doctor`.
- Add Rust unit tests for command parsing/dispatch shape, setup idempotency helpers, doctor check classification, and service manager rendering.

## Task 3: Embedded Cockpit Assets For Release

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/asylum-daemon/Cargo.toml`
- Modify: `crates/asylum-daemon/src/app.rs`

**Requirements:**
- In debug/dev builds, keep serving `cockpit/dist` from disk.
- In release builds, embed built Cockpit assets so the `asylum` binary can serve Cockpit without a source checkout.
- Serve `/` from embedded `index.html` in release builds.
- Serve `/assets/*` from embedded release assets.
- Keep the existing development error message when `cockpit/dist` is missing in debug builds.
- Document any new crate dependency with a short Cargo comment if the workspace style allows it.

## Task 4: README Product Path Refresh

**Files:**
- Modify: `README.md`

**Requirements:**
- Lead with the one-line installer:
  `curl -fsSL https://raw.githubusercontent.com/caseyID/Asylum/main/scripts/install.sh | bash`
- Make the next command `asylum`.
- Explain what bare `asylum` does: setup if needed, start if needed, wait for health, open Cockpit, print the URL.
- Document core commands: `setup`, `cockpit`, `start`, `stop`, `restart`, `status`, `doctor`, `logs`, `update`.
- Move source build and existing advanced CLI/API/MCP/operator commands below the product path.
- Include manual release artifact expectations and checksum behavior.

## Final Verification

Run these commands after all tasks and fixes:

```bash
bash scripts/test-install.sh
cargo fmt --check
cargo test
npm --prefix cockpit test
npm --prefix cockpit run build
cargo build --release
```

If `cargo build --release` requires built Cockpit assets first, run the npm build before it and keep that ordering in README.
