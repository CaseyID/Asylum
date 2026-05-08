# Changelog

## 0.1.10 — 2026-05-07

### Added

- Cockpit `NodeSession` now renders a real xterm.js terminal (via the previously-installed `@xterm/xterm` and `@xterm/addon-fit` deps), decoding ANSI/VT escape sequences and accepting raw keystroke input. The "tui / struct" toggle keeps the structured transcript available alongside.
- Browser attach (`/attach/<token>`) now serves a self-contained xterm.js HTML page that connects to the existing attach WebSocket, replacing the prior `text/plain` JSON stub.
- Daemon login-shell PATH fallback: at startup, the daemon probes `sh -lc 'echo $PATH'` once and uses it as a fallback when a harness binary isn't on the process PATH.
- `asylum service generate systemd` now bakes the install-time interactive PATH into the unit's `Environment=PATH=...` so the daemon can find harness binaries installed under `~/.local/bin`, nvm, pyenv, etc.
- `asylum doctor` now distinguishes "binary missing locally", "binary on local PATH but not in daemon PATH", and "binary found by both", with actionable guidance for each case.

### Fixed

- Failed local harness spawns now transition the node to `Failed` and record a `HarnessFailure` event instead of leaving phantom rows stuck at `Starting` forever.
- Launch-packet textarea is now actually delivered to the harness as the first user turn on the local substrate (passed as the positional `[PROMPT]` argument to both codex and claude). The label is no longer a lie.
- Codex and Claude Code workspaces are pre-trusted before launch (idempotent upserts to `~/.codex/config.toml` `[projects."<ws>"]` and `~/.claude.json` `projects[<ws>].hasTrustDialogAccepted`), including walking up to the git root for paths like `/tmp/<ws>` that share `/tmp/.git`. Combined with `--dangerously-bypass-approvals-and-sandbox` (codex) and `--dangerously-skip-permissions` (claude), fresh nodes no longer sit at trust prompts.
- Decision-line ingester's carry buffer is now flushed on each PTY read via `flush_partial`, so TUI bytes (which never include newlines) reach `transcript_chunks`. Replay-on-connect now actually replays.
- Daemon now detects harness self-exit: when a child process exits, the local substrate removes its runtime entry and transitions the node liveness `Running → Stopped`. Previously, send-input on a self-exited node returned "node not running" while the UI still showed `running`.
- Cockpit "open cli" and "read the spec" first-run buttons now do something — `open cli` opens a CLI-command modal, `read the spec` opens a modal showing the spec path with optional clipboard copy.
- Successful node launch now navigates to the new node's detail screen instead of `/fleet`.
- Send-input from the cockpit chat box renders optimistically; the daemon's `input_sent` echo is deduped client-side.
- Cockpit "logs & telemetry" screen renamed to "notifications" (matches what it actually shows); empty-state copy added on Notifications and on Hooks rules/firings tabs.
- Cockpit copy for unavailable harnesses changed from "future · adapter not built" to "binary `<name>` not found on daemon PATH — run `asylum doctor`", correctly reflecting the actual condition.

## 0.1.9 — 2026-05-07

### Added

- Added repo-local `cargo dev`, `cargo build-stack`, `cargo test-stack`, and `cargo run-stack` workflows backed by an `xtask` crate.
- Added Cargo aliases for installed product lifecycle checks (`cargo status-stack`, `cargo doctor-stack`, and related commands) that delegate to the installed `asylum` binary.
- Added Vite API/WebSocket proxying for Cockpit source development.

### Fixed

- Source dev/runtime commands now default to `.asylum-dev/` and `127.0.0.1:7788` so they do not collide with the installed `~/.asylum` daemon on `127.0.0.1:7717`.
- `cargo dev-daemon` now respects Cargo's target directory instead of assuming `target/debug/asylum`.

## 0.1.8 — 2026-05-06

### Fixed

- Fixed a Cockpit attach replay loop where historical `attach_issued` events were treated like fresh user attach clicks, causing Firefox to block a flood of popup windows.
- Renamed operator-facing attach labels from implementation terms like "browser attach" and "native attach" to clearer attach-tab / terminal wording.

## 0.1.7 — 2026-05-06

### Added

- Completed the current-spec coverage audit and added a docs map plus a single findings record for the audit/fix pass.
- Added first-class decision workflows across daemon API, CLI, MCP, and Cockpit, including create/list/inspect/approve/deny and node-linked resolution feedback.
- Added CLI/MCP coverage for graph relationships, channels, hooks, recipes, notifications, workspace/context, remote commands, and node fork.
- Added Cockpit workflows for Decisions, graph relationship create/remove, notification read/open-node, and manual channel inbound.
- Added local stdout-line decision ingestion for harnesses via `@@asylum:decision.request` control lines.

### Changed

- Improved Cockpit terminal/session UX with multiline input, shared attach/native/interrupt controls, event-backed Activity, clearer telemetry labels, responsive layout fixes, and populated-state/responsive regression coverage.
- Added inbound channel remote-command execution with token validation and durable ntfy reply correlation.
- Made Loon unsupported paths more honest: unavailable operations fail before mutating state, attach/observe copy discloses limitations, and Loon no longer advertises structured events without a real event stream.
- `status`, `status --json`, and `doctor` now surface effective non-loopback/all-interface HTTP exposure warnings.

### Fixed

- Unconfigured notify send now returns an explicit service-unavailable error.
- Hook `tool transcript.checkpoint` now reports unsupported instead of a successful no-op.
- Cockpit Settings now surfaces daemon API failures instead of silently rendering empty panels.
- `status --json` reports an explicit empty Cockpit cache list when no separate cache paths exist.

## 0.1.6 — 2026-05-05

### Fixed

- `asylum update` now refreshes an already-installed launchd/systemd service definition even when the daemon is stopped, so stale commands like the removed `asylum serve` cannot survive a binary update.
- Post-update doctor output now runs through the freshly installed `asylum` binary instead of the still-running self-replaced process, avoiding misleading `(deleted)` binary paths and old version strings immediately after update.

## 0.1.5 — 2026-05-05

### Changed

- Reshaped the Rust workspace around the finalized architecture: `asylum` is now the tiny single-binary composition crate, `asylum-cli` owns CLI/MCP/local lifecycle behavior, `asylum-daemon` owns the resident control process, and `asylum-types` owns shared data contracts.
- Replaced foreground daemon mode `asylum serve` with `asylum daemon run`. Service templates and PID identity checks now use the new command shape.
- Moved local CLI/MCP daemon control to the Unix socket at `~/.asylum/run/asylum.sock`, with `ASYLUM_SOCKET_PATH` override support. Cockpit remains on daemon HTTP/WebSocket.
- Moved daemon run configuration loading into `asylum-daemon`, including bind/database/socket/base URL/env/CLI merge behavior.

### Added

- Daemon health now reports the effective Cockpit `base_url` and local `socket_path`.
- Socket health integration coverage verifies local Unix-socket control bypasses HTTP bearer-token auth while HTTP remains protected.

### Removed

- `asylum serve` is gone; there is no compatibility alias.
- `asylum-core` crate name is gone; shared contracts now live in `asylum-types`.

## 0.1.4 — 2026-04-30

### Added

- **Unified ASYLUM block-letter banner** on first-run greeting moments: bare `asylum` invocation when `~/.asylum` does not exist, and the welcome screen after first-time `asylum setup`. Same ANSI Shadow art as the installer, centralized as the `ASYLUM_BANNER` const + `print_asylum_banner()` helper.

### Fixed

- **Installer banner ASCII art** — replaced the long-broken figlet output (a leftover from a prior project name) with proper block letters that actually spell ASYLUM.
- **`asylum update` from v0.1.2 and earlier** — `scripts/install.sh` now silently accepts `--skip-doctor` and `--yes` as no-op flags so older `asylum update` invocations can self-upgrade through to the new installer instead of erroring on unknown options.

## 0.1.3 — 2026-04-30

### Added

- **`HostState` introspection layer** (`crates/asylum/src/host.rs`) — single struct describing the machine from Asylum's perspective: binary location/version/PATH state, runtime dir contents, config dir contents, daemon state and backend, service-unit presence, port probe. Cheap to construct, side-effect-free, `Serialize`. Folds in the former `service.rs` (deleted). Carries `schema_version: 1`.
- **`asylum status --json`** — emits the full `HostState` as machine-parseable JSON. Human rendering also gains binary-path, PATH-shadow, service-unit, and port-probe lines.
- **`asylum uninstall`** — friendly removal command with plan / confirm / execute flow. Flags: `--yes`, `--keep state|config|logs` (repeatable), `--purge`, `--dry-run`, `--json`. `--purge` always requires typed-name confirmation (user types `asylum`), even with `--yes`. Default preserves `~/.config/asylum/` (signing keys live there). Binary removal is guarded against non-bin parent dirs so a dev workspace `target/release/asylum` is not deleted.
- **`asylum service generate {systemd|launchd}`** — replaces the misnamed `asylum install {systemd|launchd}`. Same output (unit text on stdout). Old form deleted outright; no alias.
- **First-run banners** — bare `asylum` invocation prints a "what is this, what's next" banner only when `~/.asylum` does not exist. `asylum setup` prints a welcome banner on first-time setup, including PATH guidance when the binary is not on the shell PATH.
- **`#[command(after_help = …)]` example blocks** on the major user-facing subcommands (`setup`, `cockpit`, `start`, `stop`, `status`, `doctor`, `update`, `service`, `uninstall`, `attach`).
- **`about` descriptions** on every top-level subcommand so `asylum --help` lists each with a one-line summary.

### Changed

- **`scripts/install.sh` shrank from 857 to 593 lines.** Now responsible only for OS/arch detection, download, signature verification, binary placement, then `exec asylum setup`. Dropped: prompt for setup, `asylum doctor` invocation, shell-rc PATH editor, "next steps" printer, `--yes`/`--skip-doctor` flags. Post-install UX now lives in `asylum setup`'s first-run banner.
- **`asylum doctor` and `asylum setup`** refactored onto `HostState`; duplicated detection logic deleted.
- **Bare `asylum`** with `~/.asylum` missing now exits cleanly after printing the banner instead of falling through to `Cockpit`.

### Removed

- **`asylum install systemd|launchd`** — replaced by `asylum service generate …`. No deprecation alias.
- **`crates/asylum/src/service.rs`** — folded into `host.rs`.
- **`scripts/install.sh` `--yes` and `--skip-doctor` flags** — no longer meaningful.

## 0.1.2 — 2026-04-29

### Fixed

**Security / correctness (Highs)**
- H1: token revocation now takes effect without daemon restart — revoked DB tokens are rejected on every request
- H2: hook dispatch task no longer dies silently on broadcast lag — receiver is repositioned after `Lagged`, task continues
- H3: signal forwarding from `asylum stop`/`interrupt` now reaches the child process reliably
- H4: schema version drift between daemon and cockpit — `NodeEvent.schema_version` field added (serde default=1)
- H5: ntfy polling timer race eliminated — live-mode wins correctly even when poll fires concurrently
- H6: toast reply lookup now finds the correct node from graph state instead of always looking up node[0]
- H7: attach token issuance under load — issuer state is stored in `Arc<AttachTokenIssuer>` and shared correctly
- H8: install-script trust bootstrap — minisign public key is an embedded constant; signature verification is automatic once key is published
- H9: installer hard-fails when no hash tool (`sha256sum`/`shasum`) is available instead of silently skipping integrity checks

**Medium findings (M1–M21)**
- M1–M5: transactional storage, WebSocket hygiene, attach-token redaction, MCP JSON-RPC notifications (no response for id=null), daemon detachment
- M6–M10: owner token security, cockpit duplicate deduplication, missing icons, release-script hardening, NodeEvent schema_version
- M11–M21: dead types removed, Settings screen backed by real daemon health/token endpoints, NativeAttach clipboard copy, Cmd-K wired to real node actions and remote commands, ntfy inbound subscriber with reconnect/backoff

**Cockpit prototype residue removed**
- Tweaks card, simSpeed, runResponse, SessionStep, streamText all deleted
- Hardcoded seed maps in graph layouts removed
- "decision" InspectorAction and NodeScreenAction enum members removed
- All prototype-residue comments scrubbed from production code

**Low findings (L1–L22, this release)**
- L1: attach signature comparison is now constant-time (hand-rolled `ct_eq`)
- L2: hook filter parse failure now fails closed (blocks the event, logs a warning) instead of silently passing it through
- L3: PTY runtime entry inserted before `spawn_blocking` so early output is not lost
- L4: token estimator uses `chars().count()` (codepoints) instead of `len()` (bytes)
- L5: `seed_builtin_channels` error now logged at ERROR level on startup
- L6: MCP `_jsonrpc` field now uses `#[serde(rename = "jsonrpc")]` for correct wire name
- L7: `asylum logs` shells out to `tail -n 80` instead of reading the full log file into memory
- L8: `asylum update` surfaces both restart and doctor errors when both fail
- L9: native-attach command renderer single-quote-escapes args/env values containing shell-special characters
- L10: `node.archive` MCP tool was already present (verified)
- L13: NodeScreen flash timer handle stored in a ref and cleared on unmount
- L14: toast spawner appends to existing toasts and caps at 3 instead of replacing all
- L17: `package_binary` in `build-release-artifacts.sh` sets `trap 'rm -rf "$tmpdir"' EXIT`
- L18: `asylum_fetch_latest_release` uses the redirect from `/releases/latest` instead of the GitHub API + sed JSON parse
- L19: `REPO_SLUG` now defaults via `ASYLUM_REPO_SLUG` env var
- L20: `asylum_path_contains` normalizes trailing slashes on all PATH segments before comparison
- L21: dead `tool_call` switch case removed from NodeSession (NodeEventKind::ToolCall does not exist)
- L22: `NodeLiveness::is_terminal` now covers only `Failed` and `Archived`; new `is_done_for_now` method covers `Stopped` and `Exited` as resumable pauses
- L25: NodeSession module comment corrected from `asylum-core::node` to `asylum-core::event`

**Deferred (with rationale)**
- L11: `download_update_installer` integrity check deferred — requires an embedded sha256 of the installer script coordinated at release time (same trust-path as H9; documented)
- L12: MCP stdio loop `spawn_blocking` refactor deferred — non-trivial async refactor; current synchronous loop is correct but occupies a worker thread

### Added

- Real daemon-backed Settings screen: health endpoint extended; token list/rotate
- Native attach copies a runnable shell command line to clipboard
- Cmd-K palette finds nodes and supports remote commands
- ntfy inbound subscriber: real ntfy.sh JSON-stream subscription with reconnect/backoff and `channel.inbound` hook firing
- `NodeLiveness::is_done_for_now()` method for distinguishing resumable pauses from terminal states
