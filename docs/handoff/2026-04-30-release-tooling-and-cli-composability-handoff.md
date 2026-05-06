# Release tooling delivered + CLI composability proposal — 2026-04-30 handoff

This handoff captures session state for a fresh agent picking up after context compaction. Three threads are in play: (1) release tooling work that just shipped, (2) the user's machine state for an upcoming end-to-end install test, and (3) an open architectural proposal around CLI composability and a new `asylum uninstall` command.

Architecture note: crate layout and daemon-entry details are superseded by [the finalized architecture refactor spec](../reviews/2026-05-04-asylum-architecture-refactor-spec.md): one installed `asylum` binary, `asylum daemon run` for foreground daemon mode, no `asylum serve`, `asylum`/`asylum-cli`/`asylum-daemon`/`asylum-types` crates, and Unix-socket CLI/MCP local control at `~/.asylum/run/asylum.sock`.

## Release status

Doc-only / internal — no release needed for this handoff at the time.

The later release state is tracked in [RELEASES.md](../../RELEASES.md). Latest release at the time of the 2026-05-05 audit: v0.1.6.

## What shipped since the previous handoff (cockpit-deliverability)

Four release-tooling commits on `main`:

1. **`05a9882`** — `RELEASES.md` ledger + release-tracking convention added to `AGENTS.md`. Establishes manual release tracking ("merged to main" ≠ "shipped to users"); every delivery doc now must include a Release status section.
2. **`b2ff5ec`** — `build-release-artifacts.sh` made cross-host capable; agent release-cut policy softened (agents track, user cuts).
3. **`49ef71f`** — All four release targets cross-built from Linux x86_64 via `ghcr.io/rust-cross/cargo-zigbuild` (image digest pinned). No QEMU, no osxcross, no apt installs needed beyond Docker. Removed obsolete `require_macos_host` and `require_emulation_for_platform` preflights.
4. **`86f8bc3`** — Minisign signature verification enabled at install time. Public key embedded in `scripts/install.sh` as `ASYLUM_RELEASE_PUBKEY_DEFAULT`. Private key lives at `~/.config/asylum/release-signing.key` (chmod 600, outside repo) and is consumed by `publish-release.sh` via `ASYLUM_RELEASE_SIGNING_KEY` (exported from the user's `~/.zshrc`).

Side-effect deliveries (not in repo):

- **Sudo askpass wrapper** added to `~/.zshrc` so `! sudo …` works from Claude Code (and any non-TTY shell). Routes through `ksshaskpass` when no TTY; defers to plain `sudo` when a real terminal exists. `~/.zshenv` and `~/.zprofile` guards extended to load `.zshrc` for both Codex (`CODEX_THREAD_ID`) and Claude Code (`CLAUDECODE`); old `_CODEX_ZSHRC_LOADED_LOCAL` guard renamed to `_AGENT_ZSHRC_LOADED_LOCAL` consistently across both files (was a regression caught and fixed before commit).
- **`/doctor` rust-analyzer LSP plugin crash fixed**: `~/.cargo/bin/rust-analyzer` is a rustup proxy that errored because the actual `rust-analyzer` rustup component wasn't installed. Fix: `rustup component add rust-analyzer` (now installed at version `1.95.0`). User reloaded plugins; `/doctor` now clean.

## User's machine state (for upcoming install-flow test)

Confirmed via direct inspection on 2026-04-30:

| Path / item | State |
|---|---|
| `~/.local/bin/asylum` | Binary, **version 0.1.1** (12 MB, predates v0.1.2). On PATH. |
| `~/.asylum/` | Exists. Contains only `logs/` and `run/` (empty). Created by previous `asylum setup`. |
| `~/.config/asylum/` | Exists but contains **only the user's release-signing keys** (`release-signing.key`, `release-signing.pub`). **DO NOT DELETE THIS DIR** during any wipe. |
| Daemon process | Not running. |
| Systemd unit | Not installed. (`asylum install systemd` is a generator, not an installer — it only prints unit text to stdout.) |
| PATH entries in shell rc | `~/.local/bin` already on PATH from before asylum existed; no asylum-specific PATH addition to remove. |

**To wipe for a clean re-install test (user has not yet authorized):**
```bash
rm -f ~/.local/bin/asylum
rm -rf ~/.asylum
# Do NOT touch ~/.config/asylum — signing keys live there.
```

## The four open threads (in the order proposed to the user)

1. **Try `asylum update` first** to see if 0.1.1 → 0.1.2 self-update works end-to-end. The 0.1.1 installer had no embedded pubkey, but `asylum update` re-fetches `install.sh` from `main` (which now has the pubkey), so signature verification kicks in transparently on this jump.
2. **Wipe the install** (commands above) once user authorizes.
3. **Run `install.sh` fresh from v0.1.2** to verify the signed-install path works on a real machine.
4. **Then write up a CLI-composability + uninstall plan** at `docs/reviews/2026-04-30-cli-composability-and-uninstall.md` (proposal sketch below) for the user to review before any code changes.

User's most recent direction: persist this state and start a fresh session, so step 1 has not yet been executed. Resume there.

## CLI composability + uninstall proposal (sketch only — not yet a plan doc)

User explicitly wants composable CLI commands where install/uninstall/update/doctor share machinery via the binary itself, not duplicate logic in shell. They also want a friendly `asylum uninstall` that confirms before acting. North-star architecture proposed:

**Layer 1 — shared inspection library (in `crates/asylum-cli/`):**
- A `HostState` struct: binary location + version, daemon state (running/stopped/missing), `~/.asylum` contents, ports in use, systemd unit presence, cockpit assets, etc.
- Returned both as a Rust struct (in-process callers) and via `--json` (shell callers).

**Layer 2 — command surface, all wired through Layer 1:**
- `asylum status` (exists) → render `HostState` for humans
- `asylum status --json` → machine-parseable
- `asylum doctor` (exists) → checks against `HostState` + extras
- `asylum setup` (exists) → idempotent post-install dance; uses `HostState` to know what's already done
- **`asylum uninstall` (new)** → reads `HostState`, shows a plan, confirms, executes. `--keep state|config|logs` flags for granular preservation. Single command — do **not** introduce per-component uninstall (`asylum uninstall daemon` etc.) at v0.1.x; daemon and cockpit ship in one binary.
- `asylum update` (exists) → uses `HostState` for current version, fetches latest, replaces binary, calls `setup` and `doctor` post-replace
- **Rename current `asylum install systemd|launchd`** (which only prints unit text) to something like `asylum service generate systemd|launchd` to free the `install` namespace for actual self-install / post-bootstrap use.

**Layer 3 — `scripts/install.sh` becomes a thin bootstrap:**
1. Detect OS/arch, download + verify + place the binary.
2. Hand off: `exec "$INSTALL_DIR/asylum" setup`.

After bootstrap, **every** lifecycle operation lives in the binary. Shell exists only because of the chicken-and-egg.

**Self-discoverability ergonomics (cheap wins):**
- Make `asylum` (no args) print a "what is this, what's running, what to do next" banner — same data as `status`, framed for first-run discoverability.
- Add clap `#[command(after_help = "Examples:\n  …")]` to each subcommand. Makes the CLI feel "discoverable without docs."

This is the **shape**, not the build plan. The plan write-up is step 4 above.

## Local dev workflow (taught to the user; recap)

For ergonomic iteration on the CLI without cutting releases:

| Approach | When | Command |
|---|---|---|
| Direct invocation | Quick one-offs | `cargo run --release -p asylum -- status` |
| Fresh binary on PATH | Want `asylum` to behave like the installed version | `cargo install --path crates/asylum --force` (overwrites `~/.cargo/bin/asylum`; check `which asylum` for shadowing vs `~/.local/bin/asylum`) |
| **Live-build symlink (most ergonomic)** | Iterating on CLI | `ln -sf "$PWD/target/release/asylum" ~/.local/bin/asylum`; then `cargo build --release -p asylum` and the next `asylum` invocation is the new code |

Daemon dev: `cargo run -p asylum -- daemon run`. Cockpit dev: `npm --prefix cockpit run dev` (Vite hot reload, proxies `/api/*` to daemon).

Runtime files asylum needs: just `~/.asylum/` (created by `asylum setup`, idempotent). Whether the binary got there via install.sh or `cargo build`, you run `asylum setup` once.

`scripts/install.sh` does **not** currently support pointing at a local tarball. Two paths if useful for E2E install testing:
1. Use a fork: `ASYLUM_REPO_SLUG=YourFork/Asylum bash scripts/install.sh --version v0.1.x-rc1` against a draft pre-release.
2. Add `--from-local <archive>` to `install.sh` (~20 lines). Worth doing if the user wants a clean way to dogfood release tarballs without GitHub round-trips.

## What a fresh agent should do on resume

1. Read this handoff and `RELEASES.md`. No code changes have been made since `86f8bc3`.
2. Confirm machine state still matches "User's machine state" table above (run `asylum --version`, check `~/.local/bin/asylum`, `~/.asylum`, `~/.config/asylum`).
3. Pick up at thread 1: ask the user whether to run `asylum update` as the first test of the new release tooling, or to skip straight to the wipe + fresh install.
4. After threads 1–3, propose writing the CLI-composability plan as a `docs/reviews/2026-04-30-cli-composability-and-uninstall.md` doc and iterate with the user before any refactor.

Do **not** start refactoring `cli.rs` autonomously — the architectural proposal needs alignment first.
