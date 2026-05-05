# Asylum Architecture Refactor Spec

**Status:** source-of-truth implementation spec for the daemon/CLI/crate-shape refactor  
**Date:** 2026-05-04  
**Scope:** make Asylum's Rust project layout, daemon model, CLI model, and local transports intuitive before feature work continues.

## Summary

Asylum should keep one installed command:

```text
~/.local/bin/asylum
```

That one command has multiple modes:

```text
asylum                 first-run/bootstrap path, then Cockpit
asylum node list       operator CLI
asylum start           start the background Asylum service
asylum stop            stop the background Asylum service
asylum status          inspect local runtime and daemon health
asylum cockpit         open Cockpit
asylum mcp             MCP stdio bridge
asylum daemon run      foreground daemon mode, used by services and development
```

Internally, the architecture is still daemon-backed:

```text
asylum CLI  ->  Asylum daemon  ->  nodes / harness sessions
```

The daemon owns live Asylum behavior. The CLI, Cockpit, MCP bridge, ntfy/hooks,
and future remote entrypoints are clients of that daemon. They must not
reimplement node behavior.

This refactor is not a rewrite. It is a project-shape correction, command
cleanup, and local transport correction.

The most important implementation trick is the tiny composition crate:
`crates/asylum` may link both the CLI library and daemon library, while those
two libraries never depend on each other. That preserves a single installed
binary without turning the CLI crate into a daemon implementation detail.

## Product Model

Asylum is a single-user, always-on control plane for real agent harness sessions.
It launches harnesses, observes them, stores their events and transcripts, lets
humans attach or intervene, and lets harnesses coordinate through shared node,
graph, channel, hook, and notification capabilities.

The core object is the Node: a live or resumable harness session.

Asylum provides:

- real harness launch for Codex, Claude Code, and future harnesses;
- durable node registry, liveness, graph, transcript, and event storage;
- node control: create, inspect, send input, interrupt, stop, archive, fork, attach;
- local PTY and Loon substrates today, with future substrate room;
- one shared capability surface used by CLI, Cockpit, MCP, hooks, and notifications;
- local runtime management: setup, start, stop, status, logs, update, uninstall.

## Naming

Use these terms consistently:

```text
Asylum             product name
asylum             installed command / executable
Asylum CLI         primary terminal interface
Asylum daemon      long-running resident control process
Asylum service     systemd/launchd/pid-fallback managed daemon instance
Cockpit            browser UI client
MCP bridge         `asylum mcp`, a stdio adapter into the daemon
```

Do not use "server" as the primary product term. The daemon exposes an HTTP
server for Cockpit, but HTTP is a transport detail.

Do not introduce a second installed `asylum-daemon` binary in this refactor.
The daemon is a mode of the one `asylum` executable.

## Runtime Model

```text
                         one installed executable: asylum

┌────────────────────────────────────────────────────────────────────┐
│ crates/asylum                                                      │
│ thin binary composition crate                                      │
│                                                                    │
│ - parses top-level mode through asylum-cli                         │
│ - dispatches normal CLI commands back to asylum-cli                │
│ - dispatches `asylum daemon run` into asylum-daemon                │
└──────────────┬─────────────────────────────────────┬───────────────┘
               │                                     │
               v                                     v
┌──────────────────────────────┐        ┌────────────────────────────┐
│ crates/asylum-cli             │        │ crates/asylum-daemon        │
│ terminal UX + local machine    │        │ daemon runtime              │
│ lifecycle                      │        │                            │
│                                │        │ owns nodes, storage, PTYs,  │
│ talks to daemon over local     │        │ Loon, hooks, channels,      │
│ daemon control transport       │        │ attach, HTTP/WS, socket     │
└──────────────┬────────────────┘        └──────────────┬─────────────┘
               │                                        │
               │ daemon requests                         │ launch/control
               v                                        v
        ~/.asylum/run/asylum.sock              real harness sessions
                                               Codex / Claude Code

                 ┌────────────────────────────┐
                 │ crates/asylum-types         │
                 │ shared data shapes only     │
                 └────────────────────────────┘
```

The key layering rule:

```text
asylum-cli must not depend on asylum-daemon.
asylum-daemon must not depend on asylum-cli.
Only the tiny asylum binary composition crate depends on both.
```

This is how we keep one binary without putting daemon imports into CLI
implementation files.

## Crate Layout

Target workspace:

```text
crates/
  asylum/          tiny binary composition crate; builds the installed `asylum`
  asylum-cli/      CLI, MCP bridge, daemon client, local machine lifecycle
  asylum-daemon/   daemon runtime and all live Asylum behavior
  asylum-types/    shared structs/enums crossing boundaries

cockpit/           browser UI; talks to the daemon over HTTP/WebSocket
```

Cargo package names, Rust import names, and binary targets:

```text
package           Rust crate import       binary target
asylum           n/a                     asylum
asylum-cli       asylum_cli              none
asylum-daemon    asylum_daemon           none
asylum-types     asylum_types            none
```

Only `crates/asylum` should define a binary target. It does not need a library
target. The other Rust crates are libraries.

Dependency graph:

```text
asylum ─────────> asylum-cli ───────┐
   │                                │
   └────────────> asylum-daemon ────┤
                                    v
                              asylum-types
```

Allowed:

- `asylum` imports `asylum-cli` and `asylum-daemon` because it is only the
  composition binary.
- `asylum-cli` imports `asylum-types`.
- `asylum-daemon` imports `asylum-types`.

Forbidden:

- `asylum-cli` importing `asylum-daemon`.
- `asylum-daemon` importing `asylum-cli`.
- shared runtime behavior leaking into `asylum-types`.

## Crate Responsibilities

### `crates/asylum`

This crate exists only because Rust needs a binary target that links the
product together.

Expected contents:

```text
crates/asylum/
  Cargo.toml
  src/main.rs
```

Responsibilities:

- build the installed binary named `asylum`;
- initialize logging/tracing;
- ask `asylum-cli` to parse the command line into a top-level action;
- execute normal CLI actions through `asylum-cli`;
- execute daemon foreground mode by calling `asylum-daemon`;
- contain no node, storage, harness, service-manager, or transport logic.

`main.rs` should be boring. If it grows beyond thin dispatch, something is in
the wrong crate.

Expected dispatch shape:

```text
asylum_cli::parse(...) -> TopLevelAction

TopLevelAction::Cli(action)        -> asylum_cli::run(action).await
TopLevelAction::DaemonRun(options) -> asylum_daemon::run(options).await
```

`TopLevelAction` is owned by `asylum-cli`. It should contain primitive/path
values needed by the composition crate to call the daemon. The daemon may expose
its own `DaemonRunOptions`; mapping between the two is allowed only in
`crates/asylum/src/main.rs`.

### `crates/asylum-cli`

This crate owns the terminal-facing product surface and local machine lifecycle.
It is more than Clap parsing: it is the operator-side implementation of the
`asylum` command.

Expected contents for this pass can stay close to the current file layout.
Splitting large files is optional later, not part of the core refactor.

Target shape:

```text
crates/asylum-cli/
  Cargo.toml
  src/
    lib.rs
    cli.rs
    client.rs
    host.rs
    mcp.rs
    native_attach.rs
    runtime.rs
```

Optional later cleanup:

```text
src/
  commands/
  daemon_client/
  local_machine/
```

Responsibilities:

- parse `asylum ...` commands;
- render terminal output;
- manage setup/start/stop/restart/status/doctor/logs/update/uninstall;
- manage service-unit generation and pid-fallback launch;
- run the MCP stdio bridge;
- call the daemon for node/graph/attach/channel/token/notification operations;
- expose a structured action for `asylum daemon run`, but not implement the daemon.

The CLI must not implement node behavior. It asks the daemon to do node work.

The CLI must not call `asylum_daemon::...` or import the daemon crate. When the
user runs `asylum daemon run`, `asylum-cli` parses that command and returns an
action to `crates/asylum`; the composition crate calls `asylum-daemon`.

CLI-side config responsibilities:

- create default config during `asylum setup`;
- show config during `asylum config show`;
- choose local runtime paths from `--config`, `ASYLUM_HOME`, and related env;
- start/stop/status the daemon process.

The CLI should not duplicate daemon config-merge behavior. Once the daemon is
running, the CLI learns daemon health and Cockpit URLs through the socket API.

### `crates/asylum-daemon`

This crate owns the daemon runtime and all live Asylum behavior.

Current large files may stay large for this pass:

```text
crates/asylum-daemon/
  Cargo.toml
  src/
    lib.rs
    app.rs
    attach.rs
    auth.rs
    capability_service.rs
    storage.rs
    channels/
    harness/
    hooks/
    notifications/
    substrate/
```

Responsibilities:

- load and merge daemon configuration for `asylum daemon run`;
- own the node registry and liveness;
- own SQLite storage and migrations;
- launch/control local PTY nodes;
- launch/control Loon-backed nodes;
- collect output/events/transcripts;
- issue browser and native attach targets;
- expose the local daemon control socket for CLI/MCP;
- expose HTTP/WebSocket for Cockpit and browser attach/observe;
- run background channel subscribers, hooks, and notifications;
- enforce auth/token rules for HTTP and remote-capable transports.

The daemon must not know about terminal formatting, install/update UX, shell rc
edits, or systemd/launchd management. Those are CLI/local-machine concerns.

Daemon run configuration:

- `asylum daemon run --config <path>` is the normal service/development entry.
- The daemon reads the config file, applies daemon-relevant environment
  overrides, applies explicit foreground flags, opens storage, and starts both
  transports.
- `--database`, `--bind`, `--socket-path`, owner-token, ntfy, harness, and Loon
  flags may exist for development and tests, but service templates should not
  need to pass them when the config file has the value.
- The daemon should expose the effective `base_url`, bind address, database
  path, and socket path through the shared health/status response
  (`HealthResponse` today) so CLI/Cockpit startup flows do not need to
  recompute them.

### `crates/asylum-types`

This crate owns data shapes crossing process/module boundaries. It is not a
runtime utility crate.

Expected contents:

```text
crates/asylum-types/src/
  lib.rs
  api.rs
  capabilities.rs
  config.rs
  event.rs
  node.rs
  relationship.rs
  security.rs
```

The config file data shape belongs here. Loading it from disk does not.
Use a shared serde struct such as `AsylumFileConfig` for the on-disk TOML shape,
including the database path, so setup/config-show/daemon-run do not grow
separate config schemas.

Allowed:

- request and response DTOs;
- node, graph, event, relationship, capability, config, token, channel, hook,
  recipe, and attach structs/enums;
- serde derives;
- simple `Display`, `FromStr`, and pure predicate helpers.

Forbidden:

- filesystem access;
- clocks or timers;
- network clients;
- sockets or HTTP;
- SQLite or migrations;
- async runtime code;
- process spawning;
- service-manager logic;
- Codex/Claude/Loon launch/control logic.

Current cleanup required:

- move or remove `chrono_like_unix_now()`;
- change `AttachToken::is_expired()` so it does not read the clock directly.
  Preferred shape: `is_expired_at(now_epoch_secs: i64)`.
- add or move the on-disk config-file DTO into `asylum-types::config` if it is
  currently private to the CLI crate.

## Command Model

Final command surface:

```text
asylum
asylum setup
asylum cockpit
asylum start
asylum stop
asylum restart
asylum status
asylum doctor
asylum logs
asylum update
asylum uninstall
asylum node ...
asylum graph ...
asylum attach ...
asylum token ...
asylum notify ...
asylum mcp
asylum daemon run
```

Remove `asylum serve`. There is no need for a compatibility alias in this
greenfield phase.

`asylum daemon run` is not a second product interface. It is the foreground
daemon entry mode for:

- systemd/launchd service templates;
- pid-fallback process launch;
- development;
- debugging daemon startup.

Normal users should mostly use `asylum`, `asylum start`, `asylum stop`,
`asylum status`, and `asylum cockpit`.

Foreground daemon flags:

```text
asylum daemon run --config ~/.asylum/config.toml
asylum daemon run --config ./dev.toml --bind 127.0.0.1:7717
asylum daemon run --config ./dev.toml --database ./target/asylum-dev.sqlite3
asylum daemon run --config ./dev.toml --socket-path ./target/asylum.sock
```

The exact flag set should match the current daemon configuration needs. Do not
preserve `serve` terminology in the new names.

## Transport Model

Final target:

```text
CLI/MCP      -> ~/.asylum/run/asylum.sock
Cockpit      -> http://127.0.0.1:<port> and WebSocket routes
ntfy/hooks   -> daemon background adapters
future remote -> explicit remote exposure, not accidental localhost behavior
```

### Local Socket

The daemon creates:

```text
~/.asylum/run/asylum.sock
```

Use a Unix domain socket on Linux/macOS. The socket carries HTTP/1.1-style JSON
requests over UDS so the daemon can reuse the same route/capability handlers
rather than inventing a custom protocol.

Rules:

- CLI and MCP use the socket as their daemon transport.
- Socket access is local and protected by filesystem permissions.
- Stale socket files are unlinked on daemon startup before binding.
- The run directory is created before binding and should be owner-only
  (`0700`) where the platform allows it.
- The socket file should be owner-only (`0600`) where the platform allows it.
- The socket is not a remote API.

Environment:

- Default socket path is derived from `ASYLUM_HOME`.
- Add `ASYLUM_SOCKET_PATH` as an explicit override for tests and unusual local
  layouts.
- Native attach targets should include `ASYLUM_SOCKET_PATH` when needed so
  `asylum attach <node>` reaches the same daemon that issued the target.

Implementation expectation:

- Server side can use `tokio::net::UnixListener` with `axum::serve`; axum 0.8
  supports Unix listeners.
- Client side can use `reqwest::ClientBuilder::unix_socket(...)` or a small
  hyper-based client. Prefer the smallest implementation that preserves the
  current typed request/response methods.

Auth:

- Unix socket requests may bypass bearer token auth because filesystem
  permissions are the local auth boundary.
- HTTP requests keep the current token behavior for browser and future remote
  uses.
- Keep the bypass explicit per transport. Do not accidentally disable auth on
  the HTTP router while making socket requests easy.

### HTTP/WebSocket

HTTP/WebSocket remains because Cockpit is a browser UI.

HTTP is not the conceptual center of Asylum. It is the Cockpit transport and
future optional remote transport.

The CLI should not rely on `ASYLUM_BASE_URL` for ordinary local daemon control
after the socket transport lands. Cockpit URL/base URL handling still matters
for opening the browser and for attach URLs.

`asylum cockpit` should use the socket for daemon health/startup checks, then
open the effective Cockpit HTTP URL reported by the daemon.

## Runtime State

Keep current runtime roots:

```text
~/.asylum/
  config.toml
  asylum.sqlite3
  run/
    asylum.sock
    asylum.pid
  logs/
    asylum.log
```

Do not rename the product state directory or database in this refactor.

## Service Management

`asylum start` starts the Asylum daemon using the best local backend:

- launchd on macOS;
- systemd user service on Linux when available;
- pid-fallback otherwise.

All generated service/process launch paths invoke:

```text
asylum daemon run --config <path>
```

Foreground overrides such as `--database`, `--bind`, or `--socket-path` are
valid for development and tests, but normal service templates should let the
daemon load those values from the config file.

The service manager remains in `asylum-cli` because it is local machine
lifecycle for the installed `asylum` command. If another first-party consumer
needs the same lifecycle library later, extract it then. Do not add an
`asylum-host` crate now.

PID identity checks must recognize the new daemon command shape. Old
`asylum serve` matching should be deleted with the command.

## Cockpit

Cockpit remains a top-level TypeScript/React app under:

```text
cockpit/
```

It talks to the daemon over HTTP/WebSocket. It does not import Rust crates.

Hand-mirrored TypeScript types stay for now. Do not add codegen in this
refactor.

If a Rust response shape changes, update the matching TypeScript type by hand.

## What This Refactor Changes

Required changes:

- split the current binary crate into:
  - tiny `crates/asylum` composition binary;
  - `crates/asylum-cli` CLI/local-machine library;
  - `crates/asylum-daemon` daemon library;
  - `crates/asylum-types` shared DTO library;
- rename `asylum-core` to `asylum-types`;
- delete `asylum serve`;
- add `asylum daemon run`;
- remove direct daemon imports from CLI implementation code;
- update service templates to use `asylum daemon run`;
- add `RuntimePaths::socket_path()` or equivalent;
- add `ASYLUM_SOCKET_PATH` support;
- move the shared on-disk config-file DTO into `asylum-types`;
- extend the shared daemon health/status response with effective `base_url` and
  `socket_path` fields if they are not already present;
- add daemon local socket listener;
- rewrite daemon client transport for local socket;
- keep HTTP/WebSocket for Cockpit;
- update live docs and scripts for the new crate names and command shape.
- keep release/install artifacts single-binary: archives and installs contain
  `asylum`, not a separate `asylum-daemon`.

Not required in this refactor:

- splitting `cli.rs` into many command files;
- splitting `app.rs`;
- splitting `capability_service.rs`;
- splitting `storage.rs`;
- introducing a versioned migration runner;
- decomposing `CapabilityService` into domain services;
- redesigning Cockpit;
- changing Loon behavior;
- changing ntfy routing semantics;
- adding hosted/SaaS/multi-user/RBAC behavior.

## Implementation Order

This is a practical delivery order, not a different target architecture.

1. Create the four-crate shape and keep behavior compiling.
2. Move current CLI files into `asylum-cli`.
3. Keep `crates/asylum/src/main.rs` as the only place that imports both
   `asylum-cli` and `asylum-daemon`.
4. Rename `asylum-core` to `asylum-types` and update imports.
5. Replace `serve` with `daemon run`.
6. Move the shared config-file DTO into `asylum-types`.
7. Move daemon run config loading into `asylum-daemon`.
8. Update service templates and pid identity tests.
9. Add socket path plumbing.
10. Add daemon Unix socket listener.
11. Move CLI/MCP daemon client to the socket transport.
12. Update live docs/scripts.
13. Verify full Rust and Cockpit test suites.

The implementation may pass through an intermediate state where the CLI client
still speaks HTTP while crates are being moved. That is an implementation
checkpoint only. The completed refactor target is socket-based CLI/MCP control.

## Definition Of Done

- Workspace contains these Rust crates:
  - `crates/asylum`
  - `crates/asylum-cli`
  - `crates/asylum-daemon`
  - `crates/asylum-types`
- `cargo build` produces one installed product binary: `asylum`.
- release archives contain exactly one executable named `asylum`.
- `cargo tree -p asylum-cli` does not contain `asylum-daemon`.
- `cargo tree -p asylum-daemon` does not contain `asylum-cli`.
- `crates/asylum/src/main.rs` is the only production code allowed to import
  both `asylum-cli` and `asylum-daemon`.
- `asylum serve` is gone.
- `asylum daemon run` starts the daemon in the foreground.
- `asylum start` launches `asylum daemon run` through the selected service
  backend.
- `asylum status` reads daemon health through `~/.asylum/run/asylum.sock`.
- `asylum mcp` reaches daemon capabilities through the same socket client.
- `ASYLUM_SOCKET_PATH` can point CLI/MCP at a non-default local socket.
- Cockpit loads and operates over HTTP/WebSocket.
- `asylum cockpit` uses the socket for local daemon control and opens the
  daemon-reported Cockpit HTTP URL.
- No user-facing runtime behavior is mocked, stubbed, or simulated.
- Live docs (`AGENTS.md`, `README.md`, active handoff/review docs) describe the
  new crate shape and command model.
- Historical archived plans may keep historical crate/command names when they
  are clearly archival.
- Rust tests pass: `cargo test --workspace`.
- Cockpit tests pass: `npm --prefix cockpit run test`.
- Targeted coverage exists for:
  - service templates rendering `asylum daemon run`;
  - PID identity matching the new daemon command shape;
  - socket health request/response;
  - CLI dependency graph not depending on daemon;
  - daemon dependency graph not depending on CLI.

## Release Status

Released as v0.1.5. See [RELEASES.md](../../RELEASES.md) for the current
published version ledger.
