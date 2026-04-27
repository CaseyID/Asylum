---
title: Asylum Binary Installer And First-Run UX Design
status: Approved direction, ready for implementation planning
date: 2026-04-27
branch: installer-work
---

# Asylum Binary Installer And First-Run UX Design

## Purpose

Asylum should be as simple to install and start using as Hermes Agent. The normal user path should not require cloning the repository, building Rust, building Cockpit, writing service files, passing database paths, or learning daemon internals.

The public install command is:

```bash
curl -fsSL https://raw.githubusercontent.com/caseyID/Asylum/main/scripts/install.sh | bash
```

After install, `asylum` must be available on `PATH`. The primary command should take the user to the product.

## Product Model

Hermes maps bare `hermes` to its primary TUI. Asylum's primary surface is Cockpit backed by the local control plane, so bare `asylum` should mean "take me to Asylum."

The happy path is:

```bash
curl -fsSL https://raw.githubusercontent.com/caseyID/Asylum/main/scripts/install.sh | bash
asylum
```

Running `asylum` should:

1. Check whether first-run setup has completed.
2. Run setup if required.
3. Start the local control plane if it is not already running.
4. Wait for `/api/health`.
5. Open Cockpit in the browser.
6. Print the Cockpit URL and concise status.

## Installer Design

Create `scripts/install.sh` as a binary release installer. It should not clone the repo or build with Cargo in the normal flow.

Responsibilities:

- Detect OS and CPU architecture.
- Resolve the requested release version, defaulting to the latest GitHub Release.
- Download the matching release archive.
- Verify checksum when a checksum file exists.
- Install the `asylum` binary into a user-writable PATH location, defaulting to `~/.local/bin`.
- Create Asylum's home/config/log directories.
- Repair shell PATH where practical, or print exact shell-specific instructions.
- Run or offer first-run setup in interactive mode.
- Run `asylum doctor` after install unless skipped.
- Print a short next-step panel centered on `asylum`.

The installer should have a polished CLI feel: banner, color when stdout is a TTY, clear step labels, concise success/failure messages, and actionable remediation. It should model Hermes' friendliness without copying its internals blindly.

Installer flags:

```text
--help
--version <tag>
--install-dir <path>
--asylum-home <path>
--yes
--skip-setup
--skip-doctor
--no-color
```

Default interactive behavior should run setup or ask to run setup immediately. Non-interactive behavior should install the binary and print `asylum` as the next command.

## Release Artifact Contract

Releases are uploaded manually for now. The installer assumes predictable GitHub Release assets.

Recommended archive names:

```text
asylum-darwin-arm64.tar.gz
asylum-darwin-x86_64.tar.gz
asylum-linux-arm64.tar.gz
asylum-linux-x86_64.tar.gz
```

Each archive should contain:

```text
asylum
README.md or INSTALL.md
LICENSE
```

Optional checksum files:

```text
checksums.txt
asylum-darwin-arm64.tar.gz.sha256
```

The installer should prefer checksum verification when published, but early manual releases may omit checksums. In that case the installer should say verification was skipped rather than pretending it happened.

## Installed Layout

Default user install:

```text
~/.local/bin/asylum
~/.asylum/config.toml
~/.asylum/asylum.sqlite3
~/.asylum/logs/asylum.log
~/.asylum/run/asylum.pid
```

The CLI should also continue to support the existing XDG config path where useful, but the simple product path should consistently treat `~/.asylum` as the user-facing Asylum home. Advanced flags and environment variables can override paths for development, testing, and operations.

## Command UX

Primary commands:

```text
asylum             setup if needed, start if needed, open Cockpit
asylum setup       first-run setup or reconfiguration wizard
asylum cockpit     open Cockpit; start the control plane if needed
asylum start       start the full local control plane
asylum stop        stop the local control plane
asylum restart     restart the local control plane
asylum status      friendly health summary
asylum doctor      detailed diagnostics and fixes
asylum logs        tail control-plane logs
asylum update      download and install the latest release binary
```

Existing power-user commands remain available:

```text
asylum serve
asylum config
asylum node
asylum graph
asylum attach
asylum token
asylum notify
asylum mcp
```

Docs should lead with the primary commands. Advanced commands should be documented as operator/developer surfaces.

## Service Model

Do not expose `asylum service ...` as part of the normal UX. Service-manager behavior is implementation detail behind `asylum start`, `asylum stop`, `asylum restart`, and `asylum status`.

Behavior by platform:

- macOS: prefer a user LaunchAgent for persistent background operation.
- systemd Linux: prefer a user service.
- Other Linux or unsupported service manager: fall back to a managed background process with PID and log files under `~/.asylum`.

`asylum start` starts the whole local control plane: daemon, API, Cockpit web server, node runtime registry, notification polling, and background supervision. Because the daemon serves Cockpit, starting Asylum makes Cockpit available.

`asylum cockpit` should be explicit UI intent. It should start Asylum if needed, wait for health, then open the browser.

Bare `asylum` should behave like `asylum cockpit` plus first-run setup.

## Setup Wizard

`asylum setup` should be short and opinionated.

It should:

- Create missing directories and config.
- Choose localhost defaults.
- Choose a default database path under `~/.asylum`.
- Detect Codex and Claude Code commands on PATH.
- Detect optional Loon configuration without requiring it.
- Detect optional ntfy configuration without requiring it.
- Configure owner-token auth only when needed by the chosen mode.
- Explain what is missing in plain language.
- End with the next command: `asylum`.

Setup must not require normal users to pass `--database`, `--config`, `--bind`, token env vars, or path flags just to begin.

## Doctor

`asylum doctor` is the diagnostic source of truth. It should power installer final checks and first-run readiness checks.

Initial checks:

- `asylum` binary is executable and version is known.
- Install directory is on PATH.
- Asylum home exists and is writable.
- Config exists and parses.
- Database path parent is writable.
- Control-plane port is available or already owned by Asylum.
- `/api/health` responds when Asylum is running.
- Cockpit assets are present in the release binary or otherwise servable.
- Codex command is available or clearly marked missing.
- Claude Code command is available or clearly marked missing.
- Loon is disabled, configured, or unreachable with a clear reason.
- ntfy is disabled, configured, or unreachable with a clear reason.
- LaunchAgent/systemd/background process state is understandable.

Output should be concise by default and detailed when invoked with `--verbose`.

## Update

`asylum update` should reuse the same release resolution and download logic as `scripts/install.sh`.

Behavior:

- Determine current version.
- Resolve latest release unless a version is provided.
- Download the matching archive.
- Replace the current binary atomically where possible.
- Restart the control plane if it was running.
- Re-run `asylum doctor`.

## Documentation

README should lead with the one-line installer and the bare `asylum` command.

Recommended structure:

1. Install
2. Start Asylum
3. What `asylum` does
4. Core commands
5. Updating
6. Troubleshooting with `asylum doctor`
7. Advanced CLI/API/MCP/operator commands
8. Manual release artifact expectations

Avoid leading with build commands. Source build instructions belong in a development section.

## Non-Goals

- No normal source-build fallback in `scripts/install.sh`.
- No CI/CD or release automation in this pass.
- No hosted installer service.
- No requirement that users understand launchd, systemd, PID files, database paths, owner tokens, or bind addresses for first use.
- No public `asylum service ...` workflow in the friendly path.

## Implementation Planning Notes

The next implementation plan should cover:

- `scripts/install.sh` with release download logic and Hermes-style UX.
- CLI reshaping for bare `asylum`, `setup`, `cockpit`, `start`, `stop`, `restart`, `status`, `doctor`, `logs`, and `update`.
- A small internal service manager abstraction for launchd, systemd user services, and PID fallback.
- Config/home path normalization around `~/.asylum`.
- Embedded Cockpit assets in release builds so the binary is self-contained.
- Tests for command dispatch, setup idempotency, doctor checks, and service manager rendering.
- Shell-based installer tests for platform/arch mapping and release URL construction.
- README rewrite around the new install path.
