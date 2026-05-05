# Asylum Architecture Refactor Implementation Plan

## Source Spec

Implement [docs/reviews/2026-05-04-asylum-architecture-refactor-spec.md](../../reviews/2026-05-04-asylum-architecture-refactor-spec.md).

## Constraints

- Keep one installed executable named `asylum`.
- Do not preserve `asylum serve`; replace it with `asylum daemon run`.
- Do not make `asylum-cli` depend on `asylum-daemon`, or the reverse.
- Keep Cockpit on daemon HTTP/WebSocket.
- Move CLI/MCP local daemon control to the Unix socket at `~/.asylum/run/asylum.sock`, with `ASYLUM_SOCKET_PATH` override.
- Validate with Rust tests, Cockpit tests, dependency graph checks, and at least one local daemon/CLI smoke.

## Execution Slices

1. Reshape crates:
   - rename `asylum-core` to `asylum-types`;
   - move current CLI files into `crates/asylum-cli`;
   - reduce `crates/asylum` to the thin binary composition crate.
2. Shared configuration/types:
   - move the file config DTO into `asylum-types::config`;
   - add `base_url` and `socket_path` to `HealthResponse`;
   - remove direct clock reads from token DTOs.
3. Daemon runtime:
   - expose daemon run options from `asylum-daemon`;
   - load config/database/bind/socket path in the daemon layer;
   - serve the same router on TCP for Cockpit and on UDS for local control;
   - bypass bearer auth only for the local UDS router.
4. CLI/client/service lifecycle:
   - add `daemon run` parsing as a top-level action;
   - switch the daemon client to UDS by default;
   - update start/status/cockpit/MCP/native attach/service templates/pid identity.
5. Docs/scripts/tests:
   - replace live `serve` references with `daemon run`;
   - update crate names and commands;
   - add targeted tests for service templates, socket path resolution, command parsing, dependency graph assumptions, and daemon socket health.
6. Verification:
   - `cargo fmt --all`;
   - `cargo test --workspace`;
   - `npm --prefix cockpit run test`;
   - dependency checks for forbidden edges;
   - smoke `asylum daemon run` plus `asylum status --json` over the socket.

## Release Status

Released as v0.1.5. See [RELEASES.md](../../RELEASES.md) for the release
ledger.
