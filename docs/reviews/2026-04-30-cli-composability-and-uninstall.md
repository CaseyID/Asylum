# CLI composability + `asylum uninstall` — design review

**Status:** design review, decisions captured. Lifecycle goals remain, but crate layout and daemon-entry details are now rebased onto the finalized 2026-05-04 architecture spec.
**Date:** 2026-04-30
**Owner:** Casey
**Scope:** v0.1.x. Architecture for shared host-state introspection, a friendly `asylum uninstall`, and trimming the install-time shell script down to a thin bootstrap.

## 1. Why we're doing this

Today, lifecycle logic is scattered:

- `scripts/install.sh` knows about platform/arch detection, archive layout, signature verification, install paths, and PATH hints.
- `crates/asylum-cli/src/cli.rs` has its own implementations of "is the daemon running?", "is `~/.asylum` set up?", "what's the binary version?" inside `setup`, `status`, `doctor`, and `update` — each command answers similar questions slightly differently.
- `crates/asylum-cli/src/service.rs` already encodes launchd / systemd-user / pid-fallback detection, but it's only consumed by a couple of commands and the same shape isn't exposed as data anywhere else.
- `asylum install systemd|launchd` is misnamed: it only prints unit text to stdout. Nothing is actually installed.
- There is no way to cleanly remove an Asylum install. Users (and us, during release testing) `rm` files by hand, which is fine for `~/.local/bin/asylum` but error-prone around `~/.asylum/`, `~/.config/asylum/` (signing keys live there), and service units.

The goal is one place where lifecycle state is defined, and a CLI surface that composes that state into install / update / doctor / setup / **uninstall** without each command duplicating discovery logic.

## 2. Crate architecture context

The finalized architecture has four crates:

- **`asylum`** — tiny composition crate that builds the only installed binary target, `asylum`.
- **`asylum-cli`** — CLI, MCP bridge, Unix-socket daemon client, service lifecycle, runtime, and native attach helpers. Depends on `asylum-types`.
- **`asylum-daemon`** — the long-running server. Owns `axum`, `rusqlite`, `portable-pty`, `rust-embed` (cockpit baked in at compile time), plus substrate / harness / channels / hooks / notifications. Depends on `asylum-types`.
- **`asylum-types`** — pure data contracts (request/response types, capability tokens, event shapes). Deps are minimal: `serde`, `uuid`, `time`. No I/O, no filesystem, no process spawning.

There is exactly **one binary on disk**: `asylum`. The "daemon" is not a separate binary; foreground daemon mode is `asylum daemon run`, which dispatches from the composition crate into `asylum-daemon`. Lifecycle introspection ("is anything running, where is my binary, what's in `~/.asylum`") is fundamentally a CLI-side concern: `asylum-cli` introspects the host; the daemon doesn't introspect itself. Local CLI/MCP daemon control uses the Unix socket at `~/.asylum/run/asylum.sock`; Cockpit remains on HTTP/WebSocket.

This frames where new shared lifecycle code should live.

## 3. North-star architecture

Three layers, top to bottom.

### Layer 1 — `HostState`: shared inspection (Rust)

A single struct describing "what does this machine look like from Asylum's perspective." Cheap to construct, side-effect-free.

**Location: `crates/asylum-cli/src/host.rs`** (in the CLI crate, alongside `cli.rs` / `service.rs` / `client.rs`). Not `asylum-types` — that crate is data-contracts-only and adding sysinfo / fs / process / launchd / systemd code there forces every consumer of the protocol types to inherit OS-introspection deps. Not a new `asylum-host` crate either — there is no second consumer today, and extracting later is a mechanical refactor (move the file, change one `use` path). The current `service.rs` content folds into `host.rs` as part of this.

Rough shape:

- Binary: location on disk, version, on PATH or not, shadowed by other entries.
- Runtime dir: `~/.asylum` presence; `config.toml`, `asylum.sqlite3`, `logs/`, `run/` presence and sizes.
- Config dir: `~/.config/asylum` presence; whether it contains user assets we must not touch (e.g. signing keys).
- Daemon: process running / stopped / unknown; bind address; PID; how it was launched (launchd / systemd-user / pid-fallback).
- Service unit: launchd plist / systemd user unit installed? where? enabled?
- Cockpit assets: any embedded-cockpit caches we'd need to clean.
- Network: is the configured bind port in use, and by whom.

Two consumers:

- **In-process**: `setup`, `status`, `doctor`, `update`, `uninstall` all build a `HostState` and read from it.
- **Out-of-process**: `asylum status --json` emits the same struct, so shell callers — including `install.sh` — can branch on it without re-implementing detection.

This lets us delete duplicated detection in `cli.rs` and shrink `install.sh`.

### Layer 2 — Command surface, all wired through `HostState`

| Command | Status | Behavior |
|---|---|---|
| `asylum status` | exists | Render `HostState` for humans. |
| `asylum status --json` | new | Same data, machine-parseable, with `schema_version` field. |
| `asylum doctor` | exists | `HostState` checks + extras (network, signing-key access, etc.). |
| `asylum setup` | exists | Idempotent post-install dance; uses `HostState` to know what's already done. Prints a first-run banner when it's doing first-time work. |
| `asylum update` | exists | Reads current version from `HostState`, fetches, replaces, then runs `setup` + `doctor`. |
| **`asylum uninstall`** | **new** | See §4. |
| `asylum service generate systemd\|launchd` | replaces `asylum install systemd\|launchd` | Still prints unit text. Old form deleted outright (no alias). |
| `asylum` (no args) | enhance | Prints a "what is this, what's running, what to do next" first-run banner sourced from `HostState`, **only when `~/.asylum` does not yet exist**. Otherwise prints clap help as it does today. |

Per-subcommand `clap` `after_help` blocks add example invocations so the CLI is discoverable without docs.

### Layer 3 — `scripts/install.sh` becomes a thin bootstrap

Responsibilities reduce to:

1. Detect OS/arch.
2. Download the right archive + checksums + signature.
3. Verify signature against the embedded pubkey (already shipped in `86f8bc3`).
4. Place the binary at the install dir.
5. `exec "$INSTALL_DIR/asylum" setup`.

`asylum setup` is idempotent and already does the post-install dance; framing the install-success banner as "first-run behavior of `setup`" means we don't invent a new flag for the install handoff.

After step 5, every lifecycle operation lives in the binary. Shell exists only because of the chicken-and-egg of "you don't have the binary yet."

## 4. `asylum uninstall` — semantics

A single command. **No** `asylum uninstall daemon`, `asylum uninstall cockpit`, etc. — daemon and cockpit ship in one binary; per-component uninstall is meaningless at v0.1.x.

Default behavior:

1. Build `HostState`.
2. Print a plan: what will be removed, what will be kept, why.
3. Confirm interactively. `--yes` to skip.
4. Stop the daemon if running.
5. Remove the service unit (launchd plist / systemd user unit) if installed.
6. Remove the binary at the discovered install dir.
7. Remove `~/.asylum/` (state + logs + sqlite + sockets).
8. **Never** touch `~/.config/asylum/` by default — it holds user-supplied signing keys and other long-lived secrets.
9. Print a "what's left" report sourced from a fresh `HostState` (e.g. "PATH still references `~/.local/bin` — that's not ours, leaving it").

Flags for granular preservation:

- `--keep state` — leave `~/.asylum/` alone.
- `--keep config` — leave `~/.config/asylum/` alone (the default; flag is for explicitness in scripts).
- `--keep logs` — preserve `~/.asylum/logs/` even if state is removed.
- `--purge` — also remove `~/.config/asylum/`. Requires **typed-name confirmation** (user must type `asylum` to proceed) since this destroys signing keys. Never the default.
- `--dry-run` — print the plan and exit.
- `--json` — emit the plan as JSON.

Non-goals:

- We do not edit shell rc files. PATH hygiene is the user's; we surface it in the report.
- We do not try to identify "Asylum-related" files outside the paths above.

## 5. Renaming `install systemd|launchd` → `service generate ...`

Today's `asylum install systemd` and `asylum install launchd` only emit unit text on stdout. The verb is wrong (nothing is installed) and `install` is a confusing namespace.

Plan: add `asylum service generate systemd|launchd` with identical output. Delete the old form in the same change. **No deprecation alias** — nobody is using this yet besides us.

This is the only intentional CLI break in this batch and lands in v0.1.x where surface stability is explicitly not promised.

## 6. JSON schema posture

`asylum status --json` output has a `schema_version` field from day one. The schema is **versioned**: every field added or changed bumps the version. Third-party tooling that parses it can branch on `schema_version`. We don't promise stability across v0.1.x → v0.2 — that's what the version is for — but every change is intentional and recorded.

## 7. Out of scope

- No daemon-mode rework beyond the finalized `asylum daemon run` and Unix-socket architecture refactor. `asylum serve` is removed there, not retained.
- No cockpit asset overhaul. Cockpit's footprint shows up in `HostState`, but build/dev/embed pipelines don't change.
- No multi-user / system-wide install. Everything stays per-user (`~/.local/bin`, `~/.asylum`, user systemd, user launchd).
- No Windows. Not yet.
- No "soft uninstall" that disables but preserves; users wanting that can `asylum stop` and live with the binary on disk.

## 8. Decisions captured (2026-04-30)

| # | Decision |
|---|---|
| 1 | `HostState` lives at `crates/asylum-cli/src/host.rs` (CLI crate). `service.rs` detection folds in. |
| 2 | `status --json` ships with `schema_version` from day one; schema is versioned, not promised stable across v0.1.x → v0.2. |
| 3 | No `install --post-bootstrap` flag. `install.sh` ends with `exec asylum setup`. First-run banner is a property of `setup`. |
| 4 | First-run banner from bare `asylum` only when `~/.asylum` does not yet exist; otherwise clap help. |
| 5 | `--purge` requires typed-name confirmation (user types `asylum` to proceed). |
| 6 | `install systemd\|launchd` → `service generate ...`. Old form deleted outright; no alias. |

## 9. Sequencing (rough — not a build plan)

For when this turns into PRs. Each step is independently shippable and revertible.

1. **`HostState` skeleton + `status --json`.** Refactor `status` to read from `HostState`. Fold `service.rs` detection into `host.rs`. No human-visible behavior change in `status`; new JSON output. No other commands touched yet.
2. **Move `doctor` and `setup` onto `HostState`.** Delete duplicated detection. Add the first-run banner to `setup`.
3. **Rename `install systemd|launchd` → `service generate ...`.** Trivial PR; safe to land anytime.
4. **`scripts/install.sh` shrinks to thin bootstrap.** Tail becomes `exec asylum setup`. Most of the post-binary-placement shell goes away.
5. **`asylum uninstall`** with the §4 surface. Largest PR; gets its own design pass on the plan/confirm UX.
6. **Bare `asylum` first-run banner + `after_help` example blocks.** Cosmetic, last.

Steps 1–3 are pure refactor + cosmetics. Step 4 changes installation. Step 5 is the new user-facing command. Steps can be parallelized after 1 lands.

The next artifact is a per-PR plan starting with step 1.
